mod agent;
mod app;
mod audio;
mod socket;
mod sync;
mod whisper;

use anyhow::anyhow;

use clap::Parser;
use cpal::traits::{DeviceTrait, HostTrait};
use whisper_rs::install_whisper_log_trampoline;

use crate::app::run_loop;

#[derive(clap::Parser)]
struct App {
    /// Length of recording in seconds
    #[clap(short, long)]
    duration_in_secs: Option<usize>,

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
    let app = App::parse();

    let device = match &app.device_name {
        Some(n) => device_matching_name(n)?,
        None => cpal::default_host()
            .default_input_device()
            .ok_or(anyhow!("no input device available"))?,
    };

    eprintln!("{:?}", device.name()?);

    // So I want to be using threads properly here. A receiver can only be used in the thread in
    // which it's created, so that should guide me especially. That means anything I want the main
    // thread to get is sending a sender. The main thread holds on to the receiver.

    run_loop(&app, &device)?;

    // remove socket
    std::fs::remove_file(&app.socket_path)?;

    Ok(())
}
