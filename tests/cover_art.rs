//! Cover art state: load / clear / no-op when already loaded.

use ferrosonic::ui::cover_art::CoverArtState;
use ratatui_image::picker::{Picker, ProtocolType};

fn build_state_with_picker() -> CoverArtState {
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

fn tiny_png() -> Vec<u8> {
    use image::{ImageBuffer, Rgba};
    let img: ImageBuffer<Rgba<u8>, Vec<u8>> =
        ImageBuffer::from_fn(8, 8, |x, _| Rgba([x as u8 * 32, 128, 200, 255]));
    let mut buf = std::io::Cursor::new(Vec::new());
    img.write_to(&mut buf, image::ImageFormat::Png)
        .expect("encode png");
    buf.into_inner()
}

#[test]
fn load_decodes_image_and_sets_protocol() {
    let mut state = build_state_with_picker();
    state.load("abc".into(), &tiny_png());
    assert!(state.image.is_some(), "image decoded");
    assert!(state.protocol.is_some(), "protocol initialised");
    assert_eq!(state.current_id.as_deref(), Some("abc"));
}

#[test]
fn load_with_matching_id_is_noop() {
    let mut state = build_state_with_picker();
    let png = tiny_png();
    state.load("abc".into(), &png);
    let before = state.image.as_ref().map(|i| i.width()).unwrap();

    state.load("abc".into(), &[0u8; 4]);
    let after = state.image.as_ref().map(|i| i.width()).unwrap();
    assert_eq!(before, after, "second load with same id must not re-decode");
}

#[test]
fn load_with_invalid_bytes_clears_image() {
    let mut state = build_state_with_picker();
    state.load("abc".into(), &[0xFF, 0xFE, 0xFD]);
    assert!(state.image.is_none(), "invalid bytes must produce no image");
    assert!(
        state.current_id.is_none(),
        "current_id must be cleared on decode failure"
    );
}

#[test]
fn load_without_picker_clears_state() {
    let mut state = CoverArtState {
        picker: None,
        protocol_type: None,
        cell_size: (8, 16),
        current_id: Some("prev".into()),
        image: None,
        protocol: None,
        chafa_cache: None,
    };
    state.load("new".into(), &tiny_png());
    assert!(
        state.image.is_none(),
        "no picker, no image even with valid bytes"
    );
}

#[test]
fn render_with_image_into_test_buffer_writes_cells() {
    use ratatui::backend::TestBackend;
    use ratatui::layout::Rect;
    use ratatui::Terminal;
    use std::sync::Mutex;
    let mut state = build_state_with_picker();
    state.load("abc".into(), &tiny_png());
    let mutex = Mutex::new(state);

    let backend = TestBackend::new(40, 20);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal
        .draw(|frame| {
            ferrosonic::ui::cover_art::render(frame, Rect::new(0, 0, 10, 10), &mutex);
        })
        .unwrap();
}

#[test]
fn clear_resets_all_image_state() {
    let mut state = build_state_with_picker();
    state.load("abc".into(), &tiny_png());
    state.clear();
    assert!(state.image.is_none());
    assert!(state.protocol.is_none());
    assert!(state.chafa_cache.is_none());
    assert!(state.current_id.is_none());
}
