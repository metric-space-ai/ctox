//! Audio preprocessing reference implementation.
//!
//! This is deliberately dependency-free and slow. It is meant as a correctness
//! anchor and as a target for platform kernels. For production throughput, move
//! STFT/mel into backend-specific kernels.

use crate::consts::{
    VOX_HOP_LENGTH, VOX_LOG_MEL_MAX, VOX_MEL_BINS, VOX_SAMPLE_RATE, VOX_WINDOW_SIZE,
};
use crate::{Error, Result};
use std::f32::consts::PI;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct WavData {
    pub sample_rate: usize,
    pub channels: usize,
    pub samples: Vec<f32>, // mono f32 in [-1,1]
}

pub fn load_wav_pcm16(path: impl AsRef<Path>) -> Result<WavData> {
    let data = fs::read(path)?;
    parse_wav_pcm16(&data)
}

pub fn parse_wav_pcm16(data: &[u8]) -> Result<WavData> {
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
        pos += size + (size & 1); // chunks are word-aligned
    }

    if audio_format != 1 || bits_per_sample != 16 || channels == 0 || sample_rate == 0 {
        return Err(Error::Unsupported(
            "only PCM16 WAV is supported in the no-deps reference loader",
        ));
    }
    let pcm = data_chunk.ok_or(Error::InvalidFormat("WAV has no data chunk"))?;
    if pcm.len() % (2 * channels) != 0 {
        return Err(Error::InvalidFormat("PCM data length not frame aligned"));
    }

    let frames = pcm.len() / (2 * channels);
    let mut mono = Vec::with_capacity(frames);
    for f in 0..frames {
        let mut acc = 0.0f32;
        for ch in 0..channels {
            let i = (f * channels + ch) * 2;
            let s = i16::from_le_bytes([pcm[i], pcm[i + 1]]) as f32 / 32768.0;
            acc += s;
        }
        mono.push(acc / channels as f32);
    }

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

pub fn log_mel_spectrogram(samples: &[f32]) -> Vec<f32> {
    let n_fft = VOX_WINDOW_SIZE;
    let hop = VOX_HOP_LENGTH;
    let pad = n_fft / 2; // center=True
    let frames = if samples.is_empty() {
        0
    } else {
        (samples.len() + 2 * pad - n_fft) / hop + 1
    };
    let n_freqs = n_fft / 2; // drop Nyquist bin, matching stft[..., :-1]
    let window = hann_periodic(n_fft);
    let mel = mel_filter_bank(VOX_MEL_BINS, n_freqs, n_fft, VOX_SAMPLE_RATE);
    let mut out = vec![0.0f32; VOX_MEL_BINS * frames];

    let mut power = vec![0.0f32; n_freqs];
    for frame in 0..frames {
        let start = frame * hop;
        for bin in 0..n_freqs {
            let mut re = 0.0f32;
            let mut im = 0.0f32;
            for n in 0..n_fft {
                let orig = start as isize + n as isize - pad as isize;
                let sample = sample_reflect(samples, orig) * window[n];
                let angle = -2.0 * PI * (bin as f32) * (n as f32) / n_fft as f32;
                let (sin, cos) = angle.sin_cos();
                re += sample * cos;
                im += sample * sin;
            }
            power[bin] = re * re + im * im;
        }

        for m in 0..VOX_MEL_BINS {
            let mut v = 0.0f32;
            let row = &mel[m * n_freqs..(m + 1) * n_freqs];
            for f in 0..n_freqs {
                v += row[f] * power[f];
            }
            let mut log_v = v.max(1e-10).log10();
            let low = VOX_LOG_MEL_MAX - 8.0;
            if log_v < low {
                log_v = low;
            }
            if log_v > VOX_LOG_MEL_MAX {
                log_v = VOX_LOG_MEL_MAX;
            }
            out[m * frames + frame] = (log_v + 4.0) / 4.0;
        }
    }
    out
}

fn sample_reflect(samples: &[f32], idx: isize) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let n = samples.len() as isize;
    let mut i = idx;
    while i < 0 || i >= n {
        if i < 0 {
            i = -i;
        }
        if i >= n {
            i = 2 * n - i - 2;
        }
        if n == 1 {
            return samples[0];
        }
    }
    samples[i as usize]
}

fn hann_periodic(n: usize) -> Vec<f32> {
    let mut w = Vec::with_capacity(n);
    for i in 0..n {
        w.push(0.5 - 0.5 * (2.0 * PI * i as f32 / n as f32).cos());
    }
    w
}

fn hz_to_mel_slaney(hz: f32) -> f32 {
    let f_sp = 200.0 / 3.0;
    let min_log_hz = 1000.0;
    let min_log_mel = min_log_hz / f_sp;
    let logstep = 6.4f32.ln() / 27.0;
    if hz < min_log_hz {
        hz / f_sp
    } else {
        min_log_mel + (hz / min_log_hz).ln() / logstep
    }
}

fn mel_to_hz_slaney(mel: f32) -> f32 {
    let f_sp = 200.0 / 3.0;
    let min_log_hz = 1000.0;
    let min_log_mel = min_log_hz / f_sp;
    let logstep = 6.4f32.ln() / 27.0;
    if mel < min_log_mel {
        mel * f_sp
    } else {
        min_log_hz * (logstep * (mel - min_log_mel)).exp()
    }
}

pub fn mel_filter_bank(
    n_mels: usize,
    n_freqs: usize,
    n_fft: usize,
    sample_rate: usize,
) -> Vec<f32> {
    let min_mel = hz_to_mel_slaney(0.0);
    let max_mel = hz_to_mel_slaney(sample_rate as f32 / 2.0);
    let mut mel_points = Vec::with_capacity(n_mels + 2);
    for i in 0..n_mels + 2 {
        let t = i as f32 / (n_mels + 1) as f32;
        mel_points.push(mel_to_hz_slaney(min_mel + t * (max_mel - min_mel)));
    }

    let mut filters = vec![0.0f32; n_mels * n_freqs];
    for m in 0..n_mels {
        let f_left = mel_points[m];
        let f_center = mel_points[m + 1];
        let f_right = mel_points[m + 2];
        let enorm = 2.0 / (f_right - f_left).max(1e-12);
        for f in 0..n_freqs {
            let hz = f as f32 * sample_rate as f32 / n_fft as f32;
            let lower = (hz - f_left) / (f_center - f_left).max(1e-12);
            let upper = (f_right - hz) / (f_right - f_center).max(1e-12);
            filters[m * n_freqs + f] = lower.min(upper).max(0.0) * enorm;
        }
    }
    filters
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resample_length() {
        let x = vec![0.0; 48_000];
        let y = linear_resample(&x, 48_000, 16_000);
        assert_eq!(y.len(), 16_000);
    }

    #[test]
    fn mels_have_expected_shape_for_1s() {
        let x = vec![0.0; VOX_SAMPLE_RATE];
        let mel = log_mel_spectrogram(&x);
        assert_eq!(mel.len() % VOX_MEL_BINS, 0);
        assert!(mel.len() / VOX_MEL_BINS > 90);
    }
}
