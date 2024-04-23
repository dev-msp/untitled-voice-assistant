use std::{
    fmt::{Debug, Display},
    sync::Arc,
    thread,
};

use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    StreamConfig,
};
use itertools::Itertools;
use serde::{Deserialize, Serialize};

use super::{
    controller::Controller,
    process::{self, read_from_device, Processor},
    MySample,
};
use crate::{audio::controller::RecordState, whisper::transcription::Model};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("no supported configs found")]
    NoSupportedConfigs,

    #[error("unsupported channel count: {0}")]
    InvalidChannelCount(u16),

    #[error("invalid sample format: {0:?}")]
    InvalidSampleFormat(cpal::SampleFormat),

    #[error("play stream error: {0}")]
    PlayStream(#[from] cpal::PlayStreamError),

    #[error("session error: {0}")]
    Session(#[from] SessionError),

    #[error("process error: {0}")]
    Process(#[from] process::Error),

    #[error("failed to join recording thread")]
    Sync,

    #[error("try to eliminate this")]
    CatchAll,
}

pub struct Recording<S, RS, RE = Error>
where
    S: MySample,
    RS: Send,
    RE: Send,
{
    handle: thread::JoinHandle<Result<StreamConfig, RE>>,
    controller: Controller,
    phantom: std::marker::PhantomData<S>,
    receiving_handle: thread::JoinHandle<RS>,
}

impl<S, RS, RE> Recording<S, RS, RE>
where
    S: MySample,
    RS: Send + Into<Vec<f32>>,
    RE: Debug + Display + Send + 'static,
{
    #[tracing::instrument(skip(self))]
    pub fn start(&self) {
        self.controller.start();
        self.controller.wait_for(RecordState::Recording);
        log::info!("Recording started");
    }

    pub fn stop(self) -> Result<(StreamConfig, Vec<f32>), Error> {
        self.controller.stop();
        let metadata = Self::join_handle(self.handle)
            .map_err(|e| {
                log::error!("Error joining recording thread: {}", e);
                Error::Sync
            })?
            .map_err(|e| {
                log::error!("Error completing recording: {}", e);
                Error::CatchAll
            })?;

        let audio = Self::join_handle(self.receiving_handle)?.into();
        Ok((metadata, audio))
    }

    fn join_handle<T>(handle: thread::JoinHandle<T>) -> Result<T, Error> {
        match handle.join() {
            Ok(inner) => Ok(inner),
            Err(e) => {
                let inner: Box<RE> = e.downcast::<RE>().map_err(|_| Error::Sync)?;
                log::error!("Error joining thread: {}", inner);
                Err(Error::CatchAll)
            }
        }
    }

    pub fn controlled(
        session: Session,
        node: crate::sync::ProcessNode<S, RS>,
    ) -> Result<Recording<S, RS, Error>, Error>
    where
        S: MySample,
        RS: Send + 'static,
    {
        let controller = Controller::new();

        let c2 = controller.clone();

        let (sink_send, sink_handle) = node.run();

        let handle = thread::spawn(move || {
            {
                let (device, supported_config) = session
                    .supported_configs()?
                    .next()
                    .ok_or(Error::NoSupportedConfigs)?;

                c2.wait_for(RecordState::Started);

                let cpal::SampleFormat::F32 = supported_config.sample_format() else {
                    return Err(Error::InvalidSampleFormat(supported_config.sample_format()));
                };

                let cfg: cpal::StreamConfig = supported_config.clone().into();
                let stream = if cfg.channels == 1 {
                    let resampler = Processor::<S, 1>::new(sink_send, cfg.clone());
                    read_from_device(resampler, &device).map_err(Error::from)
                } else if cfg.channels == 2 {
                    let resampler = Processor::<S, 2>::new(sink_send, cfg.clone());
                    read_from_device(resampler, &device).map_err(Error::from)
                } else {
                    Err(Error::InvalidChannelCount(cfg.channels))
                }?;

                stream.play()?;
                c2.recording();

                c2.wait_for(RecordState::Stopped);
                Ok(cfg)
            }
            .map_err(|e| {
                log::error!("Error attempting to record from input device: {}", e);
                e
            })
        });

        Ok(Recording {
            handle,
            controller,
            phantom: std::marker::PhantomData,
            receiving_handle: sink_handle,
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("audio device error: {0}")]
    Device(#[from] cpal::DevicesError),

    #[error("recording session parameters error: {0}")]
    Parameters(String),
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Session {
    input_device: Option<String>,
    sample_rate: Option<u32>,
    prompt: Option<String>,
    model: Option<Model>,
}

impl Session {
    #[must_use]
    pub fn new(
        input_device: Option<String>,
        sample_rate: Option<u32>,
        prompt: Option<String>,
        model: Option<Model>,
    ) -> Self {
        Self {
            input_device,
            sample_rate,
            prompt,
            model,
        }
    }

    #[must_use]
    pub fn device_name(&self) -> Option<&str> {
        self.input_device.as_deref()
    }

    #[must_use]
    pub fn sample_rate(&self) -> Option<u32> {
        self.sample_rate
    }

    #[must_use]
    pub fn model(&self) -> Option<Model> {
        self.model
    }

    pub fn supported_configs(
        &self,
    ) -> Result<
        impl Iterator<Item = (Arc<cpal::Device>, cpal::SupportedStreamConfig)> + '_,
        SessionError,
    > {
        let devices = cpal::default_host()
            .input_devices()?
            .filter_map(|x| {
                let name = x.name().ok()?;
                let Some(pat) = self.device_name() else {
                    return Some(x);
                };
                name.contains(pat).then_some(x)
            })
            .map(Arc::new)
            .collect_vec();

        let device_config_pairs = devices
            .into_iter()
            .map(|d| match d.supported_input_configs() {
                Ok(cfgs) => Ok(cfgs.map(move |cfg| (d.clone(), cfg))),
                Err(e) => {
                    log::error!("Error getting supported input configs: {}", e);
                    Err(e)
                }
            })
            .flatten_ok();

        Ok(device_config_pairs.filter_map(|r| {
            let Ok((d, c)) = r else {
                return None;
            };
            let sample_rate = self.sample_rate().map_or_else(
                || c.min_sample_rate().max(cpal::SampleRate(16000)),
                cpal::SampleRate,
            );
            if c.min_sample_rate() > sample_rate || c.max_sample_rate() < sample_rate {
                None
            } else {
                let min_sample_rate = c.min_sample_rate();
                Some((d, c.with_sample_rate(min_sample_rate)))
            }
        }))
    }

    #[must_use]
    pub fn prompt(&self) -> Option<&str> {
        self.prompt.as_deref()
    }
}
