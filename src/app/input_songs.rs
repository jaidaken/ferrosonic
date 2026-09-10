use crossterm::event::{self, KeyCode};

use crate::app::models::SongOption;
use crate::error::Error;
use strum::IntoEnumIterator;

use super::{App, AppState, DaemonRequest, EnqueueMode};

impl App {
    // Empty arms kept per-variant explicit; merging would hide which enum cases are handled.
    #[allow(clippy::match_same_arms)]
    // Cohesive single match/render; splitting would fragment one logical unit.
    #[allow(clippy::too_many_lines)]
    pub(super) async fn handle_songs_key(&self, key: event::KeyEvent) -> Result<(), Error> {
        let ds = self.daemon_state.read().await;
        let mut cs = self.client_state.write().await;
        let state = AppState {
            daemon: &ds,
            client: &mut cs,
        };
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => match state.client.songs.focus {
                0 => {
                    if let Some(option) =
                        state.client.songs.selected_option.and_then(previous_option)
                    {
                        state.client.songs.selected_option = Some(option);
                        state.client.songs.selected_index = None;
                        state.client.songs.scroll_offset = 0;
                        let _ = state;
                        drop(cs);
                        drop(ds);
                        let _ = self.client.request(option.refresh_request()).await;
                    }
                }
                1 => {
                    if let Some(sel) = state.client.songs.selected_index {
                        if sel > 0 {
                            state.client.songs.selected_index = Some(sel - 1);
                        }
                    } else if !state.songs_list().is_empty() {
                        state.client.songs.selected_index = Some(0);
                    }
                }
                _ => {}
            },
            KeyCode::Down | KeyCode::Char('j') => match state.client.songs.focus {
                0 => {
                    if let Some(option) = state.client.songs.selected_option.and_then(next_option) {
                        state.client.songs.selected_option = Some(option);
                        state.client.songs.selected_index = None;
                        state.client.songs.scroll_offset = 0;
                        let _ = state;
                        drop(cs);
                        drop(ds);
                        let _ = self.client.request(option.refresh_request()).await;
                    }
                }
                1 => {
                    let max = state.songs_list().len().saturating_sub(1);
                    if let Some(sel) = state.client.songs.selected_index {
                        if sel < max {
                            state.client.songs.selected_index = Some(sel + 1);
                        }
                    } else if !state.songs_list().is_empty() {
                        state.client.songs.selected_index = Some(0);
                    }
                }
                _ => {}
            },
            KeyCode::Enter => {
                let selected_song = state
                    .client
                    .songs
                    .selected_index
                    .filter(|&idx| idx < state.songs_list().len());

                let Some(selected_song) = selected_song else {
                    return Ok(());
                };

                let songs = state.songs_list().to_vec();
                let _ = state;
                drop(cs);
                drop(ds);

                return self
                    .client
                    .request(DaemonRequest::EnqueueSongs {
                        songs,
                        mode: EnqueueMode::Replace {
                            play_from: Some(selected_song),
                        },
                    })
                    .await
                    .map(|_| ())
                    .map_err(Error::from);
            }
            KeyCode::Tab => {
                state.client.songs.focus = usize::from(state.client.songs.focus != 1);
            }
            KeyCode::Left => {
                state.client.songs.focus = 0;
            }
            KeyCode::Right if !state.songs_list().is_empty() => {
                state.client.songs.focus = 1;
                if state.client.songs.selected_index.is_none() {
                    state.client.songs.selected_index = Some(0);
                }
            }
            KeyCode::Char('m') => {
                let song_id = state
                    .client
                    .songs
                    .selected_index
                    .and_then(|idx| state.songs_list().get(idx).map(|s| s.id.clone()));
                let _ = state;
                drop(cs);
                drop(ds);
                if let Some(id) = song_id {
                    let _ = self.client.request(DaemonRequest::ToggleStarSong(id)).await;
                }
                return Ok(());
            }
            KeyCode::Char('a') => {
                let idx = state.client.songs.selected_index;
                let song = idx.and_then(|i| state.songs_list().get(i).cloned());
                if let Some(song) = song {
                    if state.daemon.library.playlists.is_empty() {
                        state.client.notify("No playlists to add to");
                    } else {
                        state.client.open_playlist_picker(song);
                    }
                }
            }
            _ => {}
        }

        Ok(())
    }
}

fn previous_option(current: SongOption) -> Option<SongOption> {
    let options = SongOption::iter().collect::<Vec<_>>();
    let index = options.iter().position(|option| *option == current)?;
    index
        .checked_sub(1)
        .and_then(|index| options.get(index).copied())
}

fn next_option(current: SongOption) -> Option<SongOption> {
    let options = SongOption::iter().collect::<Vec<_>>();
    let index = options.iter().position(|option| *option == current)?;
    index
        .checked_add(1)
        .and_then(|index| options.get(index).copied())
}
