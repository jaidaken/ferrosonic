//! Cover-art state on top of `ratatui-image`.

use std::sync::Mutex;

use ratatui::layout::Rect;
use ratatui::Frame;
use ratatui_image::picker::Picker;
use ratatui_image::protocol::StatefulProtocol;
use ratatui_image::StatefulImage;

pub struct CoverArtState {
    /// `None` when probe failed — rendering is then a no-op.
    pub picker: Option<Picker>,
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

    /// Idempotent for the same `id`.
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

pub fn render(frame: &mut Frame, area: Rect, state: &Mutex<CoverArtState>) {
    if let Ok(mut guard) = state.try_lock() {
        if let Some(protocol) = guard.protocol.as_mut() {
            let widget = StatefulImage::default();
            frame.render_stateful_widget(widget, area, protocol);
        }
    }
}
