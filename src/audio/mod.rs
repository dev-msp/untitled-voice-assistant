use std::{
    io::{self},
    sync::{Arc, Mutex},
    time::Duration,
};

use cpal::{
    traits::{DeviceTrait, StreamTrait},
    FromSample, Sample,
};

pub fn record_from_input_device<W>(
    device: &cpal::Device,
    duration: Duration,
    writer: W,
) -> Result<(), anyhow::Error>
where
    W: io::Write + io::Seek + Send + 'static,
{
    let config = device.default_input_config()?;

    let writer = hound::WavWriter::new(writer, wav_spec_from_config(&config))?;
    let writer = Arc::new(Mutex::new(Some(writer)));
    let writer2 = writer.clone();

    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => device.build_input_stream(
            &config.into(),
            move |data, _: &_| write_input_data::<_, f32, f32>(data, &writer2),
            move |err| eprintln!("an error occurred on stream: {}", err),
            None,
        )?,
        _ => panic!("unsupported sample format"),
    };

    stream.play()?;

    std::thread::sleep(duration);
    drop(stream);
    writer.lock().unwrap().take().unwrap().finalize()?;

    Ok(())
}

fn wav_spec_from_config(config: &cpal::SupportedStreamConfig) -> hound::WavSpec {
    hound::WavSpec {
        channels: config.channels() as _,
        sample_rate: config.sample_rate().0,
        bits_per_sample: (config.sample_format().sample_size() * 8) as _,
        sample_format: if config.sample_format().is_float() {
            hound::SampleFormat::Float
        } else {
            hound::SampleFormat::Int
        },
    }
}

type WavWriterHandle<W> = Arc<Mutex<Option<hound::WavWriter<W>>>>;

fn write_input_data<W, T, U>(input: &[T], writer: &WavWriterHandle<W>)
where
    W: io::Write + io::Seek,
    T: Sample,
    U: Sample + hound::Sample + FromSample<T>,
{
    if let Ok(mut guard) = writer.try_lock() {
        if let Some(writer) = guard.as_mut() {
            for &sample in input.iter() {
                let sample: U = U::from_sample(sample);
                writer.write_sample(sample).ok();
            }
        }
    }
}
