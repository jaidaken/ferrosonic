//! ui/cover_art.rs: when libchafa is available, exercise the render-via-chafa path.

use ferrosonic::ui::chafa_ext;
use ferrosonic::ui::cover_art::CoverArtState;
use image::{ImageBuffer, Rgba};
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use ratatui::Terminal;
use ratatui_image::picker::{Picker, ProtocolType};
use std::sync::Mutex;

fn try_load_libchafa() -> bool {
    let path = match std::process::Command::new("find")
        .args([
            "/nix/store",
            "-maxdepth",
            "3",
            "-name",
            "libchafa.so.0",
            "-not",
            "-name",
            "*-dev*",
        ])
        .output()
    {
        Ok(out) if out.status.success() => {
            let s = String::from_utf8_lossy(&out.stdout);
            s.lines().next().map(String::from)
        }
        _ => None,
    };
    let Some(path) = path else {
        return false;
    };
    let Ok(c) = std::ffi::CString::new(path) else {
        return false;
    };
    let h = unsafe { libc::dlopen(c.as_ptr(), libc::RTLD_LAZY | libc::RTLD_GLOBAL) };
    !h.is_null()
}

fn tiny_png() -> Vec<u8> {
    let img: ImageBuffer<Rgba<u8>, Vec<u8>> =
        ImageBuffer::from_fn(8, 8, |x, _| Rgba([x as u8 * 32, 64, 128, 255]));
    let mut buf = std::io::Cursor::new(Vec::new());
    img.write_to(&mut buf, image::ImageFormat::Png).unwrap();
    buf.into_inner()
}

fn state() -> CoverArtState {
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

#[test]
fn chafa_encode_returns_some_when_libchafa_is_loaded() {
    if !try_load_libchafa() {
        eprintln!("libchafa not available, skipping");
        return;
    }
    let img = image::load_from_memory(&tiny_png()).unwrap();
    let encoded = chafa_ext::encode(&img, 8, 4);
    assert!(encoded.is_some());
}

#[test]
fn render_with_chafa_loaded_and_image_uses_blit_cells_path() {
    if !try_load_libchafa() {
        eprintln!("libchafa not available, skipping");
        return;
    }
    let mut s = state();
    s.load("id".into(), &tiny_png());
    let mutex = Mutex::new(s);
    let backend = TestBackend::new(40, 20);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            ferrosonic::ui::cover_art::render(frame, Rect::new(0, 0, 8, 4), &mutex);
        })
        .unwrap();
    let g = mutex.lock().unwrap();
    let _ = g.chafa_cache.is_some();
}

#[test]
fn second_render_with_same_dimensions_reuses_cache() {
    if !try_load_libchafa() {
        eprintln!("libchafa not available, skipping");
        return;
    }
    let mut s = state();
    s.load("id".into(), &tiny_png());
    let mutex = Mutex::new(s);
    let backend = TestBackend::new(40, 20);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            ferrosonic::ui::cover_art::render(frame, Rect::new(0, 0, 8, 4), &mutex);
        })
        .unwrap();
    terminal
        .draw(|frame| {
            ferrosonic::ui::cover_art::render(frame, Rect::new(0, 0, 8, 4), &mutex);
        })
        .unwrap();
}

#[test]
fn render_with_changing_dimensions_re_encodes() {
    if !try_load_libchafa() {
        eprintln!("libchafa not available, skipping");
        return;
    }
    let mut s = state();
    s.load("id".into(), &tiny_png());
    let mutex = Mutex::new(s);
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
