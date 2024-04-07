use std::{io::Write, str::FromStr};

use cpal::Device;
use regex::Regex;
use sttx::{IteratorExt, Timing};

use crate::{
    audio::input::{controlled_recording, Recording},
    socket::receive_instructions,
    sync, whisper, App,
};

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Command {
    Start,
    Stop, // need timestamp?
    Quit,
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

pub enum Response {
    Ack,
    Exit(u8),
    Transcription(String),
}

impl From<Timing> for Response {
    fn from(t: Timing) -> Self {
        Self::Transcription(t.content().to_string())
    }
}

impl std::fmt::Display for Response {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::Ack => write!(f, "ACK"),
            Self::Exit(code) => write!(f, "EXIT {}", code),
            Self::Transcription(s) => write!(f, "TX {}", s),
        }
    }
}

pub fn run_loop(app: &App, input_device: &Device) -> Result<(), anyhow::Error> {
    let mut stdout = std::io::stdout();
    let mut last: Option<Command> = None;

    let model = app.model.clone();
    let (whisper_input, recrecv) = std::sync::mpsc::channel();
    let (whisper_output, hnd) = whisper::transcription_worker(&model, recrecv)?;

    let (cmd_recv, res_snd, cmds) = receive_instructions(&app.socket_path)?;
    let mut commands = cmd_recv.iter().flat_map(|s| match s.parse::<Command>() {
        Ok(c) => Some(c),
        Err(e) => {
            eprintln!("Error: {}", e);
            None
        }
    });

    let mut exit_code = 0_u8;
    while last != Some(Command::Quit) {
        let p = sync::ProcessNode::new(|it| it.collect::<Vec<_>>());
        let rec: Recording<_, Vec<i16>> = controlled_recording(input_device, p);

        if last != Some(Command::Start) {
            commands.find(|x| x == &Command::Start);
            res_snd.send(Response::Ack.to_string())?;
        }

        rec.start();

        if let Some(dur) = app.duration_in_secs {
            std::thread::sleep(std::time::Duration::from_secs(dur as u64));
        } else {
            commands.find(|x| x == &Command::Stop);
            res_snd.send(Response::Ack.to_string())?;
        }

        let audio = rec.stop()?;
        let now = std::time::Instant::now();
        whisper_input.send(audio)?;
        let Some(transcription) = whisper_output.iter().next() else {
            eprintln!("No transcription");
            exit_code = 1;
            break;
        };
        let transcription = match transcription {
            Ok(t) => t,
            Err(e) => {
                eprintln!("Error: {}", e);
                continue;
            }
        };

        if let Some(transcription) = transcription
            .into_iter()
            .join_continuations()
            .sentences()
            .filter(|s| !s.content().starts_with('['))
            .collect::<Option<Timing>>()
        {
            eprintln!(
                "Took {:?} to transcribe: {:?}",
                now.elapsed(),
                transcription
            );

            res_snd.send(Response::from(transcription).to_string())?;
        } else {
            eprintln!("No transcription");
            res_snd.send(Response::Ack.to_string())?;
        }

        last = commands.next();
    }
    res_snd.send(Response::Exit(exit_code).to_string())?;
    // Done responding
    drop(res_snd);

    stdout.flush()?;

    hnd.join().unwrap()?;
    cmds.join().unwrap()?;
    Ok(())
}
