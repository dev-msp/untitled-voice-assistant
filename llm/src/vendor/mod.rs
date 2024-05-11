use anyhow::anyhow;

use crate::Config;

use self::openai::compat;

pub mod groq;
pub mod ollama;
pub mod openai;

#[derive(Debug, Clone, clap::Args)]
pub struct Completion {
    #[clap(short, long)]
    model: Option<String>,

    #[clap(short, long)]
    system_message: Option<String>,

    user_message: String,
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
#[value(rename_all = "lower")]
pub enum Provider {
    OpenAi,
    Groq,
    Ollama,
}

impl Provider {
    pub async fn completion(
        &self,
        config: &Config,
        comp: Completion,
    ) -> Result<openai::compat::Response, anyhow::Error> {
        let model = comp
            .model
            .clone()
            .or_else(|| self.default_model(config))
            .ok_or_else(|| {
                anyhow!(
                    "No model specified and no default configured for {}",
                    self.name()
                )
            })?;

        let system_message = comp
            .system_message
            .clone()
            .or_else(|| config.default_system_message.clone())
            .ok_or_else(|| anyhow!("No default system message configured"))?;

        match self {
            Provider::Groq => {
                let provider = &config.providers.groq;
                let api_key = provider
                    .get_api_key()
                    .await?
                    .ok_or_else(|| anyhow!("no api key?"))?;
                groq::completion(api_key, model, system_message, comp.user_message).await
            }
            Provider::OpenAi => {
                let provider = &config.providers.openai;
                let api_key = provider
                    .get_api_key()
                    .await?
                    .ok_or_else(|| anyhow!("no api key?"))?;
                openai::completion(api_key, model, system_message, comp.user_message).await
            }
            Provider::Ollama => {
                let model = comp
                    .model
                    .ok_or_else(|| anyhow!("No model specified for Ollama"))?;
                ollama::completion(model, system_message, comp.user_message).await
            }
        }
    }

    pub async fn list_models(&self, config: &Config) -> anyhow::Result<Vec<String>> {
        match self {
            Provider::Groq => {
                let provider = &config.providers.groq;
                let api_key = provider
                    .get_api_key()
                    .await?
                    .ok_or_else(|| anyhow!("no api key?"))?;
                compat::list_models(groq::GROQ_CHAT_API, api_key).await
            }
            Provider::OpenAi => {
                let provider = &config.providers.openai;
                let api_key = provider
                    .get_api_key()
                    .await?
                    .ok_or_else(|| anyhow!("no api key?"))?;
                openai::list_models(api_key).await
            }
            Provider::Ollama => ollama::list_models().await,
        }
    }

    fn name(&self) -> &'static str {
        match self {
            Provider::Groq => "Groq",
            Provider::OpenAi => "OpenAI",
            Provider::Ollama => "Ollama",
        }
    }

    fn default_model(&self, config: &Config) -> Option<String> {
        match self {
            Provider::Groq => config.providers.groq.default_model(),
            Provider::OpenAi => config.providers.openai.default_model(),
            Provider::Ollama => config.providers.ollama.default_model(),
        }
    }
}

pub async fn get_api_key(api_key_command: &[String]) -> anyhow::Result<Option<String>> {
    if api_key_command.is_empty() {
        return Ok(None);
    }
    let bin = api_key_command.first().unwrap();
    let args = api_key_command.iter().skip(1);
    let mut cmd = std::process::Command::new(bin);
    cmd.args(args);
    let output = cmd.output()?;
    let api_key = String::from_utf8(output.stdout)?.trim().to_string();

    Ok(Some(api_key))
}
