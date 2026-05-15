//! Bolero fuzz target for the lenient IPC frame reader. Asserts the parser never panics on arbitrary bytes. Runs as a property test under cargo nextest and as a fuzzer under cargo bolero.

use ferrosonic::ipc::frame::read_frame_lenient_with_cap;

const FUZZ_CAP: usize = 16 * 1024 * 1024;

#[test]
fn fuzz_read_frame_lenient_never_panics() {
    bolero::check!().for_each(|input: &[u8]| {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .expect("build tokio runtime for fuzz iteration");
        rt.block_on(async {
            let mut reader = input;
            let _ = read_frame_lenient_with_cap(&mut reader, FUZZ_CAP).await;
        });
    });
}
