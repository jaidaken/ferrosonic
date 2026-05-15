//! ui/cover_art.rs: chafa probe + nix-store discovery + blit_cells paths.

use ferrosonic::ui::chafa_ext;
use ferrosonic::ui::cover_art::{ChafaCache, CoverArtState};
use image::{ImageBuffer, Rgba};
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use ratatui::Terminal;
use ratatui_image::picker::{Picker, ProtocolType};
use std::sync::Mutex;

fn tiny_png() -> Vec<u8> {
    let img: ImageBuffer<Rgba<u8>, Vec<u8>> =
        ImageBuffer::from_fn(8, 8, |_, _| Rgba([200, 100, 50, 255]));
    let mut buf = std::io::Cursor::new(Vec::new());
    img.write_to(&mut buf, image::ImageFormat::Png).unwrap();
    buf.into_inner()
}

fn state_with_chafa_cache(width: u16, height: u16) -> CoverArtState {
    let mut picker = Picker::from_fontsize((8, 16));
    picker.set_protocol_type(ProtocolType::Halfblocks);
    let img = image::load_from_memory(&tiny_png()).unwrap();
    let cells: Vec<chafa_ext::EncodedCell> = (0..(width as usize * height as usize))
        .map(|i| chafa_ext::EncodedCell {
            ch: char::from_u32(0x2588).unwrap(),
            fg: ratatui::style::Color::Indexed((i % 256) as u8),
            bg: ratatui::style::Color::Black,
        })
        .collect();
    CoverArtState {
        picker: Some(picker),
        protocol_type: Some(ProtocolType::Halfblocks),
        cell_size: (8, 16),
        current_id: Some("id".into()),
        image: Some(img),
        protocol: None,
        chafa_cache: Some(ChafaCache {
            width,
            height,
            cells,
        }),
    }
}

#[test]
fn render_with_chafa_cache_blits_cells_to_buffer() {
    let s = state_with_chafa_cache(8, 4);
    let mutex = Mutex::new(s);
    let backend = TestBackend::new(40, 20);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            ferrosonic::ui::cover_art::render(frame, Rect::new(0, 0, 8, 4), &mutex);
        })
        .unwrap();
    insta::assert_snapshot!(format!("{:?}", terminal.backend().buffer()));
}

#[test]
fn render_area_smaller_than_cache_clips_to_area() {
    let s = state_with_chafa_cache(16, 8);
    let mutex = Mutex::new(s);
    let backend = TestBackend::new(40, 20);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            ferrosonic::ui::cover_art::render(frame, Rect::new(0, 0, 4, 2), &mutex);
        })
        .unwrap();
    insta::assert_snapshot!(format!("{:?}", terminal.backend().buffer()));
}

#[test]
fn render_area_larger_than_cache_clips_to_cache() {
    let s = state_with_chafa_cache(4, 2);
    let mutex = Mutex::new(s);
    let backend = TestBackend::new(40, 20);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            ferrosonic::ui::cover_art::render(frame, Rect::new(0, 0, 20, 10), &mutex);
        })
        .unwrap();
    insta::assert_snapshot!(format!("{:?}", terminal.backend().buffer()));
}

#[test]
fn chafa_ext_is_available_returns_bool() {
    let first = chafa_ext::is_available();
    let second = chafa_ext::is_available();
    assert_eq!(
        first, second,
        "is_available must be stable across calls within one process"
    );
}

#[test]
fn chafa_cache_size_mismatch_triggers_reencode_or_falls_through() {
    let mut s = state_with_chafa_cache(8, 4);
    s.image = Some(image::load_from_memory(&tiny_png()).unwrap());
    let mutex = Mutex::new(s);
    let backend = TestBackend::new(40, 20);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            ferrosonic::ui::cover_art::render(frame, Rect::new(0, 0, 12, 6), &mutex);
        })
        .unwrap();
    insta::assert_snapshot!(format!("{:?}", terminal.backend().buffer()));
}

#[test]
fn init_state_in_subprocess_returns_sane_state_without_terminal() {
    let s = CoverArtState::init();
    assert!(
        s.picker.is_some(),
        "init must always populate a picker, even when terminal probe fails"
    );
    assert!(
        s.cell_size.0 > 0 && s.cell_size.1 > 0,
        "init must always set a positive cell size; got {:?}",
        s.cell_size
    );
    assert!(s.image.is_none(), "init must not pre-load any image");
    assert!(s.current_id.is_none());
    assert!(s.chafa_cache.is_none());
}
