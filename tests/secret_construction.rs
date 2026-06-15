//! Secret construction and equality. from_bytes has only a doc test, which
//! nextest/cargo-mutants does not run, so its `Default::default()` mutant
//! survives; PartialEq has no test at all.

use ferrosonic::secret::Secret;

#[test]
fn from_bytes_preserves_the_exact_bytes() {
    let s = Secret::from_bytes(vec![0xDE, 0xAD, 0xBE, 0xEF]);
    assert_eq!(s.reveal_bytes(), &[0xDE, 0xAD, 0xBE, 0xEF]);
}

#[test]
fn equal_secrets_compare_equal_and_different_ones_do_not() {
    // `eq` mutated to always-true would make these two compare equal.
    let a = Secret::from_string("hunter2".to_string());
    let b = Secret::from_string("hunter2".to_string());
    let c = Secret::from_string("different".to_string());
    assert_eq!(a, b, "same plaintext must be equal");
    assert_ne!(a, c, "different plaintext must not be equal");
}
