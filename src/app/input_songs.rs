use crossterm::event::{self, KeyCode};

use crate::app::models::SongOption;
use crate::error::Error;

use super::*;

impl App {
    pub(super) async fn handle_songs_key(&mut self, key: event::KeyEvent) -> Result<(), Error> {
        let mut state = self.state.write().await;
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => match state.client.songs.focus {
                0 => {
                    match state.client.songs.selected_option {
                        Some(SongOption::Starred) => {}
                        Some(SongOption::Random) => {
                            state.client.songs.selected_option = Some(SongOption::Starred);
                            drop(state);
                            self.core.refresh_starred().await;
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
                            self.core.refresh_random().await;
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
                let selected_song = state.client
                    .songs
                    .selected_index
                    .filter(|&idx| idx < state.songs_list().len());

                let Some(selected_song) = selected_song else {
                    return Ok(());
                };

                state.daemon.queue.clear();
                let songs = state.songs_list().to_vec();
                state.daemon.queue.extend(songs);

                drop(state);

                return self.core.play_queue_position(selected_song).await;
            }
            KeyCode::Tab => state.client.songs.focus = if state.client.songs.focus == 1 { 0 } else { 1 },
            _ => {}
        }

        return Ok(());
    }
}
