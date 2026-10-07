//! Signed, nonce-correlated native handoff RPC. The receiver additionally binds
//! its one-use challenge to the exact admitted transport connection. A valid
//! signature alone is not policy authorization or connection admission.
use super::{hex, invalid, unhex, verify, Body, Envelope, SigningIdentity};
use crate::contracts::{
    SessionHandoffRequest, SessionHandoffWireReply, SessionHandoffWireRequest,
    CTOX_SYNC_SESSION_HANDOFF_MAX_WIRE_BYTES, CTOX_SYNC_SESSION_HANDOFF_REPLY_KIND as REPLY_KIND,
    CTOX_SYNC_SESSION_HANDOFF_REQUEST_KIND as REQUEST_KIND,
};
use ring::rand::{SecureRandom, SystemRandom};
use serde_json::Value;
use std::io;

const MAX_WIRE_BYTES: usize = CTOX_SYNC_SESSION_HANDOFF_MAX_WIRE_BYTES as usize;

pub fn fresh_nonce() -> io::Result<String> {
    let mut bytes = [0; 16];
    SystemRandom::new()
        .fill(&mut bytes)
        .map_err(|_| io::Error::other("handoff nonce generation failed"))?;
    Ok(hex(&bytes))
}
fn bounded(value: &Value) -> io::Result<()> {
    if serde_json::to_vec(value).map_err(io::Error::other)?.len() > MAX_WIRE_BYTES {
        return Err(invalid("handoff envelope exceeds its budget"));
    }
    Ok(())
}
pub struct SignedHandoffRequest {
    pub envelope: Value,
    nonce: String,
    sender: String,
    request: SessionHandoffRequest,
    expects_authorized: bool,
}
pub struct VerifiedHandoffRequest {
    message: SessionHandoffWireRequest,
    sender: String,
    nonce: String,
    recipient: String,
    scope: String,
}
impl VerifiedHandoffRequest {
    pub fn message(&self) -> &SessionHandoffWireRequest {
        &self.message
    }
    pub fn sender(&self) -> &str {
        &self.sender
    }
    pub fn request(&self) -> &SessionHandoffRequest {
        match &self.message {
            SessionHandoffWireRequest::Probe { request }
            | SessionHandoffWireRequest::Authorize { request, .. } => request,
        }
    }
    /// Must be called under the receiver's current encrypted issuer fence.
    pub fn reply(
        &self,
        identity: &SigningIdentity,
        reply: SessionHandoffWireReply,
    ) -> io::Result<Value> {
        if identity.public_identity() != self.recipient {
            return Err(invalid("handoff issuer changed"));
        }
        let envelope = identity.sign(Body {
            version: 1,
            sender: self.recipient.clone(),
            recipient: self.sender.clone(),
            scope_id: self.scope.clone(),
            nonce: self.nonce.clone(),
            kind: REPLY_KIND.into(),
            data: serde_json::to_value(reply).map_err(io::Error::other)?,
        })?;
        let value = serde_json::to_value(envelope).map_err(io::Error::other)?;
        bounded(&value)?;
        Ok(value)
    }
}
pub fn verify_request(
    value: Value,
    recipient: &str,
    scope: &str,
) -> io::Result<VerifiedHandoffRequest> {
    bounded(&value)?;
    let envelope: Envelope = serde_json::from_value(value).map_err(io::Error::other)?;
    verify(&envelope, recipient, scope, REQUEST_KIND)?;
    let message: SessionHandoffWireRequest =
        serde_json::from_value(envelope.body.data).map_err(io::Error::other)?;
    let result = VerifiedHandoffRequest {
        message,
        sender: envelope.body.sender,
        nonce: envelope.body.nonce,
        recipient: recipient.into(),
        scope: scope.into(),
    };
    let request = result.request();
    unhex::<16>(&request.nonce)?;
    if request.issuer_identity != recipient
        || request.audience != scope
        || request.spec.scope_id != scope
    {
        return Err(invalid("handoff request names another authority"));
    }
    if let SessionHandoffWireRequest::Authorize { challenge, .. } = &result.message {
        unhex::<16>(challenge)?;
    }
    Ok(result)
}
impl SignedHandoffRequest {
    /// Pins and current signing identity come from local native enrollment,
    /// never from the remote reply or a signaling identifier.
    pub fn new(identity: &SigningIdentity, message: SessionHandoffWireRequest) -> io::Result<Self> {
        let request = match &message {
            SessionHandoffWireRequest::Probe { request }
            | SessionHandoffWireRequest::Authorize { request, .. } => request.clone(),
        };
        unhex::<16>(&request.nonce)?;
        super::public_key(&request.issuer_identity)?;
        let nonce = fresh_nonce()?;
        let sender = identity.public_identity();
        let expects_authorized = matches!(&message, SessionHandoffWireRequest::Authorize { .. });
        let envelope = serde_json::to_value(identity.sign(Body {
            version: 1,
            sender: sender.clone(),
            recipient: request.issuer_identity.clone(),
            scope_id: request.audience.clone(),
            nonce: nonce.clone(),
            kind: REQUEST_KIND.into(),
            data: serde_json::to_value(message).map_err(io::Error::other)?,
        })?)
        .map_err(io::Error::other)?;
        bounded(&envelope)?;
        Ok(Self {
            envelope,
            nonce,
            sender,
            request,
            expects_authorized,
        })
    }
    pub fn nonce(&self) -> &str {
        &self.nonce
    }
    pub fn verify_reply(&self, value: Value, now_ms: u64) -> io::Result<SessionHandoffWireReply> {
        bounded(&value)?;
        let envelope: Envelope = serde_json::from_value(value).map_err(io::Error::other)?;
        verify(&envelope, &self.sender, &self.request.audience, REPLY_KIND)?;
        if envelope.body.sender != self.request.issuer_identity || envelope.body.nonce != self.nonce
        {
            return Err(invalid("handoff reply issuer or nonce mismatch"));
        }
        let reply: SessionHandoffWireReply =
            serde_json::from_value(envelope.body.data).map_err(io::Error::other)?;
        if matches!(&reply, SessionHandoffWireReply::Authorized { .. }) != self.expects_authorized {
            return Err(invalid("handoff reply has wrong phase stage"));
        }
        match &reply {
            SessionHandoffWireReply::Challenge { challenge } => {
                unhex::<16>(challenge)?;
            }
            SessionHandoffWireReply::Authorized { permit } => {
                super::session_handoff::verify_fresh_session_handoff_permit(
                    permit,
                    &self.request.issuer_identity,
                    &self.request.audience,
                    &self.request.nonce,
                    now_ms,
                )?;
                let r = &self.request;
                if permit.binding_digest != r.binding_digest
                    || permit.phase != r.phase
                    || permit.job_id != r.spec.job_id
                    || permit.session_id != r.spec.session_id
                    || permit.scope_id != r.spec.scope_id
                    || permit.checkpoint_digest != r.checkpoint_digest
                    || permit.checkpoint_sequence != r.checkpoint_sequence
                    || permit.ownership_generation != r.ownership.generation
                {
                    return Err(invalid("handoff permit names another request"));
                }
            }
        }
        Ok(reply)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{ExecutionOwnership, ExecutionSpec, SessionHandoffPhase};
    fn key() -> SigningIdentity {
        SigningIdentity::from_pkcs8(&SigningIdentity::generate_pkcs8().unwrap()).unwrap()
    }
    fn request(issuer: &SigningIdentity) -> SessionHandoffRequest {
        SessionHandoffRequest {
            issuer_identity: issuer.public_identity(),
            phase: SessionHandoffPhase::Receive,
            binding_digest: "ab".repeat(32),
            audience: "scope".into(),
            nonce: fresh_nonce().unwrap(),
            spec: ExecutionSpec {
                job_id: "job".into(),
                session_id: "session".into(),
                scope_id: "scope".into(),
                harness: "native".into(),
                harness_version: "1".into(),
                model_route_id: "openai".into(),
                gateway_account_id: "account".into(),
                model_id: "model".into(),
                required_capabilities: Default::default(),
            },
            checkpoint_digest: "cd".repeat(32),
            checkpoint_sequence: 1,
            ownership: ExecutionOwnership {
                node_id: 1,
                generation: 1,
            },
        }
    }
    #[test]
    fn signed_exchange_rejects_tampering_wrong_scope_issuer_and_replay() {
        let source = key();
        let target = key();
        let foreign = key();
        let r = request(&target);
        let sent = SignedHandoffRequest::new(
            &source,
            SessionHandoffWireRequest::Probe { request: r.clone() },
        )
        .unwrap();
        let verified =
            verify_request(sent.envelope.clone(), &target.public_identity(), "scope").unwrap();
        assert_eq!(verified.sender(), source.public_identity());
        let reply = verified
            .reply(
                &target,
                SessionHandoffWireReply::Challenge {
                    challenge: fresh_nonce().unwrap(),
                },
            )
            .unwrap();
        assert!(sent.verify_reply(reply.clone(), 1).is_ok());
        let second =
            SignedHandoffRequest::new(&source, SessionHandoffWireRequest::Probe { request: r })
                .unwrap();
        assert!(second.verify_reply(reply, 1).is_err());
        assert!(
            verify_request(sent.envelope.clone(), &target.public_identity(), "foreign").is_err()
        );
        assert!(
            verify_request(sent.envelope.clone(), &foreign.public_identity(), "scope").is_err()
        );
        assert!(verified
            .reply(
                &foreign,
                SessionHandoffWireReply::Challenge {
                    challenge: fresh_nonce().unwrap()
                }
            )
            .is_err());
        let mut tampered = sent.envelope;
        tampered["body"]["data"]["request"]["checkpointSequence"] = 9.into();
        assert!(verify_request(tampered, &target.public_identity(), "scope").is_err());
    }
}
