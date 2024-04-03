mod audio;
mod sync;
mod whisper;

use anyhow::anyhow;

use cpal::traits::{DeviceTrait, HostTrait};
use whisper_rs::install_whisper_log_trampoline;

use crate::{
    audio::input::{controlled_recording, Recording},
    sync::ProcessNode,
    whisper::{Segment, Whisper},
};

fn device_matching_name(name: &str) -> Result<cpal::Device, anyhow::Error> {
    let host = cpal::default_host();

    let device = host
        .input_devices()?
        .find(|d| d.name().map(|n| n.contains(name)).unwrap_or(false))
        .ok_or(anyhow!("no input device available"))?;

    Ok(device)
}

fn main() -> Result<(), anyhow::Error> {
    install_whisper_log_trampoline();

    let device = device_matching_name("Buds")?;
    println!("{:?}", device.name()?);

    let p = sync::ProcessNode::new(|it| it.collect::<Vec<_>>());

    let wh: ProcessNode<Vec<f32>, Result<Vec<Segment>, anyhow::Error>> =
        sync::ProcessNode::new(|mut it| {
            let whisper =
                Whisper::new("/Users/matt/installations/whisper.cpp/models/ggml-base.en.bin")?;

            let segments = it
                .next()
                .ok_or(anyhow!("no audio"))
                .and_then(|audio| Ok(whisper.transcribe_audio(audio)?))?;

            Ok(segments)
        });

    let (snd, hnd) = wh.run();

    {
        let rec: Recording<_, Vec<_>> = controlled_recording(&device, p);

        let mut input = String::new();
        std::io::stdin().read_line(&mut input).unwrap();
        rec.start();

        std::io::stdin().read_line(&mut input).unwrap();
        let audio = rec.stop()?;
        snd.send(audio)?;
    }

    let result = hnd.join().unwrap()?;

    println!("{:?}", result);

    Ok(())
}
