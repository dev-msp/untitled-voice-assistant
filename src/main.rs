mod app;
mod audio;
mod socket;
mod sync;
mod whisper;

use anyhow::anyhow;
use app::{command::Command, response::Response};
use clap::Parser;
use cpal::traits::{DeviceTrait, HostTrait};
use crossbeam::channel::{Receiver, Sender};
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

    #[clap(long)]
    serve: bool,
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
fn run_daemon(
    app: DaemonInit,
    commands: Receiver<Command>,
    responses: Sender<Response>,
) -> Result<bool, anyhow::Error> {
    log::info!("Launching with settings: {:?}", app);

    let device = device_matching_name(
        &app.device_name
            .clone()
            .ok_or(anyhow!("no device name provided"))?,
    )?;

    let mut daemon = Daemon::new(app, Some(device));

    daemon.run_loop(commands, responses)
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
            Ok(())
        }
        Commands::RunDaemon(app) => {
            let (rcmds, resps, listener) = socket::receive_instructions(app.socket_path.clone())?;
            let should_restart = run_daemon(app, rcmds, resps)?;
            listener.join().expect("failed to join listener thread")?;
            if should_restart {
                std::process::exit(1);
            }
            Ok(())
        }
    }
}
