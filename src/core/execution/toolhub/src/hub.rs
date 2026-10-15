//! ToolHub: catalog, credential-in connections and authenticated execution.
use std::collections::BTreeMap;
use std::sync::Arc;

use anyhow::{anyhow, bail, Context, Result};
use base64::Engine;
use serde::Serialize;
use serde_json::Value;

use crate::http::{HttpRequest, HttpTransport};
use crate::model::*;
use crate::ids;
use crate::store::{AccountStore, SecretBackend};
use crate::toolkits::ToolkitAdapter;

/// Result of a tool call as seen by the agent (never contains credentials).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum ToolOutcome {
    Success {
        data: Value,
    },
    ProviderError {
        status: u16,
        data: Value,
    },
    /// The user must (re)connect; surfaces as in-session element in phase 3.
    CredentialRequired {
        toolkit: String,
        account_id: Option<String>,
        /// Auth schemes the toolkit supports (for the connect element).
        auth_schemes: Vec<AuthSchemeKind>,
        reason: LocalizedText,
        /// Hint for agents/humans how to connect (CLI command).
        connect_hint: String,
        /// Extra inputs the credential card must ask for (DE+EN labels).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        fields: Vec<ConnectionField>,
    },
}

pub type Clock = Arc<dyn Fn() -> u64 + Send + Sync>;

pub fn system_clock() -> Clock {
    Arc::new(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    })
}

pub struct ToolHub<B: SecretBackend> {
    store: AccountStore<B>,
    transport: Arc<dyn HttpTransport>,
    toolkits: BTreeMap<String, Box<dyn ToolkitAdapter>>,
    clock: Clock,
    /// Builtins temporarily replaced by an active spec (restored on reload).
    shadowed: BTreeMap<String, Box<dyn ToolkitAdapter>>,
    /// Slugs currently served by runtime specs.
    spec_slugs: std::collections::BTreeSet<String>,
    spec_fingerprint: Option<String>,
}

/// Outcome of loading the active runtime specs into the hub.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct SpecReload {
    pub loaded: Vec<(String, String)>,
    pub failed: Vec<(String, String)>,
    pub replaced_builtins: Vec<String>,
}

impl<B: SecretBackend> ToolHub<B> {
    pub fn new(backend: B, transport: Arc<dyn HttpTransport>) -> Self {
        let mut hub = Self {
            store: AccountStore::new(backend),
            transport,
            toolkits: BTreeMap::new(),
            clock: system_clock(),
            shadowed: BTreeMap::new(),
            spec_slugs: Default::default(),
            spec_fingerprint: None,
        };
        for t in crate::toolkits::builtin() {
            hub.register(t);
        }
        hub
    }

    pub fn with_clock(mut self, clock: Clock) -> Self {
        self.clock = clock;
        self
    }

    pub fn register(&mut self, toolkit: Box<dyn ToolkitAdapter>) {
        self.toolkits.insert(toolkit.spec().slug.clone(), toolkit);
    }

    /// (Re)load the active adapter specs of this instance without a restart.
    /// Previously loaded specs are removed first; builtins they replaced come back.
    /// A spec may only replace a builtin when it says so (`replaces_builtin`).
    pub fn reload_specs(&mut self, specs: &crate::spec::SpecStore) -> Result<SpecReload> {
        for slug in std::mem::take(&mut self.spec_slugs) {
            self.toolkits.remove(&slug);
            if let Some(b) = self.shadowed.remove(&slug) {
                self.toolkits.insert(slug, b);
            }
        }
        let (adapters, mut failed) = specs.load_active()?;
        let mut report = SpecReload::default();
        for a in adapters {
            let slug = a.spec().slug.clone();
            if self.toolkits.contains_key(&slug) {
                if !a.replaces_builtin() {
                    failed.push((slug, "a built-in adapter with this name exists; set `replaces_builtin` to replace it".into()));
                    continue;
                }
                if let Some(b) = self.toolkits.remove(&slug) {
                    self.shadowed.insert(slug.clone(), b);
                }
                report.replaced_builtins.push(slug.clone());
            }
            report.loaded.push((slug.clone(), a.version().to_string()));
            self.spec_slugs.insert(slug.clone());
            self.toolkits.insert(slug, Box::new(a));
        }
        report.failed = failed;
        self.spec_fingerprint = Some(specs.fingerprint());
        Ok(report)
    }

    /// Reload only when an active pointer changed since the last load.
    pub fn reload_specs_if_changed(&mut self, specs: &crate::spec::SpecStore) -> Result<Option<SpecReload>> {
        if self.spec_fingerprint.as_deref() == Some(specs.fingerprint().as_str()) {
            return Ok(None);
        }
        self.reload_specs(specs).map(Some)
    }

    /// Whether `toolkit` is served by a runtime spec.
    pub fn is_spec_toolkit(&self, toolkit: &str) -> bool {
        self.spec_slugs.contains(toolkit)
    }

    pub fn store(&self) -> &AccountStore<B> {
        &self.store
    }

    fn now(&self) -> u64 {
        (self.clock)()
    }

    pub fn catalog(&self) -> Vec<&ToolkitSpec> {
        self.toolkits.values().map(|t| t.spec()).collect()
    }

    /// Extra connection inputs of `toolkit` (for connect dialogs / credential cards).
    pub fn connection_fields(&self, toolkit: &str) -> Result<Vec<ConnectionField>> {
        Ok(self.toolkit(toolkit)?.connection_fields())
    }

    /// Static-credential scheme for `toolkit` (API key, bearer token or Basic).
    pub fn credential_scheme(&self, toolkit: &str) -> Result<AuthSchemeKind> {
        let kinds: Vec<AuthSchemeKind> = self.toolkit(toolkit)?.spec().auth_schemes.iter().map(|s| s.kind()).collect();
        [AuthSchemeKind::ApiKey, AuthSchemeKind::Bearer, AuthSchemeKind::Basic, AuthSchemeKind::OAuthHeader]
            .into_iter()
            .find(|k| kinds.contains(k))
            .ok_or_else(|| anyhow!("toolkit `{toolkit}` only supports OAuth2"))
    }

    fn toolkit(&self, slug: &str) -> Result<&dyn ToolkitAdapter> {
        self.toolkits
            .get(slug)
            .map(|b| b.as_ref())
            .ok_or_else(|| anyhow!("unknown toolkit `{slug}`"))
    }

    /// Resolve `GITHUB_GET_REPO` or the CLI form `github.get_repo`.
    pub fn tool(&self, name: &str) -> Result<(&dyn ToolkitAdapter, &ToolSpec)> {
        let slug = match name.split_once('.') {
            // `linear.get_viewer` and `linear.LINEAR_GET_VIEWER` both mean LINEAR_GET_VIEWER.
            Some((toolkit, action)) => {
                let prefix = format!("{}_", toolkit.replace('-', "_")).to_ascii_uppercase();
                let action = action.to_ascii_uppercase();
                if action.starts_with(&prefix) {
                    action
                } else {
                    format!("{prefix}{action}")
                }
            }
            None => name.to_string(),
        };
        let slug = slug.as_str();
        for t in self.toolkits.values() {
            if let Some(spec) = t.spec().tools.iter().find(|s| s.slug == slug) {
                return Ok((t.as_ref(), spec));
            }
        }
        bail!("unknown tool `{slug}`")
    }

    fn supports(&self, toolkit: &str, kind: AuthSchemeKind) -> Result<bool> {
        Ok(self
            .toolkit(toolkit)?
            .spec()
            .auth_schemes
            .iter()
            .any(|s| s.kind() == kind))
    }

    /// Register or update an auth config (client secret goes to the secret store).
    pub fn upsert_auth_config(
        &self,
        config: &AuthConfig,
        client_secret: Option<&str>,
    ) -> Result<()> {
        if !self.supports(&config.toolkit, config.scheme)? {
            bail!(
                "toolkit `{}` does not support {:?}",
                config.toolkit,
                config.scheme
            );
        }
        if config.scheme == AuthSchemeKind::OAuth2
            && config.client_id.as_deref().unwrap_or("").is_empty()
        {
            bail!("OAuth2 auth config requires client_id");
        }
        self.store.save_auth_config(config, client_secret)
    }

    fn new_account(
        &self,
        config: &AuthConfig,
        project: &str,
        actor: &str,
        status: ConnectionStatus,
    ) -> ConnectedAccount {
        let now = self.now();
        ConnectedAccount {
            id: format!("ca_{}", ids::random_token(12)),
            toolkit: config.toolkit.clone(),
            auth_config_id: config.id.clone(),
            project: project.into(),
            created_by: actor.into(),
            status,
            scopes: config.scopes.clone(),
            expires_at: None,
            next_refresh_after: None,
            status_reason: None,
            created_at: now,
            updated_at: now,
            params: Default::default(),
        }
    }

    fn enabled_config(&self, auth_config_id: &str) -> Result<AuthConfig> {
        let cfg = self
            .store
            .auth_config(auth_config_id)?
            .ok_or_else(|| anyhow!("unknown auth config"))?;
        if !cfg.enabled {
            bail!("auth config is disabled");
        }
        Ok(cfg)
    }

    /// Connect with a credential obtained outside this crate (API key, bearer or
    /// OAuth access token, or `user:password` for Basic). No OAuth flow lives here.
    pub fn connect_with_credential(
        &self,
        auth_config_id: &str,
        project: &str,
        actor: &str,
        credential: &str,
    ) -> Result<ConnectedAccount> {
        self.connect_with_credential_params(auth_config_id, project, actor, credential, &BTreeMap::new())
    }

    /// Validate and normalize connection params for `toolkit` (fills derivable
    /// ones, rejects unknown keys, requires required fields, checks the host).
    pub fn resolve_connection_params(
        &self,
        toolkit: &str,
        credential: Option<&str>,
        token_extra: &BTreeMap<String, String>,
        given: &BTreeMap<String, String>,
    ) -> Result<BTreeMap<String, String>> {
        let adapter = self.toolkit(toolkit)?;
        let fields = adapter.connection_fields();
        let mut params: BTreeMap<String, String> = adapter
            .derive_connection_params(credential, token_extra)
            .into_iter()
            .filter(|(k, _)| fields.iter().any(|f| &f.key == k))
            .collect();
        for (k, v) in given {
            if !fields.iter().any(|f| &f.key == k) {
                bail!("unknown connection parameter `{k}` for `{toolkit}`");
            }
            if !v.trim().is_empty() {
                params.insert(k.clone(), v.trim().to_string());
            }
        }
        for f in &fields {
            if f.required && !params.contains_key(&f.key) {
                bail!("missing connection parameter `{}` ({})", f.key, f.label.en);
            }
        }
        adapter.connection_api_base(&params)?;
        Ok(params)
    }

    /// Like `connect_with_credential`, with per-connection params (instance URL, subdomain, ...).
    pub fn connect_with_credential_params(
        &self,
        auth_config_id: &str,
        project: &str,
        actor: &str,
        credential: &str,
        params: &BTreeMap<String, String>,
    ) -> Result<ConnectedAccount> {
        let cfg = self.enabled_config(auth_config_id)?;
        if credential.trim().is_empty() {
            bail!("empty credential");
        }
        if cfg.scheme == AuthSchemeKind::OAuthHeader
            && !credential.trim().split_once(':').is_some_and(|(k, t)| !k.is_empty() && !t.is_empty() && !k.contains('"') && !t.contains('"'))
        {
            bail!("this connection needs `key:token` (API key and token, colon-separated, no quotes)");
        }
        let params = self.resolve_connection_params(&cfg.toolkit, Some(credential.trim()), &BTreeMap::new(), params)?;
        let mut account = self.new_account(&cfg, project, actor, ConnectionStatus::Active);
        account.status = ConnectionStatus::Active;
        account.params = params;
        self.store.save(
            &account,
            Some(&AccountSecret {
                access_token: credential.into(),
                refresh_token: None,
                token_type: None,
            }),
        )?;
        Ok(account)
    }

    pub fn status(&self, account_id: &str) -> Result<Option<ConnectedAccount>> {
        self.store.get(account_id)
    }

    pub fn disconnect(&self, account_id: &str) -> Result<bool> {
        match self.store.get(account_id)? {
            Some(a) => {
                self.store.delete(&a)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    /// Refresh one account; on failure the account becomes EXPIRED.
    fn expire(&self, mut account: ConnectedAccount, reason: &str) -> Result<ConnectedAccount> {
        account.status = ConnectionStatus::Expired;
        account.status_reason = Some(reason.into());
        account.next_refresh_after = None;
        account.updated_at = self.now();
        self.store.save(&account, None)?;
        Ok(account)
    }

    fn select_account(
        &self,
        project: &str,
        toolkit: &str,
        account_id: Option<&str>,
    ) -> Result<Option<ConnectedAccount>> {
        let accounts = self.store.list()?;
        Ok(accounts.into_iter().find(|a| {
            a.project == project
                && a.toolkit == toolkit
                && account_id.is_none_or(|id| a.id == id)
                && matches!(
                    a.status,
                    ConnectionStatus::Active | ConnectionStatus::Expired
                )
        }))
    }

    fn authorize(
        &self,
        request: HttpRequest,
        account: &ConnectedAccount,
        secret: &AccountSecret,
    ) -> Result<HttpRequest> {
        let cfg = self
            .store
            .auth_config(&account.auth_config_id)?
            .context("auth config vanished")?;
        let spec = self.toolkit(&account.toolkit)?.spec();
        Ok(match cfg.scheme {
            AuthSchemeKind::OAuth2 | AuthSchemeKind::Bearer => {
                request.header("authorization", format!("Bearer {}", secret.access_token))
            }
            AuthSchemeKind::None => request,
            AuthSchemeKind::OAuthHeader => {
                let (key, token) = secret.access_token.split_once(':').ok_or_else(|| anyhow!("oauth_header credential must be `key:token`"))?;
                if key.is_empty() || token.is_empty() || key.contains('"') || token.contains('"') {
                    bail!("invalid oauth_header credential");
                }
                request.header("authorization", format!(r#"OAuth oauth_consumer_key="{key}", oauth_token="{token}""#))
            }
            AuthSchemeKind::Basic => request.header(
                "authorization",
                format!(
                    "Basic {}",
                    base64::engine::general_purpose::STANDARD.encode(&secret.access_token)
                ),
            ),
            AuthSchemeKind::ApiKey => {
                let (header, prefix) = spec
                    .auth_schemes
                    .iter()
                    .find_map(|s| match s {
                        AuthScheme::ApiKey {
                            header_name,
                            prefix,
                        } => Some((header_name.clone(), prefix.clone())),
                        _ => None,
                    })
                    .context("toolkit has no API key scheme")?;
                request.header(&header, format!("{prefix}{}", secret.access_token))
            }
        })
    }

    fn credential_required(&self, toolkit: &str, account_id: Option<String>) -> ToolOutcome {
        let auth_schemes = self
            .toolkit(toolkit)
            .map(|t| t.spec().auth_schemes.iter().map(AuthScheme::kind).collect())
            .unwrap_or_default();
        ToolOutcome::CredentialRequired {
            toolkit: toolkit.into(),
            fields: self.toolkit(toolkit).map(|t| t.connection_fields()).unwrap_or_default(),
            account_id,
            auth_schemes,
            connect_hint: match self
                .store
                .auth_configs()
                .unwrap_or_default()
                .into_iter()
                .find(|c| c.toolkit == toolkit && c.enabled)
            {
                Some(c) => format!("ctox toolhub connect --auth-config {}", c.id),
                None => format!(
                    "ctox toolhub config-set --toolkit {toolkit} --id <id> --scheme <oauth2|bearer> && ctox toolhub connect --auth-config <id>"
                ),
            },
            reason: LocalizedText::new(
                format!("Für „{toolkit}“ ist eine (erneute) Verbindung nötig."),
                format!("A (re)connection to \"{toolkit}\" is required."),
            ),
        }
    }

    /// Execute a tool within `project`. Tokens are injected here and never returned.
    /// On HTTP 401 the account is marked expired and a new credential is requested.
    pub fn execute(
        &self,
        project: &str,
        tool_slug: &str,
        args: &Value,
        account_id: Option<&str>,
    ) -> Result<ToolOutcome> {
        let (adapter, spec) = self.tool(tool_slug)?;
        let tool_slug = spec.slug.as_str();
        // Validate arguments before touching credentials (toolkits with
        // connection params may need them to build the request at all).
        let fields = adapter.connection_fields();
        if let Err(e) = adapter.build_request(tool_slug, args) {
            if fields.is_empty() {
                return Err(e);
            }
        }
        let toolkit = spec.toolkit.clone();
        if adapter.spec().auth_schemes == [AuthScheme::None] {
            // Public API: no account, no credential. Only allowed with an egress
            // policy (hot-plug specs), so the allowed-hosts rule still applies.
            let policy = adapter.egress_policy().ok_or_else(|| anyhow!("credential-free toolkit `{toolkit}` has no egress policy"))?;
            let request = adapter.build_request(tool_slug, args)?;
            policy.check_url(&request.url)?;
            let response = self.transport.send_restricted(request)?;
            return Ok(if (200..300).contains(&response.status) {
                match adapter.parse_response(tool_slug, &response) {
                    Ok(data) => ToolOutcome::Success { data },
                    Err(e) => ToolOutcome::ProviderError { status: response.status, data: serde_json::json!({"error": e.to_string()}) },
                }
            } else {
                let text: String = String::from_utf8_lossy(&response.body).chars().take(500).collect();
                ToolOutcome::ProviderError { status: response.status, data: response.json().ok().filter(|v| !v.is_null()).unwrap_or_else(|| serde_json::json!({"error": text})) }
            });
        }
        let Some(account) = self.select_account(project, &toolkit, account_id)? else {
            return Ok(self.credential_required(&toolkit, None));
        };
        if fields.iter().any(|f| f.required && !account.params.contains_key(&f.key)) {
            return Ok(self.credential_required(&toolkit, Some(account.id)));
        }
        let mut args = args.clone();
        if let Some(obj) = args.as_object_mut() {
            for (k, v) in adapter.connection_args(&account.params) {
                obj.entry(k).or_insert(Value::String(v));
            }
        }
        let mut request = adapter.build_request(tool_slug, &args)?;
        if let (Some(base), Some(default)) = (
            adapter.connection_api_base(&account.params)?,
            adapter.default_api_base(),
        ) {
            if let Some(rest) = request.url.strip_prefix(default.as_str()) {
                if rest.is_empty() || rest.starts_with('/') || rest.starts_with('?') {
                    request.url = format!("{base}{rest}");
                }
            }
        }
        if account.status == ConnectionStatus::Expired
            || account.expires_at.is_some_and(|t| t <= self.now())
        {
            return Ok(self.credential_required(&toolkit, Some(account.id)));
        }
        {
            let secret = self
                .store
                .secret(&account)?
                .ok_or_else(|| anyhow!("account has no credential"))?;
            let authorized = self.authorize(request.clone(), &account, &secret)?;
            let response = match adapter.egress_policy() {
                Some(policy) => {
                    policy.check_url(&authorized.url)?;
                    self.transport.send_restricted(authorized)?
                }
                None => self.transport.send(authorized)?,
            };
            if response.status == 401 {
                self.expire(account.clone(), "provider rejected credential (401)")?;
                return Ok(self.credential_required(&toolkit, Some(account.id)));
            }
            if (200..300).contains(&response.status) {
                // Providers like Slack report errors as HTTP 200 + ok:false;
                // adapters surface those as parse errors -> provider error.
                return Ok(match adapter.parse_response(tool_slug, &response) {
                    Ok(data) => ToolOutcome::Success { data },
                    Err(e) => ToolOutcome::ProviderError {
                        status: response.status,
                        data: serde_json::json!({"error": e.to_string()}),
                    },
                });
            }
            return Ok(ToolOutcome::ProviderError {
                status: response.status,
                data: response.json().ok().filter(|v| !v.is_null()).unwrap_or_else(|| {
                    let text = String::from_utf8_lossy(&response.body);
                    let text: String = text.chars().take(500).collect();
                    if text.trim().is_empty() {
                        serde_json::json!({"error": format!("HTTP {} (empty response body)", response.status)})
                    } else {
                        serde_json::json!({"error": text})
                    }
                }),
            });
        }
    }
}
