// Origin: CTOX
// License: Apache-2.0
//
// Autonomy level — a single persistent operator preference that
// controls how eagerly CTOX asks the owner for approval before acting.
// Three levels:
//
//  - `progressive` — execute aggressively; only stop for actions that
//    would be genuinely irreversible outside the current run. Any
//    approval-gate self-work item that does get created is auto-closed
//    so plans keep moving. Benchmark harnesses (Terminal-Bench) and
//    on-the-fly smoke tests use this level.
//
//  - `balanced` — the default. Everyday professional behaviour: do
//    routine work directly, create approval-gate items for real
//    high-impact moves (production cutovers, destructive migrations,
//    public communication).
//
//  - `defensive` — conservative mode. Prefer to surface risk via
//    approval-gate whenever a move touches infrastructure, external
//    services, or irreversible state. Reminders nag faster so the
//    owner is pulled in sooner.
//
// The level is read from `CTOX_AUTONOMY_LEVEL` in the process
// environment. The service propagates it from `engine.env` at boot
// (see `service::run_foreground`). The legacy benchmark flag
// `CTOX_AUTO_APPROVE_GATES=1` is honoured as a deprecated alias for
// `progressive`.

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutonomyLevel {
    Progressive,
    Balanced,
    Defensive,
}

impl AutonomyLevel {
    /// Resolve from the process environment. Falls back to `balanced`.
    pub fn from_env() -> Self {
        if let Ok(raw) = std::env::var("CTOX_AUTONOMY_LEVEL") {
            return Self::from_str_lossy(&raw);
        }
        // Deprecated alias: benchmark harnesses that still set the old
        // boolean flag get the progressive level.
        if std::env::var("CTOX_AUTO_APPROVE_GATES")
            .map(|value| {
                matches!(value.trim(), "1" | "true" | "yes" | "on")
            })
            .unwrap_or(false)
        {
            return Self::Progressive;
        }
        Self::Balanced
    }

    pub fn from_str_lossy(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "progressive" | "aggressive" | "fast" => Self::Progressive,
            "defensive" | "cautious" | "conservative" => Self::Defensive,
            _ => Self::Balanced,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Progressive => "progressive",
            Self::Balanced => "balanced",
            Self::Defensive => "defensive",
        }
    }

    /// Should any `approval-gate` self-work item that reaches `open`
    /// be auto-closed by the mission watcher? True only for the
    /// progressive level — balanced and defensive let the gate stand
    /// and rely on the reminder/approval handshake.
    pub fn auto_closes_gates(&self) -> bool {
        matches!(self, Self::Progressive)
    }

    /// Escalation cadence for the approval-nag reminder, in seconds
    /// from the gate's first-seen timestamp. Empty for progressive
    /// (gates are auto-closed, no reminders are ever sent).
    pub fn nag_cadence_seconds(&self) -> &'static [(i64, &'static str)] {
        match self {
            Self::Progressive => &[],
            Self::Balanced => &[
                (2 * 60 * 60, "email"),        // T+2h
                (24 * 60 * 60, "email"),       // T+1 day
                (2 * 24 * 60 * 60, "jami"),    // T+2 days (channel switch)
                (4 * 24 * 60 * 60, "email"),   // T+4 days (final nudge)
            ],
            Self::Defensive => &[
                (60 * 60, "email"),            // T+1h
                (4 * 60 * 60, "email"),        // T+4h
                (24 * 60 * 60, "jami"),        // T+1 day (channel switch)
                (2 * 24 * 60 * 60, "email"),   // T+2 days
                (4 * 24 * 60 * 60, "jami"),    // T+4 days (final nudge)
            ],
        }
    }

    /// Short paragraph injected into the per-turn runtime context so
    /// the model knows how aggressively to escalate. Keep it compact —
    /// this lands in every single turn.
    pub fn runtime_policy_block(&self) -> &'static str {
        match self {
            Self::Progressive =>
                "Autonomy policy: progressive. Execute directly. Do not create approval-gate self-work items as a way to defer a decision — the owner is not watching this run in real time and has accepted the risk profile. Escalate only for actions that would be genuinely irreversible outside this run (production data deletion, public broadcast, destructive external API calls).",
            Self::Balanced =>
                "Autonomy policy: balanced. Routine work (code changes in development branches, local runs, analysis, internal artefacts) executes directly. Use approval-gate only for genuinely high-impact moves: production cutovers, destructive migrations, public-facing communication, irreversible external changes. A gate that turns out not to be high-impact is waste; an uncreated gate on something truly destructive is harm.",
            Self::Defensive =>
                "Autonomy policy: defensive. Prefer to surface risk via approval-gate whenever a move touches infrastructure, external services, shared environments, or irreversible state. When uncertain, ask. The owner has opted into being pulled in sooner rather than later. Routine purely-local work (reading code, writing local notes, running tests against sandbox data) still proceeds directly.",
        }
    }

    /// Short clause inserted into `render_step_prompt` to tune how
    /// eagerly the agent creates approval-gate self-work items during
    /// the current plan step.
    pub fn step_prompt_clause(&self) -> &'static str {
        match self {
            Self::Progressive =>
                "Autonomy: progressive — execute the step directly; do not defer via approval-gate unless the very next action would be genuinely irreversible outside this run (production data deletion, public broadcast, destructive external API call). Any approval-gate you do create will be auto-closed, so creating one just to stop is wasted work.",
            Self::Balanced =>
                "Autonomy: balanced — most steps do not need an approval gate. Use approval-gate only when the next action would be genuinely high-impact (production cutover, destructive migration, public communication). A gate that turns out to be a routine change is waste.",
            Self::Defensive =>
                "Autonomy: defensive — when in doubt, ask. Use approval-gate for any move that touches infrastructure, external services, shared environments, or irreversible state. Routine purely-local work still executes directly.",
        }
    }
}

impl Default for AutonomyLevel {
    fn default() -> Self {
        Self::Balanced
    }
}

impl fmt::Display for AutonomyLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Convenience: true when the resolved level is progressive. Service
/// loops use this to decide whether to run the auto-close sweep.
pub fn is_progressive() -> bool {
    AutonomyLevel::from_env() == AutonomyLevel::Progressive
}
