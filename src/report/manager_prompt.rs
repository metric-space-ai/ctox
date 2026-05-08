//! Manager system prompt + run input builders.
//!
//! Mirrors `buildSdkSystemPrompt` and `buildManagerInput` in
//! `Foerdervorhaben-Agent.html` lines 5177–5232, adapted to the CTOX
//! deep-research skill (seven report types, eleven tools, four
//! deterministic checks). The produced strings are the verbatim
//! manager system prompt and the first-turn run-input bundle.

use anyhow::Result;
use serde_json::{json, Value};

use crate::report::tools::TOOL_NAMES;
use crate::report::workspace::Workspace;

/// Build the manager's system prompt (German + English bilingual).
///
/// The text is hardcoded here — no `include_str!` — so the exact
/// wording is reviewable inside this Rust module. Substantive content
/// is sourced from `skills/system/research/deep-research/SKILL.md` and
/// `references/manager_path.md`.
pub fn build_manager_system_prompt() -> String {
    let mut tool_inventory = String::new();
    for (idx, name) in TOOL_NAMES.iter().enumerate() {
        if idx > 0 {
            tool_inventory.push_str(", ");
        }
        tool_inventory.push_str(name);
    }

    let mut out = String::new();
    out.push_str(
        "# CTOX Deep Research Manager

Du bist der Manager-Loop des CTOX deep-research Backends. / You are the
manager loop of the CTOX deep-research backend.

## Rolle / Role

Du orchestrierst einen Forschungs-Run über den block_library-, document-
und checks-Stack hinweg. Du schreibst keine Prosa selbst, du fügst keinen
Markdown in Tool-Argumente ein, und du kondensierst keine fehlenden Fakten:
unklare oder fehlende Eingaben werden über `ask_user` oder
`blocking_questions` an den Operator zurückgegeben.

You orchestrate a deep-research run across the block_library, document,
and checks stack. You never author prose yourself, never pass markdown
into a tool argument, and never paraphrase missing facts: unclear or
missing inputs are surfaced to the operator via `ask_user` or via
`blocking_questions` raised by a sub-skill.

## Tools

Du darfst ausschliesslich diese elf Tools aufrufen / You may only invoke
these eleven tools:

",
    );
    out.push_str(&tool_inventory);
    out.push_str(
        ".

Tool-Argumente sind immer JSON-Objekte. Tool-Argumente enthalten niemals
Markdown — nur Skalare und Identifier (`instance_ids[]`, `skill_run_id`,
`goals[]`, `question`, `focus`, `seed_dois[]`, `seed_arxiv_ids[]`).

Tool arguments are always JSON objects. Tool arguments never contain
markdown — only scalars and identifiers (`instance_ids[]`,
`skill_run_id`, `goals[]`, `question`, `focus`, `seed_dois[]`,
`seed_arxiv_ids[]`).

## Tool-Reihenfolge / Tool order

Typischer Ablauf / Typical order:

1. `workspace_snapshot`  — Run-Status lesen (immer zuerst).
2. `asset_lookup`        — beim Bootstrap mit `include_report_type=true`.
3. `public_research`     — Mindest-Evidenz aufbauen (Floor:
   `depth_profile.min_evidence_count`).
4. `write_with_skill`    — Block-Pakete schreiben (max 6 instance_ids).
5. `apply_block_patch`   — gestagete Blöcke committen.
6. `completeness_check`  — fehlende Pflichtblöcke prüfen.
7. `character_budget_check` — Budget-Status prüfen.
8. `release_guard_check` — Lints (matrix, register, evidence).
9. `narrative_flow_check` — Bogen-Konsistenz prüfen.
10. `revise_with_skill`  — bei gate-Fail: Goals weiterreichen.
11. `apply_block_patch`  — revidierte Blöcke committen.

`ask_user` ist kein Phasen-Tool und kann jederzeit aufgerufen werden,
wenn ein Pflichtfaktum nicht autonom beschaffbar ist.

`ask_user` is not a phase tool — invoke it any time a required fact
cannot be obtained autonomously. The run then ends with
`decision: \"needs_user_input\"`.

## Loop-End-Gate (host-enforced)

Bevor du `decision: \"finished\"` ausgibst, MÜSSEN alle vier Checks für
diesen Run mindestens einmal aufgerufen worden sein und `ready_to_finish`
zurückgemeldet haben (oder `check_applicable=false`):

- `completeness_check`
- `character_budget_check`
- `release_guard_check`
- `narrative_flow_check`

Der Host setzt diesen Gate-Override durch und überschreibt ein
LLM-`finished` zu `blocked`, wenn auch nur ein Check fehlt oder nicht
ready ist. Bewahre die Reihenfolge: gate-Aufrufe nach jedem
`apply_block_patch`, bis alle vier ready sind.

Before you emit `decision: \"finished\"`, ALL four checks for this run
MUST have been invoked at least once and reported `ready_to_finish=true`
(or `check_applicable=false`). The host enforces this override and
downgrades any LLM `finished` to `blocked` when even one check is
missing or not ready.

## Hard rules

- max 6 instance_ids per `write_with_skill` / `revise_with_skill` call.
- max 8 `goals[]` per `revise_with_skill`.
- max 5 questions per `ask_user`, max 3 `blocking_questions` per
  sub-skill output.
- never invent or paraphrase facts; raise `blocking_questions` instead.
- every `instance_id` must resolve to a `block_id` in the run's
  `report_type.block_library_keys[]` — cross-type usage is forbidden
  and rejected by the tool layer.
- never pass markdown into a tool argument; markdown only enters via
  validated sub-skill output and `apply_block_patch`.

## Forward-momentum rule (host-enforced)

`workspace_snapshot` and `asset_lookup` are READ-ONLY. Call each AT MOST
ONCE per run during bootstrap, then move on. Repeating them does not
advance the run and the host detects the loop. Each turn after the first
two carries a `tool_call_counts` map and (when stalled) a
`required_next_action` directive in the user message — when present, that
directive is the next call you MUST make. Do not re-inspect state to
re-verify unless the host has just committed a block via
`apply_block_patch`.

Productive path (mandatory after bootstrap): `public_research` →
`write_with_skill` → `apply_block_patch` → checks (completeness,
character_budget, release_guard, narrative_flow). Repeat write/apply per
block packet until completeness reports `ready_to_finish=true`.

## Antwort-Format / Response shape

Jede Antwort ist GENAU ein JSON-Objekt — kein Markdown, kein Code-Fence
um die Antwort herum. Two acceptable shapes:

- Tool call:
  `{ \"tool\": \"<name>\", \"args\": { ... } }`
  oder / or
  `{ \"tool_calls\": [ { \"name\": \"<name>\", \"args\": { ... } } ] }`

- End decision:
  `{ \"decision\": \"finished\" | \"needs_user_input\" | \"blocked\",
     \"summary\": \"<= 240 chars\",
     \"changed_blocks\": [\"<instance_id>\", ...],
     \"open_questions\": [\"<text>\", ...],
     \"reason\": \"<= 240 chars\" }`

Free-form prose outside the JSON object is rejected. Bleibe im
JSON-Modus, bis der Run beendet ist.
",
    );
    out
}

/// Build the user-message body the manager sees on its first turn.
///
/// The body is the manager's view of the run: a 4-line bilingual
/// directive followed by the JSON-pretty-printed `manager_input`
/// snapshot from [`Workspace::manager_input`].
pub fn build_manager_run_input(workspace: &Workspace) -> Result<String> {
    let metadata = workspace.run_metadata()?;
    let snapshot = workspace.manager_input()?;
    let report_type = snapshot
        .pointer("/workspace_snapshot/report_type")
        .cloned()
        .unwrap_or(Value::Null);
    let depth_profile = snapshot
        .pointer("/workspace_snapshot/depth_profile")
        .cloned()
        .unwrap_or(Value::Null);
    let domain_profile = snapshot
        .pointer("/workspace_snapshot/domain_profile")
        .cloned()
        .unwrap_or(Value::Null);
    let character_budget = snapshot
        .pointer("/workspace_snapshot/character_budget")
        .cloned()
        .unwrap_or(Value::Null);
    let blocking_open_questions = snapshot
        .pointer("/workspace_snapshot/blocking_open_questions")
        .cloned()
        .unwrap_or(Value::Array(Vec::new()));
    let pending_blocks = snapshot
        .pointer("/workspace_snapshot/pending_blocks")
        .cloned()
        .unwrap_or(Value::Array(Vec::new()));
    let pending_count = pending_blocks.as_array().map(Vec::len).unwrap_or(0);
    let min_evidence_count = snapshot
        .pointer("/workspace_snapshot/min_evidence_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let evidence_register_size = snapshot
        .pointer("/workspace_snapshot/evidence_register_size")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let current_date = snapshot
        .pointer("/workspace_snapshot/current_date")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    let directive = report_type_directive(&metadata.report_type_id);

    let bundle = json!({
        "current_date_iso": current_date,
        "package_summary": {
            "report_type": report_type,
            "domain_profile": domain_profile,
            "depth_profile": depth_profile,
            "language": metadata.language,
        },
        "character_budget": character_budget,
        "raw_topic": metadata.raw_topic,
        "pending_pflicht_blocks_count": pending_count,
        "blocking_open_questions": blocking_open_questions,
        "evidence_floor": {
            "min_evidence_count": min_evidence_count,
            "evidence_register_size": evidence_register_size,
            "remaining": (min_evidence_count as i64)
                - (evidence_register_size as i64),
        },
        "manager_state": snapshot,
    });

    let pretty = serde_json::to_string_pretty(&bundle)?;
    let mut out = String::new();
    out.push_str("# CTOX Deep Research Run-Input\n\n");
    out.push_str(
        "Lies den Bundle. Wähle das nächste Tool. / Read the bundle. Pick the next tool.\n",
    );
    out.push_str("Bleibe im JSON-Modus. / Stay in JSON-only mode.\n");
    out.push_str(&format!(
        "Aktiver report_type: {}.\n",
        metadata.report_type_id
    ));
    out.push_str(&format!("Direktive / directive: {}\n\n", directive));
    out.push_str("```json\n");
    out.push_str(&pretty);
    out.push_str("\n```\n");
    Ok(out)
}

/// Schema descriptor for the manager's expected end-decision envelope.
/// Returned for documentation/validator wiring; the actual decoder lives
/// in `manager.rs`.
pub fn manager_output_schema() -> Value {
    json!({
        "type": "object",
        "oneOf": [
            {
                "title": "tool_call",
                "properties": {
                    "tool": { "type": "string", "enum": TOOL_NAMES },
                    "args": { "type": "object" },
                },
                "required": ["tool", "args"],
            },
            {
                "title": "tool_calls",
                "properties": {
                    "tool_calls": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "name": { "type": "string", "enum": TOOL_NAMES },
                                "args": { "type": "object" },
                            },
                            "required": ["name", "args"],
                        },
                    },
                },
                "required": ["tool_calls"],
            },
            {
                "title": "decision",
                "properties": {
                    "decision": {
                        "type": "string",
                        "enum": ["finished", "needs_user_input", "blocked"],
                    },
                    "summary": { "type": "string", "maxLength": 240 },
                    "changed_blocks": { "type": "array", "items": { "type": "string" } },
                    "open_questions": { "type": "array", "items": { "type": "string" } },
                    "reason": { "type": "string", "maxLength": 240 },
                },
                "required": ["decision", "summary"],
            },
        ],
    })
}

/// Per-report-type bootstrap directive. The text mirrors the wording
/// in `references/manager_path.md` so the manager opens with the same
/// phrase the runbook uses.
fn report_type_directive(report_type_id: &str) -> &'static str {
    match report_type_id {
        "feasibility_study" => {
            "Beginne mit workspace_snapshot, dann asset_lookup, dann public_research bis evidence_floor erreicht; danach write_with_skill packetweise."
        }
        "market_research" => {
            "Beginne mit workspace_snapshot, dann asset_lookup, dann public_research mit Fokus Marktdaten/Segmente, bis evidence_floor erreicht."
        }
        "competitive_analysis" => {
            "Beginne mit workspace_snapshot, dann asset_lookup, dann public_research zu Wettbewerber-Capabilities und Positionierung."
        }
        "technology_screening" => {
            "Beginne mit workspace_snapshot, dann asset_lookup, dann public_research zur Longlist-Beschaffung; Kriterien stehen vor Matrix."
        }
        "whitepaper" => {
            "Beginne mit workspace_snapshot, dann asset_lookup, dann public_research zur Untermauerung der These; weniger, dafür präzise Quellen."
        }
        "literature_review" => {
            "Beginne mit workspace_snapshot, dann asset_lookup, dann public_research bis evidence_floor; thematische Synthese vor Integration."
        }
        "decision_brief" => {
            "Beginne mit workspace_snapshot, dann asset_lookup, dann public_research auf das Nötigste; Recommendation steht im Brief vorn."
        }
        _ => "Beginne mit workspace_snapshot, dann asset_lookup, dann public_research.",
    }
}
