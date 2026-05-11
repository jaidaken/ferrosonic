//! Cover-art state on top of `ratatui-image`.

use std::sync::Mutex;

use ratatui::layout::Rect;
use ratatui::Frame;
use ratatui_image::picker::{Picker, ProtocolType};
use ratatui_image::protocol::StatefulProtocol;
use ratatui_image::StatefulImage;
use tracing::{info, warn};

pub struct CoverArtState {
    pub picker: Option<Picker>,
    pub protocol_type: Option<ProtocolType>,
    pub current_id: Option<String>,
    pub protocol: Option<StatefulProtocol>,
}

impl CoverArtState {
    pub fn init() -> Self {
        match Picker::from_query_stdio() {
            Ok(picker) => {
                let pt = picker.protocol_type();
                info!(
                    "Cover-art picker initialised: protocol={:?} font_size={:?}",
                    pt,
                    picker.font_size()
                );
                Self {
                    protocol_type: Some(pt),
                    picker: Some(picker),
                    current_id: None,
                    protocol: None,
                }
            }
            Err(e) => {
                warn!(
                    "Cover-art terminal probe failed ({}); falling back to half-blocks",
                    e
                );
                let mut picker = Picker::from_fontsize((8, 16));
                picker.set_protocol_type(ProtocolType::Halfblocks);
                Self {
                    protocol_type: Some(ProtocolType::Halfblocks),
                    picker: Some(picker),
                    current_id: None,
                    protocol: None,
                }
            }
        }
    }

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
                info!(
                    "Cover-art decoded: {}x{} bytes={} id={}",
                    dyn_img.width(),
                    dyn_img.height(),
                    bytes.len(),
                    id
                );
                self.protocol = Some(picker.new_resize_protocol(dyn_img));
                self.current_id = Some(id);
            }
            Err(e) => {
                warn!("Cover-art decode failed: {}", e);
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
