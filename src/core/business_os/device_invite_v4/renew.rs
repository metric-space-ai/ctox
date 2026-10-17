//! E54.5: one-use, bounded server challenges and registered-device signatures.
use super::*;
use ring::signature::{UnparsedPublicKey, ED25519};

const RENEW_DOMAIN: &[u8] = b"ctox-v4-device-renew\n";
const CHALLENGE_TTL_MS: i64 = 60_000;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RenewReject {
    DeviceUnknown,
    DeviceRevoked,
    SignatureInvalid,
    ChallengeExpired,
    RateLimited,
}
impl std::fmt::Display for RenewReject {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::DeviceUnknown => "device_unknown",
            Self::DeviceRevoked => "device_revoked",
            Self::SignatureInvalid => "signature_invalid",
            Self::ChallengeExpired => "challenge_expired",
            Self::RateLimited => "rate_limited",
        })
    }
}
impl std::error::Error for RenewReject {}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Renew {
    pub instance_id: String,
    pub device_id: String,
    pub nonce: String,
    pub signature: String,
}
#[derive(Serialize)]
pub struct Challenge {
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub nonce: String,
    pub expires_at_ms: i64,
}
pub struct PreparedRenew {
    request: Renew,
    pub principal: Principal,
    pub device_pubkey: String,
}
fn renew_message(instance: &str, device: &str, nonce: &str) -> Vec<u8> {
    [
        RENEW_DOMAIN,
        instance.as_bytes(),
        device.as_bytes(),
        nonce.as_bytes(),
    ]
    .concat()
}
fn check_signature(request: &Renew, public: &[u8; 32]) -> Result<(), RenewReject> {
    if request.signature.len() != 86 {
        return Err(RenewReject::SignatureInvalid);
    }
    let bytes = URL_SAFE_NO_PAD
        .decode(&request.signature)
        .map_err(|_| RenewReject::SignatureInvalid)?;
    if bytes.len() != 64 || URL_SAFE_NO_PAD.encode(&bytes) != request.signature {
        return Err(RenewReject::SignatureInvalid);
    }
    UnparsedPublicKey::new(&ED25519, public)
        .verify(
            &renew_message(&request.instance_id, &request.device_id, &request.nonce),
            &bytes,
        )
        .map_err(|_| RenewReject::SignatureInvalid)
}
pub(super) fn bind_launch(
    tx: &Transaction<'_>,
    device_key: &str,
    device_id: &str,
) -> Result<(), StoreError> {
    if device_id.is_empty() || device_id.len() > 256 {
        return Err(StoreError::Invalid("invalid issued device identity".into()));
    }
    // Exactly a device_id from an instance-signed E41 offer. JSON
    // supplied by an unpaired peer is never parsed as a device registry record.
    // Retain issued aliases: a lost renewal response must not strand the last
    // context actually received, and revocation by an older issued ID must work.
    tx.execute(
        "INSERT INTO _workjet_device_launches_v4(device_key,device_id) VALUES(?1,?2)",
        params![device_key, device_id],
    )?;
    Ok(())
}
impl Invites {
    pub async fn challenge(&self, instance: String) -> Result<Challenge, RenewReject> {
        let now = clock().map_err(|_| RenewReject::ChallengeExpired)?;
        let expires = now
            .checked_add(CHALLENGE_TTL_MS)
            .ok_or(RenewReject::ChallengeExpired)?;
        let mut random = [0; 32];
        SystemRandom::new()
            .fill(&mut random)
            .map_err(|_| RenewReject::ChallengeExpired)?;
        let nonce = URL_SAFE_NO_PAD.encode(random);
        let stored = nonce.clone();
        self.writer.submit(WriteJob::new(Default::default(), move |tx| {
            tx.execute("DELETE FROM _workjet_renew_challenges_v4 WHERE expires_ms<=?1",[now])?;
            let count:i64=tx.query_row("SELECT COUNT(*) FROM _workjet_renew_challenges_v4",[],|r|r.get(0))?;
            if count>=120 { return Ok(Err(RenewReject::RateLimited)); }
            tx.execute("INSERT INTO _workjet_renew_challenges_v4(nonce,instance_id,created_ms,expires_ms) VALUES(?1,?2,?3,?4)",params![stored,instance,now,expires])?;
            Ok(Ok(()))
        })).map_err(|_|RenewReject::ChallengeExpired)?.await.map_err(|_|RenewReject::ChallengeExpired)??;
        Ok(Challenge {
            kind: "pairing_challenge",
            nonce,
            expires_at_ms: expires,
        })
    }
    pub async fn prepare_renew(
        &self,
        instance: String,
        request: Renew,
    ) -> Result<PreparedRenew, RenewReject> {
        let now = clock().map_err(|_| RenewReject::ChallengeExpired)?;
        self.writer.submit(WriteJob::new(Default::default(),move |tx| {
            let prior:Option<(i64,i64)>=tx.query_row("SELECT window_ms,attempts FROM _workjet_pairing_budget_v4 WHERE singleton=1",[],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
            let (window,attempts)=prior.filter(|(start,_)|now>=*start && now-*start<60_000).unwrap_or((now,0));
            if attempts>=120 { return Ok(Err(RenewReject::RateLimited)); }
            tx.execute("INSERT INTO _workjet_pairing_budget_v4 VALUES(1,?1,?2) ON CONFLICT(singleton) DO UPDATE SET window_ms=excluded.window_ms,attempts=excluded.attempts",params![window,attempts+1])?;
            inspect_renew(tx,&instance,request,now)
        })).map_err(|_|RenewReject::DeviceUnknown)?.await.map_err(|_|RenewReject::DeviceUnknown)?
    }
    pub async fn consume_renew(
        &self,
        prepared: PreparedRenew,
        next_device_id: String,
    ) -> Result<(), RenewReject> {
        let now = clock().map_err(|_| RenewReject::ChallengeExpired)?;
        let instance = prepared.request.instance_id.clone();
        self.writer
            .submit(WriteJob::new(Default::default(), move |tx| {
                let current = match inspect_renew(tx, &instance, prepared.request, now)? {
                    Ok(v) => v,
                    Err(e) => return Ok(Err(e)),
                };
                if current.principal.id != prepared.principal.id
                    || current.principal.role != prepared.principal.role
                    || current.device_pubkey != prepared.device_pubkey
                {
                    return Ok(Err(RenewReject::DeviceRevoked));
                }
                bind_launch(tx, &current.device_pubkey, &next_device_id)?;
                tx.execute(
                    "UPDATE _workjet_renew_challenges_v4 SET used=1 WHERE nonce=?1",
                    [current.request.nonce],
                )?;
                Ok(Ok(()))
            }))
            .map_err(|_| RenewReject::DeviceUnknown)?
            .await
            .map_err(|_| RenewReject::DeviceUnknown)?
    }
    /// Called by the authenticated devices owner; never exposed to pairing JSON.
    pub async fn revoke_device(&self, device_id: String) -> Result<(), RenewReject> {
        self.writer.submit(WriteJob::new(Default::default(),move |tx| {
            let changed=tx.execute("UPDATE _workjet_devices_v4 SET active=0 WHERE device_key=(SELECT device_key FROM _workjet_device_launches_v4 WHERE device_id=?1)",[device_id])?;
            Ok(if changed==1 {Ok(())} else {Err(RenewReject::DeviceUnknown)})
        })).map_err(|_|RenewReject::DeviceUnknown)?.await.map_err(|_|RenewReject::DeviceUnknown)?
    }
}
fn inspect_renew(
    tx: &Transaction<'_>,
    instance: &str,
    request: Renew,
    now: i64,
) -> Result<Result<PreparedRenew, RenewReject>, StoreError> {
    if request.instance_id != instance
        || request.device_id.is_empty()
        || request.device_id.len() > 256
    {
        return Ok(Err(RenewReject::DeviceUnknown));
    }
    if decode(&request.nonce).is_none() {
        return Ok(Err(RenewReject::ChallengeExpired));
    }
    let challenge:Option<(String,i64,i64,bool)>=tx.query_row("SELECT instance_id,created_ms,expires_ms,used FROM _workjet_renew_challenges_v4 WHERE nonce=?1",[&request.nonce],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?;
    let Some((issued, created, expires, used)) = challenge else {
        return Ok(Err(RenewReject::ChallengeExpired));
    };
    if issued != instance
        || used
        || created > now
        || expires <= now
        || expires.checked_sub(created) != Some(CHALLENGE_TTL_MS)
    {
        return Ok(Err(RenewReject::ChallengeExpired));
    }
    let row:Option<(String,String,String,bool)>=tx.query_row("SELECT d.device_key,d.user_id,d.role,d.active FROM _workjet_devices_v4 d JOIN _workjet_device_launches_v4 l ON l.device_key=d.device_key WHERE l.device_id=?1",[&request.device_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?;
    let Some((public, user, role, active)) = row else {
        return Ok(Err(RenewReject::DeviceUnknown));
    };
    if !active {
        return Ok(Err(RenewReject::DeviceRevoked));
    }
    let Some(principal) = principal(tx, &user, &role)? else {
        return Ok(Err(RenewReject::DeviceRevoked));
    };
    let Some(raw) = decode(&public) else {
        return Ok(Err(RenewReject::SignatureInvalid));
    };
    if let Err(e) = check_signature(&request, &raw) {
        return Ok(Err(e));
    }
    Ok(Ok(PreparedRenew {
        request,
        principal,
        device_pubkey: public,
    }))
}
#[cfg(test)]
#[path = "renew_tests.rs"]
mod tests;
