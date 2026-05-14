//! Pure-logic tests for cava drain_into_parser: WouldBlock, EOF,
//! HardError, partial reads, multi-frame coalescing.

use std::io::{Cursor, Error, ErrorKind, Read, Result};

use ferrosonic::app::cava_pipe::{drain_into_parser, screen_to_cava_rows, DrainOutcome};

/// Reader that returns scripted chunks, then a final outcome (EOF,
/// WouldBlock, or HardError). Used to exercise each drain branch.
struct ScriptedReader {
    chunks: Vec<Vec<u8>>,
    final_kind: ReadEnd,
}

#[derive(Clone, Copy)]
#[allow(dead_code)]
enum ReadEnd {
    Eof,
    WouldBlock,
    HardError,
}

impl Read for ScriptedReader {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        if let Some(chunk) = self.chunks.first().cloned() {
            self.chunks.remove(0);
            let n = chunk.len().min(buf.len());
            buf[..n].copy_from_slice(&chunk[..n]);
            return Ok(n);
        }
        match self.final_kind {
            ReadEnd::Eof => Ok(0),
            ReadEnd::WouldBlock => Err(Error::new(ErrorKind::WouldBlock, "wb")),
            ReadEnd::HardError => Err(Error::other("dead pty")),
        }
    }
}

#[test]
fn drain_returns_bytes_when_chunks_arrive() {
    let mut reader = ScriptedReader {
        chunks: vec![b"hello".to_vec()],
        final_kind: ReadEnd::WouldBlock,
    };
    let mut parser = vt100::Parser::new(1, 16, 0);
    let outcome = drain_into_parser(&mut reader, &mut parser);
    assert_eq!(outcome, DrainOutcome::Bytes);
}

#[test]
fn drain_returns_no_data_on_immediate_would_block() {
    let mut reader = ScriptedReader {
        chunks: vec![],
        final_kind: ReadEnd::WouldBlock,
    };
    let mut parser = vt100::Parser::new(1, 16, 0);
    let outcome = drain_into_parser(&mut reader, &mut parser);
    assert_eq!(outcome, DrainOutcome::NoData);
}

#[test]
fn drain_returns_eof_on_immediate_eof() {
    // Previously this returned NoData and froze the visualizer. Now
    // Ok(0) is surfaced as Eof so the caller can re-spawn cava.
    let mut reader = Cursor::new(Vec::<u8>::new());
    let mut parser = vt100::Parser::new(1, 16, 0);
    let outcome = drain_into_parser(&mut reader, &mut parser);
    assert_eq!(outcome, DrainOutcome::Eof);
}

#[test]
fn drain_returns_hard_error_when_reader_errors_with_no_prior_bytes() {
    let mut reader = ScriptedReader {
        chunks: vec![],
        final_kind: ReadEnd::HardError,
    };
    let mut parser = vt100::Parser::new(1, 16, 0);
    let outcome = drain_into_parser(&mut reader, &mut parser);
    assert_eq!(outcome, DrainOutcome::HardError);
}

#[test]
fn drain_returns_hard_error_if_error_arrives_after_some_bytes() {
    let mut reader = ScriptedReader {
        chunks: vec![b"AB".to_vec()],
        final_kind: ReadEnd::HardError,
    };
    let mut parser = vt100::Parser::new(1, 8, 0);
    let outcome = drain_into_parser(&mut reader, &mut parser);
    assert_eq!(
        outcome,
        DrainOutcome::HardError,
        "hard error after partial read must short-circuit"
    );
}

#[test]
fn drain_coalesces_multiple_chunks_into_one_screen_update() {
    let mut reader = ScriptedReader {
        chunks: vec![b"ab".to_vec(), b"cd".to_vec(), b"ef".to_vec()],
        final_kind: ReadEnd::WouldBlock,
    };
    let mut parser = vt100::Parser::new(1, 8, 0);
    let outcome = drain_into_parser(&mut reader, &mut parser);
    assert_eq!(outcome, DrainOutcome::Bytes);
    let rows = screen_to_cava_rows(parser.screen());
    let text: String = rows[0].spans.iter().map(|s| s.text.as_str()).collect();
    assert!(text.starts_with("abcdef"), "got {:?}", text);
}

#[test]
fn drain_handles_partial_utf8_split_across_reads() {
    // Box-drawing block "█" (U+2588) is 3 bytes in UTF-8: E2 96 88.
    // vt100 should buffer across reads and assemble cleanly.
    let mut reader = ScriptedReader {
        chunks: vec![vec![0xE2, 0x96], vec![0x88]],
        final_kind: ReadEnd::WouldBlock,
    };
    let mut parser = vt100::Parser::new(1, 4, 0);
    let outcome = drain_into_parser(&mut reader, &mut parser);
    assert_eq!(outcome, DrainOutcome::Bytes);
    let rows = screen_to_cava_rows(parser.screen());
    let text: String = rows[0].spans.iter().map(|s| s.text.as_str()).collect();
    assert!(
        text.contains('\u{2588}'),
        "expected full block in row text; got {:?}",
        text
    );
}

#[test]
fn drain_processes_ansi_color_escapes_correctly() {
    let mut reader = Cursor::new(b"\x1b[31mR\x1b[32mG\x1b[34mB".to_vec());
    let mut parser = vt100::Parser::new(1, 8, 0);
    let outcome = drain_into_parser(&mut reader, &mut parser);
    assert_eq!(outcome, DrainOutcome::Bytes);
    let rows = screen_to_cava_rows(parser.screen());
    let span_count = rows[0].spans.len();
    assert!(
        span_count >= 3,
        "three color changes should yield at least 3 spans; got {}",
        span_count
    );
}

#[test]
fn drain_large_buffer_reads_full_payload() {
    let payload = vec![b'x'; 32 * 1024];
    let mut reader = Cursor::new(payload.clone());
    let mut parser = vt100::Parser::new(24, 80, 65536);
    let outcome = drain_into_parser(&mut reader, &mut parser);
    assert_eq!(outcome, DrainOutcome::Bytes);
}
