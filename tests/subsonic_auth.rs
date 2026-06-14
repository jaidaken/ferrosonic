//! Salt generation contract for Subsonic token auth. The doc test on
//! generate_auth_params asserts these too, but doc tests do not run under
//! nextest/cargo-mutants, so the generate_salt char-mapping mutants survive
//! without a nextest-visible test.

use ferrosonic::secret::Secret;
use ferrosonic::subsonic::auth::generate_auth_params;
use std::collections::HashSet;

#[test]
fn salt_is_sixteen_lowercase_alphanumeric_chars_spanning_the_full_range() {
    let pw = Secret::from_string("hunter2".to_string());
    let mut seen: HashSet<char> = HashSet::new();

    for _ in 0..64 {
        let (salt, _token) = generate_auth_params(&pw);
        assert_eq!(salt.len(), 16, "salt must be 16 chars");
        for c in salt.chars() {
            assert!(
                c.is_ascii_digit() || ('a'..='z').contains(&c),
                "salt char {c:?} must be a digit or lowercase letter"
            );
            seen.insert(c);
        }
    }

    // A mutant collapsing the index->char range (e.g. `idx - 10` -> `idx / 10`)
    // yields only a handful of distinct chars; the real range spans 36.
    assert!(
        seen.len() >= 30,
        "salt must draw from the full alphanumeric range, saw only {} distinct chars",
        seen.len()
    );
}

#[test]
fn token_is_a_32_char_lowercase_hex_digest() {
    let pw = Secret::from_string("hunter2".to_string());
    let (_salt, token) = generate_auth_params(&pw);
    assert_eq!(token.len(), 32, "md5 hex digest is 32 chars");
    assert!(
        token.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
        "token must be lowercase hex: {token}"
    );
}
