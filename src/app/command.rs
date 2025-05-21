use crossbeam::channel::Receiver;

use super::{
    response::Response,
    state::{Mode, RecordingState, State},
};
use crate::audio::Session;

#[derive(Debug, Clone, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
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
}

impl Plumbing {
    #[must_use]
    pub fn as_response(&self) -> Option<Response> {
        match self {
            Self::Mode(mode) => Some(Response::NewMode(mode.clone())),
            Self::Respond(r) => Some(r.clone()),
            _ => None,
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
                log::trace!("State transitioned to {:?}", state);
                (cmd, Some(state.clone()))
            } else {
                log::trace!("No state transition from {:?}", initial);
                (cmd, None)
            }
        })
    }
}
