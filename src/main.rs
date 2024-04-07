mod app;
mod audio;
mod socket;
mod sync;
mod whisper;

use anyhow::anyhow;

use clap::Parser;
use cpal::traits::{DeviceTrait, HostTrait};
use whisper_rs::install_whisper_log_trampoline;

use crate::app::Daemon;

#[derive(Debug, clap::Parser)]
struct App {
    /// Path to the model file
    #[clap(short, long)]
    model: String,

    /// Pattern to match against device name
    #[clap(long)]
    device_name: Option<String>,

    /// Socket path
    #[clap(long)]
    socket_path: String,
}

fn device_matching_name(name: &str) -> Result<cpal::Device, anyhow::Error> {
    let host = cpal::default_host();

    let device = host
        .input_devices()?
        .find(|d| d.name().map(|n| n.contains(name)).unwrap_or(false))
        .ok_or(anyhow!("no input device available"))?;

    Ok(device)
}

fn main() -> Result<(), anyhow::Error> {
    install_whisper_log_trampoline();

    env_logger::init();

    let app = App::parse();
    log::info!("Launching with settings: {:?}", app);

    let device = match &app.device_name {
        Some(n) => device_matching_name(n)?,
        None => cpal::default_host()
            .default_input_device()
            .ok_or(anyhow!("no input device available"))?,
    };

    log::info!("Found device: {:?}", device.name()?);

    let daemon = Daemon::new(app, Some(device));
    daemon.run_loop()?;

    Ok(())
}
