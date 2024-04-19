use anyhow::anyhow;
use std::{thread::JoinHandle, time::Duration};

use crossbeam::channel::{unbounded, Receiver, SendError};
use itertools::Itertools;
use sttx::Timing;
use whisper_rs::{FullParams, WhisperContext, WhisperError};

pub struct Whisper {
    context: WhisperContext,
}

impl Whisper {
    pub fn new(model_path: &str) -> Result<Self, WhisperError> {
        let mut params = whisper_rs::WhisperContextParameters::default();
        params.use_gpu(true);
        let context = whisper_rs::WhisperContext::new_with_params(model_path, params)?;
        Ok(Self { context })
    }

    pub fn create_state(&self) -> Result<whisper_rs::WhisperState, WhisperError> {
        self.context.create_state()
    }

    pub fn transcribe_audio(
        &self,
        job: TranscriptionJob,
    ) -> Result<Vec<sttx::Timing>, WhisperError> {
        let mut state = self.create_state()?;

        let mut params = FullParams::new(job.strategy);
        params.set_token_timestamps(true);
        params.set_max_len(1);
        params.set_split_on_word(true);

        match state.full(params, job.audio.as_slice()) {
            Ok(0) => {}
            Ok(n) => return Err(WhisperError::GenericError(n)),
            Err(e) => return Err(e),
        };

        let segments = state.full_n_segments()?;

        (0..segments)
            .map(|n| {
                // let start = state.full_get_segment_t0(n)?;
                // let end = state.full_get_segment_t1(n)?;

                let token_segs: Vec<Timing> = {
                    let mut out = Vec::new();
                    for i in 0..state.full_n_tokens(n)? {
                        let text = state.full_get_token_text(n, i)?;
                        if text.starts_with("[_") {
                            continue;
                        }

                        let data = state.full_get_token_data(n, i)?;

                        // we're evenly spacing t0 and t1 for each token in the segment
                        let t0 = 10 * data.t0;
                        let t1 = 10 * data.t1;
                        out.push(Timing::new(
                            t0.try_into().unwrap(),
                            t1.try_into().unwrap(),
                            text,
                        ));
                    }
                    out
                };

                Ok(token_segs)
            })
            .flatten_ok()
            .collect()
    }
}

type TranscribeResult = Result<Vec<sttx::Timing>, TranscriptionError>;

#[derive(Debug, thiserror::Error)]
pub enum TranscriptionError {
    #[error("Failed to send transcription result")]
    Sync(#[from] Box<SendError<TranscribeResult>>),

    #[error("Whisper error: {0}")]
    Whisper(#[from] WhisperError),
}

type WorkerHandle = (
    Receiver<TranscribeResult>,
    JoinHandle<Result<(), TranscriptionError>>,
);

#[derive(Debug)]
pub struct TranscriptionJob {
    audio: Vec<f32>,
    strategy: whisper_rs::SamplingStrategy,
    sample_rate: u32,
}

impl TranscriptionJob {
    pub fn new(audio: Vec<f32>, strategy: whisper_rs::SamplingStrategy, sample_rate: u32) -> Self {
        Self {
            audio,
            strategy,
            sample_rate,
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
}

pub fn transcription_worker(
    model: &str,
    jobs: Receiver<TranscriptionJob>,
) -> Result<WorkerHandle, anyhow::Error> {
    let (snd, recv) = unbounded();
    let whisper = Whisper::new(model)?;

    Ok((
        recv,
        std::thread::spawn(move || {
            for job in &jobs {
                log::debug!("Transcribing audio with duration: {:?}", job.duration());
                let results = whisper
                    .transcribe_audio(job)
                    .map_err(TranscriptionError::from);
                snd.send(results).map_err(Box::new)?;
            }
            Ok(())
        }),
    ))
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
