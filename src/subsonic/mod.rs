//! Subsonic API client module

pub mod auth;
pub mod client;
pub mod models;
pub mod stream_check;

pub use client::SubsonicClient;
