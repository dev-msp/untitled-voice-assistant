pub mod command;
pub mod response;
pub mod state;

use std::{path::PathBuf, time::SystemTime};

use crossbeam::channel::{unbounded, Receiver, SendError, Sender};
use sttx::IteratorExt;

use self::{
    command::{CmdStream, Command},
    response::Response,
};
use crate::{
    audio::{Recording, RecordingError, Session},
    sync,
    whisper::{self, transcription::Job},
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Try to eliminate this")]
    CatchAll,

    #[error("Error: {0}")]
    WithMessage(String),

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

impl Error {
    fn message<S: ToString + ?Sized>(msg: &S) -> Self {
        Self::WithMessage(msg.to_string())
    }
}

pub struct Daemon {
    config: DaemonInit,
    state: state::State,
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
            state: state::State::default(),
        }
    }

    /// Runs the main application loop.
    #[allow(clippy::missing_panics_doc)]
    #[allow(clippy::too_many_lines)]
    pub fn run_loop(
        &mut self,
        commands: Receiver<Command>,
        responses: Sender<Response>,
    ) -> Result<bool, Error> {
        let (to_whisper, from_recordings) = unbounded();
        let (whisper_output, tx_worker) =
            whisper::transcription_worker(self.config.model_dir.as_path(), from_recordings)?;

        let mut commands = CmdStream::new(commands);

        let mut exit_code = 0_u8;
        let mut rec: Option<Recording<_, Vec<f32>>> = None;
        for (ref command, ref new_state) in commands.run_state_machine(&mut self.state) {
            let Some(new_state) = new_state else {
                responses.send(Response::Nil)?;
                continue;
            };

            match command {
                Command::Start(session) => {
                    assert!(new_state.running());

                    let new_rec = match Recording::<_, _, Error>::controlled(
                        session.clone(),
                        sync::ProcessNode::new(|it| it.collect::<Vec<_>>()),
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

                Command::Stop => {
                    assert!(rec.is_some());
                    assert!(!new_state.running());

                    let (metadata, audio) = rec.take().unwrap().stop()?;
                    let job = Job::builder()
                        .model(
                            new_state
                                .session()
                                .and_then(Session::model)
                                .unwrap_or_default(),
                        )
                        .strategy(self.config.strategy())
                        .audio(audio)
                        .prompt(new_state.prompt())
                        .sample_rate(metadata.sample_rate.0)
                        .build()
                        .map_err(whisper::Error::from)?;

                    to_whisper.send(job)?;
                    let now = std::time::Instant::now();

                    let transcription = whisper_output
                        .iter()
                        .next()
                        .ok_or(Error::message("No transcription"))?;

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
