//! Shared test harness. Imported via `mod common;` in each test.

#![allow(dead_code, unused_imports)]

pub mod fake_mpv;
pub mod fake_subsonic;
pub mod fixtures;
pub mod test_daemon;

pub use fake_mpv::FakeMpv;
pub use fake_subsonic::FakeSubsonic;
pub use fixtures::{song, songs};
pub use test_daemon::TestDaemon;
