//! ui/cover_art.rs: load/clear/render branches.

use ferrosonic::ui::cover_art::CoverArtState;
use image::{ImageBuffer, Rgba};
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use ratatui::Terminal;
use ratatui_image::picker::{Picker, ProtocolType};
use std::sync::Mutex;

fn tiny_png() -> Vec<u8> {
    let img: ImageBuffer<Rgba<u8>, Vec<u8>> =
        ImageBuffer::from_fn(16, 16, |x, y| Rgba([(x ^ y) as u8 * 8, 64, 128, 255]));
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

#[test]
fn init_constructs_state_with_some_picker() {
    let s = CoverArtState::init();
    assert!(s.picker.is_some(), "init should set a picker");
}

#[test]
fn load_decodes_image_and_stores_protocol() {
    let mut s = state_with_protocol(ProtocolType::Halfblocks);
    s.load("id1".into(), &tiny_png());
    assert!(s.image.is_some());
    assert!(s.protocol.is_some());
    assert_eq!(s.current_id.as_deref(), Some("id1"));
}

#[test]
fn load_with_same_id_is_idempotent() {
    let mut s = state_with_protocol(ProtocolType::Halfblocks);
    s.load("same".into(), &tiny_png());
    let proto1 = s.protocol.is_some();
    let other_png = {
        let img: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::from_fn(4, 4, |_, _| Rgba([0; 4]));
        let mut buf = std::io::Cursor::new(Vec::new());
        img.write_to(&mut buf, image::ImageFormat::Png).unwrap();
        buf.into_inner()
    };
    s.load("same".into(), &other_png);
    assert!(proto1);
    assert!(s.image.is_some());
}

#[test]
fn load_with_invalid_bytes_clears_state() {
    let mut s = state_with_protocol(ProtocolType::Halfblocks);
    s.load("id1".into(), b"not an image");
    assert!(s.image.is_none());
    assert!(s.protocol.is_none());
    assert!(s.current_id.is_none());
}

#[test]
fn load_with_no_picker_clears_state() {
    let mut s = state_with_protocol(ProtocolType::Halfblocks);
    s.picker = None;
    s.load("id1".into(), &tiny_png());
    assert!(s.image.is_none());
    assert!(s.protocol.is_none());
}

#[test]
fn clear_resets_all_fields() {
    let mut s = state_with_protocol(ProtocolType::Halfblocks);
    s.load("id1".into(), &tiny_png());
    s.clear();
    assert!(s.image.is_none());
    assert!(s.protocol.is_none());
    assert!(s.current_id.is_none());
    assert!(s.chafa_cache.is_none());
}

#[test]
fn render_with_locked_mutex_returns_early() {
    let backend = TestBackend::new(40, 20);
    let mut terminal = Terminal::new(backend).unwrap();
    let inner = Mutex::new(state_with_protocol(ProtocolType::Halfblocks));
    terminal
        .draw(|frame| {
            ferrosonic::ui::cover_art::render(frame, Rect::new(0, 0, 10, 10), &inner);
        })
        .unwrap();
    let buf = terminal.backend().buffer().clone();
    let non_empty = (0..buf.area.height)
        .flat_map(|y| (0..buf.area.width).map(move |x| (x, y)))
        .filter(|(x, y)| buf[(*x, *y)].symbol() != " ")
        .count();
    assert_eq!(
        non_empty, 0,
        "no image and no protocol: render must not write any glyph; got {} non-empty",
        non_empty
    );
    let guard = inner.lock().unwrap();
    assert!(
        guard.image.is_none(),
        "render must not synthesise an image when none was loaded"
    );
}

#[test]
fn render_with_no_protocol_and_no_image_does_nothing() {
    let s = state_with_protocol(ProtocolType::Halfblocks);
    let mutex = Mutex::new(s);
    let backend = TestBackend::new(40, 20);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            ferrosonic::ui::cover_art::render(frame, Rect::new(0, 0, 8, 4), &mutex);
        })
        .unwrap();
    let buf = terminal.backend().buffer().clone();
    let non_empty = (0..buf.area.height)
        .flat_map(|y| (0..buf.area.width).map(move |x| (x, y)))
        .filter(|(x, y)| buf[(*x, *y)].symbol() != " ")
        .count();
    assert_eq!(
        non_empty, 0,
        "no image + no protocol: render must write no glyphs; got {} non-empty",
        non_empty
    );
    let guard = mutex.lock().unwrap();
    assert!(guard.chafa_cache.is_none());
    assert!(guard.protocol.is_none());
}

#[test]
fn render_with_loaded_image_uses_stateful_protocol_when_chafa_unavailable() {
    let mut s = state_with_protocol(ProtocolType::Kitty);
    s.load("id".into(), &tiny_png());
    let mutex = Mutex::new(s);
    let backend = TestBackend::new(40, 20);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            ferrosonic::ui::cover_art::render(frame, Rect::new(0, 0, 12, 6), &mutex);
        })
        .unwrap();
    let guard = mutex.lock().unwrap();
    assert_eq!(
        guard.protocol_type,
        Some(ProtocolType::Kitty),
        "Kitty protocol type must persist across render"
    );
    assert!(
        guard.protocol.is_some(),
        "Kitty + loaded image must route through StatefulProtocol, leaving it populated"
    );
    assert!(
        guard.chafa_cache.is_none(),
        "Kitty (not Halfblocks) must never use the chafa cache path"
    );
}

#[test]
fn render_zero_area_is_safe() {
    let mut s = state_with_protocol(ProtocolType::Halfblocks);
    s.load("id".into(), &tiny_png());
    let mutex = Mutex::new(s);
    let backend = TestBackend::new(40, 20);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            ferrosonic::ui::cover_art::render(frame, Rect::new(0, 0, 0, 0), &mutex);
        })
        .unwrap();
}
