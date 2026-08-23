//! Per-track stream stats for library songs: codec, bitrate and download
//! speed are polled from mpv on the tick, published through the lightweight
//! `StreamStatsChanged` event (never `NowPlayingChanged`, which fans out to
//! MPRIS/D-Bus), and rendered in the now-playing quality row.

mod common;

use common::{fixtures::song, TestDaemon};
use ferrosonic::daemon::state::{NowPlaying, PlaybackState};
use ferrosonic::ipc::protocol::DaemonEvent;
use ferrosonic::subsonic::models::Child;
use ferrosonic::ui::theme::ThemeData;
use ferrosonic::ui::widget_now_playing::{build_quality_string, NowPlayingWidget};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::Widget;
use serde_json::json;
use serial_test::serial;

async fn seed_playing(td: &TestDaemon, c: Child) {
    let mut s = td.state.write().await;
    s.now_playing.song = Some(c.clone());
    s.queue = vec![c];
    s.queue_position = Some(0);
    s.now_playing.state = PlaybackState::Playing;
    s.now_playing.duration = 180.0;
    s.now_playing.position = 30.0;
    s.now_playing.sample_rate = Some(44_100);
}

#[tokio::test]
#[serial]
async fn tick_reports_codec_bitrate_and_download_speed_for_a_song() {
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_ping().await;
    seed_playing(&td, song("s1", "Track")).await;
    td.fake_mpv.set_loaded_file("http://x/stream?id=s1").await;
    td.fake_mpv
        .set_property("audio-codec-name", json!("flac"))
        .await;
    td.fake_mpv
        .set_property("audio-bitrate", json!(1_014_000))
        .await;
    td.fake_mpv
        .set_property("cache-speed", json!(358_400))
        .await;
    let mut rx = td.core.subscribe();

    td.core.update_playback_info().await;

    let np = td.state.read().await.now_playing.clone();
    assert_eq!(np.codec.as_deref(), Some("flac"));
    assert_eq!(np.bitrate_kbps, Some(1014));
    assert_eq!(np.download_bps, Some(358_400));

    let mut saw_stats = false;
    while let Ok(ev) = rx.try_recv() {
        match ev {
            DaemonEvent::StreamStatsChanged { .. } => saw_stats = true,
            DaemonEvent::NowPlayingChanged(_) => {
                panic!("stats must not fan out through NowPlayingChanged (MPRIS spam)")
            }
            _ => {}
        }
    }
    assert!(saw_stats, "a StreamStatsChanged event is broadcast");
}

#[tokio::test]
#[serial]
async fn unchanged_stats_do_not_re_emit() {
    let td = TestDaemon::new().await;
    td.fake_subsonic.expect_ping().await;
    seed_playing(&td, song("s1", "Track")).await;
    td.fake_mpv.set_loaded_file("http://x/stream?id=s1").await;
    td.fake_mpv
        .set_property("audio-codec-name", json!("flac"))
        .await;
    td.fake_mpv
        .set_property("audio-bitrate", json!(1_014_000))
        .await;
    td.fake_mpv.set_property("cache-speed", json!(0)).await;
    td.core.update_playback_info().await;
    let mut rx = td.core.subscribe();

    td.core.update_playback_info().await;

    let mut n = 0;
    while let Ok(ev) = rx.try_recv() {
        if matches!(ev, DaemonEvent::StreamStatsChanged { .. }) {
            n += 1;
        }
    }
    assert_eq!(n, 0, "identical stats on the next tick stay silent");
}

#[test]
fn quality_row_shows_codec_bitrate_and_traffic_for_a_song() {
    let np = NowPlaying {
        song: Some(song("s1", "Track")),
        state: PlaybackState::Playing,
        format: Some("s16".into()),
        bit_depth: Some(16),
        sample_rate: Some(44_100),
        channels: Some("Stereo".into()),
        codec: Some("flac".into()),
        bitrate_kbps: Some(1014),
        download_bps: Some(358_400),
        ..NowPlaying::default()
    };
    assert_eq!(
        build_quality_string(&np),
        "FLAC │ 16-bit │ 44.1kHz │ Stereo │ 1014 kbps │ ↓ 350.0 KB/s"
    );
}

#[test]
fn quality_row_hides_decoded_float_depth_and_falls_back_to_file_suffix_and_server_bitrate() {
    // mpv decodes lossy codecs to floatp: "32-bit" there says nothing about
    // the source, so it is dropped. Before mpv reports, the song's own
    // suffix and the server's nominal bitRate stand in.
    let mut c = song("s1", "Track");
    c.suffix = Some("mp3".into());
    c.bit_rate = Some(320);
    let np = NowPlaying {
        song: Some(c),
        state: PlaybackState::Playing,
        format: Some("floatp".into()),
        bit_depth: Some(32),
        sample_rate: Some(48_000),
        channels: Some("Stereo".into()),
        ..NowPlaying::default()
    };
    assert_eq!(build_quality_string(&np), "MP3 │ 48kHz │ Stereo │ 320 kbps");
}

#[test]
fn quality_row_for_radio_leaves_bitrate_and_traffic_to_the_live_row() {
    let st = ferrosonic::subsonic::models::InternetRadioStation {
        id: "1".into(),
        name: "Jazz".into(),
        stream_url: "http://r/x".into(),
        home_page_url: None,
    };
    let np = NowPlaying {
        song: Some(Child::from_radio_station(&st)),
        state: PlaybackState::Playing,
        format: Some("floatp".into()),
        bit_depth: Some(32),
        sample_rate: Some(44_100),
        channels: Some("Stereo".into()),
        codec: Some("mp3".into()),
        bitrate_kbps: Some(128),
        download_bps: Some(20_480),
        position: 9.0,
        ..NowPlaying::default()
    };
    assert_eq!(build_quality_string(&np), "MP3 │ 44.1kHz │ Stereo");
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
    assert_eq!(
        s.matches("128 kbps").count(),
        1,
        "kbps shown once (LIVE row): {s}"
    );
}
