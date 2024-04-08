use crossbeam::channel::Receiver;
use itertools::Itertools;

use super::{
    response::Response,
    state::{Mode, State},
};

#[derive(Debug, Clone, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum Command {
    #[serde(rename = "start")]
    Start,

    #[serde(rename = "stop")]
    Stop, // need timestamp?

    #[serde(rename = "reset")]
    Reset,

    #[serde(rename = "mode")]
    Mode(Mode),
}

impl Command {
    pub fn as_response(&self) -> Option<Response> {
        match self {
            Self::Mode(mode) => Some(Response::NewMode(mode.clone())),
            _ => None,
        }
    }

    pub fn as_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap()
    }
}

pub struct CmdStream(Receiver<serde_json::Value>);

impl CmdStream {
    pub fn new(recv: Receiver<serde_json::Value>) -> Self {
        Self(recv)
    }

    pub fn iter(&mut self) -> impl Iterator<Item = Result<Command, serde_json::Error>> + '_ {
        self.0.iter().map(serde_json::from_value)
    }

    pub fn run_state_machine<'a>(
        &'a mut self,
        state: &'a mut super::state::State,
    ) -> impl Iterator<Item = Result<(Command, Option<State>), serde_json::Error>> + 'a {
        self.iter().map_ok(move |cmd| {
            log::debug!("Received command: {:?}", cmd);
            log::debug!("Current state: {:?}", state);
            let initial = state.clone();
            let out = state.next_state(&cmd);
            if out {
                log::debug!("State transitioned to {:?}", state);
                (cmd, Some(state.clone()))
            } else {
                log::debug!("No state transition from {:?}", initial);
                (cmd, None)
            }
        })
    }
}
