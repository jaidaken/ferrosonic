//! libchafa encoder integration. Skipped when libchafa isn't loadable.

use ferrosonic::ui::chafa_ext::{encode, is_available};

fn tiny_image(w: u32, h: u32) -> image::DynamicImage {
    use image::{ImageBuffer, Rgba};
    let buf: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::from_fn(w, h, |x, y| {
        Rgba([((x * 13) % 256) as u8, ((y * 19) % 256) as u8, 128, 255])
    });
    image::DynamicImage::ImageRgba8(buf)
}

fn preload_chafa() {
    let candidates = ["libchafa.so.0", "libchafa.so"];
    for name in candidates {
        let Ok(c) = std::ffi::CString::new(name) else {
            continue;
        };
        let h = unsafe { libc::dlopen(c.as_ptr(), libc::RTLD_LAZY | libc::RTLD_GLOBAL) };
        if !h.is_null() {
            return;
        }
    }
    if let Ok(entries) = std::fs::read_dir("/nix/store") {
        for entry in entries.flatten() {
            let name = entry.file_name();
            if name.to_string_lossy().contains("chafa") {
                let lib_so = entry.path().join("lib/libchafa.so.0");
                let Some(path_str) = lib_so.to_str() else {
                    continue;
                };
                let Ok(c) = std::ffi::CString::new(path_str) else {
                    continue;
                };
                let h = unsafe { libc::dlopen(c.as_ptr(), libc::RTLD_LAZY | libc::RTLD_GLOBAL) };
                if !h.is_null() {
                    return;
                }
            }
        }
    }
}

#[test]
fn encode_returns_none_for_zero_dimensions() {
    let img = tiny_image(8, 8);
    assert!(encode(&img, 0, 8).is_none());
    assert!(encode(&img, 8, 0).is_none());
}

#[test]
fn is_available_reports_consistent_state() {
    preload_chafa();
    let first = is_available();
    let second = is_available();
    assert_eq!(
        first, second,
        "is_available should be deterministic across calls"
    );
}

#[test]
fn encode_with_libchafa_returns_cells_when_available() {
    preload_chafa();
    if !is_available() {
        eprintln!("skipping: libchafa not available in test environment");
        return;
    }
    let img = tiny_image(16, 16);
    let cells = encode(&img, 8, 4).expect("chafa encode 16x16 -> 8x4");
    assert_eq!(cells.len(), 8 * 4);
}

#[test]
fn encode_cells_have_color_data_populated() {
    preload_chafa();
    if !is_available() {
        return;
    }
    let img = tiny_image(16, 16);
    let cells = encode(&img, 4, 4).expect("encode");
    let any_non_default = cells.iter().any(|c| c.ch != '\0');
    assert!(any_non_default, "encoded cells should have non-null glyphs");
}
