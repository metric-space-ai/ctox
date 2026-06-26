use anyhow::Result;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;

use crate::communication::discord_native as communication_discord_native;
use crate::communication::email_native as communication_email_native;
use crate::communication::gateway as communication_gateway;
use crate::communication::gateway::CommunicationAdapterBackend;
use crate::communication::gateway::CommunicationAdapterKind;
use crate::communication::google_chat_native as communication_google_chat_native;
use crate::communication::jami_native as communication_jami_native;
use crate::communication::matrix_native as communication_matrix_native;
use crate::communication::mattermost_native as communication_mattermost_native;
use crate::communication::meeting_native as communication_meeting_native;
use crate::communication::slack_native as communication_slack_native;
use crate::communication::teams_native as communication_teams_native;
use crate::communication::telegram_native as communication_telegram_native;
use crate::communication::whatsapp_native as communication_whatsapp_native;
use crate::communication::zulip_native as communication_zulip_native;

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
            CommunicationAdapterKind::Discord => "discord",
            CommunicationAdapterKind::Email => "email",
            CommunicationAdapterKind::GoogleChat => "google_chat",
            CommunicationAdapterKind::Jami => "jami",
            CommunicationAdapterKind::Matrix => "matrix",
            CommunicationAdapterKind::Mattermost => "mattermost",
            CommunicationAdapterKind::Meeting => "meeting",
            CommunicationAdapterKind::Slack => "slack",
            CommunicationAdapterKind::Teams => "teams",
            CommunicationAdapterKind::Telegram => "telegram",
            CommunicationAdapterKind::Whatsapp => "whatsapp",
            CommunicationAdapterKind::Zulip => "zulip",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DiscordAdapter;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct EmailAdapter;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GoogleChatAdapter;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct JamiAdapter;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MatrixAdapter;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MattermostAdapter;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MeetingAdapter;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SlackAdapter;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TeamsAdapter;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TelegramAdapter;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WhatsappAdapter;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ZulipAdapter;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExternalCommunicationAdapter {
    Discord(DiscordAdapter),
    Email(EmailAdapter),
    GoogleChat(GoogleChatAdapter),
    Jami(JamiAdapter),
    Matrix(MatrixAdapter),
    Mattermost(MattermostAdapter),
    Meeting(MeetingAdapter),
    Slack(SlackAdapter),
    Teams(TeamsAdapter),
    Telegram(TelegramAdapter),
    Whatsapp(WhatsappAdapter),
    Zulip(ZulipAdapter),
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
    pub attachments: &'a [String],
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
    pub attachments: &'a [String],
}

pub(crate) struct ChatSendCommandRequest<'a> {
    pub db_path: &'a Path,
    pub account_key: &'a str,
    pub thread_key: &'a str,
    pub to: &'a [String],
    pub cc: &'a [String],
    pub sender_display: Option<&'a str>,
    pub subject: &'a str,
    pub body: &'a str,
    pub attachments: &'a [String],
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

pub(crate) struct ChatTestCommandRequest<'a> {
    pub db_path: &'a Path,
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
    pub attachments: &'a [String],
}

pub(crate) struct TeamsTestCommandRequest<'a> {
    pub db_path: &'a Path,
    pub tenant_id: &'a str,
    pub profile_json: &'a Value,
}

pub(crate) struct WhatsappSendCommandRequest<'a> {
    pub db_path: &'a Path,
    pub account_key: &'a str,
    pub thread_key: &'a str,
    pub to: &'a [String],
    pub sender_display: Option<&'a str>,
    pub body: &'a str,
    pub attachments: &'a [String],
}

pub(crate) struct WhatsappTestCommandRequest<'a> {
    pub db_path: &'a Path,
    pub account_key: Option<&'a str>,
}

pub(crate) fn email() -> EmailAdapter {
    EmailAdapter
}

pub(crate) fn discord() -> DiscordAdapter {
    DiscordAdapter
}

pub(crate) fn google_chat() -> GoogleChatAdapter {
    GoogleChatAdapter
}

pub(crate) fn jami() -> JamiAdapter {
    JamiAdapter
}

pub(crate) fn matrix() -> MatrixAdapter {
    MatrixAdapter
}

pub(crate) fn mattermost() -> MattermostAdapter {
    MattermostAdapter
}

pub(crate) fn meeting() -> MeetingAdapter {
    MeetingAdapter
}

pub(crate) fn slack() -> SlackAdapter {
    SlackAdapter
}

pub(crate) fn teams() -> TeamsAdapter {
    TeamsAdapter
}

pub(crate) fn telegram() -> TelegramAdapter {
    TelegramAdapter
}

pub(crate) fn whatsapp() -> WhatsappAdapter {
    WhatsappAdapter
}

pub(crate) fn zulip() -> ZulipAdapter {
    ZulipAdapter
}

pub(crate) fn external_adapter_for_channel(channel: &str) -> Option<ExternalCommunicationAdapter> {
    match channel {
        "discord" => Some(ExternalCommunicationAdapter::Discord(discord())),
        "email" => Some(ExternalCommunicationAdapter::Email(email())),
        "google_chat" => Some(ExternalCommunicationAdapter::GoogleChat(google_chat())),
        "jami" => Some(ExternalCommunicationAdapter::Jami(jami())),
        "matrix" => Some(ExternalCommunicationAdapter::Matrix(matrix())),
        "mattermost" => Some(ExternalCommunicationAdapter::Mattermost(mattermost())),
        "meeting" => Some(ExternalCommunicationAdapter::Meeting(meeting())),
        "slack" => Some(ExternalCommunicationAdapter::Slack(slack())),
        "teams" => Some(ExternalCommunicationAdapter::Teams(teams())),
        "telegram" => Some(ExternalCommunicationAdapter::Telegram(telegram())),
        "whatsapp" => Some(ExternalCommunicationAdapter::Whatsapp(whatsapp())),
        "zulip" => Some(ExternalCommunicationAdapter::Zulip(zulip())),
        _ => None,
    }
}

impl CommunicationTransportAdapter for DiscordAdapter {
    fn kind(&self) -> CommunicationAdapterKind {
        CommunicationAdapterKind::Discord
    }
}

impl CommunicationTransportAdapter for EmailAdapter {
    fn kind(&self) -> CommunicationAdapterKind {
        CommunicationAdapterKind::Email
    }
}

impl CommunicationTransportAdapter for GoogleChatAdapter {
    fn kind(&self) -> CommunicationAdapterKind {
        CommunicationAdapterKind::GoogleChat
    }
}

impl CommunicationTransportAdapter for JamiAdapter {
    fn kind(&self) -> CommunicationAdapterKind {
        CommunicationAdapterKind::Jami
    }
}

impl CommunicationTransportAdapter for MatrixAdapter {
    fn kind(&self) -> CommunicationAdapterKind {
        CommunicationAdapterKind::Matrix
    }
}

impl CommunicationTransportAdapter for MattermostAdapter {
    fn kind(&self) -> CommunicationAdapterKind {
        CommunicationAdapterKind::Mattermost
    }
}

impl CommunicationTransportAdapter for MeetingAdapter {
    fn kind(&self) -> CommunicationAdapterKind {
        CommunicationAdapterKind::Meeting
    }
}

impl CommunicationTransportAdapter for SlackAdapter {
    fn kind(&self) -> CommunicationAdapterKind {
        CommunicationAdapterKind::Slack
    }
}

impl CommunicationTransportAdapter for TeamsAdapter {
    fn kind(&self) -> CommunicationAdapterKind {
        CommunicationAdapterKind::Teams
    }
}

impl CommunicationTransportAdapter for TelegramAdapter {
    fn kind(&self) -> CommunicationAdapterKind {
        CommunicationAdapterKind::Telegram
    }
}

impl CommunicationTransportAdapter for WhatsappAdapter {
    fn kind(&self) -> CommunicationAdapterKind {
        CommunicationAdapterKind::Whatsapp
    }
}

impl CommunicationTransportAdapter for ZulipAdapter {
    fn kind(&self) -> CommunicationAdapterKind {
        CommunicationAdapterKind::Zulip
    }
}

macro_rules! impl_chat_adapter {
    ($adapter:ty, $module:ident) => {
        impl $adapter {
            pub(crate) fn sync_cli(
                self,
                root: &Path,
                request: &AdapterSyncCommandRequest<'_>,
            ) -> Result<Value> {
                let runtime = communication_gateway::runtime_settings_from_root(root, self.kind());
                $module::sync(root, &runtime, request)
            }

            pub(crate) fn send_cli(
                self,
                root: &Path,
                request: &ChatSendCommandRequest<'_>,
            ) -> Result<Value> {
                let runtime = communication_gateway::runtime_settings_from_root(root, self.kind());
                $module::send(root, &runtime, request)
            }

            pub(crate) fn test_cli(
                self,
                root: &Path,
                request: &ChatTestCommandRequest<'_>,
            ) -> Result<Value> {
                let runtime = communication_gateway::runtime_settings_from_root(root, self.kind());
                $module::test(root, &runtime, request)
            }

            pub(crate) fn service_sync(
                self,
                root: &Path,
                settings: &BTreeMap<String, String>,
            ) -> Result<Option<Value>> {
                $module::service_sync(root, settings)
            }
        }
    };
}

impl_chat_adapter!(DiscordAdapter, communication_discord_native);
impl_chat_adapter!(GoogleChatAdapter, communication_google_chat_native);
impl_chat_adapter!(MatrixAdapter, communication_matrix_native);
impl_chat_adapter!(MattermostAdapter, communication_mattermost_native);
impl_chat_adapter!(SlackAdapter, communication_slack_native);
impl_chat_adapter!(TelegramAdapter, communication_telegram_native);
impl_chat_adapter!(ZulipAdapter, communication_zulip_native);

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

impl WhatsappAdapter {
    pub(crate) fn sync_cli(
        self,
        root: &Path,
        request: &AdapterSyncCommandRequest<'_>,
    ) -> Result<Value> {
        let runtime = communication_gateway::runtime_settings_from_root(root, self.kind());
        communication_whatsapp_native::sync(root, &runtime, request)
    }

    pub(crate) fn send_cli(
        self,
        root: &Path,
        request: &WhatsappSendCommandRequest<'_>,
    ) -> Result<Value> {
        let runtime = communication_gateway::runtime_settings_from_root(root, self.kind());
        communication_whatsapp_native::send(root, &runtime, request)
    }

    pub(crate) fn test_cli(
        self,
        root: &Path,
        request: &WhatsappTestCommandRequest<'_>,
    ) -> Result<Value> {
        let runtime = communication_gateway::runtime_settings_from_root(root, self.kind());
        communication_whatsapp_native::test(root, &runtime, request)
    }

    pub(crate) fn service_sync(
        self,
        root: &Path,
        settings: &BTreeMap<String, String>,
    ) -> Result<Option<Value>> {
        communication_whatsapp_native::service_sync(root, settings)
    }
}

#[cfg(test)]
mod tests {
    use super::discord;
    use super::email;
    use super::external_adapter_for_channel;
    use super::google_chat;
    use super::jami;
    use super::matrix;
    use super::mattermost;
    use super::meeting;
    use super::slack;
    use super::teams;
    use super::telegram;
    use super::whatsapp;
    use super::zulip;
    use super::CommunicationTransportAdapter;
    use super::ExternalCommunicationAdapter;
    use super::JamiResolveAccountCommandRequest;
    use crate::communication::gateway::CommunicationAdapterBackend;

    #[test]
    fn registry_resolves_supported_external_channels() {
        assert_eq!(
            external_adapter_for_channel("discord"),
            Some(ExternalCommunicationAdapter::Discord(discord()))
        );
        assert_eq!(
            external_adapter_for_channel("email"),
            Some(ExternalCommunicationAdapter::Email(email()))
        );
        assert_eq!(
            external_adapter_for_channel("google_chat"),
            Some(ExternalCommunicationAdapter::GoogleChat(google_chat()))
        );
        assert_eq!(
            external_adapter_for_channel("jami"),
            Some(ExternalCommunicationAdapter::Jami(jami()))
        );
        assert_eq!(
            external_adapter_for_channel("matrix"),
            Some(ExternalCommunicationAdapter::Matrix(matrix()))
        );
        assert_eq!(
            external_adapter_for_channel("mattermost"),
            Some(ExternalCommunicationAdapter::Mattermost(mattermost()))
        );
        assert_eq!(
            external_adapter_for_channel("meeting"),
            Some(ExternalCommunicationAdapter::Meeting(meeting()))
        );
        assert_eq!(
            external_adapter_for_channel("slack"),
            Some(ExternalCommunicationAdapter::Slack(slack()))
        );
        assert_eq!(
            external_adapter_for_channel("teams"),
            Some(ExternalCommunicationAdapter::Teams(teams()))
        );
        assert_eq!(
            external_adapter_for_channel("telegram"),
            Some(ExternalCommunicationAdapter::Telegram(telegram()))
        );
        assert_eq!(
            external_adapter_for_channel("whatsapp"),
            Some(ExternalCommunicationAdapter::Whatsapp(whatsapp()))
        );
        assert_eq!(
            external_adapter_for_channel("zulip"),
            Some(ExternalCommunicationAdapter::Zulip(zulip()))
        );
        assert_eq!(external_adapter_for_channel("tui"), None);
    }

    #[test]
    fn adapters_report_native_backends() {
        assert_eq!(discord().backend(), CommunicationAdapterBackend::NativeRust);
        assert_eq!(email().backend(), CommunicationAdapterBackend::NativeRust);
        assert_eq!(
            google_chat().backend(),
            CommunicationAdapterBackend::NativeRust
        );
        assert_eq!(jami().backend(), CommunicationAdapterBackend::NativeRust);
        assert_eq!(matrix().backend(), CommunicationAdapterBackend::NativeRust);
        assert_eq!(
            mattermost().backend(),
            CommunicationAdapterBackend::NativeRust
        );
        assert_eq!(meeting().backend(), CommunicationAdapterBackend::NativeRust);
        assert_eq!(slack().backend(), CommunicationAdapterBackend::NativeRust);
        assert_eq!(teams().backend(), CommunicationAdapterBackend::NativeRust);
        assert_eq!(
            telegram().backend(),
            CommunicationAdapterBackend::NativeRust
        );
        assert_eq!(
            whatsapp().backend(),
            CommunicationAdapterBackend::NativeRust
        );
        assert_eq!(zulip().backend(), CommunicationAdapterBackend::NativeRust);
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
