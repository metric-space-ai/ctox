use crate::skills::model::SkillMetadata;
use ctox_protocol::protocol::SKILLS_INSTRUCTIONS_CLOSE_TAG;
use ctox_protocol::protocol::SKILLS_INSTRUCTIONS_OPEN_TAG;
use ctox_protocol::protocol::SkillScope;

pub fn render_skills_section(skills: &[SkillMetadata]) -> Option<String> {
    if skills.is_empty() {
        return None;
    }

    let mut lines: Vec<String> = Vec::new();
    lines.push("## Skills".to_string());
    lines.push("A skill is a set of local instructions to follow. System skills are managed by CTOX and loaded from the internal SQLite skill store; user, repo, admin, and plugin skills are loaded from local `SKILL.md` files. Below is the list of skills that can be used.".to_string());
    lines.push("### Available skills".to_string());

    for skill in skills {
        let source_label = if skill.scope == SkillScope::System {
            "system-store"
        } else {
            "file"
        };
        let path_str = skill.path_to_skills_md.to_string_lossy().replace('\\', "/");
        let name = skill.name.as_str();
        let description = skill.description.as_str();
        lines.push(format!(
            "- {name}: {description} ({source_label}: {path_str})"
        ));
    }

    lines.push("### How to use skills".to_string());
    lines.push(
        r###"- Discovery: The list above is the skills available in this session (name + description + source reference). File skill bodies live on disk at the listed paths. System skill bodies live in the CTOX managed skill store.
- Trigger rules: If the user names a skill (with `$SkillName` or plain text) OR the task clearly matches a skill's description shown above, you must use that skill for that turn. Multiple mentions mean use them all. Do not carry skills across turns unless re-mentioned.
- Missing/blocked: If a named skill isn't in the list or the path can't be read, say so briefly and continue with the best fallback.
- How to use a skill (progressive disclosure):
  1) After deciding to use a skill, read its injected instructions. Read only enough to follow the workflow.
  2) For file skills, when `SKILL.md` references relative paths (e.g., `scripts/foo.py`), resolve them relative to the skill directory listed above first, and only consider other paths if needed.
  3) For system skills, use CTOX CLI/API commands referenced by the skill. Do not assume arbitrary bundled scripts are available on disk.
  4) If a file skill points to extra folders such as `references/`, load only the specific files needed for the request; don't bulk-load everything.
  5) If a file skill has `scripts/`, prefer running or patching them instead of retyping large code blocks.
  6) If a file skill has `assets/` or templates, reuse them instead of recreating from scratch.
- Coordination and sequencing:
  - If multiple skills apply, choose the minimal set that covers the request and state the order you'll use them.
  - Announce which skill(s) you're using and why (one short line). If you skip an obvious skill, say why.
- Context hygiene:
  - Keep context small: summarize long sections instead of pasting them; only load extra files when needed.
  - Avoid deep reference-chasing: prefer opening only files directly linked from `SKILL.md` unless you're blocked.
  - When variants exist (frameworks, providers, domains), pick only the relevant reference file(s) and note that choice.
- Safety and fallback: If a skill can't be applied cleanly (missing files, unclear instructions), state the issue, pick the next-best approach, and continue."###
            .to_string(),
    );

    let body = lines.join("\n");
    Some(format!(
        "{SKILLS_INSTRUCTIONS_OPEN_TAG}\n{body}\n{SKILLS_INSTRUCTIONS_CLOSE_TAG}"
    ))
}
