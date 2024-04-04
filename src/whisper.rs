use std::{
    sync::mpsc::{Receiver, SendError},
    thread::JoinHandle,
};

use itertools::Itertools;
use sttx::Timing;
use whisper_rs::{convert_integer_to_float_audio, FullParams, WhisperContext, WhisperError};

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

    pub fn transcribe_audio<T>(&self, data: T) -> Result<Vec<sttx::Timing>, WhisperError>
    where
        T: AsRef<[f32]>,
    {
        let mut state = self.create_state()?;

        let mut params = FullParams::new(whisper_rs::SamplingStrategy::Greedy { best_of: 2 });
        // params.set_audio_ctx({
        //     let blen = data.as_ref().len();
        //     let audio_secs = blen as f32 / 16000.0;
        //     eprintln!("audio_secs: {}", audio_secs);
        //     if audio_secs > 30.0 {
        //         1500
        //     } else {
        //         ((audio_secs / 30.0 * 1500.0) as i32).max(128)
        //     }
        // });
        params.set_token_timestamps(true);
        params.set_max_len(1);
        params.set_split_on_word(true);

        match state.full(params, data.as_ref()) {
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
                    let n_tokens = state.full_n_tokens(n)? as i64;

                    for i in 0..n_tokens {
                        let text = state.full_get_token_text(n, i as i32)?;
                        if text.starts_with("[_") {
                            continue;
                        }

                        let data = state.full_get_token_data(n, i as i32)?;

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

pub fn transcription_worker(
    model: &str,
    jobs: Receiver<Vec<i16>>,
) -> Result<WorkerHandle, anyhow::Error> {
    let (snd, recv) = std::sync::mpsc::channel();
    let whisper = Whisper::new(model)?;

    Ok((
        recv,
        std::thread::spawn(move || {
            for audio in jobs.iter() {
                let mut audio_fl = vec![0_f32; audio.len()];
                convert_integer_to_float_audio(&audio, &mut audio_fl)?;
                let results = whisper
                    .transcribe_audio(audio_fl)
                    .map_err(TranscriptionError::from);
                snd.send(results).map_err(Box::new)?;
            }
            Ok(())
        }),
    ))
}
