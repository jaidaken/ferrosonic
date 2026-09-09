use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use tracing::{debug, error};

use crate::config::keybind::{GlobalAction, KeyChord};
use crate::error::Error;

use super::{App, AppState, DaemonRequest, Page};

impl App {
    /// Handle terminal events. Pub for integration tests; production
    /// callers use `App::run`.
    ///
    /// # Errors
    /// Returns an `Error` if the daemon request fails.
    pub async fn handle_event(&mut self, event: Event) -> Result<(), Error> {
        match event {
            Event::Key(key) => {
                // Only handle key press events, ignore release and repeat
                if key.kind == event::KeyEventKind::Press {
                    self.handle_key(key).await
                } else {
                    Ok(())
                }
            }
            Event::Mouse(mouse) => self.handle_mouse(mouse).await,
            Event::Resize(_, _) => {
                if self.cava_parser.is_some() {
                    let (g, h, cava_h) = {
                        let cs = self.client_state.read().await;
                        let td = cs.settings_state.current_theme();
                        (
                            td.cava_gradient.clone(),
                            td.cava_horizontal_gradient.clone(),
                            u32::from(cs.settings_state.cava_size),
                        )
                    };
                    self.start_cava(&g, &h, cava_h);
                    let mut cs = self.client_state.write().await;
                    cs.cava_screen.clear();
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    /// Route one key event to the active page handler.
    ///
    /// # Errors
    /// Returns an `Error` if the daemon request fails.
    // Cohesive single match/render; splitting would fragment one logical unit.
    #[allow(clippy::too_many_lines)]
    // significant_drop_tightening: tokio guard held to scope; not tightened (early-drop is borrow-blocked, spans a trailing await, or saves nothing before return).
    #[allow(clippy::significant_drop_tightening)]
    pub async fn handle_key(&mut self, key: event::KeyEvent) -> Result<(), Error> {
        // 'p' is a permanent secondary alias for TogglePause, reserved
        // ahead of the configurable keymap so a `[Keybindings]` override
        // that remaps some other action onto plain 'p' can never shadow it.
        // Every other global action is resolved from `self.keymap` (defaults
        // merged with overrides).
        let action = if key.code == KeyCode::Char('p') && key.modifiers == KeyModifiers::NONE {
            Some(GlobalAction::TogglePause)
        } else if matches!(key.code, KeyCode::Char('1'..='5'))
            && matches!(key.modifiers, KeyModifiers::NONE | KeyModifiers::ALT)
        {
            // Digit keys 1-5 are reserved for song rating (handled below,
            // outside the configurable keymap) and must never be shadowed
            // by a `[Keybindings]` override remapping some other action
            // onto a digit chord. Alt+digit rates the highlighted row and
            // is reserved on the same grounds.
            None
        } else {
            self.keymap.get(&KeyChord::from(key)).copied()
        };

        let ds = self.daemon_state.read().await;
        let mut cs = self.client_state.write().await;
        let state = AppState {
            daemon: &ds,
            client: &mut cs,
        };

        state.client.clear_notification();

        // Quit-confirm prompt: while it is up, only y/n/esc do anything.
        if state.client.quit_prompt {
            match key.code {
                KeyCode::Char('y' | 'Y') => {
                    state.client.quit_prompt = false;
                    state.client.should_quit = true;
                    let _ = state;
                    drop(cs);
                    drop(ds);
                    let _ = self.client.request(DaemonRequest::Shutdown).await;
                    return Ok(());
                }
                KeyCode::Char('n' | 'N') => {
                    state.client.quit_prompt = false;
                    state.client.should_quit = true;
                    return Ok(());
                }
                KeyCode::Esc => {
                    state.client.quit_prompt = false;
                    return Ok(());
                }
                _ => return Ok(()),
            }
        }

        // Add-to-playlist picker: while open, the overlay owns every key.
        if state.client.playlist_picker.active {
            let _ = state;
            drop(cs);
            drop(ds);
            return self.handle_playlist_picker_key(key).await;
        }

        // Lyrics is global and modal while visible, including on narrow pages.
        if state.client.lyrics.open {
            let _ = state;
            drop(cs);
            drop(ds);
            self.handle_lyrics_key(key).await;
            return Ok(());
        }

        // Settings editors are modal and must receive keys such as q, Space,
        // and function keys before the configurable global dispatcher.
        if state.client.settings_state.filter_editor.is_some()
            || state.client.settings_state.keybinding_editor.is_some()
        {
            let _ = state;
            drop(cs);
            drop(ds);
            return self.handle_settings_editor_key(key).await;
        }

        // Settings' own h/l/space field-navigation bindings are checked
        // ahead of everything else below: they must win even when a
        // `[Keybindings]` override happens to resolve one of these keys to
        // a page-switch action, or field nav would silently break on
        // Settings whenever the user remaps e.g. GoToLibrary onto 'h'.
        if state.client.page == Page::Settings && matches!(key.code, KeyCode::Char('h' | 'l' | ' '))
        {
            let _ = state;
            drop(cs);
            drop(ds);
            return self.handle_settings_key(key).await;
        }

        // Page switches revert unsaved edits first, driven off the
        // resolved action (not the literal F-key) so remapping a page
        // switch away from its default key keeps this cleanup working.
        let is_page_switch = action.is_some_and(GlobalAction::is_page_switch);
        if is_page_switch {
            if state.client.page == Page::Server {
                let cfg = state.daemon.config.clone();
                state.client.server_state.base_url = cfg.base_url;
                state.client.server_state.username = cfg.username;
                state.client.server_state.password = cfg.password;
                state.client.server_state.status = None;
            }
            if state.client.page == Page::Library && state.client.artists.filter_active {
                state.client.artists.filter_active = false;
            }
            if state.client.page == Page::Queue && state.client.queue_state.naming_playlist {
                state.client.queue_state.naming_playlist = false;
                state.client.queue_state.playlist_name.clear();
            }
            if state.client.page == Page::Playlists {
                state.client.playlists.renaming = false;
                state.client.playlists.rename_buf.clear();
                state.client.playlists.confirming_delete = false;
            }
        } else {
            let is_server_text_field =
                state.client.page == Page::Server && state.client.server_state.selected_field <= 2;
            let is_filtering =
                state.client.page == Page::Library && state.client.artists.filter_active;
            let is_naming_playlist =
                state.client.page == Page::Queue && state.client.queue_state.naming_playlist;
            let is_editing_playlist = state.client.page == Page::Playlists
                && (state.client.playlists.renaming || state.client.playlists.confirming_delete);

            if is_server_text_field || is_filtering || is_naming_playlist || is_editing_playlist {
                let page = state.client.page;
                let _ = state;
                drop(cs);
                drop(ds);
                return match page {
                    Page::Server => self.handle_server_key(key).await,
                    Page::Library => self.handle_library_key(key).await,
                    Page::Queue => self.handle_queue_key(key).await,
                    Page::Playlists => self.handle_playlists_key(key).await,
                    Page::Settings => self.handle_settings_key(key).await,
                    Page::QuickPlay => Ok(()),
                };
            }
        }

        match action {
            Some(GlobalAction::Quit) => {
                // Results showing but box not capturing: q backs out to the
                // tree. An active box routes q to the filter above (types q).
                if state.client.page == Page::Library && !state.client.artists.filter.is_empty() {
                    state.client.artists.exit_search();
                    return Ok(());
                }
                // A separate daemon outlives the TUI, so ask whether to stop it.
                if state.client.daemon_backed {
                    state.client.quit_prompt = true;
                } else {
                    state.client.should_quit = true;
                }
                return Ok(());
            }
            Some(GlobalAction::GoToLibrary) => {
                state.client.page = Page::Library;
                return Ok(());
            }
            Some(GlobalAction::GoToQueue) => {
                state.client.page = Page::Queue;
                return Ok(());
            }
            Some(GlobalAction::GoToQuickPlay) => {
                state.client.page = Page::QuickPlay;
                return Ok(());
            }
            Some(GlobalAction::GoToPlaylists) => {
                state.client.page = Page::Playlists;
                return Ok(());
            }
            Some(GlobalAction::GoToServer) => {
                state.client.page = Page::Server;
                return Ok(());
            }
            Some(GlobalAction::GoToSettings) => {
                state.client.page = Page::Settings;
                return Ok(());
            }
            Some(GlobalAction::TogglePause) => {
                let _ = state;
                drop(cs);
                drop(ds);
                return self
                    .client
                    .request(DaemonRequest::TogglePause)
                    .await
                    .map(|_| ())
                    .map_err(Error::from);
            }
            Some(GlobalAction::NextTrack) => {
                let _ = state;
                drop(cs);
                drop(ds);
                return self
                    .client
                    .request(DaemonRequest::Next)
                    .await
                    .map(|_| ())
                    .map_err(Error::from);
            }
            Some(GlobalAction::PreviousTrack) => {
                let _ = state;
                drop(cs);
                drop(ds);
                return self
                    .client
                    .request(DaemonRequest::Previous)
                    .await
                    .map(|_| ())
                    .map_err(Error::from);
            }
            Some(GlobalAction::StarPlaying) => {
                let song_id = state.daemon.now_playing.song.as_ref().map(|s| s.id.clone());
                let _ = state;
                drop(cs);
                drop(ds);
                if let Some(id) = song_id {
                    let _ = self.client.request(DaemonRequest::ToggleStarSong(id)).await;
                }
                return Ok(());
            }
            Some(GlobalAction::ToggleLyrics) => {
                let _ = state;
                drop(cs);
                drop(ds);
                self.toggle_lyrics().await;
                return Ok(());
            }
            Some(GlobalAction::ShuffleLibrary) => {
                state.client.notify("Shuffling library...");
                let _ = state;
                drop(cs);
                drop(ds);
                let _ = self.client.request(DaemonRequest::ShuffleLibrary).await;
                return Ok(());
            }
            Some(GlobalAction::CycleRepeat) => {
                let new_mode = state.client.settings_state.repeat_mode.cycle();
                state.client.settings_state.repeat_mode = new_mode;
                state.client.notify(format!("Repeat: {}", new_mode.label()));
                let _ = state;
                drop(cs);
                drop(ds);
                let _ = self
                    .client
                    .request(DaemonRequest::SetRepeatMode(new_mode))
                    .await;
                return Ok(());
            }
            Some(GlobalAction::Refresh) => {
                state.client.notify("Refreshing...");
                let _ = state;
                drop(cs);
                drop(ds);
                self.load_initial_data().await;
                let ds = self.daemon_state.read().await;
                let mut cs = self.client_state.write().await;
                let state = AppState {
                    daemon: &ds,
                    client: &mut cs,
                };
                state.client.notify("Data refreshed");
                return Ok(());
            }
            None => {}
        }

        // Song rating: five keys feeding one conceptual action doesn't fit
        // the one-action-one-chord keymap model, so this stays hardcoded
        // and out of the configurable set (see the `keybind` module docs).
        // Plain digits rate the playing song, Alt+digit the highlighted row,
        // the same split as `n` (star playing) and `m` (star highlighted).
        if let KeyCode::Char(c @ '1'..='5') = key.code {
            let target = match key.modifiers {
                KeyModifiers::NONE => state
                    .daemon
                    .now_playing
                    .song
                    .as_ref()
                    .map(|s| (s.id.clone(), s.user_rating)),
                KeyModifiers::ALT => state.highlighted_song(),
                _ => None,
            };
            // Any other modifier combination is not a rating chord; fall
            // through so it can still reach the page handlers below.
            if key.modifiers == KeyModifiers::NONE || key.modifiers == KeyModifiers::ALT {
                let highlighted = key.modifiers == KeyModifiers::ALT;
                let page = state.client.page;
                let song_pane_focused = state.client.artists.focus == 1;
                let _ = state;
                drop(cs);
                drop(ds);
                let Some((id, current)) = target else {
                    // Say why nothing happened rather than no-opping in
                    // silence, which is indistinguishable from a key that
                    // never arrived. The Library case earns its own wording:
                    // a song row is only highlighted while the song pane has
                    // focus, so the fix there is to press Right first.
                    debug!(
                        "Rating key '{c}' ignored: no {} song (page {page:?})",
                        if highlighted {
                            "highlighted"
                        } else {
                            "playing"
                        }
                    );
                    let message = if !highlighted {
                        "Nothing is playing to rate"
                    } else if page == Page::Library && !song_pane_focused {
                        "No song highlighted - press Right to focus the song list first"
                    } else {
                        "No song highlighted to rate"
                    };
                    self.client_state.write().await.notify(message);
                    return Ok(());
                };
                // `c` is one ASCII digit '1'-'5' per the match pattern.
                let pressed = c as u8 - b'0';
                // Pressing the already-set rating again clears it.
                let rating = if current == Some(pressed) { 0 } else { pressed };
                debug!(
                    "Rating key '{c}': setting rating {rating} on {} song {id}",
                    if highlighted {
                        "highlighted"
                    } else {
                        "playing"
                    }
                );
                // A rejected rating must not fail silently: without this the
                // user sees an unchanged row and cannot tell a refused write
                // from a key that never registered.
                if let Err(e) = self
                    .client
                    .request(DaemonRequest::SetSongRating { id, rating })
                    .await
                {
                    error!("Failed to set rating: {e}");
                    let mut cs = self.client_state.write().await;
                    cs.notify_error(format!("Failed to set rating: {e}"));
                }
                return Ok(());
            }
        }

        let page = state.client.page;
        let _ = state;
        drop(cs);
        drop(ds);
        match page {
            Page::QuickPlay => self.handle_songs_key(key).await,
            Page::Library => self.handle_library_key(key).await,
            Page::Queue => self.handle_queue_key(key).await,
            Page::Playlists => self.handle_playlists_key(key).await,
            Page::Server => self.handle_server_key(key).await,
            Page::Settings => self.handle_settings_key(key).await,
        }
    }
}
