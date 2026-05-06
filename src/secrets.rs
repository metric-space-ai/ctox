use anyhow::Context;
use anyhow::Result;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine;
use regex::Regex;
use ring::aead;
use ring::rand::{SecureRandom, SystemRandom};
use rusqlite::params;
use rusqlite::Connection;
use rusqlite::OptionalExtension;
use serde::Serialize;
use serde_json::json;
use serde_json::Value;
use sha2::Digest;
use sha2::Sha256;
use std::fs;
use std::path::{Path, PathBuf};
use zeroize::Zeroize;
use zeroize::Zeroizing;

use crate::lcm;
use crate::persistence;

const MASTER_KEY_STORAGE_KEY: &str = "secret_master_key_b64";

type SecretMaterial = Zeroizing<Vec<u8>>;

#[derive(Debug, Clone, Serialize)]
pub struct SecretRecordView {
    pub secret_id: String,
    pub scope: String,
    pub secret_name: String,
    pub description: Option<String>,
    pub metadata: Value,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
struct SecretIntakeView {
    pub secret: SecretRecordView,
    pub rewrite: Option<lcm::SecretRewriteResult>,
}

#[derive(Debug, Clone)]
pub struct PromptSecretSanitization {
    pub sanitized_prompt: String,
    pub auto_ingested_secrets: usize,
}

pub fn handle_secret_command(root: &Path, args: &[String]) -> Result<()> {
    let command = args.first().map(String::as_str).unwrap_or("");
    match command {
        "init" => {
            let conn = open_secret_db(root)?;
            ensure_secret_schema(&conn)?;
            let key_source = ensure_secret_master_key(root)?.1;
            print_json(
                &json!({"ok": true, "db_path": resolve_db_path(root), "key_source": key_source}),
            )
        }
        "put" => {
            let scope = required_flag_value(args, "--scope")
                .context("usage: ctox secret put --scope <scope> --name <name> --value <text> [--description <text>] [--metadata-json <json>]")?;
            let name = required_flag_value(args, "--name")
                .context("usage: ctox secret put --scope <scope> --name <name> --value <text> [--description <text>] [--metadata-json <json>]")?;
            let value = required_flag_value(args, "--value")
                .context("usage: ctox secret put --scope <scope> --name <name> --value <text> [--description <text>] [--metadata-json <json>]")?;
            let description = find_flag_value(args, "--description").map(str::to_string);
            let metadata = find_flag_value(args, "--metadata-json")
                .map(parse_json_value)
                .transpose()?
                .unwrap_or_else(|| json!({}));
            let record = put_secret(root, scope, name, value, description, metadata)?;
            print_json(&json!({"ok": true, "secret": record}))
        }
        "intake" => {
            let scope = required_flag_value(args, "--scope")
                .context("usage: ctox secret intake --scope <scope> --name <name> --value <text> [--description <text>] [--metadata-json <json>] [--db <path> --conversation-id <id> --match-text <text> [--label <text>]]")?;
            let name = required_flag_value(args, "--name")
                .context("usage: ctox secret intake --scope <scope> --name <name> --value <text> [--description <text>] [--metadata-json <json>] [--db <path> --conversation-id <id> --match-text <text> [--label <text>]]")?;
            let value = required_flag_value(args, "--value")
                .context("usage: ctox secret intake --scope <scope> --name <name> --value <text> [--description <text>] [--metadata-json <json>] [--db <path> --conversation-id <id> --match-text <text> [--label <text>]]")?;
            let description = find_flag_value(args, "--description").map(str::to_string);
            let metadata = find_flag_value(args, "--metadata-json")
                .map(parse_json_value)
                .transpose()?
                .unwrap_or_else(|| json!({}));
            let rewrite = parse_intake_rewrite_request(args)?;
            let intake = intake_secret(root, scope, name, value, description, metadata, rewrite)?;
            print_json(&json!({"ok": true, "intake": intake}))
        }
        "list" => {
            let scope = find_flag_value(args, "--scope");
            let records = list_secrets(root, scope)?;
            print_json(&json!({"ok": true, "count": records.len(), "secrets": records}))
        }
        "show" => {
            let scope = required_flag_value(args, "--scope")
                .context("usage: ctox secret show --scope <scope> --name <name>")?;
            let name = required_flag_value(args, "--name")
                .context("usage: ctox secret show --scope <scope> --name <name>")?;
            let record = load_secret_record(root, scope, name)?.context("secret not found")?;
            print_json(&json!({"ok": true, "secret": record}))
        }
        "get" => {
            let scope = required_flag_value(args, "--scope")
                .context("usage: ctox secret get --scope <scope> --name <name>")?;
            let name = required_flag_value(args, "--name")
                .context("usage: ctox secret get --scope <scope> --name <name>")?;
            let value = get_secret_value(root, scope, name)?;
            print_json(&json!({"ok": true, "scope": scope, "name": name, "value": value}))
        }
        "delete" => {
            let scope = required_flag_value(args, "--scope")
                .context("usage: ctox secret delete --scope <scope> --name <name>")?;
            let name = required_flag_value(args, "--name")
                .context("usage: ctox secret delete --scope <scope> --name <name>")?;
            delete_secret(root, scope, name)?;
            print_json(&json!({"ok": true, "scope": scope, "name": name, "deleted": true}))
        }
        "memory-rewrite" => {
            let db_path = required_flag_value(args, "--db")
                .context("usage: ctox secret memory-rewrite --db <path> --conversation-id <id> --scope <scope> --name <name> --match-text <text> [--label <text>]")?;
            let conversation_id = required_flag_value(args, "--conversation-id")
                .context("usage: ctox secret memory-rewrite --db <path> --conversation-id <id> --scope <scope> --name <name> --match-text <text> [--label <text>]")?
                .parse::<i64>()
                .context("failed to parse conversation id")?;
            let scope = required_flag_value(args, "--scope")
                .context("usage: ctox secret memory-rewrite --db <path> --conversation-id <id> --scope <scope> --name <name> --match-text <text> [--label <text>]")?;
            let name = required_flag_value(args, "--name")
                .context("usage: ctox secret memory-rewrite --db <path> --conversation-id <id> --scope <scope> --name <name> --match-text <text> [--label <text>]")?;
            let match_text = required_flag_value(args, "--match-text")
                .context("usage: ctox secret memory-rewrite --db <path> --conversation-id <id> --scope <scope> --name <name> --match-text <text> [--label <text>]")?;
            anyhow::ensure!(
                secret_exists(root, scope, name)?,
                "secret {scope}/{name} does not exist in the local secret store"
            );
            let replacement = secret_reference_text(scope, name, find_flag_value(args, "--label"));
            let result = lcm::run_secret_rewrite(
                Path::new(db_path),
                conversation_id,
                scope,
                name,
                match_text,
                &replacement,
            )?;
            print_json(&json!({"ok": true, "rewrite": result}))
        }
        _ => anyhow::bail!(
            "usage:\n  ctox secret init\n  ctox secret put --scope <scope> --name <name> --value <text> [--description <text>] [--metadata-json <json>]\n  ctox secret intake --scope <scope> --name <name> --value <text> [--description <text>] [--metadata-json <json>] [--db <path> --conversation-id <id> --match-text <text> [--label <text>]]\n  ctox secret list [--scope <scope>]\n  ctox secret show --scope <scope> --name <name>\n  ctox secret get --scope <scope> --name <name>\n  ctox secret delete --scope <scope> --name <name>\n  ctox secret memory-rewrite --db <path> --conversation-id <id> --scope <scope> --name <name> --match-text <text> [--label <text>]"
        ),
    }
}

pub fn list_secret_records(root: &Path, scope: Option<&str>) -> Result<Vec<SecretRecordView>> {
    list_secrets(root, scope)
}

pub fn read_secret_value(root: &Path, scope: &str, name: &str) -> Result<String> {
    get_secret_value(root, scope, name)
}

pub fn write_secret_record(
    root: &Path,
    scope: &str,
    name: &str,
    value: &str,
    description: Option<String>,
    metadata: Value,
) -> Result<SecretRecordView> {
    put_secret(root, scope, name, value, description, metadata)
}

#[derive(Debug, Clone)]
struct DetectedPromptSecret {
    scope: String,
    name: String,
    literal: String,
    stored_value: String,
    label: Option<String>,
}

pub fn auto_intake_prompt_secrets(root: &Path, prompt: &str) -> Result<PromptSecretSanitization> {
    let mut sanitized_prompt = prompt.to_string();
    let mut detections = detect_prompt_secrets(prompt);
    detections.sort_by(|left, right| right.literal.len().cmp(&left.literal.len()));

    let mut seen_literals = std::collections::BTreeSet::new();
    let mut auto_ingested_secrets = 0usize;

    for detection in detections {
        if detection.literal.trim().is_empty()
            || detection.stored_value.trim().is_empty()
            || !seen_literals.insert(detection.literal.clone())
        {
            continue;
        }
        put_secret(
            root,
            &detection.scope,
            &detection.name,
            &detection.stored_value,
            Some(format!(
                "Auto-ingested from user-submitted prompt as {}",
                detection.name
            )),
            json!({
                "source": "prompt_auto_intake",
                "detected_name": detection.name,
                "detected_scope": detection.scope,
            }),
        )?;
        let replacement = secret_reference_text(
            &detection.scope,
            &detection.name,
            detection.label.as_deref(),
        );
        sanitized_prompt = sanitized_prompt.replace(&detection.literal, &replacement);
        auto_ingested_secrets += 1;
    }

    Ok(PromptSecretSanitization {
        sanitized_prompt,
        auto_ingested_secrets,
    })
}

#[derive(Debug, Clone)]
struct IntakeRewriteRequest {
    db_path: PathBuf,
    conversation_id: i64,
    match_text: String,
    label: Option<String>,
}

fn parse_intake_rewrite_request(args: &[String]) -> Result<Option<IntakeRewriteRequest>> {
    let db_path = find_flag_value(args, "--db");
    let conversation_id = find_flag_value(args, "--conversation-id");
    let match_text = find_flag_value(args, "--match-text");
    let label = find_flag_value(args, "--label").map(str::to_string);

    if db_path.is_none() && conversation_id.is_none() && match_text.is_none() {
        return Ok(None);
    }

    let db_path = db_path.context(
        "ctox secret intake requires --db together with --conversation-id and --match-text when memory rewrite is requested",
    )?;
    let conversation_id = conversation_id
        .context(
            "ctox secret intake requires --conversation-id together with --db and --match-text when memory rewrite is requested",
        )?
        .parse::<i64>()
        .context("failed to parse conversation id")?;
    let match_text = match_text
        .context(
            "ctox secret intake requires --match-text together with --db and --conversation-id when memory rewrite is requested",
        )?
        .to_string();

    Ok(Some(IntakeRewriteRequest {
        db_path: PathBuf::from(db_path),
        conversation_id,
        match_text,
        label,
    }))
}

fn intake_secret(
    root: &Path,
    scope: &str,
    name: &str,
    value: &str,
    description: Option<String>,
    metadata: Value,
    rewrite: Option<IntakeRewriteRequest>,
) -> Result<SecretIntakeView> {
    let record = put_secret(root, scope, name, value, description, metadata)?;
    let rewrite_result = match rewrite {
        Some(rewrite) => {
            let replacement = secret_reference_text(scope, name, rewrite.label.as_deref());
            Some(lcm::run_secret_rewrite(
                &rewrite.db_path,
                rewrite.conversation_id,
                scope,
                name,
                &rewrite.match_text,
                &replacement,
            )?)
        }
        None => None,
    };
    Ok(SecretIntakeView {
        secret: record,
        rewrite: rewrite_result,
    })
}

fn detect_prompt_secrets(prompt: &str) -> Vec<DetectedPromptSecret> {
    let mut detections = Vec::new();
    let same_line_re = Regex::new(
        r"(?im)^\s*(?:[-*]\s*)?(?P<label>[\p{L}][\p{L}\p{N} _./()%-]{0,80}?)\s*:\s*(?P<value>\S[^\r\n]*)\s*$",
    )
    .expect("static labeled secret regex");
    let heading_re =
        Regex::new(r"(?im)^\s*(?:[-*]\s*)?(?P<label>[\p{L}][\p{L}\p{N} _./()%-]{0,80}?)\s*:\s*$")
            .expect("static heading regex");
    let env_re =
        Regex::new(r#"(?im)^\s*(?P<label>[A-Z][A-Z0-9_]{2,})\s*=\s*(?P<value>\S[^\r\n]*)\s*$"#)
            .expect("static env assignment regex");

    let lines: Vec<&str> = prompt.lines().collect();
    let mut active_heading: Option<String> = None;
    for (index, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if looks_like_database_url(trimmed) {
            continue;
        }
        if let Some(caps) = env_re.captures(line) {
            if let Some(secret) = classify_prompt_secret(
                caps.name("label").map(|m| m.as_str()).unwrap_or_default(),
                caps.name("value").map(|m| m.as_str()).unwrap_or_default(),
                active_heading.as_deref(),
            ) {
                detections.push(secret);
            }
            continue;
        }
        if let Some(caps) = same_line_re.captures(line) {
            if let Some(secret) = classify_prompt_secret(
                caps.name("label").map(|m| m.as_str()).unwrap_or_default(),
                caps.name("value").map(|m| m.as_str()).unwrap_or_default(),
                active_heading.as_deref(),
            ) {
                detections.push(secret);
            }
            continue;
        }
        if let Some(caps) = heading_re.captures(line) {
            let heading = caps
                .name("label")
                .map(|m| m.as_str().trim().to_string())
                .unwrap_or_default();
            active_heading = Some(heading.clone());
            if let Some(next_value) = next_unlabeled_value_line(&lines, index + 1) {
                if let Some(secret) =
                    classify_prompt_secret(&heading, next_value, Some(heading.as_str()))
                {
                    detections.push(secret);
                }
            }
            if heading.to_ascii_lowercase().contains("vercel login") {
                if let Some(email_value) = next_unlabeled_value_line(&lines, index + 1) {
                    let email_trimmed = email_value.trim();
                    if looks_like_email(email_trimmed) {
                        detections.push(DetectedPromptSecret {
                            scope: "captured-input".to_string(),
                            name: "VERCEL_LOGIN_EMAIL".to_string(),
                            literal: email_trimmed.to_string(),
                            stored_value: email_trimmed.to_string(),
                            label: Some("Vercel login email".to_string()),
                        });
                        if let Some(password_value) =
                            next_unlabeled_value_line_after(&lines, index + 1, email_trimmed)
                        {
                            let password_trimmed = password_value.trim();
                            if !password_trimmed.is_empty()
                                && !looks_like_email(password_trimmed)
                                && !line_looks_labeled(password_trimmed)
                            {
                                detections.push(DetectedPromptSecret {
                                    scope: "captured-input".to_string(),
                                    name: "VERCEL_LOGIN_PASSWORD".to_string(),
                                    literal: password_trimmed.to_string(),
                                    stored_value: password_trimmed.to_string(),
                                    label: Some("Vercel login password".to_string()),
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    let standalone_patterns = [
        (
            "credentials",
            "OPENROUTER_API_KEY",
            r"\bsk-or-v1-[A-Za-z0-9]{20,}\b",
        ),
        (
            "credentials",
            "MINIMAX_API_KEY",
            r"\bsk-api-[A-Za-z0-9_-]{20,}\b",
        ),
        (
            "credentials",
            "OPENAI_API_KEY",
            r"\bsk-(?:proj|live|test)-[A-Za-z0-9_-]{20,}\b",
        ),
        (
            "credentials",
            "DATABASE_URL",
            r#"\bpostgres(?:ql)?://[^\s<>"]+"#,
        ),
    ];
    for (scope, name, pattern) in standalone_patterns {
        let regex = Regex::new(pattern).expect("static standalone secret regex");
        for matched in regex.find_iter(prompt) {
            let literal = matched.as_str().trim();
            if literal.contains("[secret-ref:") {
                continue;
            }
            detections.push(DetectedPromptSecret {
                scope: scope.to_string(),
                name: name.to_string(),
                literal: literal.to_string(),
                stored_value: literal.to_string(),
                label: Some(name.replace('_', " ")),
            });
        }
    }

    detections
}

fn classify_prompt_secret(
    label: &str,
    raw_value: &str,
    context_heading: Option<&str>,
) -> Option<DetectedPromptSecret> {
    let stored_value = normalize_secret_value(raw_value)?;
    let label_lower = label.trim().to_ascii_lowercase();
    let heading_lower = context_heading
        .map(|value| value.trim().to_ascii_lowercase())
        .unwrap_or_default();
    let combined = format!("{heading_lower} {label_lower}");

    if combined.contains("openai") && combined.contains("api key")
        || looks_like_openai_key(&stored_value)
    {
        return Some(named_prompt_secret(
            "credentials",
            "OPENAI_API_KEY",
            raw_value,
            &stored_value,
            "OpenAI API key",
        ));
    }
    if combined.contains("openrouter") && combined.contains("api key")
        || looks_like_openrouter_key(&stored_value)
    {
        return Some(named_prompt_secret(
            "credentials",
            "OPENROUTER_API_KEY",
            raw_value,
            &stored_value,
            "OpenRouter API key",
        ));
    }
    if combined.contains("minimax") && combined.contains("api key")
        || looks_like_minimax_key(&stored_value)
    {
        return Some(named_prompt_secret(
            "credentials",
            "MINIMAX_API_KEY",
            raw_value,
            &stored_value,
            "MiniMax API key",
        ));
    }
    if (combined.contains("azure") || combined.contains("foundry")) && combined.contains("token") {
        return Some(named_prompt_secret(
            "credentials",
            "AZURE_FOUNDRY_API_KEY",
            raw_value,
            &stored_value,
            "Azure Foundry token",
        ));
    }
    if combined.contains("database url")
        || combined.contains("neon postgres")
        || combined.contains("postgres zugriff")
        || looks_like_database_url(&stored_value)
    {
        return Some(named_prompt_secret(
            "credentials",
            "DATABASE_URL",
            raw_value,
            &stored_value,
            "database url",
        ));
    }
    if heading_lower.contains("bootstrap mailbox")
        && (label_lower == "password" || label_lower.contains("mailbox password"))
    {
        return Some(named_prompt_secret(
            "credentials",
            "CTO_EMAIL_PASSWORD",
            raw_value,
            &stored_value,
            "bootstrap mailbox password",
        ));
    }
    if heading_lower.contains("host of the cto1 installation") && label_lower.contains("password") {
        return Some(named_prompt_secret(
            "captured-input",
            "VPS_LOGIN_PASSWORD",
            raw_value,
            &stored_value,
            "VPS login password",
        ));
    }
    if heading_lower.contains("host of the cto1 installation")
        && (label_lower.contains("username") || label_lower.contains("benutzername"))
    {
        return Some(named_prompt_secret(
            "captured-input",
            "VPS_LOGIN_USERNAME",
            raw_value,
            &stored_value,
            "VPS login username",
        ));
    }
    if combined.contains("vercel") && combined.contains("password") {
        return Some(named_prompt_secret(
            "captured-input",
            "VERCEL_LOGIN_PASSWORD",
            raw_value,
            &stored_value,
            "Vercel login password",
        ));
    }
    if combined.contains("vercel") && combined.contains("email") && looks_like_email(&stored_value)
    {
        return Some(named_prompt_secret(
            "captured-input",
            "VERCEL_LOGIN_EMAIL",
            raw_value,
            &stored_value,
            "Vercel login email",
        ));
    }

    if looks_like_secretish_value(&stored_value) {
        let generic_name = normalize_secret_name(label, context_heading);
        if !generic_name.is_empty() {
            return Some(named_prompt_secret(
                "captured-input",
                &generic_name,
                raw_value,
                &stored_value,
                label.trim(),
            ));
        }
    }

    None
}

fn named_prompt_secret(
    scope: &str,
    name: &str,
    raw_value: &str,
    stored_value: &str,
    label: &str,
) -> DetectedPromptSecret {
    DetectedPromptSecret {
        scope: scope.to_string(),
        name: name.to_string(),
        literal: raw_value.trim().to_string(),
        stored_value: stored_value.to_string(),
        label: Some(label.to_string()),
    }
}

fn next_unlabeled_value_line<'a>(lines: &'a [&str], start: usize) -> Option<&'a str> {
    lines
        .iter()
        .skip(start)
        .map(|line| line.trim())
        .find(|line| !line.is_empty() && !line_looks_labeled(line))
}

fn next_unlabeled_value_line_after<'a>(
    lines: &'a [&str],
    start: usize,
    skip_value: &str,
) -> Option<&'a str> {
    let mut skipped = false;
    for line in lines.iter().skip(start) {
        let trimmed = line.trim();
        if trimmed.is_empty() || line_looks_labeled(trimmed) {
            continue;
        }
        if !skipped && trimmed == skip_value {
            skipped = true;
            continue;
        }
        if skipped {
            return Some(trimmed);
        }
    }
    None
}

fn line_looks_labeled(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.contains("://") {
        return false;
    }
    let stripped = trimmed
        .strip_prefix("- ")
        .or_else(|| trimmed.strip_prefix("* "))
        .unwrap_or(trimmed);
    let Some((label, _)) = stripped.split_once(':') else {
        return false;
    };
    let label = label.trim();
    !label.is_empty()
        && label.chars().next().is_some_and(|ch| ch.is_alphabetic())
        && label.len() <= 80
}

fn normalize_secret_value(raw_value: &str) -> Option<String> {
    let trimmed = raw_value.trim();
    if trimmed.is_empty() || trimmed.contains("[secret-ref:") {
        return None;
    }
    Some(
        trimmed
            .trim_matches('"')
            .trim_matches('\'')
            .trim_matches('`')
            .trim()
            .to_string(),
    )
}

fn normalize_secret_name(label: &str, context_heading: Option<&str>) -> String {
    let mut combined = String::new();
    let label_trimmed = label.trim();
    let label_is_generic = matches!(
        label_trimmed.to_ascii_lowercase().as_str(),
        "password" | "username" | "user" | "email" | "mailbox"
    );
    if label_is_generic {
        if let Some(heading) = context_heading {
            combined.push_str(heading.trim());
            combined.push(' ');
        }
    }
    combined.push_str(label_trimmed);

    let mut normalized = String::new();
    let mut previous_was_underscore = false;
    for ch in combined.chars() {
        if ch.is_ascii_alphanumeric() {
            normalized.push(ch.to_ascii_uppercase());
            previous_was_underscore = false;
        } else if !previous_was_underscore {
            normalized.push('_');
            previous_was_underscore = true;
        }
    }
    normalized.trim_matches('_').to_string()
}

fn looks_like_openai_key(value: &str) -> bool {
    value.starts_with("sk-proj-") || value.starts_with("sk-live-") || value.starts_with("sk-test-")
}

fn looks_like_openrouter_key(value: &str) -> bool {
    value.starts_with("sk-or-v1-")
}

fn looks_like_minimax_key(value: &str) -> bool {
    value.starts_with("sk-api-")
}

fn looks_like_database_url(value: &str) -> bool {
    value.starts_with("postgres://") || value.starts_with("postgresql://")
}

fn looks_like_email(value: &str) -> bool {
    let trimmed = value.trim();
    trimmed.contains('@')
        && !trimmed.contains(' ')
        && trimmed
            .split('@')
            .nth(1)
            .is_some_and(|domain| domain.contains('.'))
}

fn looks_like_secretish_value(value: &str) -> bool {
    let lowered = value.to_ascii_lowercase();
    if looks_like_email(value) || lowered.starts_with("http://") || lowered.starts_with("https://")
    {
        return false;
    }
    if looks_like_database_url(value)
        || looks_like_openai_key(value)
        || looks_like_openrouter_key(value)
        || looks_like_minimax_key(value)
    {
        return true;
    }
    if looks_like_filesystem_path_value(value) {
        return false;
    }
    value.len() >= 12
        && !value.contains(' ')
        && value.chars().any(|ch| ch.is_ascii_digit())
        && value
            .chars()
            .any(|ch| matches!(ch, '-' | '_' | ':' | '/' | '?' | '=' | '&'))
}

fn looks_like_filesystem_path_value(value: &str) -> bool {
    let trimmed = value.trim_matches(|ch: char| matches!(ch, '"' | '\'' | '`'));
    trimmed.starts_with('/')
        || trimmed.starts_with("~/")
        || trimmed.starts_with("./")
        || trimmed.starts_with("../")
}

pub fn secret_exists(root: &Path, scope: &str, name: &str) -> Result<bool> {
    let conn = open_secret_db(root)?;
    ensure_secret_schema(&conn)?;
    let exists = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM ctox_secret_records WHERE scope = ?1 AND secret_name = ?2)",
        params![scope, name],
        |row| row.get::<_, i64>(0),
    )?;
    Ok(exists != 0)
}

fn put_secret(
    root: &Path,
    scope: &str,
    name: &str,
    value: &str,
    description: Option<String>,
    metadata: Value,
) -> Result<SecretRecordView> {
    let conn = open_secret_db(root)?;
    ensure_secret_schema(&conn)?;
    let (key_bytes, _) = ensure_secret_master_key(root)?;
    let encrypted = encrypt_secret_value(&key_bytes, value.as_bytes())?;
    let now = now_iso_string();
    let secret_id = format!("secret:{}:{}", scope, stable_digest(name));
    conn.execute(
        r#"
        INSERT INTO ctox_secret_records (
            secret_id, scope, secret_name, description, metadata_json,
            nonce_b64, ciphertext_b64, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)
        ON CONFLICT(scope, secret_name) DO UPDATE SET
            description=excluded.description,
            metadata_json=excluded.metadata_json,
            nonce_b64=excluded.nonce_b64,
            ciphertext_b64=excluded.ciphertext_b64,
            updated_at=excluded.updated_at
        "#,
        params![
            secret_id,
            scope,
            name,
            description,
            serde_json::to_string(&metadata)?,
            encrypted.nonce_b64,
            encrypted.ciphertext_b64,
            now,
        ],
    )?;
    load_secret_record(root, scope, name)?.context("secret metadata vanished after write")
}

fn list_secrets(root: &Path, scope: Option<&str>) -> Result<Vec<SecretRecordView>> {
    let conn = open_secret_db(root)?;
    ensure_secret_schema(&conn)?;
    let mut statement = conn.prepare(
        r#"
        SELECT secret_id, scope, secret_name, description, metadata_json, created_at, updated_at
        FROM ctox_secret_records
        WHERE (?1 IS NULL OR scope = ?1)
        ORDER BY updated_at DESC
        "#,
    )?;
    let rows = statement.query_map(params![scope], map_secret_record_row)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(anyhow::Error::from)
}

fn load_secret_record(root: &Path, scope: &str, name: &str) -> Result<Option<SecretRecordView>> {
    let conn = open_secret_db(root)?;
    ensure_secret_schema(&conn)?;
    let record = conn
        .query_row(
            r#"
            SELECT secret_id, scope, secret_name, description, metadata_json, created_at, updated_at
            FROM ctox_secret_records
            WHERE scope = ?1 AND secret_name = ?2
            LIMIT 1
            "#,
            params![scope, name],
            map_secret_record_row,
        )
        .optional()?;
    Ok(record)
}

fn get_secret_value(root: &Path, scope: &str, name: &str) -> Result<String> {
    let conn = open_secret_db(root)?;
    ensure_secret_schema(&conn)?;
    let (nonce_b64, ciphertext_b64): (String, String) = conn
        .query_row(
            r#"
            SELECT nonce_b64, ciphertext_b64
            FROM ctox_secret_records
            WHERE scope = ?1 AND secret_name = ?2
            LIMIT 1
            "#,
            params![scope, name],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?
        .context("secret not found")?;
    let (key_bytes, _) = ensure_secret_master_key(root)?;
    let value = decrypt_secret_value(&key_bytes, &nonce_b64, &ciphertext_b64)?;
    std::str::from_utf8(&value)
        .map(str::to_owned)
        .context("secret value is not valid UTF-8")
}

fn delete_secret(root: &Path, scope: &str, name: &str) -> Result<()> {
    let conn = open_secret_db(root)?;
    ensure_secret_schema(&conn)?;
    conn.execute(
        "DELETE FROM ctox_secret_records WHERE scope = ?1 AND secret_name = ?2",
        params![scope, name],
    )?;
    Ok(())
}

fn resolve_db_path(root: &Path) -> PathBuf {
    persistence::sqlite_path(root)
}

fn open_secret_db(root: &Path) -> Result<Connection> {
    let db_path = resolve_db_path(root);
    if let Some(parent) = db_path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!("failed to create secret DB directory {}", parent.display())
        })?;
    }
    Connection::open(&db_path).with_context(|| format!("failed to open {}", db_path.display()))
}

fn ensure_secret_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS ctox_secret_records (
            secret_id TEXT PRIMARY KEY,
            scope TEXT NOT NULL,
            secret_name TEXT NOT NULL,
            description TEXT,
            metadata_json TEXT NOT NULL,
            nonce_b64 TEXT NOT NULL,
            ciphertext_b64 TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            UNIQUE(scope, secret_name)
        );

        CREATE INDEX IF NOT EXISTS idx_ctox_secret_scope
            ON ctox_secret_records(scope, updated_at DESC);
        "#,
    )?;
    Ok(())
}

fn ensure_secret_master_key(root: &Path) -> Result<(SecretMaterial, &'static str)> {
    if let Some(raw) = persistence::load_text_value(root, MASTER_KEY_STORAGE_KEY)? {
        let bytes = BASE64_STANDARD
            .decode(raw.trim())
            .context("failed to decode SQLite-stored secret master key")?;
        if bytes.len() != 32 {
            anyhow::bail!("stored secret master key must decode to exactly 32 bytes");
        }
        return Ok((Zeroizing::new(bytes), "sqlite"));
    }

    let mut key = Zeroizing::new(vec![0u8; 32]);
    SystemRandom::new()
        .fill(&mut key)
        .map_err(|_| anyhow::anyhow!("failed to generate secret master key"))?;
    let encoded = BASE64_STANDARD.encode(&key);
    persistence::store_text_value(root, MASTER_KEY_STORAGE_KEY, Some(&encoded))?;
    Ok((key, "generated_sqlite"))
}

struct EncryptedSecretValue {
    nonce_b64: String,
    ciphertext_b64: String,
}

fn encrypt_secret_value(key_bytes: &[u8], plaintext: &[u8]) -> Result<EncryptedSecretValue> {
    let unbound = aead::UnboundKey::new(&aead::AES_256_GCM, key_bytes)
        .map_err(|_| anyhow::anyhow!("failed to construct secret encryption key"))?;
    let key = aead::LessSafeKey::new(unbound);
    let mut nonce_bytes = [0u8; 12];
    SystemRandom::new()
        .fill(&mut nonce_bytes)
        .map_err(|_| anyhow::anyhow!("failed to generate encryption nonce"))?;
    let nonce = aead::Nonce::assume_unique_for_key(nonce_bytes);
    let mut buffer = plaintext.to_vec();
    key.seal_in_place_append_tag(nonce, aead::Aad::empty(), &mut buffer)
        .map_err(|_| anyhow::anyhow!("failed to encrypt secret value"))?;
    let ciphertext_b64 = BASE64_STANDARD.encode(buffer.as_slice());
    buffer.zeroize();
    Ok(EncryptedSecretValue {
        nonce_b64: BASE64_STANDARD.encode(nonce_bytes),
        ciphertext_b64,
    })
}

fn decrypt_secret_value(
    key_bytes: &[u8],
    nonce_b64: &str,
    ciphertext_b64: &str,
) -> Result<SecretMaterial> {
    let nonce_bytes = BASE64_STANDARD
        .decode(nonce_b64)
        .context("failed to decode secret nonce")?;
    let nonce_array: [u8; 12] = nonce_bytes
        .as_slice()
        .try_into()
        .map_err(|_| anyhow::anyhow!("decoded secret nonce must be 12 bytes"))?;
    let mut ciphertext = BASE64_STANDARD
        .decode(ciphertext_b64)
        .context("failed to decode secret ciphertext")?;
    let unbound = aead::UnboundKey::new(&aead::AES_256_GCM, key_bytes)
        .map_err(|_| anyhow::anyhow!("failed to construct secret decryption key"))?;
    let key = aead::LessSafeKey::new(unbound);
    let plaintext = key
        .open_in_place(
            aead::Nonce::assume_unique_for_key(nonce_array),
            aead::Aad::empty(),
            &mut ciphertext,
        )
        .map_err(|_| anyhow::anyhow!("failed to decrypt secret value"))?;
    let plaintext_copy = Zeroizing::new(plaintext.to_vec());
    ciphertext.zeroize();
    Ok(plaintext_copy)
}

fn map_secret_record_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SecretRecordView> {
    let metadata_raw: String = row.get(4)?;
    Ok(SecretRecordView {
        secret_id: row.get(0)?,
        scope: row.get(1)?,
        secret_name: row.get(2)?,
        description: row.get(3)?,
        metadata: serde_json::from_str(&metadata_raw).unwrap_or_else(|_| json!({})),
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
    })
}

fn required_flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    find_flag_value(args, flag)
}

fn find_flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.windows(2).find_map(|window| {
        if window[0] == flag {
            Some(window[1].as_str())
        } else {
            None
        }
    })
}

fn parse_json_value(raw: &str) -> Result<Value> {
    serde_json::from_str(raw).with_context(|| format!("failed to parse JSON value: {raw}"))
}

fn stable_digest(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    let digest = hasher.finalize();
    hex_encode(&digest[..12])
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push_str(&format!("{byte:02x}"));
    }
    output
}

fn now_iso_string() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn secret_reference_text(scope: &str, name: &str, label: Option<&str>) -> String {
    let handle = format!("{scope}/{name}");
    match label.map(str::trim).filter(|value| !value.is_empty()) {
        Some(label) => format!("[secret-ref:{handle} label={label}]"),
        None => format!("[secret-ref:{handle}]"),
    }
}

fn print_json(value: &Value) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

// ── Credential helpers for runtime_env integration ──────────────────────
//
// These functions provide a thin API for storing and retrieving API keys
// and other credentials in the encrypted SQLite secret store instead of
// plaintext runtime config rows. Scope is always "credentials".

const CREDENTIAL_SCOPE: &str = "credentials";

/// Keys that must be stored encrypted (never in plaintext runtime config rows).
const SECRET_KEYS: &[&str] = &[
    "OPENAI_API_KEY",
    "ANTHROPIC_API_KEY",
    "OPENROUTER_API_KEY",
    "MISTRAL_API_KEY",
    "CTOX_MISTRAL_API_KEY",
    "MINIMAX_API_KEY",
    "AZURE_FOUNDRY_API_KEY",
    "DATABASE_URL",
    "CTO_EMAIL_PASSWORD",
    "CTO_EMAIL_GRAPH_PASSWORD",
    "CTO_EMAIL_GRAPH_CLIENT_SECRET",
    "CTO_EMAIL_GRAPH_ACCESS_TOKEN",
    "CTO_TEAMS_PASSWORD",
    "CTO_TEAMS_CLIENT_SECRET",
    "CTO_TEAMS_GRAPH_ACCESS_TOKEN",
    "CTOX_WEBRTC_PASSWORD",
    "HF_TOKEN",
    "HUGGINGFACE_HUB_TOKEN",
];

/// Returns true if `key` is a credential that must be stored encrypted.
pub fn is_secret_key(key: &str) -> bool {
    SECRET_KEYS.contains(&key)
}

/// Store a credential value in the encrypted secret store.
pub fn set_credential(root: &Path, key: &str, value: &str) -> Result<()> {
    put_secret(
        root,
        CREDENTIAL_SCOPE,
        key,
        value,
        Some(format!("{key} (auto-managed)")),
        json!({"source": "runtime_env"}),
    )?;
    Ok(())
}

/// Remove a credential value from the encrypted secret store.
pub fn delete_credential(root: &Path, key: &str) -> Result<()> {
    delete_secret(root, CREDENTIAL_SCOPE, key)
}

/// Retrieve a credential value from the encrypted secret store.
/// Returns None if the key does not exist or on any error.
pub fn get_credential(root: &Path, key: &str) -> Option<String> {
    get_secret_value(root, CREDENTIAL_SCOPE, key).ok()
}

/// Merge encrypted credentials back into an env map so callers see a
/// unified view. Existing entries in the map are NOT overwritten.
pub fn merge_credentials_into_env_map(
    root: &Path,
    env_map: &mut std::collections::BTreeMap<String, String>,
) {
    for &key in SECRET_KEYS {
        if env_map.contains_key(key) {
            continue; // already populated by the runtime config map
        }
        if let Some(value) = get_credential(root, key) {
            if !value.trim().is_empty() {
                env_map.insert(key.to_string(), value);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(label: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!("ctox-secret-test-{}-{}", label, std::process::id()));
        let _ = fs::remove_dir_all(&path);
        path
    }

    #[test]
    fn secret_store_encrypts_values_at_rest_and_round_trips() -> Result<()> {
        let root = temp_root("roundtrip");
        fs::create_dir_all(&root)?;

        let record = put_secret(
            &root,
            "ticket:zammad",
            "api-token",
            "super-secret-token",
            Some("Zammad API token".to_string()),
            json!({"kind": "token"}),
        )?;
        assert_eq!(record.secret_name, "api-token");
        assert!(secret_exists(&root, "ticket:zammad", "api-token")?);

        let plaintext = get_secret_value(&root, "ticket:zammad", "api-token")?;
        assert_eq!(plaintext, "super-secret-token");

        let db_path = resolve_db_path(&root);
        let raw = fs::read(&db_path)?;
        let raw_text = String::from_utf8_lossy(&raw);
        assert!(!raw_text.contains("super-secret-token"));

        let records = list_secrets(&root, Some("ticket:zammad"))?;
        assert_eq!(records.len(), 1);

        let _ = fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn secret_intake_stores_secret_and_rewrites_memory_reference() -> Result<()> {
        let root = temp_root("intake");
        fs::create_dir_all(&root)?;
        let lcm_db = root.join("runtime").join("ctox.sqlite3");
        if let Some(parent) = lcm_db.parent() {
            fs::create_dir_all(parent)?;
        }
        let engine = lcm::LcmEngine::open(&lcm_db, lcm::LcmConfig::default())?;
        engine.add_message(
            51,
            "user",
            "Please use sk-live-super-secret for the monitoring API",
        )?;
        drop(engine);

        let intake = intake_secret(
            &root,
            "monitoring",
            "api-token",
            "sk-live-super-secret",
            Some("Monitoring API token".to_string()),
            json!({"source": "user_message"}),
            Some(IntakeRewriteRequest {
                db_path: lcm_db.clone(),
                conversation_id: 51,
                match_text: "sk-live-super-secret".to_string(),
                label: Some("monitoring api token".to_string()),
            }),
        )?;

        assert_eq!(intake.secret.scope, "monitoring");
        let rewrite = intake.rewrite.context("expected memory rewrite result")?;
        assert_eq!(rewrite.message_rows_updated, 1);
        assert_eq!(
            get_secret_value(&root, "monitoring", "api-token")?,
            "sk-live-super-secret"
        );

        let snapshot = lcm::run_dump(&lcm_db, 51)?;
        assert!(snapshot.messages[0]
            .content
            .contains("[secret-ref:monitoring/api-token label=monitoring api token]"));
        assert!(!snapshot.messages[0]
            .content
            .contains("sk-live-super-secret"));

        let _ = fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn auto_intake_prompt_secrets_stores_and_rewrites_high_confidence_literals() -> Result<()> {
        let root = temp_root("prompt-auto-intake");
        fs::create_dir_all(&root)?;
        let prompt = "\
openAI API key:
sk-proj-super-secret-key-1234567890

Neon Postgres zugriff:
postgresql://user:pw@example.neon.tech/db?sslmode=require

Vercel login über:
metricspace.ai@gmail.com
vercel-password-123
";

        let result = auto_intake_prompt_secrets(&root, prompt)?;

        assert!(result.auto_ingested_secrets >= 4);
        assert!(!result
            .sanitized_prompt
            .contains("sk-proj-super-secret-key-1234567890"));
        assert!(!result
            .sanitized_prompt
            .contains("postgresql://user:pw@example.neon.tech/db?sslmode=require"));
        assert!(!result.sanitized_prompt.contains("vercel-password-123"));
        assert!(result
            .sanitized_prompt
            .contains("[secret-ref:credentials/OPENAI_API_KEY"));
        assert!(result
            .sanitized_prompt
            .contains("[secret-ref:credentials/DATABASE_URL"));
        assert!(result
            .sanitized_prompt
            .contains("[secret-ref:captured-input/VERCEL_LOGIN_PASSWORD"));

        assert_eq!(
            get_secret_value(&root, "credentials", "OPENAI_API_KEY")?,
            "sk-proj-super-secret-key-1234567890"
        );
        assert_eq!(
            get_secret_value(&root, "credentials", "DATABASE_URL")?,
            "postgresql://user:pw@example.neon.tech/db?sslmode=require"
        );
        assert_eq!(
            get_secret_value(&root, "captured-input", "VERCEL_LOGIN_EMAIL")?,
            "metricspace.ai@gmail.com"
        );
        assert_eq!(
            get_secret_value(&root, "captured-input", "VERCEL_LOGIN_PASSWORD")?,
            "vercel-password-123"
        );

        let _ = fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn auto_intake_does_not_rewrite_workspace_paths_as_secrets() -> Result<()> {
        let root = temp_root("prompt-auto-intake-workspace-path");
        fs::create_dir_all(&root)?;
        let workspace =
            "/home/metricspace/ctox/runtime/model-smoke/20260506T195937-hy3-responses-id-smoke";
        let prompt = format!(
            "Work only inside this workspace: {workspace}\nCreate smoke.txt in that workspace."
        );

        let result = auto_intake_prompt_secrets(&root, &prompt)?;

        assert_eq!(result.auto_ingested_secrets, 0);
        assert_eq!(result.sanitized_prompt, prompt);
        assert!(!secret_exists(
            &root,
            "captured-input",
            "WORK_ONLY_INSIDE_THIS_WORKSPACE"
        )?);

        let _ = fs::remove_dir_all(&root);
        Ok(())
    }
}
