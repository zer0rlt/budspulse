//! System media control: pause what is playing, resume exactly those players later.
//! Linux: MPRIS2 over the session D-Bus. Windows: GlobalSystemMediaTransportControls (GSMTC).

/// Pause every player that is currently playing. Returns identifiers to resume later.
pub async fn pause_playing() -> Vec<String> {
    #[cfg(target_os = "linux")]
    {
        return linux::pause_playing().await.unwrap_or_default();
    }
    #[cfg(windows)]
    {
        return tokio::task::spawn_blocking(win::pause_playing).await.unwrap_or_default();
    }
    #[allow(unreachable_code)]
    Vec::new()
}

pub async fn resume(ids: &[String]) {
    let _ = ids;
    #[cfg(target_os = "linux")]
    {
        let _ = linux::resume(ids).await;
    }
    #[cfg(windows)]
    {
        let ids = ids.to_vec();
        let _ = tokio::task::spawn_blocking(move || win::resume(&ids)).await;
    }
}

#[cfg(target_os = "linux")]
pub mod linux {
    use zbus::{fdo::DBusProxy, Connection, Proxy};

    const PLAYER_IFACE: &str = "org.mpris.MediaPlayer2.Player";
    const PATH: &str = "/org/mpris/MediaPlayer2";

    async fn players(conn: &Connection) -> zbus::Result<Vec<String>> {
        let dbus = DBusProxy::new(conn).await?;
        Ok(dbus.list_names().await?.into_iter().map(|n| n.to_string()).filter(|n| n.starts_with("org.mpris.MediaPlayer2.")).collect())
    }

    pub async fn pause_playing() -> zbus::Result<Vec<String>> {
        let conn = Connection::session().await?;
        let mut paused = Vec::new();
        for name in players(&conn).await? {
            let p = Proxy::new(&conn, name.as_str(), PATH, PLAYER_IFACE).await?;
            let status: String = p.get_property("PlaybackStatus").await.unwrap_or_default();
            if status == "Playing" && p.call_method("Pause", &()).await.is_ok() {
                paused.push(name);
            }
        }
        Ok(paused)
    }

    pub async fn resume(ids: &[String]) -> zbus::Result<()> {
        let conn = Connection::session().await?;
        for name in ids {
            if let Ok(p) = Proxy::new(&conn, name.as_str(), PATH, PLAYER_IFACE).await {
                let _ = p.call_method("Play", &()).await;
            }
        }
        Ok(())
    }
}

#[cfg(windows)]
pub mod win {
    use windows::Media::Control::{GlobalSystemMediaTransportControlsSessionManager as Manager, GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status};

    pub fn pause_playing() -> Vec<String> {
        let mut out = Vec::new();
        let Ok(mgr) = Manager::RequestAsync().and_then(|op| op.get()) else { return out };
        let Ok(sessions) = mgr.GetSessions() else { return out };
        for s in sessions {
            let playing = s.GetPlaybackInfo().and_then(|i| i.PlaybackStatus()).map(|st| st == Status::Playing).unwrap_or(false);
            if !playing {
                continue;
            }
            let id = s.SourceAppUserModelId().map(|h| h.to_string()).unwrap_or_default();
            if s.TryPauseAsync().and_then(|op| op.get()).unwrap_or(false) {
                out.push(id);
            }
        }
        out
    }

    pub fn resume(ids: &[String]) {
        let Ok(mgr) = Manager::RequestAsync().and_then(|op| op.get()) else { return };
        let Ok(sessions) = mgr.GetSessions() else { return };
        for s in sessions {
            let id = s.SourceAppUserModelId().map(|h| h.to_string()).unwrap_or_default();
            if ids.contains(&id) {
                let _ = s.TryPlayAsync().and_then(|op| op.get());
            }
        }
    }
}
