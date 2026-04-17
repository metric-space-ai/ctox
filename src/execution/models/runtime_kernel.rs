use anyhow::Result;
use sha2::Digest;
use sha2::Sha256;
#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::path::PathBuf;

use crate::inference::engine;
use crate::inference::local_transport::LocalTransport;
use crate::inference::runtime_contract;
use crate::inference::runtime_env;
use crate::inference::runtime_plan;
use crate::inference::runtime_state;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeLauncherKind {
    Engine,
    LiteRt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InferenceWorkloadRole {
    PrimaryGeneration,
    Embedding,
    Transcription,
    Speech,
}

impl InferenceWorkloadRole {
    pub fn legacy_env_role(self) -> &'static str {
        match self {
            Self::PrimaryGeneration => "chat",
            Self::Embedding => "embedding",
            Self::Transcription => "stt",
            Self::Speech => "tts",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ResolvedRuntimeBinding {
    pub workload: InferenceWorkloadRole,
    pub display_model: String,
    pub request_model: String,
    pub port: u16,
    pub base_url: String,
    pub socket_path: Option<String>,
    pub transport: LocalTransport,
    pub health_path: &'static str,
    pub launcher_kind: RuntimeLauncherKind,
    pub compute_target: Option<engine::ComputeTarget>,
    pub visible_devices: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ResolvedProxyRuntime {
    pub listen_host: String,
    pub listen_port: u16,
    pub upstream_base_url: String,
    pub active_model: Option<String>,
    pub embedding_base_url: String,
    pub embedding_model: Option<String>,
    pub transcription_base_url: String,
    pub transcription_model: Option<String>,
    pub speech_base_url: String,
    pub speech_model: Option<String>,
}

#[derive(Debug, Clone)]
pub struct InferenceRuntimeKernel {
    pub state: runtime_state::InferenceRuntimeState,
    pub ownership: runtime_contract::RuntimeOwnershipState,
    pub proxy: ResolvedProxyRuntime,
    pub primary_generation: Option<ResolvedRuntimeBinding>,
    pub embedding: Option<ResolvedRuntimeBinding>,
    pub transcription: Option<ResolvedRuntimeBinding>,
    pub speech: Option<ResolvedRuntimeBinding>,
}

pub fn managed_runtime_socket_path(root: &Path, workload: InferenceWorkloadRole) -> PathBuf {
    let socket_name = match workload {
        InferenceWorkloadRole::PrimaryGeneration => "primary_generation.sock",
        InferenceWorkloadRole::Embedding => "embedding.sock",
        InferenceWorkloadRole::Transcription => "transcription.sock",
        InferenceWorkloadRole::Speech => "speech.sock",
    };
    let preferred = root.join("runtime/sockets").join(socket_name);
    #[cfg(unix)]
    {
        const MAX_UNIX_SOCKET_PATH_LEN: usize = 100;
        if preferred.as_os_str().as_bytes().len() >= MAX_UNIX_SOCKET_PATH_LEN {
            let mut hasher = Sha256::new();
            hasher.update(root.as_os_str().as_bytes());
            let digest = format!("{:x}", hasher.finalize());
            return Path::new("/tmp")
                .join(format!("ctox-sock-{}", &digest[..12]))
                .join(socket_name);
        }
    }
    preferred
}

pub fn preferred_auxiliary_selection_for_host(
    root: &Path,
    role: engine::AuxiliaryRole,
    configured_model: Option<&str>,
) -> engine::AuxiliaryModelSelection {
    let selection = engine::auxiliary_model_selection(role, configured_model);
    if selection.compute_target != engine::ComputeTarget::Gpu {
        return selection;
    }

    let visible_devices =
        runtime_plan::resolve_auxiliary_visible_devices(root, role, selection.request_model)
            .ok()
            .flatten();
    if visible_devices
        .as_deref()
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
    {
        return selection;
    }

    let cpu_alias = format!("{} [CPU]", selection.request_model);
    let cpu_selection = engine::auxiliary_model_selection(role, Some(&cpu_alias));
    if cpu_selection.compute_target == engine::ComputeTarget::Cpu {
        cpu_selection
    } else {
        selection
    }
}

impl InferenceRuntimeKernel {
    pub fn resolve(root: &Path) -> Result<Self> {
        let state = runtime_state::load_or_resolve_runtime_state(root)?;
        let ownership = runtime_contract::load_runtime_ownership_state(root).unwrap_or_default();

        let primary_generation = resolve_primary_generation(root, &state);
        let embedding = resolve_auxiliary(root, engine::AuxiliaryRole::Embedding, &state);
        let transcription = resolve_auxiliary(root, engine::AuxiliaryRole::Stt, &state);
        let speech = resolve_auxiliary(root, engine::AuxiliaryRole::Tts, &state);

        let upstream_base_url = primary_generation
            .as_ref()
            .map(|binding| binding.base_url.clone())
            .unwrap_or_else(|| state.upstream_base_url.clone());
        let proxy = ResolvedProxyRuntime {
            listen_host: state.proxy_host.clone(),
            listen_port: state.proxy_port,
            upstream_base_url,
            active_model: state.active_model.clone(),
            embedding_base_url: embedding
                .as_ref()
                .map(|binding| binding.base_url.clone())
                .unwrap_or_default(),
            embedding_model: embedding
                .as_ref()
                .map(|binding| binding.request_model.clone()),
            transcription_base_url: transcription
                .as_ref()
                .map(|binding| binding.base_url.clone())
                .unwrap_or_default(),
            transcription_model: transcription
                .as_ref()
                .map(|binding| binding.request_model.clone()),
            speech_base_url: speech
                .as_ref()
                .map(|binding| binding.base_url.clone())
                .unwrap_or_default(),
            speech_model: speech.as_ref().map(|binding| binding.request_model.clone()),
        };

        Ok(Self {
            state,
            ownership,
            proxy,
            primary_generation,
            embedding,
            transcription,
            speech,
        })
    }

    pub fn turn_context_tokens(&self) -> i64 {
        self.state
            .realized_context_tokens
            .map(|value| value as i64)
            .unwrap_or(131_072)
    }

    pub fn active_model(&self) -> Option<&str> {
        self.state.active_model.as_deref()
    }

    pub fn responses_gateway_base_url(&self) -> String {
        format!(
            "http://{}:{}",
            self.proxy.listen_host, self.proxy.listen_port
        )
    }

    pub fn internal_responses_base_url(&self) -> String {
        if let Some(binding) = self.primary_generation.as_ref() {
            return responses_api_base_url(&binding.base_url);
        }
        responses_api_base_url(&self.proxy.upstream_base_url)
    }

    pub fn auxiliary_base_url(&self, role: engine::AuxiliaryRole) -> Option<&str> {
        self.binding_for_auxiliary_role(role)
            .map(|binding| binding.base_url.as_str())
    }

    pub fn binding_for_auxiliary_role(
        &self,
        role: engine::AuxiliaryRole,
    ) -> Option<&ResolvedRuntimeBinding> {
        match role {
            engine::AuxiliaryRole::Embedding => self.embedding.as_ref(),
            engine::AuxiliaryRole::Stt => self.transcription.as_ref(),
            engine::AuxiliaryRole::Tts => self.speech.as_ref(),
        }
    }
}

fn responses_api_base_url(base_url: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    if trimmed.ends_with("/v1") {
        trimmed.to_string()
    } else {
        format!("{trimmed}/v1")
    }
}

fn resolve_primary_generation(
    root: &Path,
    state: &runtime_state::InferenceRuntimeState,
) -> Option<ResolvedRuntimeBinding> {
    if !state.source.is_local() {
        return None;
    }
    let request_model = state
        .engine_model
        .clone()
        .or_else(|| state.active_or_selected_model().map(ToOwned::to_owned))
        .filter(|value| !value.trim().is_empty())?;
    let runtime = engine::runtime_config_for_model(&request_model)
        .unwrap_or_else(|_| engine::default_runtime_config(engine::LocalModelFamily::GptOss));
    let port = state.engine_port.unwrap_or(runtime.port);
    let launcher_kind = match state.local_runtime {
        runtime_state::LocalRuntimeKind::Candle => RuntimeLauncherKind::Engine,
        runtime_state::LocalRuntimeKind::LiteRt => RuntimeLauncherKind::LiteRt,
    };
    let visible_devices = match state.local_runtime {
        runtime_state::LocalRuntimeKind::Candle | runtime_state::LocalRuntimeKind::LiteRt => {
            runtime_plan::load_persisted_chat_runtime_plan(root)
                .ok()
                .flatten()
                .map(|plan| plan.cuda_visible_devices)
                .filter(|value| !value.trim().is_empty())
                .or_else(|| runtime_env::env_or_config(root, "CTOX_ENGINE_CUDA_VISIBLE_DEVICES"))
        }
    };
    let socket_path_buf =
        managed_runtime_socket_path(root, InferenceWorkloadRole::PrimaryGeneration);
    let transport = LocalTransport::default_for_host(Some(socket_path_buf), "127.0.0.1", port);
    let socket_path = transport
        .unix_socket_path()
        .map(|path| path.display().to_string());
    let base_url = transport.http_base_url().unwrap_or_default();
    Some(ResolvedRuntimeBinding {
        workload: InferenceWorkloadRole::PrimaryGeneration,
        display_model: request_model.clone(),
        request_model,
        port,
        base_url,
        socket_path,
        transport,
        health_path: "/health",
        launcher_kind,
        compute_target: None,
        visible_devices,
    })
}

fn resolve_auxiliary(
    root: &Path,
    role: engine::AuxiliaryRole,
    state: &runtime_state::InferenceRuntimeState,
) -> Option<ResolvedRuntimeBinding> {
    let auxiliary_state = runtime_state::auxiliary_runtime_state_for_role(state, role);
    if !auxiliary_state.enabled {
        return None;
    }
    let selection = preferred_auxiliary_selection_for_host(
        root,
        role,
        auxiliary_state.configured_model.as_deref(),
    );
    let port = auxiliary_state.port.unwrap_or(selection.default_port);
    let visible_devices = if selection.compute_target == engine::ComputeTarget::Gpu {
        runtime_plan::resolve_auxiliary_visible_devices(root, role, selection.request_model)
            .ok()
            .flatten()
    } else {
        None
    };
    let workload = match role {
        engine::AuxiliaryRole::Embedding => InferenceWorkloadRole::Embedding,
        engine::AuxiliaryRole::Stt => InferenceWorkloadRole::Transcription,
        engine::AuxiliaryRole::Tts => InferenceWorkloadRole::Speech,
    };
    let socket_path_buf = managed_runtime_socket_path(root, workload);
    let transport = LocalTransport::default_for_host(Some(socket_path_buf), "127.0.0.1", port);
    let socket_path = transport
        .unix_socket_path()
        .map(|path| path.display().to_string());
    let base_url = auxiliary_state.base_url.clone().unwrap_or_else(|| {
        transport
            .http_base_url()
            .unwrap_or_else(|| format!("http://127.0.0.1:{port}"))
    });
    Some(ResolvedRuntimeBinding {
        workload,
        display_model: selection.choice.to_string(),
        request_model: selection.request_model.to_string(),
        port,
        base_url,
        socket_path,
        transport,
        health_path: "/health",
        launcher_kind: RuntimeLauncherKind::Engine,
        compute_target: Some(selection.compute_target),
        visible_devices,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn make_temp_root() -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = PathBuf::from("/tmp").join(format!("ctox-rk-{unique}"));
        std::fs::create_dir_all(path.join("runtime")).unwrap();
        path
    }

    #[test]
    fn resolves_primary_generation_from_local_runtime_state() {
        let root = make_temp_root();
        let mut env_map = BTreeMap::new();
        env_map.insert("CTOX_DISABLE_MISSION_WATCHDOG".to_string(), "1".to_string());
        env_map.insert("CTOX_CHAT_MODEL".to_string(), "stale-env".to_string());
        runtime_env::save_runtime_env_map(&root, &env_map).unwrap();
        runtime_state::persist_runtime_state(
            &root,
            &runtime_state::InferenceRuntimeState {
                version: 4,
                source: runtime_state::InferenceSource::Local,
                local_runtime: runtime_state::LocalRuntimeKind::Candle,
                base_model: Some("openai/gpt-oss-20b".to_string()),
                requested_model: Some("openai/gpt-oss-20b".to_string()),
                active_model: Some("openai/gpt-oss-20b".to_string()),
                engine_model: Some("openai/gpt-oss-20b".to_string()),
                engine_port: Some(2234),
                realized_context_tokens: Some(65_536),
                proxy_host: "127.0.0.1".to_string(),
                proxy_port: 22434,
                upstream_base_url: "http://127.0.0.1:2234".to_string(),
                local_preset: Some("Quality".to_string()),
                boost: runtime_state::BoostRuntimeState::default(),
                adapter_tuning: runtime_state::AdapterRuntimeTuning::default(),
                embedding: runtime_state::AuxiliaryRuntimeState::default(),
                transcription: runtime_state::AuxiliaryRuntimeState::default(),
                speech: runtime_state::AuxiliaryRuntimeState::default(),
            },
        )
        .unwrap();

        let resolved = InferenceRuntimeKernel::resolve(&root).unwrap();
        let operator_settings = runtime_env::effective_operator_env_map(&root).unwrap();
        assert_eq!(resolved.turn_context_tokens(), 65_536);
        assert_eq!(
            operator_settings
                .get("CTOX_DISABLE_MISSION_WATCHDOG")
                .map(String::as_str),
            Some("1")
        );
        assert!(!operator_settings.contains_key("CTOX_CHAT_MODEL"));
        assert!(!operator_settings.contains_key("CTOX_ACTIVE_MODEL"));
        assert_eq!(resolved.proxy.listen_port, 22434);
        assert_eq!(
            resolved
                .primary_generation
                .as_ref()
                .map(|binding| binding.port),
            Some(2234)
        );
        assert_eq!(
            resolved
                .primary_generation
                .as_ref()
                .map(|binding| binding.base_url.as_str()),
            Some("")
        );
        assert_eq!(
            resolved.internal_responses_base_url(),
            "/v1"
        );
        assert_eq!(
            resolved.responses_gateway_base_url(),
            "http://127.0.0.1:22434"
        );

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn resolves_api_runtime_without_primary_generation_backend() {
        let root = make_temp_root();
        runtime_state::persist_runtime_state(
            &root,
            &runtime_state::InferenceRuntimeState {
                version: 4,
                source: runtime_state::InferenceSource::Api,
                local_runtime: runtime_state::LocalRuntimeKind::Candle,
                base_model: Some("gpt-5.4".to_string()),
                requested_model: Some("gpt-5.4".to_string()),
                active_model: Some("gpt-5.4".to_string()),
                engine_model: None,
                engine_port: None,
                realized_context_tokens: None,
                proxy_host: "127.0.0.1".to_string(),
                proxy_port: 12434,
                upstream_base_url: "https://api.openai.com".to_string(),
                local_preset: None,
                boost: runtime_state::BoostRuntimeState::default(),
                adapter_tuning: runtime_state::AdapterRuntimeTuning::default(),
                embedding: runtime_state::AuxiliaryRuntimeState::default(),
                transcription: runtime_state::AuxiliaryRuntimeState::default(),
                speech: runtime_state::AuxiliaryRuntimeState::default(),
            },
        )
        .unwrap();

        let resolved = InferenceRuntimeKernel::resolve(&root).unwrap();
        assert!(resolved.primary_generation.is_none());
        assert_eq!(
            resolved.internal_responses_base_url(),
            "https://api.openai.com/v1"
        );
        assert_eq!(
            resolved.proxy.upstream_base_url,
            "https://api.openai.com".to_string()
        );
        assert_eq!(resolved.active_model(), Some("gpt-5.4"));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn resolves_auxiliary_bindings_from_runtime_state() {
        let root = make_temp_root();
        runtime_state::persist_runtime_state(
            &root,
            &runtime_state::InferenceRuntimeState {
                version: 4,
                source: runtime_state::InferenceSource::Local,
                local_runtime: runtime_state::LocalRuntimeKind::Candle,
                base_model: Some("openai/gpt-oss-20b".to_string()),
                requested_model: Some("openai/gpt-oss-20b".to_string()),
                active_model: Some("openai/gpt-oss-20b".to_string()),
                engine_model: Some("openai/gpt-oss-20b".to_string()),
                engine_port: Some(1234),
                realized_context_tokens: Some(131_072),
                proxy_host: "127.0.0.1".to_string(),
                proxy_port: 12434,
                upstream_base_url: "http://127.0.0.1:1234".to_string(),
                local_preset: Some("Quality".to_string()),
                boost: runtime_state::BoostRuntimeState::default(),
                adapter_tuning: runtime_state::AdapterRuntimeTuning::default(),
                embedding: runtime_state::AuxiliaryRuntimeState {
                    enabled: true,
                    configured_model: Some("Qwen/Qwen3-Embedding-0.6B [CPU]".to_string()),
                    port: Some(2237),
                    base_url: None,
                },
                transcription: runtime_state::AuxiliaryRuntimeState {
                    enabled: false,
                    configured_model: Some("Systran/faster-whisper-small [CPU]".to_string()),
                    port: Some(2238),
                    base_url: None,
                },
                speech: runtime_state::AuxiliaryRuntimeState {
                    enabled: true,
                    configured_model: Some(
                        "speaches-ai/piper-en_US-lessac-medium [CPU EN]".to_string(),
                    ),
                    port: Some(2239),
                    base_url: Some("http://127.0.0.1:2239".to_string()),
                },
            },
        )
        .unwrap();

        let resolved = InferenceRuntimeKernel::resolve(&root).unwrap();
        assert_eq!(
            resolved
                .embedding
                .as_ref()
                .map(|binding| binding.request_model.as_str()),
            Some("Qwen/Qwen3-Embedding-0.6B")
        );
        assert_eq!(
            resolved.embedding.as_ref().map(|binding| binding.port),
            Some(2237)
        );
        assert!(resolved.transcription.is_none());
        assert_eq!(
            resolved
                .speech
                .as_ref()
                .map(|binding| binding.request_model.as_str()),
            Some("speaches-ai/piper-en_US-lessac-medium")
        );
        assert_eq!(
            resolved
                .speech
                .as_ref()
                .map(|binding| binding.base_url.as_str()),
            Some("http://127.0.0.1:2239")
        );

        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn managed_runtime_socket_path_uses_short_tmp_path_for_long_roots() {
        let long_root = PathBuf::from("/tmp").join("a".repeat(180));
        let socket_path = managed_runtime_socket_path(&long_root, InferenceWorkloadRole::Embedding);
        assert_eq!(
            socket_path.file_name().and_then(|value| value.to_str()),
            Some("embedding.sock")
        );
        assert!(socket_path.starts_with(Path::new("/tmp")));
        assert_eq!(
            socket_path
                .parent()
                .and_then(|path| path.file_name())
                .and_then(|value| value.to_str())
                .map(|value| value.starts_with("ctox-sock-")),
            Some(true)
        );
        assert!(socket_path.as_os_str().as_bytes().len() < 100);
    }

    #[cfg(unix)]
    #[test]
    fn managed_runtime_socket_path_keeps_workspace_runtime_dir_when_short_enough() {
        let root = make_temp_root();
        let socket_path =
            managed_runtime_socket_path(&root, InferenceWorkloadRole::PrimaryGeneration);
        assert_eq!(
            socket_path,
            root.join("runtime/sockets/primary_generation.sock")
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn auxiliary_selection_falls_back_to_cpu_when_no_visible_gpus_exist() {
        let root = make_temp_root();
        let selection =
            preferred_auxiliary_selection_for_host(&root, engine::AuxiliaryRole::Embedding, None);
        assert_eq!(selection.request_model, "Qwen/Qwen3-Embedding-0.6B");
        assert_eq!(selection.compute_target, engine::ComputeTarget::Cpu);
    }
}
