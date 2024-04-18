pub mod command;
pub mod response;
pub mod state;

use std::time::SystemTime;

use anyhow::anyhow;
use crossbeam::channel::{unbounded, Receiver, Sender};
use sttx::IteratorExt;

use crate::audio::input::{controlled_recording, Recording};
use crate::whisper::TranscriptionJob;
use crate::{sync, whisper, DaemonInit};

use self::command::{CmdStream, Command};
use self::response::Response;

pub struct Daemon {
    config: DaemonInit,
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

impl Daemon {
    pub fn new(config: DaemonInit) -> Self {
        Self {
            config,
            state: state::State::default(),
        }
    }

    /// Runs the main application loop.
    pub fn run_loop(
        &mut self,
        commands: Receiver<Command>,
        responses: Sender<Response>,
    ) -> Result<bool, anyhow::Error> {
        let model = self.config.model.clone();

        let (to_whisper, from_recordings) = unbounded();
        let (whisper_output, tx_worker) = whisper::transcription_worker(&model, from_recordings)?;

        let mut commands = CmdStream::new(commands);

        #[allow(unused_assignments)]
        let mut exit_code = 0_u8;
        let mut rec: Option<Recording<_, Vec<f32>>> = None;

        for (ref command, ref new_state) in commands.run_state_machine(&mut self.state) {
            let Some(new_state) = new_state else {
                responses.send(Response::Nil)?;
                continue;
            };

            // This handles the state condition where rec must exist.
            if new_state.running() && rec.is_none() {
                let device = new_state.recording_device()?.ok_or(anyhow!("No device"))?;
                rec = Some(controlled_recording(
                    &device,
                    sync::ProcessNode::new(|it| it.collect::<Vec<_>>()),
                ));
            }

            match command {
                Command::Start(_) => {
                    assert!(rec.is_some());
                    assert!(new_state.running());

                    rec.as_mut().unwrap().start();
                    let now = SystemTime::now()
                        .duration_since(SystemTime::UNIX_EPOCH)
                        .unwrap()
                        .as_millis();
                    responses.send(Response::Ack(now))?;
                    log::debug!("Successfully sent ACK");
                }

                Command::Stop => {
                    assert!(rec.is_some());
                    assert!(!new_state.running());

                    let (metadata, audio) = rec.take().unwrap().stop()?;
                    to_whisper.send(TranscriptionJob::new(
                        audio,
                        self.config.strategy(),
                        metadata.sample_rate.0 as i32,
                    ))?;
                    let now = std::time::Instant::now();

                    let transcription = whisper_output
                        .iter()
                        .next()
                        .ok_or(anyhow!("No transcription"))?;

                    match transcription {
                        Ok(t) => {
                            let t = Transcription(t).process();

                            if t.is_some() {
                                log::info!("Transcribed: \"{}\"", t.as_ref().unwrap().content());
                                log::info!("Took {:?} to transcribe", now.elapsed(),);
                            } else {
                                log::info!("No transcription");
                            }

                            responses.send(Response::Transcription {
                                content: t.map(|t| t.content().to_string()),
                                mode: new_state.mode(),
                            })?;
                        }
                        Err(e) => {
                            log::error!("{e}");
                            responses.send(Response::Transcription {
                                content: None,
                                mode: new_state.mode(),
                            })?;
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
                    responses.send(c.as_response().unwrap_or_else(Response::ack))?;
                }
                Command::Respond(response) => {
                    log::info!("Responding with: {:?}", response);
                    responses.send(response.clone())?;
                }
            }
        }

        responses.send(Response::Exit(exit_code))?;
        // Done responding
        drop(responses);

        tx_worker.join().unwrap()?;

        // remove socket
        std::fs::remove_file(&self.config.socket_path)?;
        Ok(false)
    }
}
