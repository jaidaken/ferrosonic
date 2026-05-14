//! Pure-logic tests for cava screen_to_cava_rows.

use ferrosonic::app::cava_pipe::screen_to_cava_rows;
use ferrosonic::app::state::{CavaColor, CavaRow};

fn parse(rows: u16, cols: u16, input: &[u8]) -> Vec<CavaRow> {
    let mut parser = vt100::Parser::new(rows, cols, 0);
    parser.process(input);
    screen_to_cava_rows(parser.screen())
}

#[test]
fn empty_input_yields_empty_rows() {
    let rows = parse(2, 4, b"");
    assert_eq!(rows.len(), 2);
    for row in &rows {
        assert!(row.spans.iter().all(|s| s.text.chars().all(|c| c == ' ')));
    }
}

#[test]
fn ascii_text_renders_into_a_single_span() {
    let rows = parse(1, 8, b"hello");
    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    let text: String = row.spans.iter().map(|s| s.text.as_str()).collect();
    assert!(text.starts_with("hello"));
}

#[test]
fn fg_color_change_produces_multiple_spans() {
    let mut input = Vec::new();
    input.extend_from_slice(b"\x1b[31mAA");
    input.extend_from_slice(b"\x1b[32mBB");
    let rows = parse(1, 4, &input);
    let row = &rows[0];
    let colors: Vec<CavaColor> = row.spans.iter().map(|s| s.fg).collect();
    assert!(
        colors.windows(2).any(|w| w[0] != w[1]),
        "expected at least one color change across spans; got {:?}",
        colors
    );
}

#[test]
fn rgb_color_round_trips() {
    let input = b"\x1b[38;2;255;128;64mX";
    let rows = parse(1, 1, input);
    let row = &rows[0];
    assert!(
        row.spans
            .iter()
            .any(|s| matches!(s.fg, CavaColor::Rgb(255, 128, 64))),
        "expected RGB(255,128,64) span; got {:?}",
        row.spans.iter().map(|s| s.fg).collect::<Vec<_>>()
    );
}

#[test]
fn indexed_color_is_preserved() {
    let input = b"\x1b[38;5;201mZ";
    let rows = parse(1, 1, input);
    let row = &rows[0];
    assert!(
        row.spans
            .iter()
            .any(|s| matches!(s.fg, CavaColor::Indexed(201))),
        "expected Indexed(201) span"
    );
}

#[test]
fn rows_match_screen_dimensions() {
    let rows = parse(5, 10, b"hi");
    assert_eq!(rows.len(), 5);
}
