use std::time::Duration;

use anyhow::anyhow;
use crossbeam::channel::SendError;
use whisper_rs::WhisperError;

pub type TranscribeResult = Result<Vec<sttx::Timing>, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Failed to send transcription result")]
    Sync(#[from] Box<SendError<TranscribeResult>>),

    #[error("Whisper error: {0}")]
    Whisper(#[from] WhisperError),
}

#[derive(Debug)]
pub struct Job {
    audio: Vec<f32>,
    strategy: whisper_rs::SamplingStrategy,
    sample_rate: u32,
    prompt: Option<String>,
}

impl Job {
    pub fn new(
        audio: Vec<f32>,
        strategy: whisper_rs::SamplingStrategy,
        sample_rate: u32,
        prompt: Option<String>,
    ) -> Self {
        Self {
            audio,
            strategy,
            sample_rate,
            prompt,
        }
    }

    #[allow(clippy::cast_sign_loss)]
    #[allow(clippy::cast_precision_loss)]
    pub fn duration(&self) -> Duration {
        assert!(self.sample_rate > 0);
        let secs_int = self.audio.len() / self.sample_rate as usize;
        let rem = self.audio.len() % self.sample_rate as usize;
        Duration::from_secs(secs_int as u64)
            + Duration::from_secs_f32(rem as f32 / self.sample_rate as f32)
    }

    pub fn prompt(&self) -> Option<&str> {
        self.prompt.as_deref()
    }

    pub fn strategy(&self) -> whisper_rs::SamplingStrategy {
        self.strategy.clone()
    }

    pub fn audio(&self) -> &[f32] {
        &self.audio
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

pub fn parse_strategy(s: &str) -> Result<StrategyOpt, anyhow::Error> {
    let mut parts = s.split(':');
    let (kind, n) = (
        parts
            .next()
            .ok_or(anyhow!("strategy must be of the form 'qkind' or 'kind:n'"))?,
        parts.next().map(str::parse).transpose()?,
    );
    Ok(match kind {
        "greedy" => {
            let n = n.unwrap_or(2);
            if n < 1 {
                return Err(anyhow!("best_of must be at least 1"));
            }
            StrategyOpt::Greedy { best_of: n }
        }
        "beam" => {
            let n = n.unwrap_or(10);
            if n < 1 {
                return Err(anyhow!("beam_size must be at least 1"));
            }
            let patience = parts.next().map(str::parse).transpose()?.unwrap_or(0.0);
            StrategyOpt::Beam {
                beam_size: n,
                patience,
            }
        }
        _ => return Err(anyhow!("invalid strategy")),
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
