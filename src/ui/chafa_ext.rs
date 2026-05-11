//! Direct libchafa wrapper with high-quality knobs that
//! ratatui-image's bundled wrapper doesn't expose.
//!
//! Loaded via libloading; on probe failure callers fall through to
//! ratatui-image's encoder (primitive halfblocks or its own chafa
//! bindings depending on availability).

use std::ffi::c_void;
use std::sync::OnceLock;

use image::DynamicImage;
use libloading::Library;
use ratatui::style::Color;

type ChafaSymbolMap = *mut c_void;
type ChafaCanvasConfig = *mut c_void;
type ChafaCanvas = *mut c_void;

// Symbol-tag mask. `CHAFA_SYMBOL_TAG_ALL` from chafa.h:
// ~(EXTRA | BAD | UGLY) ≈ every symbol set chafa knows about.
const CHAFA_SYMBOL_TAG_ALL: u32 = 0xBFE7FFFF;

// CHAFA_PIXEL_RGB8 enum value.
const CHAFA_PIXEL_RGB8: u32 = 8;

// CHAFA_CANVAS_MODE_TRUECOLOR.
const CHAFA_CANVAS_MODE_TRUECOLOR: u32 = 0;

// CHAFA_COLOR_EXTRACTOR_AVERAGE — best visual quality for photographic
// content (median is sharper but more posterised).
const CHAFA_COLOR_EXTRACTOR_AVERAGE: u32 = 0;

// CHAFA_DITHER_MODE_DIFFUSION (Floyd-Steinberg) — substantially smoother
// gradients than the default NONE.
const CHAFA_DITHER_MODE_DIFFUSION: u32 = 3;

type ChafaSymbolMapNew = unsafe extern "C" fn() -> ChafaSymbolMap;
type ChafaSymbolMapAddByTags = unsafe extern "C" fn(ChafaSymbolMap, u32);
type ChafaSymbolMapUnref = unsafe extern "C" fn(ChafaSymbolMap);
type ChafaCanvasConfigNew = unsafe extern "C" fn() -> ChafaCanvasConfig;
type ChafaCanvasConfigSetSymbolMap = unsafe extern "C" fn(ChafaCanvasConfig, ChafaSymbolMap);
type ChafaCanvasConfigSetGeometry = unsafe extern "C" fn(ChafaCanvasConfig, i32, i32);
type ChafaCanvasConfigSetCanvasMode = unsafe extern "C" fn(ChafaCanvasConfig, u32);
type ChafaCanvasConfigSetColorExtractor = unsafe extern "C" fn(ChafaCanvasConfig, u32);
type ChafaCanvasConfigSetDitherMode = unsafe extern "C" fn(ChafaCanvasConfig, u32);
type ChafaCanvasConfigSetWorkFactor = unsafe extern "C" fn(ChafaCanvasConfig, f32);
type ChafaCanvasConfigUnref = unsafe extern "C" fn(ChafaCanvasConfig);
type ChafaCanvasNew = unsafe extern "C" fn(ChafaCanvasConfig) -> ChafaCanvas;
type ChafaCanvasDrawAllPixels =
    unsafe extern "C" fn(ChafaCanvas, u32, *const u8, i32, i32, i32);
type ChafaCanvasGetCharAt = unsafe extern "C" fn(ChafaCanvas, i32, i32) -> u32;
type ChafaCanvasGetColorsAt =
    unsafe extern "C" fn(ChafaCanvas, i32, i32, *mut i32, *mut i32);
type ChafaCanvasUnref = unsafe extern "C" fn(ChafaCanvas);

struct ChafaLib {
    _lib: Library,
    symbol_map: ChafaSymbolMap,
    symbol_map_unref: ChafaSymbolMapUnref,
    canvas_config_new: ChafaCanvasConfigNew,
    canvas_config_set_symbol_map: ChafaCanvasConfigSetSymbolMap,
    canvas_config_set_geometry: ChafaCanvasConfigSetGeometry,
    canvas_config_set_canvas_mode: Option<ChafaCanvasConfigSetCanvasMode>,
    canvas_config_set_color_extractor: Option<ChafaCanvasConfigSetColorExtractor>,
    canvas_config_set_dither_mode: Option<ChafaCanvasConfigSetDitherMode>,
    canvas_config_set_work_factor: Option<ChafaCanvasConfigSetWorkFactor>,
    canvas_config_unref: ChafaCanvasConfigUnref,
    canvas_new: ChafaCanvasNew,
    canvas_draw_all_pixels: ChafaCanvasDrawAllPixels,
    canvas_get_char_at: ChafaCanvasGetCharAt,
    canvas_get_colors_at: ChafaCanvasGetColorsAt,
    canvas_unref: ChafaCanvasUnref,
}

unsafe impl Send for ChafaLib {}
unsafe impl Sync for ChafaLib {}

impl Drop for ChafaLib {
    fn drop(&mut self) {
        unsafe {
            (self.symbol_map_unref)(self.symbol_map);
        }
    }
}

static CHAFA: OnceLock<Option<ChafaLib>> = OnceLock::new();

fn load() -> Option<ChafaLib> {
    unsafe {
        let names = ["libchafa.so.0", "libchafa.so", "libchafa.dylib", "chafa.dll"];
        let mut lib_opt: Option<Library> = None;
        for n in names {
            if let Ok(l) = Library::new(n) {
                lib_opt = Some(l);
                break;
            }
        }
        let lib = lib_opt?;

        let symbol_map_new: ChafaSymbolMapNew = *lib.get(b"chafa_symbol_map_new").ok()?;
        let symbol_map_add_by_tags: ChafaSymbolMapAddByTags =
            *lib.get(b"chafa_symbol_map_add_by_tags").ok()?;
        let symbol_map_unref: ChafaSymbolMapUnref = *lib.get(b"chafa_symbol_map_unref").ok()?;
        let canvas_config_new: ChafaCanvasConfigNew = *lib.get(b"chafa_canvas_config_new").ok()?;
        let canvas_config_set_symbol_map: ChafaCanvasConfigSetSymbolMap =
            *lib.get(b"chafa_canvas_config_set_symbol_map").ok()?;
        let canvas_config_set_geometry: ChafaCanvasConfigSetGeometry =
            *lib.get(b"chafa_canvas_config_set_geometry").ok()?;
        let canvas_config_unref: ChafaCanvasConfigUnref =
            *lib.get(b"chafa_canvas_config_unref").ok()?;
        let canvas_new: ChafaCanvasNew = *lib.get(b"chafa_canvas_new").ok()?;
        let canvas_draw_all_pixels: ChafaCanvasDrawAllPixels =
            *lib.get(b"chafa_canvas_draw_all_pixels").ok()?;
        let canvas_get_char_at: ChafaCanvasGetCharAt =
            *lib.get(b"chafa_canvas_get_char_at").ok()?;
        let canvas_get_colors_at: ChafaCanvasGetColorsAt =
            *lib.get(b"chafa_canvas_get_colors_at").ok()?;
        let canvas_unref: ChafaCanvasUnref = *lib.get(b"chafa_canvas_unref").ok()?;

        // Quality knobs — present in chafa >= 1.6 but probed lazily so
        // older builds still work with the basic feature set.
        let canvas_config_set_canvas_mode: Option<ChafaCanvasConfigSetCanvasMode> = lib
            .get::<ChafaCanvasConfigSetCanvasMode>(b"chafa_canvas_config_set_canvas_mode")
            .ok()
            .map(|s| *s);
        let canvas_config_set_color_extractor: Option<ChafaCanvasConfigSetColorExtractor> = lib
            .get::<ChafaCanvasConfigSetColorExtractor>(
                b"chafa_canvas_config_set_color_extractor",
            )
            .ok()
            .map(|s| *s);
        let canvas_config_set_dither_mode: Option<ChafaCanvasConfigSetDitherMode> = lib
            .get::<ChafaCanvasConfigSetDitherMode>(b"chafa_canvas_config_set_dither_mode")
            .ok()
            .map(|s| *s);
        let canvas_config_set_work_factor: Option<ChafaCanvasConfigSetWorkFactor> = lib
            .get::<ChafaCanvasConfigSetWorkFactor>(b"chafa_canvas_config_set_work_factor")
            .ok()
            .map(|s| *s);

        let symbol_map = symbol_map_new();
        if symbol_map.is_null() {
            return None;
        }
        symbol_map_add_by_tags(symbol_map, CHAFA_SYMBOL_TAG_ALL);

        Some(ChafaLib {
            _lib: lib,
            symbol_map,
            symbol_map_unref,
            canvas_config_new,
            canvas_config_set_symbol_map,
            canvas_config_set_geometry,
            canvas_config_set_canvas_mode,
            canvas_config_set_color_extractor,
            canvas_config_set_dither_mode,
            canvas_config_set_work_factor,
            canvas_config_unref,
            canvas_new,
            canvas_draw_all_pixels,
            canvas_get_char_at,
            canvas_get_colors_at,
            canvas_unref,
        })
    }
}

pub fn is_available() -> bool {
    CHAFA.get_or_init(load).is_some()
}

#[derive(Clone)]
pub struct EncodedCell {
    pub ch: char,
    pub fg: Color,
    pub bg: Color,
}

/// Encode an image into `width × height` truecolor cells with high
/// quality settings (Floyd-Steinberg dither, work factor 1.0, every
/// symbol set chafa supports). Returns `None` if libchafa isn't
/// available or the canvas allocation fails.
pub fn encode(img: &DynamicImage, width: u16, height: u16) -> Option<Vec<EncodedCell>> {
    if width == 0 || height == 0 {
        return None;
    }
    let chafa = CHAFA.get_or_init(load).as_ref()?;

    unsafe {
        let config = (chafa.canvas_config_new)();
        if config.is_null() {
            return None;
        }
        (chafa.canvas_config_set_symbol_map)(config, chafa.symbol_map);
        (chafa.canvas_config_set_geometry)(config, width as i32, height as i32);
        if let Some(f) = chafa.canvas_config_set_canvas_mode {
            f(config, CHAFA_CANVAS_MODE_TRUECOLOR);
        }
        if let Some(f) = chafa.canvas_config_set_color_extractor {
            f(config, CHAFA_COLOR_EXTRACTOR_AVERAGE);
        }
        if let Some(f) = chafa.canvas_config_set_dither_mode {
            f(config, CHAFA_DITHER_MODE_DIFFUSION);
        }
        if let Some(f) = chafa.canvas_config_set_work_factor {
            f(config, 1.0);
        }

        let canvas = (chafa.canvas_new)(config);
        if canvas.is_null() {
            (chafa.canvas_config_unref)(config);
            return None;
        }

        let rgb = img.to_rgb8();
        let (w, h) = rgb.dimensions();
        (chafa.canvas_draw_all_pixels)(
            canvas,
            CHAFA_PIXEL_RGB8,
            rgb.as_ptr(),
            w as i32,
            h as i32,
            (w * 3) as i32,
        );

        let mut cells = Vec::with_capacity((width as usize) * (height as usize));
        for y in 0..height {
            for x in 0..width {
                let c = (chafa.canvas_get_char_at)(canvas, x as i32, y as i32);
                let ch = char::from_u32(c).unwrap_or(' ');
                let mut fg: i32 = 0;
                let mut bg: i32 = 0;
                (chafa.canvas_get_colors_at)(canvas, x as i32, y as i32, &mut fg, &mut bg);
                cells.push(EncodedCell {
                    ch,
                    fg: argb_to_color(fg),
                    bg: argb_to_color(bg),
                });
            }
        }

        (chafa.canvas_unref)(canvas);
        (chafa.canvas_config_unref)(config);

        Some(cells)
    }
}

fn argb_to_color(c: i32) -> Color {
    Color::Rgb(
        ((c >> 16) & 0xff) as u8,
        ((c >> 8) & 0xff) as u8,
        (c & 0xff) as u8,
    )
}
