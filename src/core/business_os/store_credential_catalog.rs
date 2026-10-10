// Origin: CTOX
// License: Apache-2.0

//! Metadata only. Admission remains the caller's workspace SecretsManage gate.
//! Legacy name-only rows stay runtime-scoped; never flatten other scopes into
//! them, since old clients discard identity fields before issuing mutations.

use std::{collections::BTreeMap, path::Path};

use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CredentialReference {
    scope: String,
    name: String,
}

/// JSON string-tuple encoding is injective even for names/scopes containing
/// slashes, separators or Unicode. This is identity, not an authority token.
fn credential_id(scope: &str, name: &str) -> String {
    json!([scope, name]).to_string()
}

pub(super) fn validate_runtime_target(
    name: &str,
    scope: Option<&str>,
    id: Option<&str>,
    reference: Option<&CredentialReference>,
) -> anyhow::Result<()> {
    let runtime_scope = crate::secrets::credential_scope();
    anyhow::ensure!(
        scope.is_none_or(|scope| scope == runtime_scope)
            && reference.is_none_or(|reference| reference.scope == runtime_scope),
        "unsupported credential scope: this command only mutates runtime credentials"
    );
    anyhow::ensure!(
        reference.is_none_or(|reference| reference.name == name)
            && id.is_none_or(|id| id == credential_id(runtime_scope, name)),
        "credential target identity mismatch"
    );
    Ok(())
}

fn metadata_entry(
    scope: &str,
    name: &str,
    description: Option<&str>,
    is_set: bool,
    updated_at: Option<&str>,
    source: &str,
) -> Value {
    let unsupported = if scope != crate::secrets::credential_scope() {
        Some("unsupported_scope")
    } else if name != name.trim() || !super::is_valid_credential_key(name) {
        Some("invalid_runtime_key")
    } else {
        None
    };
    json!({
        "id": credential_id(scope, name),
        "scope": scope,
        "name": name,
        "reference": { "scope": scope, "name": name },
        "description": description,
        "is_set": is_set,
        "status": if is_set { "set" } else { "unset" },
        "updated_at": updated_at,
        "source": source,
        // Supported operations, NOT permission grants. The native command gate
        // still checks SecretsManage for each request, including deletions.
        "write_support": {
            "put": unsupported.is_none(),
            "delete": unsupported.is_none(),
            "reason": unsupported,
        },
    })
}

pub(super) fn list(root: &Path) -> anyhow::Result<Value> {
    let scope = crate::secrets::credential_scope();
    // No decryption. Deliberately never serialize SecretRecordView wholesale:
    // arbitrary metadata_json can itself contain sensitive user-supplied data.
    let records = crate::secrets::list_secret_records(root, None)?;
    let by_name: BTreeMap<_, _> = records
        .iter()
        .filter(|record| record.scope == scope)
        .map(|record| (record.secret_name.as_str(), record))
        .collect();
    let known: BTreeMap<_, _> = crate::secrets::known_credential_keys()
        .iter()
        .copied()
        .collect();
    let catalog: Vec<Value> = crate::secrets::known_credential_keys()
        .iter()
        .map(|(name, description)| {
            let record = by_name.get(*name);
            json!({
                "name": name,
                "description": description,
                "is_set": record.is_some(),
                "updated_at": record.map(|record| record.updated_at.as_str()),
            })
        })
        .collect();
    let extra: Vec<Value> = records
        .iter()
        .filter(|record| record.scope == scope && !known.contains_key(record.secret_name.as_str()))
        .map(|record| {
            json!({
                "name": record.secret_name,
                "description": record.description,
                "is_set": true,
                "updated_at": record.updated_at,
            })
        })
        .collect();

    // Include known unset runtime slots, then overlay every actual stored tuple.
    // Sorting by (scope, name), not updated_at, also makes rotation order-stable.
    let mut entries = BTreeMap::new();
    for (name, description) in crate::secrets::known_credential_keys() {
        entries.insert(
            (scope.to_string(), name.to_string()),
            metadata_entry(scope, name, Some(description), false, None, "catalog"),
        );
    }
    for record in &records {
        let catalog_description = (record.scope == scope)
            .then(|| known.get(record.secret_name.as_str()).copied())
            .flatten();
        entries.insert(
            (record.scope.clone(), record.secret_name.clone()),
            metadata_entry(
                &record.scope,
                &record.secret_name,
                record.description.as_deref().or(catalog_description),
                true,
                Some(&record.updated_at),
                if catalog_description.is_some() {
                    "catalog"
                } else {
                    "extra"
                },
            ),
        );
    }
    Ok(json!({
        "ok": true,
        "scope": scope,
        "catalog": catalog,
        "extra": extra,
        "scoped_catalog": {
            "schema": "ctox.credentials.scoped-metadata.v1",
            "complete": true,
            "entries": entries.into_values().collect::<Vec<_>>(),
        },
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::business_os::store;

    fn fixture() -> anyhow::Result<tempfile::TempDir> {
        let root = tempfile::tempdir()?;
        store::tests::seed_test_business_os_app_root(root.path())?;
        store::tests::seed_business_user(root.path(), "catalog-admin", "admin")?;
        store::tests::seed_business_user(root.path(), "catalog-user", "user")?;
        Ok(root)
    }

    fn command(root: &Path, actor: &str, kind: &str, payload: Value) -> anyhow::Result<Value> {
        store::accept_rxdb_business_command(
            root,
            json!({
                "id": format!("cmd_scoped_metadata_{}", uuid::Uuid::new_v4()),
                "module": "credentials",
                "type": kind,
                "payload": payload,
                "client_context": { "actor": { "id": actor } },
            }),
        )
    }

    fn seed_secret(root: &Path, scope: &str, name: &str, value: &str) -> anyhow::Result<()> {
        crate::secrets::write_secret_record(
            root,
            scope,
            name,
            value,
            Some("Account credential metadata".into()),
            json!({ "password": "ARBITRARY_METADATA_MUST_NOT_BE_PROJECTED" }),
        )?;
        Ok(())
    }

    #[test]
    fn scoped_credential_catalog_lists_all_scopes_without_name_collisions() -> anyhow::Result<()> {
        let root = fixture()?;
        let runtime = crate::secrets::credential_scope();
        for scope in [runtime, "account:crew", "system", "custom/tenant/provider"] {
            seed_secret(root.path(), scope, "OPENAI_API_KEY", "SCOPED_SECRET_VALUE")?;
        }
        seed_secret(root.path(), runtime, "CUSTOM_RUNTIME_KEY", "CUSTOM_VALUE")?;
        seed_secret(
            root.path(),
            "account:crew",
            "ANTHROPIC_API_KEY",
            "SCOPED_SECRET_VALUE",
        )?;
        seed_secret(
            root.path(),
            "account:crew",
            "login/password",
            "PASSWORD_VALUE",
        )?;
        let outcome = command(root.path(), "catalog-admin", "ctox.secret.list", json!({}))?;
        assert_eq!(outcome["status"], "completed");
        let result = &outcome["result"];
        assert_eq!(result["scope"], runtime);
        assert_eq!(
            result["scoped_catalog"]["schema"],
            "ctox.credentials.scoped-metadata.v1"
        );
        assert_eq!(result["scoped_catalog"]["complete"], true);
        let entries = result["scoped_catalog"]["entries"].as_array().unwrap();
        let same_name: Vec<_> = entries
            .iter()
            .filter(|entry| entry["name"] == "OPENAI_API_KEY")
            .collect();
        assert_eq!(same_name.len(), 4);
        let ids: std::collections::BTreeSet<_> = same_name
            .iter()
            .map(|entry| entry["id"].as_str().unwrap())
            .collect();
        assert_eq!(ids.len(), 4);
        assert_eq!(
            entries
                .iter()
                .filter(|entry| entry["is_set"] == true)
                .count(),
            7
        );
        for entry in entries {
            assert_eq!(
                entry["id"],
                credential_id(
                    entry["scope"].as_str().unwrap(),
                    entry["name"].as_str().unwrap()
                )
            );
            assert_eq!(
                entry["reference"],
                json!({ "scope": entry["scope"], "name": entry["name"] })
            );
            assert_eq!(entry["write_support"]["put"], entry["scope"] == runtime);
            assert_eq!(entry["write_support"]["delete"], entry["scope"] == runtime);
        }
        // Old name-only clients see neither duplicate names nor non-runtime targets.
        let extra = result["extra"].as_array().unwrap();
        assert_eq!(extra.len(), 1);
        assert_eq!(extra[0]["name"], "CUSTOM_RUNTIME_KEY");
        let unset = entries
            .iter()
            .find(|entry| entry["name"] == "ANTHROPIC_API_KEY" && entry["scope"] == runtime)
            .unwrap();
        assert_eq!(unset["is_set"], false);
        assert_eq!(unset["status"], "unset");
        assert_eq!(unset["updated_at"], Value::Null);
        let encoded = serde_json::to_string(&outcome)?;
        for forbidden in [
            "SCOPED_SECRET_VALUE",
            "CUSTOM_VALUE",
            "PASSWORD_VALUE",
            "ARBITRARY_METADATA_MUST_NOT_BE_PROJECTED",
            "ciphertext_b64",
            "nonce_b64",
            "metadata_json",
        ] {
            assert!(!encoded.contains(forbidden));
        }
        let conn = store::open_store(root.path())?;
        let persisted = store::outbound_load_required(
            &conn,
            "business_commands",
            outcome["command_id"].as_str().unwrap(),
            "command",
        )?;
        assert!(!serde_json::to_string(&persisted)?.contains("SCOPED_SECRET_VALUE"));
        assert!(!serde_json::to_string(&persisted)?
            .contains("ARBITRARY_METADATA_MUST_NOT_BE_PROJECTED"));
        Ok(())
    }

    #[test]
    fn scoped_credential_catalog_identity_is_stable_and_delimiter_safe() -> anyhow::Result<()> {
        let root = fixture()?;
        let pairs = [("a/b", "c"), ("a", "b/c"), ("非標準", "name\"[]")];
        for (scope, name) in pairs {
            seed_secret(root.path(), scope, name, "FIRST_VALUE")?;
        }
        let ids = |value: &Value| -> Vec<String> {
            value["scoped_catalog"]["entries"]
                .as_array()
                .unwrap()
                .iter()
                .map(|entry| entry["id"].as_str().unwrap().to_owned())
                .collect()
        };
        let before = list(root.path())?;
        seed_secret(root.path(), "a/b", "c", "ROTATED_VALUE")?;
        let after = list(root.path())?;
        assert_eq!(ids(&before), ids(&after));
        assert_ne!(credential_id("a/b", "c"), credential_id("a", "b/c"));
        for (scope, name) in pairs {
            let decoded: [String; 2] = serde_json::from_str(&credential_id(scope, name))?;
            assert_eq!(decoded, [scope.to_string(), name.to_string()]);
        }
        Ok(())
    }

    #[test]
    fn scoped_credential_catalog_does_not_decrypt_and_marks_unsupported_keys() -> anyhow::Result<()>
    {
        let root = fixture()?;
        let runtime = crate::secrets::credential_scope();
        seed_secret(root.path(), runtime, "not-an-env-key", "PRIVATE_VALUE")?;
        seed_secret(root.path(), "system", "ROOT_KEY", "SYSTEM_PRIVATE_VALUE")?;
        // Corrupt ciphertext only in this disposable fixture: metadata listing
        // must not need to read or decrypt it, or silently omit the stored row.
        let conn = rusqlite::Connection::open(crate::secrets::secret_store_path(root.path()))?;
        conn.execute("UPDATE ctox_secret_records SET ciphertext_b64 = 'invalid-ciphertext' WHERE scope = 'system'", [])?;
        drop(conn);
        assert!(crate::secrets::read_secret_value(root.path(), "system", "ROOT_KEY").is_err());
        let result = list(root.path())?;
        let entries = result["scoped_catalog"]["entries"].as_array().unwrap();
        let invalid = entries
            .iter()
            .find(|entry| entry["name"] == "not-an-env-key")
            .unwrap();
        assert_eq!(
            invalid["write_support"],
            json!({"put": false, "delete": false, "reason": "invalid_runtime_key"})
        );
        let system = entries
            .iter()
            .find(|entry| entry["scope"] == "system")
            .unwrap();
        assert_eq!(system["status"], "set");
        assert_eq!(
            system["write_support"],
            json!({"put": false, "delete": false, "reason": "unsupported_scope"})
        );
        Ok(())
    }

    #[test]
    fn scoped_credential_catalog_user_cannot_list_put_or_delete() -> anyhow::Result<()> {
        let root = fixture()?;
        let runtime = crate::secrets::credential_scope();
        seed_secret(root.path(), runtime, "OPENAI_API_KEY", "RUNTIME_ORIGINAL")?;
        seed_secret(
            root.path(),
            "private-account",
            "PRIVATE_NAME",
            "ACCOUNT_ORIGINAL",
        )?;
        for kind in ["ctox.secret.list", "ctox.secret.put", "ctox.secret.delete"] {
            let payload = if kind == "ctox.secret.put" {
                json!({"name": "OPENAI_API_KEY", "value": "DENIED_REPLACEMENT"})
            } else if kind == "ctox.secret.delete" {
                json!({"name": "OPENAI_API_KEY"})
            } else {
                json!({})
            };
            let denied = command(root.path(), "catalog-user", kind, payload)?;
            assert_eq!(denied["status"], "failed");
            let serialized = serde_json::to_string(&denied)?;
            for hidden in [
                "PRIVATE_NAME",
                "private-account",
                "ACCOUNT_ORIGINAL",
                "RUNTIME_ORIGINAL",
                "DENIED_REPLACEMENT",
                "scoped_catalog",
            ] {
                assert!(!serialized.contains(hidden));
            }
        }
        assert_eq!(
            crate::secrets::read_secret_value(root.path(), runtime, "OPENAI_API_KEY")?,
            "RUNTIME_ORIGINAL"
        );
        assert_eq!(
            crate::secrets::read_secret_value(root.path(), "private-account", "PRIVATE_NAME")?,
            "ACCOUNT_ORIGINAL"
        );
        Ok(())
    }

    #[test]
    fn scoped_credential_catalog_rejects_wrong_targets_without_mutation() -> anyhow::Result<()> {
        let root = fixture()?;
        let runtime = crate::secrets::credential_scope();
        let name = "OPENAI_API_KEY";
        seed_secret(root.path(), runtime, name, "RUNTIME_ORIGINAL")?;
        seed_secret(root.path(), "account:crew", name, "ACCOUNT_ORIGINAL")?;
        let targets = [
            json!({"scope": "account:crew"}),
            json!({"scope": "system"}),
            json!({"scope": ""}),
            json!({"scope": true}),
            json!({"reference": {"scope": "account:crew", "name": name}}),
            json!({"reference": {"scope": runtime, "name": "DIFFERENT_KEY"}}),
            json!({"id": credential_id("account:crew", name)}),
            json!({"id": credential_id(runtime, "DIFFERENT_KEY")}),
            json!({"id": "not-a-catalog-id"}),
            json!({"reference": {"scope": runtime, "name": name, "unknown": true}}),
            json!({"secret_ref": "account:crew/OPENAI_API_KEY"}),
        ];
        for kind in ["ctox.secret.put", "ctox.secret.delete"] {
            for target in &targets {
                let mut payload = target.clone();
                payload["name"] = json!(name);
                if kind == "ctox.secret.put" {
                    payload["value"] = json!("REJECTED_VALUE");
                }
                let rejected = command(root.path(), "catalog-admin", kind, payload)?;
                assert_eq!(rejected["status"], "failed", "{kind} accepted {target}");
                assert!(!serde_json::to_string(&rejected)?.contains("REJECTED_VALUE"));
                assert_eq!(
                    crate::secrets::read_secret_value(root.path(), runtime, name)?,
                    "RUNTIME_ORIGINAL"
                );
                assert_eq!(
                    crate::secrets::read_secret_value(root.path(), "account:crew", name)?,
                    "ACCOUNT_ORIGINAL"
                );
                assert_eq!(
                    crate::secrets::list_secret_records(root.path(), None)?.len(),
                    2
                );
            }
        }
        Ok(())
    }

    #[test]
    fn scoped_credential_catalog_runtime_writes_and_env_consumers_stay_scoped() -> anyhow::Result<()>
    {
        let root = fixture()?;
        let runtime = crate::secrets::credential_scope();
        let name = "OPENAI_API_KEY";
        seed_secret(root.path(), "account:crew", name, "ACCOUNT_ORIGINAL")?;
        let legacy = command(
            root.path(),
            "catalog-admin",
            "ctox.secret.put",
            json!({"name": name, "value": "LEGACY_VALUE"}),
        )?;
        assert_eq!(legacy["status"], "completed");
        let identity = credential_id(runtime, name);
        let target = json!({"scope": runtime, "name": name});
        let modern = command(
            root.path(),
            "catalog-admin",
            "ctox.secret.put",
            json!({
                "name": name, "scope": runtime, "id": identity, "reference": target, "value": "ROTATED_RUNTIME_VALUE"
            }),
        )?;
        assert_eq!(modern["status"], "completed");
        let mut env = BTreeMap::new();
        crate::secrets::merge_credentials_into_env_map(root.path(), &mut env);
        assert_eq!(
            env.get(name).map(String::as_str),
            Some("ROTATED_RUNTIME_VALUE")
        );
        let removed = command(
            root.path(),
            "catalog-admin",
            "ctox.secret.delete",
            json!({
                "name": name, "scope": runtime, "id": identity, "reference": target
            }),
        )?;
        assert_eq!(removed["status"], "completed");
        assert!(!crate::secrets::secret_exists(root.path(), runtime, name)?);
        assert_eq!(
            crate::secrets::read_secret_value(root.path(), "account:crew", name)?,
            "ACCOUNT_ORIGINAL"
        );
        let mut env = BTreeMap::new();
        crate::secrets::merge_credentials_into_env_map(root.path(), &mut env);
        assert!(!env.contains_key(name));
        Ok(())
    }
}
