mod vendor;

use clap::Parser;

#[derive(Debug, clap::Parser)]
struct App {
    #[clap(short, long)]
    system_message: Option<String>,

    user_message: String,
}

/// The async main entry point of the application.
#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let app = App::parse();
    let api_key = std::env::var("GROQ_API_KEY")?;

    let response = vendor::groq::completion(api_key, app.system_message, app.user_message).await?;
    println!("{response}");
    Ok(())
}
