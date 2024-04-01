mod audio;
mod sync;

use std::sync::{Arc, Condvar, Mutex};

use anyhow::anyhow;

use cpal::traits::{DeviceTrait, HostTrait};
use whisper_rs::{FullParams, WhisperContext, WhisperError};

use crate::audio::input::{controlled_recording, Recording};

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
        .find(|x| x.name().map(|x| x.contains("Microphone")).unwrap_or(false))
        .ok_or(anyhow!("no input device available"))?;

    Ok(device)
}

fn main() -> Result<(), anyhow::Error> {
    let device = device_matching_name()?;

    println!("{:?}", device.name()?);

    let p = sync::ProcessNode::new(|it| it.collect::<Vec<_>>());

    let audio = {
        let rec: Recording<_, Vec<_>> = controlled_recording(&device, p);

        let mut input = String::new();
        std::io::stdin().read_line(&mut input).unwrap();
        rec.start();

        std::io::stdin().read_line(&mut input).unwrap();
        rec.stop()?
    };

    println!("{:?}", audio.len());
    let segments = transcribe_audio(audio)?;

    println!("{:?}", segments);

    Ok(())
}

#[derive(Debug)]
struct Segment {
    start: usize,
    end: usize,
    text: String,
}

fn transcribe_audio<T>(data: T) -> Result<Vec<Segment>, anyhow::Error>
where
    T: AsRef<[f32]>,
{
    let ctx = whisper_context()?;
    let mut state = ctx.create_state()?;

    match state.full(
        FullParams::new(whisper_rs::SamplingStrategy::Greedy { best_of: 1 }),
        data.as_ref(),
    ) {
        Ok(0) => {}
        Ok(n) => return Err(anyhow!("whisper exited with non-zero status: {n}")),
        Err(e) => {
            return Err(e.into());
        }
    };

    let segments = state.full_n_segments()?;

    (0..segments)
        .map(|n| {
            let text = state.full_get_segment_text(n)?;
            let start = state.full_get_segment_t0(n)?;
            let end = state.full_get_segment_t1(n)?;

            Ok(Segment {
                start: start.try_into()?,
                end: end.try_into()?,
                text,
            })
        })
        .collect()
}

fn whisper_context() -> Result<WhisperContext, WhisperError> {
    let mut params = whisper_rs::WhisperContextParameters::default();
    params.use_gpu(true);
    whisper_rs::WhisperContext::new_with_params(
        "/Users/matt/installations/whisper.cpp/models/ggml-small.en.bin",
        params,
    )
}
