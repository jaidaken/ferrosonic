use crossterm::event::{self, KeyCode};

use crate::app::models::SongOption;
use crate::error::Error;

use super::*;

impl App {
    pub(super) async fn handle_songs_key(&mut self, key: event::KeyEvent) -> Result<(), Error> {
        let mut state = self.state.write().await;
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => match state.songs.focus {
                0 => {
                    match state.songs.selected_option {
                        Some(SongOption::Starred) => {}
                        Some(SongOption::Random) => {
                            state.songs.selected_option = Some(SongOption::Starred);
                            state.songs.scroll_offset = 0;
                            drop(state);
                            self.get_starred_songs().await;
                        }
                        None => {}
                    };
                }
                1 => {
                    if let Some(sel) = state.songs.selected_index {
                        if sel > 0 {
                            state.songs.selected_index = Some(sel - 1);
                        }
                    } else if !state.songs.songs.is_empty() {
                        state.songs.selected_index = Some(0);
                    }
                }
                _ => {}
            },
            KeyCode::Down | KeyCode::Char('j') => match state.songs.focus {
                0 => {
                    match state.songs.selected_option {
                        Some(SongOption::Starred) => {
                            state.songs.selected_option = Some(SongOption::Random);
                            state.songs.scroll_offset = 0;
                            drop(state);
                            self.get_random_songs().await;
                        }
                        Some(SongOption::Random) => {}
                        None => {}
                    };
                }
                1 => {
                    let max = state.songs.songs.len().saturating_sub(1);
                    if let Some(sel) = state.songs.selected_index {
                        if sel < max {
                            state.songs.selected_index = Some(sel + 1);
                        }
                    } else if !state.songs.songs.is_empty() {
                        state.songs.selected_index = Some(0);
                    }
                }
                _ => {}
            },
            KeyCode::Enter => {
                let selected_song = state
                    .songs
                    .selected_index
                    .filter(|&idx| idx < state.songs.songs.len());

                let Some(selected_song) = selected_song else {
                    return Ok(());
                };

                state.queue.clear();
                let songs = state.songs.songs.clone();
                state.queue.extend(songs);

                drop(state);

                return self.play_queue_position(selected_song).await;
            }
            KeyCode::Tab => state.songs.focus = if state.songs.focus == 1 { 0 } else { 1 },
            _ => {}
        }

        return Ok(());
    }
}
