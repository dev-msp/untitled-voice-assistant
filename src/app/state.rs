use super::command::Command;

#[derive(Debug, Default, Clone, serde::Deserialize, serde::Serialize)]
pub struct State {
    running: bool,
    mode: Mode,
}

impl State {
    pub fn running(&self) -> bool {
        self.running
    }

    pub fn mode(&self) -> Mode {
        self.mode.clone()
    }

    fn start(&mut self) -> bool {
        if self.running {
            false
        } else {
            self.running = true;
            true
        }
    }

    fn stop(&mut self) -> bool {
        if self.running {
            self.running = false;
            true
        } else {
            false
        }
    }

    pub fn change_mode(&mut self, mode: Mode) -> bool {
        if self.mode == mode {
            false
        } else {
            self.mode = mode;
            true
        }
    }

    pub fn next_state(&mut self, cmd: &Command) -> bool {
        match cmd {
            Command::Start => self.start(),
            Command::Stop => self.stop(),
            Command::Mode(mode) => {
                if !self.running {
                    self.change_mode(mode.clone())
                } else {
                    false
                }
            }
            // Nothing changes about the state when we send a reset command. It's just about
            // quitting.
            //
            // TODO: I should consider making the event loop not sort of dependent on changes in the
            // state and find some other way to represent that.
            Command::Reset => true,
        }
    }
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
