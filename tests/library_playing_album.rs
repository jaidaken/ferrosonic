//! Regression: the Library right pane defaults to the playing album (the live
//! queue) with the now-playing track selected, when the tree highlight is not
//! on a specific album. Bug: it kept showing the last-hovered album.
//!
//! The reopen path (bootstrap_and_pump) and the artist-row nav path both call
//! AppState::show_playing_album; this drives the nav path end to end.

mod common;

use common::song;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ferrosonic::app::state::Page;
use ferrosonic::app::App;
use ferrosonic::config::Config;
use serial_test::serial;

fn key(code: KeyCode) -> KeyEvent {
    let mut k = KeyEvent::new(code, KeyModifiers::NONE);
    k.kind = KeyEventKind::Press;
    k
}

async fn build_app() -> App {
    let mut config = Config::new();
    config.daemon = false;
    let tempdir = common::tempdir();
    std::env::set_var("FERROSONIC_CONFIG_DIR", tempdir.path());
    std::mem::forget(tempdir);
    App::new(config)
}

#[tokio::test]
#[serial]
async fn navigating_to_a_non_album_row_shows_the_playing_album() {
    let mut app = build_app().await;
    {
        let mut ds = app.daemon_state.write().await;
        ds.queue = vec![song("q0", "Album Track 0"), song("q1", "Album Track 1")];
        ds.queue_position = Some(1);
    }
    {
        let mut cs = app.client_state.write().await;
        cs.page = Page::Library;
        cs.artists.focus = 0;
        // Empty tree, so index 0 resolves to a non-album row.
        cs.artists.selected_index = Some(0);
        // Stale right pane from a prior album hover.
        cs.artists.songs = vec![song("stale", "Stale Hovered Song")];
        cs.artists.selected_song = Some(0);
    }

    app.handle_key(key(KeyCode::Down)).await.unwrap();

    let cs = app.client_state.read().await;
    let ds = app.daemon_state.read().await;
    let got: Vec<&str> = cs.artists.songs.iter().map(|s| s.id.as_str()).collect();
    let want: Vec<&str> = ds.queue.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(
        got, want,
        "right pane must revert to the playing album (the queue), not the stale hover"
    );
    assert_eq!(
        cs.artists.selected_song, ds.queue_position,
        "the now-playing track must be the selected row"
    );
}
