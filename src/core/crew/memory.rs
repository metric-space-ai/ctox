//! Crew memory lives in the LCM. Each member owns one continuity document set
//! (narrative = experience, anchors = secured knowledge) under the member
//! conversation `crew:<member_id>`: same tables, same commits, same refresh
//! prompt and compaction as every other conversation. There is no parallel
//! store. The persona (character, voice, working style) is owner-managed data
//! rendered into the identity lane; the memory is rendered into the runtime
//! context lane; learning happens through the existing continuity refresh.
use super::*;
use crate::lcm::{ContinuityKind, LcmConfig, LcmEngine};
use rusqlite::OptionalExtension;
use std::collections::{BTreeMap, HashSet};
use std::path::Path;

/// Bytes, not tokens: conservative ceilings for the two crew lanes.
pub(crate) const PERSONA_MAX_BYTES: usize = 2_400;
pub(crate) const MEMORY_MAX_BYTES: usize = 6_000;
const RECENT_NARRATIVE_ENTRIES: usize = 8;
const RECENT_ATTEMPTS_IN_MEMORY: usize = 6;

pub(crate) fn member_conversation_id(member_id: &str) -> i64 {
    crate::execution::agent::turn_loop::conversation_id_for_thread_key(Some(&format!(
        "crew:{member_id}"
    )))
}

pub(crate) fn open_engine(root: &Path) -> Result<LcmEngine> {
    LcmEngine::open(&crate::paths::core_db(root), LcmConfig::default())
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct MemberMemory {
    pub anchors: String,
    pub narrative: String,
    pub updated_at: String,
}

/// Reads the stored documents without creating them: a member that never
/// learned anything has no memory, and the prompt omits the block.
pub(crate) fn load_member_memory(engine: &LcmEngine, member_id: &str) -> MemberMemory {
    let conversation = member_conversation_id(member_id);
    match engine.stored_continuity_show_all(conversation) {
        Ok(all) => MemberMemory {
            anchors: all.anchors.content,
            narrative: all.narrative.content,
            updated_at: std::cmp::max(all.anchors.updated_at, all.narrative.updated_at),
        },
        Err(_) => MemberMemory::default(),
    }
}

/// Read-only view of the same documents through an existing core connection:
/// the projection pump must not open the LCM engine (migrations take a write
/// lock and starve the chat projection on the same database).
pub(crate) fn load_member_memory_from_conn(conn: &Connection, member_id: &str) -> MemberMemory {
    let conversation = member_conversation_id(member_id);
    let read = |kind: &str| -> Option<(String, String)> {
        conn.query_row(
            "SELECT c.rendered_text, d.updated_at FROM continuity_documents d
             JOIN continuity_commits c ON c.commit_id = d.head_commit_id
             WHERE d.conversation_id = ?1 AND d.kind = ?2",
            params![conversation, kind],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
        )
        .ok()
    };
    let has_tables: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='continuity_documents')
                AND EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='continuity_commits')",
            [],
            |r| r.get(0),
        )
        .unwrap_or(false);
    if !has_tables {
        return MemberMemory::default();
    }
    let anchors = read("anchors");
    let narrative = read("narrative");
    MemberMemory {
        anchors: anchors
            .as_ref()
            .map(|(text, _)| text.clone())
            .unwrap_or_default(),
        narrative: narrative
            .as_ref()
            .map(|(text, _)| text.clone())
            .unwrap_or_default(),
        updated_at: std::cmp::max(
            anchors.map(|(_, at)| at).unwrap_or_default(),
            narrative.map(|(_, at)| at).unwrap_or_default(),
        ),
    }
}

/// Strict read for execution-context restoration: absent documents mean no
/// memory, but a broken store or dangling head must never become empty memory.
pub(crate) fn load_member_memory_checked_from_conn(
    conn: &Connection,
    member_id: &str,
) -> Result<MemberMemory> {
    let read = |kind: &str| -> Result<(String, String)> {
        let row: Option<(Option<String>, String)> = conn
            .query_row(
                "SELECT c.rendered_text,d.updated_at FROM continuity_documents d
             LEFT JOIN continuity_commits c ON c.commit_id=d.head_commit_id
             WHERE d.conversation_id=?1 AND d.kind=?2",
                params![member_conversation_id(member_id), kind],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        match row {
            None => Ok((String::new(), String::new())),
            Some((text, updated)) => {
                Ok((text.context("crew memory head is unavailable")?, updated))
            }
        }
    };
    let (anchors, anchors_updated) = read("anchors")?;
    let (narrative, narrative_updated) = read("narrative")?;
    Ok(MemberMemory {
        anchors,
        narrative,
        updated_at: anchors_updated.max(narrative_updated),
    })
}

/// Entries of a continuity document as one dense line each.

fn entry_lines(
    content: &str,
    start_key: &str,
    tag_key: &str,
    primary: &str,
    secondary: Option<&str>,
) -> Vec<String> {
    let mut entries: Vec<BTreeMap<String, String>> = Vec::new();
    let mut current: Option<BTreeMap<String, String>> = None;
    for raw in content.lines() {
        let trimmed = raw.trim().trim_start_matches("- ").trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some((key, value)) = trimmed.split_once(':') else {
            continue;
        };
        let key = key.trim().to_ascii_lowercase();
        let value = prose_line(value);
        if key == start_key {
            if let Some(entry) = current.take() {
                entries.push(entry);
            }
            current = Some(BTreeMap::new());
        }
        if let Some(entry) = current.as_mut() {
            if !value.is_empty() {
                entry.entry(key).or_insert(value);
            }
        }
    }
    if let Some(entry) = current.take() {
        entries.push(entry);
    }
    entries
        .iter()
        .filter_map(|entry| {
            let main = entry.get(primary)?;
            let mut line = match entry.get(tag_key) {
                Some(tag) => format!("[{tag}] {main}"),
                None => main.clone(),
            };
            if let Some(more) = secondary.and_then(|key| entry.get(key)) {
                line.push_str(" — ");
                line.push_str(more);
            }
            Some(line)
        })
        .collect()
}

pub(crate) fn anchor_lines(anchors: &str) -> Vec<String> {
    entry_lines(anchors, "anchor_id", "anchor_type", "statement", None)
}

pub(crate) fn narrative_lines(narrative: &str) -> Vec<String> {
    entry_lines(
        narrative,
        "entry_id",
        "event_type",
        "summary",
        Some("consequence"),
    )
}

/// One-time move of the legacy `crew_member_learnings` rows into the member's
/// anchors document. Idempotent through `migrated_to_lcm`.
pub(crate) fn migrate_learnings_into_memory(
    conn: &Connection,
    engine: &LcmEngine,
    member_id: &str,
) -> Result<usize> {
    let rows = conn
        .prepare(
            "SELECT id,text,kind,scope_json,evidence_run_id,confirmed_by_owner
             FROM crew_member_learnings
             WHERE member_id=?1 AND archived=0 AND COALESCE(migrated_to_lcm,0)=0
             ORDER BY created_at,id LIMIT 200",
        )?
        .query_map([member_id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, bool>(5)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if rows.is_empty() {
        return Ok(0);
    }
    let conversation = member_conversation_id(member_id);
    engine.continuity_init_documents(conversation)?;
    let mut moved = 0;
    for (id, text, kind, scope, evidence, confirmed) in rows {
        let scope: LearningScope = serde_json::from_str(&scope).unwrap_or_default();
        let diff = anchor_diff(
            &id.replace(':', "_"),
            if confirmed {
                "owner_confirmed"
            } else {
                "hypothesis"
            },
            &text,
            &kind,
            &scope,
            &evidence,
        );
        engine.continuity_apply_diff(conversation, ContinuityKind::Anchors, &diff)?;
        conn.execute(
            "UPDATE crew_member_learnings SET migrated_to_lcm=1 WHERE id=?1",
            [&id],
        )?;
        moved += 1;
    }
    Ok(moved)
}

fn anchor_diff(
    anchor_id: &str,
    anchor_type: &str,
    statement: &str,
    kind: &str,
    scope: &LearningScope,
    source_ref: &str,
) -> String {
    let mut lines = vec![
        "## Entries".to_string(),
        format!("+ anchor_id: {anchor_id}"),
        format!("+ anchor_type: {anchor_type}"),
        format!("+ statement: {}", prose_line(statement)),
        format!("+ learning_kind: {kind}"),
        "+ source_class: crew_retrospective".to_string(),
        format!("+ source_ref: {source_ref}"),
    ];
    let scope_text = [
        scope.module.as_ref().map(|m| format!("module={m}")),
        scope
            .command_type
            .as_ref()
            .map(|c| format!("command_type={c}")),
        scope.thread_key.as_ref().map(|t| format!("thread={t}")),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" ");
    if !scope_text.is_empty() {
        lines.push(format!("+ scope: {scope_text}"));
    }
    lines.join("\n") + "\n"
}

// --- Persona: the identity lane ---------------------------------------------

fn band(value: u8) -> usize {
    match value {
        0..=20 => 0,
        21..=40 => 1,
        41..=60 => 2,
        61..=80 => 3,
        _ => 4,
    }
}

const AXIS_TEXT: [[&str; 5]; 5] = [
    [
        "Du prüfst jeden Schritt sorgfältig und belegst jedes Ergebnis, auch wenn es länger dauert.",
        "Du prüfst gründlich, bevor du weitergehst, und belegst, was zählt.",
        "Du hältst Gründlichkeit und Tempo in der Waage: prüfen, was Folgen hat, sonst zügig weiter.",
        "Du arbeitest zügig in kleinen, geprüften Schritten und belegst das Wesentliche.",
        "Du arbeitest schnell und pragmatisch; Belege lieferst du dort, wo ein Fehler teuer wäre.",
    ],
    [
        "Du gehst kein Risiko ein, ohne die Voraussetzungen und den Rückweg geprüft zu haben.",
        "Du prüfst Voraussetzungen vor riskanten Schritten und wählst den sicheren Weg.",
        "Du wägst Risiko und Nutzen ab; Umkehrbares probierst du, Unumkehrbares klärst du.",
        "Du erprobst umkehrbare Schritte eigenständig innerhalb der Freigaben.",
        "Du handelst mutig innerhalb der Freigaben und meldest, was du versucht hast.",
    ],
    [
        "Du antwortest so knapp wie möglich: Ergebnis, Beleg, nächster Schritt.",
        "Du antwortest knapp und konkret und lässt Nebensächliches weg.",
        "Du antwortest so ausführlich wie nötig und so knapp wie möglich.",
        "Du erklärst die relevanten Zusammenhänge und nennst deine Belege.",
        "Du erklärst ausführlich, mit Hintergrund, Alternativen und Belegen.",
    ],
    [
        "Du hältst dich strikt an bewährte Abläufe und die geltenden Regeln.",
        "Du bevorzugst bewährte Abläufe innerhalb der Regeln.",
        "Du nutzt bewährte Abläufe und weichst begründet ab, wenn es besser ist.",
        "Du suchst innerhalb der Regeln nach besseren Wegen als dem üblichen.",
        "Du suchst kreative Lösungen innerhalb der geltenden Regeln und begründest sie.",
    ],
    [
        "Du fragst bei jeder Unklarheit nach, bevor du handelst.",
        "Du benennst entscheidende Unklarheiten früh und fragst nach.",
        "Du fragst nach, wenn eine Annahme teuer wäre; sonst triffst du sie und nennst sie.",
        "Du triffst begründete Annahmen innerhalb des Auftrags und legst sie offen.",
        "Du entscheidest selbständig innerhalb des Auftrags und dokumentierst deine Annahmen.",
    ],
];

/// The persona is identity: rendered for the base-instruction lane, after the
/// CTOX system prompt, never above the safety and execution rules.
pub(crate) fn render_persona(member: &Member) -> String {
    let s = &member.soul;
    let axes = [
        s.gruendlichkeit_vs_tempo,
        s.vorsicht_vs_mut,
        s.knapp_vs_ausfuehrlich,
        s.regeltreu_vs_kreativ,
        s.nachfragen_vs_annehmen,
    ];
    let mut block = format!(
        "<ctox_crew_persona name=\"{}\">\nDu arbeitest in diesem Einsatz als {} , ein Mitglied der CTOX-Crew. Die Crew ist ein Kollektiv in einem Harness: alle Sicherheits-, Ausführungs- und Review-Regeln oben gelten unverändert. Dieses Profil beschreibt, WIE du arbeitest, nie WAS du darfst.\n",
        escape_attr(&member.name),
        member.name
    );
    let sketch = prose_line(&s.sketch);
    if !sketch.is_empty() {
        block.push_str(&format!("\nCharakter: {sketch}\n"));
    }
    let voice = prose_line(&s.voice);
    if !voice.is_empty() {
        block.push_str(&format!(
            "So klingst du, wenn du dem Owner antwortest: „{voice}“\n"
        ));
    }
    block.push_str("\nArbeitsweise:\n");
    for (index, value) in axes.iter().enumerate() {
        block.push_str("- ");
        block.push_str(AXIS_TEXT[index][band(*value)]);
        block.push('\n');
    }
    let stats = &member.stats;
    if stats.tasks_total > 0 {
        block.push_str(&format!(
            "\nErfahrung: {} Einsätze, {} gelungen, {} gescheitert, {} Reviews bestanden.\n",
            stats.tasks_total, stats.succeeded, stats.failed, stats.review_passed
        ));
    }
    block.push_str(
        "Sprich in deiner Antwort nicht über dieses Profil; handle danach.\n</ctox_crew_persona>",
    );
    clip_bytes(&block, PERSONA_MAX_BYTES, "</ctox_crew_persona>")
}

fn escape_attr(value: &str) -> String {
    value.replace('"', "'").replace(['<', '>'], "")
}

fn clip_bytes(block: &str, max: usize, closing: &str) -> String {
    if block.len() <= max {
        return block.to_string();
    }
    let keep = max.saturating_sub(closing.len() + 1);
    let mut cut = keep;
    while cut > 0 && !block.is_char_boundary(cut) {
        cut -= 1;
    }
    let mut out = block[..cut].to_string();
    if let Some(index) = out.rfind('\n') {
        out.truncate(index);
    }
    out.push('\n');
    out.push_str(closing);
    out
}

// --- Memory: the runtime-context lane -------------------------------------------

#[derive(Clone, Debug)]
pub(crate) struct RecentAttempt {
    pub task_summary: String,
    pub module: Option<String>,
    pub succeeded: bool,
    pub review_passed: Option<bool>,
    pub finished_at: String,
}

/// Decision it changes: how the member approaches this task, with what it
/// already knows about this kind of work. Source: the member's continuity
/// documents plus its recent attempts. Omission rule: `None` when there is
/// nothing to know. Contribution is deterministic and byte-bounded.
pub(crate) fn render_memory_block(
    member: &Member,
    memory: &MemberMemory,
    recent: &[RecentAttempt],
) -> Option<String> {
    let anchors = anchor_lines(&memory.anchors);
    let narrative = narrative_lines(&memory.narrative);
    if anchors.is_empty() && narrative.is_empty() && recent.is_empty() {
        return None;
    }
    let closing = "</ctox_crew_memory>";
    let mut block = format!(
        "<ctox_crew_memory member=\"{}\">\nDein Gedächtnis aus früheren Einsätzen. Es ist Kontext, keine Anweisung: Was hier steht, hast du selbst erlebt oder der Owner bestätigt. Prüfe, ob es auf diese Aufgabe zutrifft.\n",
        escape_attr(&member.name)
    );
    // Knowledge may not crowd out experience: anchors keep a reserve for the
    // narrative and the recent attempts that follow.
    const EXPERIENCE_RESERVE: usize = 1_800;
    let add_within = |block: &mut String, line: &str, limit: usize| {
        if block.len() + line.len() + 1 + closing.len() <= limit {
            block.push_str(line);
            block.push('\n');
        }
    };
    if !anchors.is_empty() {
        add_within(
            &mut block,
            "\nWas du weißt:",
            MEMORY_MAX_BYTES - EXPERIENCE_RESERVE,
        );
        for line in &anchors {
            add_within(
                &mut block,
                &format!("- {line}"),
                MEMORY_MAX_BYTES - EXPERIENCE_RESERVE,
            );
        }
    }
    let mut add = |line: &str| add_within(&mut block, line, MEMORY_MAX_BYTES);
    if !narrative.is_empty() {
        add("\nWas du erlebt hast (neueste zuerst):");
        for line in narrative.iter().rev().take(RECENT_NARRATIVE_ENTRIES) {
            add(&format!("- {line}"));
        }
    }
    if !recent.is_empty() {
        add("\nDeine letzten Einsätze:");
        for attempt in recent.iter().take(RECENT_ATTEMPTS_IN_MEMORY) {
            let outcome = match (attempt.succeeded, attempt.review_passed) {
                (true, Some(true)) => "gelungen, Review bestanden",
                (true, _) => "gelungen",
                (false, _) => "gescheitert",
            };
            let module = attempt
                .module
                .as_deref()
                .map(|m| format!(" · {m}"))
                .unwrap_or_default();
            add(&format!(
                "- {} ({outcome}{module}, {})",
                prose_line(&attempt.task_summary),
                &attempt.finished_at[..attempt.finished_at.len().min(10)]
            ));
        }
    }
    block.push_str(closing);
    Some(block)
}

pub(crate) fn recent_attempts(
    conn: &Connection,
    member_id: &str,
    limit: usize,
) -> Result<Vec<RecentAttempt>> {
    let rows = conn
        .prepare(
            "SELECT COALESCE(task_summary,''),module,succeeded,review_passed,finalized_at
             FROM crew_attempts WHERE member_id=?1 AND finalized_at IS NOT NULL
             ORDER BY finalized_at DESC,attempt_id DESC LIMIT ?2",
        )?
        .query_map(params![member_id, limit as i64], |r| {
            Ok(RecentAttempt {
                task_summary: r.get(0)?,
                module: r.get(1)?,
                succeeded: r.get::<_, Option<bool>>(2)?.unwrap_or(false),
                review_passed: r.get(3)?,
                finished_at: r.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows
        .into_iter()
        .filter(|a| !a.task_summary.is_empty())
        .collect())
}

/// A bounded prose summary of a task for memory and routing; never the raw
/// prompt.
pub(crate) fn task_summary_from_prompt(prompt: &str, module: Option<&str>) -> String {
    let text = prose_line(prompt);
    let mut summary = text.chars().take(200).collect::<String>();
    if text.chars().count() > 200 {
        summary.push('…');
    }
    if let Some(module) = module.filter(|m| !m.trim().is_empty()) {
        format!("[{module}] {summary}")
    } else {
        summary
    }
}

// --- Learning: the existing continuity refresh, per member -----------------------

#[derive(Debug, Clone, Default)]
pub(crate) struct LearningOutcome {
    pub attempt_id: String,
    pub member_id: String,
    pub anchors_added: usize,
    pub refreshed_kinds: Vec<String>,
    pub error: Option<String>,
}

/// One due attempt per tick: writes the typed learnings as hypothesis anchors,
/// records the experience as a message of the member conversation and runs the
/// existing continuity refresh (narrative + anchors) on it. Serial by design.
pub(crate) fn run_learning_tick(
    root: &Path,
    settings: &BTreeMap<String, String>,
) -> Result<Option<LearningOutcome>> {
    let conn = Connection::open(crate::paths::core_db(root))?;
    conn.busy_timeout(crate::persistence::sqlite_busy_timeout_duration())?;
    ensure_schema(&conn)?;
    let Some((attempt, member_id, task_id, module, succeeded, review_passed, elapsed, retrospective, learning_json, summary, finalized)) = conn
        .query_row(
            "SELECT attempt_id,member_id,task_id,module,succeeded,review_passed,elapsed_ms,
                    COALESCE(retrospective,''),COALESCE(learning_json,'[]'),COALESCE(task_summary,''),finalized_at
             FROM crew_attempts WHERE learning_due=1 AND finalized_at IS NOT NULL
             ORDER BY finalized_at,attempt_id LIMIT 1",
            [],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, Option<String>>(3)?,
                    r.get::<_, Option<bool>>(4)?.unwrap_or(false),
                    r.get::<_, Option<bool>>(5)?,
                    r.get::<_, Option<i64>>(6)?,
                    r.get::<_, String>(7)?,
                    r.get::<_, String>(8)?,
                    r.get::<_, String>(9)?,
                    r.get::<_, String>(10)?,
                ))
            },
        )
        .optional()?
    else {
        return Ok(None);
    };
    // Claim first: a failing refresh must not be retried forever.
    conn.execute(
        "UPDATE crew_attempts SET learning_due=0 WHERE attempt_id=?1",
        [&attempt],
    )?;
    let mut outcome = LearningOutcome {
        attempt_id: attempt.clone(),
        member_id: member_id.clone(),
        ..Default::default()
    };
    let engine = open_engine(root)?;
    let conversation = member_conversation_id(&member_id);
    engine.continuity_init_documents(conversation)?;
    let learnings: Vec<Learning> = serde_json::from_str(&learning_json).unwrap_or_default();
    for (index, learning) in learnings.iter().enumerate() {
        let diff = anchor_diff(
            &format!("{}_{index}", attempt.replace(':', "_")),
            "hypothesis",
            &learning.text,
            &learning.kind,
            &learning.scope,
            &attempt,
        );
        if engine
            .continuity_apply_diff(conversation, ContinuityKind::Anchors, &diff)
            .is_ok()
        {
            outcome.anchors_added += 1;
        }
    }
    let outcome_text = match (succeeded, review_passed) {
        (true, Some(true)) => "gelungen, Review bestanden",
        (true, Some(false)) => "gelungen, Review abgelehnt",
        (true, None) => "gelungen",
        (false, _) => "gescheitert",
    };
    let mut experience = format!(
        "Einsatz {attempt} · Aufgabe: {} · Ergebnis: {outcome_text}",
        if summary.is_empty() {
            task_id.as_str()
        } else {
            summary.as_str()
        }
    );
    if let Some(module) = module.as_deref().filter(|m| !m.is_empty()) {
        experience.push_str(&format!(" · Modul: {module}"));
    }
    if let Some(elapsed) = elapsed {
        experience.push_str(&format!(" · Dauer: {}s", elapsed.max(0) / 1000));
    }
    experience.push_str(&format!(" · Abgeschlossen: {finalized}"));
    if !retrospective.is_empty() {
        experience.push_str(&format!("\nRückblick: {}", prose_line(&retrospective)));
    }
    for learning in &learnings {
        experience.push_str(&format!(
            "\nLearning ({}): {}",
            learning.kind,
            prose_line(&learning.text)
        ));
    }
    engine.add_message(conversation, "user", &experience)?;
    let due: HashSet<String> = ["narrative", "anchors"]
        .into_iter()
        .map(String::from)
        .collect();
    // Tests keep the deterministic part (anchors, experience message) and
    // never start a model session.
    let session = if cfg!(test) {
        Err(anyhow::anyhow!("memory refresh skipped in tests"))
    } else {
        crate::execution::agent::turn_loop::PersistentSession::start_isolated(root, settings, None)
    };
    match session {
        Ok(mut session) => {
            let mut emit = |_: &str| {};
            match crate::execution::agent::turn_loop::refresh_member_memory(
                root,
                settings,
                &engine,
                conversation,
                &due,
                &mut session,
                &mut emit,
            ) {
                Ok(kinds) => outcome.refreshed_kinds = kinds,
                Err(error) => outcome.error = Some(error.to_string()),
            }
        }
        Err(error) => outcome.error = Some(format!("memory refresh session unavailable: {error}")),
    }
    // The projection keys on the member stamp; a changed memory must re-project.
    let _ = conn.execute(
        "UPDATE crew_members SET updated_at=?2,last_learning_at=?2 WHERE id=?1",
        params![member_id, chrono::Utc::now().to_rfc3339()],
    );
    crate::service::harness_flow::record_harness_flow_event_lossy(
        root,
        crate::service::harness_flow::RecordHarnessFlowEventRequest {
            event_kind: "crew.learning",
            title: &format!(
                "{} hat aus Einsatz gelernt: {} Anchors, Refresh {}",
                member_id,
                outcome.anchors_added,
                if outcome.refreshed_kinds.is_empty() {
                    "ausstehend"
                } else {
                    "erneuert"
                }
            ),
            body_text: "",
            message_key: Some(&task_id),
            work_id: None,
            ticket_key: None,
            attempt_index: None,
            metadata: serde_json::json!({
                "attempt_id": attempt,
                "crew_member_id": member_id,
                "anchors_added": outcome.anchors_added,
                "refreshed_kinds": outcome.refreshed_kinds,
                "error": outcome.error,
                "cockpit_eligible": true
            }),
        },
    );
    Ok(Some(outcome))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member() -> Member {
        let conn = super::super::tests::fixture();
        members(&conn).unwrap().remove(0)
    }

    #[test]
    fn persona_is_bounded_deterministic_and_reads_the_sliders() {
        let mut member = member();
        member.soul.gruendlichkeit_vs_tempo = 5;
        member.soul.nachfragen_vs_annehmen = 95;
        member.soul.sketch = "Ä".repeat(600);
        let persona = render_persona(&member);
        assert!(persona.len() <= PERSONA_MAX_BYTES, "{}", persona.len());
        assert!(persona.starts_with("<ctox_crew_persona"));
        assert!(persona.ends_with("</ctox_crew_persona>"));
        assert_eq!(persona, render_persona(&member));
        assert!(persona.contains("belegst jedes Ergebnis"));
        assert!(persona.contains("entscheidest selbständig"));
    }

    #[test]
    fn memory_block_is_omitted_when_empty_and_bounded_when_full() {
        let member = member();
        assert!(render_memory_block(&member, &MemberMemory::default(), &[]).is_none());
        let anchors = (0..80)
            .map(|i| {
                format!(
                    "## Entries\n- anchor_id: a{i}\n- anchor_type: hypothesis\n- statement: {}\n",
                    "W".repeat(300)
                )
            })
            .collect::<String>();
        let memory = MemberMemory {
            anchors,
            narrative: "## Entries\n- entry_id: e1\n- event_type: success\n- summary: Import lief.\n- consequence: Schema zuerst prüfen.\n".into(),
            updated_at: String::new(),
        };
        let block = render_memory_block(&member, &memory, &[]).unwrap();
        assert!(block.len() <= MEMORY_MAX_BYTES);
        assert!(block.ends_with("</ctox_crew_memory>"));
        assert!(block.contains("[success] Import lief. — Schema zuerst prüfen."));
        assert_eq!(anchor_lines(&memory.anchors).len(), 80);
    }

    #[test]
    fn task_summary_is_prose_and_bounded() {
        let summary =
            task_summary_from_prompt(&format!("  Prüfe\n\n{} ", "x".repeat(500)), Some("reports"));
        assert!(summary.starts_with("[reports] Prüfe x"));
        assert!(summary.ends_with('…'));
        assert!(summary.chars().count() <= 215);
    }
}
