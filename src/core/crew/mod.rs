//! Durable crew identity. Selection and prompt rendering are pure; migrations and
//! attempt bookkeeping remain owned by CTOX, never by the embedded worker.
mod finalization;
pub(crate) use finalization::*;
mod lifecycle;
#[cfg(test)]
mod lifecycle_tests;
mod memory;
mod router;
mod runtime;
use anyhow::{bail, Context, Result};
pub(crate) use lifecycle::*;
pub(crate) use memory::*;
pub(crate) use router::*;
pub(crate) use runtime::*;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
pub(crate) const PUBLIC_MEMBER_FIELDS: &[&str] = &[
    "id",
    "name",
    "shape",
    "color",
    "archived",
    "state",
    "active_task_id",
    "updated_at_ms",
];
pub(crate) fn public_member_document(document: &mut serde_json::Value) {
    if let Some(object) = document.as_object_mut() {
        object.retain(|key, _| PUBLIC_MEMBER_FIELDS.contains(&key.as_str()));
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Soul {
    pub gruendlichkeit_vs_tempo: u8,
    pub vorsicht_vs_mut: u8,
    pub knapp_vs_ausfuehrlich: u8,
    pub regeltreu_vs_kreativ: u8,
    pub nachfragen_vs_annehmen: u8,
    pub sketch: String,
    pub voice: String,
}
impl Soul {
    pub fn normalize(&mut self) {
        self.sketch = prose_line(&self.sketch);
    }
    pub fn validate(&self) -> Result<()> {
        if [
            self.gruendlichkeit_vs_tempo,
            self.vorsicht_vs_mut,
            self.knapp_vs_ausfuehrlich,
            self.regeltreu_vs_kreativ,
            self.nachfragen_vs_annehmen,
        ]
        .iter()
        .any(|v| *v > 100)
            || self.sketch.chars().count() > 600
            || self.voice.chars().count() > 200
            || self.voice.trim().is_empty()
            || self.voice.contains(['\n', '\r'])
            || self.voice.matches(['.', '!', '?']).count() > 1
            || (!self.sketch.is_empty() && !safe_prose(&self.sketch, 600))
        {
            bail!("invalid crew soul: axes must be 0–100, sketch ≤600 and voice ≤200 characters");
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct Specialties {
    pub modules: Vec<String>,
    pub command_types: Vec<String>,
    pub skills: Vec<String>,
    pub tags: Vec<String>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct Stats {
    pub tasks_total: u64,
    pub succeeded: u64,
    pub failed: u64,
    pub review_passed: u64,
    pub review_rejected: u64,
    pub avg_elapsed_ms: u64,
    pub last_active_at: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Member {
    pub id: String,
    pub name: String,
    pub shape: String,
    pub color: String,
    pub created_at: String,
    pub archived: bool,
    pub soul: Soul,
    pub specialties: Specialties,
    pub stats: Stats,
    pub updated_at: String,
}

pub(crate) fn ensure_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS crew_members (
            id TEXT PRIMARY KEY, name TEXT NOT NULL, shape TEXT NOT NULL
                CHECK(shape IN ('round','square','triangle','blob')),
            color TEXT NOT NULL, created_at TEXT NOT NULL, archived INTEGER NOT NULL DEFAULT 0,
            soul_json TEXT NOT NULL, specialties_json TEXT NOT NULL,
            stats_json TEXT NOT NULL, updated_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS crew_member_learnings (
            id TEXT PRIMARY KEY, member_id TEXT NOT NULL REFERENCES crew_members(id),
            text TEXT NOT NULL, normalized_text TEXT NOT NULL, kind TEXT NOT NULL
                CHECK(kind IN ('insight','pitfall','preference')),
            scope_json TEXT NOT NULL, evidence_run_id TEXT NOT NULL, created_at TEXT NOT NULL,
            confirmed_by_owner INTEGER NOT NULL DEFAULT 0, archived INTEGER NOT NULL DEFAULT 0,
            UNIQUE(member_id, normalized_text)
        );
        CREATE INDEX IF NOT EXISTS idx_crew_learning_retention
            ON crew_member_learnings(member_id, confirmed_by_owner, created_at, id);
        CREATE INDEX IF NOT EXISTS idx_crew_learning_context
            ON crew_member_learnings(member_id, archived, confirmed_by_owner DESC, created_at DESC, id);
        CREATE TABLE IF NOT EXISTS crew_attempts (
            attempt_id TEXT PRIMARY KEY, task_id TEXT NOT NULL, member_id TEXT NOT NULL,
            module TEXT, thread_key TEXT, selected_at TEXT NOT NULL,
            selection_reason TEXT NOT NULL DEFAULT '',
            finalized_at TEXT, succeeded INTEGER, review_passed INTEGER,
            elapsed_ms INTEGER, retrospective TEXT
        );

        CREATE INDEX IF NOT EXISTS idx_crew_attempt_thread ON crew_attempts(thread_key, selected_at DESC, attempt_id DESC);
        CREATE INDEX IF NOT EXISTS idx_crew_attempt_finished ON crew_attempts(finalized_at DESC, attempt_id DESC);
        CREATE INDEX IF NOT EXISTS idx_crew_attempt_member_finished ON crew_attempts(member_id, finalized_at DESC, attempt_id DESC);"
    )?;
    let has_column: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM pragma_table_info('communication_routing_state') WHERE name='crew_member_id')", [], |r| r.get(0))?;
    if !has_column {
        conn.execute_batch(
            "ALTER TABLE communication_routing_state ADD COLUMN crew_member_id TEXT;",
        )?;
    }
    for (table, column) in [
        ("communication_routing_state", "crew_assigned_member_id"),
        ("crew_attempts", "started_at"),
        // Memory in the LCM (2026-09-06): the task summary feeds routing and
        // experience; learnings travel as JSON until the learner has written
        // them into the member's anchors; legacy rows are migrated once.
        ("crew_attempts", "task_summary"),
        ("crew_attempts", "learning_json"),
        ("crew_member_learnings", "migrated_to_lcm"),
        // Expression stamps (2026-09-07): when a member last read its memory
        // and last learned; the app shows "liest"/"lernt" from them.
        ("crew_members", "last_memory_read_at"),
        ("crew_members", "last_learning_at"),
    ] {
        let exists: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info(?1) WHERE name=?2)",
            params![table, column],
            |r| r.get(0),
        )?;
        if !exists {
            conn.execute_batch(&format!("ALTER TABLE {table} ADD COLUMN {column} TEXT;"))?;
        }
    }
    let has_learning_due: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM pragma_table_info('crew_attempts') WHERE name='learning_due')",
        [],
        |r| r.get(0),
    )?;
    if !has_learning_due {
        conn.execute_batch(
            "ALTER TABLE crew_attempts ADD COLUMN learning_due INTEGER NOT NULL DEFAULT 0;",
        )?;
    }
    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_crew_attempt_learning_due
            ON crew_attempts(finalized_at, attempt_id) WHERE learning_due=1;
         CREATE INDEX IF NOT EXISTS idx_crew_attempt_unstarted
            ON crew_attempts(started_at,finalized_at,selected_at,attempt_id)
            WHERE started_at IS NULL AND finalized_at IS NULL;
         CREATE TABLE IF NOT EXISTS crew_projection_tombstones(event_id TEXT PRIMARY KEY);",
    )?;
    ensure_selection_event_index(conn)?;
    // Learning edits/deletions invalidate the member projection without scanning
    // its learning rows on every wake. Advance even for two writes in one ms.
    for (operation, reference) in [("INSERT", "NEW"), ("UPDATE", "NEW"), ("DELETE", "OLD")] {
        conn.execute_batch(&format!(
            "CREATE TRIGGER IF NOT EXISTS crew_learning_touch_{operation}
             AFTER {operation} ON crew_member_learnings BEGIN
                 UPDATE crew_members SET updated_at=strftime('%Y-%m-%dT%H:%M:%fZ',
                     max(julianday('now'), coalesce(julianday(updated_at),0)+0.001/86400.0))
                 WHERE id={reference}.member_id;
             END;"
        ))?;
    }
    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_crew_active_task
        ON communication_routing_state(crew_member_id,route_status,leased_at,message_key);",
    )?;
    // Stable IDs plus INSERT OR IGNORE preserve operator edits and archived seeds.

    let now = chrono::Utc::now().to_rfc3339();
    for (id, name, shape, color, axes, sketch, modules, commands, tags) in [
        (
            "crew-milo",
            "Milo",
            "round",
            "#1685ee",
            [25, 35, 35, 35, 40],
            "Ich prüfe Zusammenhänge und mache Änderungen nachvollziehbar.",
            vec!["ctox", "browser"],
            vec!["ctox.coding.turn"],
            vec!["code", "apps"],
        ),
        (
            "crew-nori",
            "Nori",
            "square",
            "#00aa9a",
            [15, 25, 70, 45, 25],
            "Ich suche belastbare Quellen und benenne Unsicherheit.",
            vec!["knowledge"],
            vec!["knowledge.research"],
            vec!["research", "recherche", "wissen"],
        ),
        (
            "crew-lumi",
            "Lumi",
            "triangle",
            "#e97255",
            [20, 20, 40, 20, 30],
            "Ich prüfe Daten sorgfältig und halte Strukturen konsistent.",
            vec!["reports"],
            vec!["data.import"],
            vec!["import", "daten", "tabellen"],
        ),
        (
            "crew-pico",
            "Pico",
            "blob",
            "#7c6df2",
            [50, 40, 65, 45, 25],
            "Ich fasse Anliegen klar zusammen und achte auf Rückmeldungen.",
            vec!["tickets", "outbound"],
            vec!["chat.message"],
            vec!["kommunikation", "tickets", "chat"],
        ),
    ] {
        let soul = Soul {
            gruendlichkeit_vs_tempo: axes[0],
            vorsicht_vs_mut: axes[1],
            knapp_vs_ausfuehrlich: axes[2],
            regeltreu_vs_kreativ: axes[3],
            nachfragen_vs_annehmen: axes[4],
            sketch: sketch.into(),
            voice: "Freundlich, konkret und ohne unbelegte Behauptungen.".into(),
        };
        let specialties = Specialties {
            modules: modules.into_iter().map(String::from).collect(),
            command_types: commands.into_iter().map(String::from).collect(),
            skills: vec![],
            tags: tags.into_iter().map(String::from).collect(),
        };
        conn.execute("INSERT OR IGNORE INTO crew_members
            (id,name,shape,color,created_at,archived,soul_json,specialties_json,stats_json,updated_at)
            VALUES (?1,?2,?3,?4,?5,0,?6,?7,?8,?5)",
            params![id,name,shape,color,now,serde_json::to_string(&soul)?,serde_json::to_string(&specialties)?,serde_json::to_string(&Stats::default())?])?;
    }
    // Lumi was seeded in the neutral "no member" grey, so its tasks looked
    // unassigned and colourless. Recolour only the untouched seed; an owner's
    // own colour choice stays.
    conn.execute(
        "UPDATE crew_members SET color='#e97255', updated_at=?1
         WHERE id='crew-lumi' AND color='#7d7f84'",
        params![now],
    )?;
    Ok(())
}

pub(crate) fn members(conn: &Connection) -> Result<Vec<Member>> {
    Ok(members_with_errors(conn)?.0)
}

fn members_with_errors(conn: &Connection) -> Result<(Vec<Member>, Vec<String>)> {
    let mut statement = conn.prepare("SELECT id,name,shape,color,created_at,archived,soul_json,specialties_json,stats_json,updated_at FROM crew_members ORDER BY id LIMIT 1000")?;
    let rows = statement.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, String>(4)?,
            r.get::<_, bool>(5)?,
            r.get::<_, String>(6)?,
            r.get::<_, String>(7)?,
            r.get::<_, String>(8)?,
            r.get::<_, String>(9)?,
        ))
    })?;
    let mut members = Vec::new();
    let mut errors = Vec::new();
    for r in rows {
        let (id, name, shape, color, created_at, archived, soul, specialties, stats, updated_at) =
            r?;
        let parsed = (|| -> Result<Member> {
            Ok(Member {
                id: id.clone(),
                name,
                shape,
                color,
                created_at,
                archived,
                soul: serde_json::from_str(&soul)?,
                specialties: serde_json::from_str(&specialties)?,
                stats: serde_json::from_str(&stats)?,
                updated_at,
            })
        })();
        match parsed {
            Ok(member) => members.push(member),
            // Never copy malformed JSON (potentially sensitive) into diagnostics.
            Err(_) => errors.push(format!("invalid crew member data: {id}")),
        }
    }
    Ok((members, errors))
}

#[derive(Default)]
pub(crate) struct TaskTraits {
    pub module: Option<String>,
    pub command_type: Option<String>,
    pub thread_key: Option<String>,
    pub skills: Vec<String>,
    pub tags: Vec<String>,
    pub manual_member: Option<String>,
    pub continuity_member: Option<String>,
}
#[derive(Default)]
pub(crate) struct History {
    pub member_id: String,
    pub module: Option<String>,
    pub thread_key: Option<String>,
    pub succeeded: bool,
    pub finished_at_ms: i64,
}
#[derive(Debug, PartialEq)]
pub(crate) struct Selection {
    pub member_id: String,
    pub reason: String,
}
/// Owner assignment before the lease: the router honours it first. Works on
/// the transaction or connection the caller holds; the task must be unleased.
pub(crate) fn assign_member_before_lease(
    conn: &Connection,
    task_id: &str,
    member_id: &str,
    now: &str,
) -> Result<()> {
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM crew_members WHERE id=?1 AND archived=0)",
        [member_id],
        |r| r.get(0),
    )?;
    if !exists {
        bail!("active member not found");
    }
    let changed = conn.execute(
        "UPDATE communication_routing_state SET crew_assigned_member_id=?2,updated_at=?3
         WHERE message_key=?1 AND route_status IN ('pending','blocked') AND lease_owner IS NULL",
        params![task_id, member_id, now],
    )?;
    if changed != 1 {
        bail!("assignment requires an unleased pending or blocked task");
    }
    Ok(())
}

/// No I/O or clock access: every scheduling input is explicit and replayable.
pub(crate) fn select(
    candidates: &[Member],
    task: &TaskTraits,
    history: &[History],
    now_ms: i64,
) -> Option<Selection> {
    for (wanted, reason) in [
        (
            &task.manual_member,
            "assigned: Manuelle Zuordnung vor dem Lease",
        ),
        (
            &task.continuity_member,
            "continuity: Kontinuität im bestehenden Thread",
        ),
    ] {
        if let Some(member) = wanted
            .as_ref()
            .and_then(|id| candidates.iter().find(|m| &m.id == id && !m.archived))
        {
            return Some(Selection {
                member_id: member.id.clone(),
                reason: format!("{}: {} ({})", reason, member.name, member.id),
            });
        }
    }
    candidates.iter().filter(|m| !m.archived).map(|m| {
        let hits = usize::from(task.module.as_ref().is_some_and(|v| m.specialties.modules.contains(v)))
            + usize::from(task.command_type.as_ref().is_some_and(|v| m.specialties.command_types.contains(v)))
            + task.skills.iter().filter(|v| m.specialties.skills.contains(v)).count()
            + task.tags.iter().filter(|v| m.specialties.tags.contains(v)).count();
        let successes = history.iter().filter(|h| h.member_id==m.id && h.succeeded &&
            ((task.module.is_some() && h.module==task.module) || (task.thread_key.is_some() && h.thread_key==task.thread_key))).count().min(10);
        let failures = history.iter().filter(|h| h.member_id==m.id && !h.succeeded && task.module.is_some() && h.module==task.module && h.finished_at_ms >= now_ms.saturating_sub(86_400_000)).count().min(10);
        (m, (hits.min(20) * 10 + successes * 3) as i64 - (failures * 5) as i64, hits, successes, failures)
    }).min_by(|a,b| b.1.cmp(&a.1).then_with(|| a.0.stats.last_active_at.cmp(&b.0.stats.last_active_at)).then_with(|| a.0.id.cmp(&b.0.id)))
        .map(|(m,score,hits,successes,failures)| Selection { member_id:m.id.clone(),reason:format!("selected: {} ({}): {} Punkte; {} Spezialitäten-Treffer, {} passende Erfolge, {} Modul-Fehlschläge in 24 h; Gleichstand nach letzter Aktivität und ID",m.name,m.id,score,hits,successes,failures) })
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct LearningScope {
    pub module: Option<String>,
    pub command_type: Option<String>,
    pub thread_key: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Learning {
    pub text: String,
    pub kind: String,
    pub scope: LearningScope,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Retrospective {
    pub retrospective: String,
    pub learnings: Vec<Learning>,
}
pub(crate) fn normalized(text: &str) -> String {
    prose_line(text).to_lowercase()
}
pub(crate) fn prose_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}
/// Store only prose. Reject path/credential-shaped content rather than guessing at
/// redaction; arbitrary worker fields never enter durable crew records.
pub(crate) fn safe_prose(text: &str, limit: usize) -> bool {
    let prose = prose_line(text);
    let lower = prose.to_lowercase();
    static SENSITIVE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let sensitive = SENSITIVE.get_or_init(|| regex::Regex::new(
        r"(?i)(?:/users/|/home/|/tmp/|/var/|[a-z]:\\|~/|\./|(?:^|\s)/[\w.-]+|\\+[\w.-]+\\[\w.-]+|\b[\w.-]+(?:/[\w.-]+)+\.[a-z0-9]+\b|\b(?:AKIA|ghp_|eyJ|sk-)|\b[\w-]*key[\w-]*\s*=|[\w.+-]+@[\w.-]+\.[a-z]{2,})"
    ).expect("constant crew prose pattern"));
    !text.trim().is_empty()
        && text.chars().count() <= limit
        && !text.contains('\0')
        && !sensitive.is_match(&prose)
        && !text.chars().any(|c| c.is_control() && !c.is_whitespace())
        && ![
            "bearer ",
            "api_key",
            "api-key",
            "password",
            "passwd",
            "secret",
            "token=",
            "token:",
            "passwort",
            "authorization",
            "credential",
            "pwd=",
            "private key",
            "ctox_crew_soul",
        ]
        .iter()
        .any(|s| lower.contains(s))
}
impl Retrospective {
    pub fn normalize(&mut self) {
        self.retrospective = prose_line(&self.retrospective);
        for learning in &mut self.learnings {
            learning.text = prose_line(&learning.text);
        }
    }
    pub fn validate(&self, passed: bool, owner_feedback: Option<&str>) -> Result<()> {
        if !safe_prose(&self.retrospective, 300) || self.learnings.len() > 3 {
            bail!("invalid crew retrospective");
        }
        for learning in &self.learnings {
            if !safe_prose(&learning.text, 400) {
                bail!("invalid learning text");
            }
            match learning.kind.as_str() {
                "insight" if passed => {}
                "pitfall" => {}
                "preference"
                    if owner_feedback.is_some_and(|feedback| {
                        // Exact quoted feedback is evidence, not the worker's own claim.
                        learning
                            .text
                            .split('"')
                            .skip(1)
                            .step_by(2)
                            .any(|quote| quote.chars().count() >= 8 && feedback.contains(quote))
                    }) => {}
                _ => bail!("learning kind is not supported by attempt evidence"),
            }
            for value in [
                &learning.scope.module,
                &learning.scope.command_type,
                &learning.scope.thread_key,
            ]
            .into_iter()
            .flatten()
            {
                if !safe_prose(value, 200) {
                    bail!("invalid learning scope");
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    pub(super) fn fixture() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE communication_routing_state(message_key TEXT PRIMARY KEY,route_status TEXT,leased_at TEXT);
            INSERT INTO communication_routing_state(message_key) VALUES ('existing');",
        )
        .unwrap();
        ensure_schema(&conn).unwrap();
        conn
    }
    #[test]
    fn fixture_and_central_policy_keep_private_crew_fields_and_grants_closed() {
        use crate::business_os::policy::*;
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../rxdb/tests/fixtures/crew-identity.json"))
                .unwrap();
        let mut public = fixture["member"].clone();
        let declared: BTreeSet<&str> = fixture["public_fields"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(
            PUBLIC_MEMBER_FIELDS
                .iter()
                .copied()
                .collect::<BTreeSet<_>>(),
            declared
        );
        public_member_document(&mut public);
        assert!(public.get("soul").is_none());
        assert!(public.get("stats").is_none());
        for role in ["admin", "founder", "user"] {
            let actor = BusinessOsActor::new(Some(role.into()), role);
            for command in fixture["commands"].as_array().unwrap() {
                let command = command.as_str().unwrap();
                assert_eq!(
                    evaluate(
                        &actor,
                        BusinessOsPermission::CrewManage,
                        &BusinessOsScope::record(command)
                    )
                    .allowed,
                    role == "admin"
                        || (role == "founder"
                            && matches!(
                                command,
                                "ctox.crew.learning.confirm" | "ctox.crew.learning.update"
                            ))
                );
            }
            for collection in ["ctox_crew_members", "ctox_crew_learnings"] {
                assert!(
                    !evaluate(
                        &actor,
                        BusinessOsPermission::DataWrite,
                        &BusinessOsScope::collection(collection)
                    )
                    .allowed
                );
                assert_eq!(
                    evaluate(
                        &actor,
                        BusinessOsPermission::DataRead,
                        &BusinessOsScope::collection(collection)
                    )
                    .allowed,
                    collection == "ctox_crew_members" || role != "user"
                );
            }
        }
    }
    #[test]
    fn seed_grey_lumi_gets_a_colour_but_owner_colours_stay() {
        let conn = fixture();
        conn.execute(
            "UPDATE crew_members SET color='#7d7f84' WHERE id='crew-lumi'",
            [],
        )
        .unwrap();
        conn.execute(
            "UPDATE crew_members SET color='#7d7f84' WHERE id='crew-pico'",
            [],
        )
        .unwrap();
        ensure_schema(&conn).unwrap();
        let all = members(&conn).unwrap();
        let color = |id: &str| all.iter().find(|m| m.id == id).unwrap().color.clone();
        assert_eq!(color("crew-lumi"), "#e97255");
        assert_eq!(
            color("crew-pico"),
            "#7d7f84",
            "an owner colour is not rewritten"
        );
        assert_eq!(
            all.iter()
                .filter(|m| m.id != "crew-pico")
                .map(|m| m.color.as_str())
                .collect::<BTreeSet<_>>()
                .len(),
            3,
            "the seeded crew wears distinct colours"
        );
        conn.execute(
            "UPDATE crew_members SET color='#34a26f' WHERE id='crew-lumi'",
            [],
        )
        .unwrap();
        ensure_schema(&conn).unwrap();
        assert_eq!(
            members(&conn)
                .unwrap()
                .iter()
                .find(|m| m.id == "crew-lumi")
                .unwrap()
                .color,
            "#34a26f",
            "an owner's later colour for Lumi stays"
        );
    }

    #[test]
    fn migration_preserves_existing_rows_and_seed_edits() {
        let conn = fixture();
        conn.execute(
            "UPDATE crew_members SET name='Owner name', archived=1 WHERE id='crew-milo'",
            [],
        )
        .unwrap();
        ensure_schema(&conn).unwrap();
        let all = members(&conn).unwrap();
        assert_eq!(all.len(), 4);
        assert_eq!(
            all.iter().find(|m| m.id == "crew-milo").unwrap().name,
            "Owner name"
        );
        assert_eq!(
            all.iter().map(|m| &m.shape).collect::<BTreeSet<_>>().len(),
            4
        );
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM communication_routing_state WHERE message_key='existing'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
    }
    #[test]
    fn selection_manual_continuity_archive_and_stable_ties() {
        let mut all = members(&fixture()).unwrap();
        let mut task = TaskTraits {
            manual_member: Some("crew-milo".into()),
            continuity_member: Some("crew-nori".into()),
            ..Default::default()
        };
        assert_eq!(select(&all, &task, &[], 0).unwrap().member_id, "crew-milo");
        all.iter_mut()
            .find(|m| m.id == "crew-milo")
            .unwrap()
            .archived = true;
        assert_eq!(select(&all, &task, &[], 0).unwrap().member_id, "crew-nori");
        task.manual_member = None;
        task.continuity_member = None;
        let chosen = select(&all, &task, &[], 0);
        all.reverse();
        assert_eq!(select(&all, &task, &[], 0), chosen);
        let previous = chosen.unwrap().member_id;
        all.iter_mut()
            .find(|member| member.id == previous)
            .unwrap()
            .stats
            .last_active_at = Some("2026-09-05T12:00:00Z".into());
        assert_ne!(select(&all, &task, &[], 0).unwrap().member_id, previous);
        task.module = Some("tickets".into());
        assert_eq!(select(&all, &task, &[], 0).unwrap().member_id, "crew-pico");
    }
    #[test]
    fn selection_scores_history_and_recent_failures_without_ambient_time() {
        let all = members(&fixture()).unwrap();
        let task = TaskTraits {
            module: Some("ctox".into()),
            ..Default::default()
        };
        assert_eq!(
            select(&all, &task, &[], 100_000_000).unwrap().member_id,
            "crew-milo"
        );
        let failures = (0..3)
            .map(|_| History {
                member_id: "crew-milo".into(),
                module: Some("ctox".into()),
                finished_at_ms: 100_000_000,
                ..Default::default()
            })
            .collect::<Vec<_>>();
        assert_ne!(
            select(&all, &task, &failures, 100_000_000)
                .unwrap()
                .member_id,
            "crew-milo"
        );
        assert_eq!(
            select(&all, &task, &failures, 200_000_000)
                .unwrap()
                .member_id,
            "crew-milo"
        );
        let successes = (0..4)
            .map(|_| History {
                member_id: "crew-nori".into(),
                module: Some("ctox".into()),
                succeeded: true,
                finished_at_ms: 100_000_000,
                ..Default::default()
            })
            .collect::<Vec<_>>();
        assert_eq!(
            select(&all, &task, &successes, 100_000_000)
                .unwrap()
                .member_id,
            "crew-nori"
        );
        assert!(select(&[], &task, &[], 0).is_none());
    }
    #[test]
    fn learnings_require_evidence_and_prose() {
        let mut r = Retrospective {
            retrospective: "Die Prüfung hat den Fehler eingegrenzt.".into(),
            learnings: vec![Learning {
                text: "Vor dem Import das Schema prüfen.".into(),
                kind: "insight".into(),
                scope: LearningScope::default(),
            }],
        };
        assert!(r.validate(false, None).is_err());
        assert!(r.validate(true, None).is_ok());
        r.learnings[0].kind = "preference".into();
        assert!(r.validate(true, None).is_err());
        r.learnings[0].text = "Owner sagte: \"Bitte kurz antworten\".".into();
        assert!(r.validate(true, Some("Bitte kurz antworten")).is_ok());
        r.learnings[0].text = "Datei /Users/private prüfen".into();
        assert!(r.validate(true, None).is_err());
        assert_eq!(normalized("  Bitte  PRÜFEN\n"), "bitte prüfen");
    }
    #[test]
    fn learning_indexes_cover_retention_and_context() {
        let conn = fixture();
        for (query,index) in [
            ("SELECT id FROM crew_member_learnings WHERE member_id='crew-milo' ORDER BY confirmed_by_owner,created_at,id LIMIT 1","idx_crew_learning_retention"),
            ("SELECT id FROM crew_member_learnings WHERE member_id='crew-milo' AND archived=0 ORDER BY confirmed_by_owner DESC,created_at DESC,id LIMIT 10","idx_crew_learning_context"),
            ("SELECT member_id FROM crew_attempts WHERE thread_key='thread' ORDER BY selected_at DESC,attempt_id DESC LIMIT 1","idx_crew_attempt_thread"),
            ("SELECT member_id,module,thread_key,succeeded,finalized_at FROM crew_attempts WHERE finalized_at IS NOT NULL ORDER BY finalized_at DESC,attempt_id DESC LIMIT 1000","idx_crew_attempt_finished"),
            ("SELECT succeeded FROM crew_attempts WHERE member_id='crew-milo' AND finalized_at IS NOT NULL ORDER BY finalized_at DESC,attempt_id DESC LIMIT 1","idx_crew_attempt_member_finished"),
            ("SELECT message_key FROM communication_routing_state WHERE crew_member_id='crew-milo' AND route_status='leased' ORDER BY leased_at,message_key LIMIT 1","idx_crew_active_task")
        ] {
            let plan=conn.prepare(&format!("EXPLAIN QUERY PLAN {query}")).unwrap().query_map([],|r|r.get::<_,String>(3)).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap().join("\n");
            assert!(plan.contains(index),"{plan}"); assert!(!plan.contains("TEMP B-TREE"),"{plan}");
        }
    }
}
