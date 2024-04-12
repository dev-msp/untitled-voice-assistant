pub mod command;
mod input;
mod response;
mod state;

use anyhow::anyhow;
use cpal::Device;
use crossbeam::channel::unbounded;
use itertools::Itertools;
use regex::Regex;
use sttx::{IteratorExt, Timing};

use crate::app::input::iter::alpha_only;
use crate::audio::input::{controlled_recording, Recording};
use crate::sync;
use crate::{socket::receive_instructions, whisper, App};

use self::command::{CmdStream, Command};
use self::input::iter;
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

impl TryFrom<&Timing> for Command {
    type Error = ();

    fn try_from(t: &Timing) -> Result<Self, Self::Error> {
        let content = t.content().to_ascii_lowercase();
        {
            if let Some(cmd) = handle_reset(&content) {
                return Ok(cmd);
            };

            if let Some(cmd) = handle_hey_robot(&content) {
                return Ok(cmd);
            }
        }

        let word_re = Regex::new(r"^(set|mode|to|chat|live|clipboard)$").unwrap();
        let it = iter::Iter::from(content.to_string());
        let mut words = it
            .words()
            .map(alpha_only)
            .filter(|bos| word_re.is_match(bos));

        let prefix = words.clone().map(|bos| bos.to_string()).take(3).join(" ");
        log::info!("Prefix: {}", prefix);

        let mode = if prefix == "set mode to" {
            words.nth(3)
        } else {
            let md = words.next();
            let wd = words.next();
            match (md, wd) {
                (Some(word), Some(wd)) if wd.as_ref() == "mode" => Some(word),
                _ => None,
            }
        };

        let Some(mode) = mode else {
            return Err(());
        };

        log::info!("Mode: {}", mode);

        match mode.as_ref() {
            "chat" => Ok(Command::Mode(Mode::Chat(Chat::StartNew(
                "You are a terse assistant with minimal affect.".into(),
            )))),
            "live" => Ok(Command::Mode(Mode::LiveTyping)),
            "clipboard" => Ok(Command::Mode(Mode::Clipboard {
                use_clipboard: content.contains("use the clipboard"),
                use_llm: false,
            })),
            _ => Err(()),
        }
    }
}

fn handle_reset(content: &str) -> Option<Command> {
    let it = iter::Iter::from(content.to_string());
    let (fst, snd) = it.words().tuple_windows().next()?;
    match (fst.as_ref(), snd.as_ref()) {
        ("reset", "yourself") => Some(Command::Reset),
        _ => None,
    }
}

fn handle_hey_robot(content: &str) -> Option<Command> {
    let it = iter::Iter::from(content.to_string());
    let words = it.words().map(alpha_only);

    log::debug!("Words: {:?}", it.words().collect::<Vec<_>>());
    let (fst, snd, fol) = words.tuple_windows().next()?;
    log::debug!("Fst: {:?}, Snd: {:?}", fst, snd);
    match (fst.as_ref(), snd.as_ref()) {
        ("hey", "robot") => {
            let use_clipboard = content.contains("use the clipboard");
            // SAFETY: From the implementation of ByteOffsetString, the segment offset is known to
            // be within the bounds of the content string.
            let slice = &content[fol.segment_offset()..];
            let content = if use_clipboard {
                slice.replace("clipboard", "content provided")
            } else {
                slice.to_string()
            };
            Some(Command::Respond(Response::Transcription {
                content: Some(content),
                mode: Mode::Clipboard {
                    use_clipboard,
                    use_llm: true,
                },
            }))
        }
        _ => None,
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
                    resps.send(Response::Error(e.to_string()).as_json())?;
                    continue;
                }
            };
            let Some(new_state) = new_state else {
                resps.send(Response::Nil.as_json())?;
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
                    resps.send(Response::Ack.as_json())?;
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
                                scmds.send(cmd.as_json())?;
                                log::info!("Sent command: {}", cmd.as_json().to_string());
                                resps.send(cmd.as_response().unwrap_or(Response::Ack).as_json())?;
                            } else {
                                resps.send(
                                    Response::Transcription {
                                        content: t.map(|t| t.content().to_string()),
                                        mode: new_state.mode(),
                                    }
                                    .as_json(),
                                )?;
                            }
                        }
                        Err(e) => {
                            log::error!("{e}");
                            resps.send(
                                Response::Transcription {
                                    content: None,
                                    mode: new_state.mode(),
                                }
                                .as_json(),
                            )?;
                            exit_code = 1;
                        }
                    }
                }
                Command::Reset => {
                    log::info!("Resetting");
                    return Ok(true);
                }
                c @ Command::Mode(_) => {
                    assert!(!new_state.running());
                    resps.send(c.as_response().unwrap_or(Response::Ack).as_json())?;
                }
                Command::Respond(response) => {
                    log::info!("Responding with: {:?}", response);
                    log::info!("Actually responding with: {:?}", response.as_json());
                    resps.send(response.as_json())?;
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
