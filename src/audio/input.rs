use std::{
    sync::mpsc::{self, Sender},
    thread,
};

use anyhow::anyhow;
use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    Sample, SampleFormat,
};

use crate::Notifier;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RecordState {
    Recording,

    #[default]
    Stopped,
}

#[derive(Debug, Clone)]
pub struct Controller {
    notifier: Notifier<RecordState>,
}

impl Controller {
    pub fn new() -> Self {
        Self {
            notifier: Notifier::new(),
        }
    }

    pub fn start(&self) {
        self.notifier.notify(RecordState::Recording);
    }

    pub fn stop(&self) {
        self.notifier.notify(RecordState::Stopped);
    }

    pub fn wait_for(&self, state: RecordState) {
        self.notifier.wait_until(state);
    }
}

pub trait MySample: Send + hound::Sample + Sample + 'static {}
impl<S> MySample for S where S: Send + hound::Sample + Sample + 'static {}

pub struct Recording<S>
where
    S: MySample,
{
    handle: thread::JoinHandle<Result<(), String>>,
    controller: Controller,
    phantom: std::marker::PhantomData<S>,
}

impl<S> Recording<S>
where
    S: MySample,
{
    pub fn start(&self) {
        self.controller.start();
        println!("Recording started");
    }

    pub fn stop(self) -> Result<(), String> {
        self.controller.stop();
        println!("Recording stopped");
        self.handle.join().unwrap()
    }
}

pub fn controlled_recording<S>(device: &cpal::Device, snd: Sender<S>) -> Recording<S>
where
    S: MySample,
{
    let controller = Controller::new();

    let c2 = controller.clone();
    let device_name = device.name().unwrap().to_string();

    let handle = thread::spawn(move || {
        record_from_input_device::<S>(&cpal::default_host(), device_name, snd, c2)
            .map_err(|e| e.to_string())
    });

    Recording {
        handle,
        controller,
        phantom: std::marker::PhantomData,
    }
}

pub fn record_from_input_device<S>(
    host: &cpal::Host,
    device_name: String,
    chan: Sender<S>,
    controller: Controller,
) -> Result<(), anyhow::Error>
where
    S: Send + hound::Sample + Sample + 'static,
{
    let device = host
        .input_devices()?
        .find(|x| x.name().map(|x| x.contains(&device_name)).unwrap_or(false))
        .ok_or(anyhow!("no input device available"))?;
    let config = device.default_input_config()?;

    controller.wait_for(RecordState::Recording);

    {
        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => device.build_input_stream(
                &config.into(),
                move |data, info| {
                    write_input_data::<f32, S>(data, chan.clone()).expect("failed to write data")
                },
                move |err| eprintln!("an error occurred on stream: {}", err),
            )?,
            _ => panic!("unsupported sample format"),
        };
        stream.play()?;

        controller.wait_for(RecordState::Stopped);
    }
    Ok(())
}

pub fn wav_spec_from_config(config: &cpal::SupportedStreamConfig) -> hound::WavSpec {
    hound::WavSpec {
        channels: config.channels() as _,
        sample_rate: config.sample_rate().0,
        bits_per_sample: (config.sample_format().sample_size() * 8) as _,
        sample_format: if config.sample_format() == SampleFormat::F32 {
            hound::SampleFormat::Float
        } else {
            hound::SampleFormat::Int
        },
    }
}

fn write_input_data<T, U>(input: &[T], chan: Sender<U>) -> Result<(), mpsc::SendError<U>>
where
    T: Sample,
    U: Sample + hound::Sample,
{
    for &sample in input.iter() {
        let sample: U = U::from(&sample);
        let _ = chan.send(sample);
    }

    Ok(())
}
