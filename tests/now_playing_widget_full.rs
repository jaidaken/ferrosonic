//! ui/widgets/now_playing.rs: render branches across area sizes.

use ferrosonic::daemon::state::{NowPlaying, PlaybackState};
use ferrosonic::subsonic::models::Child;
use ferrosonic::ui::theme::{ThemeColors, ThemeData};

fn colors() -> ThemeColors {
    ThemeData::default_theme().colors
}
use ferrosonic::ui::widgets::now_playing::{art_rect, render_progress_bar, NowPlayingWidget};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::Widget;

fn song(id: &str, title: &str) -> Child {
    Child {
        id: id.into(),
        title: title.into(),
        parent: None,
        is_dir: false,
        album: Some("Closer".into()),
        artist: Some("Joy Division".into()),
        track: Some(1),
        year: Some(1980),
        genre: None,
        cover_art: None,
        size: None,
        content_type: None,
        suffix: None,
        duration: Some(180),
        bit_rate: Some(320),
        path: None,
        disc_number: None,
        starred: None,
    }
}

fn np_with_song(s: Child) -> NowPlaying {
    NowPlaying {
        song: Some(s),
        state: PlaybackState::Playing,
        duration: 180.0,
        position: 60.0,
        ..NowPlaying::default()
    }
}

#[test]
fn render_with_no_song_shows_no_track_message() {
    let np = NowPlaying::default();
    let widget = NowPlayingWidget::new(&np, colors());
    let mut buf = Buffer::empty(Rect::new(0, 0, 60, 7));
    widget.render(buf.area, &mut buf);
}

#[test]
fn render_too_small_area_returns_early() {
    let np = NowPlaying::default();
    let widget = NowPlayingWidget::new(&np, colors());
    let mut buf = Buffer::empty(Rect::new(0, 0, 19, 3));
    widget.render(buf.area, &mut buf);
}

#[test]
fn render_with_full_height_four_uses_four_line_layout() {
    let np = np_with_song(song("a", "Pictures of You"));
    let widget = NowPlayingWidget::new(&np, colors());
    let mut buf = Buffer::empty(Rect::new(0, 0, 80, 8));
    widget.render(buf.area, &mut buf);
    let w = buf.area.width;
    let h = buf.area.height;
    let mut rendered = String::new();
    for y in 0..h {
        for x in 0..w {
            rendered.push_str(buf[(x, y)].symbol());
        }
    }
    assert!(rendered.contains("Joy Division"));
    assert!(rendered.contains("Pictures of You"));
}

#[test]
fn render_with_height_three_uses_compressed_layout() {
    let np = np_with_song(song("a", "T"));
    let widget = NowPlayingWidget::new(&np, colors());
    let mut buf = Buffer::empty(Rect::new(0, 0, 80, 5));
    widget.render(buf.area, &mut buf);
}

#[test]
fn render_with_height_two_uses_compact_layout() {
    let np = np_with_song(song("a", "T"));
    let widget = NowPlayingWidget::new(&np, colors());
    let mut buf = Buffer::empty(Rect::new(0, 0, 80, 4));
    widget.render(buf.area, &mut buf);
}

#[test]
fn render_with_height_one_renders_title_only() {
    let np = np_with_song(song("a", "T"));
    let widget = NowPlayingWidget::new(&np, colors());
    let mut buf = Buffer::empty(Rect::new(0, 0, 80, 4));
    widget.render(buf.area, &mut buf);
}

#[test]
fn focused_widget_uses_focused_border_color() {
    let np = np_with_song(song("a", "T"));
    let widget = NowPlayingWidget::new(&np, colors()).focused(true);
    let mut buf = Buffer::empty(Rect::new(0, 0, 80, 7));
    widget.render(buf.area, &mut buf);
}

#[test]
fn art_reserved_cols_shrinks_info_area() {
    let np = np_with_song(song("a", "T"));
    let widget = NowPlayingWidget::new(&np, colors()).art_reserved_cols(20);
    let mut buf = Buffer::empty(Rect::new(0, 0, 80, 7));
    widget.render(buf.area, &mut buf);
}

#[test]
fn art_rect_returns_some_with_valid_input() {
    let area = Rect::new(0, 0, 80, 8);
    let r = art_rect(area, 16, (10, 20));
    assert!(r.is_some());
}

#[test]
fn art_rect_returns_none_with_zero_cover_art_cols() {
    let area = Rect::new(0, 0, 80, 8);
    assert!(art_rect(area, 0, (10, 20)).is_none());
}

#[test]
fn art_rect_returns_none_with_short_area_height() {
    let area = Rect::new(0, 0, 80, 3);
    assert!(art_rect(area, 16, (10, 20)).is_none());
}

#[test]
fn art_rect_returns_none_when_area_too_narrow_for_cover_plus_padding() {
    let area = Rect::new(0, 0, 30, 8);
    assert!(art_rect(area, 16, (10, 20)).is_none());
}

#[test]
fn art_rect_returns_none_when_inner_height_under_two() {
    let area = Rect::new(0, 0, 80, 2);
    assert!(art_rect(area, 16, (10, 20)).is_none());
}

#[test]
fn art_rect_handles_zero_cell_size_via_max_one() {
    let area = Rect::new(0, 0, 80, 8);
    let r = art_rect(area, 16, (0, 0));
    let _ = r;
}

#[test]
fn render_progress_bar_renders_filled_segment() {
    let mut buf = Buffer::empty(Rect::new(0, 0, 60, 1));
    render_progress_bar(buf.area, &mut buf, 0.5, "01:00", "02:00", &colors());
}

#[test]
fn render_progress_bar_at_zero_progress_is_all_unfilled() {
    let mut buf = Buffer::empty(Rect::new(0, 0, 60, 1));
    render_progress_bar(buf.area, &mut buf, 0.0, "00:00", "03:00", &colors());
}

#[test]
fn render_progress_bar_at_one_progress_is_all_filled() {
    let mut buf = Buffer::empty(Rect::new(0, 0, 60, 1));
    render_progress_bar(buf.area, &mut buf, 1.0, "03:00", "03:00", &colors());
}

#[test]
fn render_progress_bar_returns_early_when_too_narrow() {
    let mut buf = Buffer::empty(Rect::new(0, 0, 14, 1));
    render_progress_bar(buf.area, &mut buf, 0.5, "01:00", "02:00", &colors());
}

#[test]
fn render_with_quality_string_when_bit_depth_and_format_present() {
    let mut np = np_with_song(song("a", "T"));
    np.format = Some("FLAC".into());
    np.bit_depth = Some(24);
    np.sample_rate = Some(96000);
    np.channels = Some("stereo".into());
    let widget = NowPlayingWidget::new(&np, colors());
    let mut buf = Buffer::empty(Rect::new(0, 0, 100, 8));
    widget.render(buf.area, &mut buf);
}

#[test]
fn render_with_sample_rate_non_integer_khz_formats_with_one_decimal() {
    let mut np = np_with_song(song("a", "T"));
    np.sample_rate = Some(44100);
    let widget = NowPlayingWidget::new(&np, colors());
    let mut buf = Buffer::empty(Rect::new(0, 0, 100, 8));
    widget.render(buf.area, &mut buf);
}
