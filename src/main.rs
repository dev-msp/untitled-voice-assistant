#![deny(clippy::pedantic)]
mod app;
mod audio;
mod socket;
mod sync;
mod web;
mod whisper;

use app::{command::Command, response::Response};
use clap::Parser;
use crossbeam::channel::{Receiver, Sender};
use tokio::task::spawn_blocking;
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

    /// Socket path
    #[clap(long)]
    socket_path: Option<String>,

    #[clap(long, value_parser = web::parse_addr_option)]
    serve: Option<(String, u16)>,
}

impl DaemonInit {
    pub fn strategy(&self) -> whisper_rs::SamplingStrategy {
        self.strategy.clone().unwrap_or_default().into()
    }
}

fn run_daemon(
    app: DaemonInit,
    commands: Receiver<Command>,
    responses: Sender<Response>,
) -> Result<bool, anyhow::Error> {
    log::info!("Launching with settings: {:?}", app);

    let mut daemon = Daemon::new(app);

    daemon.run_loop(commands, responses)
}

async fn run_web_server(addr: (String, u16), app: DaemonInit) -> std::io::Result<bool> {
    let (commands_out, commands_in) = crossbeam::channel::bounded(1);
    let (responses_out, responses_in) = crossbeam::channel::bounded(1);
    let handle = spawn_blocking(|| run_daemon(app, commands_in, responses_out));

    let server = web::run(addr, commands_out, responses_in);

    tokio::select! {
        app_finished = handle => {
            Ok(app_finished.expect("failed to join app thread").expect("app failed"))
        },
        outcome = server => {
            outcome.expect("server failed");
            log::info!("server finished");
            Ok(false)
        },
    }
}

#[actix_web::main]
async fn main() -> Result<(), anyhow::Error> {
    install_whisper_log_trampoline();

    env_logger::init();

    match App::parse().command {
        Commands::ListChannels => audio::input::list_channels(),
        Commands::RunDaemon(app) => {
            let should_reset = match (&app.socket_path, &app.serve) {
                (None, Some(a)) => run_web_server(a.clone(), app).await?,
                (Some(p), None) => {
                    let (rcmds, resps, listener) = socket::receive_instructions(p.clone())?;
                    let outcome = run_daemon(app, rcmds, resps)?;
                    listener.join().expect("failed to join listener thread")?;
                    outcome
                }
                _ => {
                    log::error!(
                        "Invalid arguments: socket path and serve flag cannot be used together but at least one must be provided"
                    );
                    true
                }
            };
            if should_reset {
                std::process::exit(1);
            }
            Ok(())
        }
    }
}
