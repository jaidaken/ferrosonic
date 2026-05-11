//! Cover-art state + render helpers built on top of `ratatui-image`.
//!
//! Lifecycle:
//! 1. `CoverArtState::init()` queries the terminal for image-protocol
//!    support (kitty / iTerm2 / sixel) and font-cell size. Falls back
//!    to a half-block protocol if detection fails.
//! 2. The event-pump task calls `load()` on every `NowPlayingChanged`
//!    with fresh bytes from the daemon. Decoded and turned into a
//!    `StatefulProtocol` ready to render.
//! 3. The render loop locks the state and hands `&mut protocol` to a
//!    `StatefulImage` widget for that frame.

use std::sync::Mutex;

use ratatui::layout::Rect;
use ratatui::Frame;
use ratatui_image::picker::Picker;
use ratatui_image::protocol::StatefulProtocol;
use ratatui_image::StatefulImage;

pub struct CoverArtState {
    /// Terminal protocol probe. `None` when detection failed; in that
    /// case we just skip image rendering entirely.
    pub picker: Option<Picker>,
    /// Subsonic `coverArt` id currently loaded into `protocol`. Used
    /// to skip the (expensive) decode + protocol build when the
    /// playing track hasn't changed.
    pub current_id: Option<String>,
    pub protocol: Option<StatefulProtocol>,
}

impl CoverArtState {
    pub fn init() -> Self {
        let picker = Picker::from_query_stdio().ok();
        Self {
            picker,
            current_id: None,
            protocol: None,
        }
    }

    /// Replace the active image with a new decode. Cheap to call with
    /// the same id repeatedly — it short-circuits.
    pub fn load(&mut self, id: String, bytes: &[u8]) {
        if self.current_id.as_deref() == Some(id.as_str()) && self.protocol.is_some() {
            return;
        }
        let Some(picker) = self.picker.as_ref() else {
            self.protocol = None;
            return;
        };
        match image::load_from_memory(bytes) {
            Ok(dyn_img) => {
                self.protocol = Some(picker.new_resize_protocol(dyn_img));
                self.current_id = Some(id);
            }
            Err(_) => {
                self.protocol = None;
                self.current_id = None;
            }
        }
    }

    pub fn clear(&mut self) {
        self.current_id = None;
        self.protocol = None;
    }
}

/// Convenience: lock the shared cover-art state and render the active
/// image into `area`. No-op if no image is loaded or detection failed.
pub fn render(frame: &mut Frame, area: Rect, state: &Mutex<CoverArtState>) {
    if let Ok(mut guard) = state.try_lock() {
        if let Some(protocol) = guard.protocol.as_mut() {
            let widget = StatefulImage::default();
            frame.render_stateful_widget(widget, area, protocol);
        }
    }
}
