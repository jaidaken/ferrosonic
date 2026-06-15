//! Regression for #23: the form pages (Settings, Server) must not go blank
//! when the visualizer or a short terminal starves the content area.

mod common;

use common::render;
use ferrosonic::app::client_state::ClientState;
use ferrosonic::app::state::{CavaRow, Page};
use ferrosonic::config::Config;
use ferrosonic::daemon::DaemonState;

fn state(page: Page) -> (DaemonState, ClientState) {
    let config = Config::new();
    let mut daemon = DaemonState::new(config);
    daemon.config.theme = "default".into();
    let client = ClientState {
        page,
        ..ClientState::default()
    };
    (daemon, client)
}

fn enable_visualizer(client: &mut ClientState) {
    client.cava_available = true;
    client.settings_state.cava_enabled = true;
    client.settings_state.cava_size = 40;
    client.cava_screen = vec![CavaRow::default()];
}

#[test]
fn settings_not_blank_with_visualizer_active() {
    let (daemon, mut client) = state(Page::Settings);
    enable_visualizer(&mut client);
    let frame = render(80, 30, &daemon, &mut client);
    assert!(
        frame.contains("Theme") && frame.contains("Daemon"),
        "settings page blanked with the visualizer on:\n{frame}"
    );
}

#[test]
fn server_not_blank_with_visualizer_active() {
    let (daemon, mut client) = state(Page::Server);
    enable_visualizer(&mut client);
    let frame = render(80, 30, &daemon, &mut client);
    assert!(
        frame.contains("Username") && frame.contains("Password"),
        "server page blanked with the visualizer on:\n{frame}"
    );
}

#[test]
fn settings_degrades_instead_of_blanking_in_a_short_area() {
    let (daemon, mut client) = state(Page::Settings);
    // No visualizer; just a short terminal that the old `< 18` guard blanked.
    let frame = render(80, 20, &daemon, &mut client);
    assert!(
        frame.contains("Theme"),
        "settings page blanked in a short area instead of degrading:\n{frame}"
    );
}

#[test]
fn settings_clips_rows_that_do_not_fit_instead_of_overflowing() {
    let (daemon, mut client) = state(Page::Settings);
    // Short area fits only the top rows; later rows must clip at the row
    // limit, not overflow past the panel into the widgets below.
    let frame = render(80, 14, &daemon, &mut client);
    assert!(
        frame.contains("Theme"),
        "top rows must still render:\n{frame}"
    );
    assert!(
        !frame.contains("Auto-continue"),
        "rows past the row limit must clip, not overflow:\n{frame}"
    );
}

#[test]
fn settings_scrolls_to_keep_the_selected_row_visible() {
    let (daemon, mut client) = state(Page::Settings);
    client.settings_state.selected_field = 7; // Daemon, the last row
    let frame = render(80, 14, &daemon, &mut client);
    assert!(
        frame.contains("Daemon"),
        "the selected bottom row must scroll into view on a short panel:\n{frame}"
    );
}
