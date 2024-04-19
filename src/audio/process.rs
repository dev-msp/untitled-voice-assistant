use crossbeam::channel::Sender;
use rubato::{FftFixedOut, Resampler, ResamplerConstructionError};
use whisper_rs::convert_stereo_to_mono_audio;

use super::MySample;

pub struct Processor<S, O>
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
    pub fn new(
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
            buffer: Vec::with_capacity(
                chunk_size_out.max(resampler.nbr_channels() * resampler.input_frames_max()),
            ),
            config,
            resampler,
            sink,
        })
    }
}

impl<O: MySample> Processor<f32, O> {
    pub fn write_input_data(&mut self, input: &[f32]) -> Result<(), anyhow::Error> {
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

    pub fn flush_to_sink(&mut self) -> Result<(), anyhow::Error> {
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

    fn consume_input_data<'a>(&mut self, input: &'a [f32]) -> &'a [f32] {
        let remaining = self.buffer.capacity() - self.buffer.len();
        let cutoff = remaining.min(input.len());
        self.buffer.extend_from_slice(&input[0..cutoff]);

        &input[cutoff..]
    }

    fn input_buffer(&self) -> Vec<f32> {
        let cap = self.buffer.capacity();
        log::trace!("Allocating input buffer with capacity: {}", cap);
        Vec::with_capacity(cap)
    }
}
