//! Shared test harness for ferrosonic integration tests.
//!
//! Pulled into each `tests/<name>.rs` via `mod common;`. Cargo treats
//! files inside this directory as a private module and never builds
//! them as a standalone test binary.

#![allow(dead_code, unused_imports)]

pub mod fake_mpv;
pub mod fake_subsonic;
pub mod fixtures;
pub mod test_daemon;

pub use fake_mpv::FakeMpv;
pub use fake_subsonic::FakeSubsonic;
pub use fixtures::{song, songs};
pub use test_daemon::TestDaemon;
