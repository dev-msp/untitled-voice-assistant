use std::sync::{Arc, Mutex};

use cpal::Device;

use crate::App;

pub struct Daemon {
    app: App,
    input_device: Option<Device>,
    state: Arc<Mutex<State>>,
}

#[derive(Debug, Default, Clone, serde::Deserialize, serde::Serialize)]
pub struct State {
    running: bool,
    mode: Mode,
}

#[derive(Debug, Default, Clone, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(tag = "type", content = "data")]
pub enum Mode {
    #[default]
    #[serde(rename = "standard")]
    Standard,

    #[serde(rename = "clipboard")]
    Clipboard,

    #[serde(rename = "live_typing")]
    LiveTyping,

    #[serde(rename = "chat")]
    Chat(Chat),
}

impl std::fmt::Display for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::Standard => write!(f, "standard"),
            Self::Clipboard {
                use_clipboard,
                use_llm,
            } => {
                write!(
                    f,
                    "clipboard: {}, llm: {}",
                    if *use_clipboard { "using" } else { "not using" },
                    if *use_llm { "using" } else { "not using" }
                )
            }
            Self::LiveTyping => write!(f, "live_typing"),
            Self::Chat(_) => write!(f, "chat"),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(tag = "type", content = "prompt")]
pub enum Chat {
    StartNew(String),
    Continue,
}

#[derive(Debug, Clone, Eq, PartialEq, serde::Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum Command {
    #[serde(rename = "start")]
    Start,

    #[serde(rename = "stop")]
    Stop, // need timestamp?

    #[serde(rename = "mode")]
    Mode(String),
}
