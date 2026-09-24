//! Persönliche externe E-Mail-Konten (App-Setting der Mail-App).
//!
//! Abgrenzung: Der Kanal `email` im Operator-Env (`CTO_EMAIL_*`) ist die
//! Identität der CTOX-Instanz selbst (System-Einstellung). Die Konten hier
//! sind die Postfächer einzelner Nutzer:innen ("mein Konto"), verwaltet als
//! App-Setting über die Mail-App. Beide teilen denselben nativen
//! IMAP/SMTP-Konnektor (`email_native`).
//!
//! Ablage: Die Registry (ohne Secrets) liegt im Runtime-Env unter
//! `CTO_EMAIL_ACCOUNTS` (bestehender `runtime_env`-Pfad, nie in RxDB);
//! Passwörter liegen ausschließlich im CTOX-Secret-Store
//! (Scope `email-account`, Name = normalisierte Adresse).

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::{bail, Context, Result};
use rusqlite::OptionalExtension;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::inference::runtime_env;
use crate::secrets;

pub(crate) const REGISTRY_ENV_KEY: &str = "CTO_EMAIL_ACCOUNTS";
pub(crate) const SECRET_SCOPE: &str = "email-account";

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(crate) struct EmailAccountConfig {
    pub address: String,
    #[serde(default)]
    pub display_name: String,
    /// `imap` (Default) oder ein anderer vom nativen Konnektor unterstützter
    /// Provider (`graph`, `ews`, …).
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub imap_host: String,
    #[serde(default)]
    pub imap_port: u16,
    #[serde(default)]
    pub smtp_host: String,
    #[serde(default)]
    pub smtp_port: u16,
    /// SMTP/IMAP-Benutzername, falls abweichend von der Adresse.
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub ews_url: String,
    #[serde(default)]
    pub owa_url: String,
    #[serde(default)]
    pub ews_auth_type: String,
    #[serde(default)]
    pub ews_version: String,
    /// Business-OS-Benutzer, dem dieses Konto gehört.
    #[serde(default)]
    pub owner_user_id: String,
    /// Explizit freigegebene Business-OS-Benutzer. `None` bedeutet bei einem
    /// Upsert: die bisherige Freigabe unverändert lassen; `Some([])` widerruft
    /// alle Freigaben. Neue Konten beginnen ohne Freigabe.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shared_user_ids: Option<Vec<String>>,
}

pub(crate) fn normalize_address(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

pub(crate) fn load_accounts(root: &Path) -> Result<Vec<EmailAccountConfig>> {
    let env_map = runtime_env::effective_operator_env_map(root).unwrap_or_default();
    let raw = env_map
        .get(REGISTRY_ENV_KEY)
        .map(String::as_str)
        .unwrap_or("");
    if raw.trim().is_empty() {
        return Ok(Vec::new());
    }
    let accounts: Vec<EmailAccountConfig> =
        serde_json::from_str(raw).context("CTO_EMAIL_ACCOUNTS ist kein gültiges JSON-Array")?;
    Ok(accounts)
}

fn save_accounts(root: &Path, accounts: &[EmailAccountConfig]) -> Result<()> {
    let mut env_map = runtime_env::effective_operator_env_map(root).unwrap_or_default();
    if accounts.is_empty() {
        env_map.remove(REGISTRY_ENV_KEY);
    } else {
        env_map.insert(
            REGISTRY_ENV_KEY.to_owned(),
            serde_json::to_string(accounts)?,
        );
    }
    runtime_env::save_runtime_env_map(root, &env_map)
}

/// Konto anlegen/aktualisieren. `password` wird — falls angegeben — in den
/// Secret-Store geschrieben und taucht nie in der Registry auf.
pub(crate) fn upsert_account(
    root: &Path,
    mut config: EmailAccountConfig,
    password: Option<&str>,
) -> Result<EmailAccountConfig> {
    config.address = normalize_address(&config.address);
    if config.address.is_empty() || !config.address.contains('@') {
        bail!("email account address is required");
    }
    if config.provider.trim().is_empty() {
        config.provider = "imap".to_owned();
    }
    let mut accounts = load_accounts(root)?;
    if let Some(existing) = accounts.iter().find(|item| item.address == config.address) {
        // Owner bleibt stabil; leere Felder überschreiben Bestehendes nicht.
        if config.owner_user_id.trim().is_empty() {
            config.owner_user_id = existing.owner_user_id.clone();
        }
        if config.shared_user_ids.is_none() {
            config.shared_user_ids = existing.shared_user_ids.clone();
        }
    }
    config.shared_user_ids.get_or_insert_with(Vec::new);
    validate_account_users(root, &mut config)?;
    if let Some(existing) = accounts
        .iter_mut()
        .find(|item| item.address == config.address)
    {
        *existing = config.clone();
    } else {
        accounts.push(config.clone());
    }
    // The registry and native channel projection cannot share one transaction.
    // Revoke native reads first, then persist the registry and finally publish
    // the new grants. An error at any stage may leave the account temporarily
    // unavailable, but it cannot leave a revoked reader with the old profile.
    let db_path = root.join("runtime/ctox.sqlite3");
    {
        let mut conn = crate::communication_store::open_channel_db(&db_path)?;
        crate::mission::channels::upsert_communication_account(
            &mut conn,
            &format!("email:{}", config.address),
            "email",
            &config.address,
            &config.provider,
            account_profile_json(&config, "", &[]),
        )?;
        save_accounts(root, &accounts)?;
        if let Some(secret) = password.map(str::trim).filter(|value| !value.is_empty()) {
            secrets::write_secret_record(
                root,
                SECRET_SCOPE,
                &config.address,
                secret,
                Some("Mail-App: persönliches E-Mail-Konto".to_owned()),
                json!({ "owner_user_id": config.owner_user_id }),
            )?;
        }
        crate::mission::channels::upsert_communication_account(
            &mut conn,
            &format!("email:{}", config.address),
            "email",
            &config.address,
            &config.provider,
            account_profile_json(
                &config,
                &config.owner_user_id,
                config.shared_user_ids.as_deref().unwrap_or(&[]),
            ),
        )?;
    }
    Ok(config)
}

fn account_profile_json(
    config: &EmailAccountConfig,
    owner: &str,
    shared_users: &[String],
) -> Value {
    json!({
        "imapHost": config.imap_host,
        "imapPort": config.imap_port,
        "smtpHost": config.smtp_host,
        "smtpPort": config.smtp_port,
        "username": config.username,
        "ewsUrl": config.ews_url,
        "owaUrl": config.owa_url,
        "ewsUsername": config.username,
        "ownerUserId": owner,
        "shared_user_ids": shared_users,
        "displayName": config.display_name,
        "source": "mail-app-account",
    })
}

fn validate_account_users(root: &Path, config: &mut EmailAccountConfig) -> Result<()> {
    config.owner_user_id = config.owner_user_id.trim().to_owned();
    let mut users = BTreeSet::new();
    if !config.owner_user_id.is_empty() {
        users.insert(config.owner_user_id.clone());
    }
    if let Some(shares) = &mut config.shared_user_ids {
        for shared_id in shares.iter_mut() {
            *shared_id = shared_id.trim().to_owned();
            if shared_id.is_empty() || !users.insert(shared_id.clone()) {
                bail!("shared_user_ids must contain distinct, nonempty users other than the owner");
            }
        }
    }
    if users.is_empty() {
        return Ok(());
    }
    let conn = crate::business_os::store::open_store(root)?;
    for user_id in users {
        let active = conn
            .query_row(
                "SELECT active FROM business_users WHERE user_id = ?1",
                [&user_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()?;
        if active != Some(1) {
            bail!("mail account user is not an active Business OS user: {user_id}");
        }
    }
    Ok(())
}

pub(crate) fn delete_account(root: &Path, address: &str) -> Result<bool> {
    let address = normalize_address(address);
    let mut accounts = load_accounts(root)?;
    let before = accounts.len();
    accounts.retain(|item| item.address != address);
    if accounts.len() == before {
        return Ok(false);
    }
    save_accounts(root, &accounts)?;
    let _ = secrets::delete_secret_record(root, SECRET_SCOPE, &address);
    Ok(true)
}

/// CTO_EMAIL_*-Overrides für ein Konto (inkl. Passwort aus dem Secret-Store),
/// damit der bestehende native Konnektor unverändert benutzt werden kann.
pub(crate) fn account_runtime_overrides(
    root: &Path,
    config: &EmailAccountConfig,
) -> BTreeMap<String, String> {
    let mut overrides = BTreeMap::new();
    overrides.insert("CTO_EMAIL_ADDRESS".to_owned(), config.address.clone());
    overrides.insert("CTO_EMAIL_PROVIDER".to_owned(), config.provider.clone());
    let set = |map: &mut BTreeMap<String, String>, key: &str, value: &str| {
        if !value.trim().is_empty() {
            map.insert(key.to_owned(), value.trim().to_owned());
        } else {
            // Leere Felder dürfen NICHT auf die Werte des Instanz-Kontos
            // (CTO_EMAIL_*) zurückfallen — sonst landet Post im falschen
            // Postfach-Kontext. Explizit leeren.
            map.insert(key.to_owned(), String::new());
        }
    };
    set(&mut overrides, "CTO_EMAIL_IMAP_HOST", &config.imap_host);
    overrides.insert(
        "CTO_EMAIL_IMAP_PORT".to_owned(),
        if config.imap_port > 0 {
            config.imap_port.to_string()
        } else {
            String::new()
        },
    );
    set(&mut overrides, "CTO_EMAIL_SMTP_HOST", &config.smtp_host);
    overrides.insert(
        "CTO_EMAIL_SMTP_PORT".to_owned(),
        if config.smtp_port > 0 {
            config.smtp_port.to_string()
        } else {
            String::new()
        },
    );
    let password =
        secrets::read_secret_value(root, SECRET_SCOPE, &config.address).unwrap_or_default();
    overrides.insert("CTO_EMAIL_PASSWORD".to_owned(), password);
    // Instanz-spezifische Graph/EWS/ActiveSync-Werte nicht erben.
    for key in [
        "CTO_EMAIL_GRAPH_ACCESS_TOKEN",
        "CTO_EMAIL_GRAPH_BASE_URL",
        "CTO_EMAIL_GRAPH_USER",
        "CTO_EMAIL_GRAPH_TENANT_ID",
        "CTO_EMAIL_GRAPH_CLIENT_ID",
        "CTO_EMAIL_GRAPH_CLIENT_SECRET",
        "CTO_EMAIL_GRAPH_USERNAME",
        "CTO_EMAIL_GRAPH_PASSWORD",
        "CTO_EMAIL_EWS_URL",
        "CTO_EMAIL_OWA_URL",
        "CTO_EMAIL_EWS_USERNAME",
        "CTO_EMAIL_EWS_BEARER_TOKEN",
        "CTO_EMAIL_ACTIVESYNC_SERVER",
        "CTO_EMAIL_ACTIVESYNC_USERNAME",
        "CTO_EMAIL_ACTIVESYNC_PATH",
        "CTO_EMAIL_ACTIVESYNC_DEVICE_ID",
        "CTO_EMAIL_ACTIVESYNC_DEVICE_TYPE",
        "CTO_EMAIL_ACTIVESYNC_PROTOCOL_VERSION",
        "CTO_EMAIL_ACTIVESYNC_POLICY_KEY",
    ] {
        overrides.insert(key.to_owned(), String::new());
    }
    // Ein Exchange-Konto (OWA/EWS) braucht seinen eigenen Server und seinen
    // Domänen-Benutzernamen. Beides wurde oben geleert und nie aus dem Konto
    // gesetzt: der Konnektor meldete sich mit der Mailadresse an und bekam
    // HTTP 401 (lena.ogiermann@thesen-ag.com, gemessen 22.09.2026), obwohl
    // "thesen-ag\\lena.ogiermann" im Konto hinterlegt war.
    if matches!(config.provider.trim(), "owa" | "ews" | "exchange") {
        let owa_url = config.owa_url.trim();
        let ews_url = if !config.ews_url.trim().is_empty() {
            config.ews_url.trim().to_owned()
        } else {
            ews_url_from_owa_url(owa_url)
        };
        set(&mut overrides, "CTO_EMAIL_OWA_URL", owa_url);
        set(&mut overrides, "CTO_EMAIL_EWS_URL", &ews_url);
        set(&mut overrides, "CTO_EMAIL_EWS_USERNAME", &config.username);
        set(
            &mut overrides,
            "CTO_EMAIL_ACTIVESYNC_USERNAME",
            &config.username,
        );
        if let Some(server) = url_origin(owa_url) {
            set(&mut overrides, "CTO_EMAIL_ACTIVESYNC_SERVER", &server);
        }
    }
    set(
        &mut overrides,
        "CTO_EMAIL_EWS_AUTH_TYPE",
        &config.ews_auth_type,
    );
    set(&mut overrides, "CTO_EMAIL_EWS_VERSION", &config.ews_version);
    overrides
}

fn url_origin(value: &str) -> Option<String> {
    let url = url::Url::parse(value.trim()).ok()?;
    let host = url.host_str()?;
    Some(match url.port() {
        Some(port) => format!("{}://{}:{}", url.scheme(), host, port),
        None => format!("{}://{}", url.scheme(), host),
    })
}

fn ews_url_from_owa_url(owa_url: &str) -> String {
    url_origin(owa_url)
        .map(|origin| format!("{origin}/EWS/Exchange.asmx"))
        .unwrap_or_default()
}

/// Öffentliche (secret-freie) Sicht für Listen-Endpunkte.
pub(crate) fn public_json(root: &Path, config: &EmailAccountConfig) -> Value {
    let has_password = secrets::read_secret_value(root, SECRET_SCOPE, &config.address)
        .map(|value| !value.is_empty())
        .unwrap_or(false);
    json!({
        "address": config.address,
        "display_name": config.display_name,
        "provider": config.provider,
        "imap_host": config.imap_host,
        "imap_port": config.imap_port,
        "smtp_host": config.smtp_host,
        "smtp_port": config.smtp_port,
        "username": config.username,
        "owa_url": config.owa_url,
        "ews_url": config.ews_url,
        "ews_auth_type": config.ews_auth_type,
        "ews_version": config.ews_version,
        "owner_user_id": config.owner_user_id,
        "shared_user_ids": config.shared_user_ids.clone().unwrap_or_default(),
        "has_password": has_password,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seed_user(root: &Path, user_id: &str) -> Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis() as i64;
        crate::business_os::store::issue_business_os_capability_token_for_managed_user(
            root, user_id, user_id, "user", now,
        )?;
        Ok(())
    }

    #[test]
    fn exchange_account_roundtrip_preserves_other_accounts_and_hides_password() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let root = dir.path();
        std::fs::create_dir_all(root.join("runtime"))?;
        for user_id in ["crew-owner", "crew-reader", "lena-owner"] {
            seed_user(root, user_id)?;
        }
        let crew = upsert_account(
            root,
            EmailAccountConfig {
                address: "crew@example.test".into(),
                provider: "imap".into(),
                imap_host: "crew.example.test".into(),
                owner_user_id: "crew-owner".into(),
                shared_user_ids: Some(vec!["crew-reader".into()]),
                ..Default::default()
            },
            Some("crew-fixture"),
        )?;
        let lena = upsert_account(
            root,
            EmailAccountConfig {
                address: "Lena@Example.test".into(),
                provider: "owa".into(),
                username: "DOMAIN\\lena".into(),
                owa_url: "https://lena.example.test/owa/".into(),
                ews_url: "https://lena.example.test/EWS/Exchange.asmx".into(),
                ews_auth_type: "basic".into(),
                ews_version: "Exchange2013".into(),
                owner_user_id: "lena-owner".into(),
                ..Default::default()
            },
            Some("lena-fixture"),
        )?;
        let accounts = load_accounts(root)?;
        assert_eq!(accounts.len(), 2);
        assert_eq!(
            serde_json::to_value(&accounts[0])?,
            serde_json::to_value(&crew)?
        );
        assert_eq!(
            serde_json::to_value(&accounts[1])?,
            serde_json::to_value(&lena)?
        );
        let settings = account_runtime_overrides(root, &accounts[1]);
        assert_eq!(settings["CTO_EMAIL_EWS_USERNAME"], "DOMAIN\\lena");
        assert_eq!(settings["CTO_EMAIL_OWA_URL"], lena.owa_url);
        assert_eq!(settings["CTO_EMAIL_EWS_URL"], "https://lena.example.test");
        assert_eq!(settings["CTO_EMAIL_PASSWORD"], "lena-fixture");
        assert_eq!(settings["CTO_EMAIL_IMAP_HOST"], "");
        assert_eq!(settings["CTO_EMAIL_GRAPH_ACCESS_TOKEN"], "");
        assert_eq!(settings["CTO_EMAIL_ACTIVESYNC_SERVER"], "");
        assert_eq!(
            account_runtime_overrides(root, &crew)["CTO_EMAIL_PASSWORD"],
            "crew-fixture"
        );
        let public = public_json(root, &accounts[1]);
        assert_eq!(public["has_password"], true);
        assert_eq!(public["username"], "DOMAIN\\lena");
        assert_eq!(public["owa_url"], lena.owa_url);
        assert_eq!(public["shared_user_ids"], json!([]));
        assert!(!public.to_string().contains("lena-fixture"));
        assert!(!serde_json::to_string(&accounts)?.contains("lena-fixture"));
        // A fresh runtime must expose the assigned account immediately, not
        // silently skip projection because the channel schema did not exist.
        let conn = crate::communication_store::open_channel_db(&root.join("runtime/ctox.sqlite3"))?;
        let profile: String = conn.query_row(
            "SELECT profile_json FROM communication_accounts WHERE account_key = ?1",
            ["email:lena@example.test"],
            |row| row.get(0),
        )?;
        let profile: Value = serde_json::from_str(&profile)?;
        assert_eq!(profile["ownerUserId"], "lena-owner");
        assert_eq!(profile["shared_user_ids"], json!([]));
        assert_eq!(profile["displayName"], "");
        assert_eq!(profile["owaUrl"], "https://lena.example.test/owa/");
        Ok(())
    }

    #[test]
    fn upsert_normalizes_and_keeps_owner() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let root = dir.path();
        std::fs::create_dir_all(root.join("runtime"))?;
        seed_user(root, "local-dev")?;
        let first = upsert_account(
            root,
            EmailAccountConfig {
                address: " Jill@Example.COM ".into(),
                imap_host: "imap.example.com".into(),
                imap_port: 993,
                smtp_host: "smtp.example.com".into(),
                smtp_port: 465,
                owner_user_id: "local-dev".into(),
                ..Default::default()
            },
            Some("geheim"),
        )?;
        assert_eq!(first.address, "jill@example.com");
        assert_eq!(first.provider, "imap");

        // Update ohne Owner: Owner bleibt erhalten.
        let second = upsert_account(
            root,
            EmailAccountConfig {
                address: "jill@example.com".into(),
                imap_host: "imap2.example.com".into(),
                ..Default::default()
            },
            None,
        )?;
        assert_eq!(second.owner_user_id, "local-dev");

        let accounts = load_accounts(root)?;
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].imap_host, "imap2.example.com");

        let overrides = account_runtime_overrides(root, &accounts[0]);
        assert_eq!(
            overrides.get("CTO_EMAIL_ADDRESS").unwrap(),
            "jill@example.com"
        );
        assert_eq!(overrides.get("CTO_EMAIL_PASSWORD").unwrap(), "geheim");
        // Instanzwerte werden explizit geleert, nicht geerbt.
        assert_eq!(overrides.get("CTO_EMAIL_EWS_URL").unwrap(), "");

        assert!(delete_account(root, "JILL@example.com")?);
        assert!(load_accounts(root)?.is_empty());

        // Exchange-Konto: eigener Server und Domänen-Benutzer kommen an.
        let exchange = upsert_account(
            root,
            EmailAccountConfig {
                address: "lena@example.com".into(),
                provider: "owa".into(),
                username: "example\\lena".into(),
                owa_url: "https://mail.example.com/".into(),
                ..Default::default()
            },
            Some("geheim"),
        )?;
        let overrides = account_runtime_overrides(root, &exchange);
        assert_eq!(
            overrides.get("CTO_EMAIL_EWS_USERNAME").unwrap(),
            "example\\lena"
        );
        assert_eq!(
            overrides.get("CTO_EMAIL_EWS_URL").unwrap(),
            "https://mail.example.com/EWS/Exchange.asmx"
        );
        assert_eq!(
            overrides.get("CTO_EMAIL_ACTIVESYNC_SERVER").unwrap(),
            "https://mail.example.com"
        );
        assert!(delete_account(root, "lena@example.com")?);
        assert!(load_accounts(root)?.is_empty());
        Ok(())
    }

    #[test]
    fn shared_users_are_validated_preserved_and_explicitly_revoked() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let root = dir.path();
        for user_id in ["owner", "reader", "inactive"] {
            seed_user(root, user_id)?;
        }
        let users = crate::business_os::store::open_store(root)?;
        users.execute(
            "UPDATE business_users SET active = 0 WHERE user_id = ?1",
            ["inactive"],
        )?;
        drop(users);
        let initial: EmailAccountConfig = serde_json::from_value(json!({
            "address": "team@example.test",
            "provider": "owa",
            "owner_user_id": "owner",
            "shared_user_ids": ["reader"],
        }))?;
        let saved = upsert_account(root, initial, Some("fixture-password"))?;
        assert_eq!(saved.shared_user_ids, Some(vec!["reader".to_owned()]));
        assert_eq!(
            public_json(root, &saved)["shared_user_ids"],
            json!(["reader"])
        );

        // Omitted shares retain the prior grants while an unrelated setting
        // changes; omitting the password retains its secret too.
        let update: EmailAccountConfig = serde_json::from_value(json!({
            "address": "team@example.test",
            "provider": "owa",
            "display_name": "Team mail",
        }))?;
        let saved = upsert_account(root, update, None)?;
        assert_eq!(saved.owner_user_id, "owner");
        assert_eq!(saved.shared_user_ids, Some(vec!["reader".to_owned()]));
        assert_eq!(
            account_runtime_overrides(root, &saved)["CTO_EMAIL_PASSWORD"],
            "fixture-password"
        );
        let conn = crate::communication_store::open_channel_db(&root.join("runtime/ctox.sqlite3"))?;
        let profile_raw: String = conn.query_row(
            "SELECT profile_json FROM communication_accounts WHERE account_key = ?1",
            ["email:team@example.test"],
            |row| row.get(0),
        )?;
        let profile: Value = serde_json::from_str(&profile_raw)?;
        assert_eq!(profile["shared_user_ids"], json!(["reader"]));
        assert_eq!(profile["ownerUserId"], "owner");
        drop(conn);

        let duplicate = EmailAccountConfig {
            address: saved.address.clone(),
            shared_user_ids: Some(vec!["reader".into(), "reader".into()]),
            ..saved.clone()
        };
        assert!(upsert_account(root, duplicate, None).is_err());
        let unknown = EmailAccountConfig {
            address: saved.address.clone(),
            shared_user_ids: Some(vec!["unknown-user".into()]),
            ..saved.clone()
        };
        assert!(upsert_account(root, unknown, None).is_err());
        let inactive = EmailAccountConfig {
            address: saved.address.clone(),
            shared_user_ids: Some(vec!["inactive".into()]),
            ..saved.clone()
        };
        assert!(upsert_account(root, inactive, None).is_err());
        assert_eq!(
            load_accounts(root)?[0].shared_user_ids,
            saved.shared_user_ids
        );

        let revoked = upsert_account(
            root,
            EmailAccountConfig {
                address: saved.address.clone(),
                shared_user_ids: Some(Vec::new()),
                ..saved
            },
            None,
        )?;
        assert_eq!(revoked.shared_user_ids, Some(Vec::new()));
        assert_eq!(public_json(root, &revoked)["shared_user_ids"], json!([]));
        let conn = crate::communication_store::open_channel_db(&root.join("runtime/ctox.sqlite3"))?;
        let profile_raw: String = conn.query_row(
            "SELECT profile_json FROM communication_accounts WHERE account_key = ?1",
            ["email:team@example.test"],
            |row| row.get(0),
        )?;
        let profile: Value = serde_json::from_str(&profile_raw)?;
        assert_eq!(profile["shared_user_ids"], json!([]));
        Ok(())
    }

    #[test]
    fn registry_failure_revokes_native_access_before_returning_error() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let root = dir.path();
        seed_user(root, "owner")?;
        seed_user(root, "reader")?;
        let saved = upsert_account(
            root,
            EmailAccountConfig {
                address: "team@example.test".into(),
                owner_user_id: "owner".into(),
                shared_user_ids: Some(vec!["reader".into()]),
                ..Default::default()
            },
            None,
        )?;
        let registry_path = crate::inference::runtime_env::runtime_config_path(root);
        std::fs::remove_file(&registry_path)?;
        std::fs::create_dir(&registry_path)?;
        let result = upsert_account(
            root,
            EmailAccountConfig {
                shared_user_ids: Some(Vec::new()),
                ..saved
            },
            None,
        );
        assert!(
            result.is_err(),
            "an unwritable registry must fail the upsert"
        );
        let account = crate::mission::channels::pull_communication_record_for_business_os(
            root,
            "communication_accounts",
            "email:team@example.test",
        )?
        .context("native account after failed registry write")?;
        assert_eq!(account["profile_json"]["ownerUserId"], "");
        assert_eq!(account["profile_json"]["shared_user_ids"], json!([]));
        Ok(())
    }
}
