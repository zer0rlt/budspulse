//! Windows Bluetooth transports.
//!
//! RFCOMM strategies (tried in a vendor-specific order):
//! * WinRT `RfcommDeviceService` + `StreamSocket` by service UUID (Nothing/CMF: some models reject
//!   raw AF_BTH sockets but accept WinRT).
//! * Winsock `AF_BTH` socket with `serviceClassId` = UUID and port 0 → Windows performs the SDP
//!   lookup itself (proven for OnePlus by QuickBuds), then direct channels as last resort.
//! * BLE GATT (OnePlus `079A` service, write-without-response + notify) as an experimental fallback.

use super::{EarbudTransport, PairedDevice, Target, NOTHING_SPP_UUID, OPO_BLE_NOTIFY_UUID, OPO_BLE_WRITE_UUID, OPO_LEGACY_UUID, OPO_SERVICE_UUID};
use crate::protocols::{self, TransportKind, Vendor};
use async_trait::async_trait;
use std::time::Duration;
use tokio::sync::mpsc;
use windows::core::GUID;
use windows::Devices::Bluetooth::GenericAttributeProfile::{
    GattCharacteristic, GattClientCharacteristicConfigurationDescriptorValue, GattCommunicationStatus, GattSession, GattValueChangedEventArgs, GattWriteOption,
};
use windows::Devices::Bluetooth::Rfcomm::RfcommServiceId;
use windows::Devices::Bluetooth::{BluetoothCacheMode, BluetoothConnectionStatus, BluetoothDevice, BluetoothLEDevice};
use windows::Devices::Enumeration::DeviceInformation;
use windows::Foundation::TypedEventHandler;
use windows::Networking::Sockets::StreamSocket;
use windows::Storage::Streams::{DataReader, DataWriter, InputStreamOptions};

fn guid(s: &str) -> GUID {
    GUID::from_u128(u128::from_str_radix(&s.replace('-', ""), 16).expect("valid uuid literal"))
}

fn werr(e: windows::core::Error) -> String {
    format!("{} (0x{:08X})", e.message(), e.code().0)
}

async fn blocking<T: Send + 'static>(secs: u64, f: impl FnOnce() -> Result<T, String> + Send + 'static) -> Result<T, String> {
    match tokio::time::timeout(Duration::from_secs(secs), tokio::task::spawn_blocking(f)).await {
        Ok(Ok(r)) => r,
        Ok(Err(e)) => Err(format!("внутренняя ошибка: {e}")),
        Err(_) => Err("таймаут подключения".into()),
    }
}

// ---------------------------------------------------------------------------------------------
// Discovery
// ---------------------------------------------------------------------------------------------

pub async fn list_paired() -> Result<Vec<PairedDevice>, String> {
    blocking(15, || {
        let selector = BluetoothDevice::GetDeviceSelectorFromPairingState(true).map_err(werr)?;
        let infos = DeviceInformation::FindAllAsyncAqsFilter(&selector).map_err(werr)?.get().map_err(werr)?;
        let mut out = Vec::new();
        for info in infos {
            let Ok(id) = info.Id() else { continue };
            let Ok(dev) = BluetoothDevice::FromIdAsync(&id).and_then(|op| op.get()) else { continue };
            let name = dev.Name().map(|n| n.to_string()).unwrap_or_default();
            let Ok(addr) = dev.BluetoothAddress() else { continue };
            let mac = super::u64_to_mac(addr);
            let connected = dev.ConnectionStatus().map(|s| s == BluetoothConnectionStatus::Connected).unwrap_or(false);
            let model = protocols::identify_by_name(&name).or_else(|| identify_by_services(&dev));
            let Some(model) = model else { continue };
            out.push(PairedDevice { id: mac.clone(), name, address: mac, vendor: Some(model.vendor), model: Some(model.name), connected });
        }
        Ok(out)
    })
    .await
}

/// Removes the pairing (Windows "Remove device"). Earbuds usually register both a Classic and a
/// BLE entry with the same address, so both are unpaired; a missing BLE entry is not an error.
pub async fn unpair(address: &str) -> Result<(), String> {
    use windows::Devices::Enumeration::{DeviceInformationPairing, DeviceUnpairingResultStatus};
    let addr = super::mac_to_u64(address).ok_or("неверный адрес устройства")?;
    blocking(30, move || {
        fn unpair_one(pairing: DeviceInformationPairing) -> Result<(), String> {
            if !pairing.IsPaired().map_err(werr)? {
                return Ok(());
            }
            let status = pairing.UnpairAsync().map_err(werr)?.get().map_err(werr)?.Status().map_err(werr)?;
            match status {
                DeviceUnpairingResultStatus::Unpaired | DeviceUnpairingResultStatus::AlreadyUnpaired => Ok(()),
                DeviceUnpairingResultStatus::AccessDenied => Err("Windows запретила удаление (нет доступа)".into()),
                DeviceUnpairingResultStatus::OperationAlreadyInProgress => Err("удаление уже выполняется".into()),
                s => Err(format!("Windows не удалила устройство (код {})", s.0)),
            }
        }
        let classic = BluetoothDevice::FromBluetoothAddressAsync(addr)
            .and_then(|op| op.get())
            .and_then(|d| d.DeviceInformation())
            .and_then(|i| i.Pairing());
        let le = BluetoothLEDevice::FromBluetoothAddressAsync(addr)
            .and_then(|op| op.get())
            .and_then(|d| d.DeviceInformation())
            .and_then(|i| i.Pairing());
        if classic.is_err() && le.is_err() {
            return Err("устройство не найдено в Windows".into());
        }
        let r = classic.map_err(werr).and_then(unpair_one);
        if let Ok(p) = le {
            let _ = unpair_one(p);
        }
        r
    })
    .await
}

/// Renamed devices: look at cached RFCOMM services.
fn identify_by_services(dev: &BluetoothDevice) -> Option<protocols::ModelInfo> {
    let has = |uuid: &str| -> bool {
        RfcommServiceId::FromUuid(guid(uuid))
            .and_then(|sid| dev.GetRfcommServicesForIdWithCacheModeAsync(&sid, BluetoothCacheMode::Cached))
            .and_then(|op| op.get())
            .and_then(|r| r.Services())
            .and_then(|s| s.Size())
            .map(|n| n > 0)
            .unwrap_or(false)
    };
    if has(NOTHING_SPP_UUID) {
        return protocols::model_by_code("B171");
    }
    if has(OPO_SERVICE_UUID) || has(OPO_LEGACY_UUID) {
        return Some(protocols::oneplus::generic_model());
    }
    None
}

/// Local query of the Windows Bluetooth stack (ACL link present?). Does not page the device.
pub async fn os_connected(address: &str) -> Option<bool> {
    let addr = super::mac_to_u64(address)?;
    blocking(5, move || {
        let dev = BluetoothDevice::FromBluetoothAddressAsync(addr).map_err(werr)?.get().map_err(werr)?;
        dev.ConnectionStatus().map(|s| s == BluetoothConnectionStatus::Connected).map_err(werr)
    })
    .await
    .ok()
}

/// Windows exposes the HFP battery level as PnP property {104EA319-…} 2 (fallback only).
pub async fn pnp_battery(address: &str) -> Option<u8> {
    let mac = address.replace(':', "").to_uppercase();
    let script = format!(
        "Get-PnpDevice -Class System,Bluetooth -ErrorAction SilentlyContinue | Where-Object {{ $_.InstanceId -like '*{mac}*' }} | ForEach-Object {{ (Get-PnpDeviceProperty -InstanceId $_.InstanceId -KeyName '{{104EA319-6EE2-4701-BD47-8DDBF425BBE5}} 2' -ErrorAction SilentlyContinue).Data }} | Where-Object {{ $_ -ne $null }} | Select-Object -First 1"
    );
    let fut = tokio::process::Command::new("powershell.exe").args(["-NoProfile", "-NonInteractive", "-Command", &script]).creation_flags(0x0800_0000).output();
    let out = tokio::time::timeout(Duration::from_secs(6), fut).await.ok()?.ok()?;
    String::from_utf8_lossy(&out.stdout).trim().parse::<u8>().ok().filter(|v| *v <= 100)
}

// ---------------------------------------------------------------------------------------------
// Connect strategies
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone)]
enum Strategy {
    WinRt(&'static str),
    Bth { uuid: Option<&'static str>, channel: u8 },
    Ble,
}

pub async fn open(target: &Target) -> Result<Box<dyn EarbudTransport>, String> {
    let addr = super::mac_to_u64(&target.address).ok_or("неверный MAC-адрес")?;
    let plan: Vec<Strategy> = match target.model.vendor {
        Vendor::Nothing | Vendor::Cmf => vec![
            Strategy::WinRt(NOTHING_SPP_UUID),
            Strategy::Bth { uuid: Some(NOTHING_SPP_UUID), channel: 0 },
            Strategy::Bth { uuid: None, channel: 15 },
            Strategy::Bth { uuid: None, channel: 16 },
        ],
        Vendor::Oneplus => vec![
            Strategy::Bth { uuid: Some(OPO_SERVICE_UUID), channel: 0 },
            Strategy::Bth { uuid: Some(OPO_LEGACY_UUID), channel: 0 },
            Strategy::WinRt(OPO_SERVICE_UUID),
            Strategy::Bth { uuid: None, channel: 15 },
            Strategy::Bth { uuid: None, channel: 12 },
            Strategy::Ble,
        ],
    };
    let mut errors = Vec::new();
    for s in plan {
        let res: Result<Box<dyn EarbudTransport>, String> = match s.clone() {
            Strategy::WinRt(uuid) => WinRtRfcomm::open(addr, uuid).await.map(|t| Box::new(t) as Box<dyn EarbudTransport>),
            Strategy::Bth { uuid, channel } => BthSocket::open(addr, uuid, channel).await.map(|t| Box::new(t) as Box<dyn EarbudTransport>),
            Strategy::Ble => BleGatt::open(addr).await.map(|t| Box::new(t) as Box<dyn EarbudTransport>),
        };
        match res {
            Ok(t) => {
                log::info!("connected to {} via {:?}", target.name, s);
                return Ok(t);
            }
            Err(e) => {
                log::debug!("{:?} failed: {e}", s);
                errors.push(format!("{s:?}: {e}"));
            }
        }
    }
    Err(format!(
        "не удалось открыть канал управления ({}). Убедитесь, что наушники подключены к ПК и не заняты другим приложением.",
        errors.last().cloned().unwrap_or_default()
    ))
}

// ---------------------------------------------------------------------------------------------
// WinRT RFCOMM
// ---------------------------------------------------------------------------------------------

struct WinRtRfcomm {
    socket: StreamSocket,
    writer: DataWriter,
    rx: Option<mpsc::Receiver<Vec<u8>>>,
}

// SAFETY: StreamSocket and DataWriter are agile WinRT objects (free-threaded marshaler); we only
// touch them from blocking worker threads, one call at a time.
unsafe impl Send for WinRtRfcomm {}

impl WinRtRfcomm {
    async fn open(addr: u64, uuid: &'static str) -> Result<Self, String> {
        let (socket, writer, reader) = blocking(12, move || {
            let dev = BluetoothDevice::FromBluetoothAddressAsync(addr).map_err(werr)?.get().map_err(werr)?;
            let sid = RfcommServiceId::FromUuid(guid(uuid)).map_err(werr)?;
            let mut services = None;
            for mode in [BluetoothCacheMode::Uncached, BluetoothCacheMode::Cached] {
                if let Ok(r) = dev.GetRfcommServicesForIdWithCacheModeAsync(&sid, mode).and_then(|op| op.get()) {
                    if let Ok(list) = r.Services() {
                        if list.Size().unwrap_or(0) > 0 {
                            services = Some(list.GetAt(0).map_err(werr)?);
                            break;
                        }
                    }
                }
            }
            let svc = services.ok_or("сервис RFCOMM не найден (SDP)")?;
            let socket = StreamSocket::new().map_err(werr)?;
            socket.ConnectAsync(&svc.ConnectionHostName().map_err(werr)?, &svc.ConnectionServiceName().map_err(werr)?).map_err(werr)?.get().map_err(werr)?;
            let writer = DataWriter::CreateDataWriter(&socket.OutputStream().map_err(werr)?).map_err(werr)?;
            let reader = DataReader::CreateDataReader(&socket.InputStream().map_err(werr)?).map_err(werr)?;
            reader.SetInputStreamOptions(InputStreamOptions::Partial).map_err(werr)?;
            Ok(SendBox((socket, writer, reader)))
        })
        .await?
        .0;
        let (tx, rx) = mpsc::channel(128);
        let reader = SendBox(reader);
        std::thread::Builder::new()
            .name("rfcomm-winrt-rx".into())
            .spawn(move || {
                let reader = reader;
                loop {
                    let n = match reader.0.LoadAsync(1024).and_then(|op| op.get()) {
                        Ok(n) if n > 0 => n as usize,
                        _ => break,
                    };
                    let mut buf = vec![0u8; n];
                    if reader.0.ReadBytes(&mut buf).is_err() || tx.blocking_send(buf).is_err() {
                        break;
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(WinRtRfcomm { socket, writer, rx: Some(rx) })
    }
}

struct SendBox<T>(T);
// SAFETY: used only for agile WinRT objects / plain handles moved between threads
unsafe impl<T> Send for SendBox<T> {}

#[async_trait]
impl EarbudTransport for WinRtRfcomm {
    fn kind(&self) -> TransportKind {
        TransportKind::Rfcomm
    }
    async fn connect(&mut self) -> Result<(), String> {
        Ok(())
    }
    async fn disconnect(&mut self) {
        let _ = self.socket.Close();
    }
    async fn send_frame(&mut self, data: &[u8]) -> Result<(), String> {
        let w = SendBox(self.writer.clone());
        let d = data.to_vec();
        blocking(5, move || {
            let w = w;
            w.0.WriteBytes(&d).map_err(werr)?;
            w.0.StoreAsync().map_err(werr)?.get().map_err(werr)?;
            Ok(())
        })
        .await
    }
    fn subscribe_rx(&mut self) -> mpsc::Receiver<Vec<u8>> {
        self.rx.take().unwrap_or_else(|| mpsc::channel(1).1)
    }
}

// ---------------------------------------------------------------------------------------------
// Winsock AF_BTH RFCOMM
// ---------------------------------------------------------------------------------------------

mod bth {
    use windows::core::GUID;
    use windows::Win32::Networking::WinSock::{closesocket, connect, recv, send, socket, WSAStartup, SEND_RECV_FLAGS, SOCKADDR, SOCKET, SOCK_STREAM, WSADATA};

    const AF_BTH: u16 = 32;
    const BTHPROTO_RFCOMM: i32 = 3;

    #[repr(C, packed(1))]
    struct SockaddrBth {
        address_family: u16,
        bt_addr: u64,
        service_class_id: GUID,
        port: u32,
    }

    pub fn startup() {
        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(|| {
            let mut data = WSADATA::default();
            // SAFETY: valid out pointer
            unsafe {
                let _ = WSAStartup(0x0202, &mut data);
            }
        });
    }

    pub fn connect_rfcomm(addr: u64, uuid: Option<GUID>, channel: u8) -> Result<usize, String> {
        startup();
        // SAFETY: plain Winsock calls with valid arguments
        unsafe {
            let s = socket(AF_BTH as i32, SOCK_STREAM, BTHPROTO_RFCOMM).map_err(|e| e.message().to_string())?;
            let sa = SockaddrBth { address_family: AF_BTH, bt_addr: addr, service_class_id: uuid.unwrap_or(GUID::zeroed()), port: channel as u32 };
            let r = connect(s, &sa as *const _ as *const SOCKADDR, std::mem::size_of::<SockaddrBth>() as i32);
            if r != 0 {
                let err = windows::Win32::Networking::WinSock::WSAGetLastError();
                let _ = closesocket(s);
                return Err(format!("connect AF_BTH: WSA ошибка {}", err.0));
            }
            Ok(s.0)
        }
    }

    pub fn recv_into(s: usize, buf: &mut [u8]) -> i32 {
        // SAFETY: socket handle owned by the transport
        unsafe { recv(SOCKET(s), buf, SEND_RECV_FLAGS(0)) }
    }

    pub fn send_all(s: usize, mut data: &[u8]) -> Result<(), String> {
        while !data.is_empty() {
            // SAFETY: socket handle owned by the transport
            let n = unsafe { send(SOCKET(s), data, SEND_RECV_FLAGS(0)) };
            if n <= 0 {
                return Err("RFCOMM: ошибка отправки".into());
            }
            data = &data[n as usize..];
        }
        Ok(())
    }

    pub fn close(s: usize) {
        // SAFETY: closing our own socket
        unsafe {
            let _ = closesocket(SOCKET(s));
        }
    }
}

struct BthSocket {
    sock: usize,
    rx: Option<mpsc::Receiver<Vec<u8>>>,
    closed: bool,
}

impl BthSocket {
    async fn open(addr: u64, uuid: Option<&'static str>, channel: u8) -> Result<Self, String> {
        let sock = blocking(12, move || bth::connect_rfcomm(addr, uuid.map(guid), channel)).await?;
        let (tx, rx) = mpsc::channel(128);
        std::thread::Builder::new()
            .name("rfcomm-bth-rx".into())
            .spawn(move || {
                let mut buf = [0u8; 2048];
                loop {
                    let n = bth::recv_into(sock, &mut buf);
                    if n <= 0 || tx.blocking_send(buf[..n as usize].to_vec()).is_err() {
                        break;
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(BthSocket { sock, rx: Some(rx), closed: false })
    }
}

#[async_trait]
impl EarbudTransport for BthSocket {
    fn kind(&self) -> TransportKind {
        TransportKind::Rfcomm
    }
    async fn connect(&mut self) -> Result<(), String> {
        Ok(())
    }
    async fn disconnect(&mut self) {
        if !self.closed {
            self.closed = true;
            bth::close(self.sock);
        }
    }
    async fn send_frame(&mut self, data: &[u8]) -> Result<(), String> {
        let (s, d) = (self.sock, data.to_vec());
        blocking(5, move || bth::send_all(s, &d)).await
    }
    fn subscribe_rx(&mut self) -> mpsc::Receiver<Vec<u8>> {
        self.rx.take().unwrap_or_else(|| mpsc::channel(1).1)
    }
}

impl Drop for BthSocket {
    fn drop(&mut self) {
        if !self.closed {
            bth::close(self.sock);
        }
    }
}

// ---------------------------------------------------------------------------------------------
// BLE GATT (OnePlus fallback)
// ---------------------------------------------------------------------------------------------

struct BleGatt {
    device: BluetoothLEDevice,
    write: GattCharacteristic,
    _notify: GattCharacteristic,
    mtu: usize,
    rx: Option<mpsc::Receiver<Vec<u8>>>,
}

// SAFETY: agile WinRT objects
unsafe impl Send for BleGatt {}

impl BleGatt {
    async fn open(addr: u64) -> Result<Self, String> {
        let (tx, rx) = mpsc::channel::<Vec<u8>>(128);
        let res = blocking(15, move || {
            let device = BluetoothLEDevice::FromBluetoothAddressAsync(addr).map_err(werr)?.get().map_err(werr)?;
            let svcs = device.GetGattServicesForUuidWithCacheModeAsync(guid(OPO_SERVICE_UUID), BluetoothCacheMode::Uncached).map_err(werr)?.get().map_err(werr)?;
            if svcs.Status().map_err(werr)? != GattCommunicationStatus::Success {
                return Err("BLE: сервис OPO недоступен".into());
            }
            let list = svcs.Services().map_err(werr)?;
            if list.Size().map_err(werr)? == 0 {
                return Err("BLE: сервис OPO не найден".into());
            }
            let svc = list.GetAt(0).map_err(werr)?;
            let ch = |u: &str| -> Result<GattCharacteristic, String> {
                let r = svc.GetCharacteristicsForUuidAsync(guid(u)).map_err(werr)?.get().map_err(werr)?;
                let l = r.Characteristics().map_err(werr)?;
                if l.Size().map_err(werr)? == 0 {
                    return Err(format!("BLE: характеристика {u} не найдена"));
                }
                l.GetAt(0).map_err(werr)
            };
            let write = ch(OPO_BLE_WRITE_UUID)?;
            let notify = ch(OPO_BLE_NOTIFY_UUID)?;
            let tx2 = tx.clone();
            notify
                .ValueChanged(&TypedEventHandler::<GattCharacteristic, GattValueChangedEventArgs>::new(move |_, args| {
                    if let Some(a) = args.as_ref() {
                        let buf = a.CharacteristicValue()?;
                        let len = buf.Length()? as usize;
                        let reader = DataReader::FromBuffer(&buf)?;
                        let mut v = vec![0u8; len];
                        reader.ReadBytes(&mut v)?;
                        let _ = tx2.blocking_send(v);
                    }
                    Ok(())
                }))
                .map_err(werr)?;
            let st = notify
                .WriteClientCharacteristicConfigurationDescriptorAsync(GattClientCharacteristicConfigurationDescriptorValue::Notify)
                .map_err(werr)?
                .get()
                .map_err(werr)?;
            if st != GattCommunicationStatus::Success {
                return Err("BLE: не удалось включить уведомления".into());
            }
            let mtu = device
                .BluetoothDeviceId()
                .and_then(|id| GattSession::FromDeviceIdAsync(&id))
                .and_then(|op| op.get())
                .and_then(|s| s.MaxPduSize())
                .map(|m| (m as usize).saturating_sub(3).max(20))
                .unwrap_or(20);
            Ok(SendBox((device, write, mtu, notify)))
        })
        .await?
        .0;
        let (device, write, mtu, notify) = res;
        // keep `notify` alive: the ValueChanged subscription lives as long as the object
        Ok(BleGatt { device, write, _notify: notify, mtu, rx: Some(rx) })
    }
}

#[async_trait]
impl EarbudTransport for BleGatt {
    fn kind(&self) -> TransportKind {
        TransportKind::Ble
    }
    async fn connect(&mut self) -> Result<(), String> {
        Ok(())
    }
    async fn disconnect(&mut self) {
        let _ = self.device.Close();
    }
    async fn send_frame(&mut self, data: &[u8]) -> Result<(), String> {
        let w = SendBox(self.write.clone());
        let (d, mtu) = (data.to_vec(), self.mtu);
        blocking(5, move || {
            let w = w;
            for chunk in d.chunks(mtu) {
                let writer = DataWriter::new().map_err(werr)?;
                writer.WriteBytes(chunk).map_err(werr)?;
                let buf = writer.DetachBuffer().map_err(werr)?;
                // OPO firmware ignores with-response writes
                let st = w.0.WriteValueWithOptionAsync(&buf, GattWriteOption::WriteWithoutResponse).map_err(werr)?.get().map_err(werr)?;
                if st != GattCommunicationStatus::Success {
                    return Err("BLE: запись не удалась".into());
                }
            }
            Ok(())
        })
        .await
    }
    fn subscribe_rx(&mut self) -> mpsc::Receiver<Vec<u8>> {
        self.rx.take().unwrap_or_else(|| mpsc::channel(1).1)
    }
}
