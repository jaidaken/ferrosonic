//! Desktop notifications on track change via the freedesktop.org D-Bus
//! interface, which every Linux notification daemon implements, so this is
//! daemon-agnostic.

use crate::subsonic::models::Child;

/// Notification body: artist on the first line, album on the second.
pub fn track_body(song: &Child) -> String {
    let artist = song.artist.as_deref().unwrap_or("Unknown Artist");
    match song.album.as_deref() {
        Some(album) if !album.is_empty() => format!("{artist}\n{album}"),
        _ => artist.to_string(),
    }
}

#[cfg(target_os = "linux")]
pub use linux::Notifier;
#[cfg(not(target_os = "linux"))]
pub use stub::Notifier;

#[cfg(target_os = "linux")]
mod linux {
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Mutex as StdMutex;

    use tempfile::NamedTempFile;
    use tokio::sync::{Mutex, OnceCell};
    use tracing::debug;
    use zbus::zvariant::Value;
    use zbus::{proxy, Connection};

    #[proxy(
        interface = "org.freedesktop.Notifications",
        default_service = "org.freedesktop.Notifications",
        default_path = "/org/freedesktop/Notifications"
    )]
    trait Notifications {
        fn notify(
            &self,
            app_name: &str,
            replaces_id: u32,
            app_icon: &str,
            summary: &str,
            body: &str,
            actions: &[&str],
            hints: HashMap<&str, &Value<'_>>,
            expire_timeout: i32,
        ) -> zbus::Result<u32>;
    }

    /// Sends track-change notifications over the session bus. The connection is
    /// established lazily and cached; a missing session bus (headless / TTY)
    /// disables notifications instead of erroring.
    pub struct Notifier {
        conn: OnceCell<Option<Connection>>,
        last_notif_id: AtomicU32,
        last_song: StdMutex<Option<String>>,
        cover_file: Mutex<Option<NamedTempFile>>,
    }

    impl Notifier {
        /// Construct an idle notifier; no D-Bus connection is made until the
        /// first notification is shown.
        pub fn new() -> Self {
            Self {
                conn: OnceCell::new(),
                last_notif_id: AtomicU32::new(0),
                last_song: StdMutex::new(None),
                cover_file: Mutex::new(None),
            }
        }

        /// True when `song_id` differs from the last notified track, recording
        /// it so the 500ms tick fires a notification once per track change.
        pub fn mark_if_changed(&self, song_id: &str) -> bool {
            let mut last = self
                .last_song
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if last.as_deref() == Some(song_id) {
                false
            } else {
                *last = Some(song_id.to_string());
                true
            }
        }

        async fn proxy(&self) -> Option<NotificationsProxy<'_>> {
            let conn = self
                .conn
                .get_or_init(|| async { Connection::session().await.ok() })
                .await
                .as_ref()?;
            NotificationsProxy::new(conn).await.ok()
        }

        async fn cover_uri(&self, bytes: &[u8]) -> Option<String> {
            let mut guard = self.cover_file.lock().await;
            if guard.is_none() {
                *guard = tempfile::Builder::new()
                    .prefix("ferrosonic-notify-")
                    .suffix(".img")
                    .tempfile()
                    .ok();
            }
            let path = guard.as_ref()?.path().to_path_buf();
            // Atomic write off the async worker (atomic_write_bytes fsyncs);
            // the lock spans the await so concurrent writes to the path serialize.
            let dest = path.clone();
            let owned = bytes.to_vec();
            tokio::task::spawn_blocking(move || crate::io_util::atomic_write_bytes(&dest, &owned))
                .await
                .ok()?
                .ok()?;
            Some(format!("file://{}", path.display()))
        }

        /// Show or replace the track-change notification. A failed `Notify`
        /// (no daemon listening) is logged and ignored.
        pub async fn show(&self, title: &str, body: &str, cover: Option<&[u8]>) {
            let Some(proxy) = self.proxy().await else {
                return;
            };
            let uri = match cover {
                Some(bytes) => self.cover_uri(bytes).await,
                None => None,
            };
            let uri_val = uri.as_deref().map(Value::from);
            let mut hints: HashMap<&str, &Value> = HashMap::new();
            if let Some(v) = &uri_val {
                hints.insert("image-path", v);
            }
            let replaces = self.last_notif_id.load(Ordering::Relaxed);
            match proxy
                .notify("Ferrosonic", replaces, "", title, body, &[], hints, 5000)
                .await
            {
                Ok(id) => self.last_notif_id.store(id, Ordering::Relaxed),
                Err(e) => debug!("desktop notify failed: {e}"),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::track_body;
    use crate::subsonic::models::Child;

    fn song(artist: Option<&str>, album: Option<&str>) -> Child {
        Child {
            id: "x".into(),
            title: "Title".into(),
            artist: artist.map(str::to_string),
            album: album.map(str::to_string),
            ..Child::default()
        }
    }

    #[test]
    fn body_is_artist_then_album_on_two_lines() {
        assert_eq!(
            track_body(&song(Some("Boards"), Some("Geogaddi"))),
            "Boards\nGeogaddi"
        );
    }

    #[test]
    fn body_drops_the_album_line_when_absent_or_empty() {
        assert_eq!(track_body(&song(Some("Boards"), None)), "Boards");
        assert_eq!(track_body(&song(Some("Boards"), Some(""))), "Boards");
    }

    #[test]
    fn body_falls_back_when_artist_missing() {
        assert_eq!(
            track_body(&song(None, Some("Geogaddi"))),
            "Unknown Artist\nGeogaddi"
        );
    }
}

#[cfg(not(target_os = "linux"))]
mod stub {
    /// No-op notifier on non-Linux targets (no freedesktop notifications).
    pub struct Notifier;
    impl Notifier {
        pub fn new() -> Self {
            Self
        }
        pub fn mark_if_changed(&self, _song_id: &str) -> bool {
            false
        }
        pub async fn show(&self, _title: &str, _body: &str, _cover: Option<&[u8]>) {}
    }
}
