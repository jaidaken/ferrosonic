//! ui/widget_cava.rs: every render branch + color conversion.

use ferrosonic::app::state::{CavaColor, CavaRow, CavaSpan};
use ferrosonic::ui::widget_cava::CavaWidget;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::Widget;

fn span(text: &str, fg: CavaColor, bg: CavaColor) -> CavaSpan {
    CavaSpan {
        text: text.into(),
        fg,
        bg,
    }
}

#[test]
fn cava_widget_with_zero_area_returns_early() {
    let rows = vec![CavaRow {
        spans: vec![span("█", CavaColor::Default, CavaColor::Default)],
    }];
    let w = CavaWidget::new(&rows);
    let mut buf = Buffer::empty(Rect::new(0, 0, 0, 5));
    w.render(buf.area, &mut buf);
}

#[test]
fn cava_widget_with_empty_screen_returns_early() {
    let rows: Vec<CavaRow> = vec![];
    let w = CavaWidget::new(&rows);
    let mut buf = Buffer::empty(Rect::new(0, 0, 20, 5));
    w.render(buf.area, &mut buf);
}

#[test]
fn cava_widget_renders_indexed_color() {
    let rows = vec![CavaRow {
        spans: vec![span("X", CavaColor::Indexed(42), CavaColor::Indexed(7))],
    }];
    let w = CavaWidget::new(&rows);
    let mut buf = Buffer::empty(Rect::new(0, 0, 10, 2));
    w.render(buf.area, &mut buf);
}

#[test]
fn cava_widget_renders_rgb_color() {
    let rows = vec![CavaRow {
        spans: vec![span(
            "Y",
            CavaColor::Rgb(255, 128, 0),
            CavaColor::Rgb(0, 0, 0),
        )],
    }];
    let w = CavaWidget::new(&rows);
    let mut buf = Buffer::empty(Rect::new(0, 0, 10, 2));
    w.render(buf.area, &mut buf);
}

#[test]
fn cava_widget_renders_default_color() {
    let rows = vec![CavaRow {
        spans: vec![span("Z", CavaColor::Default, CavaColor::Default)],
    }];
    let w = CavaWidget::new(&rows);
    let mut buf = Buffer::empty(Rect::new(0, 0, 10, 2));
    w.render(buf.area, &mut buf);
}

#[test]
fn cava_widget_truncates_text_overflowing_width() {
    let rows = vec![CavaRow {
        spans: vec![span("1234567890", CavaColor::Default, CavaColor::Default)],
    }];
    let w = CavaWidget::new(&rows);
    let mut buf = Buffer::empty(Rect::new(0, 0, 5, 1));
    w.render(buf.area, &mut buf);
}

#[test]
fn cava_widget_breaks_at_height_overflow() {
    let rows = vec![
        CavaRow {
            spans: vec![span("r0", CavaColor::Default, CavaColor::Default)],
        },
        CavaRow {
            spans: vec![span("r1", CavaColor::Default, CavaColor::Default)],
        },
        CavaRow {
            spans: vec![span("r2", CavaColor::Default, CavaColor::Default)],
        },
    ];
    let w = CavaWidget::new(&rows);
    let mut buf = Buffer::empty(Rect::new(0, 0, 10, 2));
    w.render(buf.area, &mut buf);
}

#[test]
fn cava_widget_renders_multiple_spans_in_a_row() {
    let rows = vec![CavaRow {
        spans: vec![
            span("AAA", CavaColor::Indexed(1), CavaColor::Default),
            span("BBB", CavaColor::Indexed(2), CavaColor::Default),
            span("CCC", CavaColor::Rgb(10, 20, 30), CavaColor::Default),
        ],
    }];
    let w = CavaWidget::new(&rows);
    let mut buf = Buffer::empty(Rect::new(0, 0, 30, 2));
    w.render(buf.area, &mut buf);
}
