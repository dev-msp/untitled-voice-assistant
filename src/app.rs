use std::{
    io::Write,
    sync::mpsc::{Receiver, Sender},
};

use cpal::Device;
use sttx::{IteratorExt, Timing};

use crate::{
    audio::input::{controlled_recording, Recording},
    sync,
    whisper::TranscriptionError,
    App,
};

pub fn run_loop(
    app: &App,
    input_device: &Device,
    commands: Receiver<String>,
    whisper_input: Sender<Vec<i16>>,
    whisper_output: Receiver<Result<Vec<sttx::Timing>, TranscriptionError>>,
) -> Result<(), anyhow::Error> {
    let mut stdout = std::io::stdout();
    let mut last: Option<String> = None;

    while last != Some(String::from("quit")) {
        let p = sync::ProcessNode::new(|it| it.collect::<Vec<_>>());
        let rec: Recording<_, Vec<i16>> = controlled_recording(input_device, p);

        if last != Some(String::from("start")) {
            commands.iter().find(|x| x == "start");
        }

        rec.start();

        if let Some(dur) = app.duration_in_secs {
            std::thread::sleep(std::time::Duration::from_secs(dur as u64));
        } else {
            commands.iter().find(|x| x == "stop");
        }

        let audio = rec.stop()?;
        let now = std::time::Instant::now();
        whisper_input.send(audio)?;
        let Some(transcription) = whisper_output.iter().next() else {
            eprintln!("No transcription");
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

            writeln!(&mut stdout, "{}", transcription.content())?;
            stdout.flush()?;
        }

        last = commands.iter().next();

        // Process WAV data
        // {
        //     let path_to_file = args.next().ok_or(anyhow!("no file"))?;
        //     let f = std::fs::File::open(path_to_file)?;
        //     let bf = BufReader::new(f);
        //     let audio = WavReader::new(bf)?;
        //     let audio = audio
        //         .into_samples::<i16>()
        //         .map(|s| s.unwrap())
        //         .collect::<Vec<_>>();
        //     let mut audio_fl = vec![0_f32; audio.len()];
        //     convert_integer_to_float_audio(&audio, &mut audio_fl)?;
        //     snd.send(audio_fl)?;
        // }
    }
    stdout.flush()?;
    Ok(())
}
