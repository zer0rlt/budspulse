//! Linux Bluetooth transports (BlueZ over D-Bus via `zbus`, RFCOMM sockets via `libc`).
//!
//! * Discovery: `org.freedesktop.DBus.ObjectManager` on `org.bluez` → `org.bluez.Device1`.
//! * RFCOMM, preferred: register a client `org.bluez.Profile1` for the vendor service UUID and
//!   call `Device1.ConnectProfile(uuid)`; BlueZ performs the SDP lookup and hands us the
//!   connected socket in `NewConnection`. Fallback: raw `AF_BLUETOOTH/BTPROTO_RFCOMM` socket on
//!   the usual channels.
//! * BLE GATT (OnePlus fallback): `GattCharacteristic1.WriteValue(type=command)` + `StartNotify`.

use super::{EarbudTransport, PairedDevice, Target, NOTHING_SPP_UUID, OPO_BLE_NOTIFY_UUID, OPO_BLE_WRITE_UUID, OPO_LEGACY_UUID, OPO_SERVICE_UUID};
use crate::protocols::{self, TransportKind, Vendor};
use async_trait::async_trait;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::os::fd::{FromRawFd, OwnedFd};
use std::time::Duration;
use tokio::sync::mpsc;
use zbus::fdo::ObjectManagerProxy;
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};
use zbus::{Connection, Proxy};

type Props = HashMap<String, OwnedValue>;

fn s(v: Option<&OwnedValue>) -> Option<String> {
    v.and_then(|v| <&str>::try_from(v).ok()).map(|x| x.to_string())
}

fn b(v: Option<&OwnedValue>) -> Option<bool> {
    v.and_then(|v| bool::try_from(v).ok())
}

fn uuids(v: Option<&OwnedValue>) -> Vec<String> {
    v.and_then(|v| v.try_clone().ok()).and_then(|v| Vec::<String>::try_from(v).ok()).unwrap_or_default()
}

async fn managed(conn: &Connection) -> zbus::Result<HashMap<OwnedObjectPath, HashMap<String, Props>>> {
    let om = ObjectManagerProxy::builder(conn).destination("org.bluez")?.path("/")?.build().await?;
    let objs = om.get_managed_objects().await?;
    Ok(objs
        .into_iter()
        .map(|(p, ifaces)| (p, ifaces.into_iter().map(|(k, v)| (k.to_string(), v)).collect()))
        .collect())
}

async fn device_path(conn: &Connection, mac: &str) -> zbus::Result<Option<OwnedObjectPath>> {
    for (path, ifaces) in managed(conn).await? {
        if let Some(d) = ifaces.get("org.bluez.Device1") {
            if s(d.get("Address")).map(|a| a.eq_ignore_ascii_case(mac)).unwrap_or(false) {
                return Ok(Some(path));
            }
        }
    }
    Ok(None)
}

pub async fn list_paired() -> Result<Vec<PairedDevice>, String> {
    let conn = Connection::system().await.map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for (_path, ifaces) in managed(&conn).await.map_err(|e| e.to_string())? {
        let Some(d) = ifaces.get("org.bluez.Device1") else { continue };
        if !b(d.get("Paired")).unwrap_or(false) {
            continue;
        }
        let name = s(d.get("Alias")).or_else(|| s(d.get("Name"))).unwrap_or_default();
        let Some(addr) = s(d.get("Address")) else { continue };
        let ids = uuids(d.get("UUIDs"));
        let has = |u: &str| ids.iter().any(|x| x.eq_ignore_ascii_case(u));
        let model = protocols::identify_by_name(&name).or_else(|| {
            if has(NOTHING_SPP_UUID) {
                protocols::model_by_code("B171")
            } else if has(OPO_SERVICE_UUID) || has(OPO_LEGACY_UUID) {
                Some(protocols::oneplus::generic_model())
            } else {
                None
            }
        });
        let Some(model) = model else { continue };
        out.push(PairedDevice {
            id: addr.to_uppercase(),
            name,
            address: addr.to_uppercase(),
            vendor: Some(model.vendor),
            model: Some(model.name),
            connected: b(d.get("Connected")).unwrap_or(false),
        });
    }
    Ok(out)
}

/// Removes the pairing: `org.bluez.Adapter1.RemoveDevice(device_path)` on the owning adapter.
pub async fn unpair(mac: &str) -> Result<(), String> {
    let conn = Connection::system().await.map_err(|e| e.to_string())?;
    let path = device_path(&conn, mac).await.map_err(|e| e.to_string())?.ok_or("устройство не найдено в BlueZ")?;
    let adapter = adapter_of(path.as_str()).ok_or("не найден Bluetooth-адаптер устройства")?;
    let a = Proxy::new(&conn, "org.bluez", adapter, "org.bluez.Adapter1").await.map_err(|e| e.to_string())?;
    let r: zbus::Result<()> = a.call("RemoveDevice", &(path,)).await;
    r.map_err(|e| format!("BlueZ RemoveDevice: {e}"))
}

/// `/org/bluez/hci0/dev_AA_BB_…` → `/org/bluez/hci0`.
fn adapter_of(device_path: &str) -> Option<&str> {
    device_path.rsplit_once('/').map(|(a, _)| a).filter(|a| !a.is_empty())
}

/// `org.bluez.Device1.Connected` — local D-Bus property read, no radio traffic.
pub async fn os_connected(mac: &str) -> Option<bool> {
    let conn = Connection::system().await.ok()?;
    let path = device_path(&conn, mac).await.ok()??;
    let p = Proxy::new(&conn, "org.bluez", path.as_str(), "org.bluez.Device1").await.ok()?;
    p.get_property::<bool>("Connected").await.ok()
}

/// `org.bluez.Battery1.Percentage` (HFP/AVRCP battery reported by BlueZ).
pub async fn battery1(mac: &str) -> Option<u8> {
    let conn = Connection::system().await.ok()?;
    let path = device_path(&conn, mac).await.ok()??;
    let p = Proxy::new(&conn, "org.bluez", path.as_str(), "org.bluez.Battery1").await.ok()?;
    p.get_property::<u8>("Percentage").await.ok()
}

// ---------------------------------------------------------------------------------------------
// Profile1 (BlueZ performs SDP and gives us the connected RFCOMM fd)
// ---------------------------------------------------------------------------------------------

struct ProfileHandler {
    tx: mpsc::Sender<OwnedFd>,
}

#[zbus::interface(name = "org.bluez.Profile1")]
impl ProfileHandler {
    async fn release(&self) {}

    async fn new_connection(&self, _device: OwnedObjectPath, fd: zbus::zvariant::OwnedFd, _props: HashMap<String, OwnedValue>) {
        let fd: OwnedFd = fd.into();
        let _ = self.tx.send(fd).await;
    }

    async fn request_disconnection(&self, _device: OwnedObjectPath) {}
}

async fn connect_via_profile(mac: &str, uuid: &str) -> Result<(OwnedFd, Connection, String), String> {
    let conn = Connection::system().await.map_err(|e| e.to_string())?;
    let dev = device_path(&conn, mac).await.map_err(|e| e.to_string())?.ok_or("устройство не найдено в BlueZ")?;
    let (tx, mut rx) = mpsc::channel(1);
    let obj = format!("/dev/budspulse/profile_{}", uuid.replace('-', "_"));
    conn.object_server().at(obj.as_str(), ProfileHandler { tx }).await.map_err(|e| e.to_string())?;
    let pm = Proxy::new(&conn, "org.bluez", "/org/bluez", "org.bluez.ProfileManager1").await.map_err(|e| e.to_string())?;
    let mut opts: HashMap<&str, Value> = HashMap::new();
    opts.insert("Name", Value::from("BudsPulse"));
    opts.insert("Role", Value::from("client"));
    opts.insert("AutoConnect", Value::from(false));
    let path = zbus::zvariant::ObjectPath::try_from(obj.as_str()).map_err(|e| e.to_string())?;
    let reg: zbus::Result<()> = pm.call("RegisterProfile", &(path.clone(), uuid, opts)).await;
    if let Err(e) = reg {
        let _ = conn.object_server().remove::<ProfileHandler, _>(obj.as_str()).await;
        return Err(format!("RegisterProfile: {e}"));
    }
    let d = Proxy::new(&conn, "org.bluez", dev.as_str(), "org.bluez.Device1").await.map_err(|e| e.to_string())?;
    let call: zbus::Result<()> = d.call("ConnectProfile", &(uuid,)).await;
    let fd = match call {
        Ok(()) => tokio::time::timeout(Duration::from_secs(8), rx.recv()).await.ok().flatten(),
        Err(e) => {
            log::debug!("ConnectProfile {uuid}: {e}");
            None
        }
    };
    match fd {
        Some(fd) => Ok((fd, conn, obj)),
        None => {
            let _: zbus::Result<()> = pm.call("UnregisterProfile", &(path,)).await;
            let _ = conn.object_server().remove::<ProfileHandler, _>(obj.as_str()).await;
            Err("BlueZ не выдал RFCOMM-соединение (сервис не найден или занят)".into())
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Raw RFCOMM socket
// ---------------------------------------------------------------------------------------------

const AF_BLUETOOTH: libc::c_int = 31;
const BTPROTO_RFCOMM: libc::c_int = 3;

#[repr(C)]
struct SockaddrRc {
    rc_family: libc::sa_family_t,
    rc_bdaddr: [u8; 6],
    rc_channel: u8,
}

fn rfcomm_connect(mac: &str, channel: u8) -> Result<OwnedFd, String> {
    let addr = super::mac_to_u64(mac).ok_or("неверный MAC")?;
    let mut bd = [0u8; 6];
    for (i, item) in bd.iter_mut().enumerate() {
        *item = ((addr >> (8 * i)) & 0xFF) as u8; // bdaddr_t is little-endian
    }
    // SAFETY: plain socket syscalls with a correctly sized sockaddr
    unsafe {
        let fd = libc::socket(AF_BLUETOOTH, libc::SOCK_STREAM | libc::SOCK_CLOEXEC, BTPROTO_RFCOMM);
        if fd < 0 {
            return Err(format!("socket(AF_BLUETOOTH): {}", std::io::Error::last_os_error()));
        }
        let sa = SockaddrRc { rc_family: AF_BLUETOOTH as libc::sa_family_t, rc_bdaddr: bd, rc_channel: channel };
        let r = libc::connect(fd, &sa as *const SockaddrRc as *const libc::sockaddr, std::mem::size_of::<SockaddrRc>() as libc::socklen_t);
        if r != 0 {
            let e = std::io::Error::last_os_error();
            libc::close(fd);
            return Err(format!("RFCOMM канал {channel}: {e}"));
        }
        Ok(OwnedFd::from_raw_fd(fd))
    }
}

// ---------------------------------------------------------------------------------------------
// Stream transport over an RFCOMM fd
// ---------------------------------------------------------------------------------------------

struct FdTransport {
    file: Option<std::fs::File>,
    rx: Option<mpsc::Receiver<Vec<u8>>>,
    /// keeps the Profile1 object registered for the lifetime of the link
    _profile: Option<(Connection, String)>,
}

impl FdTransport {
    fn new(fd: OwnedFd, profile: Option<(Connection, String)>) -> Result<Self, String> {
        let file = std::fs::File::from(fd);
        let mut reader = file.try_clone().map_err(|e| e.to_string())?;
        let (tx, rx) = mpsc::channel(128);
        std::thread::Builder::new()
            .name("rfcomm-rx".into())
            .spawn(move || {
                let mut buf = [0u8; 2048];
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            if tx.blocking_send(buf[..n].to_vec()).is_err() {
                                break;
                            }
                        }
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(FdTransport { file: Some(file), rx: Some(rx), _profile: profile })
    }
}

#[async_trait]
impl EarbudTransport for FdTransport {
    fn kind(&self) -> TransportKind {
        TransportKind::Rfcomm
    }
    async fn connect(&mut self) -> Result<(), String> {
        Ok(())
    }
    async fn disconnect(&mut self) {
        if let Some(f) = self.file.take() {
            use std::os::fd::AsRawFd;
            // SAFETY: shutting down our own socket wakes the reader thread
            unsafe {
                libc::shutdown(f.as_raw_fd(), libc::SHUT_RDWR);
            }
        }
        if let Some((conn, obj)) = self._profile.take() {
            if let Ok(pm) = Proxy::new(&conn, "org.bluez", "/org/bluez", "org.bluez.ProfileManager1").await {
                if let Ok(p) = zbus::zvariant::ObjectPath::try_from(obj.as_str()) {
                    let _: zbus::Result<()> = pm.call("UnregisterProfile", &(p,)).await;
                }
            }
            let _ = conn.object_server().remove::<ProfileHandler, _>(obj.as_str()).await;
        }
    }
    async fn send_frame(&mut self, data: &[u8]) -> Result<(), String> {
        let f = self.file.as_mut().ok_or("соединение закрыто")?;
        f.write_all(data).map_err(|e| format!("RFCOMM запись: {e}"))
    }
    fn subscribe_rx(&mut self) -> mpsc::Receiver<Vec<u8>> {
        self.rx.take().unwrap_or_else(|| mpsc::channel(1).1)
    }
}

// ---------------------------------------------------------------------------------------------
// BLE GATT transport
// ---------------------------------------------------------------------------------------------

struct GattTransport {
    write: Proxy<'static>,
    notify: Proxy<'static>,
    rx: Option<mpsc::Receiver<Vec<u8>>>,
    task: Option<tokio::task::JoinHandle<()>>,
}

async fn open_gatt(mac: &str) -> Result<GattTransport, String> {
    let conn = Connection::system().await.map_err(|e| e.to_string())?;
    let dev = device_path(&conn, mac).await.map_err(|e| e.to_string())?.ok_or("устройство не найдено")?;
    let objs = managed(&conn).await.map_err(|e| e.to_string())?;
    let find = |uuid: &str| -> Option<OwnedObjectPath> {
        objs.iter()
            .find(|(p, i)| p.as_str().starts_with(dev.as_str()) && i.get("org.bluez.GattCharacteristic1").and_then(|c| s(c.get("UUID"))).map(|u| u.eq_ignore_ascii_case(uuid)).unwrap_or(false))
            .map(|(p, _)| p.clone())
    };
    let wp = find(OPO_BLE_WRITE_UUID).ok_or("BLE: характеристика записи OPO не найдена (GATT не разрешён?)")?;
    let np = find(OPO_BLE_NOTIFY_UUID).ok_or("BLE: характеристика уведомлений OPO не найдена")?;
    let write = Proxy::new_owned(conn.clone(), "org.bluez".to_string(), wp.to_string(), "org.bluez.GattCharacteristic1".to_string()).await.map_err(|e| e.to_string())?;
    let notify = Proxy::new_owned(conn.clone(), "org.bluez".to_string(), np.to_string(), "org.bluez.GattCharacteristic1".to_string()).await.map_err(|e| e.to_string())?;
    let props = Proxy::new_owned(conn.clone(), "org.bluez".to_string(), np.to_string(), "org.freedesktop.DBus.Properties".to_string()).await.map_err(|e| e.to_string())?;
    let mut stream = props.receive_signal("PropertiesChanged").await.map_err(|e| e.to_string())?;
    let _: () = notify.call("StartNotify", &()).await.map_err(|e| format!("StartNotify: {e}"))?;
    let (tx, rx) = mpsc::channel(128);
    let task = tokio::spawn(async move {
        use futures::StreamExt;
        while let Some(msg) = stream.next().await {
            let body = msg.body();
            let Ok((_iface, changed, _inv)) = body.deserialize::<(String, HashMap<String, OwnedValue>, Vec<String>)>() else { continue };
            if let Some(v) = changed.get("Value").and_then(|v| v.try_clone().ok()).and_then(|v| Vec::<u8>::try_from(v).ok()) {
                if tx.send(v).await.is_err() {
                    break;
                }
            }
        }
    });
    Ok(GattTransport { write, notify, rx: Some(rx), task: Some(task) })
}

#[async_trait]
impl EarbudTransport for GattTransport {
    fn kind(&self) -> TransportKind {
        TransportKind::Ble
    }
    async fn connect(&mut self) -> Result<(), String> {
        Ok(())
    }
    async fn disconnect(&mut self) {
        let _: zbus::Result<()> = self.notify.call("StopNotify", &()).await;
        if let Some(t) = self.task.take() {
            t.abort();
        }
    }
    async fn send_frame(&mut self, data: &[u8]) -> Result<(), String> {
        for chunk in data.chunks(20) {
            let mut opts: HashMap<&str, Value> = HashMap::new();
            // write-without-response (OPO ignores with-response writes)
            opts.insert("type", Value::from("command"));
            let r: zbus::Result<()> = self.write.call("WriteValue", &(chunk.to_vec(), opts)).await;
            r.map_err(|e| format!("BLE запись: {e}"))?;
        }
        Ok(())
    }
    fn subscribe_rx(&mut self) -> mpsc::Receiver<Vec<u8>> {
        self.rx.take().unwrap_or_else(|| mpsc::channel(1).1)
    }
}

// ---------------------------------------------------------------------------------------------
// Strategy
// ---------------------------------------------------------------------------------------------

pub async fn open(target: &Target) -> Result<Box<dyn EarbudTransport>, String> {
    let mac = target.address.clone();
    let (uuids, channels): (&[&str], &[u8]) = match target.model.vendor {
        Vendor::Nothing | Vendor::Cmf => (&[NOTHING_SPP_UUID], &[15, 16, 14, 17]),
        Vendor::Oneplus => (&[OPO_SERVICE_UUID, OPO_LEGACY_UUID], &[15, 12]),
    };
    let mut last = String::new();
    for u in uuids {
        match connect_via_profile(&mac, u).await {
            Ok((fd, conn, obj)) => return Ok(Box::new(FdTransport::new(fd, Some((conn, obj)))?)),
            Err(e) => last = e,
        }
    }
    for ch in channels {
        let m = mac.clone();
        let ch = *ch;
        match tokio::task::spawn_blocking(move || rfcomm_connect(&m, ch)).await {
            Ok(Ok(fd)) => return Ok(Box::new(FdTransport::new(fd, None)?)),
            Ok(Err(e)) => last = e,
            Err(e) => last = e.to_string(),
        }
    }
    if target.model.vendor == Vendor::Oneplus {
        match open_gatt(&mac).await {
            Ok(t) => return Ok(Box::new(t)),
            Err(e) => last = e,
        }
    }
    Err(format!("не удалось открыть канал управления: {last}"))
}

#[cfg(test)]
mod tests {
    use super::adapter_of;

    #[test]
    fn adapter_from_device_path() {
        assert_eq!(adapter_of("/org/bluez/hci0/dev_2C_BE_EB_00_12_34"), Some("/org/bluez/hci0"));
        assert_eq!(adapter_of("/dev"), None);
        assert_eq!(adapter_of("nopath"), None);
    }
}
