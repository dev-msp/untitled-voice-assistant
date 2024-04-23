pub mod transcription;

use std::{
    collections::{hash_map::Entry, HashMap},
    path::Path,
    thread::JoinHandle,
};

use crossbeam::channel::{unbounded, Receiver};
use itertools::Itertools;
use sttx::Timing;
use whisper_rs::{FullParams, WhisperContext, WhisperError};

use self::transcription::{Model, TranscribeResult};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("lib error: {0}")]
    Lib(#[from] WhisperError),

    #[error("transcription error: {0}")]
    Transcription(#[from] transcription::Error),

    #[error("Send error")]
    Send,

    #[error("Try to eliminate this")]
    CatchAll,
}

impl<T> From<crossbeam::channel::SendError<T>> for Error {
    fn from(_: crossbeam::channel::SendError<T>) -> Self {
        Self::Send
    }
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

    /// # Panics
    /// Panics if the strategy is not supported by the model
    pub fn transcribe_audio(
        &self,
        job: &transcription::Job,
    ) -> Result<Vec<sttx::Timing>, WhisperError> {
        let mut state = self.create_state()?;

        let mut params = FullParams::new(job.strategy());
        params.set_token_timestamps(true);
        params.set_max_len(1);
        params.set_split_on_word(true);
        if let Some(prompt) = job.prompt() {
            log::debug!("Setting initial prompt: {:?}", prompt);
            params.set_initial_prompt(prompt);
        }

        match state.full(params, job.audio()) {
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
                        if is_internal_token(text.as_str()) {
                            continue;
                        }

                        let data = state.full_get_token_data(n, i)?;

                        // we're evenly spacing t0 and t1 for each token in the segment
                        let t0 = 10 * data.t0;
                        let t1 = 10 * data.t1;
                        assert!(t0 >= 0);
                        assert!(t1 >= 0);

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

type WorkerHandle = (Receiver<TranscribeResult>, JoinHandle<Result<(), Error>>);

/// # Panics
///
/// Thread will panic if the Whisper instance for the given model is missing even after checking
/// for its existence.
pub fn transcription_worker(
    model_base_dir: &Path,
    jobs: Receiver<transcription::Job>,
) -> Result<WorkerHandle, Error> {
    let (snd, recv) = unbounded();

    let mut whispers: HashMap<Model, Whisper> = HashMap::new();
    let base_dir = model_base_dir.to_owned();

    Ok((
        recv,
        std::thread::spawn(move || {
            for job in &jobs {
                log::debug!("Transcribing audio with duration: {:?}", job.duration());
                let entry = whispers.entry(job.model());
                if let Entry::Vacant(e) = entry {
                    log::info!("Creating new whisper instance for model: {:?}", job.model());
                    // &job.model().filename()
                    let model_path = base_dir.join(job.model().filename());
                    e.insert(Whisper::new(
                        model_path.to_string_lossy().to_string().as_str(),
                    )?);
                };
                let whisper = whispers.get(&job.model()).expect("Whisper not found");
                let results = whisper
                    .transcribe_audio(&job)
                    .map_err(transcription::Error::from);
                snd.send(results)?;
            }
            Ok(())
        }),
    ))
}

fn is_internal_token(text: &str) -> bool {
    text.starts_with("[_") || (text.starts_with("<|") && text.ends_with("|>"))
}
