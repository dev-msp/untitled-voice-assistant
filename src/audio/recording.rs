use std::{
    fmt::{Debug, Display},
    sync::Arc,
    thread,
};

use anyhow::anyhow;
use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    StreamConfig,
};
use itertools::Itertools;
use serde::{Deserialize, Serialize};

use super::{
    controller::Controller,
    process::{read_from_device, Processor},
    MySample,
};
use crate::audio::controller::RecordState;

#[derive(Debug, Default, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Session {
    input_device: Option<String>,
    sample_rate: Option<u32>,
    prompt: Option<String>,
}

impl Session {
    pub fn new(
        input_device: Option<String>,
        sample_rate: Option<u32>,
        prompt: Option<String>,
    ) -> Self {
        Self {
            input_device,
            sample_rate,
            prompt,
        }
    }

    pub fn device_name(&self) -> Option<&str> {
        self.input_device.as_deref()
    }

    pub fn sample_rate(&self) -> Option<u32> {
        self.sample_rate
    }

    pub fn supported_configs(
        &self,
    ) -> Result<
        impl Iterator<Item = (Arc<cpal::Device>, cpal::SupportedStreamConfig)> + '_,
        anyhow::Error,
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
                Err(e) => Err(e),
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

    pub fn prompt(&self) -> Option<&str> {
        self.prompt.as_deref()
    }
}

pub struct Recording<S, RS, RE = anyhow::Error>
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

#[derive(Debug, thiserror::Error)]
pub enum Error<E>
where
    E: Display + Send + Sync + 'static,
{
    #[error("failed to join recording thread")]
    Sync,

    #[error("{0}")]
    Other(E),
}

impl<S, RS, RE> Recording<S, RS, RE>
where
    S: MySample,
    RS: Send + Into<Vec<f32>>,
    RE: Send + Sync + Display + Debug,
{
    #[tracing::instrument(skip(self))]
    pub fn start(&self) {
        self.controller.start();
        self.controller.wait_for(RecordState::Recording);
        log::info!("Recording started");
    }

    pub fn stop(self) -> Result<(StreamConfig, Vec<f32>), Error<RE>> {
        self.controller.stop();
        let metadata = Self::join_handle(self.handle)
            .map_err(|e| {
                log::error!("Error joining recording thread: {}", e);
                Error::<RE>::Sync
            })
            .unwrap()
            .map_err(|e| {
                log::error!("Error joining recording thread: {}", e);
                Error::Other(e)
            })?;

        let audio = Self::join_handle(self.receiving_handle)?.into();
        Ok((metadata, audio))
    }

    fn join_handle<T>(handle: thread::JoinHandle<T>) -> Result<T, Error<RE>> {
        match handle.join() {
            Ok(inner) => Ok(inner),
            Err(e) => {
                let inner: Box<RE> = e.downcast::<RE>().map_err(|_| Error::Sync)?;
                Err(Error::Other(*inner))
            }
        }
    }

    pub fn controlled(session: Session, node: crate::sync::ProcessNode<S, RS>) -> Recording<S, RS>
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
                    .ok_or(anyhow!("no supported input configuration"))?;

                c2.wait_for(RecordState::Started);

                let cpal::SampleFormat::F32 = supported_config.sample_format() else {
                    panic!("unsupported sample format");
                };

                let cfg: cpal::StreamConfig = supported_config.clone().into();
                let stream = if cfg.channels == 1 {
                    let resampler = Processor::<S, 1>::new(sink_send, cfg.clone(), 512);
                    read_from_device(resampler, &device)
                } else if cfg.channels == 2 {
                    let resampler = Processor::<S, 2>::new(sink_send, cfg.clone(), 512);
                    read_from_device(resampler, &device)
                } else {
                    panic!("unsupported channel count: {}", cfg.channels);
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

        Recording {
            handle,
            controller,
            phantom: std::marker::PhantomData,
            receiving_handle: sink_handle,
        }
    }
}
