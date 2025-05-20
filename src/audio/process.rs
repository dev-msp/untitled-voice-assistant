use std::{iter::Copied, slice::Iter};

use cpal::{traits::DeviceTrait, Device, Stream};
use crossbeam::channel::{SendError, Sender};
use dasp::{
    interpolate::sinc::Sinc,
    ring_buffer,
    signal::{interpolate::Converter, FromInterleavedSamplesIterator, FromIterator},
    Signal,
};
use itertools::Itertools;

use super::MySample;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("build stream error: {0}")]
    BuildStream(#[from] cpal::BuildStreamError),

    #[error("Send error")]
    Send,
}

impl<T> From<SendError<T>> for Error {
    fn from(_: SendError<T>) -> Self {
        Self::Send
    }
}

pub trait Process {
    type Frame: dasp::Frame + Default;
    type Input: cpal::Sample + dasp::Sample + Default;
    type Output: MySample;
    type Signal<'a>: Signal<Frame = Self::Frame> + Sized + 'a
    where
        Self: 'a;

    fn config(&self) -> &cpal::StreamConfig;

    fn send(&self, sample: Self::Output) -> Result<(), Error>;
    fn send_error(&self, error: Error) -> Result<(), Error>;

    fn interpolator() -> Sinc<[Self::Frame; 128]>;

    fn signal<'a>(&'a mut self, input: &'a [Self::Input]) -> Self::Signal<'a>;

    fn frame_to_sample(frame: Self::Frame) -> Self::Output;

    fn mono_samples<'a>(
        &'a mut self,
        input: &'a [Self::Input],
    ) -> Box<dyn Iterator<Item = Self::Output> + 'a> {
        Box::new(
            self.signal(input).until_exhausted().map(move |frame| {
                <Self::Output as cpal::Sample>::from(&Self::frame_to_sample(frame))
            }),
        )
    }

    fn write_input_data(&mut self, input: &[Self::Input]) {
        for sample in self.mono_samples(input).collect_vec() {
            self.send(sample)
                .or_else(|e| self.send_error(e))
                .expect("Could not send message to audio thread");
        }
    }
}

pub fn read_from_device<P: Process + Send + Sync + 'static>(
    mut processor: P,
    device: &Device,
) -> Result<Stream, Error> {
    Ok(device.build_input_stream(
        &processor.config().clone(),
        move |data, _| processor.write_input_data(data),
        |err| {
            log::trace!("an error occurred on stream: {}", err);
        },
    )?)
}

pub enum AudioMessage<O>
where
    O: MySample,
{
    Data(O),
    Error(Error),
}

pub struct Processor<O, const CHANNELS: usize>
where
    O: MySample,
{
    config: cpal::StreamConfig,
    sink: Sender<AudioMessage<O>>,
}

impl<O, const CHANNELS: usize> Processor<O, CHANNELS>
where
    O: MySample,
{
    pub fn new(sink: Sender<AudioMessage<O>>, config: cpal::StreamConfig) -> Self {
        Self { config, sink }
    }
}

impl<O: MySample> Process for Processor<O, 1> {
    type Input = f32;
    type Output = O;
    type Frame = Self::Input;

    type Signal<'a> =
        Converter<FromIterator<Copied<Iter<'a, Self::Frame>>>, Sinc<[Self::Frame; 128]>>;

    fn config(&self) -> &cpal::StreamConfig {
        &self.config
    }

    fn interpolator() -> Sinc<[Self::Frame; 128]> {
        Sinc::new(ring_buffer::Fixed::from([Self::Frame::default(); 128]))
    }

    fn signal<'a>(&'a mut self, input: &'a [Self::Input]) -> Self::Signal<'a> {
        let signal = dasp::signal::from_iter(input.iter().copied());

        signal.from_hz_to_hz(
            Self::interpolator(),
            f64::from(self.config.sample_rate.0),
            16_000.0,
        )
    }

    #[inline]
    fn frame_to_sample(frame: Self::Frame) -> Self::Output {
        <Self::Output as cpal::Sample>::from(&frame)
    }

    fn send(&self, sample: O) -> Result<(), Error> {
        Ok(self.sink.send(AudioMessage::Data(sample))?)
    }

    fn send_error(&self, error: Error) -> Result<(), Error> {
        Ok(self.sink.send(AudioMessage::Error(error))?)
    }
}

impl<O: MySample> Process for Processor<O, 2> {
    type Input = f32;
    type Output = O;
    type Frame = [f32; 2];

    type Signal<'a> = Converter<
        FromInterleavedSamplesIterator<Copied<Iter<'a, Self::Input>>, Self::Frame>,
        Sinc<[Self::Frame; 128]>,
    >;

    fn config(&self) -> &cpal::StreamConfig {
        &self.config
    }

    fn interpolator() -> Sinc<[Self::Frame; 128]> {
        Sinc::new(ring_buffer::Fixed::from([Self::Frame::default(); 128]))
    }

    fn signal<'a>(&'a mut self, input: &'a [Self::Input]) -> Self::Signal<'a> {
        let signal =
            dasp::signal::from_interleaved_samples_iter::<_, [f32; 2]>(input.iter().copied());

        signal.from_hz_to_hz(
            Self::interpolator(),
            f64::from(self.config.sample_rate.0),
            16_000.0,
        )
    }

    #[inline]
    fn frame_to_sample(frame: Self::Frame) -> Self::Output {
        let avg = (frame[0] + frame[1]) / 2.0;
        <Self::Output as cpal::Sample>::from(&avg)
    }

    fn send(&self, sample: O) -> Result<(), Error> {
        Ok(self.sink.send(AudioMessage::Data(sample))?)
    }

    fn send_error(&self, error: Error) -> Result<(), Error> {
        Ok(self.sink.send(AudioMessage::Error(error))?)
    }
}
