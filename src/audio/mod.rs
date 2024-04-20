pub mod input;
pub mod vad;

mod controller;
mod process;
mod recording;

use input::MySample;
pub use recording::{Recording, Session};
