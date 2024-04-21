pub mod vad;

mod controller;
mod process;
mod recording;

pub use recording::{Error as RecordingError, Recording, Session};

use cpal::{
    traits::{DeviceTrait, HostTrait},
    DevicesError,
};

pub trait MySample: Send + hound::Sample + cpal::Sample + 'static {}
impl<S> MySample for S where S: Send + hound::Sample + cpal::Sample + 'static {}

pub fn list_channels() -> Result<(), DevicesError> {
    let host = cpal::default_host();
    for (device_name, config) in host.input_devices()?.enumerate().flat_map(|(i, d)| {
        let name = d.name().unwrap_or_else(|_| format!("Unnamed device #{i}"));
        let configs: Vec<_> = d
            .supported_input_configs()
            .map(std::iter::Iterator::collect)
            .unwrap_or_default();

        configs.into_iter().map(move |c| (name.clone(), c))
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
