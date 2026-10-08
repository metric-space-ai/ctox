// Origin: CTOX
// License: AGPL-3.0-only
//! Target speech authority and durable start-intent tombstones.
//! A signature/Build grant never substitutes for this local operator grant.
use super::*;
use ctox_sync::authority::auth::{
    public_key,
    speech_wire::{
        verify_request, SpeechComputerBinding, SpeechComputerReply, SpeechComputerRequest,
        SpeechDenial as Denial, SpeechOperation as Op, SpeechWorkload, VerifiedSpeechRequest,
    },
    SigningIdentity,
};
use std::sync::Arc;

const CONFIG_KEY: &str = "speech_computer_target_grants";
const LEDGER_KEY: &str = "speech_computer_target_intents";
const MAX_GRANTS: usize = 8;
const MAX_INTENTS: usize = 256;
const MAX_CONFIG_BYTES: usize = 32 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SpeechTargetGrant {
    pub scope_id: String,
    pub source_signing_identity: String,
    pub target_signing_identity: String,
    pub binding: SpeechComputerBinding,
    pub expires_at_unix_ms: u64,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SpeechTargetConfig {
    pub grants: Vec<SpeechTargetGrant>,
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Saved {
    epoch: String,
    config: SpeechTargetConfig,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Intent {
    key: String,
    digest: String,
    object_id: String,
    host_generation: String,
    expires_at_unix_ms: u64,
}
#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Ledger {
    intents: Vec<Intent>,
}
pub(crate) enum IntentAdmission {
    New(String),
    Existing(String),
}
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}
fn digest(value: &impl Serialize) -> Result<String, Denial> {
    let bytes = serde_json::to_vec(value).map_err(|_| Denial::GrantDenied)?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}
fn load(root: &Path) -> Result<Saved, Denial> {
    crate::persistence::load_json_payload(root, CONFIG_KEY)
        .map_err(|_| Denial::RouteRetired)?
        .ok_or(Denial::GrantDenied)
}
fn validate(grant: &SpeechTargetGrant, now: u64) -> anyhow::Result<()> {
    public_key(&grant.source_signing_identity)?;
    public_key(&grant.target_signing_identity)?;
    anyhow::ensure!(
        grant.source_signing_identity != grant.target_signing_identity,
        "speech target must differ from source"
    );
    anyhow::ensure!(
        grant.expires_at_unix_ms > now && grant.expires_at_unix_ms - now <= 86_400_000,
        "speech grant must expire within one day"
    );
    let operation = match grant.binding.workload {
        SpeechWorkload::Transcription => {
            anyhow::ensure!(
                grant.binding.model == "engineai/Voxtral-Mini-4B-Realtime-2602",
                "speech target transcription grant requires the local Voxtral model"
            );
            Op::OpenTranscription {
                intent_id: "validate".into(),
                sample_rate_hz: 16_000,
            }
        }
        SpeechWorkload::Synthesis => {
            anyhow::ensure!(
                grant.binding.model == "engineai/Voxtral-4B-TTS-2603",
                "speech target synthesis grant requires the local Voxtral model"
            );
            Op::StartSynthesis {
                intent_id: "validate".into(),
                text: "validate".into(),
                voice_id: "validate".into(),
            }
        }
    };
    SpeechComputerRequest {
        binding: grant.binding.clone(),
        operation,
    }
    .validate()?;
    Ok(())
}
impl SpeechTargetConfig {
    /// Existing local operator boundary only. This cannot provision a signing
    /// key, change another client or authorize a browser caller.
    pub fn save(&self, root: &Path) -> anyhow::Result<()> {
        anyhow::ensure!(self.grants.len() <= MAX_GRANTS, "too many speech grants");
        let host = crate::sync_host::handoff_configuration(root)?;
        let mut seen = std::collections::BTreeSet::new();
        for grant in &self.grants {
            validate(grant, now_ms())?;
            anyhow::ensure!(
                grant.scope_id == host.scope_id,
                "speech grant scope differs from native host"
            );
            anyhow::ensure!(
                seen.insert((grant.binding.grant_id.clone(), grant.binding.grant_revision)),
                "duplicate speech grant revision"
            );
        }
        crate::sync_host::with_current_signing_identity(root, |identity| {
            host.validate_key(identity)?;
            for grant in &self.grants {
                anyhow::ensure!(
                    grant.target_signing_identity == identity.public_identity(),
                    "speech grant target differs from current native identity"
                );
            }
            crate::persistence::store_json_payload(
                root,
                CONFIG_KEY,
                Some(&Saved {
                    epoch: Uuid::new_v4().to_string(),
                    config: self.clone(),
                }),
            )
        })
    }
}
pub fn configure_from_file(root: &Path, path: &Path) -> anyhow::Result<SpeechTargetConfig> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(MAX_CONFIG_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    anyhow::ensure!(
        bytes.len() <= MAX_CONFIG_BYTES,
        "speech target configuration exceeds its budget"
    );
    let config: SpeechTargetConfig = serde_json::from_slice(&bytes)?;
    config.save(root)?;
    Ok(config)
}

/// Private, non-deserializable authority snapshot. Receiver code must still
/// fence the live host and exact native connection at effects/publication.
pub(crate) struct TargetPolicy {
    root: PathBuf,
    saved: Saved,
    grant: SpeechTargetGrant,
}
impl TargetPolicy {
    pub(crate) fn verify(
        root: &Path,
        envelope: Value,
    ) -> Result<(Arc<Self>, Arc<VerifiedSpeechRequest>), Denial> {
        let host =
            crate::sync_host::handoff_configuration(root).map_err(|_| Denial::RouteRetired)?;
        crate::sync_host::with_current_signing_identity(root, |identity| {
            Ok((|| {
                host.validate_key(identity)
                    .map_err(|_| Denial::RouteRetired)?;
                let verified =
                    verify_request(envelope, &identity.public_identity(), &host.scope_id)
                        .map_err(|_| Denial::GrantDenied)?;
                let saved = load(root)?;
                let grant = saved
                    .config
                    .grants
                    .iter()
                    .find(|grant| {
                        grant.scope_id == host.scope_id
                            && grant.source_signing_identity == verified.sender()
                            && grant.target_signing_identity == identity.public_identity()
                            && grant.binding == verified.request().binding
                    })
                    .cloned()
                    .ok_or(Denial::GrantDenied)?;
                if grant.expires_at_unix_ms <= now_ms() {
                    return Err(Denial::GrantExpired);
                }
                Ok((
                    Arc::new(Self {
                        root: root.into(),
                        saved,
                        grant,
                    }),
                    Arc::new(verified),
                ))
            })())
        })
        .map_err(|_| Denial::RouteRetired)?
    }

    pub(crate) fn with_current<T>(
        &self,
        apply: impl FnOnce(&SigningIdentity) -> Result<T, Denial>,
    ) -> Result<T, Denial> {
        crate::sync_host::with_current_signing_identity(&self.root, |identity| {
            Ok((|| {
                if identity.public_identity() != self.grant.target_signing_identity {
                    return Err(Denial::RouteRetired);
                }
                if self.grant.expires_at_unix_ms <= now_ms() {
                    return Err(Denial::GrantExpired);
                }
                if load(&self.root)? != self.saved {
                    return Err(Denial::GrantDenied);
                }
                apply(identity)
            })())
        })
        .map_err(|_| Denial::RouteRetired)?
    }

    pub(crate) fn reply(
        &self,
        request: &VerifiedSpeechRequest,
        reply: SpeechComputerReply,
    ) -> Result<Value, Denial> {
        if request.sender() != self.grant.source_signing_identity
            || request.request().binding != self.grant.binding
        {
            return Err(Denial::GrantDenied);
        }
        self.with_current(|identity| {
            request
                .reply(identity, reply)
                .map_err(|_| Denial::InvalidSequence)
        })
    }

    /// Commit before opening IPC or starting synthesis. Existing intents must
    /// resolve their exact live object/connection; they never restart effects.
    /// Claims survive host restart/revocation until grant expiry. Raw PCM, text,
    /// voice and credentials are never stored here, only request digests.
    pub(crate) fn reserve_intent(
        &self,
        request: &VerifiedSpeechRequest,
        generation: &str,
    ) -> Result<IntentAdmission, Denial> {
        if generation.is_empty() || generation.len() > 64 {
            return Err(Denial::RouteRetired);
        }
        let intent_id = match &request.request().operation {
            Op::OpenTranscription { intent_id, .. } | Op::StartSynthesis { intent_id, .. } => {
                intent_id
            }
            _ => return Err(Denial::InvalidSequence),
        };
        if request.sender() != self.grant.source_signing_identity
            || request.request().binding != self.grant.binding
        {
            return Err(Denial::GrantDenied);
        }
        let key = digest(&(&self.grant, intent_id))?;
        let request_digest = digest(request.request())?;
        self.with_current(|_| {
            let mut ledger: Ledger = crate::persistence::load_json_payload(&self.root, LEDGER_KEY)
                .map_err(|_| Denial::RouteRetired)?
                .unwrap_or_default();
            if ledger.intents.len() > MAX_INTENTS {
                return Err(Denial::Backpressure);
            }
            ledger
                .intents
                .retain(|claim| claim.expires_at_unix_ms > now_ms());
            if let Some(claim) = ledger.intents.iter().find(|claim| claim.key == key) {
                if claim.digest != request_digest {
                    return Err(Denial::InvalidSequence);
                }
                if claim.host_generation != generation {
                    return Err(Denial::RouteRetired);
                }
                return Ok(IntentAdmission::Existing(claim.object_id.clone()));
            }
            if ledger.intents.len() >= MAX_INTENTS {
                return Err(Denial::Backpressure);
            }
            let id = Uuid::new_v4().to_string();
            ledger.intents.push(Intent {
                key,
                digest: request_digest,
                object_id: id.clone(),
                host_generation: generation.into(),
                expires_at_unix_ms: self.grant.expires_at_unix_ms,
            });
            crate::persistence::store_json_payload(&self.root, LEDGER_KEY, Some(&ledger))
                .map_err(|_| Denial::RouteRetired)?;
            Ok(IntentAdmission::New(id))
        })
    }
}

#[cfg(test)]
#[path = "speech_target_policy_tests.rs"]
mod tests;
