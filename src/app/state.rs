use std::sync::Arc;

use cpal::{traits::DeviceTrait, Device};
use serde::{ser::SerializeStruct, Deserialize, Serialize, Serializer};

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

#[derive(Debug, thiserror::Error)]
pub enum DeviceError {
    #[error("Device error: {0}")]
    Core(#[from] cpal::DevicesError),

    #[error("Device name error: {0}")]
    Name(#[from] cpal::DeviceNameError),
}

pub struct SupportedDevice {
    device: Arc<Device>,
    sample_rate_range: (u32, u32),
    sample_format: cpal::SampleFormat,
    channels: u16,
    buffer_size: cpal::SupportedBufferSize,
}

impl Serialize for SupportedDevice {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let (min, max) = self.sample_rate_range;
        let (buf_floor, buf_ceil) = match self.buffer_size {
            cpal::SupportedBufferSize::Range { min, max } => (min, max),
            cpal::SupportedBufferSize::Unknown => unimplemented!(),
        };
        let sample_format = match self.sample_format {
            cpal::SampleFormat::F32 => "f32",
            cpal::SampleFormat::I16 => "i16",
            cpal::SampleFormat::U16 => "u16",
        };
        let mut state = serializer.serialize_struct("SupportedDevice", 6)?;
        state.serialize_field("device", &self.device.name().unwrap())?;
        state.serialize_field("sample_rate", &(min, max))?;
        state.serialize_field("sample_format", sample_format)?;
        state.serialize_field("channels", &self.channels)?;
        state.serialize_field("buffer_size", &(buf_floor, buf_ceil))?;
        state.end()
    }
}

impl State {
    pub fn running(&self) -> bool {
        matches!(self.audio, Audio::Started(_))
    }

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
