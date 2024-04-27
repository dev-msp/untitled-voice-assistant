mod vendor;

use clap::{Parser, ValueEnum};
use vendor::openai::compat::Response;

#[derive(Debug, Clone, Copy, ValueEnum)]
enum Provider {
    OpenAi,
    Groq,
}

impl Provider {
    async fn completion(
        &self,
        system_message: Option<String>,
        user_message: String,
    ) -> Result<Response, anyhow::Error> {
        match self {
            Provider::Groq => {
                let api_key = get_env("GROQ_API_KEY")?;
                vendor::groq::completion(api_key, system_message, user_message).await
            }
            Provider::OpenAi => {
                let api_key = get_env("OPENAI_API_KEY")?;
                vendor::openai::completion(api_key, system_message, user_message).await
            }
        }
    }
}

fn get_env(key: &str) -> Result<String, anyhow::Error> {
    std::env::var(key).map_err(|_| anyhow::anyhow!("{} is not set", key))
}

#[derive(Debug, clap::Parser)]
struct App {
    #[clap(short, long)]
    provider: Provider,

    #[clap(short, long)]
    system_message: Option<String>,

    user_message: String,
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let app = App::parse();
    let response = app
        .provider
        .completion(app.system_message, app.user_message)
        .await?;
    println!("{response}");
    Ok(())
}
