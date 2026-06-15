//! B3 bolero fuzz target for `SubsonicResponse` deserialization. Arbitrary bytes must never panic the parser.

use ferrosonic::subsonic::models::SubsonicResponse;

#[test]
fn fuzz_subsonic_response_never_panics() {
    bolero::check!().with_type::<Vec<u8>>().for_each(|input| {
        let _ = serde_json::from_slice::<SubsonicResponse<serde_json::Value>>(input);
    });
}
