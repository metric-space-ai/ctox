//! Trusted-local operator intake. Passwords are accepted only on bounded stdin,
//! never as command-line arguments or in public account receipts.
use std::io::Read;
use std::path::Path;

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};

use super::email_accounts::{self, EmailAccountConfig};

const MAX_INPUT_BYTES: u64 = 16 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AccountInput {
    account: EmailAccountConfig,
    #[serde(default)]
    password: Option<String>,
}

fn read_input(reader: impl Read) -> Result<AccountInput> {
    let mut bytes = Vec::new();
    reader.take(MAX_INPUT_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_INPUT_BYTES {
        bail!("email account input exceeds 16384 bytes");
    }
    // Serde errors may quote an input value: never expose them for secret intake.
    serde_json::from_slice(&bytes).map_err(|_| anyhow::anyhow!("invalid email account JSON"))
}

pub(crate) fn run(root: &Path, args: &[String]) -> Result<Value> {
    match args.first().map(String::as_str) {
        Some("list") if args.len() == 1 => {
            let accounts = email_accounts::load_accounts(root)?;
            Ok(json!({"ok": true, "accounts": accounts.iter()
                .map(|account| email_accounts::public_json(root, account)).collect::<Vec<_>>()}))
        }
        Some("upsert") if args.len() == 2 && args[1] == "--stdin" => {
            let input = read_input(std::io::stdin().lock())?;
            let saved = email_accounts::upsert_account(root, input.account, input.password.as_deref())?;
            Ok(json!({"ok": true, "account": email_accounts::public_json(root, &saved)}))
        }
        Some("sync") => {
            let mut address = None;
            let mut limit = 10;
            let mut rest = &args[1..];
            while !rest.is_empty() {
                if rest.len() < 2 { bail!("email-account sync expects flag/value pairs"); }
                match rest[0].as_str() {
                    "--address" if address.is_none() => address = Some(rest[1].as_str()),
                    "--limit" => limit = rest[1].parse::<usize>().context("invalid sync limit")?,
                    _ => bail!("unsupported email-account sync flag"),
                }
                rest = &rest[2..];
            }
            let address = address.context("email-account sync requires --address")?;
            super::email_native::sync_registered_account(root, address, limit)
        }
        Some("trusted-authserv") => trusted_authserv(root, &args[1..]),
        Some("auth-results") if args.len() == 3 && args[1] == "--message-key" => {
            auth_results(root, &args[2])
        }
        _ => bail!("usage: ctox channel email-account list | upsert --stdin | sync --address <address> [--limit <1..100>] | trusted-authserv [--set <id,id> | --clear] | auth-results --message-key <key>"),
    }
}

/// Receiving servers whose `Authentication-Results` decide whether a mail's
/// From domain is authenticated (`sender_authentication`).
fn trusted_authserv(root: &Path, args: &[String]) -> Result<Value> {
    use super::sender_authentication::{trusted_authserv_ids, TRUSTED_AUTHSERV_IDS_KEY};
    use crate::inference::runtime_env;
    match args {
        [] => {}
        [flag, ids] if flag == "--set" => {
            let entries = ids
                .split(',')
                .map(str::trim)
                .filter(|id| !id.is_empty())
                .collect::<Vec<_>>();
            if entries.is_empty() {
                bail!("trusted-authserv --set needs at least one authserv-id");
            }
            if let Some(invalid) = entries
                .iter()
                .find(|id| !super::sender_authentication::valid_authserv_id(id))
            {
                bail!("not an authserv-id (no '=', ';', blanks or parentheses): {invalid}");
            }
            let ids = trusted_authserv_ids(ids);
            runtime_env::set_runtime_env_value(root, TRUSTED_AUTHSERV_IDS_KEY, &ids.join(","))?;
        }
        [flag] if flag == "--clear" => {
            runtime_env::clear_runtime_env_value(root, TRUSTED_AUTHSERV_IDS_KEY)?;
        }
        _ => bail!("usage: ctox channel email-account trusted-authserv [--set <id,id> | --clear]"),
    }
    let ids = trusted_authserv_ids(
        &runtime_env::env_or_config(root, TRUSTED_AUTHSERV_IDS_KEY).unwrap_or_default(),
    );
    Ok(json!({"ok": true, "trusted_authserv_ids": ids}))
}

/// The stored `Authentication-Results` of one inbound mail and whether they
/// authenticate its From domain under the current trusted servers.
fn auth_results(root: &Path, message_key: &str) -> Result<Value> {
    use super::sender_authentication::{
        sender_domain_authenticated, trusted_authserv_ids, TRUSTED_AUTHSERV_IDS_KEY,
    };
    let conn = crate::communication_store::open_channel_db(&crate::paths::core_db(root))?;
    let (sender, metadata_json): (String, String) = conn
        .query_row(
            "SELECT sender_address, metadata_json FROM communication_messages
             WHERE message_key = ?1 AND channel = 'email' AND direction = 'inbound'",
            [message_key],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .context("no inbound mail with this message key")?;
    let metadata = serde_json::from_str::<Value>(&metadata_json).unwrap_or(Value::Null);
    let results = metadata
        .get("authenticationResults")
        .cloned()
        .and_then(|value| serde_json::from_value::<Vec<String>>(value).ok());
    let trust_headers = metadata.get("trustHeaders").cloned().unwrap_or(Value::Null);
    let trusted = trusted_authserv_ids(
        &crate::inference::runtime_env::env_or_config(root, TRUSTED_AUTHSERV_IDS_KEY)
            .unwrap_or_default(),
    );
    let authenticated = results
        .as_deref()
        .is_some_and(|results| sender_domain_authenticated(results, &trusted, &sender));
    Ok(json!({
        "ok": true,
        "sender": sender,
        "headers_captured": results.is_some(),
        "authentication_results": results.unwrap_or_default(),
        "trusted_authserv_ids": trusted,
        "authenticated": authenticated,
        "trust_headers": trust_headers,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stdin_contract_is_bounded_and_does_not_echo_invalid_secret_values() {
        let input = br#"{"account":{"address":"lena@example.test","provider":"owa","username":"DOMAIN\\lena","owa_url":"https://mail.example.test/owa/"},"password":"private-fixture"}"#;
        let parsed = read_input(&input[..]).unwrap();
        assert_eq!(parsed.account.username, "DOMAIN\\lena");
        assert_eq!(parsed.password.as_deref(), Some("private-fixture"));
        let invalid =
            br#"{"account":{"address":"a@example.test"},"password":{"private-fixture":1}}"#;
        let error = read_input(&invalid[..]).err().unwrap().to_string();
        assert_eq!(error, "invalid email account JSON");
        assert!(!error.contains("private-fixture"));
        assert!(read_input(vec![b' '; MAX_INPUT_BYTES as usize + 1].as_slice()).is_err());
    }

    #[test]
    fn cli_rejects_secret_arguments_and_unknown_accounts() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let args = ["upsert", "--password", "private-fixture"].map(str::to_owned);
        let error = run(dir.path(), &args).unwrap_err().to_string();
        assert!(!error.contains("private-fixture"));
        let args = ["sync", "--address", "missing@example.test"].map(str::to_owned);
        assert!(run(dir.path(), &args).is_err());
        Ok(())
    }

    #[test]
    fn trusted_authserv_ids_are_set_normalized_and_cleared() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let root = dir.path();
        let args = |values: &[&str]| {
            values
                .iter()
                .map(|value| value.to_string())
                .collect::<Vec<_>>()
        };
        let set = run(
            root,
            &args(&[
                "trusted-authserv",
                "--set",
                " MX.Example.test, ,b.example.test",
            ]),
        )?;
        assert_eq!(
            set["trusted_authserv_ids"],
            json!(["mx.example.test", "b.example.test"])
        );
        assert!(run(root, &args(&["trusted-authserv", "--set", " , "])).is_err());
        for invalid in ["spf=pass", "mx.example.test;", "a b", "(comment)"] {
            assert!(run(root, &args(&["trusted-authserv", "--set", invalid])).is_err());
        }
        let cleared = run(root, &args(&["trusted-authserv", "--clear"]))?;
        assert_eq!(cleared["trusted_authserv_ids"], json!([]));
        Ok(())
    }
}
