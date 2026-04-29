pub mod conversations;
pub mod harness_flow;
pub mod kanban;
pub mod queue;
pub mod threads;
pub mod logs;

// Unused old modules
mod lcm_tree;
mod memories;
mod mission;
mod tools;
mod continuity;

use std::path::PathBuf;
use std::time::Instant;

use crate::db_reader;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DataView {
    #[default]
    Terminal,
    Tickets,
    Queue,
    HarnessFlow,
    Conversations,
    Threads,
    Logs,
}

impl DataView {
    pub const DATA_VIEWS: [Self; 6] = [
        Self::Tickets,
        Self::Queue,
        Self::HarnessFlow,
        Self::Conversations,
        Self::Threads,
        Self::Logs,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Terminal => "Terminal",
            Self::Tickets => "Tickets",
            Self::Queue => "Queue",
            Self::HarnessFlow => "Flow",
            Self::Conversations => "Conversations",
            Self::Threads => "Threads",
            Self::Logs => "Logs",
        }
    }
}

pub struct DataViewState {
    pub active_view: DataView,
    pub root: Option<PathBuf>,
    pub last_refresh: Option<Instant>,

    // State DB
    pub threads: Vec<db_reader::ThreadRow>,

    // Logs DB
    pub logs: Vec<db_reader::LogRow>,

    // LCM DB
    pub lcm_messages: Vec<db_reader::LcmMessageRow>,
    pub mission_states: Vec<db_reader::MissionStateRow>,
    pub continuity_docs: Vec<db_reader::ContinuityDocRow>,

    // Runtime DB (ctox.sqlite3)
    pub ticket_items: Vec<db_reader::TicketItemRow>,
    pub ticket_cases: Vec<db_reader::TicketCaseRow>,
    pub comm_messages: Vec<db_reader::CommMessageRow>,
    pub execution_actions: Vec<db_reader::ExecutionActionRow>,
    pub harness_flow_text: String,

    // Per-view state
    pub kanban_state: kanban::KanbanState,
    pub queue_state: queue::QueueState,
    pub conversations_state: conversations::ConversationsState,
    pub threads_state: threads::ThreadsState,
    pub logs_state: logs::LogsState,
}

impl Default for DataViewState {
    fn default() -> Self {
        Self {
            active_view: DataView::Terminal,
            root: None,
            last_refresh: None,
            threads: Vec::new(),
            logs: Vec::new(),
            lcm_messages: Vec::new(),
            mission_states: Vec::new(),
            continuity_docs: Vec::new(),
            ticket_items: Vec::new(),
            ticket_cases: Vec::new(),
            comm_messages: Vec::new(),
            execution_actions: Vec::new(),
            harness_flow_text: String::new(),
            kanban_state: kanban::KanbanState::default(),
            queue_state: queue::QueueState::default(),
            conversations_state: conversations::ConversationsState::default(),
            threads_state: threads::ThreadsState::default(),
            logs_state: logs::LogsState::default(),
        }
    }
}

impl DataViewState {
    pub fn refresh(&mut self) {
        let Some(root) = self.root.as_ref() else { return };
        let root = root.clone();

        self.threads = db_reader::query_threads(&root);

        let level = if self.logs_state.level_filter.is_empty() { None } else { Some(self.logs_state.level_filter.as_str()) };
        let thread = if self.logs_state.thread_filter.is_empty() { None } else { Some(self.logs_state.thread_filter.as_str()) };
        self.logs = db_reader::query_logs(&root, level, thread, 500);

        self.lcm_messages = db_reader::query_lcm_messages(&root, 0);
        self.mission_states = db_reader::query_mission_state(&root);
        self.continuity_docs = db_reader::query_continuity_documents(&root);

        // Agent DB
        self.ticket_items = db_reader::query_ticket_items(&root);
        self.ticket_cases = db_reader::query_ticket_cases(&root);
        self.comm_messages = db_reader::query_comm_messages(&root);
        self.execution_actions = db_reader::query_execution_actions(&root);
        self.harness_flow_text = db_reader::query_harness_flow_text(&root, 132);

        self.last_refresh = Some(Instant::now());
    }

    pub fn needs_refresh(&self) -> bool {
        self.last_refresh
            .map(|t| t.elapsed().as_secs() >= 3)
            .unwrap_or(true)
    }

    pub fn set_root(&mut self, root: Option<PathBuf>) {
        if self.root != root {
            self.root = root;
            self.last_refresh = None;
        }
    }
}
