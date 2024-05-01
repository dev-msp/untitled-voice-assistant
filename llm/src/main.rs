use clap::Parser;
use itertools::Itertools;
use llm::{
    vendor::{self, openai},
    Config,
};

#[derive(Debug, clap::Parser)]
#[command(version, about, long_about = None)]
struct App {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, clap::Subcommand)]
enum Commands {
    Completion(Completion),
    ListModels,
}

#[derive(Debug, clap::Args)]
struct Completion {
    #[clap(short, long)]
    provider: vendor::Provider,

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
    env_logger::init();

    let app = App::parse();
    let config = Config::read()?;
    match app.command {
        Commands::Completion(c) => {
            let response = c.run(&config).await?;
            println!("{}", response.content());
        }
        Commands::ListModels => {
            let models: Vec<_> = vendor::ollama::list_models().await?.into();
            println!(
                "{:?}",
                models
                    .iter()
                    .map(|m| (m.name(), m.human_size()))
                    .collect_vec()
            );
        }
    }
    Ok(())
}
