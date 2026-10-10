// Origin: CTOX
// License: AGPL-3.0-only

//! Exact-account model dispatch for one real native Supervisor controller.
//! This is a private Rust consumer, not a public HTTP listener or SDK producer.

use super::cliproxyapi_claude_sdk::{
    NativeClaudeSdkAccountReservation, NativeClaudeSdkConfiguration,
};
use crate::business_os::mcp_channel::NativeSupervisorHoldingController;
use anyhow::{ensure, Context, Result};
use ctox_cliproxyapi::internal::{
    auth::claude::SecretString,
    runtime::executor::{
        claude_executor::{
            ClaudeCredentialMode, ClaudeMessagesRequest, ClaudeMessagesResponse,
            ClaudeMessagesStreamResponse, ClaudeMessagesStreamingTransport,
            ClaudeMessagesTransport, ClaudeUpstreamTarget,
        },
        claude_executor_request::ClaudeMessagesHttpTransport,
    },
};
use ring::{
    hmac,
    rand::{SecureRandom, SystemRandom},
};
use std::{
    future::Future,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::sync::{watch, OwnedSemaphorePermit, Semaphore};

const MAX_BYTES: usize = 8 * 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(300);
const CURRENT_POLL: Duration = Duration::from_millis(250);

#[derive(Clone, Copy)]
pub(crate) enum NativeClaudeOperation {
    Messages,
    CountTokens,
}

/// Holder-only upstream witness. No Serialize/Debug/Clone; this is neither an
/// SDK session/turn witness nor a process-stop or replicated receipt.
pub(crate) struct NativeClaudeModelExchange {
    pub(crate) model: String,
    pub(crate) private_binding: String,
    pub(crate) controller_id: String,
    pub(crate) client_request_id: String,
    pub(crate) http_status: u16,
    pub(crate) elapsed_ms: u64,
}

pub(crate) struct NativeClaudeLeaseModelProxy {
    controller: Arc<NativeSupervisorHoldingController>,
    account: NativeClaudeSdkAccountReservation,
    capability: Mutex<Option<SecretString>>,
    retired: watch::Sender<bool>,
    transport: ClaudeMessagesHttpTransport,
    slot: Arc<Semaphore>,
}
enum NativeClaudeBody {
    Buffered(ClaudeMessagesResponse),
    Stream(ClaudeMessagesStreamResponse),
}
pub(crate) struct NativeClaudeLeaseModelReply {
    proxy: Arc<NativeClaudeLeaseModelProxy>,
    body: NativeClaudeBody,
    exchange: NativeClaudeModelExchange,
    deadline: Instant,
    bytes: usize,
    slot: Option<OwnedSemaphorePermit>,
}

fn new_capability() -> Result<SecretString> {
    let mut bytes = zeroize::Zeroizing::new([0u8; 32]);
    SystemRandom::new()
        .fill(bytes.as_mut())
        .map_err(|_| anyhow::anyhow!("native model capability generation failed"))?;
    SecretString::new(
        bytes
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>(),
    )
    .map_err(|_| anyhow::anyhow!("native model capability generation failed"))
}
fn capability_matches(expected: &str, presented: &str) -> bool {
    if presented.len() != 64 {
        return false;
    }
    let key = hmac::Key::new(hmac::HMAC_SHA256, expected.as_bytes());
    let expected_mac = hmac::sign(&key, expected.as_bytes());
    hmac::verify(&key, presented.as_bytes(), expected_mac.as_ref()).is_ok()
}
fn request_stream(
    body: &[u8],
    model: &str,
    session: &str,
    op: NativeClaudeOperation,
) -> Result<bool> {
    ensure!(
        !body.is_empty() && body.len() <= MAX_BYTES,
        "invalid native model request size"
    );
    ensure!(
        !session.is_empty() && session.len() <= 256 && !session.chars().any(char::is_control),
        "invalid native model session correlation"
    );
    let value: serde_json::Value =
        serde_json::from_slice(body).context("invalid native model request")?;
    ensure!(
        value.get("model").and_then(serde_json::Value::as_str) == Some(model),
        "model is outside this native lease"
    );
    ensure!(
        value
            .get("messages")
            .is_some_and(serde_json::Value::is_array),
        "native Claude messages required"
    );
    let stream = match value.get("stream") {
        None => false,
        Some(serde_json::Value::Bool(value)) => *value,
        _ => anyhow::bail!("invalid native model streaming flag"),
    };
    ensure!(
        !matches!(op, NativeClaudeOperation::CountTokens) || !stream,
        "token counting cannot stream"
    );
    Ok(stream)
}

impl NativeClaudeLeaseModelProxy {
    pub(crate) fn reserve(controller: NativeSupervisorHoldingController) -> Result<Arc<Self>> {
        let account = NativeClaudeSdkAccountReservation::prepare(
            controller.authority(),
            controller.selection(),
        )?;
        account.with_current_controller_configuration(&controller, |_| Ok(()))?;
        let transport = ClaudeMessagesHttpTransport::new(None)
            .map_err(|_| anyhow::anyhow!("native Claude transport unavailable"))?;
        let proxy = Arc::new(Self {
            controller: Arc::new(controller),
            account,
            transport,
            capability: Mutex::new(Some(new_capability()?)),
            retired: watch::channel(false).0,
            slot: Arc::new(Semaphore::new(1)),
        });
        proxy.with_current(|_| Ok(()))?;
        Ok(proxy)
    }
    /// Only the registered private Source broker receives this scoped token.
    /// OAuth and the private account/config fingerprint remain on the holder.
    /// Bounded synchronous handoff only; no network/secret/controller reentry
    /// and no cancellation or proxy reentry while this callback is active.
    pub(crate) fn with_scoped_capability<T>(
        &self,
        apply: impl FnOnce(&str, &SecretString) -> Result<T>,
    ) -> Result<T> {
        let capability = self
            .capability
            .try_lock()
            .map_err(|_| anyhow::anyhow!("native model capability unavailable"))?;
        let capability = capability
            .as_ref()
            .context("native model capability retired")?;
        self.with_current(|configuration| apply(configuration.model(), capability))
    }
    fn authorize(&self, presented: &str) -> Result<()> {
        let capability = self
            .capability
            .try_lock()
            .map_err(|_| anyhow::anyhow!("native model capability unavailable"))?;
        ensure!(
            capability
                .as_ref()
                .is_some_and(|capability| capability_matches(
                    capability.expose_secret(),
                    presented
                )),
            "native model capability rejected"
        );
        Ok(())
    }
    fn with_current<T>(
        &self,
        apply: impl FnOnce(NativeClaudeSdkConfiguration<'_>) -> Result<T>,
    ) -> Result<T> {
        ensure!(!*self.retired.borrow(), "native model proxy retired");
        self.account
            .with_current_controller_configuration(&self.controller, |configuration| {
                ensure!(!*self.retired.borrow(), "native model proxy retired");
                apply(configuration)
            })
    }
    async fn while_current<T>(
        &self,
        deadline: Instant,
        future: impl Future<Output = T>,
    ) -> Result<T> {
        self.with_current(|_| Ok(()))?;
        let mut retired = self.retired.subscribe();
        let mut poll = tokio::time::interval(CURRENT_POLL);
        poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let timeout = tokio::time::sleep(deadline.saturating_duration_since(Instant::now()));
        tokio::pin!(future, timeout);
        loop {
            tokio::select! {
                biased;
                _ = retired.changed() => anyhow::bail!("native model proxy retired"),
                _ = &mut timeout => anyhow::bail!("native model request timed out"),
                result = &mut future => {
                    self.with_current(|_| Ok(()))?;
                    return Ok(result);
                }
                _ = poll.tick() => {
                    if self.with_current(|_| Ok(())).is_err() {
                        let _ = self.cancel();
                        anyhow::bail!("native model lease or account retired");
                    }
                }
            }
        }
    }
    /// Genuine SDK Messages only; caller cannot override model/account/URL.
    /// SDK correlation strings do not establish session/turn execution proof.
    pub(crate) async fn invoke(
        self: &Arc<Self>,
        capability: &str,
        op: NativeClaudeOperation,
        body: Vec<u8>,
        sdk_session: &str,
    ) -> Result<NativeClaudeLeaseModelReply> {
        self.authorize(capability)?;
        let started = Instant::now();
        let deadline = started + TIMEOUT;
        let model = self.with_current(|configuration| Ok(configuration.model().to_owned()))?;
        let stream = request_stream(&body, &model, sdk_session, op)?;
        let slot = self
            .while_current(deadline, Arc::clone(&self.slot).acquire_owned())
            .await?
            .map_err(|_| anyhow::anyhow!("native model proxy retired"))?;
        let (request, mut exchange) = self.with_current(|configuration| {
            let request = ClaudeMessagesRequest::new_with_session(
                ClaudeUpstreamTarget::new("https", "api.anthropic.com")?,
                ClaudeCredentialMode::OAuth,
                configuration.access_token(),
                body,
                stream,
                sdk_session,
            )?;
            let exchange = NativeClaudeModelExchange {
                model: configuration.model().to_owned(),
                private_binding: configuration.private_binding().to_owned(),
                controller_id: self.controller.controller_id().to_owned(),
                client_request_id: request.fingerprint().client_request_id().to_owned(),
                http_status: 0,
                elapsed_ms: 0,
            };
            Ok((request, exchange))
        })?;
        let body = if stream {
            upstream_stream_body(
                self.while_current(deadline, self.transport.execute_stream(&request, TIMEOUT))
                    .await?
                    .map_err(|_| anyhow::anyhow!("native Claude upstream transport failed"))?,
            )?
        } else {
            let response = match op {
                NativeClaudeOperation::Messages => {
                    self.while_current(deadline, self.transport.execute(&request, TIMEOUT))
                        .await?
                }
                NativeClaudeOperation::CountTokens => {
                    self.while_current(
                        deadline,
                        self.transport.execute_count_tokens(&request, TIMEOUT),
                    )
                    .await?
                }
            }
            .map_err(|_| anyhow::anyhow!("native Claude upstream transport failed"))?;
            ensure!(
                response.body().len() <= MAX_BYTES,
                "native model response too large"
            );
            NativeClaudeBody::Buffered(response)
        };
        exchange.http_status = match &body {
            NativeClaudeBody::Buffered(response) => response.status(),
            NativeClaudeBody::Stream(response) => response.status(),
        };
        exchange.elapsed_ms = started.elapsed().as_millis() as u64;
        self.with_current(|_| Ok(()))?;
        Ok(NativeClaudeLeaseModelReply {
            proxy: Arc::clone(self),
            body,
            exchange,
            deadline,
            bytes: 0,
            slot: Some(slot),
        })
    }
    /// Retires only this controller/capability/account; no SDK stop claim.
    pub(crate) fn cancel(&self) -> Result<()> {
        self.retired.send_replace(true);
        self.slot.close();
        self.capability
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        self.account.release();
        self.controller.cancel()
    }
}
impl Drop for NativeClaudeLeaseModelProxy {
    fn drop(&mut self) {
        let _ = self.cancel();
    }
}
impl NativeClaudeLeaseModelReply {
    /// A streaming request can yield a buffered upstream HTTP error.
    /// Inspect this before choosing the publication operation.
    pub(crate) fn is_streaming(&self) -> bool {
        matches!(&self.body, NativeClaudeBody::Stream(_))
    }
    pub(crate) fn publish_buffered<T>(
        self,
        publish: impl FnOnce(&[u8], &NativeClaudeModelExchange) -> Result<T>,
    ) -> Result<T> {
        let NativeClaudeBody::Buffered(ref response) = self.body else {
            anyhow::bail!("native model reply is streaming");
        };
        ensure!(Instant::now() < self.deadline, "native model reply expired");
        self.proxy
            .with_current(|_| publish(response.body(), &self.exchange))
    }
    /// Bounded native enqueue only: no await/network/store/transport reentry.
    /// The actual receiver must also fence its physical transport publication.
    pub(crate) async fn publish_next<T>(
        &mut self,
        publish: impl FnOnce(&[u8], &NativeClaudeModelExchange) -> Result<T>,
    ) -> Result<Option<T>> {
        let NativeClaudeBody::Stream(ref mut response) = self.body else {
            anyhow::bail!("native model reply is buffered");
        };
        ensure!(self.slot.is_some(), "native model stream already finished");
        let result = async {
            let chunk = self
                .proxy
                .while_current(self.deadline, response.next_chunk())
                .await?;
            let Some(chunk) = chunk else {
                return Ok(None);
            };
            let chunk =
                chunk.map_err(|_| anyhow::anyhow!("native Claude upstream stream failed"))?;
            self.bytes = self
                .bytes
                .checked_add(chunk.len())
                .context("native model response too large")?;
            ensure!(self.bytes <= MAX_BYTES, "native model response too large");
            self.proxy
                .with_current(|_| publish(&chunk, &self.exchange))
                .map(Some)
        }
        .await;
        if !matches!(&result, Ok(Some(_))) {
            self.slot.take();
        }
        result
    }
}

fn upstream_stream_body(response: ClaudeMessagesStreamResponse) -> Result<NativeClaudeBody> {
    if (200..300).contains(&response.status()) {
        return Ok(NativeClaudeBody::Stream(response));
    }
    ensure!(
        response.error_body().len() <= MAX_BYTES,
        "native model response too large"
    );
    // Preserve the actual HTTP failure. A closed SSE receiver must never
    // turn a rejected credential/model into a successful empty response.
    Ok(NativeClaudeBody::Buffered(
        ClaudeMessagesResponse::new(response.status(), response.error_body().to_vec())
            .with_retry_after(response.retry_after())
            .with_headers(response.headers().clone()),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejected_stream_retains_actual_http_status_and_error_body() -> Result<()> {
        let (_, receiver) = tokio::sync::mpsc::channel(1);
        let response =
            ClaudeMessagesStreamResponse::new(401, Some(Duration::from_secs(2)), receiver)
                .with_error_body(br#"{"error":{"type":"authentication_error"}}"#.to_vec());
        let NativeClaudeBody::Buffered(response) = upstream_stream_body(response)? else {
            panic!("an HTTP rejection is not a successful SSE stream");
        };
        assert_eq!(response.status(), 401);
        assert_eq!(response.retry_after(), Some(Duration::from_secs(2)));
        assert_eq!(
            response.body(),
            br#"{"error":{"type":"authentication_error"}}"#
        );
        Ok(())
    }

    #[test]
    fn successful_stream_is_not_flattened_and_error_buffer_is_bounded() -> Result<()> {
        let (_, receiver) = tokio::sync::mpsc::channel(1);
        assert!(matches!(
            upstream_stream_body(ClaudeMessagesStreamResponse::new(200, None, receiver))?,
            NativeClaudeBody::Stream(_)
        ));
        let (_, receiver) = tokio::sync::mpsc::channel(1);
        assert!(upstream_stream_body(
            ClaudeMessagesStreamResponse::new(403, None, receiver).with_error_body(vec![
                b' ';
                MAX_BYTES
                    + 1
            ])
        )
        .is_err());
        Ok(())
    }

    // Authenticated live source catalog, g3-claude-live-models-20261009.json.
    const MODEL: &str = "claude-opus-5-5";

    #[test]
    fn capability_is_exact_and_independent_per_lease() -> Result<()> {
        let first = new_capability()?;
        let second = new_capability()?;
        assert_eq!(first.expose_secret().len(), 64);
        assert_ne!(first.expose_secret(), second.expose_secret());
        assert!(capability_matches(
            first.expose_secret(),
            first.expose_secret()
        ));
        assert!(!capability_matches(
            first.expose_secret(),
            second.expose_secret()
        ));
        assert!(!capability_matches(first.expose_secret(), ""));
        Ok(())
    }
    #[test]
    fn request_cannot_change_model_or_stream_protocol() -> Result<()> {
        let request = serde_json::json!({"model":MODEL,"messages":[],"stream":true});
        let bytes = serde_json::to_vec(&request)?;
        assert!(request_stream(
            &bytes,
            MODEL,
            "sdk-correlation",
            NativeClaudeOperation::Messages
        )?);
        assert!(request_stream(
            &bytes,
            MODEL,
            "sdk-correlation",
            NativeClaudeOperation::CountTokens
        )
        .is_err());
        let wrong = serde_json::to_vec(&serde_json::json!({"model":null,"messages":[]}))?;
        assert!(request_stream(&wrong, MODEL, "session", NativeClaudeOperation::Messages).is_err());
        assert!(
            request_stream(&bytes, MODEL, "session\n", NativeClaudeOperation::Messages).is_err()
        );
        assert!(request_stream(
            &vec![b' '; MAX_BYTES + 1],
            MODEL,
            "session",
            NativeClaudeOperation::Messages
        )
        .is_err());
        Ok(())
    }
}
