//! Bilateral app IPC declarations. User policy is additionally checked by Core.
use anyhow::{ensure, Context, Result};
use serde_json::Value;

pub fn validate(manifest: &Value) -> Result<()> {
    for key in ["commands_out", "commands_in"] {
        if let Some(value) = manifest.get(key) {
            let rules = value.as_array().context("invalid: command declarations")?;
            for rule in rules {
                crate::string(rule, "type")?;
                if key == "commands_out" {
                    crate::string(rule, "app")?;
                } else {
                    ensure!(
                        rule["from"] == "*"
                            || rule["from"].as_array().is_some_and(|v| !v.is_empty()
                                && v.iter().all(|a| a.as_str().is_some_and(|s| !s.is_empty()))),
                        "invalid: commands_in source"
                    );
                }
            }
        }
    }
    Ok(())
}

pub fn target<'a>(manifest: &'a Value, kind: &str, options: &'a Value) -> Result<&'a str> {
    if let Some(module) = options.get("module") {
        return module
            .as_str()
            .filter(|s| !s.is_empty())
            .context("invalid: command module");
    }
    if manifest["handlers"].get(kind).is_some() {
        return crate::string(manifest, "app_id");
    }
    let targets: std::collections::BTreeSet<_> = manifest["commands_out"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|rule| rule["type"] == kind)
        .filter_map(|rule| rule["app"].as_str())
        .collect();
    ensure!(
        targets.len() == 1,
        "forbidden: command target missing or ambiguous"
    );
    Ok(*targets.first().unwrap())
}

pub fn check(sender: &Value, receiver: &Value, kind: &str, options: &Value) -> Result<()> {
    let options = options.as_object().context("invalid: command options")?;
    ensure!(
        options.keys().all(|k| matches!(
            k.as_str(),
            "id" | "record_id" | "module" | "client_context" | "on_complete" | "on_failed"
        )),
        "invalid: command option"
    );
    let from = crate::string(sender, "app_id")?;
    let to = crate::string(receiver, "app_id")?;
    ensure!(
        receiver["handlers"].get(kind).is_some(),
        "forbidden: target handler missing"
    );
    if from != to {
        ensure!(
            sender["commands_out"]
                .as_array()
                .is_some_and(|rules| rules.iter().any(|r| r["app"] == to && r["type"] == kind)),
            "forbidden: sender commands_out missing"
        );
        ensure!(
            receiver["commands_in"]
                .as_array()
                .is_some_and(|rules| rules.iter().any(|r| r["type"] == kind
                    && (r["from"] == "*"
                        || r["from"]
                            .as_array()
                            .is_some_and(|apps| apps.iter().any(|a| a == from))))),
            "forbidden: receiver commands_in missing"
        );
    }
    for key in ["on_complete", "on_failed"] {
        if let Some(callback) = options.get(key) {
            ensure!(
                callback
                    .as_str()
                    .is_some_and(|t| sender["handlers"].get(t).is_some()),
                "forbidden: callback must be a handler of the triggering app"
            );
        }
    }
    Ok(())
}
