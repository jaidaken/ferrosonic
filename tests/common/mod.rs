//! Shared test harness. Imported via `mod common;` in each test.

#![allow(dead_code, unused_imports)]

pub mod fake_mpv;
pub mod fake_subsonic;
pub mod fixtures;
pub mod pw_recorder;
pub mod recording_client;
pub mod render;
pub mod test_daemon;

pub use fake_mpv::FakeMpv;
pub use fake_subsonic::FakeSubsonic;
pub use fixtures::{song, song_starred, songs};
pub use pw_recorder::RecordingPwRunner;
pub use recording_client::RecordingClient;
pub use render::{render, render_styled, StyledScreen};
pub use test_daemon::TestDaemon;
