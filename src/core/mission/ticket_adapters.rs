use anyhow::Result;
use serde_json::Value;
use std::path::Path;

use crate::mission::ticket_gateway;
use crate::mission::ticket_gateway::TicketAdapterCapabilities;
use crate::mission::ticket_gateway::TicketAdapterKind;
use crate::mission::ticket_local_native;
use crate::mission::ticket_protocol::TicketCommentWritebackRequest;
use crate::mission::ticket_protocol::TicketSelfWorkAssignRequest;
use crate::mission::ticket_protocol::TicketSelfWorkAssignResult;
use crate::mission::ticket_protocol::TicketSelfWorkNoteRequest;
use crate::mission::ticket_protocol::TicketSelfWorkPublishRequest;
use crate::mission::ticket_protocol::TicketSelfWorkPublishResult;
use crate::mission::ticket_protocol::TicketSelfWorkTransitionRequest;
use crate::mission::ticket_protocol::TicketSyncBatch;
use crate::mission::ticket_protocol::TicketTransitionWritebackRequest;
use crate::mission::ticket_protocol::TicketWritebackResult;
use crate::mission::ticket_zammad_native;

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) trait TicketSystemAdapter {
    fn kind(&self) -> TicketAdapterKind;

    fn system_name(&self) -> &'static str {
        match self.kind() {
            TicketAdapterKind::Local => "local",
            TicketAdapterKind::Zammad => "zammad",
        }
    }

    fn capabilities(&self) -> TicketAdapterCapabilities {
        self.kind().spec().capabilities
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LocalTicketAdapter;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ZammadTicketAdapter;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExternalTicketAdapter {
    Local(LocalTicketAdapter),
    Zammad(ZammadTicketAdapter),
}

pub(crate) fn local() -> LocalTicketAdapter {
    LocalTicketAdapter
}

pub(crate) fn zammad() -> ZammadTicketAdapter {
    ZammadTicketAdapter
}

pub(crate) fn adapter_for_system(system: &str) -> Option<ExternalTicketAdapter> {
    match system {
        "local" => Some(ExternalTicketAdapter::Local(local())),
        "zammad" => Some(ExternalTicketAdapter::Zammad(zammad())),
        _ => None,
    }
}

impl TicketSystemAdapter for LocalTicketAdapter {
    fn kind(&self) -> TicketAdapterKind {
        TicketAdapterKind::Local
    }
}

impl TicketSystemAdapter for ZammadTicketAdapter {
    fn kind(&self) -> TicketAdapterKind {
        TicketAdapterKind::Zammad
    }
}

impl ExternalTicketAdapter {
    pub(crate) fn capabilities(self) -> TicketAdapterCapabilities {
        match self {
            ExternalTicketAdapter::Local(adapter) => adapter.capabilities(),
            ExternalTicketAdapter::Zammad(adapter) => adapter.capabilities(),
        }
    }

    pub(crate) fn sync_batch(self, root: &Path) -> Result<TicketSyncBatch> {
        match self {
            ExternalTicketAdapter::Local(adapter) => adapter.sync_batch(root),
            ExternalTicketAdapter::Zammad(adapter) => adapter.sync_batch(root),
        }
    }

    pub(crate) fn test(self, root: &Path) -> Result<Value> {
        match self {
            ExternalTicketAdapter::Local(adapter) => adapter.test(root),
            ExternalTicketAdapter::Zammad(adapter) => adapter.test(root),
        }
    }

    pub(crate) fn writeback_comment(
        self,
        root: &Path,
        request: TicketCommentWritebackRequest<'_>,
    ) -> Result<TicketWritebackResult> {
        match self {
            ExternalTicketAdapter::Local(adapter) => adapter.writeback_comment(root, &request),
            ExternalTicketAdapter::Zammad(adapter) => adapter.writeback_comment(root, &request),
        }
    }

    pub(crate) fn writeback_transition(
        self,
        root: &Path,
        request: TicketTransitionWritebackRequest<'_>,
    ) -> Result<TicketWritebackResult> {
        match self {
            ExternalTicketAdapter::Local(adapter) => adapter.writeback_transition(root, &request),
            ExternalTicketAdapter::Zammad(adapter) => adapter.writeback_transition(root, &request),
        }
    }

    pub(crate) fn publish_self_work_item(
        self,
        root: &Path,
        request: TicketSelfWorkPublishRequest<'_>,
    ) -> Result<TicketSelfWorkPublishResult> {
        match self {
            ExternalTicketAdapter::Local(adapter) => adapter.publish_self_work_item(root, &request),
            ExternalTicketAdapter::Zammad(adapter) => {
                adapter.publish_self_work_item(root, &request)
            }
        }
    }

    pub(crate) fn assign_self_work_item(
        self,
        root: &Path,
        request: TicketSelfWorkAssignRequest<'_>,
    ) -> Result<TicketSelfWorkAssignResult> {
        match self {
            ExternalTicketAdapter::Local(adapter) => adapter.assign_self_work_item(root, &request),
            ExternalTicketAdapter::Zammad(adapter) => adapter.assign_self_work_item(root, &request),
        }
    }

    pub(crate) fn append_self_work_note(
        self,
        root: &Path,
        request: TicketSelfWorkNoteRequest<'_>,
    ) -> Result<TicketWritebackResult> {
        match self {
            ExternalTicketAdapter::Local(adapter) => adapter.append_self_work_note(root, &request),
            ExternalTicketAdapter::Zammad(adapter) => adapter.append_self_work_note(root, &request),
        }
    }

    pub(crate) fn transition_self_work_item(
        self,
        root: &Path,
        request: TicketSelfWorkTransitionRequest<'_>,
    ) -> Result<TicketWritebackResult> {
        match self {
            ExternalTicketAdapter::Local(adapter) => {
                adapter.transition_self_work_item(root, &request)
            }
            ExternalTicketAdapter::Zammad(adapter) => {
                adapter.transition_self_work_item(root, &request)
            }
        }
    }
}

impl LocalTicketAdapter {
    pub(crate) fn sync_batch(self, root: &Path) -> Result<TicketSyncBatch> {
        ticket_local_native::fetch_sync_batch(root)
    }

    pub(crate) fn test(self, root: &Path) -> Result<Value> {
        ticket_local_native::test(root)
    }

    pub(crate) fn writeback_comment(
        self,
        root: &Path,
        request: &TicketCommentWritebackRequest<'_>,
    ) -> Result<TicketWritebackResult> {
        ticket_local_native::writeback_comment(
            root,
            request.remote_ticket_id,
            request.body,
            request.internal,
        )
    }

    pub(crate) fn writeback_transition(
        self,
        root: &Path,
        request: &TicketTransitionWritebackRequest<'_>,
    ) -> Result<TicketWritebackResult> {
        ticket_local_native::writeback_transition(
            root,
            request.remote_ticket_id,
            request.state,
            request.note_body,
            request.internal_note,
        )
    }

    pub(crate) fn publish_self_work_item(
        self,
        root: &Path,
        request: &TicketSelfWorkPublishRequest<'_>,
    ) -> Result<TicketSelfWorkPublishResult> {
        let record = ticket_local_native::create_local_ticket(
            root,
            request.title,
            request.body,
            Some("open"),
            Some("low"),
        )?;
        Ok(TicketSelfWorkPublishResult {
            remote_ticket_id: Some(record.ticket_id),
            remote_locator: None,
        })
    }

    pub(crate) fn assign_self_work_item(
        self,
        root: &Path,
        request: &TicketSelfWorkAssignRequest<'_>,
    ) -> Result<TicketSelfWorkAssignResult> {
        ticket_local_native::assign_local_ticket(root, request.remote_ticket_id, request.assignee)
    }

    pub(crate) fn append_self_work_note(
        self,
        root: &Path,
        request: &TicketSelfWorkNoteRequest<'_>,
    ) -> Result<TicketWritebackResult> {
        ticket_local_native::writeback_comment(
            root,
            request.remote_ticket_id,
            request.body,
            request.internal,
        )
    }

    pub(crate) fn transition_self_work_item(
        self,
        root: &Path,
        request: &TicketSelfWorkTransitionRequest<'_>,
    ) -> Result<TicketWritebackResult> {
        ticket_local_native::writeback_transition(
            root,
            request.remote_ticket_id,
            request.state,
            request.note_body,
            request.internal_note,
        )
    }
}

impl ZammadTicketAdapter {
    pub(crate) fn sync_batch(self, root: &Path) -> Result<TicketSyncBatch> {
        let runtime = ticket_gateway::runtime_settings_from_root(root, self.kind());
        ticket_zammad_native::fetch_sync_batch(root, &runtime)
    }

    pub(crate) fn test(self, root: &Path) -> Result<Value> {
        let runtime = ticket_gateway::runtime_settings_from_root(root, self.kind());
        ticket_zammad_native::test(root, &runtime)
    }

    pub(crate) fn writeback_comment(
        self,
        root: &Path,
        request: &TicketCommentWritebackRequest<'_>,
    ) -> Result<TicketWritebackResult> {
        let runtime = ticket_gateway::runtime_settings_from_root(root, self.kind());
        ticket_zammad_native::writeback_comment(root, &runtime, request)
    }

    pub(crate) fn writeback_transition(
        self,
        root: &Path,
        request: &TicketTransitionWritebackRequest<'_>,
    ) -> Result<TicketWritebackResult> {
        let runtime = ticket_gateway::runtime_settings_from_root(root, self.kind());
        ticket_zammad_native::writeback_transition(root, &runtime, request)
    }

    pub(crate) fn publish_self_work_item(
        self,
        root: &Path,
        request: &TicketSelfWorkPublishRequest<'_>,
    ) -> Result<TicketSelfWorkPublishResult> {
        let runtime = ticket_gateway::runtime_settings_from_root(root, self.kind());
        ticket_zammad_native::publish_self_work_item(root, &runtime, request)
    }

    pub(crate) fn assign_self_work_item(
        self,
        root: &Path,
        request: &TicketSelfWorkAssignRequest<'_>,
    ) -> Result<TicketSelfWorkAssignResult> {
        let runtime = ticket_gateway::runtime_settings_from_root(root, self.kind());
        ticket_zammad_native::assign_self_work_item(root, &runtime, request)
    }

    pub(crate) fn append_self_work_note(
        self,
        root: &Path,
        request: &TicketSelfWorkNoteRequest<'_>,
    ) -> Result<TicketWritebackResult> {
        let runtime = ticket_gateway::runtime_settings_from_root(root, self.kind());
        ticket_zammad_native::append_self_work_note(root, &runtime, request)
    }

    pub(crate) fn transition_self_work_item(
        self,
        root: &Path,
        request: &TicketSelfWorkTransitionRequest<'_>,
    ) -> Result<TicketWritebackResult> {
        let runtime = ticket_gateway::runtime_settings_from_root(root, self.kind());
        ticket_zammad_native::transition_self_work_item(root, &runtime, request)
    }
}

#[cfg(test)]
mod tests {
    use super::adapter_for_system;
    use super::local;
    use super::zammad;
    use super::ExternalTicketAdapter;
    use super::TicketSystemAdapter;

    #[test]
    fn registry_resolves_supported_ticket_systems() {
        assert_eq!(
            adapter_for_system("local"),
            Some(ExternalTicketAdapter::Local(local()))
        );
        assert_eq!(
            adapter_for_system("zammad"),
            Some(ExternalTicketAdapter::Zammad(zammad()))
        );
        assert_eq!(adapter_for_system("jira"), None);
    }

    #[test]
    fn local_adapter_reports_local_system_name() {
        assert_eq!(local().system_name(), "local");
    }

    #[test]
    fn zammad_adapter_reports_zammad_system_name() {
        assert_eq!(zammad().system_name(), "zammad");
    }
}
