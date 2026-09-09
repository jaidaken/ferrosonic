//! Lyrics overlay input and non-blocking retrieval.

use crossterm::event::{KeyCode, KeyEvent};

use crate::app::page_state::LyricsStatus;
use crate::ipc::{DaemonRequest, DaemonResponse};

use super::App;

impl App {
    /// Open or close lyrics for the currently loaded song.
    pub(super) async fn toggle_lyrics(&self) {
        {
            let mut client = self.client_state.write().await;
            if client.lyrics.open {
                client.lyrics.open = false;
                drop(client);
                return;
            }
            drop(client);
        }

        if self.daemon_state.read().await.now_playing.song.is_none() {
            self.client_state
                .write()
                .await
                .notify("Nothing is playing to show lyrics for");
            return;
        }

        self.client_state.write().await.lyrics.open = true;
        self.ensure_open_lyrics().await;
    }

    /// Start a lyric request when an open overlay has no result for the
    /// currently loaded song. The request itself runs outside the input/tick
    /// path so a slow server cannot delay playback controls or rendering.
    pub(super) async fn ensure_open_lyrics(&self) {
        let song = self.daemon_state.read().await.now_playing.song.clone();
        let Some(song) = song else {
            let mut client = self.client_state.write().await;
            if client.lyrics.open {
                client.lyrics.song_id = None;
                client.lyrics.status = LyricsStatus::Empty;
            }
            drop(client);
            return;
        };

        let generation = {
            let mut client = self.client_state.write().await;
            if !client.lyrics.open {
                return;
            }
            let changed_song = client.lyrics.song_id.as_deref() != Some(song.id.as_str());
            if !changed_song && !matches!(client.lyrics.status, LyricsStatus::Idle) {
                return;
            }

            client.lyrics.song_id = Some(song.id.clone());
            client.lyrics.selected_source = 0;
            client.lyrics.scroll = 0;
            client.lyrics.follow = true;
            if let Some(cached) = client.lyrics.cache.get(&song.id).cloned() {
                client.lyrics.status = if cached.is_empty() {
                    LyricsStatus::Empty
                } else {
                    LyricsStatus::Ready(cached)
                };
                return;
            }
            client.lyrics.request_generation = client.lyrics.request_generation.wrapping_add(1);
            client.lyrics.status = LyricsStatus::Loading;
            client.lyrics.request_generation
        };

        let daemon = self.client.clone();
        let client_state = self.client_state.clone();
        tokio::spawn(async move {
            let song_id = song.id.clone();
            let result = daemon
                .request(DaemonRequest::FetchLyrics {
                    id: song.id,
                    artist: song.artist,
                    title: song.title,
                })
                .await;
            let mut client = client_state.write().await;
            match result {
                Ok(DaemonResponse::Lyrics(sources)) => {
                    client.lyrics.cache.insert(song_id.clone(), sources.clone());
                    if client.lyrics.request_generation == generation
                        && client.lyrics.song_id.as_deref() == Some(song_id.as_str())
                    {
                        client.lyrics.status = if sources.is_empty() {
                            LyricsStatus::Empty
                        } else {
                            LyricsStatus::Ready(sources)
                        };
                    }
                }
                Ok(_) => {
                    if client.lyrics.request_generation == generation
                        && client.lyrics.song_id.as_deref() == Some(song_id.as_str())
                    {
                        client.lyrics.status = LyricsStatus::Error(
                            "The daemon returned an invalid lyrics reply".into(),
                        );
                    }
                }
                Err(error) => {
                    if client.lyrics.request_generation == generation
                        && client.lyrics.song_id.as_deref() == Some(song_id.as_str())
                    {
                        client.lyrics.status = LyricsStatus::Error(error.to_string());
                    }
                }
            }
        });
    }

    /// Handle a key while the lyrics overlay owns input.
    pub(super) async fn handle_lyrics_key(&self, key: KeyEvent) {
        let mut retry = false;
        {
            let mut client = self.client_state.write().await;
            let lyrics = &mut client.lyrics;
            match key.code {
                KeyCode::Esc | KeyCode::Char('y' | 'Y') => lyrics.open = false,
                KeyCode::Down | KeyCode::Char('j') => {
                    lyrics.scroll = lyrics.scroll.saturating_add(1);
                    lyrics.follow = false;
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    lyrics.scroll = lyrics.scroll.saturating_sub(1);
                    lyrics.follow = false;
                }
                KeyCode::PageDown => {
                    lyrics.scroll = lyrics.scroll.saturating_add(8);
                    lyrics.follow = false;
                }
                KeyCode::PageUp => {
                    lyrics.scroll = lyrics.scroll.saturating_sub(8);
                    lyrics.follow = false;
                }
                KeyCode::Home => {
                    lyrics.scroll = 0;
                    lyrics.follow = false;
                }
                KeyCode::Char('f') => lyrics.follow = !lyrics.follow,
                KeyCode::Left | KeyCode::Char('h') => {
                    if let LyricsStatus::Ready(sources) = &lyrics.status {
                        lyrics.selected_source = lyrics.selected_source.saturating_sub(1);
                        lyrics.selected_source =
                            lyrics.selected_source.min(sources.len().saturating_sub(1));
                        lyrics.scroll = 0;
                    }
                }
                KeyCode::Right | KeyCode::Char('l') => {
                    if let LyricsStatus::Ready(sources) = &lyrics.status {
                        lyrics.selected_source = lyrics
                            .selected_source
                            .saturating_add(1)
                            .min(sources.len().saturating_sub(1));
                        lyrics.scroll = 0;
                    }
                }
                KeyCode::Char('r') => {
                    if let Some(id) = lyrics.song_id.clone() {
                        lyrics.cache.remove(&id);
                    }
                    lyrics.status = LyricsStatus::Idle;
                    retry = true;
                }
                _ => {}
            }
            drop(client);
        }
        if retry {
            self.ensure_open_lyrics().await;
        }
    }
}
