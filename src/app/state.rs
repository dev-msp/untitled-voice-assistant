use std::sync::Arc;

use cpal::{
    traits::{DeviceTrait, HostTrait},
    Device, SupportedStreamConfig,
};
use itertools::Itertools;
use serde::{ser::SerializeStruct, Deserialize, Serialize, Serializer};

use super::command::Command;

#[derive(Debug, Default, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct RecordingSession {
    input_device: Option<String>,
    sample_rate: Option<u32>,
}

impl RecordingSession {
    pub fn new(input_device: Option<String>, sample_rate: Option<u32>) -> Self {
        Self {
            input_device,
            sample_rate,
        }
    }

    pub fn device_name(&self) -> Option<&str> {
        self.input_device.as_deref()
    }

    pub fn sample_rate(&self) -> Option<u32> {
        self.sample_rate
    }

    pub fn supported_configs(
        &self,
    ) -> Result<impl Iterator<Item = (Arc<Device>, SupportedStreamConfig)> + '_, anyhow::Error>
    {
        let devices = cpal::default_host()
            .input_devices()?
            .filter_map(|x| {
                let name = x.name().ok()?;
                let Some(pat) = self.device_name() else {
                    return Some(x);
                };
                name.contains(pat).then_some(x)
            })
            .map(Arc::new)
            .collect_vec();

        let device_config_pairs = devices
            .into_iter()
            .map(|d| match d.supported_input_configs() {
                Ok(cfgs) => Ok(cfgs.map(move |cfg| (d.clone(), cfg))),
                Err(e) => Err(e),
            })
            .flatten_ok();

        Ok(device_config_pairs.filter_map(|r| {
            let Ok((d, c)) = r else {
                return None;
            };
            let sample_rate = self.sample_rate().map_or_else(
                || c.min_sample_rate().max(cpal::SampleRate(16000)),
                cpal::SampleRate,
            );
            if c.min_sample_rate() > sample_rate || c.max_sample_rate() < sample_rate {
                None
            } else {
                let min_sample_rate = c.min_sample_rate();
                Some((d, c.with_sample_rate(min_sample_rate)))
            }
        }))
    }
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
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

impl State {
    pub fn running(&self) -> bool {
        self.recording_session.is_some()
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

#[derive(Debug, Default, Clone, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(tag = "type", content = "data")]
pub enum Mode {
    #[serde(rename = "standard")]
    Standard,

    #[serde(rename = "clipboard")]
    Clipboard,

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
            Self::Clipboard => write!(f, "clipboard"),
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
