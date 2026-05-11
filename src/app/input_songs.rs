use crossterm::event::{self, KeyCode};

use crate::app::models::SongOption;
use crate::error::Error;

use super::*;

impl App {
    pub(super) async fn handle_songs_key(&mut self, key: event::KeyEvent) -> Result<(), Error> {
        let ds = self.daemon_state.read().await;
        let mut cs = self.client_state.write().await;
        let state = AppState {
            daemon: &ds,
            client: &mut cs,
        };
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => match state.client.songs.focus {
                0 => {
                    match state.client.songs.selected_option {
                        Some(SongOption::Starred) => {}
                        Some(SongOption::Random) => {
                            state.client.songs.selected_option = Some(SongOption::Starred);
                            drop(state);
                            drop(cs);
                            drop(ds);
                            let _ = self.client.request(DaemonRequest::RefreshStarred).await;
                        }
                        None => {}
                    };
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
                    match state.client.songs.selected_option {
                        Some(SongOption::Starred) => {
                            state.client.songs.selected_option = Some(SongOption::Random);
                            drop(state);
                            drop(cs);
                            drop(ds);
                            let _ = self.client.request(DaemonRequest::RefreshRandom).await;
                        }
                        Some(SongOption::Random) => {}
                        None => {}
                    };
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
                drop(state);
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
                state.client.songs.focus = if state.client.songs.focus == 1 { 0 } else { 1 }
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
                drop(state);
                drop(cs);
                drop(ds);
                if let Some(id) = song_id {
                    let _ = self.client.request(DaemonRequest::ToggleStarSong(id)).await;
                }
                return Ok(());
            }
            _ => {}
        }

        Ok(())
    }
}
