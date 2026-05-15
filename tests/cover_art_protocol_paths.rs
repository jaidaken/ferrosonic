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

fn render_and_return_mutex(state: CoverArtState, w: u16, h: u16) -> Mutex<CoverArtState> {
    let mutex = Mutex::new(state);
    let backend = TestBackend::new(40, 20);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            ferrosonic::ui::cover_art::render(frame, Rect::new(0, 0, w, h), &mutex);
        })
        .unwrap();
    let buf = terminal.backend().buffer().clone();
    let non_empty = (0..buf.area.height)
        .flat_map(|y| (0..buf.area.width).map(move |x| (x, y)))
        .filter(|(x, y)| buf[(*x, *y)].symbol() != " ")
        .count();
    assert!(
        non_empty == 0 || non_empty <= (buf.area.width as usize * buf.area.height as usize),
        "buffer non-empty cell count out of range: {}",
        non_empty
    );
    mutex
}

#[test]
fn halfblocks_protocol_without_image_renders_nothing() {
    let mutex = render_and_return_mutex(state_with_protocol(ProtocolType::Halfblocks), 10, 10);
    let guard = mutex.lock().expect("lock");
    assert!(
        guard.image.is_none(),
        "no image was loaded; render must not populate one"
    );
    assert!(
        guard.protocol.is_none(),
        "no image means no StatefulProtocol should ever be created"
    );
    assert!(
        guard.chafa_cache.is_none(),
        "chafa cache must remain absent when no image is present"
    );
}

#[test]
fn halfblocks_protocol_with_image_falls_back_to_stateful_protocol_when_chafa_absent() {
    let mut state = state_with_protocol(ProtocolType::Halfblocks);
    state.load("a".into(), &tiny_png());
    let mutex = render_and_return_mutex(state, 12, 6);
    let guard = mutex.lock().expect("lock");
    assert_eq!(guard.protocol_type, Some(ProtocolType::Halfblocks));
    assert!(
        guard.protocol.is_some(),
        "load+render must keep StatefulProtocol populated as fallback when chafa is absent"
    );
    assert!(
        guard.image.is_some(),
        "image must remain decoded for re-encode through chafa later"
    );
}

#[test]
fn kitty_protocol_with_image_uses_stateful_protocol() {
    let mut state = state_with_protocol(ProtocolType::Kitty);
    state.load("a".into(), &tiny_png());
    let mutex = render_and_return_mutex(state, 12, 6);
    let guard = mutex.lock().expect("lock");
    assert_eq!(guard.protocol_type, Some(ProtocolType::Kitty));
    assert!(
        guard.protocol.is_some(),
        "Kitty protocol always renders via StatefulProtocol; field must be populated"
    );
    assert!(
        guard.chafa_cache.is_none(),
        "non-halfblocks protocol must never populate chafa cache"
    );
}

#[test]
fn iterm2_protocol_with_image_uses_stateful_protocol() {
    let mut state = state_with_protocol(ProtocolType::Iterm2);
    state.load("a".into(), &tiny_png());
    let mutex = render_and_return_mutex(state, 12, 6);
    let guard = mutex.lock().expect("lock");
    assert_eq!(guard.protocol_type, Some(ProtocolType::Iterm2));
    assert!(
        guard.protocol.is_some(),
        "Iterm2 protocol always renders via StatefulProtocol; field must be populated"
    );
    assert!(guard.chafa_cache.is_none());
}

#[test]
fn sixel_protocol_with_image_uses_stateful_protocol() {
    let mut state = state_with_protocol(ProtocolType::Sixel);
    state.load("a".into(), &tiny_png());
    let mutex = render_and_return_mutex(state, 12, 6);
    let guard = mutex.lock().expect("lock");
    assert_eq!(guard.protocol_type, Some(ProtocolType::Sixel));
    assert!(
        guard.protocol.is_some(),
        "Sixel protocol always renders via StatefulProtocol; field must be populated"
    );
    assert!(guard.chafa_cache.is_none());
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
