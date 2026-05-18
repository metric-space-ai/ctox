//! Audio preprocessing port from voxtral.cpp with reusable FFT planning.

use crate::consts::{
    VOX_GLOBAL_LOG_MEL_MAX, VOX_HOP_LENGTH, VOX_NUM_MEL_BINS, VOX_RAW_AUDIO_LENGTH_PER_TOK,
    VOX_SAMPLE_RATE, VOX_WINDOW_SIZE,
};
use crate::{Error, Result};
use rustfft::num_complex::Complex32;
use rustfft::{Fft, FftPlanner};
use std::f32::consts::PI;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct WavData {
    pub sample_rate: usize,
    pub channels: usize,
    pub samples: Vec<f32>,
}

pub fn parse_wav(data: &[u8]) -> Result<WavData> {
    if data.len() < 44 || &data[0..4] != b"RIFF" || &data[8..12] != b"WAVE" {
        return Err(Error::InvalidFormat("not a RIFF/WAVE file"));
    }
    let mut pos = 12usize;
    let mut sample_rate = 0usize;
    let mut channels = 0usize;
    let mut bits_per_sample = 0usize;
    let mut audio_format = 0usize;
    let mut data_chunk: Option<&[u8]> = None;

    while pos + 8 <= data.len() {
        let id = &data[pos..pos + 4];
        let size = u32::from_le_bytes([data[pos + 4], data[pos + 5], data[pos + 6], data[pos + 7]])
            as usize;
        pos += 8;
        if pos + size > data.len() {
            return Err(Error::InvalidFormat("WAV chunk exceeds file"));
        }
        match id {
            b"fmt " => {
                if size < 16 {
                    return Err(Error::InvalidFormat("fmt chunk too short"));
                }
                audio_format = u16::from_le_bytes([data[pos], data[pos + 1]]) as usize;
                channels = u16::from_le_bytes([data[pos + 2], data[pos + 3]]) as usize;
                sample_rate = u32::from_le_bytes([
                    data[pos + 4],
                    data[pos + 5],
                    data[pos + 6],
                    data[pos + 7],
                ]) as usize;
                bits_per_sample = u16::from_le_bytes([data[pos + 14], data[pos + 15]]) as usize;
            }
            b"data" => data_chunk = Some(&data[pos..pos + size]),
            _ => {}
        }
        pos += size + (size & 1);
    }

    let pcm = data_chunk.ok_or(Error::InvalidFormat("WAV has no data chunk"))?;
    if channels == 0 || sample_rate == 0 {
        return Err(Error::InvalidFormat("WAV audio format is incomplete"));
    }

    let mono = match (audio_format, bits_per_sample) {
        (1, 16) => pcm16_to_mono(pcm, channels)?,
        (3, 32) => f32_to_mono(pcm, channels)?,
        _ => return Err(Error::Unsupported("only PCM16 or float32 WAV is supported")),
    };
    let samples = if sample_rate == VOX_SAMPLE_RATE {
        mono
    } else {
        linear_resample(&mono, sample_rate, VOX_SAMPLE_RATE)
    };
    Ok(WavData {
        sample_rate: VOX_SAMPLE_RATE,
        channels: 1,
        samples,
    })
}

fn pcm16_to_mono(data: &[u8], channels: usize) -> Result<Vec<f32>> {
    if data.len() % (2 * channels) != 0 {
        return Err(Error::InvalidFormat(
            "PCM16 data length is not frame aligned",
        ));
    }
    let frames = data.len() / (2 * channels);
    let mut mono = Vec::with_capacity(frames);
    for frame in 0..frames {
        let mut acc = 0.0f32;
        for ch in 0..channels {
            let i = (frame * channels + ch) * 2;
            acc += i16::from_le_bytes([data[i], data[i + 1]]) as f32 / 32768.0;
        }
        mono.push(acc / channels as f32);
    }
    Ok(mono)
}

fn f32_to_mono(data: &[u8], channels: usize) -> Result<Vec<f32>> {
    if data.len() % (4 * channels) != 0 {
        return Err(Error::InvalidFormat(
            "float32 data length is not frame aligned",
        ));
    }
    let frames = data.len() / (4 * channels);
    let mut mono = Vec::with_capacity(frames);
    for frame in 0..frames {
        let mut acc = 0.0f32;
        for ch in 0..channels {
            let i = (frame * channels + ch) * 4;
            acc += f32::from_le_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]]);
        }
        mono.push(acc / channels as f32);
    }
    Ok(mono)
}

pub fn linear_resample(input: &[f32], src_rate: usize, dst_rate: usize) -> Vec<f32> {
    if input.is_empty() || src_rate == dst_rate {
        return input.to_vec();
    }
    let out_len = ((input.len() as u128 * dst_rate as u128 + src_rate as u128 / 2)
        / src_rate as u128) as usize;
    let scale = src_rate as f64 / dst_rate as f64;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let src_pos = i as f64 * scale;
        let idx = src_pos.floor() as usize;
        let frac = (src_pos - idx as f64) as f32;
        let a = input.get(idx).copied().unwrap_or(0.0);
        let b = input.get(idx + 1).copied().unwrap_or(a);
        out.push(a * (1.0 - frac) + b * frac);
    }
    out
}

pub fn pad_audio_streaming(samples: &[f32], left_tokens: usize, right_tokens: usize) -> Vec<f32> {
    let align_pad = (VOX_RAW_AUDIO_LENGTH_PER_TOK - (samples.len() % VOX_RAW_AUDIO_LENGTH_PER_TOK))
        % VOX_RAW_AUDIO_LENGTH_PER_TOK;
    let left = left_tokens * VOX_RAW_AUDIO_LENGTH_PER_TOK;
    let right = align_pad + right_tokens * VOX_RAW_AUDIO_LENGTH_PER_TOK;
    let mut out = vec![0.0f32; left + samples.len() + right];
    out[left..left + samples.len()].copy_from_slice(samples);
    out
}

pub fn mel_filter_bank() -> Vec<f32> {
    let n_freq = VOX_WINDOW_SIZE / 2 + 1;
    let n_mel = VOX_NUM_MEL_BINS;
    let mut filters = vec![0.0f32; n_freq * n_mel];
    let fft_freqs = (0..n_freq)
        .map(|i| (VOX_SAMPLE_RATE as f32 / 2.0) * i as f32 / (n_freq - 1) as f32)
        .collect::<Vec<_>>();
    let mel_min = hertz_to_mel(0.0);
    let mel_max = hertz_to_mel(8000.0);
    let mel_pts = (0..n_mel + 2)
        .map(|i| mel_min + (mel_max - mel_min) * i as f32 / (n_mel + 1) as f32)
        .collect::<Vec<_>>();
    let filter_freqs = mel_pts.into_iter().map(mel_to_hertz).collect::<Vec<_>>();
    for m in 0..n_mel {
        let f_left = filter_freqs[m];
        let f_center = filter_freqs[m + 1];
        let f_right = filter_freqs[m + 2];
        let enorm = 2.0 / (f_right - f_left);
        for (k, f) in fft_freqs.iter().copied().enumerate() {
            let down = (f - f_left) / (f_center - f_left);
            let up = (f_right - f) / (f_right - f_center);
            filters[k * n_mel + m] = down.min(up).max(0.0) * enorm;
        }
    }
    filters
}

#[derive(Clone)]
pub struct MelSpectrogramPlan {
    sparse_mel_filters: Vec<Vec<(usize, f32)>>,
    window: Vec<f32>,
    fft: Arc<dyn Fft<f32>>,
}

impl std::fmt::Debug for MelSpectrogramPlan {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MelSpectrogramPlan")
            .field("sparse_mel_filter_rows", &self.sparse_mel_filters.len())
            .field("window_len", &self.window.len())
            .finish_non_exhaustive()
    }
}

impl Default for MelSpectrogramPlan {
    fn default() -> Self {
        Self::new(mel_filter_bank())
    }
}

impl MelSpectrogramPlan {
    pub fn new(mel_filters: Vec<f32>) -> Self {
        let mut planner = FftPlanner::<f32>::new();
        Self {
            sparse_mel_filters: sparse_mel_filters(&mel_filters),
            window: hann_periodic(VOX_WINDOW_SIZE),
            fft: planner.plan_fft_forward(VOX_WINDOW_SIZE),
        }
    }

    pub fn compute(&self, samples: &[f32]) -> Vec<f32> {
        compute_mel_spectrogram_with_plan(
            samples,
            &self.sparse_mel_filters,
            &self.window,
            &self.fft,
        )
    }
}

pub fn compute_mel_spectrogram(samples: &[f32], mel_filters: &[f32]) -> Vec<f32> {
    let plan = MelSpectrogramPlan::new(mel_filters.to_vec());
    plan.compute(samples)
}

fn compute_mel_spectrogram_with_plan(
    samples: &[f32],
    sparse_mel_filters: &[Vec<(usize, f32)>],
    window: &[f32],
    fft: &Arc<dyn Fft<f32>>,
) -> Vec<f32> {
    let n_fft = VOX_WINDOW_SIZE;
    let n_freq = VOX_WINDOW_SIZE / 2 + 1;
    let frames = samples.len() / VOX_HOP_LENGTH;
    let mut out = vec![0.0f32; VOX_NUM_MEL_BINS * frames];
    let mut padded = vec![0.0f32; samples.len() + n_fft];
    let pad = n_fft / 2;
    if !samples.is_empty() {
        for (i, sample) in padded.iter_mut().enumerate() {
            let src = i as isize - pad as isize;
            *sample = samples[reflect_index(src, samples.len())];
        }
    }
    let mut power = vec![0.0f32; n_freq];
    let mut mel_accum = vec![0.0f32; VOX_NUM_MEL_BINS];
    let mut spectrum = vec![Complex32::new(0.0, 0.0); n_fft];

    for frame in 0..frames {
        let start = frame * VOX_HOP_LENGTH;
        for n in 0..n_fft {
            spectrum[n] = Complex32::new(
                padded.get(start + n).copied().unwrap_or(0.0) * window[n],
                0.0,
            );
        }
        fft.process(&mut spectrum);
        for k in 0..n_freq {
            power[k] = spectrum[k].norm_sqr();
        }
        mel_accum.fill(0.0);
        for (k, row) in sparse_mel_filters.iter().enumerate() {
            let p = power[k];
            for &(m, weight) in row {
                mel_accum[m] += weight * p;
            }
        }
        for m in 0..VOX_NUM_MEL_BINS {
            let mut value = mel_accum[m].max(1e-10).log10();
            value = value.max(VOX_GLOBAL_LOG_MEL_MAX - 8.0);
            out[m * frames + frame] = (value + 4.0) / 4.0;
        }
    }
    out
}

fn reflect_index(mut idx: isize, len: usize) -> usize {
    if len <= 1 {
        return 0;
    }
    let len = len as isize;
    while idx < 0 || idx >= len {
        if idx < 0 {
            idx = -idx;
        } else {
            idx = 2 * len - 2 - idx;
        }
    }
    idx as usize
}

fn sparse_mel_filters(mel_filters: &[f32]) -> Vec<Vec<(usize, f32)>> {
    let n_freq = VOX_WINDOW_SIZE / 2 + 1;
    let mut rows = vec![Vec::new(); n_freq];
    for k in 0..n_freq {
        for m in 0..VOX_NUM_MEL_BINS {
            let weight = mel_filters[k * VOX_NUM_MEL_BINS + m];
            if weight != 0.0 {
                rows[k].push((m, weight));
            }
        }
    }
    rows
}

fn hann_periodic(n: usize) -> Vec<f32> {
    (0..n)
        .map(|i| 0.5 * (1.0 - (2.0 * PI * i as f32 / n as f32).cos()))
        .collect()
}

fn hertz_to_mel(freq_hz: f32) -> f32 {
    let min_log_hertz = 1000.0;
    let min_log_mel = 15.0;
    let logstep = 27.0 / 6.4_f32.ln();
    if freq_hz >= min_log_hertz {
        min_log_mel + (freq_hz / min_log_hertz).ln() * logstep
    } else {
        3.0 * freq_hz / 200.0
    }
}

fn mel_to_hertz(mels: f32) -> f32 {
    let min_log_hertz = 1000.0;
    let min_log_mel = 15.0;
    let logstep = 6.4_f32.ln() / 27.0;
    if mels >= min_log_mel {
        min_log_hertz * ((mels - min_log_mel) * logstep).exp()
    } else {
        200.0 * mels / 3.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn streaming_padding_aligns_to_audio_token() {
        let samples = vec![1.0; 1600];
        let padded = pad_audio_streaming(&samples, 32, 17);
        assert_eq!(padded.len() % VOX_RAW_AUDIO_LENGTH_PER_TOK, 0);
        assert_eq!(padded[32 * VOX_RAW_AUDIO_LENGTH_PER_TOK], 1.0);
    }

    #[test]
    fn mel_filter_shape_matches_voxtral() {
        assert_eq!(
            mel_filter_bank().len(),
            (VOX_WINDOW_SIZE / 2 + 1) * VOX_NUM_MEL_BINS
        );
    }

    #[test]
    fn mel_spectrogram_has_debug_performance_budget() {
        let samples = vec![0.0f32; VOX_SAMPLE_RATE * 10];
        let plan = MelSpectrogramPlan::default();
        let start = Instant::now();
        let mel = plan.compute(&samples);
        let elapsed = start.elapsed();
        assert!(!mel.is_empty());
        assert!(
            elapsed < Duration::from_secs(2),
            "10s mel preprocessing took {elapsed:?}, which is too slow for meeting STT"
        );
    }
}
