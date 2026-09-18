//! Fetch the selected playlist's cover art for the Playlists page.
//!
//! Runs on the tick path so a slow server cannot delay input or rendering,
//! and stores the image in a state separate from the now-playing cover art so
//! the two never overwrite each other.

use crate::app::state::Page;
use crate::ipc::{DaemonRequest, DaemonResponse};

use super::App;

impl App {
    /// Start a cover-art fetch when the Playlists page is active and the
    /// highlighted playlist has cover art. No-op when the image is already
    /// held or a fetch is pending; a failed fetch clears the reservation so
    /// the next tick retries.
    pub(super) async fn ensure_playlist_cover(&self) {
        let selected = {
            let cs = self.client_state.read().await;
            if cs.page != Page::Playlists || !cs.settings_state.cover_art {
                return;
            }
            cs.playlists.selected_playlist
        };
        let cover_id = {
            let ds = self.daemon_state.read().await;
            selected
                .and_then(|i| ds.library.playlists.get(i))
                .and_then(|p| p.cover_art.clone())
        };
        let Some(id) = cover_id else {
            let mut guard = self
                .playlist_cover_art
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if guard.current_id.is_some() {
                guard.clear();
            }
            drop(guard);
            return;
        };
        {
            let guard = self
                .playlist_cover_art
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            // Same id is either already loaded or an in-flight reservation;
            // both mean there is nothing to do this tick.
            if guard.current_id.as_deref() == Some(id.as_str()) {
                return;
            }
        }
        self.playlist_cover_art
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .set_pending(id.clone());

        let client = self.client.clone();
        let state = self.playlist_cover_art.clone();
        tokio::spawn(async move {
            if let Ok(DaemonResponse::CoverArt(bytes)) = client
                .request(DaemonRequest::FetchCoverArt {
                    id: id.clone(),
                    size: 256,
                })
                .await
            {
                if !bytes.is_empty() {
                    state
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .load(id, &bytes);
                    return;
                }
            }
            state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .fail_pending(&id);
        });
    }
}
