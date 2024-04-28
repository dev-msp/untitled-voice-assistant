use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    model_dir: PathBuf,
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
        let config = toml::from_str(&toml)?;
        Ok(config)
    }

    pub fn write(&self) -> Result<(), Error> {
        let toml = toml::to_string_pretty(self)?;
        std::fs::write(Self::path()?, toml)?;

        Ok(())
    }

    #[must_use]
    pub fn model_dir(&self) -> &Path {
        self.model_dir.as_path()
    }

    fn path() -> Result<PathBuf, Error> {
        Ok(xdg::BaseDirectories::with_prefix("voice")
            .unwrap()
            .place_config_file("config.toml")?)
    }
}
