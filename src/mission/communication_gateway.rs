use std::collections::BTreeMap;
use std::path::Path;

use crate::inference::runtime_env;
use crate::inference::runtime_kernel;

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CommunicationAdapterBackend {
    NativeRust,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CommunicationAdapterKind {
    Email,
    Jami,
    Meeting,
    Teams,
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CommunicationAdapterSpec {
    pub kind: CommunicationAdapterKind,
    pub backend: CommunicationAdapterBackend,
    pub runtime_env_keys: &'static [&'static str],
}

const EMAIL_RUNTIME_ENV_KEYS: &[&str] = &[
    "CTO_EMAIL_ADDRESS",
    "CTO_EMAIL_PROVIDER",
    "CTO_EMAIL_PASSWORD",
    "CTO_EMAIL_GRAPH_ACCESS_TOKEN",
    "CTO_EMAIL_GRAPH_BASE_URL",
    "CTO_EMAIL_GRAPH_USER",
    "CTO_EMAIL_EWS_URL",
    "CTO_EMAIL_OWA_URL",
    "CTO_EMAIL_EWS_VERSION",
    "CTO_EMAIL_EWS_AUTH_TYPE",
    "CTO_EMAIL_EWS_USERNAME",
    "CTO_EMAIL_EWS_BEARER_TOKEN",
    "CTO_EMAIL_ACTIVESYNC_SERVER",
    "CTO_EMAIL_ACTIVESYNC_USERNAME",
    "CTO_EMAIL_ACTIVESYNC_PATH",
    "CTO_EMAIL_ACTIVESYNC_DEVICE_ID",
    "CTO_EMAIL_ACTIVESYNC_DEVICE_TYPE",
    "CTO_EMAIL_ACTIVESYNC_PROTOCOL_VERSION",
    "CTO_EMAIL_ACTIVESYNC_POLICY_KEY",
    "CTO_EMAIL_VERIFY_SEND",
    "CTO_EMAIL_SENT_VERIFY_WINDOW_SECONDS",
];

const TEAMS_RUNTIME_ENV_KEYS: &[&str] = &[
    "CTO_TEAMS_USERNAME",
    "CTO_TEAMS_PASSWORD",
    "CTO_TEAMS_TENANT_ID",
    "CTO_TEAMS_CLIENT_ID",
    "CTO_TEAMS_CLIENT_SECRET",
    "CTO_TEAMS_BOT_ID",
    "CTO_TEAMS_GRAPH_BASE_URL",
    "CTO_TEAMS_TEAM_ID",
    "CTO_TEAMS_CHANNEL_ID",
    "CTO_TEAMS_CHAT_ID",
];

const MEETING_RUNTIME_ENV_KEYS: &[&str] = &[
    "CTO_MEETING_BOT_NAME",
    "CTO_MEETING_MAX_DURATION_MINUTES",
    "CTO_MEETING_AUDIO_CHUNK_SECONDS",
    "CTOX_PROXY_HOST",
    "CTOX_PROXY_PORT",
    "CTOX_STT_MODEL",
];

const JAMI_RUNTIME_ENV_KEYS: &[&str] = &[
    "CTOX_PROXY_HOST",
    "CTOX_PROXY_PORT",
    "CTOX_STT_MODEL",
    "CTOX_TTS_MODEL",
    "CTO_JAMI_TTS_VOICE",
    "CTO_JAMI_DBUS_ENV_FILE",
    "CTO_JAMI_INBOX_DIR",
    "CTO_JAMI_OUTBOX_DIR",
    "CTO_JAMI_ARCHIVE_DIR",
    "CTO_JAMI_PROFILE_NAME",
    "CTO_JAMI_ACCOUNT_ID",
];

impl CommunicationAdapterKind {
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn spec(self) -> CommunicationAdapterSpec {
        match self {
            Self::Email => CommunicationAdapterSpec {
                kind: self,
                backend: CommunicationAdapterBackend::NativeRust,
                runtime_env_keys: EMAIL_RUNTIME_ENV_KEYS,
            },
            Self::Jami => CommunicationAdapterSpec {
                kind: self,
                backend: CommunicationAdapterBackend::NativeRust,
                runtime_env_keys: JAMI_RUNTIME_ENV_KEYS,
            },
            Self::Meeting => CommunicationAdapterSpec {
                kind: self,
                backend: CommunicationAdapterBackend::NativeRust,
                runtime_env_keys: MEETING_RUNTIME_ENV_KEYS,
            },
            Self::Teams => CommunicationAdapterSpec {
                kind: self,
                backend: CommunicationAdapterBackend::NativeRust,
                runtime_env_keys: TEAMS_RUNTIME_ENV_KEYS,
            },
        }
    }
}

pub(crate) fn runtime_settings_from_root(
    root: &Path,
    kind: CommunicationAdapterKind,
) -> BTreeMap<String, String> {
    let mut settings = runtime_env::effective_operator_env_map(root).unwrap_or_default();
    apply_kernel_runtime_settings(kind, root, &mut settings);
    settings
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn runtime_settings_from_settings(
    root: &Path,
    kind: CommunicationAdapterKind,
    settings: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    let mut merged = runtime_env::load_runtime_env_map(root).unwrap_or_default();
    merged.extend(settings.clone());
    apply_kernel_runtime_settings(kind, root, &mut merged);
    merged
}

fn apply_kernel_runtime_settings(
    kind: CommunicationAdapterKind,
    root: &Path,
    settings: &mut BTreeMap<String, String>,
) {
    if !matches!(
        kind,
        CommunicationAdapterKind::Jami | CommunicationAdapterKind::Meeting
    ) {
        return;
    }
    let Ok(resolved) = runtime_kernel::InferenceRuntimeKernel::resolve(root) else {
        return;
    };
    settings.insert("CTOX_PROXY_HOST".to_string(), resolved.proxy.listen_host);
    settings.insert(
        "CTOX_PROXY_PORT".to_string(),
        resolved.proxy.listen_port.to_string(),
    );
    if let Some(model) = resolved
        .proxy
        .transcription_model
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        settings.insert("CTOX_STT_MODEL".to_string(), model.to_string());
    }
    if let Some(model) = resolved
        .proxy
        .speech_model
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        settings.insert("CTOX_TTS_MODEL".to_string(), model.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::{
        runtime_settings_from_settings, CommunicationAdapterBackend, CommunicationAdapterKind,
    };
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    #[test]
    fn communication_adapter_specs_use_native_backends() {
        assert_eq!(
            CommunicationAdapterKind::Email.spec().backend,
            CommunicationAdapterBackend::NativeRust
        );
        assert_eq!(
            CommunicationAdapterKind::Jami.spec().backend,
            CommunicationAdapterBackend::NativeRust
        );
        assert_eq!(
            CommunicationAdapterKind::Meeting.spec().backend,
            CommunicationAdapterBackend::NativeRust
        );
        assert_eq!(
            CommunicationAdapterKind::Teams.spec().backend,
            CommunicationAdapterBackend::NativeRust
        );
    }

    #[test]
    fn runtime_settings_preserve_explicit_email_configuration() {
        let root = PathBuf::from("/tmp/ctox-root");
        let mut settings = BTreeMap::new();
        settings.insert(
            "CTO_EMAIL_ADDRESS".to_string(),
            "owner@example.com".to_string(),
        );
        let merged =
            runtime_settings_from_settings(&root, CommunicationAdapterKind::Email, &settings);
        assert_eq!(
            merged.get("CTO_EMAIL_ADDRESS").map(String::as_str),
            Some("owner@example.com")
        );
    }
}
