use crossbeam::channel::Receiver;

use super::{
    response::Response,
    state::{Mode, RecordingState, State},
};
use crate::{audio::Session, whisper::transcription::Model}; // Import Model

#[derive(Debug, Clone, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
// Add struct for transcription-specific parameters
pub struct TranscriptionParams {
    // Based on requirements and available Job fields
    #[serde(default)] // Make optional for deserialization
    pub model: Option<Model>,
    #[serde(default)]
    pub sample_rate: Option<u32>, // Or cpal::SampleRate? Using u32 for simplicity for now.
    #[serde(default)]
    pub prompt: Option<String>,
    // Add other relevant params if needed
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum Plumbing {
    #[serde(rename = "start")]
    Start(Session),

    #[serde(rename = "stop")]
    Stop, // need timestamp?

    #[serde(rename = "reset")]
    Reset,

    #[serde(rename = "mode")]
    Mode(Mode),

    #[serde(rename = "respond")]
    Respond(Response),

    // Add the command for triggering a transcription directly
    #[serde(rename = "transcribe")]
    Transcribe {
        audio_data: Vec<f32>,
        params: TranscriptionParams,
    },
}

impl Plumbing {
    #[must_use]
    pub fn as_response(&self) -> Option<Response> {
        match self {
            Self::Mode(mode) => Some(Response::NewMode(mode.clone())),
            Self::Respond(r) => Some(r.clone()),
            // Transcription command doesn't generate an immediate Response on receipt
            // The response comes back later via the channel after processing
            Self::Transcribe { .. } | Self::Start(_) | Self::Stop | Self::Reset => None,
        }
    }
}

pub struct CmdStream(Receiver<Plumbing>);

impl CmdStream {
    #[must_use]
    pub fn new(recv: Receiver<Plumbing>) -> Self {
        Self(recv)
    }

    pub fn iter(&mut self) -> impl Iterator<Item = Plumbing> + '_ {
        self.0.iter()
    }

    pub fn run_state_machine<'a>(
        &'a mut self,
        state: &'a mut super::state::RecordingState,
    ) -> impl Iterator<Item = (Plumbing, Option<RecordingState>)> + 'a {
        self.iter().map(move |cmd| {
            log::debug!("Received command: {:?}", cmd);
            log::trace!("Current state: {:?}", state);
            let initial = state.clone();
            if state.handle(cmd.clone()) {
                // State transitions only happen for Start, Stop, Mode
                let transitioned = match &cmd {
                    Plumbing::Start(session) => state.start(session.clone()),
                    Plumbing::Stop => state.stop(),
                    Plumbing::Mode(mode) if !state.running() => state.change_mode(mode.clone()),
                    _ => false, // Transcribe, Reset, Respond do not change the core State::audio or State::mode
                };

                if transitioned {
                    log::trace!("State transitioned to {:?}", state);
                    (cmd, Some(state.clone()))
                } else {
                    log::trace!("No state transition from {:?}", initial);
                    // Return the original state if no transition occurred,
                    // but still process commands that don't change state.
                    // The run_state_machine concept might need refinement if
                    // commands without state changes should trigger processing.
                    // For now, let's return Some(initial) for commands that don't
                    // change state but should be processed (Transcribe, Reset, Respond).
                    match &cmd {
                        Plumbing::Transcribe { .. } | Plumbing::Reset | Plumbing::Respond(_) => {
                            (cmd, Some(initial))
                        }
                        _ => (cmd, None), // For commands that *could* change state but didn't (e.g. Start when already Started)
                    }
                }
            } else {
                // Commands that don't change state are processed here
                log::trace!("No state transition from {:?}", initial);
                (cmd, None)
            }
        })
    }
}
