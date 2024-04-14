use std::{
    fmt::{Debug, Display},
    ops::Deref,
    sync::{Arc, Condvar, Mutex},
    thread,
};

use anyhow::anyhow;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use crossbeam::channel::Sender;
use rubato::{FftFixedOut, Resampler, ResamplerConstructionError};
use whisper_rs::convert_stereo_to_mono_audio;

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
    #[tracing::instrument(skip(self))]
    pub fn start(&self) {
        self.controller.start();
        self.controller.wait_for(RecordState::Recording);
        log::info!("Recording started");
    }

    pub fn stop(self) -> Result<RS, RecordingError<RE>> {
        self.controller.stop();
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
        record_from_input_device::<S>(&cpal::default_host(), device_name, sink_send, c2).map_err(
            |e| {
                log::error!("Error attempting to record from input device: {}", e);
                e
            },
        )
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
    host: &cpal::Host,
    device_name: String,
    chan: Sender<S>,
    controller: Controller,
) -> Result<(), anyhow::Error>
where
    S: MySample,
{
    let device = host
        .input_devices()?
        .find(|x| x.name().map(|x| x.contains(&device_name)).unwrap_or(false))
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
        .map(|c| {
            let min_sample_rate = c.min_sample_rate();
            c.with_sample_rate(min_sample_rate)
        })
        .next()
        .ok_or(anyhow!("no supported input configuration"))?;

    controller.wait_for(RecordState::Started);

    let cpal::SampleFormat::F32 = supported_config.sample_format() else {
        panic!("unsupported sample format");
    };

    let cfg: cpal::StreamConfig = supported_config.clone().into();
    let cfg_inner = cfg.clone();
    let resampler = Processor::<f32, S>::new(chan.clone(), cfg_inner.clone(), 512)
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
    Ok(())
}

struct Processor<S, O>
where
    S: cpal::Sample + rubato::Sample,
{
    config: cpal::StreamConfig,
    resampler: FftFixedOut<S>,
    buffer: Vec<S>,
    sink: Sender<O>,
}

impl<S, O> Processor<S, O>
where
    S: cpal::Sample + rubato::Sample,
    O: MySample,
{
    fn new(
        sink: Sender<O>,
        config: cpal::StreamConfig,
        chunk_size_out: usize,
    ) -> Result<Self, ResamplerConstructionError> {
        let input_rate = config.sample_rate.0 as usize;
        let channels = config.channels as usize;
        log::debug!(
            "Creating resampler with input rate: {}, chunk size: {}, channels: {}",
            input_rate,
            chunk_size_out,
            channels
        );

        // Set to next multiple of 512
        let chunk_size_out = (chunk_size_out + 511) & !511;
        let resampler = FftFixedOut::new(input_rate, 16_000, chunk_size_out, 1, channels)?;

        Ok(Self {
            buffer: Vec::with_capacity(resampler.nbr_channels() * resampler.input_frames_next()),
            config,
            resampler,
            sink,
        })
    }
}

impl<O: MySample> Processor<f32, O> {
    fn write_input_data(&mut self, input: &[f32]) -> Result<(), anyhow::Error> {
        log::trace!(
            "Got input data: {}, remaining in buffer: {}",
            input.len(),
            self.buffer.capacity() - self.buffer.len()
        );
        let remaining = self.consume_input_data(input);
        if self.buffer.len() == self.buffer.capacity() {
            self.flush_to_sink()?;
            self.consume_input_data(remaining);
            log::trace!(
                "Buffer full to its capacity of {}, remaining: {}",
                self.buffer.capacity(),
                remaining.len()
            );
            assert!(self.buffer.len() <= self.buffer.capacity());
            Ok(())
        } else {
            Ok(())
        }
    }

    fn consume_input_data<'a>(&mut self, input: &'a [f32]) -> &'a [f32] {
        let remaining = self.buffer.capacity() - self.buffer.len();
        let cutoff = remaining.min(input.len());
        self.buffer.extend_from_slice(&input[0..cutoff]);

        &input[cutoff..]
    }

    fn input_buffer(&self) -> Vec<f32> {
        let cap = self.resampler.nbr_channels() * self.resampler.input_frames_max();
        log::trace!("Allocating input buffer with capacity: {}", cap);
        Vec::with_capacity(cap)
    }

    fn flush_to_sink(&mut self) -> Result<(), anyhow::Error> {
        if self.buffer.len() < self.buffer.capacity() {
            log::trace!("Buffer not full, returning");
            return Ok(());
        }
        let mut data = self.input_buffer();
        std::mem::swap(&mut self.buffer, &mut data);

        log::trace!("Data length: {}", data.len());

        if self.config.sample_rate != cpal::SampleRate(16_000) {
            let mut output = self.resampler.output_buffer_allocate(true);
            self.resampler
                .process_into_buffer(&[data], &mut output, None)?;
            data = output.first().cloned().expect("no output from resampler");
            log::trace!("Data length after resampling: {}", data.len());
        };

        if self.config.channels as usize != 1 {
            data = convert_stereo_to_mono_audio(&data).expect("failed to convert stereo to mono");
            log::trace!("Data length after stereo to mono: {}", data.len());
        };

        for sample in data {
            let sample: O = O::from(&sample);
            let _ = self.sink.send(sample);
        }
        Ok(())
    }
}
