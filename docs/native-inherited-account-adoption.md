# Existing native main gateway account

The native adoption adapter also represents the inherited main gateway credential as a native-held account. It reads the same private runtime route and verifies an unchanged route/credential snapshot without network calls. It exposes only the configured provider, never the selected model, endpoint, credential selector or key.

Missing/invalid credentials do not create placeholder accounts. Previously adopted main-route accounts retain their identities and withdrawals but become disabled when the credential is absent or the provider is switched. Unsupported/unreadable main-route metadata preserves its last observation and does not prevent independent subscription adoption. Credential presence never proves live catalog membership, quota or inference. Holder execution must still capture and revalidate its actual credential and route.

This follows the instance registry in PR511. It adds metadata only; live discovery, model selections, holder execution and the unified Models surface still require their adapters. No existing configuration, credential or selected model is changed.
