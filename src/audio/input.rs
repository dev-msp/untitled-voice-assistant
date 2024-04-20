use cpal::traits::{DeviceTrait, HostTrait};

pub trait MySample: Send + hound::Sample + cpal::Sample + 'static {}
impl<S> MySample for S where S: Send + hound::Sample + cpal::Sample + 'static {}

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
