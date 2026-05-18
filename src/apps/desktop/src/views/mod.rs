pub mod conversations;
pub mod forensics;
pub mod harness_flow;
pub mod kanban;
pub mod logs;
pub mod overview;
pub mod queue;
pub mod threads;

// Unused old modules
mod continuity;
mod lcm_tree;
mod memories;
mod mission;
mod tools;

use std::path::PathBuf;
use std::time::Instant;

use crate::db_reader;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DataView {
    #[default]
    Terminal,
    Overview,
    Tickets,
    Queue,
    HarnessFlow,
    Forensics,
    Conversations,
    Threads,
    Logs,
}

impl DataView {
    pub const DATA_VIEWS: [Self; 8] = [
        Self::Overview,
        Self::Tickets,
        Self::Queue,
        Self::HarnessFlow,
        Self::Forensics,
        Self::Conversations,
        Self::Threads,
        Self::Logs,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Terminal => "Terminal",
            Self::Overview => "Overview",
            Self::Tickets => "Tickets",
            Self::Queue => "Queue",
            Self::HarnessFlow => "Flow",
            Self::Forensics => "Forensics",
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

    // Overview aggregates
    pub token_summary: db_reader::ThreadTokenSummary,
    pub ticket_counts: db_reader::TicketCounts,
    pub comm_counts: db_reader::CommCounts,
    pub review_counts: db_reader::ReviewCounts,
    pub pm_counts: db_reader::PmCounts,

    // Forensics
    pub pm_activities: Vec<db_reader::PmActivityRow>,
    pub pm_dfg: Vec<db_reader::PmDfgEdge>,
    pub pm_cases: Vec<db_reader::PmCaseRow>,
    pub pm_case_events: Vec<db_reader::PmCaseEventRow>,
    pub pm_violations: Vec<db_reader::PmViolationRow>,
    pub spawn_edges: Vec<db_reader::SpawnEdgeRow>,

    // Per-view state
    pub kanban_state: kanban::KanbanState,
    pub queue_state: queue::QueueState,
    pub conversations_state: conversations::ConversationsState,
    pub threads_state: threads::ThreadsState,
    pub logs_state: logs::LogsState,
    pub forensics_state: forensics::ForensicsState,
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
            token_summary: db_reader::ThreadTokenSummary::default(),
            ticket_counts: db_reader::TicketCounts::default(),
            comm_counts: db_reader::CommCounts::default(),
            review_counts: db_reader::ReviewCounts::default(),
            pm_counts: db_reader::PmCounts::default(),
            pm_activities: Vec::new(),
            pm_dfg: Vec::new(),
            pm_cases: Vec::new(),
            pm_case_events: Vec::new(),
            pm_violations: Vec::new(),
            spawn_edges: Vec::new(),
            kanban_state: kanban::KanbanState::default(),
            queue_state: queue::QueueState::default(),
            conversations_state: conversations::ConversationsState::default(),
            threads_state: threads::ThreadsState::default(),
            logs_state: logs::LogsState::default(),
            forensics_state: forensics::ForensicsState::default(),
        }
    }
}

impl DataViewState {
    pub fn refresh(&mut self) {
        let Some(root) = self.root.as_ref() else {
            return;
        };
        let root = root.clone();

        self.threads = db_reader::query_threads(&root);

        let level = if self.logs_state.level_filter.is_empty() {
            None
        } else {
            Some(self.logs_state.level_filter.as_str())
        };
        let thread = if self.logs_state.thread_filter.is_empty() {
            None
        } else {
            Some(self.logs_state.thread_filter.as_str())
        };
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

        // Overview / forensics aggregates. These are cheap COUNT queries plus
        // a handful of bounded ORDER BY ... LIMIT reads, so the same 3s
        // refresh cadence is fine.
        match self.active_view {
            DataView::Overview => {
                self.token_summary = db_reader::query_thread_token_summary(&root);
                self.ticket_counts = db_reader::query_ticket_counts(&root);
                self.comm_counts = db_reader::query_comm_counts(&root);
                self.review_counts = db_reader::query_review_counts(&root);
                self.pm_counts = db_reader::query_pm_counts(&root);
            }
            DataView::Forensics => {
                self.pm_counts = db_reader::query_pm_counts(&root);
                self.pm_activities = db_reader::query_pm_activities(&root, 30);
                self.pm_dfg = db_reader::query_pm_dfg(&root, 80);
                self.pm_cases = db_reader::query_pm_cases(&root, 200);
                self.pm_violations = db_reader::query_pm_violations(&root, 100);
                self.spawn_edges = db_reader::query_spawn_edges(&root, 200);
                self.pm_case_events = match self.forensics_state.selected_case.as_deref() {
                    Some(case_id) => db_reader::query_pm_case_events(&root, case_id, 300),
                    None => Vec::new(),
                };
            }
            _ => {}
        }

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
