mod audio;

use std::{io::BufWriter, time::Duration};

use anyhow::anyhow;
use cpal::traits::HostTrait;

fn main() -> Result<(), anyhow::Error> {
    let host = cpal::default_host();

    let device = host
        .default_input_device()
        .ok_or(anyhow!("no input device available"))?;

    const PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/output.wav");
    let writer = BufWriter::new(std::fs::File::create(PATH)?);

    audio::record_from_input_device(&device, Duration::from_secs(5), writer)?;

    Ok(())
}
