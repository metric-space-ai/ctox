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

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{bail, Context, Result};

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
    if let Some(existing) = accounts
        .iter_mut()
        .find(|item| item.address == config.address)
    {
        // Owner bleibt stabil; leere Felder überschreiben Bestehendes nicht.
        if config.owner_user_id.trim().is_empty() {
            config.owner_user_id = existing.owner_user_id.clone();
        }
        *existing = config.clone();
    } else {
        accounts.push(config.clone());
    }
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

    // Konto sofort in communication_accounts sichtbar machen (Mail-App-Liste).
    let db_path = root.join("runtime/ctox.sqlite3");
    {
        let mut conn = crate::communication_store::open_channel_db(&db_path)?;
        let profile_json = json!({
            "imapHost": config.imap_host,
            "imapPort": config.imap_port,
            "smtpHost": config.smtp_host,
            "smtpPort": config.smtp_port,
            "username": config.username,
            "ewsUrl": config.ews_url,
            "owaUrl": config.owa_url,
            "ewsUsername": config.username,
            "ownerUserId": config.owner_user_id,
            "displayName": config.display_name,
            "source": "mail-app-account",
        });
        crate::mission::channels::upsert_communication_account(
            &mut conn,
            &format!("email:{}", config.address),
            "email",
            &config.address,
            &config.provider,
            profile_json,
        )?;
    }
    Ok(config)
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
        "has_password": has_password,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exchange_account_roundtrip_preserves_other_accounts_and_hides_password() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let root = dir.path();
        std::fs::create_dir_all(root.join("runtime"))?;
        let crew = upsert_account(
            root,
            EmailAccountConfig {
                address: "crew@example.test".into(),
                provider: "imap".into(),
                imap_host: "crew.example.test".into(),
                owner_user_id: "crew-owner".into(),
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
        assert_eq!(settings["CTO_EMAIL_EWS_URL"], lena.ews_url);
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
        assert_eq!(profile["displayName"], "");
        assert_eq!(profile["owaUrl"], "https://lena.example.test/owa/");
        Ok(())
    }

    #[test]
    fn upsert_normalizes_and_keeps_owner() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let root = dir.path();
        std::fs::create_dir_all(root.join("runtime"))?;
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
}
