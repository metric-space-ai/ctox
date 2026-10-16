//! E55 control protocol. A peer boot owns this bounded, idempotent dispatcher.
use super::app_grant::{AppGrantSession, GrantRequest};
use super::StoreRightsAuthority;
use serde_json::{json, Value};
use std::{collections::BTreeMap, sync::Arc};

pub struct AppGrantRoute {
    session: Arc<AppGrantSession>,
    authority: Arc<StoreRightsAuthority>,
    answered: tokio::sync::Mutex<BTreeMap<String, String>>,
}
impl AppGrantRoute {
    pub fn new(session: Arc<AppGrantSession>, authority: Arc<StoreRightsAuthority>) -> Self {
        Self {
            session,
            authority,
            answered: Default::default(),
        }
    }
    pub fn session(&self) -> Arc<AppGrantSession> { self.session.clone() }
    pub async fn shell_token(&self, now: u64) -> Result<String, String> {
        self.handle(json!({"type":"scope.grant.request","app_id":"shell"}), now)
            .await?
            .and_then(|v| v["token"].as_str().map(str::to_owned))
            .ok_or_else(|| "shell grant unavailable".into())
    }
    /// None means an already answered, still-current request. No second token
    /// or response is emitted; stale cached grants fail closed under the writer.
    pub async fn handle(&self, message: Value, now: u64) -> Result<Option<Value>, String> {
        let request: GrantRequest =
            serde_json::from_value(message).map_err(|_| "invalid grant request")?;
        let GrantRequest::Request { ref app_id } = request;
        let app = app_id.clone();
        if app.len() > 256 || app.is_empty() {
            return Err("invalid app id".into());
        }
        let mut answered = self.answered.lock().await;
        if let Some(token) = answered.get(&app).cloned() {
            if token.is_empty() {
                return Ok(None);
            }
            let session = self.session.clone();
            let valid = self
                .authority
                .writer()
                .submit(ctox_store::WriteJob::new(Default::default(), move |tx| {
                    session
                        .verify_in_transaction(tx, &token, now)
                        .map(|_| ())
                        .map_err(ctox_store::StoreError::Invalid)
                }))
                .map_err(|_| "grant unavailable")?
                .await;
            return Ok(if valid.is_ok() {
                None
            } else {
                Some(json!({"type":"forbidden","app_id":app,"reason":"revoked"}))
            });
        }
        if answered.len() >= 256 {
            return Err("grant budget".into());
        }
        let response = self.session.handle_request(request, now).await;
        let token = response.get("token").and_then(Value::as_str).unwrap_or("");
        if answered.values().map(String::len).sum::<usize>() + token.len() > 256 * 1024 {
            return Err("grant budget".into());
        }
        answered.insert(app, token.to_owned());
        Ok(Some(response))
    }
}
