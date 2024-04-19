use std::{
    fmt::{Debug, Display},
    ops::Deref,
    sync::{Arc, Condvar, Mutex},
    thread,
};

use anyhow::anyhow;
use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    SampleRate, StreamConfig,
};
use crossbeam::channel::Sender;

use crate::app::state::RecordingSession;

#[derive(Debug, Clone)]
pub struct Notifier<T: Clone>(Arc<(Mutex<T>, Condvar)>);

impl<T: Debug + Clone + PartialEq> Notifier<T> {
    pub fn notify(&self, value: T) {
        let (lock, cvar) = &*self.0;
        log::trace!("Trying to lock (notify)");
        let mut state = lock.lock().unwrap();
        log::trace!("Locked (notify)");
        *state = value;
        cvar.notify_one();
    }

    pub fn wait_until(&self, value: T) {
        let (lock, cvar) = &*self.0;
        log::trace!("Trying to lock (wait_until)");
        let mut state = lock.lock().unwrap();
        log::trace!("Locked (wait_until)");
        log::trace!("Waiting for value: {:?}", value);
        while *state != value {
            state = cvar.wait(state).unwrap();
            log::trace!("got value: {:?}", state.deref());
            if *state != value {
                log::trace!("Got wrong value ({:?}), continuing", state.deref());
            }
        }
    }
}

impl<T: Clone + Default> Notifier<T> {
    pub fn new() -> Self {
        Self(Arc::new((Mutex::new(T::default()), Condvar::new())))
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RecordState {
    #[default]
    Stopped,
    Started,
    Recording,
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
        self.notifier.notify(RecordState::Started);
    }

    pub fn recording(&self) {
        self.notifier.notify(RecordState::Recording);
    }

    pub fn stop(&self) {
        self.notifier.notify(RecordState::Stopped);
    }

    pub fn wait_for(&self, state: RecordState) {
        self.notifier.wait_until(state);
    }
}

pub trait MySample: Send + hound::Sample + cpal::Sample + 'static {}
impl<S> MySample for S where S: Send + hound::Sample + cpal::Sample + 'static {}

pub struct Recording<S, RS, RE = anyhow::Error>
where
    S: MySample,
    RS: Send,
    RE: Send,
{
    handle: thread::JoinHandle<Result<StreamConfig, RE>>,
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
    RS: Send + Into<Vec<f32>>,
    RE: Send + Sync + Display + Debug,
{
    #[tracing::instrument(skip(self))]
    pub fn start(&self) {
        self.controller.start();
        self.controller.wait_for(RecordState::Recording);
        log::info!("Recording started");
    }

    pub fn stop(self) -> Result<(StreamConfig, Vec<f32>), RecordingError<RE>> {
        self.controller.stop();
        let metadata = Self::join_handle(self.handle)
            .map_err(|e| {
                log::error!("Error joining recording thread: {}", e);
                RecordingError::<RE>::Sync
            })
            .unwrap()
            .map_err(|e| {
                log::error!("Error joining recording thread: {}", e);
                RecordingError::Other(e)
            })?;

        let audio = Self::join_handle(self.receiving_handle)?.into();
        Ok((metadata, audio))
    }

    fn join_handle<T>(handle: thread::JoinHandle<T>) -> Result<T, RecordingError<RE>> {
        match handle.join() {
            Ok(inner) => Ok(inner),
            Err(e) => {
                let inner: Box<RE> = e.downcast::<RE>().map_err(|_| RecordingError::Sync)?;
                Err(RecordingError::Other(*inner))
            }
        }
    }
}

pub fn controlled_recording<S, RS>(
    session: RecordingSession,
    node: crate::sync::ProcessNode<S, RS>,
) -> Recording<S, RS>
where
    S: MySample,
    RS: Send + 'static,
{
    let controller = Controller::new();

    let c2 = controller.clone();

    let (sink_send, sink_handle) = node.run();

    let handle = thread::spawn(move || {
        record_from_input_device::<S>(session, sink_send, c2).map_err(|e| {
            log::error!("Error attempting to record from input device: {}", e);
            e
        })
    });

    Recording {
        handle,
        controller,
        phantom: std::marker::PhantomData,
        receiving_handle: sink_handle,
    }
}

#[tracing::instrument(skip_all)]
pub fn record_from_input_device<S>(
    session: RecordingSession,
    chan: Sender<S>,
    controller: Controller,
) -> Result<StreamConfig, anyhow::Error>
where
    S: MySample,
{
    let host = cpal::default_host();
    let device = host
        .input_devices()?
        .find(|x| {
            x.name()
                .map(|x| x.contains(session.device_name()))
                .unwrap_or(false)
        })
        .ok_or(anyhow!("no input device available"))?;

    let supported_config = device
        .supported_input_configs()?
        .map(|c| {
            log::debug!(
                "channels: {}, sample rate: {} - {}",
                c.channels(),
                c.min_sample_rate().0,
                c.max_sample_rate().0
            );
            c
        })
        .find_map(|c| {
            let sample_rate = session
                .sample_rate()
                .map(SampleRate)
                .unwrap_or_else(|| c.min_sample_rate().max(SampleRate(16000)));
            if c.min_sample_rate() > sample_rate || c.max_sample_rate() < sample_rate {
                None
            } else {
                let min_sample_rate = c.min_sample_rate();
                Some(c.with_sample_rate(min_sample_rate))
            }
        })
        .ok_or(anyhow!("no supported input configuration"))?;

    controller.wait_for(RecordState::Started);

    let cpal::SampleFormat::F32 = supported_config.sample_format() else {
        panic!("unsupported sample format");
    };

    let cfg: cpal::StreamConfig = supported_config.clone().into();
    let cfg_inner = cfg.clone();
    let resampler = super::process::Processor::<f32, S>::new(chan.clone(), cfg_inner.clone(), 512)
        .expect("failed to create resampler");

    let resampler = Arc::new(Mutex::new(resampler));
    {
        let resampler_send = resampler.clone();
        let stream = device.build_input_stream(
            &cfg,
            move |data, _| {
                resampler_send
                    .lock()
                    .expect("failed to lock resampler")
                    .write_input_data(data)
                    .expect("failed to write data");
            },
            move |err| log::trace!("an error occurred on stream: {}", err),
        )?;
        stream.play()?;
        controller.recording();

        controller.wait_for(RecordState::Stopped);
    }
    let mut resampler = resampler.lock().expect("failed to lock resampler");
    resampler.flush_to_sink()?;
    Ok(cfg)
}

pub fn list_channels() -> Result<(), anyhow::Error> {
    let host = cpal::default_host();
    for (device_name, config) in host.input_devices()?.flat_map(|d| {
        let name = d.name().expect("failed to get device name");
        d.supported_input_configs()
            .unwrap()
            .map(move |c| (name.clone(), c))
    }) {
        let (buf_floor, buf_ceil) = match config.buffer_size() {
            cpal::SupportedBufferSize::Range { min, max } => (min, max),
            cpal::SupportedBufferSize::Unknown => unimplemented!(),
        };
        println!(
            "{}: sample_rate:{}-{}, sample_format:{:?}, channels:{}, buffer_size: {}-{}",
            device_name,
            config.min_sample_rate().0,
            config.max_sample_rate().0,
            config.sample_format(),
            config.channels(),
            buf_floor,
            buf_ceil
        );
    }
    Ok(())
}
