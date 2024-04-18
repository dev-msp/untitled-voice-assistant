use std::sync::Arc;

use cpal::{
    traits::{DeviceTrait, HostTrait},
    Device,
};
use serde::{ser::SerializeStruct, Serialize, Serializer};

use super::command::Command;

#[derive(Debug, Default, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct RecordingSession {
    input_device: String,
    sample_rate: u32,
}

#[derive(Debug, Default, Clone, serde::Deserialize, serde::Serialize)]
pub struct State {
    recording_session: Option<RecordingSession>,
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

impl SupportedDevice {
    fn name(&self) -> Result<String, cpal::DeviceNameError> {
        self.device.name()
    }

    fn matches_session(&self, session: &RecordingSession) -> bool {
        self.name()
            .map(|n| n.contains(&session.input_device))
            .unwrap_or(false)
            && self.sample_rate_range.0 <= session.sample_rate
            && session.sample_rate <= self.sample_rate_range.1
    }
}

impl State {
    pub fn running(&self) -> bool {
        self.recording_session.is_some()
    }

    pub fn supported_devices(&self) -> Result<impl Iterator<Item = SupportedDevice>, DeviceError> {
        let supported_devices = cpal::default_host()
            .input_devices()?
            .filter_map(|device| {
                let supported_configs = device.supported_input_configs().ok()?;
                let d = Arc::new(device);

                Some(supported_configs.map(move |c| SupportedDevice {
                    device: d.clone(),
                    sample_rate_range: (c.min_sample_rate().0, c.max_sample_rate().0),
                    sample_format: c.sample_format(),
                    channels: c.channels(),
                    buffer_size: c.buffer_size().clone(),
                }))
            })
            .flatten();
        Ok(supported_devices)
    }

    pub fn recording_device(&self) -> Result<Option<Arc<Device>>, DeviceError> {
        let Some(ref session) = self.recording_session else {
            return Ok(None);
        };

        Ok(self
            .supported_devices()?
            .find_map(|d| d.matches_session(session).then_some(d.device)))
    }

    pub fn mode(&self) -> Mode {
        self.mode.clone()
    }

    fn start(&mut self, session: RecordingSession) -> bool {
        if self.running() {
            false
        } else {
            self.recording_session = Some(session);
            true
        }
    }

    fn stop(&mut self) -> bool {
        if self.running() {
            self.recording_session = None;
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
            Command::Start(session) => self.start(session.clone()),
            Command::Stop => self.stop(),
            Command::Mode(mode) => {
                if !self.running() {
                    self.change_mode(mode.clone())
                } else {
                    false
                }
            }
            // Nothing changes about the state when we send these commands, but we still need to
            // return true so the event loop is triggered.
            //
            // TODO: I should consider making the event loop not sort of dependent on changes in
            // the state and find some other way to represent that.
            Command::Reset => true,
            Command::Respond(_) => true,
        }
    }
}

#[derive(Debug, Default, Clone, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(tag = "type", content = "data")]
pub enum Mode {
    #[serde(rename = "standard")]
    Standard,

    #[serde(rename = "clipboard")]
    Clipboard { use_clipboard: bool, use_llm: bool },

    #[default]
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
