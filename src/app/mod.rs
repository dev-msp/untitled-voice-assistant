pub mod command;
mod response;
mod state;

use anyhow::anyhow;
use cpal::Device;
use crossbeam::channel::unbounded;
use itertools::Itertools;
use regex::Regex;
use sttx::{IteratorExt, Timing};

use crate::audio::input::{controlled_recording, Recording};
use crate::sync;
use crate::{socket::receive_instructions, whisper, App};

use self::command::{CmdStream, Command};
use self::response::Response;
use self::state::{Chat, Mode};

pub struct Daemon {
    app: App,
    input_device: Option<Device>,
    state: state::State,
}

#[derive(Debug, Clone)]
struct Transcription(Vec<sttx::Timing>);

impl Transcription {
    fn into_iter(self) -> impl Iterator<Item = sttx::Timing> {
        self.0.into_iter()
    }

    fn process(self) -> Option<sttx::Timing> {
        self.into_iter()
            .join_continuations()
            .sentences()
            .filter(|s| !s.content().starts_with('['))
            .collect()
    }
}

fn filter_words(
    content: &str,
    word_re: Option<regex::Regex>,
) -> impl Iterator<Item = String> + Clone + '_ {
    content
        .split_whitespace()
        .map(|w| w.chars().filter(|c| c.is_alphabetic()).collect::<String>())
        .filter(move |w| {
            if let Some(ref re) = word_re {
                re.is_match(w)
            } else {
                true
            }
        })
}

impl TryFrom<&Timing> for Command {
    type Error = ();

    fn try_from(t: &Timing) -> Result<Self, Self::Error> {
        let word_re = Regex::new(r"^(set|mode|to|chat|live|clipboard)$").unwrap();
        let content = t.content().to_ascii_lowercase();
        {
            let words = filter_words(&content, None);
            if let Ok(cmd) = handle_reset(words) {
                return Ok(cmd);
            };
        }

        let mut words = filter_words(&content, Some(word_re));

        let prefix = words.clone().take(3).join(" ");
        log::info!("Prefix: {}", prefix);

        let mode = if prefix == "set mode to" {
            words.nth(3)
        } else {
            let md = words.next();
            let wd = words.next();
            match (md, wd.as_deref()) {
                (Some(word), Some("mode")) => Some(word),
                _ => None,
            }
        };

        let Some(mode) = mode else {
            return Err(());
        };

        log::info!("Mode: {}", mode);

        match mode.as_str() {
            "chat" => Ok(Command::Mode(Mode::Chat(Chat::StartNew(
                "You are a terse assistant with minimal affect.".into(),
            )))),
            "live" => Ok(Command::Mode(Mode::LiveTyping)),
            "clipboard" => Ok(Command::Mode(Mode::Clipboard)),
            _ => Err(()),
        }
    }
}

fn handle_reset<T: Iterator<Item = String>>(mut words: T) -> Result<Command, ()> {
    log::info!("Handling reset");
    let temp = words.next();
    let temp2 = words.next();
    let x = (temp.as_deref(), temp2.as_deref());
    log::info!("Reset words: {:?}", x);
    match x {
        (Some("reset"), Some("yourself")) => Ok(Command::Reset),
        _ => Err(()),
    }
}

impl Daemon {
    pub fn new(app: App, input_device: Option<Device>) -> Self {
        Self {
            app,
            input_device,
            state: state::State::default(),
        }
    }

    /// Runs the main application loop.
    ///
    pub fn run_loop(&mut self) -> Result<bool, anyhow::Error> {
        let model = self.app.model.clone();
        let device = self
            .input_device
            .as_ref()
            .ok_or_else(|| anyhow!("No input device"))?;

        let (to_whisper, from_recordings) = unbounded();
        let (whisper_output, tx_worker) = whisper::transcription_worker(&model, from_recordings)?;

        let ((rcmds, scmds), resps, listener) = receive_instructions(&self.app.socket_path)?;
        let mut commands = CmdStream::new(rcmds);

        #[allow(unused_assignments)]
        let mut exit_code = 0_u8;

        let mut rec: Option<Recording<_, Vec<i16>>> = None;

        for result in commands.run_state_machine(&mut self.state) {
            let (ref command, ref new_state) = match result {
                Ok((c, s)) => (c, s),
                Err(e) => {
                    log::error!("{e}");
                    resps.send(
                        serde_json::to_value(Response::Error(e.to_string()))
                            .expect("Failed to serialize response"),
                    )?;
                    continue;
                }
            };
            let Some(new_state) = new_state else {
                resps.send(
                    serde_json::to_value(Response::Nil).expect("Failed to serialize response"),
                )?;
                continue;
            };

            // This handles the state condition where rec must exist.
            if new_state.running() && rec.is_none() {
                rec = Some(controlled_recording(
                    device,
                    sync::ProcessNode::new(|it| it.collect::<Vec<_>>()),
                ));
            }

            match command {
                Command::Start => {
                    assert!(rec.is_some());
                    assert!(new_state.running());

                    rec.as_mut().unwrap().start();
                    resps.send(
                        serde_json::to_value(Response::Ack).expect("Failed to serialize response"),
                    )?;
                    log::debug!("Successfully sent ACK");
                }

                Command::Stop => {
                    assert!(rec.is_some());
                    assert!(!new_state.running());

                    let audio = rec.take().unwrap().stop()?;
                    to_whisper.send(audio)?;
                    let now = std::time::Instant::now();

                    let transcription = whisper_output
                        .iter()
                        .next()
                        .ok_or(anyhow!("No transcription"))?;

                    match transcription {
                        Ok(t) => {
                            let t = Transcription(t).process();

                            if t.is_some() {
                                log::info!("Took {:?} to transcribe", now.elapsed(),);
                            } else {
                                log::info!("No transcription");
                            }

                            if let Some(cmd) = t.clone().and_then(|t| Command::try_from(&t).ok()) {
                                scmds.send(
                                    serde_json::to_value(cmd).expect("Failed to serialize command"),
                                )?;
                                resps.send(
                                    serde_json::to_value(Response::Ack)
                                        .expect("Failed to serialize response"),
                                )?;
                            } else {
                                resps.send(
                                    serde_json::to_value(Response::Transcription {
                                        content: t.map(|t| t.content().to_string()),
                                        mode: new_state.mode(),
                                    })
                                    .expect("Failed to serialize response"),
                                )?;
                            }
                        }
                        Err(e) => {
                            log::error!("{e}");
                            resps.send(
                                serde_json::to_value(Response::Transcription {
                                    content: None,
                                    mode: new_state.mode(),
                                })
                                .expect("Failed to serialize response"),
                            )?;
                            exit_code = 1;
                        }
                    }
                }
                Command::Mode(_) => {
                    assert!(!new_state.running());

                    resps.send(
                        serde_json::to_value(Response::Ack).expect("Failed to serialize response"),
                    )?;
                Command::Reset => {
                    log::info!("Resetting");
                    return Ok(true);
                }
                c @ Command::Mode(_) => {
                    assert!(!new_state.running());
                    resps.send(c.as_response().unwrap_or(Response::Ack).as_json())?;
                }
            }
        }
        resps.send(
            serde_json::to_value(Response::Exit(exit_code)).expect("Failed to serialize response"),
        )?;
        // Done responding
        drop(resps);

        tx_worker.join().unwrap()?;
        listener.join().unwrap()?;

        // remove socket
        std::fs::remove_file(&self.app.socket_path)?;
        Ok(false)
    }
}
