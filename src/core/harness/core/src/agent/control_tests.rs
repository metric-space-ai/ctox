use super::*;
use crate::CodexAuth;
use crate::CodexThread;
use crate::ThreadManager;
use crate::agent::agent_status_from_event;
use crate::config::AgentRoleConfig;
use crate::config::Config;
use crate::config::ConfigBuilder;
use crate::config_loader::LoaderOverrides;
use crate::contextual_user_message::SUBAGENT_NOTIFICATION_OPEN_TAG;
use crate::features::Feature;
use crate::model_provider_info::OLLAMA_OSS_PROVIDER_ID;
use crate::model_provider_info::WireApi;
use crate::model_provider_info::create_oss_provider_with_base_url;
use assert_matches::assert_matches;
use ctox_protocol::config_types::ModeKind;
use ctox_protocol::models::ContentItem;
use ctox_protocol::models::ResponseItem;
use ctox_protocol::protocol::ErrorEvent;
use ctox_protocol::protocol::EventMsg;
use ctox_protocol::protocol::SessionSource;
use ctox_protocol::protocol::SubAgentSource;
use ctox_protocol::protocol::TurnAbortReason;
use ctox_protocol::protocol::TurnAbortedEvent;
use ctox_protocol::protocol::TurnCompleteEvent;
use ctox_protocol::protocol::TurnStartedEvent;
use pretty_assertions::assert_eq;
use tempfile::TempDir;
use tokio::time::Duration;
use tokio::time::sleep;
use tokio::time::timeout;
use toml::Value as TomlValue;

async fn test_config_with_cli_overrides(
    cli_overrides: Vec<(String, TomlValue)>,
) -> (TempDir, Config) {
    let home = TempDir::new().expect("create temp dir");
    let config = ConfigBuilder::default()
        .codex_home(home.path().to_path_buf())
        .cli_overrides(cli_overrides)
        .loader_overrides(LoaderOverrides {
            #[cfg(target_os = "macos")]
            managed_preferences_base64: Some(String::new()),
            macos_managed_config_requirements_base64: Some(String::new()),
            ..LoaderOverrides::default()
        })
        .build()
        .await
        .expect("load default test config");
    (home, config)
}

async fn test_config() -> (TempDir, Config) {
    test_config_with_cli_overrides(Vec::new()).await
}

fn text_input(text: &str) -> Vec<UserInput> {
    vec![UserInput::Text {
        text: text.to_string(),
        text_elements: Vec::new(),
    }]
}

struct AgentControlHarness {
    _home: TempDir,
    config: Config,
    manager: ThreadManager,
    control: AgentControl,
}

impl AgentControlHarness {
    async fn new() -> Self {
        let (home, config) = test_config().await;
        let manager = ThreadManager::with_models_provider_and_home_for_tests(
            CodexAuth::from_api_key("dummy"),
            config.model_provider.clone(),
            config.codex_home.clone(),
        );
        let control = manager.agent_control();
        Self {
            _home: home,
            config,
            manager,
            control,
        }
    }

    async fn start_thread(&self) -> (ThreadId, Arc<CodexThread>) {
        let new_thread = self
            .manager
            .start_thread(self.config.clone())
            .await
            .expect("start thread");
        (new_thread.thread_id, new_thread.thread)
    }
}

fn has_subagent_notification(history_items: &[ResponseItem]) -> bool {
    history_items.iter().any(|item| {
        let ResponseItem::Message { role, content, .. } = item else {
            return false;
        };
        if role != "user" {
            return false;
        }
        content.iter().any(|content_item| match content_item {
            ContentItem::InputText { text } | ContentItem::OutputText { text } => {
                text.contains(SUBAGENT_NOTIFICATION_OPEN_TAG)
            }
            ContentItem::InputImage { .. } => false,
        })
    })
}

/// Returns true when any message item contains `needle` in a text span.
fn history_contains_text(history_items: &[ResponseItem], needle: &str) -> bool {
    history_items.iter().any(|item| {
        let ResponseItem::Message { content, .. } = item else {
            return false;
        };
        content.iter().any(|content_item| match content_item {
            ContentItem::InputText { text } | ContentItem::OutputText { text } => {
                text.contains(needle)
            }
            ContentItem::InputImage { .. } => false,
        })
    })
}

async fn wait_for_subagent_notification(parent_thread: &Arc<CodexThread>) -> bool {
    let wait = async {
        loop {
            let history_items = parent_thread
                .codex
                .session
                .clone_history()
                .await
                .raw_items()
                .to_vec();
            if has_subagent_notification(&history_items) {
                return true;
            }
            sleep(Duration::from_millis(25)).await;
        }
    };
    timeout(Duration::from_secs(2), wait).await.is_ok()
}

fn captured_user_input_contains(
    manager: &ThreadManager,
    thread_id: ThreadId,
    needle: &str,
) -> bool {
    manager.captured_ops().into_iter().any(|(id, op)| {
        if id != thread_id {
            return false;
        }
        let Op::UserInput { items, .. } = op else {
            return false;
        };
        items.into_iter().any(|item| match item {
            UserInput::Text { text, .. } => text.contains(needle),
            _ => false,
        })
    })
}

async fn wait_for_captured_user_input(
    manager: &ThreadManager,
    thread_id: ThreadId,
    needle: &str,
) -> bool {
    let wait = async {
        loop {
            if captured_user_input_contains(manager, thread_id, needle) {
                return true;
            }
            sleep(Duration::from_millis(25)).await;
        }
    };
    timeout(Duration::from_secs(2), wait).await.is_ok()
}



#[tokio::test]
async fn on_event_updates_status_from_task_started() {
    let status = agent_status_from_event(&EventMsg::TurnStarted(TurnStartedEvent {
        turn_id: "turn-1".to_string(),
        model_context_window: None,
        collaboration_mode_kind: ModeKind::Default,
    }));
    assert_eq!(status, Some(AgentStatus::Running));
}

#[tokio::test]
async fn on_event_updates_status_from_task_complete() {
    let status = agent_status_from_event(&EventMsg::TurnComplete(TurnCompleteEvent {
        turn_id: "turn-1".to_string(),
        last_agent_message: Some("done".to_string()),
    }));
    let expected = AgentStatus::Completed(Some("done".to_string()));
    assert_eq!(status, Some(expected));
}

#[tokio::test]
async fn on_event_updates_status_from_error() {
    let status = agent_status_from_event(&EventMsg::Error(ErrorEvent {
        message: "boom".to_string(),
        codex_error_info: None,
    }));

    let expected = AgentStatus::Errored("boom".to_string());
    assert_eq!(status, Some(expected));
}

#[tokio::test]
async fn on_event_updates_status_from_turn_aborted() {
    let status = agent_status_from_event(&EventMsg::TurnAborted(TurnAbortedEvent {
        turn_id: Some("turn-1".to_string()),
        reason: TurnAbortReason::Interrupted,
    }));

    let expected = AgentStatus::Interrupted;
    assert_eq!(status, Some(expected));
}

#[tokio::test]
async fn on_event_updates_status_from_shutdown_complete() {
    let status = agent_status_from_event(&EventMsg::ShutdownComplete);
    assert_eq!(status, Some(AgentStatus::Shutdown));
}
