#![deny(clippy::pedantic)]

mod web;

use clap::Parser;
use tokio::task::spawn_blocking;
use voice::app::{Daemon, DaemonInit};

#[derive(Debug, clap::Parser)]
#[command(version, about, long_about = None)]
struct App {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, clap::Args)]
struct ServeDaemonOpts {
    #[clap(flatten)]
    delegate: DaemonInit,

    #[clap(long, value_parser = web::parse_addr_option)]
    serve: (String, u16),
}

impl ServeDaemonOpts {
    pub fn serve_addr(&self) -> (String, u16) {
        self.serve.clone()
    }
}

#[derive(Debug, clap::Subcommand)]
enum Commands {
    RunDaemon(ServeDaemonOpts),
}

#[derive(Debug, thiserror::Error)]
enum Error {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Daemon error: {0}")]
    Daemon(#[from] voice::app::Error),
}

async fn run_web_server(app: ServeDaemonOpts) -> std::io::Result<bool> {
    let (commands_out, commands_in) = crossbeam::channel::bounded(1);
    let (responses_out, responses_in) = crossbeam::channel::bounded(1);

    // NOTES cleanup: The inline notes questioned the naming and separation of Commands.
    // The approach taken here is to use a single set of channels to the daemon,
    // where the daemon's Command enum handles all requests from the web layer.
    // The ApiCommand/ApiResponse separation previously in web.rs is removed,
    // and all web requests needing daemon interaction now use AppChannel.

    let addr = app.serve_addr();
    let handle = spawn_blocking(move || {
        log::info!("Launching with settings: {:?}", app);
        let mut daemon = Daemon::new(app.delegate);
        daemon.run_loop(commands_in, responses_out)
    });

    // What does the plan say to do here?
    // Plan (from audio.md) implies daemon handles new command.
    // This structure passes daemon communication channels to web server.
    let server = web::Server::new(
        addr,
        commands_out,
        responses_in,
        // api_commands_out, // removed
        // api_responses_in, // removed
    );
    let server_handle = server.run();

    tokio::select! {
        app_finished = handle => {
            Ok(app_finished.expect("failed to join app thread").expect("app failed"))
        },
        outcome = server_handle => {
            outcome.expect("server failed");
            log::info!("server finished");
            Ok(true)
        },
    }
}

#[actix_web::main]
async fn main() -> Result<(), Error> {
    env_logger::init();

    match App::parse().command {
        Commands::RunDaemon(app) => {
            if run_web_server(app).await? {
                std::process::exit(1);
            }
            Ok(())
        }
    }
}
