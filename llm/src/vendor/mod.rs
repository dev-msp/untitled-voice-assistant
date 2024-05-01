use anyhow::anyhow;

use crate::Config;

pub mod groq;
pub mod ollama;
pub mod openai;

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
                groq::completion(api_key, system_message, user_message).await
            }
            Provider::OpenAi => {
                let provider = &config.providers.openai;
                let api_key = provider
                    .get_api_key()
                    .await?
                    .ok_or_else(|| anyhow!("no api key?"))?;
                openai::completion(api_key, system_message, user_message).await
            }
            Provider::Ollama => ollama::completion(system_message, user_message).await,
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
