use std::path::PathBuf;

use serde::{de::Error as _, Deserialize, Serialize};

use crate::vendor::{ollama, openai::compat};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub default_system_message: Option<String>,
    pub providers: Providers,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Providers {
    pub groq: compat::Provider,
    pub openai: compat::Provider,
    pub ollama: ollama::Provider,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Config error: {0}")]
    Config(#[from] xdg::BaseDirectoriesError),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Error serializing TOML: {0}")]
    Write(#[from] toml::ser::Error),

    #[error("Error deserializing TOML: {0}")]
    Read(#[from] toml::de::Error),
}

impl Config {
    pub fn read() -> Result<Self, Error> {
        let toml = std::fs::read_to_string(Self::path()?)?;
        let val: toml::Value = toml::from_str(&toml)?;
        let table = val
            .as_table()
            .ok_or_else(|| Error::Read(toml::de::Error::custom("expected table")))?;

        let cfg = table
            .get("llm")
            .ok_or_else(|| Error::Read(toml::de::Error::custom("missing llm")))?;

        let config = toml::from_str(toml::ser::to_string(cfg)?.as_str())?;

        Ok(config)
    }

    fn path() -> Result<PathBuf, Error> {
        Ok(xdg::BaseDirectories::with_prefix("voice")
            .unwrap()
            .place_config_file("config.toml")?)
    }
}
