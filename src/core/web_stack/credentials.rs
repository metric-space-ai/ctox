use ctox_web_stack::credentials::{
    CredentialReference, CredentialResolveError, CredentialResolver, SecretValue,
};
use std::path::Path;

/// An in-process capability bound to one trusted native root and exact refs.
/// Construct only at the native operation boundary, never from model-supplied
/// scope/name lists. This is not an externally callable secret-read tool.
pub(super) struct NativeCredentialResolver<'a> {
    root: &'a Path,
    allowed: &'a [(&'a str, &'a str)],
}

const RESEARCH_CREDENTIALS: &[(&str, &str)] = &[
    ("credentials", "LEADFEEDER_API_KEY"),
    ("credentials", "LEADFEEDER_LEGACY_API_TOKEN"),
];

impl<'a> NativeCredentialResolver<'a> {
    pub(super) fn for_research(root: &'a Path) -> Self {
        Self {
            root,
            allowed: RESEARCH_CREDENTIALS,
        }
    }
}

impl CredentialResolver for NativeCredentialResolver<'_> {
    fn resolve(
        &self,
        reference: &CredentialReference,
    ) -> Result<Option<SecretValue>, CredentialResolveError> {
        if !self
            .allowed
            .iter()
            .any(|(scope, name)| *scope == reference.scope && *name == reference.name)
        {
            return Err(CredentialResolveError::Denied);
        }
        crate::secrets::read_secret_value_optional(self.root, &reference.scope, &reference.name)
            .map(|value| value.map(SecretValue::new))
            .map_err(|error| match error {
                crate::secrets::SecretReadError::Unavailable => CredentialResolveError::Unavailable,
                crate::secrets::SecretReadError::DecryptionFailed => {
                    CredentialResolveError::DecryptionFailed
                }
                crate::secrets::SecretReadError::InvalidEncoding => {
                    CredentialResolveError::InvalidEncoding
                }
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn encrypted_lookup_is_scoped_and_distinguishes_absence_and_failure() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let resolver = NativeCredentialResolver::for_research(root.path());
        let reference = CredentialReference {
            scope: "credentials".into(),
            name: "LEADFEEDER_API_KEY".into(),
        };
        assert!(resolver.resolve(&reference)?.is_none());
        let canary = "synthetic-resolver-canary-not-a-provider-token";
        crate::secrets::write_secret_record(
            root.path(),
            &reference.scope,
            &reference.name,
            canary,
            None,
            serde_json::json!({}),
        )?;
        let value = resolver
            .resolve(&reference)?
            .expect("encrypted fixture exists");
        assert_eq!(value.expose_secret(), canary);
        assert!(!format!("{value:?}").contains(canary));
        for denied in [
            CredentialReference {
                scope: "other".into(),
                name: reference.name.clone(),
            },
            CredentialReference {
                scope: reference.scope.clone(),
                name: "UNRELATED_SECRET".into(),
            },
        ] {
            assert_eq!(
                resolver.resolve(&denied).unwrap_err(),
                CredentialResolveError::Denied
            );
        }
        let conn = rusqlite::Connection::open(crate::secrets::secret_store_path(root.path()))?;
        conn.execute("UPDATE ctox_secret_records SET ciphertext_b64 = 'invalid-base64-canary' WHERE scope = ?1 AND secret_name = ?2", rusqlite::params![reference.scope, reference.name])?;
        let error = resolver.resolve(&reference).unwrap_err();
        assert_eq!(error, CredentialResolveError::DecryptionFailed);
        assert!(!format!("{error:?}: {error}").contains("canary"));
        let bad_root = root.path().join("not-a-directory");
        std::fs::write(&bad_root, "fixture")?;
        assert_eq!(
            NativeCredentialResolver::for_research(&bad_root)
                .resolve(&reference)
                .unwrap_err(),
            CredentialResolveError::Unavailable
        );
        Ok(())
    }
}
