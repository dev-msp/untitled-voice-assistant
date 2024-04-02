mod audio;
mod sync;
mod whisper;

use std::sync::{Arc, Condvar, Mutex};

use anyhow::anyhow;

use cpal::traits::{DeviceTrait, HostTrait};
use whisper_rs::install_whisper_log_trampoline;

use crate::{
    audio::input::{controlled_recording, Recording},
    sync::ProcessNode,
    whisper::{Segment, Whisper},
};

#[derive(Debug, Clone)]
struct Notifier<T: Clone>(Arc<(Mutex<T>, Condvar)>);

impl<T: Clone + PartialEq> Notifier<T> {
    fn notify(&self, value: T) {
        let (lock, cvar) = &*self.0;
        let mut state = lock.lock().unwrap();
        *state = value;
        cvar.notify_one();
    }

    fn wait_until(&self, value: T) {
        let (lock, cvar) = &*self.0;
        let mut state = lock.lock().unwrap();
        while *state != value {
            state = cvar.wait(state).unwrap();
        }
    }
}

impl<T: Clone + Default> Notifier<T> {
    fn new() -> Self {
        Self(Arc::new((Mutex::new(T::default()), Condvar::new())))
    }
}

fn device_matching_name() -> Result<cpal::Device, anyhow::Error> {
    let host = cpal::default_host();

    let device = host
        .input_devices()?
        .find(|x| x.name().map(|x| x.contains("Buds")).unwrap_or(false))
        .ok_or(anyhow!("no input device available"))?;

    Ok(device)
}

fn main() -> Result<(), anyhow::Error> {
    install_whisper_log_trampoline();

    let device = device_matching_name()?;
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
