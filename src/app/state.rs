use serde::{Deserialize, Serialize};

use super::command::Command;
use crate::audio::Session;

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub enum Audio {
    #[default]
    Idle,
    Started(Session),
    Stopped(Session),
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct State {
    audio: Audio,
    mode: Mode,
}

impl State {
    #[must_use]
    pub fn running(&self) -> bool {
        matches!(self.audio, Audio::Started(_))
    }

    #[must_use]
    pub fn mode(&self) -> Mode {
        self.mode.clone()
    }

    pub fn prompt(&self) -> Option<String> {
        match &self.audio {
            Audio::Started(s) | Audio::Stopped(s) => s.prompt().map(str::to_owned),
            Audio::Idle => None,
        }
    }

    fn start(&mut self, session: Session) -> bool {
        match self.audio {
            Audio::Idle | Audio::Stopped(_) => {
                self.audio = Audio::Started(session);
                true
            }
            Audio::Started(_) => false,
        }
    }

    fn stop(&mut self) -> bool {
        match &self.audio {
            Audio::Started(s) => {
                self.audio = Audio::Stopped(s.clone());
                true
            }
            _ => false,
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
            Command::Start(session) => self.start(session.clone()),
            Command::Stop => self.stop(),
            Command::Mode(mode) if !self.running() => self.change_mode(mode.clone()),
            Command::Mode(_) => false,
            // Nothing changes about the state when we send these commands, but we still need to
            // return true so the event loop is triggered.
            //
            // TODO: I should consider making the event loop not sort of dependent on changes in
            // the state and find some other way to represent that.
            Command::Reset | Command::Respond(_) => true,
        }
    }
}

#[derive(Debug, Default, Clone, Eq, PartialEq, clap::ValueEnum, Deserialize, Serialize)]
#[serde(tag = "type", content = "data")]
pub enum Mode {
    #[serde(rename = "standard")]
    Standard,

    #[default]
    #[serde(rename = "live_typing")]
    LiveTyping,
}

impl std::fmt::Display for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::Standard => write!(f, "standard"),
            Self::LiveTyping => write!(f, "live_typing"),
        }
    }
}
