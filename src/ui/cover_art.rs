//! Cover-art state on top of `ratatui-image`, with our own chafa-direct
//! encoder layered on top for the half-blocks path (richer dither +
//! work factor than ratatui-image's bundled chafa wrapper exposes).

use std::sync::Mutex;

use image::DynamicImage;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::Frame;
use ratatui_image::picker::{Picker, ProtocolType};
use ratatui_image::protocol::StatefulProtocol;
use ratatui_image::StatefulImage;
use tracing::{info, warn};

use super::chafa_ext;

pub struct CoverArtState {
    pub picker: Option<Picker>,
    pub protocol_type: Option<ProtocolType>,
    pub cell_size: (u16, u16),
    pub current_id: Option<String>,
    /// Decoded image kept around so we can re-encode through chafa
    /// when the art rect resizes.
    pub image: Option<DynamicImage>,
    /// ratatui-image protocol — used for sixel / kitty / iTerm2 and as
    /// a fallback if chafa is unavailable.
    pub protocol: Option<StatefulProtocol>,
    /// Cached chafa encoding for the current (image, width, height).
    pub chafa_cache: Option<ChafaCache>,
}

pub struct ChafaCache {
    pub width: u16,
    pub height: u16,
    pub cells: Vec<chafa_ext::EncodedCell>,
}

fn query_cell_size() -> Option<(u16, u16)> {
    use std::os::unix::io::AsRawFd;
    let fd = std::io::stdout().as_raw_fd();
    let mut ws: libc::winsize = unsafe { std::mem::zeroed() };
    let r = unsafe { libc::ioctl(fd, libc::TIOCGWINSZ, &mut ws as *mut _) };
    if r != 0 || ws.ws_xpixel == 0 || ws.ws_ypixel == 0 || ws.ws_col == 0 || ws.ws_row == 0 {
        return None;
    }
    Some((ws.ws_xpixel / ws.ws_col, ws.ws_ypixel / ws.ws_row))
}

fn probe_chafa() {
    let by_soname = ["libchafa.so.0", "libchafa.so", "libchafa.dylib"];
    for name in by_soname {
        if try_dlopen(name) {
            info!("Cover-art: libchafa loaded via system loader ({})", name);
            return;
        }
    }
    let common = [
        "/usr/lib/libchafa.so.0",
        "/usr/lib64/libchafa.so.0",
        "/usr/lib/x86_64-linux-gnu/libchafa.so.0",
        "/usr/local/lib/libchafa.so.0",
        "/lib/libchafa.so.0",
        "/lib64/libchafa.so.0",
        "/lib/x86_64-linux-gnu/libchafa.so.0",
        "/opt/homebrew/lib/libchafa.dylib",
        "/usr/local/lib/libchafa.dylib",
    ];
    for path in common {
        if std::path::Path::new(path).exists() && try_dlopen(path) {
            info!("Cover-art: libchafa preloaded from {}", path);
            return;
        }
    }
    if let Some(path) = find_chafa_in_nix_store() {
        if try_dlopen(&path) {
            info!("Cover-art: libchafa preloaded from {}", path);
            return;
        }
    }
    info!(
        "Cover-art: libchafa not found; using primitive halfblocks. \
         Install `chafa` (system or NixOS) for higher fidelity."
    );
}

fn try_dlopen(path: &str) -> bool {
    let Ok(c) = std::ffi::CString::new(path) else {
        return false;
    };
    let h = unsafe { libc::dlopen(c.as_ptr(), libc::RTLD_LAZY) };
    !h.is_null()
}

fn find_chafa_in_nix_store() -> Option<String> {
    let store = std::path::Path::new("/nix/store");
    if !store.is_dir() {
        return None;
    }
    let entries = std::fs::read_dir(store).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if !name.contains("-chafa-") {
            continue;
        }
        if name.ends_with("-bin") || name.ends_with("-dev") || name.ends_with("-man") {
            continue;
        }
        let lib = path.join("lib").join("libchafa.so.0");
        if lib.is_file() {
            if let Some(s) = lib.to_str().map(String::from) {
                return Some(s);
            }
        }
    }
    None
}

impl CoverArtState {
    pub fn init() -> Self {
        let queried_cell = query_cell_size();
        probe_chafa();

        let (picker, protocol_type) = match Picker::from_query_stdio() {
            Ok(picker) => {
                let pt = picker.protocol_type();
                let picker_fs = picker.font_size();
                let cell_size = queried_cell.unwrap_or(picker_fs);
                info!(
                    "Cover-art picker initialised: protocol={:?} cell_size={:?} (picker reported {:?})",
                    pt, cell_size, picker_fs
                );
                (Some(picker), Some(pt))
            }
            Err(e) => {
                warn!(
                    "Cover-art terminal probe failed ({}); falling back to half-blocks",
                    e
                );
                let mut picker = Picker::from_fontsize((8, 16));
                picker.set_protocol_type(ProtocolType::Halfblocks);
                (Some(picker), Some(ProtocolType::Halfblocks))
            }
        };

        let cell_size = queried_cell
            .unwrap_or_else(|| picker.as_ref().map(|p| p.font_size()).unwrap_or((10, 20)));

        Self {
            picker,
            protocol_type,
            cell_size,
            current_id: None,
            image: None,
            protocol: None,
            chafa_cache: None,
        }
    }

    /// Decode raw bytes into our held image + a ratatui-image protocol
    /// fallback. Clears the chafa cache so the next render re-encodes.
    pub fn load(&mut self, id: String, bytes: &[u8]) {
        if self.current_id.as_deref() == Some(id.as_str()) && self.image.is_some() {
            return;
        }
        let Some(picker) = self.picker.as_ref() else {
            self.image = None;
            self.protocol = None;
            self.chafa_cache = None;
            return;
        };
        match image::load_from_memory(bytes) {
            Ok(dyn_img) => {
                info!(
                    "Cover-art decoded: {}x{} bytes={} id={}",
                    dyn_img.width(),
                    dyn_img.height(),
                    bytes.len(),
                    id
                );
                self.protocol = Some(picker.new_resize_protocol(dyn_img.clone()));
                self.image = Some(dyn_img);
                self.current_id = Some(id);
                self.chafa_cache = None;
            }
            Err(e) => {
                warn!("Cover-art decode failed: {}", e);
                self.image = None;
                self.protocol = None;
                self.chafa_cache = None;
                self.current_id = None;
            }
        }
    }

    pub fn clear(&mut self) {
        self.current_id = None;
        self.image = None;
        self.protocol = None;
        self.chafa_cache = None;
    }

    /// Re-encode via chafa for the requested cell area, caching the
    /// result. Returns true if the cache is populated for that size.
    fn ensure_chafa(&mut self, width: u16, height: u16) -> bool {
        if let Some(cache) = &self.chafa_cache {
            if cache.width == width && cache.height == height {
                return true;
            }
        }
        let Some(img) = self.image.as_ref() else {
            return false;
        };
        match chafa_ext::encode(img, width, height) {
            Some(cells) => {
                self.chafa_cache = Some(ChafaCache {
                    width,
                    height,
                    cells,
                });
                true
            }
            None => false,
        }
    }
}

pub fn render(frame: &mut Frame, area: Rect, state: &Mutex<CoverArtState>) {
    let Ok(mut guard) = state.try_lock() else {
        return;
    };

    let use_chafa = matches!(guard.protocol_type, Some(ProtocolType::Halfblocks))
        && chafa_ext::is_available()
        && guard.image.is_some();

    if use_chafa && guard.ensure_chafa(area.width, area.height) {
        let cache = guard
            .chafa_cache
            .as_ref()
            .expect("ensure_chafa returned true but cache is empty");
        blit_cells(frame.buffer_mut(), area, cache);
        return;
    }

    // Fallback: ratatui-image's StatefulImage (handles sixel / kitty /
    // iTerm2 and its own halfblocks if our chafa path isn't usable).
    if let Some(protocol) = guard.protocol.as_mut() {
        let widget = StatefulImage::default();
        frame.render_stateful_widget(widget, area, protocol);
    }
}

fn blit_cells(buf: &mut Buffer, area: Rect, cache: &ChafaCache) {
    let w = cache.width.min(area.width);
    let h = cache.height.min(area.height);
    for y in 0..h {
        for x in 0..w {
            let idx = (y as usize) * (cache.width as usize) + (x as usize);
            let Some(cell) = cache.cells.get(idx) else {
                continue;
            };
            if let Some(buf_cell) = buf.cell_mut((area.x + x, area.y + y)) {
                buf_cell
                    .set_char(cell.ch)
                    .set_style(Style::default().fg(cell.fg).bg(cell.bg));
            }
        }
    }
}
