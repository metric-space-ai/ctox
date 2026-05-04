// Origin: CTOX
// License: Apache-2.0

//! Per-model local backend registry — maps a request model ID onto
//! the matching server binary under `src/inference/models/<model>/`
//! and the CLI args needed to spawn it.
//!
//! Scope today: curated self-contained model servers, starting with the
//! Qwen3.5-27B Q4_K_M + DFlash-draft pair, native Voxtral STT,
//! and native Voxtral TTS.
//! Adding a second curated model means appending one entry to
//! [`resolve_local_model_backend`] plus any new SQLite runtime-config
//! keys for weight/tokenizer paths.
//!
//! The design is deliberately tiny — no config file, no dynamic
//! discovery. The supervisor is the only caller and needs one thing:
//! "for this request model, what binary + what args?".
//!
//! # Weight / tokenizer paths
//!
//! Per CTOX's operator guardrails, paths live in the persisted
//! SQLite runtime-env store (`runtime_env_kv` table), **not** in
//! process environment. Resolved via
//! `runtime_env::env_or_config(root, key)` which reads the SQLite
//! store first and falls back to the canonical dflash-ref dev-box
//! layout under `$HOME` so an out-of-the-box A6000 install works
//! without any configuration. No new `std::env::var*` calls.
//!
//! | Key (in `runtime_env_kv`)        | Default fallback                                                 |
//! |----------------------------------|------------------------------------------------------------------|
//! | `CTOX_QWEN35_TARGET_GGUF`        | `$HOME/dflash-ref/dflash/models/Qwen3.5-27B-Q4_K_M.gguf`         |
//! | `CTOX_QWEN35_DRAFT_SAFETENSORS`  | `$HOME/dflash-ref/dflash/models/draft/model.safetensors`         |
//! | `CTOX_QWEN35_TOKENIZER`          | _discovered by the server bin from the HF cache_                 |
//! | `CTOX_QWEN35_GGML_LIB_DIR`        | `$HOME/dflash-ref/dflash/build/deps/llama.cpp/ggml/src`          |
//!
//! Set via either the TUI's runtime-settings page or
//! `ctox secret set <key> <value>` (the runtime-env store persists
//! any key; secret classification is per-key).

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use super::runtime_env;

/// One local-model backend: binary to spawn + CLI args + extra env.
/// `env` is used only for per-backend overrides the supervisor will
/// apply on top of its own clean-env base; no global process env is
/// consulted anywhere in this module.
pub struct LocalModelBackend {
    pub binary: PathBuf,
    pub args: Vec<OsString>,
    pub env: Vec<(&'static str, OsString)>,
    /// Human-readable model id reported back in health/chat responses.
    pub model_id: &'static str,
}

/// Inputs the registry needs to build a launch spec.
pub struct LocalModelRequest<'a> {
    /// The request model (`launch_spec.request_model`).
    pub request_model: &'a str,
    /// The Unix-socket path the gateway expects.
    pub transport_endpoint: Option<&'a str>,
    /// CTOX install root — used to locate the built server binary
    /// + read the runtime-env SQLite store for per-backend paths.
    pub root: &'a Path,
}

/// Returns `Some(...)` if the request model has a local in-tree
/// server binary, else `None`. `None` means the caller must treat
/// this model as API-only.
pub fn resolve_local_model_backend(req: LocalModelRequest<'_>) -> Option<LocalModelBackend> {
    let model = req.request_model.trim();
    if is_qwen35_27b(model) {
        return Some(qwen35_27b_q4km_dflash_backend(
            req.root,
            req.transport_endpoint,
        ));
    }
    if is_voxtral_mini_4b_realtime(model) {
        return Some(voxtral_mini_4b_realtime_backend(
            req.root,
            req.transport_endpoint,
        ));
    }
    if is_voxtral_4b_tts(model) {
        return Some(voxtral_4b_tts_backend(req.root, req.transport_endpoint));
    }
    None
}

/// Is `model` handled by the Qwen3.5-27B Q4_K_M + DFlash server?
/// Matches the canonical request-model IDs we expect CTOX to
/// pipe in for local chat.
pub fn is_qwen35_27b(model: &str) -> bool {
    // Request model aliases — keep in sync with the model registry.
    model == "qwen35-27b-q4km-dflash"
        || model == "Qwen/Qwen3.5-27B"
        || model.starts_with("Qwen/Qwen3.5-27B-")
        || model.starts_with("unsloth/Qwen3.5-27B")
}

pub fn is_voxtral_4b_tts(model: &str) -> bool {
    model == "engineai/Voxtral-4B-TTS-2603"
        || model.eq_ignore_ascii_case("engineai/voxtral-4b-tts-2603")
        || model.eq_ignore_ascii_case("voxtral-4b-tts-2603")
}

pub fn is_voxtral_mini_4b_realtime(model: &str) -> bool {
    model == "engineai/Voxtral-Mini-4B-Realtime-2602"
        || model.eq_ignore_ascii_case("engineai/voxtral-mini-4b-realtime-2602")
        || model.eq_ignore_ascii_case("voxtral-mini-4b-realtime-2602")
}

fn qwen35_27b_q4km_dflash_backend(
    root: &Path,
    transport_endpoint: Option<&str>,
) -> LocalModelBackend {
    let binary = root
        .join("src/inference/models/qwen35_27b_q4km_dflash/target/release")
        .join("qwen35-27b-q4km-dflash-server");

    let target = config_path_or(root, "CTOX_QWEN35_TARGET_GGUF", default_qwen35_target(root));
    let draft = config_path_or(
        root,
        "CTOX_QWEN35_DRAFT_SAFETENSORS",
        default_qwen35_draft(root),
    );
    let tokenizer = runtime_env::env_or_config(root, "CTOX_QWEN35_TOKENIZER")
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from);

    // Socket path comes from the gateway's runtime_state resolution.
    // Fallback to the canonical runtime/sockets/ path under `root`
    // if the caller didn't pass one — should only happen in tests.
    let socket = match transport_endpoint {
        Some(ep) => PathBuf::from(ep),
        None => root.join("runtime/sockets/primary_generation.sock"),
    };

    let mut args: Vec<OsString> = Vec::new();
    args.push("--target".into());
    args.push(target.into());
    args.push("--draft".into());
    args.push(draft.into());
    if let Some(tok) = tokenizer {
        args.push("--tokenizer".into());
        args.push(tok.into());
    }
    args.push("--socket".into());
    args.push(socket.into());
    args.push("--model-id".into());
    args.push("qwen35-27b-q4km-dflash".into());
    args.push("--fast-rollback".into());
    args.push("--ddtree-budget".into());
    args.push("22".into());
    args.push("--ddtree-temp".into());
    args.push("0.6".into());
    // Embed the canonical request-model alias in the command line so
    // the supervisor's `socket_backed_process_matches_spec` check
    // (which does a literal `command.contains(spec.request_model)`
    // match against the `ps -o command=` output) succeeds against
    // the managed backend spec for Chat, whose request_model is
    // "Qwen/Qwen3.5-27B". The server bin accepts the flag but does
    // not need it for runtime behavior — it's a commandline marker.
    args.push("--request-model-alias".into());
    args.push("Qwen/Qwen3.5-27B".into());

    // The current hybrid binary still links ggml-cuda dynamically.
    // Keep the runtime linker path explicit so supervisor-spawned
    // CTOX processes work under the clean child environment.
    let ggml_lib_dir = config_path_or(
        root,
        "CTOX_QWEN35_GGML_LIB_DIR",
        default_qwen35_ggml_lib(root),
    );
    let ld_library_path = format!(
        "{}:{}",
        ggml_lib_dir.display(),
        ggml_lib_dir.join("ggml-cuda").display()
    );
    let env: Vec<(&'static str, OsString)> =
        vec![("LD_LIBRARY_PATH", OsString::from(ld_library_path))];

    LocalModelBackend {
        binary,
        args,
        env,
        model_id: "qwen35-27b-q4km-dflash",
    }
}

fn voxtral_4b_tts_backend(root: &Path, transport_endpoint: Option<&str>) -> LocalModelBackend {
    let binary = std::env::current_exe().unwrap_or_else(|_| root.join("target/release/ctox"));
    let socket = match transport_endpoint {
        Some(ep) => PathBuf::from(ep),
        None => root.join("runtime/sockets/speech.sock"),
    };
    let model_dir = config_path_optional(root, "CTOX_VOXTRAL_TTS_MODEL_DIR")
        .or_else(|| config_path_optional(root, "CTOX_TTS_MODEL_DIR"))
        .or_else(|| config_path_optional(root, "CTOX_SPEECH_MODEL_DIR"));

    let mut args: Vec<OsString> = Vec::new();
    args.push("__native-voxtral-tts-service".into());
    args.push("--transport".into());
    args.push(socket.into());
    args.push("--compute-target".into());
    args.push("gpu".into());
    if let Some(model_dir) = model_dir {
        args.push("--model-dir".into());
        args.push(model_dir.into());
    }
    args.push("--request-model-alias".into());
    args.push("engineai/Voxtral-4B-TTS-2603".into());

    LocalModelBackend {
        binary,
        args,
        env: Vec::new(),
        model_id: "voxtral-4b-tts-2603",
    }
}

fn voxtral_mini_4b_realtime_backend(
    root: &Path,
    transport_endpoint: Option<&str>,
) -> LocalModelBackend {
    let binary = std::env::current_exe().unwrap_or_else(|_| root.join("target/release/ctox"));
    let socket = match transport_endpoint {
        Some(ep) => PathBuf::from(ep),
        None => root.join("runtime/sockets/transcription.sock"),
    };
    let model_path = config_path_optional(root, "CTOX_VOXTRAL_STT_GGUF")
        .or_else(|| config_path_optional(root, "CTOX_STT_MODEL_PATH"))
        .or_else(|| {
            config_path_optional(root, "CTOX_VOXTRAL_STT_MODEL_DIR")
                .or_else(|| config_path_optional(root, "CTOX_STT_MODEL_DIR"))
                .map(|dir| dir.join("voxtral.gguf"))
        });

    let mut args: Vec<OsString> = Vec::new();
    args.push("__native-voxtral-stt-service".into());
    args.push("--transport".into());
    args.push(socket.into());
    args.push("--compute-target".into());
    args.push("gpu".into());
    if let Some(model_path) = model_path {
        args.push("--model-path".into());
        args.push(model_path.into());
    }
    args.push("--request-model-alias".into());
    args.push("engineai/Voxtral-Mini-4B-Realtime-2602".into());

    LocalModelBackend {
        binary,
        args,
        env: Vec::new(),
        model_id: "voxtral-mini-4b-realtime-2602",
    }
}

/// Resolve a filesystem path: sqlite runtime-config first, then
/// caller-supplied fallback. No process env fallback.
fn config_path_or(root: &Path, key: &str, fallback: PathBuf) -> PathBuf {
    runtime_env::env_or_config(root, key)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or(fallback)
}

fn config_path_optional(root: &Path, key: &str) -> Option<PathBuf> {
    runtime_env::env_or_config(root, key)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
}

/// Home-dir resolution — uses `$HOME` purely as an OS-level POSIX
/// hint (same pattern as `src/main.rs::home_dir`, not a CTOX
/// runtime-state toggle). Falls back to `/` so the rest of the
/// resolver still produces a path that will fail cleanly at spawn
/// time if no valid home-dir exists AND no explicit
/// `CTOX_QWEN35_TARGET_GGUF` / `CTOX_QWEN35_DRAFT_SAFETENSORS` is
/// set in the SQLite runtime-env store.
fn resolve_home_dir(_root: &Path) -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

fn default_qwen35_target(root: &Path) -> PathBuf {
    resolve_home_dir(root).join("dflash-ref/dflash/models/Qwen3.5-27B-Q4_K_M.gguf")
}

fn default_qwen35_draft(root: &Path) -> PathBuf {
    resolve_home_dir(root).join("dflash-ref/dflash/models/draft/model.safetensors")
}

fn default_qwen35_ggml_lib(root: &Path) -> PathBuf {
    resolve_home_dir(root).join("dflash-ref/dflash/build/deps/llama.cpp/ggml/src")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qwen35_aliases_resolve() {
        assert!(is_qwen35_27b("qwen35-27b-q4km-dflash"));
        assert!(is_qwen35_27b("Qwen/Qwen3.5-27B"));
        assert!(is_qwen35_27b("Qwen/Qwen3.5-27B-Instruct"));
        assert!(is_qwen35_27b("unsloth/Qwen3.5-27B-GGUF"));
        assert!(!is_qwen35_27b("Qwen/Qwen3-4B"));
        assert!(!is_qwen35_27b("anthropic/claude-sonnet-4.7"));
    }

    #[test]
    fn voxtral_tts_aliases_resolve() {
        assert!(is_voxtral_4b_tts("engineai/Voxtral-4B-TTS-2603"));
        assert!(is_voxtral_4b_tts("engineai/voxtral-4b-tts-2603"));
        assert!(is_voxtral_4b_tts("voxtral-4b-tts-2603"));
        assert!(!is_voxtral_4b_tts("engineai/Voxtral-Mini-4B-Realtime-2602"));
    }

    #[test]
    fn voxtral_stt_aliases_resolve() {
        assert!(is_voxtral_mini_4b_realtime(
            "engineai/Voxtral-Mini-4B-Realtime-2602"
        ));
        assert!(is_voxtral_mini_4b_realtime(
            "engineai/voxtral-mini-4b-realtime-2602"
        ));
        assert!(is_voxtral_mini_4b_realtime("voxtral-mini-4b-realtime-2602"));
        assert!(!is_voxtral_mini_4b_realtime("engineai/Voxtral-4B-TTS-2603"));
    }

    #[test]
    fn voxtral_stt_backend_assembles_hidden_service_cli() {
        let root = Path::new("/tmp/ctoxroot");
        let backend = resolve_local_model_backend(LocalModelRequest {
            request_model: "engineai/Voxtral-Mini-4B-Realtime-2602",
            transport_endpoint: Some("/tmp/ctoxroot/runtime/sockets/transcription.sock"),
            root,
        })
        .expect("voxtral stt backend must resolve");
        let joined: String = backend
            .args
            .iter()
            .map(|s| s.to_string_lossy().to_string())
            .collect::<Vec<_>>()
            .join(" ");
        assert!(joined.contains("__native-voxtral-stt-service"));
        assert!(joined.contains("--transport"));
        assert!(joined.contains("transcription.sock"));
        assert!(joined.contains("--request-model-alias engineai/Voxtral-Mini-4B-Realtime-2602"));
        assert_eq!(backend.model_id, "voxtral-mini-4b-realtime-2602");
    }

    #[test]
    fn voxtral_tts_backend_assembles_hidden_service_cli() {
        let root = Path::new("/tmp/ctoxroot");
        let backend = resolve_local_model_backend(LocalModelRequest {
            request_model: "engineai/Voxtral-4B-TTS-2603",
            transport_endpoint: Some("/tmp/ctoxroot/runtime/sockets/speech.sock"),
            root,
        })
        .expect("voxtral tts backend must resolve");
        let joined: String = backend
            .args
            .iter()
            .map(|s| s.to_string_lossy().to_string())
            .collect::<Vec<_>>()
            .join(" ");
        assert!(joined.contains("__native-voxtral-tts-service"));
        assert!(joined.contains("--transport"));
        assert!(joined.contains("speech.sock"));
        assert!(joined.contains("--request-model-alias engineai/Voxtral-4B-TTS-2603"));
        assert_eq!(backend.model_id, "voxtral-4b-tts-2603");
    }

    #[test]
    fn qwen35_backend_assembles_expected_cli() {
        let root = Path::new("/tmp/ctoxroot");
        let backend = resolve_local_model_backend(LocalModelRequest {
            request_model: "Qwen/Qwen3.5-27B-Instruct",
            transport_endpoint: Some("/tmp/ctoxroot/runtime/sockets/primary_generation.sock"),
            root,
        })
        .expect("qwen35 backend must resolve");
        let joined: String = backend
            .args
            .iter()
            .map(|s| s.to_string_lossy().to_string())
            .collect::<Vec<_>>()
            .join(" ");
        assert!(joined.contains("--target"));
        assert!(joined.contains("--draft"));
        assert!(joined.contains("--socket"));
        assert!(joined.contains("primary_generation.sock"));
        assert!(joined.contains("--model-id qwen35-27b-q4km-dflash"));
        assert!(joined.contains("--fast-rollback"));
        assert!(joined.contains("--ddtree-budget 22"));
        assert!(joined.contains("--ddtree-temp 0.6"));
        assert!(backend
            .env
            .iter()
            .any(|(key, value)| *key == "LD_LIBRARY_PATH"
                && value.to_string_lossy().contains("ggml-cuda")));
        assert_eq!(backend.model_id, "qwen35-27b-q4km-dflash");
        assert!(backend
            .binary
            .ends_with("src/inference/models/qwen35_27b_q4km_dflash/target/release/qwen35-27b-q4km-dflash-server"));
    }

    #[test]
    fn unsupported_model_returns_none() {
        let root = Path::new("/tmp/ctoxroot");
        assert!(resolve_local_model_backend(LocalModelRequest {
            request_model: "anthropic/claude-sonnet-4.7",
            transport_endpoint: None,
            root,
        })
        .is_none());
    }
}
