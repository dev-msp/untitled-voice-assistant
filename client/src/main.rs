mod client;

use clap::Parser;
use client::{App, Commands, RunningApp};
use voice::app::response::Response;

#[tokio::main]
async fn main() -> Result<(), client::Error> {
    env_logger::init();

    let app = App::parse();
    match &app.command {
        Commands::Start { .. } => {
            match RunningApp::from(app).execute().await? {
                Response::Ack(_) => (),
                r => return Err(client::api::Error::UnexpectedResponse(r).into()),
            }
            Ok(())
        }
        Commands::Stop => {
            match RunningApp::from(app).execute().await? {
                Response::Transcription { content, .. } => {
                    let Some(content) = content else {
                        eprintln!("No transcription available");
                        return Ok(());
                    };
                    println!("{content}");
                }
                r => return Err(client::api::Error::UnexpectedResponse(r).into()),
            }
            Ok(())
        }

        _ => {
            let resp = RunningApp::from(app).execute().await?;
            log::info!("{:?}", resp);
            Ok(())
        }
    }
}
