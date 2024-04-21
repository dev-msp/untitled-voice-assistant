mod client;

use clap::Parser;
use client::RunningApp;

#[derive(Debug, clap::Parser)]
#[command(version, about, long_about = None)]
struct App {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, clap::Subcommand)]
enum Commands {
    Client(client::App),
}

#[tokio::main]
async fn main() -> Result<(), client::Error> {
    env_logger::init();

    match App::parse().command {
        Commands::Client(client_command) => {
            let resp = RunningApp::from(client_command).execute().await?;
            log::info!("{:?}", resp);
            Ok(())
        }
    }
}
