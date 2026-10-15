// Origin: CTOX; License: AGPL-3.0-only
//! Native quota credentials: canonical topology + the encrypted secret authority.
//! Neither a plugin request nor a renderer can supply credential values or bindings.
use super::*;
use ctox_cliproxyapi::{
    internal::api::handlers::management::{
        ManagementQuotaCredential, ManagementQuotaCredentialError, ManagementQuotaCredentialSource,
    },
    sdk::pluginapi::{
        HostHttpClient, HttpRequest, HttpResponse, HttpStreamResponse, PluginExecutionError,
        PluginFuture, QuotaFetchRequest,
    },
};
use std::io::Read;

#[derive(Clone, Debug)]
pub(crate) struct CtoxQuotaCredentials {
    root: PathBuf,
}
#[derive(Clone, PartialEq)]
struct Account {
    provider: &'static str,
    config: serde_json::Value,
    origin: String,
    keys: Vec<RuntimeSecretRef>,
    proxy: Option<usize>,
}
impl Account {
    fn new(
        provider: &'static str,
        config: serde_json::Value,
        origin: String,
        mut keys: Vec<RuntimeSecretRef>,
        proxy: Option<&RuntimeSecretRef>,
    ) -> Self {
        let proxy = proxy.map(|value| {
            keys.push(value.clone());
            keys.len() - 1
        });
        Self {
            provider,
            config,
            origin,
            keys,
            proxy,
        }
    }
}
fn account(root: &Path, id: &str) -> anyhow::Result<Option<Account>> {
    let Some(stored) = load_instance_proxy_config(root)? else {
        return Ok(None);
    };
    for a in stored.runtime.claude_accounts {
        if a.id == id && !a.disabled {
            return Ok(Some(Account::new(
                "claude",
                serde_json::to_value(&a)?,
                format!("{}://{}", a.upstream_scheme, a.upstream_authority),
                vec![
                    a.access_token_secret.clone(),
                    a.refresh_token_secret.clone(),
                ],
                a.proxy_url_secret.as_ref(),
            )));
        }
    }
    for a in stored.runtime.codex_accounts {
        if a.id == id && !a.disabled {
            return Ok(Some(Account::new(
                "codex",
                serde_json::to_value(&a)?,
                a.upstream_base_url.clone(),
                vec![
                    a.access_token_secret.clone(),
                    a.refresh_token_secret.clone(),
                    a.id_token_secret.clone(),
                ],
                a.proxy_url_secret.as_ref(),
            )));
        }
    }
    for a in stored.runtime.antigravity_accounts {
        if a.id == id && !a.disabled {
            return Ok(Some(Account::new(
                "antigravity",
                serde_json::to_value(&a)?,
                a.upstream_base_url.clone(),
                vec![
                    a.access_token_secret.clone(),
                    a.refresh_token_secret.clone(),
                    a.state_secret.clone(),
                ],
                a.proxy_url_secret.as_ref(),
            )));
        }
    }
    Ok(None)
}
fn binding(account: &Account, fingerprint: &str) -> anyhow::Result<String> {
    let mut hash = Sha256::new();
    hash.update(b"ctox.plugin-quota.account-binding.v1");
    for value in [
        account.provider.as_bytes(),
        &serde_json::to_vec(&account.config)?,
        fingerprint.as_bytes(),
    ] {
        hash.update((value.len() as u64).to_be_bytes());
        hash.update(value);
    }
    Ok(format!("{:x}", hash.finalize()))
}
impl CtoxQuotaCredentials {
    pub(crate) fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
    // The public runtime replacement and account controls serialize on Runtime;
    // encrypted record mutations serialize on the secret fence. No schema setup,
    // migration, network IO or credential retention takes place in this callback.
    fn with_account<T>(
        &self,
        index: &str,
        expected: Option<&str>,
        apply: impl FnOnce(&Account, &str, &[&[u8]]) -> anyhow::Result<T>,
    ) -> anyhow::Result<Option<T>> {
        if !ctox_cliproxyapi::internal::api::account_selection::valid_account_id(index) {
            return Ok(None);
        }
        let _lifecycle = crate::secrets::credential_lifecycle_guard();
        let Some(captured) = account(&self.root, index)? else {
            return Ok(None);
        };
        let keys = captured
            .keys
            .iter()
            .map(|k| (k.scope.as_str(), k.name.as_str()))
            .collect::<Vec<_>>();
        crate::secrets::with_current_secret_values_and_fingerprint(
            &self.root,
            &keys,
            |values, fingerprint| {
                // Open existing storage only. The authoritative reader must not create
                // runtime directories/tables on an absent or unavailable deployment.
                let mut conn = Connection::open_with_flags(
                    crate::inference::runtime_env::runtime_config_path(&self.root),
                    OpenFlags::SQLITE_OPEN_READ_WRITE,
                )?;
                conn.busy_timeout(Duration::ZERO)?;
                let _runtime = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
                let Some(current) = account(&self.root, index)? else {
                    return Ok(None);
                };
                if current != captured {
                    return Ok(None);
                }
                let binding = binding(&current, fingerprint)?;
                if expected.is_some_and(|expected| expected != binding) {
                    return Ok(None);
                }
                apply(&current, &binding, values).map(Some)
            },
        )
    }
}
impl ManagementQuotaCredentialSource for CtoxQuotaCredentials {
    fn resolve(
        &self,
        index: &str,
    ) -> Result<Option<ManagementQuotaCredential>, ManagementQuotaCredentialError> {
        self.with_account(index, None, |a, binding, _| {
            Ok(ManagementQuotaCredential {
                binding: binding.into(),
                request: QuotaFetchRequest {
                    auth_index: index.into(),
                    auth_id: index.into(),
                    provider: a.provider.into(),
                    http_client: Some(Arc::new(AccountHttp {
                        source: self.clone(),
                        index: index.into(),
                        binding: binding.into(),
                    })),
                    ..Default::default()
                },
                probe: None,
            })
        })
        .map_err(|_| ManagementQuotaCredentialError::Unavailable)
    }
    fn with_current_binding(
        &self,
        index: &str,
        binding: &str,
        publish: &mut dyn FnMut(),
    ) -> Result<bool, ManagementQuotaCredentialError> {
        self.with_account(index, Some(binding), |_, _, _| {
            publish();
            Ok(())
        })
        .map(|value| value.is_some())
        .map_err(|_| ManagementQuotaCredentialError::Unavailable)
    }
}
#[derive(Clone)]
struct AccountHttp {
    source: CtoxQuotaCredentials,
    index: String,
    binding: String,
}
struct Prepared {
    access: Zeroizing<String>,
    proxy: Option<Zeroizing<String>>,
}
fn failed(message: &'static str) -> PluginExecutionError {
    Arc::new(std::io::Error::other(message))
}
fn validate(request: &HttpRequest, a: &Account) -> anyhow::Result<()> {
    use ctox_cliproxyapi::internal::pluginhost::http_bridge::MAX_PLUGIN_HTTP_BODY_BYTES;
    let target = url::Url::parse(&request.url)?;
    let origin = url::Url::parse(&a.origin)?;
    anyhow::ensure!(
        matches!(target.scheme(), "http" | "https")
            && target.origin() == origin.origin()
            && target.username().is_empty()
            && target.password().is_none()
            && target.fragment().is_none(),
        "quota request origin is not authorized"
    );
    anyhow::ensure!(
        request.url.len() <= 8192
            && request.body.len() <= MAX_PLUGIN_HTTP_BODY_BYTES
            && matches!(
                request.method.as_str(),
                "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "HEAD" | "OPTIONS"
            ),
        "invalid quota request"
    );
    anyhow::ensure!(request.headers.len() <= 64, "too many quota headers");
    let mut bytes = 0usize;
    for (name, values) in &request.headers {
        anyhow::ensure!(
            !name.is_empty()
                && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
                && values.len() <= 16,
            "invalid quota header"
        );
        for value in values {
            bytes = bytes.saturating_add(name.len()).saturating_add(value.len());
            anyhow::ensure!(
                value
                    .bytes()
                    .all(|byte| byte == b'\t' || (byte >= 0x20 && byte != 0x7f))
                    && bytes <= 65536,
                "invalid quota header"
            );
        }
    }
    Ok(())
}
impl AccountHttp {
    fn execute_blocking(&self, request: HttpRequest) -> Result<HttpResponse, PluginExecutionError> {
        let prepared = self
            .source
            .with_account(&self.index, Some(&self.binding), |a, _, values| {
                validate(&request, a)?;
                let access = Zeroizing::new(std::str::from_utf8(values[0])?.to_owned());
                anyhow::ensure!(
                    !access.trim().is_empty()
                        && access.len() <= 16384
                        && !access.chars().any(char::is_control),
                    "invalid quota credential"
                );
                let proxy = a
                    .proxy
                    .map(|i| std::str::from_utf8(values[i]).map(|v| Zeroizing::new(v.to_owned())))
                    .transpose()?;
                Ok(Prepared { access, proxy })
            })
            .map_err(|_| failed("quota credential or request authority unavailable"))?
            .ok_or_else(|| failed("quota account binding retired"))?;
        let mut builder = ureq::AgentBuilder::new()
            .redirects(0)
            .try_proxy_from_env(false)
            .timeout_connect(Duration::from_secs(5))
            .timeout(Duration::from_secs(10));
        if let Some(proxy) = &prepared.proxy {
            builder = builder.proxy(
                ureq::Proxy::new(proxy.as_str())
                    .map_err(|_| failed("quota proxy configuration unavailable"))?,
            );
        }
        let agent = builder.build();
        let mut upstream = agent.request(&request.method, &request.url);
        for (name, values) in &request.headers {
            if [
                "authorization",
                "proxy-authorization",
                "cookie",
                "host",
                "x-api-key",
            ]
            .iter()
            .any(|blocked| name.eq_ignore_ascii_case(blocked))
            {
                continue;
            }
            for value in values {
                upstream = upstream.set(name, value);
            }
        }
        let authorization = Zeroizing::new(format!("Bearer {}", prepared.access.as_str()));
        let response = upstream
            .set("Authorization", &authorization)
            .send_bytes(&request.body);
        let response = match response {
            Ok(r) | Err(ureq::Error::Status(_, r)) => Ok(r),
            Err(ureq::Error::Transport(_)) => Err(failed("quota provider transport failed")),
        };
        let result = response.and_then(|response| {
            let status_code = response.status();
            let headers = response
                .headers_names()
                .into_iter()
                .filter(|name| {
                    !["authorization", "proxy-authorization", "set-cookie"]
                        .iter()
                        .any(|blocked| name.eq_ignore_ascii_case(blocked))
                })
                .map(|name| {
                    let values = response.all(&name).into_iter().map(str::to_owned).collect();
                    (name, values)
                })
                .collect();
            let mut body = Vec::new();
            let max =
                ctox_cliproxyapi::internal::pluginhost::http_bridge::MAX_PLUGIN_HTTP_BODY_BYTES;
            response
                .into_reader()
                .take(max as u64 + 1)
                .read_to_end(&mut body)
                .map_err(|_| failed("quota provider response unavailable"))?;
            if body.len() > max {
                return Err(failed("quota provider response exceeds limit"));
            }
            Ok(HttpResponse {
                status_code,
                headers,
                body,
            })
        });
        self.source
            .with_account(&self.index, Some(&self.binding), |_, _, _| Ok(result))
            .map_err(|_| failed("quota account authority unavailable after request"))?
            .ok_or_else(|| failed("quota account binding retired"))?
    }
}
impl HostHttpClient for AccountHttp {
    fn execute<'a>(&'a self, request: HttpRequest) -> PluginFuture<'a, HttpResponse> {
        let client = self.clone();
        Box::pin(async move {
            tokio::task::spawn_blocking(move || client.execute_blocking(request))
                .await
                .map_err(|_| failed("quota transport worker unavailable"))?
        })
    }
    fn execute_stream<'a>(&'a self, _: HttpRequest) -> PluginFuture<'a, HttpStreamResponse> {
        Box::pin(async { Err(failed("native quota HTTP supports bounded unary responses")) })
    }
}

#[cfg(test)]
#[path = "cliproxyapi_plugin_quota_tests.rs"]
mod tests;
