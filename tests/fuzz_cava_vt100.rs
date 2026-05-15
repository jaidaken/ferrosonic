//! Bolero fuzz target for screen_to_cava_rows over arbitrary vt100 byte streams. Asserts the screen-to-row converter never panics. Runs as a property test under cargo nextest and as a fuzzer under cargo bolero.

use ferrosonic::app::cava_pipe::screen_to_cava_rows;

#[test]
fn fuzz_cava_screen_never_panics() {
    bolero::check!().for_each(|input: &[u8]| {
        let mut parser = vt100::Parser::new(24, 80, 0);
        parser.process(input);
        let _ = screen_to_cava_rows(parser.screen());
    });
}
