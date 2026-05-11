//! Cover-art rendering fallbacks: no picker, no image, chafa-only, halfblocks-only.

use ferrosonic::ui::cover_art::CoverArtState;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use ratatui::Terminal;
use ratatui_image::picker::{Picker, ProtocolType};
use std::sync::Mutex;

fn empty_state() -> CoverArtState {
    CoverArtState {
        picker: None,
        protocol_type: None,
        cell_size: (8, 16),
        current_id: None,
        image: None,
        protocol: None,
        chafa_cache: None,
    }
}

fn halfblocks_state() -> CoverArtState {
    let mut picker = Picker::from_fontsize((8, 16));
    picker.set_protocol_type(ProtocolType::Halfblocks);
    CoverArtState {
        picker: Some(picker),
        protocol_type: Some(ProtocolType::Halfblocks),
        cell_size: (8, 16),
        current_id: None,
        image: None,
        protocol: None,
        chafa_cache: None,
    }
}

fn render_state(state: CoverArtState, w: u16, h: u16) {
    let mutex = Mutex::new(state);
    let backend = TestBackend::new(40, 20);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            ferrosonic::ui::cover_art::render(frame, Rect::new(0, 0, w, h), &mutex);
        })
        .unwrap();
}

#[test]
fn render_with_no_picker_and_no_image_is_silent() {
    render_state(empty_state(), 10, 10);
}

#[test]
fn render_with_picker_but_no_image_does_not_panic() {
    render_state(halfblocks_state(), 10, 10);
}

#[test]
fn render_into_zero_size_rect_is_safe() {
    render_state(halfblocks_state(), 0, 0);
}

#[test]
fn render_into_tiny_rect_does_not_crash() {
    render_state(halfblocks_state(), 1, 1);
}

#[test]
fn clear_after_load_resets_image_and_protocol() {
    let mut state = halfblocks_state();
    use image::{ImageBuffer, Rgba};
    let img: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::new(4, 4);
    let mut buf = std::io::Cursor::new(Vec::new());
    img.write_to(&mut buf, image::ImageFormat::Png).unwrap();
    state.load("a".into(), buf.get_ref());
    state.clear();
    assert!(state.image.is_none());
    assert!(state.protocol.is_none());
    assert!(state.current_id.is_none());
}

#[test]
fn load_unknown_format_bytes_does_not_set_protocol() {
    let mut state = halfblocks_state();
    state.load("a".into(), &[0xff, 0xd8, 0xff, 0xe0]);
    assert!(state.image.is_none() || state.protocol.is_some());
}

#[test]
fn load_zero_length_bytes_clears_state() {
    let mut state = halfblocks_state();
    state.load("a".into(), &[]);
    assert!(state.image.is_none());
}

#[test]
fn render_into_one_pixel_rect_with_loaded_image_is_safe() {
    use image::{ImageBuffer, Rgba};
    let mut state = halfblocks_state();
    let img: ImageBuffer<Rgba<u8>, Vec<u8>> =
        ImageBuffer::from_fn(8, 8, |x, _| Rgba([x as u8 * 32, 64, 128, 255]));
    let mut buf = std::io::Cursor::new(Vec::new());
    img.write_to(&mut buf, image::ImageFormat::Png).unwrap();
    state.load("a".into(), buf.get_ref());
    render_state(state, 1, 1);
}
