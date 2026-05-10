//! Server page with connection settings form

use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::app::state::AppState;
use crate::ui::theme::ThemeColors;

/// Render the server page
pub fn render(frame: &mut Frame, area: Rect, state: &AppState<'_>) {
    let colors = *state.client.settings_state.theme_colors();

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Server Connection ")
        .border_style(Style::default().fg(colors.border_focused));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 10 {
        return;
    }

    let server = &state.client.server_state;

    // Layout fields vertically with spacing
    let chunks = Layout::vertical([
        Constraint::Length(1), // Spacing
        Constraint::Length(4), // Server URL (1 label + 3 field)
        Constraint::Length(4), // Username (1 label + 3 field)
        Constraint::Length(4), // Password (1 label + 3 field)
        Constraint::Length(1), // Spacing
        Constraint::Length(1), // Test button
        Constraint::Length(1), // Save button
        Constraint::Length(1), // Spacing
        Constraint::Min(1),    // Status
    ])
    .split(inner);

    // Server URL field - show cursor when selected (always editable)
    render_field(
        frame,
        chunks[1],
        "Server URL",
        &server.base_url,
        server.selected_field == 0,
        server.selected_field == 0, // cursor when selected
        &colors,
    );

    // Username field
    render_field(
        frame,
        chunks[2],
        "Username",
        &server.username,
        server.selected_field == 1,
        server.selected_field == 1,
        &colors,
    );

    // Password field
    render_field(
        frame,
        chunks[3],
        "Password",
        &"*".repeat(server.password.len()),
        server.selected_field == 2,
        server.selected_field == 2,
        &colors,
    );

    // Test button
    render_button(
        frame,
        chunks[5],
        "Test Connection",
        server.selected_field == 3,
        &colors,
    );

    // Save button
    render_button(
        frame,
        chunks[6],
        "Save",
        server.selected_field == 4,
        &colors,
    );

    // Status message
    if let Some(ref status) = server.status {
        let style: Style = if status.contains("failed") || status.contains("error") {
            Style::default().fg(colors.error)
        } else if status.contains("saved") || status.contains("success") {
            Style::default().fg(colors.success)
        } else {
            Style::default().fg(colors.accent)
        };

        let status_text = Paragraph::new(status.as_str()).style(style);
        frame.render_widget(status_text, chunks[8]);
    }
}

/// Render a form field
fn render_field(
    frame: &mut Frame,
    area: Rect,
    label: &str,
    value: &str,
    selected: bool,
    editing: bool,
    colors: &ThemeColors,
) {
    let label_style = if selected {
        Style::default()
            .fg(colors.primary)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(colors.highlight_fg)
    };

    let value_style = if editing {
        Style::default().fg(colors.accent)
    } else if selected {
        Style::default().fg(colors.primary)
    } else {
        Style::default().fg(colors.muted)
    };

    let border_style = if selected {
        Style::default().fg(colors.border_focused)
    } else {
        Style::default().fg(colors.border_unfocused)
    };

    // Label on first line
    let label_text = Paragraph::new(label).style(label_style);
    frame.render_widget(label_text, Rect::new(area.x, area.y, area.width, 1));

    // Value field with border on second line (height 3 = 1 top border + 1 content + 1 bottom border)
    let field_area = Rect::new(area.x, area.y + 1, area.width.min(60), 3);

    let display_value = if editing {
        format!("{}▏", value)
    } else {
        value.to_string()
    };

    let field = Paragraph::new(display_value).style(value_style).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(border_style),
    );

    frame.render_widget(field, field_area);
}

/// Render a button
fn render_button(frame: &mut Frame, area: Rect, label: &str, selected: bool, colors: &ThemeColors) {
    let style = if selected {
        Style::default()
            .fg(colors.highlight_fg)
            .bg(colors.primary)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(colors.muted)
    };

    let text = format!("[ {} ]", label);
    let button = Paragraph::new(text).style(style);
    frame.render_widget(button, area);
}
