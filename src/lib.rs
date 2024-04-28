#![deny(clippy::style)]
#![deny(clippy::pedantic)]
#![allow(clippy::missing_errors_doc)]

pub mod app;
pub mod audio;
pub mod config;
pub mod socket;
pub mod sync;
pub mod whisper;

pub use app::DaemonInit;
