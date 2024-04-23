use crossbeam::channel::Receiver;

use super::{
    response::Response,
    state::{Mode, State},
};
use crate::audio::Session;

#[derive(Debug, Clone, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum Command {
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

impl Command {
    #[must_use]
    pub fn as_response(&self) -> Option<Response> {
        match self {
            Self::Mode(mode) => Some(Response::NewMode(mode.clone())),
            Self::Respond(r) => Some(r.clone()),
            _ => None,
        }
    }
}

pub struct CmdStream(Receiver<Command>);

impl CmdStream {
    #[must_use]
    pub fn new(recv: Receiver<Command>) -> Self {
        Self(recv)
    }

    pub fn iter(&mut self) -> impl Iterator<Item = Command> + '_ {
        self.0.iter()
    }

    pub fn run_state_machine<'a>(
        &'a mut self,
        state: &'a mut super::state::State,
    ) -> impl Iterator<Item = (Command, Option<State>)> + 'a {
        self.iter().map(move |cmd| {
            log::debug!("Received command: {:?}", cmd);
            log::trace!("Current state: {:?}", state);
            let initial = state.clone();
            if state.next_state(&cmd) {
                log::trace!("State transitioned to {:?}", state);
                (cmd, Some(state.clone()))
            } else {
                log::trace!("No state transition from {:?}", initial);
                (cmd, None)
            }
        })
    }
}
