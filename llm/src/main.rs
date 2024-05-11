use clap::Parser;
use itertools::Itertools;
use llm::{vendor, Config};

#[derive(Debug, clap::Parser)]
#[command(version, about, long_about = None)]
struct App {
    #[clap(short, long)]
    provider: vendor::Provider,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, clap::Subcommand)]
enum Commands {
    Completion(vendor::Completion),
    ListModels,
}

impl App {
    async fn run(self, config: &Config) -> Result<(), anyhow::Error> {
        match self.command {
            Commands::Completion(c) => {
                let response = self.provider.completion(config, c).await?;
                println!("{}", response.content());
            }
            Commands::ListModels => {
                let models = self.provider.list_models(config).await?;
                println!("{}", models.iter().join("\n"));
            }
        }
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    env_logger::init();

    let app = App::parse();
    let config = Config::read()?;
    app.run(&config).await
}
