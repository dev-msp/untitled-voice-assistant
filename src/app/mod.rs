mod command;
mod response;
mod state;

use std::io::Write;

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
