use crossbeam::channel::Sender;

use dasp::{interpolate::sinc::Sinc, ring_buffer, Signal};
use itertools::Itertools;

use super::MySample;

pub struct Processor<S, O, const CHANNELS: usize>
where
    S: cpal::Sample + dasp::Sample,
{
    config: cpal::StreamConfig,
    buffer: ring_buffer::Bounded<[[S; CHANNELS]; 64]>,
    sink: Sender<O>,
}

impl<S, O, const CHANNELS: usize> Processor<S, O, CHANNELS>
where
    S: cpal::Sample + dasp::Sample + Default,
    O: MySample,
{
    pub fn new(
        sink: Sender<O>,
        config: cpal::StreamConfig,
        chunk_size_out: usize,
    ) -> Result<Self, anyhow::Error> {
        let input_rate = config.sample_rate.0 as usize;
        let channels = config.channels as usize;
        log::debug!(
            "Creating resampler with input rate: {}, chunk size: {}, channels: {}",
            input_rate,
            chunk_size_out,
            channels
        );

        Ok(Self {
            config,
            buffer: ring_buffer::Bounded::from([[S::default(); CHANNELS]; 64]),
            sink,
        })
    }
}

impl<S, O> Processor<S, O, 1>
where
    S: cpal::Sample + dasp::Sample + dasp::Frame + Default,
{
    fn interpolator(&self) -> Sinc<[S; 128]> {
        Sinc::new(ring_buffer::Fixed::from([S::default(); 128]))
    }
}

impl<S, O> Processor<S, O, 2>
where
    S: cpal::Sample + dasp::Sample + Default,
{
    fn interpolator(&self) -> Sinc<[[S; 2]; 128]> {
        Sinc::new(ring_buffer::Fixed::from([[S::default(); 2]; 128]))
    }
}

impl<O: MySample> Processor<f32, O, 1> {
    pub fn mono_samples<'a>(&'a mut self, input: &'a [f32]) -> impl Iterator<Item = O> + 'a {
        let signal = dasp::signal::from_iter(input.iter().copied());
        let new_signal = signal.from_hz_to_hz(
            self.interpolator(),
            self.config.sample_rate.0 as f64,
            16_000.0,
        );
        new_signal
            .until_exhausted()
            .map(move |sample| O::from(&sample))
    }

    pub fn write_input_data(&mut self, input: &[f32]) -> Result<(), anyhow::Error> {
        for sample in self.mono_samples(input).collect_vec() {
            self.sink.send(sample).unwrap();
        }

        Ok(())
    }
}

impl<O: MySample> Processor<f32, O, 2> {
    pub fn mono_samples<'a>(&'a mut self, input: &'a [f32]) -> impl Iterator<Item = O> + 'a {
        let signal =
            dasp::signal::from_interleaved_samples_iter::<_, [f32; 2]>(input.iter().copied());
        let new_signal = signal
            .from_hz_to_hz(
                self.interpolator(),
                self.config.sample_rate.0 as f64,
                16_000.0,
            )
            .buffered(self.buffer);
        new_signal.until_exhausted().map(move |sample| {
            let mono = (sample[0] + sample[1]) / 2.0;
            O::from(&mono)
        })
    }

    pub fn write_input_data(&mut self, input: &[f32]) -> Result<(), anyhow::Error> {
        for sample in self.mono_samples(input).collect_vec() {
            self.sink.send(sample).unwrap();
        }

        Ok(())
    }
}
