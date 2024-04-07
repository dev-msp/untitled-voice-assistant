use std::{io::Write, str::FromStr};

use cpal::Device;
use crossbeam::channel::{unbounded, Receiver};
use regex::Regex;
use sttx::{IteratorExt, Timing};

use crate::{
    audio::input::{controlled_recording, Recording},
    socket::receive_instructions,
    sync, whisper, App,
};

#[derive(Debug, Clone, Eq, PartialEq, serde::Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum Command {
    #[serde(rename = "start")]
    Start,

    #[serde(rename = "stop")]
    Stop, // need timestamp?

    #[serde(rename = "mode")]
    Mode(String),
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
                Self::Mode(mode.as_str().to_string())
            }
        };
        Ok(value)
    }
}

#[derive(Debug, Clone, Eq, PartialEq, serde::Serialize)]
#[serde(tag = "type", content = "data")]
pub enum Response {
    #[serde(rename = "ack")]
    Ack,

    #[serde(rename = "exit")]
    Exit(u8),

    #[serde(rename = "transcription")]
    Transcription(Option<String>),
}

impl From<Timing> for Response {
    fn from(t: Timing) -> Self {
        Self::Transcription(Some(t.content().to_string()))
    }
}

impl std::fmt::Display for Response {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::Ack => write!(f, "ACK"),
            Self::Exit(code) => write!(f, "EXIT {}", code),
            Self::Transcription(Some(s)) => write!(f, "TX {}", s),
            Self::Transcription(None) => write!(f, "TX_EMPTY"),
        }
    }
}

struct CmdStream(Receiver<serde_json::Value>);

impl CmdStream {
    fn new(recv: Receiver<serde_json::Value>) -> Self {
        Self(recv)
    }

    fn iter(&mut self) -> impl Iterator<Item = Command> + '_ {
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

    fn wait_for<F>(&mut self, f: F) -> Result<Command, anyhow::Error>
    where
        F: Fn(&Command) -> bool,
    {
        self.iter()
            .find(|c| f(c))
            .ok_or_else(|| anyhow::anyhow!("exhausted command receiver"))
    }
}

pub fn run_loop(app: &App, input_device: &Device) -> Result<(), anyhow::Error> {
    let mut stdout = std::io::stdout();

    let model = app.model.clone();
    let (whisper_input, recrecv) = unbounded();
    let (whisper_output, hnd) = whisper::transcription_worker(&model, recrecv)?;

    let (cmd_recv, res_snd, cmds) = receive_instructions(&app.socket_path)?;
    let mut commands = CmdStream::new(cmd_recv);

    #[allow(unused_assignments)]
    let mut exit_code = 0_u8;

    loop {
        let p = sync::ProcessNode::new(|it| it.collect::<Vec<_>>());
        let rec: Recording<_, Vec<i16>> = controlled_recording(input_device, p);

        commands.wait_for(|x| x == &Command::Start)?;
        res_snd.send(serde_json::to_value(Response::Ack).expect("Failed to serialize response"))?;
        log::debug!("Successfully sent ACK");

        rec.start();
        log::debug!("made it out of rec.start()");

        commands.wait_for(|x| x == &Command::Stop)?;

        let audio = rec.stop()?;
        let now = std::time::Instant::now();
        whisper_input.send(audio)?;
        let Some(transcription) = whisper_output.iter().next() else {
            log::warn!("No transcription");
            exit_code = 1;
            break;
        };
        let transcription = match transcription {
            Ok(t) => t,
            Err(e) => {
                log::error!("{e}");
                continue;
            }
        };

        let transcription = transcription
            .into_iter()
            .join_continuations()
            .sentences()
            .filter(|s| !s.content().starts_with('['))
            .collect::<Option<Timing>>();

        if transcription.is_some() {
            log::info!("Took {:?} to transcribe", now.elapsed(),);
        } else {
            log::info!("No transcription");
        }

        res_snd.send(
            serde_json::to_value(Response::Transcription(
                transcription.map(|t| t.content().to_string()),
            ))
            .expect("Failed to serialize response"),
        )?;
    }
    res_snd.send(
        serde_json::to_value(Response::Exit(exit_code)).expect("Failed to serialize response"),
    )?;
    // Done responding
    drop(res_snd);

    stdout.flush()?;

    hnd.join().unwrap()?;
    cmds.join().unwrap()?;
    Ok(())
}
