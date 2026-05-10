use crossterm::event::{self, MouseButton, MouseEventKind};

use crate::error::Error;

use super::*;

impl App {
    /// Handle mouse input
    pub(super) async fn handle_mouse(&mut self, mouse: event::MouseEvent) -> Result<(), Error> {
        let x = mouse.column;
        let y = mouse.row;

        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => self.handle_mouse_click(x, y).await,
            MouseEventKind::ScrollUp => self.handle_mouse_scroll_up().await,
            MouseEventKind::ScrollDown => self.handle_mouse_scroll_down().await,
            _ => Ok(()),
        }
    }

    /// Handle left mouse click
    async fn handle_mouse_click(&mut self, x: u16, y: u16) -> Result<(), Error> {
        use crate::ui::header::{Header, HeaderRegion};

        let state = self.state.read().await;
        let layout = state.client.layout.clone();
        let page = state.client.page;
        let duration = state.daemon.now_playing.duration;
        drop(state);

        // Check header area
        if y >= layout.header.y && y < layout.header.y + layout.header.height {
            if let Some(region) = Header::region_at(layout.header, x, y) {
                match region {
                    HeaderRegion::Tab(tab_page) => {
                        let mut state = self.state.write().await;
                        state.client.page = tab_page;
                    }
                    HeaderRegion::PrevButton => {
                        return self.prev_track().await;
                    }
                    HeaderRegion::PlayButton => {
                        return self.toggle_pause().await;
                    }
                    HeaderRegion::PauseButton => {
                        return self.toggle_pause().await;
                    }
                    HeaderRegion::StopButton => {
                        return self.stop_playback().await;
                    }
                    HeaderRegion::NextButton => {
                        return self.next_track().await;
                    }
                }
            }
            return Ok(());
        }

        // Check now playing area (progress bar seeking)
        if y >= layout.now_playing.y && y < layout.now_playing.y + layout.now_playing.height {
            let inner_bottom = layout.now_playing.y + layout.now_playing.height - 2;
            if y == inner_bottom && duration > 0.0 {
                let inner_x_start = layout.now_playing.x + 1;
                let inner_width = layout.now_playing.width.saturating_sub(2);
                if inner_width > 15 && x >= inner_x_start {
                    let rel_x = x - inner_x_start;
                    let time_width = 15u16;
                    let bar_width = inner_width.saturating_sub(time_width + 2);
                    let bar_start = (inner_width.saturating_sub(time_width + 2 + bar_width)) / 2
                        + time_width
                        + 2;
                    if bar_width > 0 && rel_x >= bar_start && rel_x < bar_start + bar_width {
                        let fraction = (rel_x - bar_start) as f64 / bar_width as f64;
                        let seek_pos = fraction * duration;
                        let _ = self.mpv.seek(seek_pos);
                        let mut state = self.state.write().await;
                        state.daemon.now_playing.position = seek_pos;
                    }
                }
            }
            return Ok(());
        }

        // Check content area
        if y >= layout.content.y && y < layout.content.y + layout.content.height {
            return self.handle_content_click(x, y, page, &layout).await;
        }

        Ok(())
    }

    /// Handle click within the content area
    async fn handle_content_click(
        &mut self,
        x: u16,
        y: u16,
        page: Page,
        layout: &LayoutAreas,
    ) -> Result<(), Error> {
        match page {
            Page::Songs => {
                let mut state = self.state.write().await;
                state.client.notify("Mouse input not supported for this page yet");
                Ok(())
            }
            Page::Artists => self.handle_artists_click(x, y, layout).await,
            Page::Queue => self.handle_queue_click(y, layout).await,
            Page::Playlists => self.handle_playlists_click(x, y, layout).await,
            _ => Ok(()),
        }
    }

    /// Handle click on queue page
    async fn handle_queue_click(&mut self, y: u16, layout: &LayoutAreas) -> Result<(), Error> {
        let mut state = self.state.write().await;
        let content = layout.content;

        // Account for border (1 row top)
        let row_in_viewport = y.saturating_sub(content.y + 1) as usize;
        let item_index = state.client.queue_state.scroll_offset + row_in_viewport;

        if item_index < state.daemon.queue.len() {
            let was_selected = state.client.queue_state.selected == Some(item_index);
            state.client.queue_state.selected = Some(item_index);

            let is_second_click = was_selected
                && self
                    .last_click
                    .is_some_and(|(_, ly, t)| ly == y && t.elapsed().as_millis() < 500);

            if is_second_click {
                drop(state);
                self.last_click = Some((0, y, std::time::Instant::now()));
                return self.play_queue_position(item_index).await;
            }
        }

        self.last_click = Some((0, y, std::time::Instant::now()));
        Ok(())
    }

    /// Handle mouse scroll up (move selection up in current list)
    async fn handle_mouse_scroll_up(&mut self) -> Result<(), Error> {
        let mut state = self.state.write().await;
        match state.client.page {
            Page::Artists => {
                if state.client.artists.focus == 0 {
                    if let Some(sel) = state.client.artists.selected_index {
                        if sel > 0 {
                            state.client.artists.selected_index = Some(sel - 1);
                        }
                    }
                } else if let Some(sel) = state.client.artists.selected_song {
                    if sel > 0 {
                        state.client.artists.selected_song = Some(sel - 1);
                    }
                }
            }
            Page::Queue => {
                if let Some(sel) = state.client.queue_state.selected {
                    if sel > 0 {
                        state.client.queue_state.selected = Some(sel - 1);
                    }
                } else if !state.daemon.queue.is_empty() {
                    state.client.queue_state.selected = Some(0);
                }
            }
            Page::Playlists => {
                if state.client.playlists.focus == 0 {
                    if let Some(sel) = state.client.playlists.selected_playlist {
                        if sel > 0 {
                            state.client.playlists.selected_playlist = Some(sel - 1);
                        }
                    }
                } else if let Some(sel) = state.client.playlists.selected_song {
                    if sel > 0 {
                        state.client.playlists.selected_song = Some(sel - 1);
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Handle mouse scroll down (move selection down in current list)
    async fn handle_mouse_scroll_down(&mut self) -> Result<(), Error> {
        let mut state = self.state.write().await;
        match state.client.page {
            Page::Artists => {
                if state.client.artists.focus == 0 {
                    let tree_items = crate::ui::pages::artists::build_tree_items(&state);
                    let max = tree_items.len().saturating_sub(1);
                    if let Some(sel) = state.client.artists.selected_index {
                        if sel < max {
                            state.client.artists.selected_index = Some(sel + 1);
                        }
                    } else if !tree_items.is_empty() {
                        state.client.artists.selected_index = Some(0);
                    }
                } else {
                    let max = state.client.artists.songs.len().saturating_sub(1);
                    if let Some(sel) = state.client.artists.selected_song {
                        if sel < max {
                            state.client.artists.selected_song = Some(sel + 1);
                        }
                    } else if !state.client.artists.songs.is_empty() {
                        state.client.artists.selected_song = Some(0);
                    }
                }
            }
            Page::Queue => {
                let max = state.daemon.queue.len().saturating_sub(1);
                if let Some(sel) = state.client.queue_state.selected {
                    if sel < max {
                        state.client.queue_state.selected = Some(sel + 1);
                    }
                } else if !state.daemon.queue.is_empty() {
                    state.client.queue_state.selected = Some(0);
                }
            }
            Page::Playlists => {
                if state.client.playlists.focus == 0 {
                    let max = state.daemon.library.playlists.len().saturating_sub(1);
                    if let Some(sel) = state.client.playlists.selected_playlist {
                        if sel < max {
                            state.client.playlists.selected_playlist = Some(sel + 1);
                        }
                    } else if !state.daemon.library.playlists.is_empty() {
                        state.client.playlists.selected_playlist = Some(0);
                    }
                } else {
                    let max = state.client.playlists.songs.len().saturating_sub(1);
                    if let Some(sel) = state.client.playlists.selected_song {
                        if sel < max {
                            state.client.playlists.selected_song = Some(sel + 1);
                        }
                    } else if !state.client.playlists.songs.is_empty() {
                        state.client.playlists.selected_song = Some(0);
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
}
