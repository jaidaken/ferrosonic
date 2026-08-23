//! Internet radio stations (Navidrome `getInternetRadioStations`): the daemon
//! fetches them into the library cache as synthetic `Child` entries, and
//! playing one hands the raw stream URL to mpv instead of `rest/stream`.

mod common;

use common::TestDaemon;
use ferrosonic::daemon::core::PlayMode;
use ferrosonic::subsonic::models::{Child, InternetRadioStation};
use serial_test::serial;

fn station(id: &str, name: &str, url: &str) -> InternetRadioStation {
    InternetRadioStation {
        id: id.into(),
        name: name.into(),
        stream_url: url.into(),
        home_page_url: Some("https://example.org".into()),
    }
}

#[test]
fn radio_child_carries_stream_url_and_prefixed_id() {
    let c = Child::from_radio_station(&station("7", "Jazz FM", "http://r.example/jazz"));
    assert!(c.is_radio(), "a station child must identify as radio");
    assert_eq!(
        c.id, "radio:7",
        "id is namespaced so it can never collide with a song id"
    );
    assert_eq!(c.title, "Jazz FM");
    assert_eq!(c.radio_stream_url.as_deref(), Some("http://r.example/jazz"));
    assert!(c.cover_art.is_none(), "stations have no cover art id");
    assert!(c.duration.is_none(), "live streams have no duration");
    assert!(!common::fixtures::song("s1", "x").is_radio());
}

#[tokio::test]
#[serial]
async fn refresh_radio_stations_populates_library() {
    let td = TestDaemon::new().await;
    td.fake_subsonic
        .expect_internet_radio_stations(&[
            ("1", "Jazz FM", "http://r.example/jazz"),
            ("2", "Rock", "http://r.example/rock"),
        ])
        .await;

    td.core.refresh_radio_stations().await;

    let st = td.state.read().await;
    assert_eq!(st.library.radio_stations.len(), 2);
    assert_eq!(st.library.radio_stations[0].title, "Jazz FM");
    assert_eq!(st.library.radio_stations[1].id, "radio:2");
    assert!(st.library.radio_stations.iter().all(Child::is_radio));
}

#[tokio::test]
#[serial]
async fn buffered_mode_is_bypassed_for_a_live_station() {
    // Quick Play `Enter` uses PlayMode::Buffered, which downloads the whole
    // file to disk before loadfile; a live stream never ends, so a station
    // must be handed to mpv directly instead.
    let td = TestDaemon::new().await;
    let url = td
        .fake_subsonic
        .expect_slow_stream("/live/jazz", 10_000)
        .await;
    let c = Child::from_radio_station(&station("1", "Jazz FM", &url));

    td.core
        .replace_queue_and_play(vec![c], Some(0), PlayMode::Buffered)
        .await
        .unwrap();

    let loaded = td
        .fake_mpv
        .wait_for(2000, |cmds| {
            cmds.iter()
                .any(|c| c.first().and_then(serde_json::Value::as_str) == Some("loadfile"))
        })
        .await;
    assert!(
        loaded,
        "mpv must receive a loadfile promptly, not after a download"
    );
    assert_eq!(
        td.fake_mpv.loaded_file().await.as_deref(),
        Some(url.as_str()),
        "the station URL goes to mpv as-is, never a local prebuf temp path"
    );
}

#[tokio::test]
#[serial]
async fn playing_a_station_loads_its_stream_url_directly() {
    let td = TestDaemon::new().await;
    let c = Child::from_radio_station(&station("1", "Jazz FM", "http://r.example/jazz"));

    td.core
        .replace_queue_and_play(vec![c], Some(0), PlayMode::Direct)
        .await
        .unwrap();

    let loaded = td
        .fake_mpv
        .wait_for(2000, |cmds| {
            cmds.iter()
                .any(|c| c.first().and_then(serde_json::Value::as_str) == Some("loadfile"))
        })
        .await;
    assert!(loaded, "mpv must receive a loadfile for the station");
    let url = td.fake_mpv.loaded_file().await.unwrap();
    assert_eq!(
        url, "http://r.example/jazz",
        "a radio entry plays its raw stream URL, not rest/stream"
    );
    let st = td.state.read().await;
    assert_eq!(
        st.now_playing.song.as_ref().map(|s| s.id.as_str()),
        Some("radio:1")
    );
    assert_eq!(
        st.now_playing.duration, 0.0,
        "live stream: no known duration"
    );
}

// ---- live-stream behaviour on the playback tick ----

/// Seed the daemon as if station `c` were playing at `pos` seconds.
async fn seed_playing_radio(td: &TestDaemon, queue: Vec<Child>, pos: f64) {
    let mut s = td.state.write().await;
    s.now_playing.song = Some(queue[0].clone());
    s.queue = queue;
    s.queue_position = Some(0);
    s.now_playing.state = ferrosonic::daemon::state::PlaybackState::Playing;
    s.now_playing.duration = 0.0;
    s.now_playing.position = pos;
}

#[tokio::test]
#[serial]
async fn tick_never_backfills_a_duration_for_a_live_station() {
    // mpv reports `duration` for a live stream as the buffered window (a few
    // seconds); adopting it would draw a progress bar that "ends" and would
    // arm the near-end AdvanceEarly path. Stay at 0 = unknown/live.
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_ping().await;
    let c = Child::from_radio_station(&station("1", "Jazz FM", "http://r.example/jazz"));
    seed_playing_radio(&td, vec![c], 30.0).await;
    td.fake_mpv.set_loaded_file("http://r.example/jazz").await;
    td.fake_mpv.set_duration(10.0).await;

    td.core.update_playback_info().await;

    assert_eq!(
        td.state.read().await.now_playing.duration,
        0.0,
        "live station keeps duration 0 even when mpv reports a cache window"
    );
}

#[tokio::test]
#[serial]
async fn tick_does_not_preload_the_next_entry_while_a_station_plays() {
    // A live stream never EOFs, so prefetching the next queue entry into mpv's
    // playlist only opens an idle second connection (for another station, an
    // endless second download).
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_ping().await;
    let a = Child::from_radio_station(&station("1", "Jazz FM", "http://r.example/jazz"));
    let b = common::fixtures::song("s2", "Next Song");
    seed_playing_radio(&td, vec![a, b], 30.0).await;
    td.fake_mpv.set_loaded_file("http://r.example/jazz").await;
    td.fake_mpv
        .set_playlist(vec!["http://r.example/jazz".into()])
        .await;

    td.core.update_playback_info().await;

    let appended = td.fake_mpv.commands().await.iter().any(|c| {
        c.first().and_then(serde_json::Value::as_str) == Some("loadfile")
            && c.get(2).and_then(serde_json::Value::as_str) == Some("append")
    });
    assert!(
        !appended,
        "no loadfile append while a live station is playing"
    );
}

#[tokio::test]
#[serial]
async fn tick_reports_stream_bitrate_and_download_speed_for_a_station() {
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_ping().await;
    let c = Child::from_radio_station(&station("1", "Jazz FM", "http://r.example/jazz"));
    seed_playing_radio(&td, vec![c], 30.0).await;
    td.fake_mpv.set_loaded_file("http://r.example/jazz").await;
    td.fake_mpv
        .set_property("audio-bitrate", serde_json::json!(128_000))
        .await;
    td.fake_mpv
        .set_property("cache-speed", serde_json::json!(20_480))
        .await;

    td.core.update_playback_info().await;

    let np = td.state.read().await.now_playing.clone();
    assert_eq!(
        np.stream_bitrate_kbps,
        Some(128),
        "audio-bitrate b/s -> kbps"
    );
    assert_eq!(
        np.stream_speed_bps,
        Some(20_480),
        "cache-speed bytes/s as-is"
    );
}

#[test]
fn now_playing_widget_renders_a_live_row_instead_of_a_progress_bar() {
    use ferrosonic::daemon::state::{NowPlaying, PlaybackState};
    use ferrosonic::ui::theme::ThemeData;
    use ferrosonic::ui::widget_now_playing::NowPlayingWidget;
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    use ratatui::widgets::Widget;

    let np = NowPlaying {
        song: Some(Child::from_radio_station(&station(
            "1",
            "Jazz FM",
            "http://r.example/jazz",
        ))),
        state: PlaybackState::Playing,
        position: 65.0,
        duration: 0.0,
        stream_bitrate_kbps: Some(128),
        stream_speed_bps: Some(20_480),
        ..NowPlaying::default()
    };
    let widget = NowPlayingWidget::new(&np, ThemeData::default_theme().colors);
    let mut buf = Buffer::empty(Rect::new(0, 0, 80, 7));
    widget.render(buf.area, &mut buf);
    let mut s = String::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            s.push_str(buf[(x, y)].symbol());
        }
        s.push('\n');
    }
    assert!(s.contains("LIVE"), "live marker shown: {s}");
    assert!(s.contains("01:05"), "elapsed listening time shown: {s}");
    assert!(s.contains("128 kbps"), "stream bitrate shown: {s}");
    assert!(s.contains("20.0 KB/s"), "download speed shown: {s}");
    assert!(
        !s.contains(" / 00:00"),
        "no `pos / dur` pair for a live stream: {s}"
    );
}
