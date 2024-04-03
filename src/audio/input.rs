use std::{
    fmt::Display,
    sync::mpsc::{self, Sender},
    thread,
};

use anyhow::anyhow;
use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    Sample, SampleFormat,
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

pub struct Recording<S, RS, RE = anyhow::Error>
where
    S: MySample,
    RS: Send,
    RE: Send,
{
    handle: thread::JoinHandle<Result<(), RE>>,
    controller: Controller,
    phantom: std::marker::PhantomData<S>,
    receiving_handle: thread::JoinHandle<RS>,
}

#[derive(Debug, thiserror::Error)]
pub enum RecordingError<E>
where
    E: Display + Send + Sync + 'static,
{
    #[error("failed to join recording thread")]
    Sync,
    #[error("{0}")]
    Other(E),
}

impl<S, RS, RE> Recording<S, RS, RE>
where
    S: MySample,
    RS: Send,
    RE: Send + Sync + Display,
{
    pub fn start(&self) {
        self.controller.start();
        println!("Recording started");
    }

    pub fn stop(self) -> Result<RS, RecordingError<RE>> {
        self.controller.stop();
        println!("Recording stopped");
        let _ = Self::join_handle(self.handle)?;

        Self::join_handle(self.receiving_handle)
    }

    fn join_handle<T, E>(handle: thread::JoinHandle<T>) -> Result<T, RecordingError<E>>
    where
        E: Display + Send + Sync + 'static,
    {
        match handle.join() {
            Ok(inner) => Ok(inner),
            Err(e) => {
                let inner: Box<E> = e.downcast::<E>().map_err(|_| RecordingError::Sync)?;
                Err(RecordingError::Other(*inner))
            }
        }
    }
}

pub fn controlled_recording<S, RS>(
    device: &cpal::Device,
    node: crate::sync::ProcessNode<S, RS>,
) -> Recording<S, RS>
where
    S: MySample,
    RS: Send + 'static,
{
    let controller = Controller::new();

    let c2 = controller.clone();
    let device_name = device.name().unwrap().to_string();

    let (sink_send, sink_handle) = node.run();

    let handle = thread::spawn(move || {
        record_from_input_device::<S>(&cpal::default_host(), device_name, sink_send, c2)
    });

    Recording {
        handle,
        controller,
        phantom: std::marker::PhantomData,
        receiving_handle: sink_handle,
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
                move |data, _| {
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
