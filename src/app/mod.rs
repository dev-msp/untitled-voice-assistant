mod command;
mod response;
mod state;

use std::sync::{Arc, Mutex};

use anyhow::anyhow;
use cpal::Device;
use crossbeam::channel::unbounded;
use sttx::{IteratorExt, Timing};

use crate::{
    audio::input::{controlled_recording, Recording},
    socket::receive_instructions,
    sync, whisper, App,
};

use self::command::{CmdStream, Command};
use self::response::Response;

pub struct Daemon {
    app: App,
    input_device: Option<Device>,
    state: Arc<Mutex<state::State>>,
}

impl Daemon {
    pub fn new(app: App, input_device: Option<Device>) -> Self {
        Self {
            app,
            input_device,
            state: Arc::new(Mutex::new(state::State::new())),
        }
    }

    pub fn run_loop(&self) -> Result<(), anyhow::Error> {
        let model = self.app.model.clone();
        let device = self
            .input_device
            .as_ref()
            .ok_or_else(|| anyhow!("No input device"))?;

        let (to_whisper, from_recordings) = unbounded();
        let (whisper_output, tx_worker) = whisper::transcription_worker(&model, from_recordings)?;

        let (cmds, resps, listener) = receive_instructions(&self.app.socket_path)?;
        let mut commands = CmdStream::new(cmds);

        #[allow(unused_assignments)]
        let mut exit_code = 0_u8;

        loop {
            let p = sync::ProcessNode::new(|it| it.collect::<Vec<_>>());
            let rec: Recording<_, Vec<i16>> = controlled_recording(device, p);

            commands.wait_for(|x| x == &Command::Start)?;
            resps
                .send(serde_json::to_value(Response::Ack).expect("Failed to serialize response"))?;
            log::debug!("Successfully sent ACK");

            rec.start();
            log::debug!("made it out of rec.start()");

            commands.wait_for(|x| x == &Command::Stop)?;

            let audio = rec.stop()?;
            let now = std::time::Instant::now();
            to_whisper.send(audio)?;
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

            resps.send(
                serde_json::to_value(Response::Transcription(
                    transcription.map(|t| t.content().to_string()),
                ))
                .expect("Failed to serialize response"),
            )?;
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
        Ok(())
    }
}
