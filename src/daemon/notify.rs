//! Desktop notifications on track change.
//!
//! Linux uses the freedesktop.org D-Bus interface, which every Linux
//! notification daemon implements. macOS has no such interface, so it shells
//! out to `osascript`'s `display notification`. Other platforms get a no-op.

use crate::subsonic::models::Child;

/// Notification body: artist on the first line, album on the second.
#[must_use]
pub fn track_body(song: &Child) -> String {
    let artist = song.artist.as_deref().unwrap_or("Unknown Artist");
    match song.album.as_deref() {
        Some(album) if !album.is_empty() => format!("{artist}\n{album}"),
        _ => artist.to_string(),
    }
}

#[cfg(target_os = "linux")]
pub use linux::Notifier;
#[cfg(target_os = "macos")]
pub use macos::Notifier;
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
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
        // org.freedesktop.Notifications.Notify signature is fixed by the D-Bus spec.
        #[allow(clippy::too_many_arguments)]
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

    impl Default for Notifier {
        fn default() -> Self {
            Self::new()
        }
    }

    impl Notifier {
        /// Construct an idle notifier; no D-Bus connection is made until the
        /// first notification is shown.
        #[must_use]
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
            // Test/headless guard: never touch the real session bus when set. The
            // test harness sets it (tests/common) so the suite cannot spam the desktop.
            if std::env::var_os("FERROSONIC_NO_DESKTOP_NOTIFY").is_some() {
                return None;
            }
            let conn = self
                .conn
                .get_or_init(|| async { Connection::session().await.ok() })
                .await
                .as_ref()?;
            NotificationsProxy::new(conn).await.ok()
        }

        // cover_file lock intentionally spans the spawn_blocking write so concurrent
        // cover writes to the shared tempfile path serialize; do not tighten.
        #[allow(clippy::significant_drop_tightening)]
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
            let mut hints: HashMap<&str, &Value<'_>> = HashMap::new();
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

/// Build the `osascript` invocation that displays `body` under `title`.
///
/// The text is passed as `argv` and read back inside `on run argv`, so no
/// `AppleScript` string escaping is needed and a title/body containing quotes (or
/// anything else) cannot alter the script.
///
/// Compiled under `test` as well so the argv construction is covered by a unit
/// test on any host; only macOS actually runs it.
#[cfg(any(target_os = "macos", test))]
fn osascript_notification_command(title: &str, body: &str) -> tokio::process::Command {
    const SCRIPT: &str = "on run argv\n\
                          \tdisplay notification (item 2 of argv) with title (item 1 of argv)\n\
                          \tend run";
    let mut cmd = tokio::process::Command::new("osascript");
    cmd.arg("-e").arg(SCRIPT).arg("--").arg(title).arg(body);
    cmd
}

#[cfg(target_os = "macos")]
mod macos {
    use std::sync::Mutex as StdMutex;

    use tracing::debug;

    /// Track-change notifications on macOS via `osascript`.
    ///
    /// macOS ships no freedesktop notification daemon, so the Linux D-Bus path
    /// does not apply. `AppleScript`'s `display notification` is the built-in
    /// equivalent and needs no extra dependency. Cover art is not attached —
    /// `display notification` has no image parameter — so only the title and
    /// the artist/album body are shown.
    pub struct Notifier {
        last_song: StdMutex<Option<String>>,
    }

    impl Default for Notifier {
        fn default() -> Self {
            Self::new()
        }
    }

    impl Notifier {
        /// Construct an idle notifier; nothing runs until the first change.
        #[must_use]
        pub fn new() -> Self {
            Self {
                last_song: StdMutex::new(None),
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

        /// Show a track-change notification. Text is passed as `argv` and read
        /// back inside `on run argv`, so no `AppleScript` string escaping is
        /// needed and an arbitrary title/body cannot alter the script. A failed
        /// `osascript` (notifications disabled, no GUI session) is logged and
        /// ignored, matching the Linux notifier's best-effort behaviour.
        // `&self` is unused here but kept for API parity with the Linux notifier.
        #[allow(clippy::unused_self)]
        pub async fn show(&self, title: &str, body: &str, _cover: Option<&[u8]>) {
            match super::osascript_notification_command(title, body)
                .output()
                .await
            {
                Ok(out) if out.status.success() => {}
                Ok(out) => debug!(
                    "desktop notify failed (osascript {}): {}",
                    out.status,
                    String::from_utf8_lossy(&out.stderr).trim()
                ),
                Err(e) => debug!("desktop notify failed (osascript spawn): {e}"),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{osascript_notification_command, track_body};
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

    #[test]
    fn osascript_command_passes_text_as_argv_not_in_script() {
        let cmd = osascript_notification_command("Title \"quoted\"", "Artist\nAlbum");
        let args: Vec<String> = cmd
            .as_std()
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        // Layout: osascript -e <script> -- <title> <body>. Every fixed slot is
        // checked so a reordering cannot silently break the argv passing.
        assert_eq!(args.len(), 5);
        assert_eq!(args[0], "-e");
        assert_eq!(args[2], "--");
        assert_eq!(args[3], "Title \"quoted\"");
        assert_eq!(args[4], "Artist\nAlbum");
        // Untrusted text is never interpolated into the AppleScript source.
        assert!(args[1].contains("on run argv"));
        assert!(
            !args[1].contains("quoted"),
            "title must not be embedded in the script"
        );
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
mod stub {
    /// No-op notifier on platforms with no notification backend wired up.
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
