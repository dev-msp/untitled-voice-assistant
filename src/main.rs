mod app;
mod audio;
mod socket;
mod sync;
mod whisper;

use anyhow::anyhow;

use clap::Parser;
use cpal::traits::{DeviceTrait, HostTrait};
use whisper_rs::install_whisper_log_trampoline;

use crate::{app::run_loop, socket::receive_instructions};

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

    let (cmd_recv, cmds) = receive_instructions(&app.socket_path)?;

    let model = app.model.clone();
    let (recsnd, recrecv) = std::sync::mpsc::channel();
    let (wh_recv, hnd) = whisper::transcription_worker(&model, recrecv)?;

    run_loop(&app, &device, cmd_recv, recsnd, wh_recv)?;

    hnd.join().unwrap()?;
    cmds.join().unwrap();

    Ok(())
}
