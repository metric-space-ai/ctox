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
        _ => bail!("usage: ctox channel email-account list | upsert --stdin | sync --address <address> [--limit <1..100>]"),
    }
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
}
