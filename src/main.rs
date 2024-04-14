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
#[command(version, about, long_about = None)]
struct App {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, clap::Subcommand)]
enum Commands {
    ListChannels,
    RunDaemon(DaemonInit),
}

#[derive(Debug, clap::Args)]
struct DaemonInit {
    /// Path to the model file
    #[clap(short, long)]
    model: String,

    #[clap(short, long, value_parser = whisper::parse_strategy)]
    strategy: Option<whisper::StrategyOpt>,

    /// Pattern to match against device name
    #[clap(long)]
    device_name: Option<String>,

    /// Socket path
    #[clap(long)]
    socket_path: String,
}

impl DaemonInit {
    pub fn strategy(&self) -> whisper_rs::SamplingStrategy {
        self.strategy.clone().unwrap_or_default().into()
    }
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

    match App::parse().command {
        Commands::ListChannels => {
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
            return Ok(());
        }
        Commands::RunDaemon(app) => {
            log::info!("Launching with settings: {:?}", app);

            let device = match &app.device_name {
                Some(n) => device_matching_name(n)?,
                None => cpal::default_host()
                    .default_input_device()
                    .ok_or(anyhow!("no input device available"))?,
            };

            log::info!("Found device: {:?}", device.name()?);

            let mut daemon = Daemon::new(app, Some(device));
            let should_reset = daemon.run_loop()?;
            if should_reset {
                std::process::exit(1);
            }
        }
    }
    Ok(())
}
