//! ToolHub: catalog, connection lifecycle, refresh and authenticated execution.
use std::collections::BTreeMap;
use std::sync::Arc;

use anyhow::{anyhow, bail, Context, Result};
use base64::Engine;
use serde::Serialize;
use serde_json::Value;

use crate::http::{HttpRequest, HttpTransport};
use crate::model::*;
use crate::oauth2::{self, CallbackResult, ClientCredentials, RedirectTarget, TokenSet};
use crate::pkce;
use crate::refresh;
use crate::store::{AccountStore, PendingAuthorization, SecretBackend};
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

/// Pending connections expire after 15 minutes.
pub const PENDING_TTL_SECS: u64 = 900;

pub struct ToolHub<B: SecretBackend> {
    store: AccountStore<B>,
    transport: Arc<dyn HttpTransport>,
    toolkits: BTreeMap<String, Box<dyn ToolkitAdapter>>,
    redirect: RedirectTarget,
    clock: Clock,
    refresh_lead: u64,
}

impl<B: SecretBackend> ToolHub<B> {
    pub fn new(backend: B, transport: Arc<dyn HttpTransport>, redirect: RedirectTarget) -> Self {
        let mut hub = Self {
            store: AccountStore::new(backend),
            transport,
            toolkits: BTreeMap::new(),
            redirect,
            clock: system_clock(),
            refresh_lead: refresh::DEFAULT_REFRESH_LEAD_SECS,
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

    pub fn store(&self) -> &AccountStore<B> {
        &self.store
    }

    fn now(&self) -> u64 {
        (self.clock)()
    }

    pub fn catalog(&self) -> Vec<&ToolkitSpec> {
        self.toolkits.values().map(|t| t.spec()).collect()
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
            Some((toolkit, action)) => format!("{}_{}", toolkit, action).to_ascii_uppercase(),
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

    fn oauth_scheme(&self, toolkit: &str) -> Result<OAuth2Scheme> {
        self.toolkit(toolkit)?
            .spec()
            .auth_schemes
            .iter()
            .find_map(|s| match s {
                AuthScheme::OAuth2(o) => Some(o.clone()),
                _ => None,
            })
            .ok_or_else(|| anyhow!("toolkit `{toolkit}` has no OAuth2 scheme"))
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
            id: format!("ca_{}", pkce::random_token(12)),
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

    /// Start an interactive OAuth2 connection; returns the consent URL.
    pub fn connect_oauth(
        &self,
        auth_config_id: &str,
        project: &str,
        actor: &str,
    ) -> Result<ConnectionRequest> {
        let cfg = self.enabled_config(auth_config_id)?;
        if cfg.scheme != AuthSchemeKind::OAuth2 {
            bail!("auth config is not OAuth2");
        }
        let scheme = self.oauth_scheme(&cfg.toolkit)?;
        let account = self.new_account(&cfg, project, actor, ConnectionStatus::Initializing);
        let state = pkce::random_token(24);
        let pkce = scheme.pkce.then(pkce::generate);
        let redirect_uri = self.redirect.redirect_uri();
        let scopes = if cfg.scopes.is_empty() {
            scheme.default_scopes.clone()
        } else {
            cfg.scopes.clone()
        };
        let url = oauth2::authorize_url(
            &scheme,
            &oauth2::AuthorizeParams {
                client_id: cfg.client_id.as_deref().unwrap_or_default(),
                redirect_uri: &redirect_uri,
                scopes: &scopes,
                state: &state,
                code_challenge: pkce.as_ref().map(|p| p.challenge.as_str()),
            },
        )?;
        self.store.save(&account, None)?;
        self.store.put_pending(
            &account,
            &PendingAuthorization {
                state: state.clone(),
                code_verifier: pkce.map(|p| p.verifier),
                redirect_uri,
                created_at: account.created_at,
            },
        )?;
        Ok(ConnectionRequest {
            account_id: account.id,
            toolkit: cfg.toolkit,
            redirect_url: url,
            state,
            status: ConnectionStatus::Initializing,
        })
    }

    /// Finish the OAuth2 connection from the provider callback (URL or query).
    pub fn complete_oauth(&self, callback: &str) -> Result<ConnectedAccount> {
        let (code, state, denied) = match oauth2::parse_callback(callback)? {
            CallbackResult::Code { code, state } => (Some(code), state, None),
            CallbackResult::Denied { error, state } => (
                None,
                state.ok_or_else(|| anyhow!("callback without state"))?,
                Some(error),
            ),
        };
        let now = self.now();
        let mut found = None;
        for account in self
            .store
            .list()?
            .into_iter()
            .filter(|a| a.status == ConnectionStatus::Initializing)
        {
            if let Some(p) = self.store.take_pending(&account)? {
                if constant_time_eq(p.state.as_bytes(), state.as_bytes()) {
                    found = Some((account, p));
                    break;
                }
                self.store.put_pending(&account, &p)?; // not ours: put back
            }
        }
        let (mut account, pending) =
            found.ok_or_else(|| anyhow!("unknown or already used OAuth state"))?;
        account.updated_at = now;
        if now.saturating_sub(pending.created_at) > PENDING_TTL_SECS {
            return self.fail(account, "authorization expired");
        }
        if let Some(error) = denied {
            return self.fail(account, &format!("provider denied: {error}"));
        }
        let cfg = self
            .store
            .auth_config(&account.auth_config_id)?
            .context("auth config vanished")?;
        let secret = self.store.client_secret(&cfg)?;
        let scheme = self.oauth_scheme(&account.toolkit)?;
        let client = ClientCredentials {
            client_id: cfg.client_id.as_deref().unwrap_or_default(),
            client_secret: secret.as_deref(),
        };
        match oauth2::exchange_code(
            self.transport.as_ref(),
            &scheme,
            &client,
            code.as_deref().unwrap_or_default(),
            &pending.redirect_uri,
            pending.code_verifier.as_deref(),
        ) {
            Ok(tokens) => {
                self.apply_tokens(&mut account, &tokens, scheme.refreshable, None)?;
                Ok(account)
            }
            Err(e) => self.fail(account, &e.to_string()),
        }
    }

    fn fail(&self, mut account: ConnectedAccount, reason: &str) -> Result<ConnectedAccount> {
        account.status = ConnectionStatus::Failed;
        account.status_reason = Some(reason.into());
        self.store.save(&account, None)?;
        Ok(account)
    }

    fn apply_tokens(
        &self,
        account: &mut ConnectedAccount,
        t: &TokenSet,
        refreshable: bool,
        previous_refresh: Option<String>,
    ) -> Result<()> {
        let now = self.now();
        account.status = ConnectionStatus::Active;
        account.status_reason = None;
        account.updated_at = now;
        if let Some(scopes) = &t.scopes {
            account.scopes = scopes.clone();
        }
        account.expires_at = refresh::expires_at(now, t.expires_in);
        let refresh_token = t.refresh_token.clone().or(previous_refresh);
        account.next_refresh_after = refresh::next_refresh_after(
            now,
            account.expires_at,
            self.refresh_lead,
            refreshable && refresh_token.is_some(),
        );
        self.store.save(
            account,
            Some(&AccountSecret {
                access_token: t.access_token.clone(),
                refresh_token,
                token_type: t.token_type.clone(),
            }),
        )
    }

    /// Connect with a static credential (API key, bearer token, or `user:password` for Basic).
    pub fn connect_with_credential(
        &self,
        auth_config_id: &str,
        project: &str,
        actor: &str,
        credential: &str,
    ) -> Result<ConnectedAccount> {
        let cfg = self.enabled_config(auth_config_id)?;
        if cfg.scheme == AuthSchemeKind::OAuth2 {
            bail!("OAuth2 auth configs use connect_oauth");
        }
        if credential.trim().is_empty() {
            bail!("empty credential");
        }
        let mut account = self.new_account(&cfg, project, actor, ConnectionStatus::Active);
        account.status = ConnectionStatus::Active;
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
    pub fn refresh(&self, account_id: &str) -> Result<ConnectedAccount> {
        let mut account = self
            .store
            .get(account_id)?
            .ok_or_else(|| anyhow!("unknown account"))?;
        let secret = self.store.secret(&account)?;
        let Some(refresh_token) = secret.and_then(|s| s.refresh_token) else {
            return self.expire(account, "no refresh token");
        };
        let cfg = self
            .store
            .auth_config(&account.auth_config_id)?
            .context("auth config vanished")?;
        let client_secret = self.store.client_secret(&cfg)?;
        let scheme = self.oauth_scheme(&account.toolkit)?;
        let client = ClientCredentials {
            client_id: cfg.client_id.as_deref().unwrap_or_default(),
            client_secret: client_secret.as_deref(),
        };
        match oauth2::refresh_token(self.transport.as_ref(), &scheme, &client, &refresh_token) {
            Ok(tokens) => {
                self.apply_tokens(
                    &mut account,
                    &tokens,
                    scheme.refreshable,
                    Some(refresh_token),
                )?;
                Ok(account)
            }
            Err(e) => self.expire(account, &e.to_string()),
        }
    }

    fn expire(&self, mut account: ConnectedAccount, reason: &str) -> Result<ConnectedAccount> {
        account.status = ConnectionStatus::Expired;
        account.status_reason = Some(reason.into());
        account.next_refresh_after = None;
        account.updated_at = self.now();
        self.store.save(&account, None)?;
        Ok(account)
    }

    /// Scheduler tick: refresh every due account. Returns resulting states.
    pub fn refresh_due(&self) -> Result<Vec<ConnectedAccount>> {
        let now = self.now();
        let accounts = self.store.list()?;
        let ids: Vec<String> = refresh::due(&accounts, now).map(|a| a.id.clone()).collect();
        ids.iter().map(|id| self.refresh(id)).collect()
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
    /// On HTTP 401 the token is refreshed once and the call replayed once.
    pub fn execute(
        &self,
        project: &str,
        tool_slug: &str,
        args: &Value,
        account_id: Option<&str>,
    ) -> Result<ToolOutcome> {
        let (adapter, spec) = self.tool(tool_slug)?;
        let tool_slug = spec.slug.as_str();
        // Validate arguments before touching credentials.
        let request = adapter.build_request(tool_slug, args)?;
        let toolkit = spec.toolkit.clone();
        let Some(mut account) = self.select_account(project, &toolkit, account_id)? else {
            return Ok(self.credential_required(&toolkit, None));
        };
        let refreshable = |a: &ConnectedAccount| {
            a.next_refresh_after.is_some()
                || self
                    .store
                    .secret(a)
                    .ok()
                    .flatten()
                    .is_some_and(|s| s.refresh_token.is_some())
        };
        if account.status == ConnectionStatus::Expired || refresh::is_expired(&account, self.now())
        {
            if !refreshable(&account) {
                return Ok(self.credential_required(&toolkit, Some(account.id)));
            }
            account = self.refresh(&account.id)?;
            if account.status != ConnectionStatus::Active {
                return Ok(self.credential_required(&toolkit, Some(account.id)));
            }
        }
        let mut refreshed = false;
        loop {
            let secret = self
                .store
                .secret(&account)?
                .ok_or_else(|| anyhow!("account has no credential"))?;
            let response =
                self.transport
                    .send(self.authorize(request.clone(), &account, &secret)?)?;
            if response.status == 401 {
                if !refreshed && secret.refresh_token.is_some() {
                    refreshed = true;
                    account = self.refresh(&account.id)?;
                    if account.status == ConnectionStatus::Active {
                        continue;
                    }
                } else {
                    self.expire(account.clone(), "provider rejected credential (401)")?;
                }
                return Ok(self.credential_required(&toolkit, Some(account.id)));
            }
            if (200..300).contains(&response.status) {
                return Ok(ToolOutcome::Success {
                    data: adapter.parse_response(tool_slug, &response)?,
                });
            }
            return Ok(ToolOutcome::ProviderError {
                status: response.status,
                data: response.json().unwrap_or(Value::Null),
            });
        }
    }
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}
