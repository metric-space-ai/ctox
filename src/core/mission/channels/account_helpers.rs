// Origin: CTOX
// License: Apache-2.0

use super::*;

pub(super) fn admin_email_policy_summaries(settings: &BTreeMap<String, String>) -> Vec<String> {
    let admins = parse_admin_email_policies(settings);
    if admins.is_empty() {
        return vec!["- no additional admin mail profiles configured".to_string()];
    }
    admins
        .into_iter()
        .map(|entry| {
            format!(
                "- {} ({})",
                entry.email,
                if entry.can_sudo {
                    "admin with sudo"
                } else {
                    "admin without sudo"
                }
            )
        })
        .collect()
}

pub(super) fn parse_founder_email_addresses(settings: &BTreeMap<String, String>) -> Vec<String> {
    let raw = settings
        .get("CTOX_FOUNDER_EMAIL_ADDRESSES")
        .map(String::as_str)
        .unwrap_or("");
    let mut seen = BTreeSet::new();
    raw.split(|ch| matches!(ch, '\n' | ',' | ';'))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(normalize_email_address)
        .filter(|value| !value.is_empty())
        .filter(|value| seen.insert(value.clone()))
        .collect()
}

pub(super) fn parse_founder_email_roles(
    settings: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    let raw = settings
        .get("CTOX_FOUNDER_EMAIL_ROLES")
        .map(String::as_str)
        .unwrap_or("");
    let mut roles = BTreeMap::new();
    for entry in raw
        .split(|ch| matches!(ch, '\n' | ',' | ';'))
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let separator_index = entry.find(['|', ':', '=']);
        let Some(index) = separator_index else {
            continue;
        };
        let email = normalize_email_address(entry[..index].trim());
        let role = entry[index + 1..].trim();
        if email.is_empty() || role.is_empty() {
            continue;
        }
        roles.insert(email, role.to_string());
    }
    roles
}

pub(super) fn founder_email_role_summaries(settings: &BTreeMap<String, String>) -> Vec<String> {
    let roles = parse_founder_email_roles(settings);
    parse_founder_email_addresses(settings)
        .into_iter()
        .map(|email| {
            let role = roles
                .get(&email)
                .cloned()
                .unwrap_or_else(|| "Founder".to_string());
            format!("{email} ({role})")
        })
        .collect()
}

pub(super) fn parse_admin_email_policies(
    settings: &BTreeMap<String, String>,
) -> Vec<AdminEmailPolicy> {
    let raw = settings
        .get("CTOX_EMAIL_ADMIN_POLICIES")
        .map(String::as_str)
        .unwrap_or("");
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for entry in raw
        .split(|ch| matches!(ch, '\n' | ',' | ';'))
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let separator_index = entry.find(['|', ':', '=']);
        let (email_part, policy_part) = if let Some(index) = separator_index {
            (entry[..index].trim(), entry[index + 1..].trim())
        } else {
            (entry, "")
        };
        let email = normalize_email_address(email_part);
        if email.is_empty() || !seen.insert(email.clone()) {
            continue;
        }
        let policy = policy_part.to_ascii_lowercase().replace(' ', "");
        let can_sudo = policy == "sudo"
            || policy == "admin+sudo"
            || policy == "withsudo"
            || (policy.contains("sudo")
                && !policy.contains("no-sudo")
                && !policy.contains("nosudo")
                && !policy.contains("withoutsudo"));
        out.push(AdminEmailPolicy { email, can_sudo });
    }
    out
}

pub(super) fn normalize_email_address(value: &str) -> String {
    value
        .trim()
        .trim_matches('<')
        .trim_matches('>')
        .to_lowercase()
}

pub(super) fn normalized_allowed_email_domain(
    settings: &BTreeMap<String, String>,
) -> Option<String> {
    settings
        .get("CTOX_ALLOWED_EMAIL_DOMAIN")
        .map(|value| value.trim().trim_start_matches('@').to_lowercase())
        .filter(|value| !value.is_empty())
        .or_else(|| {
            settings
                .get("CTOX_OWNER_EMAIL_ADDRESS")
                .map(|value| normalize_email_address(value))
                .and_then(|value| value.split_once('@').map(|(_, domain)| domain.to_string()))
                .filter(|value| !value.is_empty())
        })
}

pub(super) fn email_matches_domain(email: &str, domain: &str) -> bool {
    email
        .rsplit_once('@')
        .map(|(_, candidate_domain)| candidate_domain.eq_ignore_ascii_case(domain))
        .unwrap_or(false)
}

pub(super) fn ensure_account_tx(
    tx: &Transaction<'_>,
    account_key: &str,
    channel: &str,
    address: &str,
    provider: &str,
    mut profile_json: Value,
) -> Result<()> {
    // Native send/test profiles describe the connector, not Business OS
    // account access. Preserve the existing email owner and shares only when
    // these fields are absent. The account configuration path supplies
    // explicit empty values to revoke access, which must not be restored.
    if channel == "email" {
        if let Some(incoming) = profile_json.as_object_mut() {
            let missing_owner =
                !incoming.contains_key("ownerUserId") && !incoming.contains_key("owner_user_id");
            let missing_shares = !incoming.contains_key("shared_user_ids")
                && !incoming.contains_key("sharedUserIds")
                && !incoming.contains_key("member_user_ids");
            if missing_owner || missing_shares {
                let previous: Option<String> = tx
                    .query_row(
                        "SELECT profile_json FROM communication_accounts \
                         WHERE account_key = ?1 AND channel = 'email'",
                        [account_key],
                        |row| row.get(0),
                    )
                    .optional()?;
                if let Some(previous) = previous
                    .as_deref()
                    .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
                    .and_then(|value| value.as_object().cloned())
                {
                    if missing_owner {
                        if let Some(owner) = previous
                            .get("owner_user_id")
                            .or_else(|| previous.get("ownerUserId"))
                            .filter(|value| value.is_string())
                        {
                            incoming.insert("ownerUserId".into(), owner.clone());
                        }
                    }
                    if missing_shares {
                        if let Some(shares) = previous
                            .get("shared_user_ids")
                            .or_else(|| previous.get("sharedUserIds"))
                            .or_else(|| previous.get("member_user_ids"))
                            .filter(|value| value.is_array())
                        {
                            incoming.insert("shared_user_ids".into(), shares.clone());
                        }
                    }
                }
            }
        }
    }
    let now = now_iso_string();
    tx.execute(
        r#"
        INSERT INTO communication_accounts (
            account_key, channel, address, provider, profile_json, created_at, updated_at, last_inbound_ok_at, last_outbound_ok_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6, NULL, NULL)
        ON CONFLICT(account_key) DO UPDATE SET
            channel=excluded.channel,
            address=excluded.address,
            provider=excluded.provider,
            profile_json=excluded.profile_json,
            updated_at=excluded.updated_at
        "#,
        params![
            account_key,
            channel,
            address,
            provider,
            serde_json::to_string(&profile_json)?,
            now,
        ],
    )?;
    Ok(())
}

pub(crate) fn upsert_communication_account(
    conn: &mut Connection,
    account_key: &str,
    channel: &str,
    address: &str,
    provider: &str,
    profile_json: Value,
) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    ensure_account_tx(&tx, account_key, channel, address, provider, profile_json)?;
    tx.commit()?;
    Ok(())
}

pub(crate) fn record_communication_sync_run(
    conn: &mut Connection,
    run: CommunicationSyncRun<'_>,
) -> Result<()> {
    if run.ok && run.stored_count <= 0 && run.error_text.trim().is_empty() {
        return Ok(());
    }
    conn.execute(
        r#"
        INSERT INTO communication_sync_runs (
            run_key, channel, account_key, folder_hint, started_at, finished_at,
            ok, fetched_count, stored_count, error_text, metadata_json
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
        "#,
        params![
            run.run_key,
            run.channel,
            run.account_key,
            run.folder_hint,
            run.started_at,
            run.finished_at,
            if run.ok { 1 } else { 0 },
            run.fetched_count,
            run.stored_count,
            run.error_text,
            run.metadata_json,
        ],
    )?;
    Ok(())
}

pub(crate) fn stable_digest(input: &str) -> String {
    let digest = Sha256::digest(input.as_bytes());
    let hex = format!("{digest:x}");
    hex[..24].to_string()
}

pub(super) fn email_address_from_account_key(account_key: &str) -> String {
    account_key
        .strip_prefix("email:")
        .unwrap_or(account_key)
        .to_string()
}

#[derive(Debug)]
pub(super) struct AccountConfig {
    pub(super) provider: String,
    pub(super) profile_json: Value,
}

pub(super) fn load_account_config(
    conn: &Connection,
    account_key: &str,
) -> Result<Option<AccountConfig>> {
    let row = conn
        .query_row(
            r#"
            SELECT provider, profile_json
            FROM communication_accounts
            WHERE account_key = ?1
            LIMIT 1
            "#,
            params![account_key],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?;
    let Some((provider, profile_json)) = row else {
        return Ok(None);
    };
    let parsed_profile = serde_json::from_str(&profile_json)
        .unwrap_or_else(|_| json!({ "raw_profile_json": profile_json }));
    Ok(Some(AccountConfig {
        provider,
        profile_json: parsed_profile,
    }))
}

pub(super) fn jami_address_from_account_key(account_key: &str) -> String {
    account_key
        .strip_prefix("jami:")
        .unwrap_or(account_key)
        .to_string()
}

pub(super) fn teams_tenant_from_account_config(
    account_config: Option<&AccountConfig>,
) -> Option<String> {
    account_config
        .and_then(|config| config.profile_json.get("tenantId"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|tenant_id| !tenant_id.is_empty())
        .map(ToOwned::to_owned)
}

pub(super) fn resolve_account_key(
    conn: &Connection,
    channel: &str,
    explicit: Option<&str>,
) -> Result<String> {
    if let Some(value) = explicit.map(str::trim).filter(|value| !value.is_empty()) {
        return Ok(value.to_string());
    }
    conn.query_row(
        r#"
        SELECT account_key
        FROM communication_accounts
        WHERE channel = ?1
        ORDER BY updated_at DESC, account_key ASC
        LIMIT 1
        "#,
        params![channel],
        |row| row.get::<_, String>(0),
    )
    .optional()?
    .ok_or_else(|| anyhow::anyhow!("no configured account found for channel {channel}"))
}

pub(super) fn resolve_db_path(root: &Path, explicit: Option<&str>) -> PathBuf {
    explicit
        .map(PathBuf::from)
        .unwrap_or_else(|| crate::paths::core_db(&root))
}

pub(super) fn required_flag_value<'a>(args: &'a [String], flag: &str) -> Result<&'a str> {
    find_flag_value(args, flag).with_context(|| format!("missing required flag {flag}"))
}

pub(super) fn find_flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let mut index = 0usize;
    while index < args.len() {
        if args[index] == flag {
            return args.get(index + 1).map(String::as_str);
        }
        index += 1;
    }
    None
}

pub(super) fn collect_flag_values(args: &[String], flag: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut index = 0usize;
    while index < args.len() {
        if args[index] == flag {
            if let Some(value) = args.get(index + 1) {
                values.push(value.clone());
            }
            index += 2;
        } else {
            index += 1;
        }
    }
    values
}

pub(super) fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|arg| arg == flag)
}

pub(super) fn positional_after_flags(args: &[String]) -> Vec<String> {
    let mut items = Vec::new();
    let mut index = 0usize;
    while index < args.len() {
        let token = &args[index];
        if token.starts_with("--") {
            index += 1;
            if index < args.len() && !args[index].starts_with("--") {
                index += 1;
            }
            continue;
        }
        items.push(token.clone());
        index += 1;
    }
    items
}

pub(super) fn print_json(value: &Value) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}
