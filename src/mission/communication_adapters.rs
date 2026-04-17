use anyhow::Result;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;

use crate::mission::communication_email_native;
use crate::mission::communication_gateway;
use crate::mission::communication_gateway::CommunicationAdapterBackend;
use crate::mission::communication_gateway::CommunicationAdapterKind;
use crate::mission::communication_jami_native;
use crate::mission::communication_meeting_native;
use crate::mission::communication_teams_native;

// External communication transports enter CTOX only through this module.
// New transports should:
// 1. add the transport identity/spec to `communication_gateway`;
// 2. add one native Rust adapter module that owns `sync`, `send`, `test`, and service sync wiring;
// 3. keep the rest of CTOX on typed adapter requests instead of ad hoc process assembly.

pub(crate) trait CommunicationTransportAdapter {
    fn kind(&self) -> CommunicationAdapterKind;

    #[cfg_attr(not(test), allow(dead_code))]
    fn backend(&self) -> CommunicationAdapterBackend {
        self.kind().spec().backend
    }

    fn channel_name(&self) -> &'static str {
        match self.kind() {
            CommunicationAdapterKind::Email => "email",
            CommunicationAdapterKind::Jami => "jami",
            CommunicationAdapterKind::Meeting => "meeting",
            CommunicationAdapterKind::Teams => "teams",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct EmailAdapter;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct JamiAdapter;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MeetingAdapter;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TeamsAdapter;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExternalCommunicationAdapter {
    Email(EmailAdapter),
    Jami(JamiAdapter),
    Meeting(MeetingAdapter),
    Teams(TeamsAdapter),
}

pub(crate) struct AdapterSyncCommandRequest<'a> {
    pub db_path: &'a Path,
    pub passthrough_args: &'a [String],
    #[cfg_attr(not(test), allow(dead_code))]
    pub skip_flags: &'a [&'a str],
}

pub(crate) struct EmailSendCommandRequest<'a> {
    pub db_path: &'a Path,
    pub sender_email: &'a str,
    pub provider: Option<&'a str>,
    pub profile_json: Option<&'a Value>,
    pub thread_key: &'a str,
    pub to: &'a [String],
    pub cc: &'a [String],
    pub sender_display: Option<&'a str>,
    pub subject: &'a str,
    pub body: &'a str,
}

pub(crate) struct JamiSendCommandRequest<'a> {
    pub db_path: &'a Path,
    pub account_id: &'a str,
    pub thread_key: &'a str,
    pub to: &'a [String],
    pub sender_display: Option<&'a str>,
    pub subject: &'a str,
    pub body: &'a str,
    pub send_voice: bool,
}

pub(crate) struct EmailTestCommandRequest<'a> {
    pub db_path: &'a Path,
    pub email_address: &'a str,
    pub provider: &'a str,
    pub profile_json: &'a Value,
}

pub(crate) struct JamiTestCommandRequest<'a> {
    pub db_path: &'a Path,
    pub account_id: &'a str,
    pub provider: &'a str,
    pub profile_json: &'a Value,
}

pub(crate) struct JamiResolveAccountCommandRequest<'a> {
    pub account_id: Option<&'a str>,
    pub profile_name: Option<&'a str>,
}

pub(crate) struct MeetingSendCommandRequest<'a> {
    pub db_path: &'a Path,
    pub session_id: &'a str,
    pub body: &'a str,
}

pub(crate) struct TeamsSendCommandRequest<'a> {
    pub db_path: &'a Path,
    pub tenant_id: &'a str,
    pub thread_key: &'a str,
    pub to: &'a [String],
    pub sender_display: Option<&'a str>,
    pub subject: &'a str,
    pub body: &'a str,
}

pub(crate) struct TeamsTestCommandRequest<'a> {
    pub db_path: &'a Path,
    pub tenant_id: &'a str,
    pub profile_json: &'a Value,
}

pub(crate) fn email() -> EmailAdapter {
    EmailAdapter
}

pub(crate) fn jami() -> JamiAdapter {
    JamiAdapter
}

pub(crate) fn meeting() -> MeetingAdapter {
    MeetingAdapter
}

pub(crate) fn teams() -> TeamsAdapter {
    TeamsAdapter
}

pub(crate) fn external_adapter_for_channel(channel: &str) -> Option<ExternalCommunicationAdapter> {
    match channel {
        "email" => Some(ExternalCommunicationAdapter::Email(email())),
        "jami" => Some(ExternalCommunicationAdapter::Jami(jami())),
        "meeting" => Some(ExternalCommunicationAdapter::Meeting(meeting())),
        "teams" => Some(ExternalCommunicationAdapter::Teams(teams())),
        _ => None,
    }
}

impl CommunicationTransportAdapter for EmailAdapter {
    fn kind(&self) -> CommunicationAdapterKind {
        CommunicationAdapterKind::Email
    }
}

impl CommunicationTransportAdapter for JamiAdapter {
    fn kind(&self) -> CommunicationAdapterKind {
        CommunicationAdapterKind::Jami
    }
}

impl CommunicationTransportAdapter for MeetingAdapter {
    fn kind(&self) -> CommunicationAdapterKind {
        CommunicationAdapterKind::Meeting
    }
}

impl CommunicationTransportAdapter for TeamsAdapter {
    fn kind(&self) -> CommunicationAdapterKind {
        CommunicationAdapterKind::Teams
    }
}

impl EmailAdapter {
    pub(crate) fn sync_cli(
        self,
        root: &Path,
        request: &AdapterSyncCommandRequest<'_>,
    ) -> Result<Value> {
        let runtime = communication_gateway::runtime_settings_from_root(root, self.kind());
        communication_email_native::sync(root, &runtime, request)
    }

    pub(crate) fn send_cli(
        self,
        root: &Path,
        request: &EmailSendCommandRequest<'_>,
    ) -> Result<Value> {
        let runtime = communication_gateway::runtime_settings_from_root(root, self.kind());
        communication_email_native::send(root, &runtime, request)
    }

    pub(crate) fn test_cli(
        self,
        root: &Path,
        request: &EmailTestCommandRequest<'_>,
    ) -> Result<Value> {
        let runtime = communication_gateway::runtime_settings_from_root(root, self.kind());
        communication_email_native::test(root, &runtime, request)
    }

    pub(crate) fn service_sync(
        self,
        root: &Path,
        settings: &BTreeMap<String, String>,
    ) -> Result<Option<Value>> {
        communication_email_native::service_sync(root, settings)
    }
}

impl JamiAdapter {
    pub(crate) fn sync_cli(
        self,
        root: &Path,
        request: &AdapterSyncCommandRequest<'_>,
    ) -> Result<Value> {
        let runtime = communication_gateway::runtime_settings_from_root(root, self.kind());
        communication_jami_native::sync(root, &runtime, request)
    }

    pub(crate) fn send_cli(
        self,
        root: &Path,
        request: &JamiSendCommandRequest<'_>,
    ) -> Result<Value> {
        let runtime = communication_gateway::runtime_settings_from_root(root, self.kind());
        communication_jami_native::send(root, &runtime, request)
    }

    pub(crate) fn test_cli(
        self,
        root: &Path,
        request: &JamiTestCommandRequest<'_>,
    ) -> Result<Value> {
        let runtime = communication_gateway::runtime_settings_from_root(root, self.kind());
        communication_jami_native::test(root, &runtime, request)
    }

    pub(crate) fn service_sync(
        self,
        root: &Path,
        settings: &BTreeMap<String, String>,
    ) -> Result<Option<Value>> {
        communication_jami_native::service_sync(root, settings)
    }

    pub(crate) fn resolve_account(
        self,
        root: &Path,
        request: &JamiResolveAccountCommandRequest<'_>,
    ) -> Result<Value> {
        let runtime = communication_gateway::runtime_settings_from_root(root, self.kind());
        communication_jami_native::resolve_account(root, &runtime, request)
    }
}

impl MeetingAdapter {
    pub(crate) fn sync_cli(
        self,
        root: &Path,
        request: &AdapterSyncCommandRequest<'_>,
    ) -> Result<Value> {
        let runtime = communication_gateway::runtime_settings_from_root(root, self.kind());
        communication_meeting_native::sync(root, &runtime, request)
    }

    pub(crate) fn send_cli(
        self,
        root: &Path,
        request: &MeetingSendCommandRequest<'_>,
    ) -> Result<Value> {
        let runtime = communication_gateway::runtime_settings_from_root(root, self.kind());
        communication_meeting_native::send(root, &runtime, request)
    }

    pub(crate) fn service_sync(
        self,
        root: &Path,
        settings: &BTreeMap<String, String>,
    ) -> Result<Option<Value>> {
        communication_meeting_native::service_sync(root, settings)
    }
}

impl TeamsAdapter {
    pub(crate) fn sync_cli(
        self,
        root: &Path,
        request: &AdapterSyncCommandRequest<'_>,
    ) -> Result<Value> {
        let runtime = communication_gateway::runtime_settings_from_root(root, self.kind());
        communication_teams_native::sync(root, &runtime, request)
    }

    pub(crate) fn send_cli(
        self,
        root: &Path,
        request: &TeamsSendCommandRequest<'_>,
    ) -> Result<Value> {
        let runtime = communication_gateway::runtime_settings_from_root(root, self.kind());
        communication_teams_native::send(root, &runtime, request)
    }

    pub(crate) fn test_cli(
        self,
        root: &Path,
        request: &TeamsTestCommandRequest<'_>,
    ) -> Result<Value> {
        let runtime = communication_gateway::runtime_settings_from_root(root, self.kind());
        communication_teams_native::test(root, &runtime, request)
    }

    pub(crate) fn service_sync(
        self,
        root: &Path,
        settings: &BTreeMap<String, String>,
    ) -> Result<Option<Value>> {
        communication_teams_native::service_sync(root, settings)
    }
}

#[cfg(test)]
mod tests {
    use super::email;
    use super::external_adapter_for_channel;
    use super::jami;
    use super::meeting;
    use super::teams;
    use super::CommunicationTransportAdapter;
    use super::ExternalCommunicationAdapter;
    use super::JamiResolveAccountCommandRequest;
    use crate::mission::communication_gateway::CommunicationAdapterBackend;

    #[test]
    fn registry_resolves_supported_external_channels() {
        assert_eq!(
            external_adapter_for_channel("email"),
            Some(ExternalCommunicationAdapter::Email(email()))
        );
        assert_eq!(
            external_adapter_for_channel("jami"),
            Some(ExternalCommunicationAdapter::Jami(jami()))
        );
        assert_eq!(
            external_adapter_for_channel("meeting"),
            Some(ExternalCommunicationAdapter::Meeting(meeting()))
        );
        assert_eq!(
            external_adapter_for_channel("teams"),
            Some(ExternalCommunicationAdapter::Teams(teams()))
        );
        assert_eq!(external_adapter_for_channel("tui"), None);
    }

    #[test]
    fn adapters_report_native_backends() {
        assert_eq!(email().backend(), CommunicationAdapterBackend::NativeRust);
        assert_eq!(jami().backend(), CommunicationAdapterBackend::NativeRust);
        assert_eq!(meeting().backend(), CommunicationAdapterBackend::NativeRust);
        assert_eq!(teams().backend(), CommunicationAdapterBackend::NativeRust);
    }

    #[test]
    fn resolve_account_request_stays_typed() {
        let request = JamiResolveAccountCommandRequest {
            account_id: Some("ring:abc"),
            profile_name: Some("CTOX"),
        };
        assert_eq!(request.account_id, Some("ring:abc"));
        assert_eq!(request.profile_name, Some("CTOX"));
    }
}
