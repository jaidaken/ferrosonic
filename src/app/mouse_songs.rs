use super::*;
use crate::app::models::SongOption;
use crate::error::Error;
use ::ratatui::prelude::{Constraint, Layout};
use strum::IntoEnumIterator;

impl App {
    pub(super) async fn handle_songs_click(
        &mut self,
        x: u16,
        y: u16,
        layout: &LayoutAreas,
    ) -> Result<(), Error> {
        let content = layout.content;
        let chunks = Layout::vertical([Constraint::Percentage(15), Constraint::Percentage(85)])
            .split(content);

        let options_area = chunks[0];
        let songs_area = chunks[1];

        if y >= options_area.y && y < options_area.y + options_area.height {
            // Click in the options pane
            let row_in_viewport = y.saturating_sub(options_area.y + 1) as usize; // +1 for border
            let mut state = self.state.write().await;
            state.songs.focus = 0;

            let option = SongOption::iter().nth(row_in_viewport);

            if let Some(opt) = option {
                if state.songs.selected_option.as_ref() != Some(&opt) {
                    state.songs.selected_option = Some(opt.clone());
                    state.songs.scroll_offset = 0;
                    drop(state);
                    match opt {
                        SongOption::Starred => self.get_starred_songs().await,
                        SongOption::Random => self.get_random_songs().await,
                    }
                }
            }
        } else if y >= songs_area.y && y < songs_area.y + songs_area.height {
            // Click in the song list
            let row_in_viewport = y.saturating_sub(songs_area.y + 1) as usize; // +1 for border
            let mut state = self.state.write().await;
            let item_index = state.songs.scroll_offset + row_in_viewport;

            if item_index < state.songs.songs.len() {
                let was_selected = state.songs.selected_index == Some(item_index);
                state.songs.focus = 1;
                state.songs.selected_index = Some(item_index);

                // Double-click to play
                let is_second_click = was_selected
                    && self.last_click.is_some_and(|(lx, ly, t)| {
                        lx == x && ly == y && t.elapsed().as_millis() < 500
                    });

                if is_second_click {
                    let songs = state.songs.songs.clone();
                    state.queue.clear();
                    state.queue.extend(songs);
                    drop(state);
                    self.last_click = Some((x, y, std::time::Instant::now()));
                    return self.play_queue_position(item_index).await;
                }
            }
        }

        self.last_click = Some((x, y, std::time::Instant::now()));
        Ok(())
    }
}
