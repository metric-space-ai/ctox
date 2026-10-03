//! The router decides which member's knowledge fits a task. Like an expert
//! router in a mixture of experts: it judges the task against every member's
//! experience (memory, recent attempts, persona) with one bounded, tool-free
//! model call, and spreads work across the pool while nobody has experience.
//! The deterministic score stays as the fallback when no judge is available.
use super::*;
use crate::crew::memory::{anchor_lines, narrative_lines, MemberMemory, RecentAttempt};
use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

pub(crate) const ROUTER_TIMEOUT_SECS: u64 = 60;
const ROUTER_INSTRUCTIONS: &str = "Du bist der Router der CTOX-Crew. Du benutzt keine Werkzeuge. Du antwortest ausschließlich mit einem JSON-Objekt, ohne Erklärung davor oder danach.";

pub(crate) struct RouterCandidate<'a> {
    pub member: &'a Member,
    pub memory: &'a MemberMemory,
    pub recent: &'a [RecentAttempt],
    pub failures_24h: usize,
}

pub(crate) struct RouterTask<'a> {
    pub summary: &'a str,
    pub module: Option<&'a str>,
    pub command_type: Option<&'a str>,
    pub skills: &'a [String],
    pub thread_key: Option<&'a str>,
}

/// One judgment, one reply. Implementations are the model (production) or a
/// scripted reply (tests); `None` means no judge and the deterministic path.
pub(crate) trait RouterJudge {
    fn judge(&self, prompt: &str) -> Result<String>;
}

pub(crate) struct ModelRouterJudge<'a> {
    pub root: &'a Path,
    pub settings: &'a BTreeMap<String, String>,
}

impl RouterJudge for ModelRouterJudge<'_> {
    fn judge(&self, prompt: &str) -> Result<String> {
        let mut session =
            crate::execution::agent::turn_loop::PersistentSession::start_with_instructions(
                self.root,
                self.settings,
                Some(ROUTER_INSTRUCTIONS),
                true,
            )?;
        session.run_turn(
            prompt,
            Some(Duration::from_secs(ROUTER_TIMEOUT_SECS)),
            None,
            Some(false),
            0,
        )
    }
}

fn clip(text: &str, max_chars: usize) -> String {
    let text = prose_line(text);
    if text.chars().count() <= max_chars {
        return text;
    }
    let mut out = text.chars().take(max_chars).collect::<String>();
    out.push('…');
    out
}

pub(crate) fn render_router_prompt(
    task: &RouterTask<'_>,
    candidates: &[RouterCandidate<'_>],
) -> String {
    let mut prompt = String::new();
    prompt.push_str(
        "Ein Task der CTOX-Crew steht an. Entscheide, welches Mitglied ihn übernimmt.\n\n",
    );
    prompt.push_str("Regeln, in dieser Reihenfolge:\n");
    prompt.push_str("1. Wähle das Mitglied, dessen bisherige Einsätze und dessen Wissen dieser Aufgabe am ähnlichsten sind und gut ausgingen: gleiche Tätigkeit, gleiches Modul, gleicher Auftragstyp, gleiche Domäne. Ähnlichkeit zählt mehr als Persona.\n");
    prompt.push_str("2. Hat kein Mitglied verwandte Erfahrung, wähle das Mitglied mit den wenigsten Einsätzen, bei Gleichstand das am längsten inaktive. So verteilt sich Wissen im Pool, und später kommen für ähnliche Aufgaben dieselben Mitglieder zum Einsatz.\n");
    prompt.push_str("3. Ein Mitglied, das in dieser Tätigkeit zuletzt wiederholt gescheitert ist, nur wählen, wenn es keine Alternative gibt.\n");
    prompt.push_str("4. Nenne in der Begründung die konkrete Erfahrung oder den Verteilungsgrund, in einem Satz auf Deutsch.\n\n");
    prompt.push_str("AUFGABE\n");
    prompt.push_str(&format!("Zusammenfassung: {}\n", clip(task.summary, 400)));
    if let Some(module) = task.module.filter(|m| !m.is_empty()) {
        prompt.push_str(&format!("Modul: {module}\n"));
    }
    if let Some(command) = task.command_type.filter(|c| !c.is_empty()) {
        prompt.push_str(&format!("Auftragstyp: {command}\n"));
    }
    if !task.skills.is_empty() {
        prompt.push_str(&format!("Skill: {}\n", task.skills.join(", ")));
    }
    if task.thread_key.is_some() {
        prompt.push_str("Teil eines laufenden Gesprächs: ja\n");
    }
    prompt.push_str("\nMITGLIEDER\n");
    for candidate in candidates {
        let member = candidate.member;
        prompt.push_str(&format!(
            "\n[{}] {} · {} Einsätze ({} gelungen, {} gescheitert, {} Reviews bestanden){}\n",
            member.id,
            member.name,
            member.stats.tasks_total,
            member.stats.succeeded,
            member.stats.failed,
            member.stats.review_passed,
            member
                .stats
                .last_active_at
                .as_deref()
                .map(|at| format!(", zuletzt aktiv {}", &at[..at.len().min(10)]))
                .unwrap_or_else(|| ", noch nie im Einsatz".to_string())
        ));
        let sketch = prose_line(&member.soul.sketch);
        if !sketch.is_empty() {
            prompt.push_str(&format!("  Charakter: {}\n", clip(&sketch, 160)));
        }
        if candidate.failures_24h > 0 {
            prompt.push_str(&format!(
                "  Fehlschläge in diesem Modul in den letzten 24 h: {}\n",
                candidate.failures_24h
            ));
        }
        let recent = candidate.recent.iter().take(6).collect::<Vec<_>>();
        if !recent.is_empty() {
            prompt.push_str("  Letzte Einsätze:\n");
            for attempt in recent {
                prompt.push_str(&format!(
                    "  - {} → {}\n",
                    clip(&attempt.task_summary, 140),
                    match (attempt.succeeded, attempt.review_passed) {
                        (true, Some(true)) => "gelungen, Review bestanden",
                        (true, _) => "gelungen",
                        (false, _) => "gescheitert",
                    }
                ));
            }
        }
        let anchors = anchor_lines(&candidate.memory.anchors);
        if !anchors.is_empty() {
            prompt.push_str("  Wissen:\n");
            for line in anchors.iter().take(8) {
                prompt.push_str(&format!("  - {}\n", clip(line, 160)));
            }
        }
        let narrative = narrative_lines(&candidate.memory.narrative);
        if !narrative.is_empty() {
            prompt.push_str("  Erfahrung:\n");
            for line in narrative.iter().rev().take(4) {
                prompt.push_str(&format!("  - {}\n", clip(line, 160)));
            }
        }
    }
    prompt.push_str("\nAntworte nur mit: {\"member_id\":\"<id aus den eckigen Klammern>\",\"reason\":\"<ein Satz>\"}\n");
    prompt
}

/// Accepts the JSON object anywhere in the reply; the member must be one of
/// the candidates. Anything else is a fallback, never a guess.
pub(crate) fn parse_router_reply(reply: &str, candidates: &[Member]) -> Option<Selection> {
    let start = reply.find('{')?;
    let end = reply.rfind('}')?;
    if end <= start {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(&reply[start..=end]).ok()?;
    let member_id = value.get("member_id")?.as_str()?.trim();
    let member = candidates
        .iter()
        .find(|m| m.id == member_id && !m.archived)?;
    let reason = value
        .get("reason")
        .and_then(serde_json::Value::as_str)
        .map(prose_line)
        .filter(|r| !r.is_empty() && safe_prose(r, 300))
        .unwrap_or_else(|| "Erfahrung passt zur Aufgabe".to_string());
    Some(Selection {
        member_id: member.id.clone(),
        reason: format!("routed: {} ({}): {}", member.name, member.id, reason),
    })
}

/// Manual assignment and thread continuity stay ahead of every judgment.
/// With one candidate there is nothing to decide. Without a judge, or when
/// the judge fails, the deterministic score decides and the reason says so.
pub(crate) fn route(
    candidates: &[Member],
    task: &TaskTraits,
    history: &[History],
    router_task: &RouterTask<'_>,
    router_candidates: &[RouterCandidate<'_>],
    judge: Option<&dyn RouterJudge>,
    now_ms: i64,
) -> Option<Selection> {
    let fallback = select(candidates, task, history, now_ms)?;
    if fallback.reason.starts_with("assigned:") || fallback.reason.starts_with("continuity:") {
        return Some(fallback);
    }
    let active = candidates.iter().filter(|m| !m.archived).count();
    let Some(judge) = judge else {
        return Some(fallback);
    };
    if active < 2 {
        return Some(fallback);
    }
    let prompt = render_router_prompt(router_task, router_candidates);
    match judge.judge(&prompt) {
        Ok(reply) => match parse_router_reply(&reply, candidates) {
            Some(selection) => Some(selection),
            None => Some(Selection {
                member_id: fallback.member_id,
                reason: format!(
                    "{} · Router-Antwort unbrauchbar, Punktzahl entschied",
                    fallback.reason
                ),
            }),
        },
        Err(error) => Some(Selection {
            member_id: fallback.member_id,
            reason: format!(
                "{} · Router nicht erreichbar ({}), Punktzahl entschied",
                fallback.reason,
                clip(&error.to_string(), 80)
            ),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Scripted(&'static str);
    impl RouterJudge for Scripted {
        fn judge(&self, _prompt: &str) -> Result<String> {
            Ok(self.0.to_string())
        }
    }
    struct Broken;
    impl RouterJudge for Broken {
        fn judge(&self, _prompt: &str) -> Result<String> {
            anyhow::bail!("provider offline")
        }
    }

    fn pool() -> Vec<Member> {
        let conn = super::super::tests::fixture();
        members(&conn).unwrap()
    }

    #[test]
    fn router_prompt_names_experience_and_rules() {
        let pool = pool();
        let memory = MemberMemory {
            anchors: "## Entries\n- anchor_id: a1\n- anchor_type: owner_confirmed\n- statement: Vor dem Import das Schema prüfen.\n".into(),
            narrative: String::new(),
            updated_at: String::new(),
        };
        let recent = vec![RecentAttempt {
            task_summary: "[reports] Kundenliste importieren".into(),
            module: Some("reports".into()),
            succeeded: true,
            review_passed: Some(true),
            finished_at: "2026-09-05T10:00:00Z".into(),
        }];
        let candidates = pool
            .iter()
            .map(|member| RouterCandidate {
                member,
                memory: &memory,
                recent: &recent,
                failures_24h: 0,
            })
            .collect::<Vec<_>>();
        let task = RouterTask {
            summary: "[reports] Neue Kundenliste importieren",
            module: Some("reports"),
            command_type: None,
            skills: &[],
            thread_key: None,
        };
        let prompt = render_router_prompt(&task, &candidates);
        assert!(prompt.contains("Vor dem Import das Schema prüfen."));
        assert!(prompt.contains("Kundenliste importieren → gelungen, Review bestanden"));
        assert!(prompt.contains(&format!("[{}] {}", pool[0].id, pool[0].name)));
        assert!(prompt.contains("wenigsten Einsätzen"));
    }

    #[test]
    fn route_prefers_judgment_and_falls_back_honestly() {
        let pool = pool();
        assert!(pool.len() >= 2, "fixture needs a pool");
        let task = TaskTraits::default();
        let router_task = RouterTask {
            summary: "Aufgabe",
            module: None,
            command_type: None,
            skills: &[],
            thread_key: None,
        };
        let memory = MemberMemory::default();
        let candidates = pool
            .iter()
            .map(|member| RouterCandidate {
                member,
                memory: &memory,
                recent: &[],
                failures_24h: 0,
            })
            .collect::<Vec<_>>();
        let chosen = &pool[1];
        let reply = format!(
            "Hier: {{\"member_id\":\"{}\",\"reason\":\"hat das schon dreimal gemacht\"}}",
            chosen.id
        );
        let judged = route(
            &pool,
            &task,
            &[],
            &router_task,
            &candidates,
            Some(&Scripted(Box::leak(reply.into_boxed_str()))),
            0,
        )
        .unwrap();
        assert_eq!(judged.member_id, chosen.id);
        assert!(judged.reason.starts_with("routed:"));
        assert!(judged.reason.contains("dreimal"));
        let garbage = route(
            &pool,
            &task,
            &[],
            &router_task,
            &candidates,
            Some(&Scripted("kein json")),
            0,
        )
        .unwrap();
        assert!(garbage.reason.contains("Router-Antwort unbrauchbar"));
        let broken = route(
            &pool,
            &task,
            &[],
            &router_task,
            &candidates,
            Some(&Broken),
            0,
        )
        .unwrap();
        assert!(broken.reason.contains("Router nicht erreichbar"));
        let unknown = route(
            &pool,
            &task,
            &[],
            &router_task,
            &candidates,
            Some(&Scripted(
                "{\"member_id\":\"crew:nobody\",\"reason\":\"x\"}",
            )),
            0,
        )
        .unwrap();
        assert!(unknown.reason.contains("unbrauchbar"));
        let manual = TaskTraits {
            manual_member: Some(chosen.id.clone()),
            ..Default::default()
        };
        let pinned = route(
            &pool,
            &manual,
            &[],
            &router_task,
            &candidates,
            Some(&Broken),
            0,
        )
        .unwrap();
        assert!(pinned.reason.starts_with("assigned:"));
        assert_eq!(pinned.member_id, chosen.id);
    }
}
