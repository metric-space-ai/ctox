use anyhow::Context;
use anyhow::Result;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct FollowUpDecision {
    pub status: String,
    pub rationale: String,
    pub follow_up_title: Option<String>,
    pub follow_up_prompt: Option<String>,
    pub suggested_skill: Option<String>,
    pub suggested_thread_key: Option<String>,
    pub owner_communication_recommended: bool,
    pub owner_note: Option<String>,
}

#[derive(Debug, Clone)]
pub struct FollowUpRequest {
    pub goal: String,
    pub result: String,
    pub step_title: Option<String>,
    pub suggested_skill: Option<String>,
    pub thread_key: Option<String>,
    pub blocker: Option<String>,
    pub open_items: Vec<String>,
    pub requirements_changed: bool,
    pub owner_visible: bool,
    pub review_required: bool,
    pub review_verdict: Option<String>,
    pub review_summary: Option<String>,
}

pub fn handle_follow_up_command(args: &[String]) -> Result<()> {
    let command = args.first().map(String::as_str).unwrap_or("");
    match command {
        "evaluate" => {
            let request = parse_evaluate_request(args)?;
            let decision = evaluate_follow_up(request);
            println!("{}", serde_json::to_string_pretty(&decision)?);
            Ok(())
        }
        _ => anyhow::bail!(
            "usage:\n  ctox follow-up evaluate --goal <text> --result <text> [--step-title <text>] [--skill <name>] [--thread-key <key>] [--blocker <text>] [--open-item <text>]... [--requirements-changed] [--owner-visible] [--review-required] [--review-verdict <pass|fail|partial|unavailable>] [--review-summary <text>]"
        ),
    }
}

fn evaluate_follow_up(request: FollowUpRequest) -> FollowUpDecision {
    if request.requirements_changed {
        return FollowUpDecision {
            status: "needs_replan".to_string(),
            rationale: "New or changed requirements superseded the current execution path."
                .to_string(),
            follow_up_title: None,
            follow_up_prompt: None,
            suggested_skill: request.suggested_skill,
            suggested_thread_key: request.thread_key,
            owner_communication_recommended: request.owner_visible,
            owner_note: owner_note(
                request.owner_visible,
                "The original plan needs to be refreshed because the requirements changed.",
            ),
        };
    }

    if let Some(blocker) = request
        .blocker
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
    {
        let blocked_on_user = blocker_mentions_user(&blocker);
        return FollowUpDecision {
            status: if blocked_on_user {
                "blocked_on_user".to_string()
            } else {
                "blocked_on_external".to_string()
            },
            rationale: format!("The work cannot proceed until this blocker is resolved: {blocker}"),
            follow_up_title: None,
            follow_up_prompt: None,
            suggested_skill: request.suggested_skill,
            suggested_thread_key: request.thread_key,
            owner_communication_recommended: request.owner_visible || blocked_on_user,
            owner_note: owner_note(
                request.owner_visible || blocked_on_user,
                &render_owner_blocker_note(&blocker),
            ),
        };
    }

    let mut open_items = request.open_items.clone();
    if let Some(review_item) = review_gate_open_item(
        request.review_required,
        request.review_verdict.as_deref(),
        request.review_summary.as_deref(),
    ) {
        open_items.push(review_item);
    }

    if !open_items.is_empty() {
        let title = if let Some(step_title) = request
            .step_title
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            format!("Follow up after {}", step_title)
        } else {
            format!("Continue {}", clip_text(&request.goal, 72))
        };
        let prompt = render_follow_up_prompt(&request.goal, &request.result, &open_items);
        return FollowUpDecision {
            status: "needs_followup".to_string(),
            rationale: "The latest turn produced useful progress but did not fully close the goal."
                .to_string(),
            follow_up_title: Some(title),
            follow_up_prompt: Some(prompt),
            suggested_skill: request.suggested_skill,
            suggested_thread_key: request.thread_key,
            owner_communication_recommended: request.owner_visible
                && !review_verdict_passed(request.review_verdict.as_deref()),
            owner_note: owner_note(
                request.owner_visible && !review_verdict_passed(request.review_verdict.as_deref()),
                review_summary_note(request.review_summary.as_deref()),
            ),
        };
    }

    FollowUpDecision {
        status: "done".to_string(),
        rationale: "The current turn appears to have completed the active scope without an explicit remaining slice.".to_string(),
        follow_up_title: None,
        follow_up_prompt: None,
        suggested_skill: request.suggested_skill,
        suggested_thread_key: request.thread_key,
        owner_communication_recommended: false,
        owner_note: None,
    }
}

fn render_follow_up_prompt(goal: &str, result: &str, open_items: &[String]) -> String {
    let mut lines = vec![
        "Continue the broader goal using the latest completed turn as the starting point."
            .to_string(),
        String::new(),
        "Goal:".to_string(),
        goal.trim().to_string(),
        String::new(),
        "Latest concrete result:".to_string(),
        clip_text(result, 700),
        String::new(),
        "Open follow-up items:".to_string(),
    ];
    for item in open_items {
        lines.push(format!("- {}", item.trim()));
    }
    lines.push(String::new());
    lines.push(
        "Before acting, revalidate the current repo/runtime state and replan if the situation changed."
            .to_string(),
    );
    lines.push(
        "If work still remains after this turn, keep exactly one open follow-up item in CTOX runtime state. A sentence in the reply does not count as open work."
            .to_string(),
    );
    lines.join("\n")
}

fn parse_evaluate_request(args: &[String]) -> Result<FollowUpRequest> {
    Ok(FollowUpRequest {
        goal: required_flag_value(args, "--goal")
            .context("usage: ctox follow-up evaluate --goal <text> --result <text>")?
            .to_string(),
        result: required_flag_value(args, "--result")
            .context("usage: ctox follow-up evaluate --goal <text> --result <text>")?
            .to_string(),
        step_title: find_flag_value(args, "--step-title").map(ToOwned::to_owned),
        suggested_skill: find_flag_value(args, "--skill").map(ToOwned::to_owned),
        thread_key: find_flag_value(args, "--thread-key").map(ToOwned::to_owned),
        blocker: find_flag_value(args, "--blocker").map(ToOwned::to_owned),
        open_items: find_all_flag_values(args, "--open-item"),
        requirements_changed: args.iter().any(|arg| arg == "--requirements-changed"),
        owner_visible: args.iter().any(|arg| arg == "--owner-visible"),
        review_required: args.iter().any(|arg| arg == "--review-required"),
        review_verdict: find_flag_value(args, "--review-verdict").map(ToOwned::to_owned),
        review_summary: find_flag_value(args, "--review-summary").map(ToOwned::to_owned),
    })
}

fn required_flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    find_flag_value(args, flag)
}

fn find_flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let index = args.iter().position(|arg| arg == flag)?;
    args.get(index + 1).map(String::as_str)
}

fn find_all_flag_values(args: &[String], flag: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut index = 0usize;
    while index < args.len() {
        if args[index] == flag {
            if let Some(value) = args.get(index + 1) {
                values.push(value.clone());
                index += 2;
                continue;
            }
        }
        index += 1;
    }
    values
}

fn blocker_mentions_user(value: &str) -> bool {
    let lowered = value.to_lowercase();
    [
        "user",
        "owner",
        "approval",
        "clarify",
        "credentials",
        "decision",
    ]
    .iter()
    .any(|needle| lowered.contains(needle))
}

fn review_gate_open_item(
    review_required: bool,
    review_verdict: Option<&str>,
    review_summary: Option<&str>,
) -> Option<String> {
    if !review_required || review_verdict_passed(review_verdict) {
        return None;
    }
    let summary = review_summary
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| clip_text(value, 180));
    let prefix = match normalize_review_verdict(review_verdict).as_deref() {
        Some("fail") => "Resolve review findings before treating the slice as done",
        Some("partial") => "Complete the missing verification before treating the slice as done",
        Some("unavailable") => "Re-run completion review before treating the slice as done",
        _ => "Run or complete the required completion review before treating the slice as done",
    };
    Some(match summary {
        Some(summary) => format!("{prefix}: {summary}"),
        None => prefix.to_string(),
    })
}

fn review_verdict_passed(value: Option<&str>) -> bool {
    matches!(normalize_review_verdict(value).as_deref(), Some("pass"))
}

fn normalize_review_verdict(value: Option<&str>) -> Option<String> {
    let normalized = value?.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "pass" | "passed" | "ok" => Some("pass".to_string()),
        "fail" | "failed" => Some("fail".to_string()),
        "partial" => Some("partial".to_string()),
        "unavailable" | "missing" => Some("unavailable".to_string()),
        _ => None,
    }
}

fn owner_note(enabled: bool, note: &str) -> Option<String> {
    enabled.then(|| note.trim().to_string())
}

fn review_summary_note(summary: Option<&str>) -> &'static str {
    let _ = summary;
    "The owner-visible mission is not actually done yet. Reconstruct the latest thread context, explain the blocker or quality gap plainly, and ask for approval or feedback only when a real external decision is needed."
}

fn render_owner_blocker_note(blocker: &str) -> String {
    let requested_inputs = extract_requested_inputs(blocker);
    if requested_inputs.is_empty() {
        return format!(
            "Work is blocked: {blocker}\nReply in TUI or answer the current owner message with the exact missing input or approval needed."
        );
    }
    format!(
        "Work is blocked: {blocker}\nProvide exactly these inputs: {}.\nReply in TUI or answer the current owner message with those exact values.",
        requested_inputs.join(", ")
    )
}

fn extract_requested_inputs(text: &str) -> Vec<String> {
    let mut values = Vec::new();
    for token in text.split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')) {
        let trimmed = token.trim_matches(|ch: char| ch == ':' || ch == ',' || ch == '.');
        if trimmed.is_empty() {
            continue;
        }
        let looks_like_env_key = trimmed.contains('_')
            && trimmed.chars().any(|ch| ch.is_ascii_uppercase())
            && trimmed
                .chars()
                .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '_');
        if looks_like_env_key {
            push_unique(&mut values, trimmed.to_string());
            continue;
        }
        let lowered = trimmed.to_lowercase();
        if matches!(
            lowered.as_str(),
            "username"
                | "user"
                | "password"
                | "url"
                | "token"
                | "secret"
                | "apikey"
                | "api-key"
                | "email"
                | "host"
                | "port"
        ) {
            push_unique(&mut values, trimmed.to_string());
        }
    }
    values
}

fn push_unique(values: &mut Vec<String>, candidate: String) {
    if !values
        .iter()
        .any(|existing| existing.eq_ignore_ascii_case(&candidate))
    {
        values.push(candidate);
    }
}

fn clip_text(value: &str, max_chars: usize) -> String {
    let collapsed = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= max_chars {
        return collapsed;
    }
    let mut clipped = collapsed
        .chars()
        .take(max_chars.saturating_sub(1))
        .collect::<String>();
    clipped.push('…');
    clipped
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evaluates_done_when_no_open_signals_exist() {
        let decision = evaluate_follow_up(FollowUpRequest {
            goal: "Ship the fix".to_string(),
            result: "Patched the deploy script and verified the smoke test.".to_string(),
            step_title: Some("verify rollout".to_string()),
            suggested_skill: None,
            thread_key: None,
            blocker: None,
            open_items: Vec::new(),
            requirements_changed: false,
            owner_visible: false,
            review_required: false,
            review_verdict: None,
            review_summary: None,
        });
        assert_eq!(decision.status, "done");
    }

    #[test]
    fn evaluates_followup_when_open_items_exist() {
        let decision = evaluate_follow_up(FollowUpRequest {
            goal: "Stabilize rollout".to_string(),
            result: "Updated the script but the remote smoke check is still pending.".to_string(),
            step_title: Some("update script".to_string()),
            suggested_skill: Some("playwright".to_string()),
            thread_key: Some("plan/test".to_string()),
            blocker: None,
            open_items: vec!["Run the remote smoke check".to_string()],
            requirements_changed: false,
            owner_visible: false,
            review_required: false,
            review_verdict: None,
            review_summary: None,
        });
        assert_eq!(decision.status, "needs_followup");
        assert!(decision
            .follow_up_prompt
            .unwrap()
            .contains("Run the remote smoke check"));
    }

    #[test]
    fn evaluates_blocked_on_user() {
        let decision = evaluate_follow_up(FollowUpRequest {
            goal: "Finish rollout".to_string(),
            result: "Reached the approval gate.".to_string(),
            step_title: None,
            suggested_skill: None,
            thread_key: None,
            blocker: Some("Need owner approval before production deploy".to_string()),
            open_items: Vec::new(),
            requirements_changed: false,
            owner_visible: true,
            review_required: false,
            review_verdict: None,
            review_summary: None,
        });
        assert_eq!(decision.status, "blocked_on_user");
        assert!(decision.owner_communication_recommended);
    }

    #[test]
    fn result_text_alone_does_not_force_follow_up() {
        let decision = evaluate_follow_up(FollowUpRequest {
            goal: "Installiere Nextcloud".to_string(),
            result: "Die Vorarbeiten sind erledigt. Nextcloud folgt als nächster Schritt."
                .to_string(),
            step_title: None,
            suggested_skill: Some("change-lifecycle".to_string()),
            thread_key: Some("email/thread".to_string()),
            blocker: None,
            open_items: Vec::new(),
            requirements_changed: false,
            owner_visible: true,
            review_required: false,
            review_verdict: None,
            review_summary: None,
        });
        assert_eq!(decision.status, "done");
    }

    #[test]
    fn explicit_blocker_controls_blocked_status() {
        let decision = evaluate_follow_up(FollowUpRequest {
            goal: "Installiere Nextcloud".to_string(),
            result: "Blocked: NEXTCLOUD_URL, username, and password are missing, so the rollout cannot finish safely.".to_string(),
            step_title: None,
            suggested_skill: Some("change-lifecycle".to_string()),
            thread_key: Some("email/thread".to_string()),
            blocker: Some(
                "NEXTCLOUD_URL, username, and password are missing, so the rollout cannot finish safely."
                    .to_string(),
            ),
            open_items: Vec::new(),
            requirements_changed: false,
            owner_visible: true,
            review_required: false,
            review_verdict: None,
            review_summary: None,
        });
        assert_eq!(decision.status, "blocked_on_user");
        assert!(decision.owner_communication_recommended);
    }

    #[test]
    fn blocked_owner_note_lists_requested_inputs() {
        let decision = evaluate_follow_up(FollowUpRequest {
            goal: "Installiere Nextcloud".to_string(),
            result: "Blocked: NEXTCLOUD_URL, username, and password are missing, so the rollout cannot finish safely.".to_string(),
            step_title: None,
            suggested_skill: Some("change-lifecycle".to_string()),
            thread_key: Some("email/thread".to_string()),
            blocker: Some(
                "NEXTCLOUD_URL, username, and password are missing, so the rollout cannot finish safely."
                    .to_string(),
            ),
            open_items: Vec::new(),
            requirements_changed: false,
            owner_visible: true,
            review_required: false,
            review_verdict: None,
            review_summary: None,
        });
        let note = decision.owner_note.expect("owner note");
        assert!(note.contains("NEXTCLOUD_URL"));
        assert!(note.contains("username"));
        assert!(note.contains("password"));
        assert!(note.contains("Reply in TUI or answer the current owner message"));
    }

    #[test]
    fn review_required_without_pass_keeps_slice_open() {
        let decision = evaluate_follow_up(FollowUpRequest {
            goal: "Deploy the service".to_string(),
            result: "Deployment completed successfully.".to_string(),
            step_title: Some("deploy".to_string()),
            suggested_skill: None,
            thread_key: None,
            blocker: None,
            open_items: Vec::new(),
            requirements_changed: false,
            owner_visible: false,
            review_required: true,
            review_verdict: Some("partial".to_string()),
            review_summary: Some(
                "HTTP health check was not exercised from the live service.".to_string(),
            ),
        });
        assert_eq!(decision.status, "needs_followup");
        assert!(decision
            .follow_up_prompt
            .expect("follow-up prompt")
            .contains("HTTP health check"));
    }

    #[test]
    fn review_pass_allows_done() {
        let decision = evaluate_follow_up(FollowUpRequest {
            goal: "Deploy the service".to_string(),
            result: "Deployment completed successfully.".to_string(),
            step_title: Some("deploy".to_string()),
            suggested_skill: None,
            thread_key: None,
            blocker: None,
            open_items: Vec::new(),
            requirements_changed: false,
            owner_visible: false,
            review_required: true,
            review_verdict: Some("pass".to_string()),
            review_summary: Some("Live health check and smoke test both passed.".to_string()),
        });
        assert_eq!(decision.status, "done");
    }
}
