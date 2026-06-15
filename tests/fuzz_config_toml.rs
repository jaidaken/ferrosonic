//! Bolero fuzz target for Config::load_from_file. Writes arbitrary bytes to a tempfile then parses; the loader must never panic regardless of input. Runs as a property test under cargo nextest and as a fuzzer under cargo bolero.

mod common;
use ferrosonic::config::Config;

#[test]
fn fuzz_config_load_never_panics() {
    bolero::check!().for_each(|input: &[u8]| {
        let dir = common::tempdir();
        let path = dir.path().join("fuzz.toml");
        std::fs::write(&path, input).expect("write fuzz input to tempfile");
        let _ = Config::load_from_file(&path);
    });
}
