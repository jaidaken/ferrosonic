//! Settings modal editors for name-list filters and global keybindings.

use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::Line,
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
    Frame,
};

use crate::app::state::{AppState, FilterListKind};
use crate::config::keybind::{effective_chord, DEFAULT_BINDINGS};
use crate::ui::theme::ThemeColors;

/// Draw the active Settings editor, if any, over the full terminal area.
pub fn render(frame: &mut Frame<'_>, area: Rect, state: &AppState<'_>, colors: &ThemeColors) {
    if let Some(editor) = &state.client.settings_state.filter_editor {
        render_filter(frame, area, editor, colors);
    } else if let Some(editor) = &state.client.settings_state.keybinding_editor {
        render_keybindings(frame, area, editor, colors);
    }
}

fn modal_rect(area: Rect, wanted_width: u16, wanted_height: u16) -> Option<Rect> {
    let width = wanted_width.min(area.width);
    let height = wanted_height.min(area.height);
    if width < 12 || height < 5 {
        return None;
    }
    Some(Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    ))
}

fn render_filter(
    frame: &mut Frame<'_>,
    area: Rect,
    editor: &crate::app::state::FilterListEditor,
    colors: &ThemeColors,
) {
    let Some(rect) = modal_rect(area, 68, 18) else {
        return;
    };
    let noun = match editor.kind {
        FilterListKind::Genres => "genres",
        FilterListKind::Artists => "artists",
    };
    let title = format!(" Excluded {noun} ");
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::default().fg(colors.accent));
    let inner = block.inner(rect);
    frame.render_widget(Clear, rect);
    frame.render_widget(block, rect);

    let list_height = inner.height.saturating_sub(2);
    let list_area = Rect::new(inner.x, inner.y, inner.width, list_height);
    let items: Vec<ListItem<'_>> = if editor.entries.is_empty() {
        vec![ListItem::new("No exclusions")]
    } else {
        editor
            .entries
            .iter()
            .map(|entry| ListItem::new(Line::from(entry.as_str())))
            .collect()
    };
    let mut list_state = ListState::default();
    if !editor.entries.is_empty() {
        list_state.select(Some(editor.selected.min(editor.entries.len() - 1)));
    }
    let list = List::new(items).highlight_symbol("▸ ").highlight_style(
        Style::default()
            .bg(colors.highlight_bg)
            .add_modifier(Modifier::BOLD),
    );
    frame.render_stateful_widget(list, list_area, &mut list_state);

    let input_y = inner.y + list_height;
    let input = if editor.adding {
        format!("Add: {}▏", editor.input)
    } else {
        "a:Add  d:Remove  Ctrl+S:Save  Esc:Cancel".to_string()
    };
    frame.render_widget(
        Paragraph::new(input).style(Style::default().fg(colors.muted)),
        Rect::new(inner.x, input_y, inner.width, 1),
    );
}

fn render_keybindings(
    frame: &mut Frame<'_>,
    area: Rect,
    editor: &crate::app::state::KeybindingEditor,
    colors: &ThemeColors,
) {
    let Some(rect) = modal_rect(area, 68, 20) else {
        return;
    };
    let title = if editor.capturing {
        " Keybindings — press a key (Esc cancels capture) "
    } else {
        " Global keybindings "
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::default().fg(colors.accent));
    let inner = block.inner(rect);
    frame.render_widget(Clear, rect);
    frame.render_widget(block, rect);

    let items: Vec<ListItem<'_>> = DEFAULT_BINDINGS
        .iter()
        .map(|(action, _)| {
            let chord = effective_chord(&editor.bindings, *action);
            ListItem::new(format!("{:<24} {chord}", action.label()))
        })
        .collect();
    let mut list_state = ListState::default();
    list_state.select(Some(editor.selected.min(DEFAULT_BINDINGS.len() - 1)));
    let list = List::new(items).highlight_symbol("▸ ").highlight_style(
        Style::default()
            .bg(colors.highlight_bg)
            .add_modifier(Modifier::BOLD),
    );
    frame.render_stateful_widget(
        list,
        Rect::new(
            inner.x,
            inner.y,
            inner.width,
            inner.height.saturating_sub(1),
        ),
        &mut list_state,
    );
    let help = if editor.capturing {
        "The next key becomes this action's shortcut"
    } else {
        "Enter:Rebind  d:Reset one  D:Reset all  Ctrl+S:Save  Esc:Cancel"
    };
    frame.render_widget(
        Paragraph::new(help).style(Style::default().fg(colors.muted)),
        Rect::new(
            inner.x,
            inner.y + inner.height.saturating_sub(1),
            inner.width,
            1,
        ),
    );
}
