//! Declarative, fail-closed manifest authorization (§16.20/16.21).
//! Filters use the scope RowFilter grammar; {"roles":{role:filter},"default":filter}
//! selects a filter by authenticated role. No script/eval or ambient identity.
use super::{scope, Operation, Right};
use scope::{FieldMask, RowFilter};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Visibility {
    Roles(RoleVisibility),
    Filter(RowFilter),
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoleVisibility {
    pub roles: BTreeMap<String, RowFilter>,
    #[serde(default)]
    pub default: Option<RowFilter>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WriteGuard {
    pub command_only_fields: BTreeSet<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandRight {
    pub roles: BTreeSet<String>,
    #[serde(default)]
    pub assigned_users: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestRules {
    app_id: String,
    app_ver: String,
    collections: BTreeSet<String>,
    #[serde(default)]
    row_filters: BTreeMap<String, Visibility>,
    #[serde(default)]
    write_guard: BTreeMap<String, WriteGuard>,
    #[serde(default)]
    command_rights: BTreeMap<String, CommandRight>,
}
/// Constructed only by current Core command authorization, never deserialized.
#[derive(Debug, Clone)]
pub struct CommandPermit {
    pub(super) app_id: String,
    pub(super) actor_id: String,
    pub(super) role: String,
    pub(super) rights_ver: u64,
    pub(super) manifest_hash: String,
}
impl ManifestRules {
    /// Ownership is supplied by the trusted registry, not the manifest itself.
    pub fn from_manifest(
        app_id: &str,
        app_ver: &str,
        owned: BTreeSet<String>,
        manifest: &Value,
    ) -> scope::Result<Self> {
        if !manifest.is_object() {
            return Err("manifest must be an object".into());
        }
        fn part<T: serde::de::DeserializeOwned + Default>(v: &Value, k: &str) -> scope::Result<T> {
            match v.get(k) {
                None => Ok(T::default()),
                Some(v) => {
                    serde_json::from_value(v.clone()).map_err(|_| format!("invalid manifest {k}"))
                }
            }
        }
        let mut rules = Self {
            app_id: app_id.into(),
            app_ver: app_ver.into(),
            collections: owned,
            row_filters: part(manifest, "row_filters")?,
            write_guard: part(manifest, "write_guard")?,
            command_rights: part(manifest, "command_rights")?,
        };
        // Plain field names mean a top-level field; nested paths are JSON pointers.
        for guard in rules.write_guard.values_mut() {
            guard.command_only_fields = guard
                .command_only_fields
                .iter()
                .map(|p| {
                    if p.starts_with('/') {
                        p.clone()
                    } else {
                        format!("/{}", p.replace('~', "~0").replace('/', "~1"))
                    }
                })
                .collect();
        }
        rules.validate()?;
        Ok(rules)
    }
    pub fn app_id(&self) -> &str {
        &self.app_id
    }
    pub fn owns(&self, collection: &str) -> bool {
        self.collections.contains(collection)
    }
    pub fn collections(&self) -> &BTreeSet<String> {
        &self.collections
    }
    pub fn validate(&self) -> scope::Result<()> {
        if self.app_id.is_empty()
            || self.app_id.len() > 256
            || self.app_ver.is_empty()
            || self.app_ver.len() > 128
            || self.collections.len() > 4096
            || scope::canonical_json(&serde_json::to_value(self).map_err(|_| "invalid manifest")?)?
                .len()
                > 262144
        {
            return Err("invalid manifest identity or size".into());
        }
        for name in &self.collections {
            if name.is_empty() || name.len() > 256 {
                return Err("invalid collection".into());
            }
        }
        for (collection, filter) in &self.row_filters {
            if !self.owns(collection) {
                return Err("row filter outside registry ownership".into());
            }
            match filter {
                Visibility::Filter(f) => {
                    f.resolve("validation-actor")?;
                }
                Visibility::Roles(r) => {
                    if r.roles.len() > 128 || r.roles.keys().any(|r| r.is_empty() || r.len() > 128)
                    {
                        return Err("invalid role filter".into());
                    }
                    for f in r.roles.values().chain(r.default.iter()) {
                        f.resolve("validation-actor")?;
                    }
                }
            }
        }
        for (collection, guard) in &self.write_guard {
            if !self.owns(collection) {
                return Err("write guard outside registry ownership".into());
            }
            FieldMask::Fields {
                paths: guard.command_only_fields.clone(),
            }
            .normalize()?;
        }
        if self.command_rights.len() > 1024 {
            return Err("too many command rules".into());
        }
        for (pattern, right) in &self.command_rights {
            let prefix = pattern.strip_suffix('*').unwrap_or(pattern);
            if pattern.is_empty()
                || pattern.len() > 256
                || prefix.is_empty()
                || prefix.contains('*')
                || right.roles.len() > 128
                || right.roles.iter().any(|r| r.is_empty() || r.len() > 128)
            {
                return Err("invalid command rule".into());
            }
        }
        Ok(())
    }
    pub fn filter(&self, collection: &str, role: &str, actor: &str) -> scope::Result<RowFilter> {
        let rule = match self.row_filters.get(collection) {
            None => return Ok(RowFilter::All),
            Some(Visibility::Filter(f)) => f,
            Some(Visibility::Roles(r)) => match r.roles.get(role).or(r.default.as_ref()) {
                Some(f) => f,
                None => return Ok(RowFilter::None),
            },
        };
        rule.resolve(actor)
    }
    pub fn guard_direct_write(
        &self,
        collection: &str,
        before: Option<&Value>,
        after: Option<&Value>,
    ) -> scope::Result<()> {
        if let Some(guard) = self.write_guard.get(collection) {
            if guard
                .command_only_fields
                .iter()
                .any(|p| before.and_then(|v| v.pointer(p)) != after.and_then(|v| v.pointer(p)))
            {
                return Err("command_only_field".into());
            }
        }
        Ok(())
    }
    fn hash(&self) -> scope::Result<String> {
        Ok(scope::digest(&scope::canonical_json(
            &serde_json::to_value(self).map_err(|_| "invalid manifest")?,
        )?))
    }
    /// Explicit exact rule wins; otherwise longest prefix wins. An explicit
    /// denial never falls through to legacy app-write or a shorter prefix.
    pub fn authorize_command(
        &self,
        rights_ver: u64,
        actor: &str,
        role: &str,
        command: &str,
        assigned_by_core: bool,
        rights: &[Right],
    ) -> scope::Result<CommandPermit> {
        if actor.is_empty() || command.is_empty() || command.len() > 256 {
            return Err("forbidden".into());
        }
        let explicit = self.command_rights.get(command).or_else(|| {
            self.command_rights
                .iter()
                .filter_map(|(p, r)| {
                    p.strip_suffix('*')
                        .filter(|prefix| command.starts_with(prefix))
                        .map(|prefix| (prefix.len(), r))
                })
                .max_by_key(|(n, _)| *n)
                .map(|(_, r)| r)
        });
        let allowed = match explicit {
            Some(r) => r.roles.contains(role) || (r.assigned_users && assigned_by_core),
            None => rights.iter().any(|r| {
                r.role == role && self.owns(&r.collection) && r.ops.contains(&Operation::Write)
            }),
        };
        if !allowed {
            return Err("forbidden".into());
        }
        Ok(CommandPermit {
            app_id: self.app_id.clone(),
            actor_id: actor.into(),
            role: role.into(),
            rights_ver,
            manifest_hash: self.hash()?,
        })
    }
    pub fn validate_permit(
        &self,
        permit: &CommandPermit,
        rights_ver: u64,
        actor: &str,
        role: &str,
    ) -> scope::Result<()> {
        if permit.app_id != self.app_id
            || permit.rights_ver != rights_ver
            || permit.actor_id != actor
            || permit.role != role
            || permit.manifest_hash != self.hash()?
        {
            return Err("revoked".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn rules() -> ManifestRules {
        ManifestRules::from_manifest("app","1",["records".into()].into(),&json!({
            "row_filters":{"records":{"roles":{"admin":{"op":"all"},"user":{"op":"eq","path":"/owner","value":{"kind":"actor_id"}}}}},
            "write_guard":{"records":{"command_only_fields":["review","/nested/state"]}},
            "command_rights":{"review.*":{"roles":["admin"]},"review.submit":{"roles":[],"assigned_users":true},
                "review.delete":{"roles":[]}}
        })).unwrap()
    }
    #[test]
    fn row_filters_resolve_actor_and_unknown_roles_fail_closed() {
        let m = rules();
        let d = json!({"owner":"alice"});
        assert!(m.filter("records", "user", "alice").unwrap().matches(&d));
        assert!(!m.filter("records", "user", "bob").unwrap().matches(&d));
        assert!(!m.filter("records", "unknown", "alice").unwrap().matches(&d));
        assert!(m.filter("records", "admin", "bob").unwrap().matches(&d));
    }
    #[test]
    fn direct_writes_cannot_set_clear_or_remove_command_only_fields() {
        let m = rules();
        let old = json!({"review":"pending","nested":{"state":1},"title":"A"});
        assert!(m
            .guard_direct_write(
                "records",
                Some(&old),
                Some(&json!({"review":"pending","nested":{"state":1},"title":"B"}))
            )
            .is_ok());
        for after in [
            json!({"review":"done","nested":{"state":1}}),
            json!({"nested":{"state":1}}),
            json!({"review":"pending","nested":null}),
        ] {
            assert!(m
                .guard_direct_write("records", Some(&old), Some(&after))
                .is_err());
        }
        assert!(m.guard_direct_write("records", None, Some(&old)).is_err());
        assert!(m.guard_direct_write("records", Some(&old), None).is_err());
    }
    #[test]
    fn command_rights_are_checked_before_dispatch_and_exact_deny_wins() {
        let m = rules();
        assert!(m
            .authorize_command(2, "u", "admin", "review.other", false, &[])
            .is_ok());
        assert!(m
            .authorize_command(2, "u", "admin", "review.delete", true, &[])
            .is_err());
        assert!(m
            .authorize_command(2, "u", "user", "review.submit", false, &[])
            .is_err());
        let permit = m
            .authorize_command(2, "u", "user", "review.submit", true, &[])
            .unwrap();
        assert!(m.validate_permit(&permit, 2, "u", "user").is_ok());
        assert!(m.validate_permit(&permit, 3, "u", "user").is_err());
        assert!(m.validate_permit(&permit, 2, "other", "user").is_err());
    }
    #[test]
    fn malformed_manifest_never_silently_widens_access() {
        for bad in [
            json!({"row_filters":{"foreign":{"op":"all"}}}),
            json!({"row_filters":{"records":{"unknown":"all"}}}),
            json!({"command_rights":{"*":{"roles":["user"]}}}),
            json!({"write_guard":{"records":{"command_only_fields":["/bad~2"]}}}),
        ] {
            assert!(
                ManifestRules::from_manifest("app", "1", ["records".into()].into(), &bad).is_err()
            );
        }
    }
}
