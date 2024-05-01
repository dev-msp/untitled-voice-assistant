use anyhow::anyhow;
use clap::{Parser, ValueEnum};
use llm::{
    vendor::{self, openai},
    Config,
};

#[derive(Debug, Clone, Copy, ValueEnum)]
enum Provider {
    OpenAi,
    Groq,
}

impl Provider {
    async fn completion(
        &self,
        config: &Config,
        system_message: Option<String>,
        user_message: String,
    ) -> Result<openai::compat::Response, anyhow::Error> {
        match self {
            Provider::Groq => {
                let provider = &config.providers.groq;
                let api_key = provider
                    .get_api_key()
                    .await?
                    .ok_or_else(|| anyhow!("no api key?"))?;
                vendor::groq::completion(api_key, system_message, user_message).await
            }
            Provider::OpenAi => {
                let provider = &config.providers.openai;
                let api_key = provider
                    .get_api_key()
                    .await?
                    .ok_or_else(|| anyhow!("no api key?"))?;
                vendor::openai::completion(api_key, system_message, user_message).await
            }
        }
    }
}

#[derive(Debug, clap::Parser)]
#[command(version, about, long_about = None)]
struct App {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, clap::Subcommand)]
enum Commands {
    Completion(Completion),
}

#[derive(Debug, clap::Args)]
struct Completion {
    #[clap(short, long)]
    provider: Provider,

    #[clap(short, long)]
    system_message: Option<String>,

    user_message: String,
}

impl Completion {
    async fn run(self, config: &Config) -> Result<openai::compat::Response, anyhow::Error> {
        self.provider
            .completion(config, self.system_message, self.user_message)
            .await
    }
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let app = App::parse();
    let config = Config::read()?;
    match app.command {
        Commands::Completion(c) => {
            let response = c.run(&config).await?;
            println!("{response}");
        }
    }
    Ok(())
}
