use crate::inference::runtime_env;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::Path;

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TicketAdapterBackend {
    NativeRust,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TicketAdapterKind {
    Local,
    Zammad,
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(crate) struct TicketAdapterCapabilities {
    pub can_sync: bool,
    pub can_test: bool,
    pub can_comment_writeback: bool,
    pub can_transition_writeback: bool,
    pub can_create_self_work_items: bool,
    pub can_assign_self_work_items: bool,
    pub can_append_self_work_notes: bool,
    pub can_transition_self_work_items: bool,
    pub can_internal_comments: bool,
    pub can_public_comments: bool,
    pub state_transition_by_name: bool,
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TicketAdapterSpec {
    pub kind: TicketAdapterKind,
    pub backend: TicketAdapterBackend,
    pub runtime_env_keys: &'static [&'static str],
    pub capabilities: TicketAdapterCapabilities,
}

const LOCAL_RUNTIME_ENV_KEYS: &[&str] = &[];

const ZAMMAD_RUNTIME_ENV_KEYS: &[&str] = &[
    "CTO_ZAMMAD_BASE_URL",
    "CTO_ZAMMAD_TOKEN",
    "CTO_ZAMMAD_USER",
    "CTO_ZAMMAD_PASSWORD",
    "CTO_ZAMMAD_HTTP_TIMEOUT_SECS",
    "CTO_ZAMMAD_PAGE_SIZE",
    "CTO_ZAMMAD_ARTICLE_TYPE",
    "CTO_ZAMMAD_COMMENT_INTERNAL",
    "CTO_ZAMMAD_SELF_WORK_GROUP",
    "CTO_ZAMMAD_SELF_WORK_CUSTOMER",
    "CTO_ZAMMAD_SELF_WORK_PRIORITY",
];

impl TicketAdapterKind {
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn spec(self) -> TicketAdapterSpec {
        match self {
            Self::Local => TicketAdapterSpec {
                kind: self,
                backend: TicketAdapterBackend::NativeRust,
                runtime_env_keys: LOCAL_RUNTIME_ENV_KEYS,
                capabilities: TicketAdapterCapabilities {
                    can_sync: true,
                    can_test: true,
                    can_comment_writeback: true,
                    can_transition_writeback: true,
                    can_create_self_work_items: true,
                    can_assign_self_work_items: true,
                    can_append_self_work_notes: true,
                    can_transition_self_work_items: true,
                    can_internal_comments: true,
                    can_public_comments: true,
                    state_transition_by_name: true,
                },
            },
            Self::Zammad => TicketAdapterSpec {
                kind: self,
                backend: TicketAdapterBackend::NativeRust,
                runtime_env_keys: ZAMMAD_RUNTIME_ENV_KEYS,
                capabilities: TicketAdapterCapabilities {
                    can_sync: true,
                    can_test: true,
                    can_comment_writeback: true,
                    can_transition_writeback: true,
                    can_create_self_work_items: true,
                    can_assign_self_work_items: true,
                    can_append_self_work_notes: true,
                    can_transition_self_work_items: true,
                    can_internal_comments: true,
                    can_public_comments: true,
                    state_transition_by_name: true,
                },
            },
        }
    }
}

pub(crate) fn runtime_settings_from_root(
    root: &Path,
    _kind: TicketAdapterKind,
) -> BTreeMap<String, String> {
    runtime_env::effective_runtime_env_map(root).unwrap_or_default()
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn runtime_settings_from_settings(
    root: &Path,
    kind: TicketAdapterKind,
    settings: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    let mut merged = runtime_settings_from_root(root, kind);
    merged.extend(settings.clone());
    merged
}

#[cfg(test)]
mod tests {
    use super::{runtime_settings_from_settings, TicketAdapterBackend, TicketAdapterKind};
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    #[test]
    fn ticket_adapter_specs_use_native_backends() {
        assert_eq!(
            TicketAdapterKind::Local.spec().backend,
            TicketAdapterBackend::NativeRust
        );
        assert_eq!(
            TicketAdapterKind::Zammad.spec().backend,
            TicketAdapterBackend::NativeRust
        );
    }

    #[test]
    fn runtime_settings_preserve_explicit_zammad_configuration() {
        let root = PathBuf::from("/tmp/ctox-root");
        let mut settings = BTreeMap::new();
        settings.insert(
            "CTO_ZAMMAD_BASE_URL".to_string(),
            "https://zammad.example.test".to_string(),
        );
        let merged = runtime_settings_from_settings(&root, TicketAdapterKind::Zammad, &settings);
        assert_eq!(
            merged.get("CTO_ZAMMAD_BASE_URL").map(String::as_str),
            Some("https://zammad.example.test")
        );
    }
}
