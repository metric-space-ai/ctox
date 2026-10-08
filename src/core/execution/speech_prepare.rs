// Origin: CTOX
// License: AGPL-3.0-only
//! Explicit operator warmup through the existing managed local runtime.
use super::{SpeechBackend, SpeechRuntimeConfig};
use crate::inference::{
    engine::AuxiliaryRole, local_transport::LocalTransport, runtime_kernel::InferenceRuntimeKernel,
    supervisor,
};
use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::{json, Value};
use std::{
    io::{BufRead, BufReader, Read, Write},
    path::Path,
    time::{Duration, Instant},
};

#[derive(Debug, Serialize)]
pub struct LocalSpeechReadiness {
    pub model: String,
    pub backend: String,
    pub elapsed_ms: u64,
}

/// Starts only the two explicitly configured local speech roles. It does not
/// switch chat models, fetch weights, contact a cloud provider or grant speech.
pub fn warm_local_runtime(root: &Path) -> Result<Vec<LocalSpeechReadiness>> {
    let config = SpeechRuntimeConfig::load(root)?;
    anyhow::ensure!(
        config.transcription == SpeechBackend::Runtime
            && config.synthesis == SpeechBackend::Runtime,
        "speech warmup requires both runtime backends"
    );
    let kernel = InferenceRuntimeKernel::resolve(root)?;
    let mut selected = Vec::new();
    for (role, model) in [
        (AuxiliaryRole::Stt, "engineai/Voxtral-Mini-4B-Realtime-2602"),
        (AuxiliaryRole::Tts, "engineai/Voxtral-4B-TTS-2603"),
    ] {
        let binding = kernel
            .binding_for_auxiliary_role(role)
            .filter(|binding| binding.request_model == model)
            .context("configure the local Voxtral speech runtime before warmup")?;
        anyhow::ensure!(
            matches!(
                binding.transport,
                LocalTransport::UnixSocket { .. } | LocalTransport::NamedPipe { .. }
            ),
            "speech warmup requires managed private IPC"
        );
        selected.push((role, model, binding.transport.clone()));
    }
    let mut ready = Vec::new();
    for (role, model, transport) in selected {
        let started = Instant::now();
        supervisor::ensure_auxiliary_backend_launchable(root, role)?;
        supervisor::ensure_auxiliary_backend_ready(root, role, false)?;
        let mut stream = transport.connect_blocking(Duration::from_secs(5))?;
        stream.set_read_timeout(Some(Duration::from_secs(5)))?;
        stream.set_write_timeout(Some(Duration::from_secs(5)))?;
        let mut request = serde_json::to_vec(&json!({"kind": "runtime_health"}))?;
        request.push(b'\n');
        stream.write_all(&request)?;
        let mut line = String::new();
        BufReader::new(stream).take(8193).read_line(&mut line)?;
        anyhow::ensure!(line.len() <= 8192, "local speech health response too large");
        let health: Value = serde_json::from_str(&line)?;
        verify_health(role, model, &health)?;
        ready.push(LocalSpeechReadiness {
            model: model.into(),
            backend: health["backend"]
                .as_str()
                .context("missing local speech backend")?
                .into(),
            elapsed_ms: started.elapsed().as_millis().min(u64::MAX as u128) as u64,
        });
    }
    Ok(ready)
}

fn verify_health(role: AuxiliaryRole, model: &str, health: &Value) -> Result<()> {
    anyhow::ensure!(
        health["kind"] == "runtime_health"
            && health["healthy"] == true
            && health["artifacts_loaded"] == true
            && health["loaded_models"]
                .as_array()
                .is_some_and(|models| models.iter().any(|loaded| loaded.as_str() == Some(model))),
        "configured local speech model is not loaded and healthy"
    );
    let graph_ready = match role {
        AuxiliaryRole::Stt => {
            health["transcription_graph_wired"] == true
                && health["live_transcription"]["streaming_supported"] == true
        }
        AuxiliaryRole::Tts => health["speech_synthesis_wired"] == true,
        _ => false,
    };
    anyhow::ensure!(graph_ready, "local speech execution graph is not ready");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn warmup_never_starts_a_cloud_or_computer_backend() -> Result<()> {
        let root = tempfile::tempdir()?;
        SpeechRuntimeConfig {
            synthesis: SpeechBackend::Mistral,
            transcription: SpeechBackend::Computer,
            voice_id: None,
        }
        .save(root.path())?;
        assert!(warm_local_runtime(root.path())
            .unwrap_err()
            .to_string()
            .contains("requires both runtime backends"));
        assert!(!root.path().join("runtime/ctox_stt_backend.pid").exists());
        assert!(!root.path().join("runtime/ctox_tts_backend.pid").exists());
        Ok(())
    }

    #[test]
    fn open_socket_or_wired_graph_alone_cannot_report_a_ready_model() {
        let model = "engineai/Voxtral-Mini-4B-Realtime-2602";
        let good = json!({"kind":"runtime_health","healthy":true,"artifacts_loaded":true,
            "loaded_models":[model],"transcription_graph_wired":true,
            "live_transcription":{"streaming_supported":true}});
        assert!(verify_health(AuxiliaryRole::Stt, model, &good).is_ok());
        for (pointer, bad) in [
            ("/healthy", json!(false)),
            ("/artifacts_loaded", json!(false)),
            ("/loaded_models", json!(["other-model"])),
            ("/live_transcription/streaming_supported", json!(false)),
        ] {
            let mut reply = good.clone();
            *reply.pointer_mut(pointer).unwrap() = bad;
            assert!(verify_health(AuxiliaryRole::Stt, model, &reply).is_err());
        }
    }
}
