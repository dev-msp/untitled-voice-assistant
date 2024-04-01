use std::{
    iter::{repeat, Chain, Cloned, Repeat, Take},
    slice::Iter,
    time::Duration,
};

use thiserror::Error;
use webrtc_vad::{Vad, VadMode as BadVadMode};

#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
pub enum SampleSize {
    Small = 10,
    Medium = 20,
    Large = 30,
}

#[derive(Debug, Clone, Copy)]
pub enum VadMode {
    Quality = 0,
    LowBitrate = 1,
    Aggressive = 2,
    VeryAggressive = 3,
}

impl From<VadMode> for BadVadMode {
    fn from(value: VadMode) -> Self {
        match value {
            VadMode::Quality => BadVadMode::Quality,
            VadMode::LowBitrate => BadVadMode::LowBitrate,
            VadMode::Aggressive => BadVadMode::Aggressive,
            VadMode::VeryAggressive => BadVadMode::VeryAggressive,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Config {
    mode: VadMode,
    sample_rate: i32,

    sample_size: SampleSize,
    resolution: Duration,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            mode: VadMode::Quality,
            sample_rate: 16000,

            sample_size: SampleSize::Medium,
            resolution: Duration::from_millis(200),
        }
    }
}

impl Config {
    pub fn samples_per_frame(&self) -> usize {
        self.resolution.as_millis() as usize / self.sample_size as usize
    }
}

impl TryFrom<&Config> for Vad {
    type Error = Error;

    fn try_from(cfg: &Config) -> Result<Self, Self::Error> {
        Ok(Vad::new_with_rate_and_mode(
            cfg.sample_rate.try_into().or(Err(Error::BadSampleRate))?,
            cfg.mode.into(),
        ))
    }
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("problem detecting voice activity")]
    Vad,

    #[error("couldn't convert the sample rate into an acceptable value")]
    BadSampleRate,
}

pub struct Buffer<'a> {
    data: &'a [i16],
    size: usize,
    index: usize,
}

impl Buffer<'_> {
    fn size(&self) -> usize {
        self.size
    }

    fn is_voice_segment(&self, vad: &mut Vad) -> Result<bool, Error> {
        vad.is_voice_segment(self.as_ref()).or(Err(Error::Vad))
    }
}

impl Config {
    pub fn buffer_size(&self) -> usize {
        (self.sample_size as u32 * self.sample_rate as u32 / 1000) as usize
    }

    pub fn buffer_from<'a>(&self, index: usize, data: &'a [i16]) -> Buffer<'a> {
        Buffer {
            index,
            data,
            size: self.buffer_size(),
        }
    }
}

impl<'a> AsRef<[i16]> for Buffer<'a> {
    fn as_ref(&self) -> &'a [i16] {
        let start = self.index * self.size();
        let end = start + self.size();
        self.data[start..end].as_ref()
    }
}

impl<'a> IntoIterator for Buffer<'a> {
    type Item = i16;
    type IntoIter = Chain<Cloned<Iter<'a, i16>>, Take<Repeat<i16>>>;

    fn into_iter(self) -> Self::IntoIter {
        let fill = self.size() - self.data.len();
        self.data.iter().cloned().chain(repeat(0).take(fill))
    }
}

// Given a configuration value, produce a sequence of values corresponding to a rough level of
// confidence in whether the underlying audio segment contains speech
impl Config {
    pub fn detect_voice<B: AsRef<[i16]>>(&self, input: B) -> Result<Vec<usize>, Error> {
        let mut vad: Vad = self.try_into()?;
        let data = input.as_ref();

        let output: Result<Vec<_>, _> = (0..data.len() / self.buffer_size())
            .map(|i| self.buffer_from(i, data).is_voice_segment(&mut vad))
            .collect();

        Ok(output?
            .chunks(self.resolution.as_millis() as usize / self.sample_size as usize)
            .map(|chk| chk.iter().cloned().filter(|x| *x).count())
            .collect::<Vec<_>>())
    }
}
