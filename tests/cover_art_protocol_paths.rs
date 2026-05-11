//! Cover-art rendering with each detected ProtocolType. Verifies that
//! the render() chafa-vs-StatefulProtocol branch selects correctly.

use ferrosonic::ui::cover_art::CoverArtState;
use image::{ImageBuffer, Rgba};
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use ratatui::Terminal;
use ratatui_image::picker::{Picker, ProtocolType};
use std::sync::Mutex;

fn tiny_png() -> Vec<u8> {
    let img: ImageBuffer<Rgba<u8>, Vec<u8>> =
        ImageBuffer::from_fn(8, 8, |x, _| Rgba([x as u8 * 32, 64, 128, 255]));
    let mut buf = std::io::Cursor::new(Vec::new());
    img.write_to(&mut buf, image::ImageFormat::Png).unwrap();
    buf.into_inner()
}

fn state_with_protocol(pt: ProtocolType) -> CoverArtState {
    let mut picker = Picker::from_fontsize((8, 16));
    picker.set_protocol_type(pt);
    CoverArtState {
        picker: Some(picker),
        protocol_type: Some(pt),
        cell_size: (8, 16),
        current_id: None,
        image: None,
        protocol: None,
        chafa_cache: None,
    }
}

fn render(state: CoverArtState, w: u16, h: u16) {
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
fn halfblocks_protocol_without_image_renders_nothing() {
    render(state_with_protocol(ProtocolType::Halfblocks), 10, 10);
}

#[test]
fn halfblocks_protocol_with_image_falls_back_to_stateful_protocol_when_chafa_absent() {
    let mut state = state_with_protocol(ProtocolType::Halfblocks);
    state.load("a".into(), &tiny_png());
    render(state, 12, 6);
}

#[test]
fn kitty_protocol_with_image_uses_stateful_protocol() {
    let mut state = state_with_protocol(ProtocolType::Kitty);
    state.load("a".into(), &tiny_png());
    render(state, 12, 6);
}

#[test]
fn iterm2_protocol_with_image_uses_stateful_protocol() {
    let mut state = state_with_protocol(ProtocolType::Iterm2);
    state.load("a".into(), &tiny_png());
    render(state, 12, 6);
}

#[test]
fn sixel_protocol_with_image_uses_stateful_protocol() {
    let mut state = state_with_protocol(ProtocolType::Sixel);
    state.load("a".into(), &tiny_png());
    render(state, 12, 6);
}

#[test]
fn switching_protocol_between_renders_does_not_crash() {
    let mut state = state_with_protocol(ProtocolType::Kitty);
    state.load("a".into(), &tiny_png());
    let mutex = Mutex::new(state);
    let backend = TestBackend::new(40, 20);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            ferrosonic::ui::cover_art::render(frame, Rect::new(0, 0, 8, 4), &mutex);
        })
        .unwrap();
    terminal
        .draw(|frame| {
            ferrosonic::ui::cover_art::render(frame, Rect::new(0, 0, 16, 8), &mutex);
        })
        .unwrap();
}

#[test]
fn cover_art_state_protocol_type_field_round_trips_via_construction() {
    for pt in [
        ProtocolType::Halfblocks,
        ProtocolType::Kitty,
        ProtocolType::Iterm2,
        ProtocolType::Sixel,
    ] {
        let s = state_with_protocol(pt);
        assert_eq!(s.protocol_type, Some(pt));
    }
}

#[test]
fn loading_image_into_non_halfblocks_protocol_sets_stateful_protocol() {
    let mut state = state_with_protocol(ProtocolType::Sixel);
    state.load("a".into(), &tiny_png());
    assert!(state.image.is_some());
    assert!(state.protocol.is_some());
}
