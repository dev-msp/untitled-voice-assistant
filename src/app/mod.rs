pub mod command;
pub mod response;
pub mod state;

use std::{path::PathBuf, time::SystemTime};

use crossbeam::channel::{unbounded, Receiver, SendError, Sender};
use sttx::IteratorExt;

use self::{
    command::{CmdStream, Plumbing, TranscriptionParams}, // Import TranscriptionParams
    response::Response,
};
use crate::{
    audio::{self, AudioMessage, Recording, RecordingError, Session},
    sync,
    whisper::{self, transcription::Job},
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("No transcription result")]
    NoTranscriptionResult,

    #[error("Send error")]
    Send,

    #[error("Whisper error: {0}")]
    Whisper(#[from] whisper::Error),

    #[error("Recording error: {0}")]
    Recording(#[from] RecordingError),

    #[error("Socket/IO error: {0}")]
    Socket(#[from] std::io::Error),
}

impl<T> From<SendError<T>> for Error {
    fn from(_: SendError<T>) -> Self {
        Self::Send
    }
}

pub struct Daemon {
    config: DaemonInit,
    state: state::RecordingState,
}

#[derive(Debug, clap::Args)]
pub struct DaemonInit {
    #[clap(short, long, value_parser = whisper::transcription::parse_strategy)]
    strategy: Option<whisper::transcription::StrategyOpt>,

    #[clap(long)]
    model_dir: PathBuf,

    /// Socket path
    #[clap(long)]
    socket_path: Option<String>,
}

impl DaemonInit {
    #[must_use]
    pub fn strategy(&self) -> whisper_rs::SamplingStrategy {
        self.strategy.clone().unwrap_or_default().into()
    }

    #[must_use]
    pub fn socket_path(&self) -> Option<&str> {
        self.socket_path.as_deref()
    }
}

impl Daemon {
    #[must_use]
    pub fn new(config: DaemonInit) -> Self {
        Self {
            config,
            state: state::RecordingState::default(),
        }
    }

    /// Runs the main application loop.
    #[allow(clippy::missing_panics_doc)]
    #[allow(clippy::too_many_lines)] // Will increase with new command
    pub fn run_loop(
        &mut self,
        commands: Receiver<Plumbing>,
        responses: Sender<Response>,
    ) -> Result<bool, Error> {
        let (to_whisper, from_recordings) = unbounded();
        let (whisper_output, tx_worker) =
            whisper::transcription_worker(self.config.model_dir.as_path(), from_recordings)?;

        let mut commands = CmdStream::new(commands);

        let exit_code = 0_u8;
        let mut rec: Option<Recording<_, Vec<f32>>> = None;
        for (ref command, ref new_state_opt) in commands.run_state_machine(&mut self.state) {
            // Now new_state_opt might be Some(initial_state) for commands that don't change state
            // but are processed.
            // We still only want to send a Response if a state transition happened *or* if the
            // command itself implies a response (like Transcribe, Respond).
            //
            // Let's adjust the logic to always process the command if run_state_machine yields it.

            let Some(new_state) = new_state_opt else {
                // This case should now only happen if run_state_machine yielded None,
                // which indicates a command that didn't transition and shouldn't be processed further.
                responses.send(Response::Nil)?;
                continue;
            };

            match command {
                Plumbing::Start(session) => {
                    assert!(new_state.running()); // Assert based on the state *after* the potential transition

                    let new_rec = match Recording::<_, _, audio::RecordingError>::controlled(
                        session.clone(),
                        sync::ProcessNode::new(|it| {
                            it.map(|msg| match msg {
                                AudioMessage::Data(data) => data,
                                AudioMessage::Error(e) => panic!("{e}"),
                            })
                            .collect::<Vec<_>>()
                        }),
                    ) {
                        Ok(new_rec) => new_rec,
                        Err(e) => {
                            responses.send(Response::Error(e.to_string()))?;
                            continue;
                        }
                    };

                    rec = Some(new_rec);

                    rec.as_mut().unwrap().start();
                    let now = SystemTime::now()
                        .duration_since(SystemTime::UNIX_EPOCH)
                        .unwrap()
                        .as_millis();
                    responses.send(Response::Ack(now))?;
                    log::debug!("Successfully sent ACK");
                }

                Plumbing::Stop => {
                    // Assert based on the state *after* the potential transition
                    // The state should now be Stopped if successful
                    assert!(!new_state.running());
                    // The previous state must have been Started for stop() to return true
                    assert!(rec.is_some());

                    let (metadata, audio) = rec.take().unwrap().stop()?;
                    let job = Job::builder()
                        .model(
                            new_state // Use the state *after* transition (now Stopped)
                                .session() // Should get the session from the Stopped state
                                .and_then(Session::model)
                                .unwrap_or_default(),
                        )
                        .strategy(self.config.strategy())
                        .audio(audio)
                        .prompt(new_state.prompt()) // Prompt from the Stopped state session
                        .sample_rate(metadata.sample_rate.0)
                        .build()
                        .map_err(whisper::Error::from)?;

                    to_whisper.send(job)?;
                    let now = std::time::Instant::now();

                    let transcription = whisper_output
                        .iter()
                        .next()
                        .ok_or(Error::NoTranscriptionResult)?;

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
                                mode: new_state.mode(), // Mode from the state *after* transition
                            })?;
                        }
                        Err(e) => {
                            log::error!("{e}");
                            responses.send(Response::Transcription {
                                content: None,
                                mode: new_state.mode(),
                            })?;
                            // Decide if a transcription error should set exit_code
                            // exit_code = 1;
                        }
                    }
                }
                Plumbing::Reset => {
                    log::info!("Resetting");
                    return Ok(true); // Exit loop to potentially reinitialize
                }
                Plumbing::Mode(_) => {
                    // This command only transitions if not running, handled by run_state_machine
                    // The response is already handled by as_response if needed, but here we send Ack
                    responses.send(Response::ack())?;
                }
                Plumbing::Respond(response) => {
                    log::info!("Responding with: {:?}", response);
                    responses.send(response.clone())?;
                }
                Plumbing::Transcribe { audio_data, params } => {
                    // This command does not change the state, but triggers work
                    log::info!("Received transcribe command");
                    let now = std::time::Instant::now();

                    // Build Job from provided audio_data and params
                    let job = Job::builder()
                        .model(params.model.unwrap_or_default()) // Use model from params, default if None
                        // Strategy comes from daemon config, not request params currently
                        .strategy(self.config.strategy())
                        .audio(audio_data.clone()) // Clone data for the job
                        .prompt(params.prompt.clone()) // Prompt from params
                        .sample_rate(params.sample_rate.unwrap_or(16000)) // Use sample rate from params, default if None
                        .build()
                        .map_err(whisper::Error::from)?;

                    to_whisper.send(job)?;

                    // Wait for transcription result
                    let transcription = whisper_output
                        .iter()
                        .next()
                        .ok_or(Error::NoTranscriptionResult)?;

                    match transcription {
                        Ok(t) => {
                            let t = Transcription(t).process();
                            if t.is_some() {
                                log::info!("Transcribed: \"{}\"", t.as_ref().unwrap().content());
                                log::info!("Took {:?} to transcribe", now.elapsed());
                            } else {
                                log::info!("No transcription");
                            }
                            // Send the transcription response back
                            responses.send(Response::Transcription {
                                content: t.map(|t| t.content().to_string()),
                                mode: new_state.mode(), // Use current daemon mode
                            })?;
                        }
                        Err(e) => {
                            log::error!("{e}");
                            responses.send(Response::Transcription {
                                content: None,
                                mode: new_state.mode(),
                            })?;
                            // Decide if a transcription error should set exit_code
                            // exit_code = 1;
                        }
                    }
                }
            }
        }

        responses.send(Response::Exit(exit_code))?;
        // Done responding
        drop(responses);

        if let Err(e) = tx_worker.join() {
            log::error!(
                "Transcription worker thread panicked: {}",
                e.downcast_ref::<String>()
                    .map_or("Unknown panic payload", |v| v)
            );
        } else {
            log::debug!("Transcription worker thread finished");
        }

        // remove socket
        if let Some(ref p) = self.config.socket_path {
            std::fs::remove_file(p)?;
        }
        Ok(false)
    }
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
