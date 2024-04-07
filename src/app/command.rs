use std::str::FromStr;

use crossbeam::channel::Receiver;
use regex::Regex;

use super::state::Mode;

#[derive(Debug, Clone, Eq, PartialEq, serde::Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum Command {
    #[serde(rename = "start")]
    Start,

    #[serde(rename = "stop")]
    Stop, // need timestamp?

    #[serde(rename = "mode")]
    Mode(Mode),
}

impl FromStr for Command {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let value = match s {
            "start" => Self::Start,
            "stop" => Self::Stop,
            s => {
                let re = Regex::new(r"^mode ([a-z_]+)$")?;
                let mode = re
                    .captures(s)
                    .and_then(|c| c.get(1))
                    .ok_or_else(|| anyhow::anyhow!("Invalid mode").context(s.to_string()))?;
                Self::Mode(serde_json::from_str(&format!("\"{}\"", mode.as_str()))?)
            }
        };
        Ok(value)
    }
}

pub struct CmdStream(Receiver<serde_json::Value>);

impl CmdStream {
    pub fn new(recv: Receiver<serde_json::Value>) -> Self {
        Self(recv)
    }

    pub fn iter(&mut self) -> impl Iterator<Item = Command> + '_ {
        self.0.iter().flat_map(|s| {
            log::debug!("Received: {}", s);
            let out: Option<Command> = match serde_json::from_value(s) {
                Ok(c) => Some(c),
                Err(e) => {
                    log::error!("{e}");
                    None
                }
            };
            log::debug!("Parsed: {:?}", out);
            out
        })
    }

    pub fn wait_for<F>(&mut self, f: F) -> Result<Command, anyhow::Error>
    where
        F: Fn(&Command) -> bool,
    {
        self.iter()
            .find(|c| f(c))
            .ok_or_else(|| anyhow::anyhow!("exhausted command receiver"))
    }
}
