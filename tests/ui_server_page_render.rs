//! ui/pages/server.rs: render with each status keyword + selected button.

mod common;

use common::render;
use ferrosonic::app::client_state::ClientState;
use ferrosonic::app::state::Page;
use ferrosonic::config::Config;
use ferrosonic::daemon::DaemonState;

fn build_state() -> (DaemonState, ClientState) {
    let config = Config::new();
    let mut daemon = DaemonState::new(config);
    daemon.config.theme = "default".into();
    let client = ClientState {
        page: Page::Server,
        ..ClientState::default()
    };
    (daemon, client)
}

#[test]
fn server_page_with_failed_status_uses_error_style() {
    let (daemon, mut client) = build_state();
    client.server_state.status = Some("Connection failed: timeout".into());
    let frame = render(80, 30, &daemon, &mut client);
    insta::assert_snapshot!(frame);
}

#[test]
fn server_page_with_error_status_uses_error_style() {
    let (daemon, mut client) = build_state();
    client.server_state.status = Some("IPC error: disconnected".into());
    let frame = render(80, 30, &daemon, &mut client);
    insta::assert_snapshot!(frame);
}

#[test]
fn server_page_with_saved_status_uses_success_style() {
    let (daemon, mut client) = build_state();
    client.server_state.status = Some("Settings saved".into());
    let frame = render(80, 30, &daemon, &mut client);
    insta::assert_snapshot!(frame);
}

#[test]
fn server_page_with_success_status_uses_success_style() {
    let (daemon, mut client) = build_state();
    client.server_state.status = Some("Connection successful!".into());
    let frame = render(80, 30, &daemon, &mut client);
    insta::assert_snapshot!(frame);
}

#[test]
fn server_page_with_neutral_status_uses_accent_style() {
    let (daemon, mut client) = build_state();
    client.server_state.status = Some("Testing connection...".into());
    let frame = render(80, 30, &daemon, &mut client);
    insta::assert_snapshot!(frame);
}

#[test]
fn server_page_with_no_status_renders_without_panic() {
    let (daemon, mut client) = build_state();
    client.server_state.status = None;
    let frame = render(80, 30, &daemon, &mut client);
    insta::assert_snapshot!(frame);
}

#[test]
fn server_page_with_selected_save_button_uses_highlight_style() {
    let (daemon, mut client) = build_state();
    client.server_state.selected_field = 4;
    let frame = render(80, 30, &daemon, &mut client);
    insta::assert_snapshot!(frame);
}

#[test]
fn server_page_with_selected_test_button_uses_highlight_style() {
    let (daemon, mut client) = build_state();
    client.server_state.selected_field = 3;
    let frame = render(80, 30, &daemon, &mut client);
    insta::assert_snapshot!(frame);
}

#[test]
fn server_page_with_text_field_selected_renders_editing_value() {
    let (daemon, mut client) = build_state();
    client.server_state.selected_field = 0;
    client.server_state.base_url = "https://test".into();
    let frame = render(80, 30, &daemon, &mut client);
    insta::assert_snapshot!(frame);
}

#[test]
fn server_page_with_populated_username_renders_value() {
    let (daemon, mut client) = build_state();
    client.server_state.username = "myuser".into();
    client.server_state.selected_field = 1;
    let frame = render(80, 30, &daemon, &mut client);
    insta::assert_snapshot!(frame);
}
