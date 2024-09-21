use std::time::Duration;

use clap::ValueEnum;
use crossbeam::channel::SendError;
use derive_builder::{Builder, UninitializedFieldError};
use serde::{Deserialize, Serialize};
use whisper_rs::WhisperError;

pub type TranscribeResult = Result<Vec<sttx::Timing>, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Failed to send transcription result")]
    Sync(#[from] Box<SendError<TranscribeResult>>),

    #[error("Whisper error: {0}")]
    Whisper(#[from] WhisperError),

    #[error("Job builder received incomplete data: {0}")]
    JobBuild(#[from] UninitializedFieldError),
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ValueEnum)]
pub enum Model {
    #[default]
    #[serde(rename = "base")]
    Base,

    #[serde(rename = "small")]
    Small,

    #[serde(rename = "medium")]
    Medium,

    #[serde(rename = "large")]
    Large,
}

impl Model {
    #[must_use]
    pub fn filename(&self) -> String {
        let base_name = match self {
            Model::Base => "base.en",
            Model::Small => "small.en",
            Model::Medium => "medium",
            Model::Large => "large-v3-turbo",
        };
        format!("ggml-{base_name}.bin")
    }
}

#[derive(Debug, Builder)]
#[builder(build_fn(error = "Error"))]
pub struct Job {
    model: Model,
    audio: Vec<f32>,
    strategy: whisper_rs::SamplingStrategy,
    sample_rate: u32,
    prompt: Option<String>,
}

impl Job {
    #[must_use]
    pub fn builder() -> JobBuilder {
        JobBuilder::default()
    }

    /// # Panics
    /// - when sample rate <= 0
    #[allow(clippy::cast_sign_loss)]
    #[allow(clippy::cast_precision_loss)]
    #[must_use]
    pub fn duration(&self) -> Duration {
        assert!(self.sample_rate > 0);
        let secs_int = self.audio.len() / self.sample_rate as usize;
        let rem = self.audio.len() % self.sample_rate as usize;
        Duration::from_secs(secs_int as u64)
            + Duration::from_secs_f32(rem as f32 / self.sample_rate as f32)
    }

    #[must_use]
    pub fn prompt(&self) -> Option<&str> {
        self.prompt.as_deref()
    }

    #[must_use]
    pub fn strategy(&self) -> whisper_rs::SamplingStrategy {
        self.strategy.clone()
    }

    #[must_use]
    pub fn audio(&self) -> &[f32] {
        &self.audio
    }

    #[must_use]
    pub fn model(&self) -> Model {
        self.model
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum StrategyOpt {
    Greedy { best_of: i32 },
    Beam { beam_size: i32, patience: f32 },
}

impl Default for StrategyOpt {
    fn default() -> Self {
        StrategyOpt::Beam {
            beam_size: 5,
            patience: 0.0,
        }
    }
}

impl From<StrategyOpt> for whisper_rs::SamplingStrategy {
    fn from(opt: StrategyOpt) -> Self {
        match opt {
            StrategyOpt::Greedy { best_of } => whisper_rs::SamplingStrategy::Greedy { best_of },
            StrategyOpt::Beam {
                beam_size,
                patience,
            } => whisper_rs::SamplingStrategy::BeamSearch {
                beam_size,
                patience,
            },
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum StrategyParseError {
    #[error("'{0}' is not among the supported strategies: 'greedy', 'beam'")]
    Unsupported(String),

    #[error("strategy must be of the form 'qkind' or 'kind:n'")]
    Malformed,

    #[error("{0} must be at least 1")]
    AtLeastOne(String),

    #[error("error parsing integer")]
    ParseIntError(#[from] std::num::ParseIntError),

    #[error("error parsing float")]
    ParseFloatError(#[from] std::num::ParseFloatError),
}

pub fn parse_strategy(s: &str) -> Result<StrategyOpt, StrategyParseError> {
    let mut parts = s.split(':');
    let (kind, n) = (
        parts.next().ok_or(StrategyParseError::Malformed)?,
        parts
            .next()
            .map(str::parse)
            .transpose()
            .map_err(StrategyParseError::ParseIntError)?,
    );
    Ok(match kind {
        "greedy" => {
            let n = n.unwrap_or(2);
            if n < 1 {
                return Err(StrategyParseError::AtLeastOne("best_of".to_string()));
            }
            StrategyOpt::Greedy { best_of: n }
        }
        "beam" => {
            let n = n.unwrap_or(10);
            if n < 1 {
                return Err(StrategyParseError::AtLeastOne("beam_size".to_string()));
            }
            let patience = parts
                .next()
                .map(str::parse)
                .transpose()
                .map_err(StrategyParseError::ParseFloatError)?
                .unwrap_or(0.0);
            StrategyOpt::Beam {
                beam_size: n,
                patience,
            }
        }
        sgy => return Err(StrategyParseError::Unsupported(sgy.to_string())),
    })
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_parse_strategy() {
        assert_eq!(
            parse_strategy("greedy").unwrap(),
            StrategyOpt::Greedy { best_of: 2 }
        );
        assert_eq!(
            parse_strategy("greedy:3").unwrap(),
            StrategyOpt::Greedy { best_of: 3 }
        );
        assert_eq!(
            parse_strategy("beam").unwrap(),
            StrategyOpt::Beam {
                beam_size: 10,
                patience: 0.0
            }
        );
        assert_eq!(
            parse_strategy("beam:5").unwrap(),
            StrategyOpt::Beam {
                beam_size: 5,
                patience: 0.0
            }
        );
        assert_eq!(
            parse_strategy("beam:5:0.5").unwrap(),
            StrategyOpt::Beam {
                beam_size: 5,
                patience: 0.5
            }
        );
    }

    #[test]
    #[should_panic(expected = "best_of must be at least 1")]
    fn test_bad_beam_parse() {
        parse_strategy("beam:0").unwrap();
    }

    #[test]
    #[should_panic(expected = "best_of must be at least 1")]
    fn test_bad_greedy_parse() {
        parse_strategy("greedy:0").unwrap();
    }

    #[test]
    #[should_panic(expected = "invalid strategy")]
    fn test_bad_parse() {
        parse_strategy("berm").unwrap();
    }
}
