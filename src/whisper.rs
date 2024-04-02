use itertools::Itertools;
use whisper_rs::{FullParams, WhisperContext, WhisperError};

#[derive(Debug)]
pub struct Segment {
    start: usize,
    end: usize,
    text: String,
}

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

    pub fn transcribe_audio<T>(&self, data: T) -> Result<Vec<Segment>, WhisperError>
    where
        T: AsRef<[f32]>,
    {
        let mut state = self.create_state()?;

        let mut params = FullParams::new(whisper_rs::SamplingStrategy::Greedy { best_of: 1 });
        params.set_max_len(1);
        params.set_split_on_word(true);
        params.set_audio_ctx({
            let blen = data.as_ref().len();
            let audio_secs = blen as f32 / 16.0;
            if audio_secs > 30.0 {
                1500
            } else {
                (audio_secs as i32) / 30 * 1500 + 128
            }
        });

        match state.full(params, data.as_ref()) {
            Ok(0) => {}
            Ok(n) => return Err(WhisperError::GenericError(n)),
            Err(e) => return Err(e),
        };

        let segments = state.full_n_segments()?;

        (0..segments)
            .map(|n| {
                let start = state.full_get_segment_t0(n)?;
                let end = state.full_get_segment_t1(n)?;

                let token_segs: Vec<Segment> = {
                    let mut out = Vec::new();
                    let n_tokens = state.full_n_tokens(n)? as i64;
                    // TODO fix this by filtering out meta first and collecting, then we have the
                    // actual n_tokens value to do math with.
                    let non_meta_tokens = (0..n_tokens)
                        .filter(|i| {
                            let text = state.full_get_token_text(n, *i as i32).unwrap();
                            !text.starts_with("[_")
                        })
                        .enumerate()
                        .collect_vec();
                    let n_non_meta_tokens = non_meta_tokens.len() as i64;

                    for (i, original_i) in non_meta_tokens {
                        let i = i as i64;
                        let text = state.full_get_token_text(n, original_i as i32)?;
                        // we're evenly spacing t0 and t1 for each token in the segment
                        let t0 = start + (end - start) * i / n_non_meta_tokens;
                        let t1 = start + (end - start) * (i + 1) / n_non_meta_tokens;
                        out.push(Segment {
                            start: t0.try_into().unwrap(),
                            end: t1.try_into().unwrap(),
                            text,
                        });
                    }
                    out
                };

                Ok(token_segs)
            })
            .flatten_ok()
            .collect()
    }
}
