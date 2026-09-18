//! Artist/album info overlay: on-demand fetch, modal keys, and the
//! selection-to-target helper used by the Library page.

use crossterm::event::{self, KeyCode};

use crate::app::page_state::{InfoKind, InfoPayload, InfoStatus, LibraryView};
use crate::app::state::AppState;
use crate::error::Error;
use crate::ipc::{DaemonRequest, DaemonResponse};
use crate::subsonic::models::{AlbumInfo, ArtistInfo2};
use crate::ui::pages::library::{build_tree_items, TreeItem};

use super::App;

/// True when the server returned no usable artist information. Navidrome
/// without an external integration returns an all-empty object, which is an
/// empty state, not an error.
fn artist_info_empty(info: &ArtistInfo2) -> bool {
    info.biography.as_deref().unwrap_or("").trim().is_empty()
        && info.music_brainz_id.is_none()
        && info.last_fm_url.is_none()
        && info.similar_artist.is_empty()
}

/// True when the server returned no usable album information.
fn album_info_empty(info: &AlbumInfo) -> bool {
    info.notes.as_deref().unwrap_or("").trim().is_empty()
        && info.music_brainz_id.is_none()
        && info.last_fm_url.is_none()
}

impl App {
    /// Resolve the highlighted Library tree/album-list row to an info target.
    /// `None` when the highlight is a song, a greyed context label, or the
    /// focus is on the song pane.
    pub(super) fn selected_artist_or_album(
        state: &AppState<'_>,
    ) -> Option<(InfoKind, String, String)> {
        let artists = &state.client.artists;
        if artists.focus != 0 {
            return None;
        }
        if artists.view == LibraryView::AlbumList {
            return artists
                .album_selected
                .and_then(|i| artists.albums.get(i))
                .map(|album| (InfoKind::Album, album.id.clone(), album.name.clone()));
        }
        let items = build_tree_items(state);
        let item = artists.selected_index.and_then(|i| items.get(i))?;
        match item {
            TreeItem::Artist { artist, .. } => {
                Some((InfoKind::Artist, artist.id.clone(), artist.name.clone()))
            }
            TreeItem::Album { album } => {
                Some((InfoKind::Album, album.id.clone(), album.name.clone()))
            }
            TreeItem::Song { .. } | TreeItem::ArtistLabel { .. } | TreeItem::AlbumLabel { .. } => {
                None
            }
        }
    }

    /// Open the info overlay for `kind`/`id` and fetch its data on a
    /// background task. A newer open supersedes an in-flight one.
    pub(super) async fn open_info(&self, kind: InfoKind, id: String, title: String) {
        let generation = {
            let mut cs = self.client_state.write().await;
            cs.info.open = true;
            cs.info.kind = kind;
            cs.info.target_id = Some(id.clone());
            cs.info.title = title;
            cs.info.scroll = 0;
            cs.info.status = InfoStatus::Loading;
            cs.info.request_generation = cs.info.request_generation.wrapping_add(1);
            cs.info.request_generation
        };

        let client = self.client.clone();
        let client_state = self.client_state.clone();
        tokio::spawn(async move {
            let resp = match kind {
                InfoKind::Artist => {
                    client
                        .request(DaemonRequest::FetchArtistInfo { id: id.clone() })
                        .await
                }
                InfoKind::Album => {
                    client
                        .request(DaemonRequest::FetchAlbumInfo { id: id.clone() })
                        .await
                }
            };
            let mut cs = client_state.write().await;
            if cs.info.request_generation != generation
                || cs.info.target_id.as_deref() != Some(id.as_str())
            {
                return;
            }
            cs.info.status = match resp {
                Ok(DaemonResponse::ArtistInfo(info)) if !artist_info_empty(&info) => {
                    InfoStatus::Ready(InfoPayload::Artist(*info))
                }
                Ok(DaemonResponse::AlbumInfo(info)) if !album_info_empty(&info) => {
                    InfoStatus::Ready(InfoPayload::Album(*info))
                }
                Ok(_) => InfoStatus::Empty,
                Err(e) => InfoStatus::Error(e.to_string()),
            };
        });
    }

    /// Modal key handling while the info overlay is open.
    ///
    /// # Errors
    /// Returns an `Error` if a retry request fails to dispatch.
    pub(super) async fn handle_info_key(&self, key: event::KeyEvent) -> Result<(), Error> {
        let mut retry: Option<(InfoKind, String, String)> = None;
        {
            let mut cs = self.client_state.write().await;
            match key.code {
                KeyCode::Esc | KeyCode::Char('I') => {
                    cs.info.open = false;
                    cs.info.status = InfoStatus::Idle;
                    cs.info.target_id = None;
                    cs.info.scroll = 0;
                }
                KeyCode::Char('j') | KeyCode::Down => {
                    cs.info.scroll = cs.info.scroll.saturating_add(1);
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    cs.info.scroll = cs.info.scroll.saturating_sub(1);
                }
                KeyCode::PageDown => cs.info.scroll = cs.info.scroll.saturating_add(10),
                KeyCode::PageUp => cs.info.scroll = cs.info.scroll.saturating_sub(10),
                KeyCode::Char('r') => {
                    if let Some(id) = cs.info.target_id.clone() {
                        retry = Some((cs.info.kind, id, cs.info.title.clone()));
                    }
                }
                _ => {}
            }
        }
        if let Some((kind, id, title)) = retry {
            self.open_info(kind, id, title).await;
        }
        Ok(())
    }
}
