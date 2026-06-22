//! Ferrosonic, terminal-based Subsonic music client.
#![warn(clippy::pedantic, clippy::nursery, missing_docs, rust_2018_idioms)]
// Audited: flagged sites are clear `match Some/None`, not map_or targets.
#![allow(clippy::option_if_let_else)]
// Audited: every `as` cast is bounded-safe in context (UI dims, clamped settings,
// masked bytes, frame-limited lengths, guarded indices, small audio values).
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss
)]
// Audited: the flagged structs are config/UI-state with independent toggles;
// an enum or bitflags would not fit serde'd independent on/off flags.
#![allow(clippy::struct_excessive_bools)]

pub mod app;
pub mod audio;
pub mod config;
pub mod daemon;
pub mod error;
pub mod io_util;
pub mod ipc;
pub mod mpris;
pub mod proc_util;
pub mod secret;
pub mod secret_store;
pub mod subsonic;
pub mod ui;
