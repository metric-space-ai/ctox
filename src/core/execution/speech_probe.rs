// Origin: CTOX
// License: AGPL-3.0-only
//! A real capture-cadence speech probe. Generated reference audio stays native;
//! its final receipt is a diagnostic, never authority to append a meeting transcript.
use super::*;
use tokio::time::{interval, MissedTickBehavior};

pub struct SpeechTranscriptionProbe {
    pub text: String,
    pub model: String,
    pub finish_to_final_ms: u64,
    pub audio_duration_ms: u64,
    pub partial_before_audio_end: bool,
}

fn wav_pcm(audio: &[u8]) -> Result<(PcmFormat, &[u8]), SpeechError> {
    if audio.len() < 44 || audio.len() > 1_000_000
        || &audio[..4] != b"RIFF" || &audio[8..12] != b"WAVE"
        || u32::from_le_bytes(audio[4..8].try_into().unwrap()) as usize + 8 != audio.len()
    {
        return Err(SpeechError::InvalidResponse);
    }
    let mut at = 12usize;
    let mut format = None;
    let mut data = None;
    while at + 8 <= audio.len() {
        let size = u32::from_le_bytes(audio[at + 4..at + 8].try_into().unwrap()) as usize;
        let start = at + 8;
        let end = start.checked_add(size).filter(|end| *end <= audio.len())
            .ok_or(SpeechError::InvalidResponse)?;
        match &audio[at..at + 4] {
            b"fmt " => {
                if format.is_some() || size < 16
                    || audio[start..start + 2] != 1u16.to_le_bytes()
                    || audio[start + 2..start + 4] != 1u16.to_le_bytes()
                    || audio[start + 12..start + 14] != 2u16.to_le_bytes()
                    || audio[start + 14..start + 16] != 16u16.to_le_bytes()
                {
                    return Err(SpeechError::InvalidResponse);
                }
                let rate = u32::from_le_bytes(audio[start + 4..start + 8].try_into().unwrap());
                let pcm = PcmFormat { sample_rate_hz: rate };
                pcm.validate().map_err(|_| SpeechError::InvalidResponse)?;
                if audio[start + 8..start + 12] != (rate * 2).to_le_bytes() {
                    return Err(SpeechError::InvalidResponse);
                }
                format = Some(pcm);
            }
            b"data" => {
                if data.is_some() || size == 0 || size % 2 != 0 {
                    return Err(SpeechError::InvalidResponse);
                }
                data = Some(&audio[start..end]);
            }
            _ => {}
        }
        at = end.checked_add(size % 2).ok_or(SpeechError::InvalidResponse)?;
    }
    let format = format.ok_or(SpeechError::InvalidResponse)?;
    let data = data.ok_or(SpeechError::InvalidResponse)?;
    if at != audio.len() || data.len() > format.sample_rate_hz as usize * 2 * 10 {
        return Err(SpeechError::InvalidResponse);
    }
    Ok((format, data))
}

async fn replay(
    mut stream: TranscriptionStream,
    format: PcmFormat,
    pcm: &[u8],
    current: impl Fn() -> bool,
) -> Result<SpeechTranscriptionProbe, SpeechError> {
    let mut clock = interval(Duration::from_millis(20));
    clock.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut retirement = interval(Duration::from_millis(250));
    let chunk_bytes = format.sample_rate_hz as usize / 50 * 2;
    let mut offset = 0;
    let mut finished = false;
    let mut partial_before_audio_end = false;
    loop {
        if !current() {
            stream.cancel().await;
            return Err(SpeechError::Closed);
        }
        tokio::select! {
            _ = retirement.tick() => {}
            _ = clock.tick(), if !finished => {
                if offset == pcm.len() {
                    stream.finish_audio()?;
                    finished = true;
                } else {
                    let end = (offset + chunk_bytes).min(pcm.len());
                    stream.append_pcm(&pcm[offset..end])?;
                    offset = end;
                }
            }
            event = stream.next_verified_event() => match event {
                Some(Ok(VerifiedTranscriptEvent::Partial { .. })) => {
                    if !finished { partial_before_audio_end = true; }
                }
                Some(Ok(VerifiedTranscriptEvent::Final(receipt))) => {
                    if !finished || receipt.text().trim().is_empty() || receipt.text().len() > 8192 {
                        return Err(SpeechError::InvalidResponse);
                    }
                    let finish_to_final_ms = receipt.finish_to_final_ms()
                        .ok_or(SpeechError::InvalidResponse)?;
                    return Ok(SpeechTranscriptionProbe {
                        text: receipt.text().to_owned(),
                        model: receipt.model().to_owned(),
                        finish_to_final_ms,
                        audio_duration_ms: receipt.audio_duration_ms(),
                        partial_before_audio_end,
                    });
                }
                Some(Err(error)) => return Err(error),
                None => return Err(SpeechError::InvalidResponse),
            }
        }
    }
}

impl SpeechGateway {
    /// Checks the configured Mistral transcription path with an actual short
    /// sentence synthesized by the saved voice. No microphone or meeting writes.
    /// The diagnostic boundary excludes TTS, network setup and browser/VAD.
    pub async fn check_transcription(
        &self,
        current: impl Fn() -> bool + Send + Sync,
    ) -> Result<SpeechTranscriptionProbe, SpeechError> {
        if self.config.transcription != SpeechBackend::Mistral
            || self.config.synthesis != SpeechBackend::Mistral
        {
            return Err(SpeechError::UnsupportedBackend);
        }
        if !current() { return Err(SpeechError::Closed); }
        tokio::time::timeout(Duration::from_secs(25), async {
            let gateway = SpeechGateway::from_root(&self.root)?;
            let output = tokio::task::spawn_blocking(move || gateway.synthesize_with_timeout(
                &SpeechRequest {
                    text: "Der Sprachtest für Workjet ist bereit.".into(),
                    format: SpeechAudioFormat::Wav,
                    voice_id: None,
                }, Duration::from_secs(8),
            )).await.map_err(|_| SpeechError::ExecutionUnavailable)??;
            if !current() { return Err(SpeechError::Closed); }
            let (format, pcm) = wav_pcm(&output.audio)?;
            let stream = self.open_transcription(format).await?;
            replay(stream, format, pcm, current).await
        }).await.map_err(|_| SpeechError::TimedOut)?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn wav(rate: u32, samples: usize) -> Vec<u8> {
        let data_bytes = samples * 2;
        let mut out = Vec::new();
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(36 + data_bytes as u32).to_le_bytes());
        out.extend_from_slice(b"WAVEfmt ");
        out.extend_from_slice(&16u32.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&rate.to_le_bytes());
        out.extend_from_slice(&(rate * 2).to_le_bytes());
        out.extend_from_slice(&2u16.to_le_bytes());
        out.extend_from_slice(&16u16.to_le_bytes());
        out.extend_from_slice(b"data");
        out.extend_from_slice(&(data_bytes as u32).to_le_bytes());
        out.resize(44 + data_bytes, 0);
        out
    }
    #[test]
    fn probe_accepts_actual_pcm_and_rejects_unplayable_or_unbounded_audio() {
        for rate in [16_000, 24_000, 48_000] {
            let audio = wav(rate, rate as usize);
            let (format, pcm) = wav_pcm(&audio).unwrap();
            assert_eq!(format.sample_rate_hz, rate);
            assert_eq!(pcm.len(), rate as usize * 2);
        }
        let mut stereo = wav(24_000, 20);
        stereo[22..24].copy_from_slice(&2u16.to_le_bytes());
        assert!(wav_pcm(&stereo).is_err());
        let mut truncated = wav(24_000, 20);
        truncated.pop();
        assert!(wav_pcm(&truncated).is_err());
        let mut duplicate = wav(24_000, 20);
        duplicate.extend_from_slice(b"data");
        duplicate.extend_from_slice(&2u32.to_le_bytes());
        duplicate.extend_from_slice(&[0, 0]);
        let size = (duplicate.len() - 8) as u32;
        duplicate[4..8].copy_from_slice(&size.to_le_bytes());
        assert!(wav_pcm(&duplicate).is_err());
        assert!(wav_pcm(&wav(16_000, 160_001)).is_err());
        assert!(wav_pcm(b"provider error").is_err());
    }
    #[tokio::test]
    async fn actual_gateway_final_and_partial_define_probe_diagnostics() {
        let (endpoint, server) = super::super::tests::fixture("normal").await;
        let stream = super::super::tests::start(&endpoint).await;
        let result = replay(stream, PcmFormat::default(), &[0; 640], || true).await.unwrap();
        assert_eq!(result.text, "Hallo Welt.");
        assert_eq!(result.audio_duration_ms, 20);
        assert!(result.finish_to_final_ms >= 40);
        assert!(result.partial_before_audio_end);
        tokio::time::timeout(Duration::from_secs(2), server).await.unwrap().unwrap();
    }
    #[tokio::test]
    async fn retired_probe_closes_its_stream_without_accepting_final() {
        let (endpoint, server) = super::super::tests::fixture("normal").await;
        let stream = super::super::tests::start(&endpoint).await;
        assert!(matches!(replay(stream, PcmFormat::default(), &[0; 640], || false).await, Err(SpeechError::Closed)));
        tokio::time::timeout(Duration::from_secs(2), server).await.unwrap().unwrap();
    }
}

