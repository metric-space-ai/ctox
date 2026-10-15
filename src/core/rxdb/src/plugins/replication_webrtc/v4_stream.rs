//! Sync v4 protocol values only. Transport scheduling, retries and key custody belong to their owners.
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Seq(pub u64);
impl Serialize for Seq {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.0.to_string())
    }
}
impl<'de> Deserialize<'de> for Seq {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        let text = String::deserialize(d)?;
        if text.is_empty()
            || !text.bytes().all(|b| b.is_ascii_digit())
            || (text.len() > 1 && text.starts_with('0'))
        {
            return Err(D::Error::custom("canonical decimal sequence required"));
        }
        text.parse().map(Self).map_err(D::Error::custom)
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Checkpoint {
    pub instance_id: String,
    pub participant: String,
    pub collection: String,
    pub scope_hash: String,
    pub gen: u64,
    pub epoch: String,
    pub seq: Seq,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Subscribe {
    pub collection: String,
    pub scope_hash: String,
    pub gen: u64,
    pub checkpoint: Option<Checkpoint>,
    pub priority: Priority,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Priority {
    P0,
    P1,
    P2,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StreamKind {
    Page,
    Live,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocumentOp {
    Upsert,
    Delete,
    Unproject,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Document {
    pub id: String,
    pub seq: Seq,
    pub op: DocumentOp,
    pub rev: String,
    pub schema_ver: u32,
    pub app_ver: String,
    pub body: Option<Value>,
}
/// The signature covers EVERY field other than sig, including all document bodies and the recipient.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StreamPacket {
    #[serde(rename = "type")]
    pub kind: StreamKind,
    pub instance_id: String,
    pub key_id: String,
    pub participant: String,
    pub collection: String,
    pub gen: u64,
    pub package_id: String,
    pub seq_from: Seq,
    pub seq_to: Seq,
    pub head_seq: Seq,
    pub scope_hash: String,
    pub epoch: String,
    pub core_sent_at_ms: u64,
    pub backlog: u64,
    pub complete: bool,
    pub docs: Vec<Document>,
    pub sig: Vec<u8>,
}
/// Production implementations fetch/use the instance key through the native secret provider.
/// The provider pins one immutable instance/key identity for the packet's signing lifetime.
/// There is deliberately no default signer, environment key, or caller-supplied key material.
pub trait InstanceSigner: Send + Sync {
    fn instance_id(&self) -> &str;
    fn key_id(&self) -> &str;
    fn sign(&self, canonical: &[u8]) -> Result<Vec<u8>, String>;
}
pub trait InstanceVerifier {
    fn verify(
        &self,
        instance: &str,
        key_id: &str,
        canonical: &[u8],
        signature: &[u8],
    ) -> Result<(), String>;
}
fn canonical(value: &Value, out: &mut Vec<u8>) -> Result<(), String> {
    match value {
        Value::Object(map) => {
            out.push(b'{');
            let mut keys: Vec<_> = map.keys().collect();
            keys.sort();
            for (i, key) in keys.into_iter().enumerate() {
                if i > 0 {
                    out.push(b',');
                }
                out.extend(serde_json::to_vec(key).map_err(|e| e.to_string())?);
                out.push(b':');
                canonical(&map[key], out)?;
            }
            out.push(b'}');
        }
        Value::Array(rows) => {
            out.push(b'[');
            for (i, row) in rows.iter().enumerate() {
                if i > 0 {
                    out.push(b',');
                }
                canonical(row, out)?;
            }
            out.push(b']');
        }
        other => out.extend(serde_json::to_vec(other).map_err(|e| e.to_string())?),
    }
    Ok(())
}
impl StreamPacket {
    pub fn signing_bytes(&self) -> Result<Vec<u8>, String> {
        let mut value = serde_json::to_value(self).map_err(|e| e.to_string())?;
        value
            .as_object_mut()
            .ok_or("packet object required")?
            .remove("sig");
        let mut bytes = b"ctox.sync.v4.stream\0".to_vec();
        canonical(&value, &mut bytes)?;
        Ok(bytes)
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.instance_id.is_empty()
            || self.key_id.is_empty()
            || self.participant.is_empty()
            || self.collection.is_empty()
            || self.scope_hash.is_empty()
            || self.epoch.is_empty()
            || self.package_id.is_empty()
            || self.gen == 0
            || self.seq_from > self.seq_to
            || self.seq_to > self.head_seq
            || self.complete != (self.seq_to == self.head_seq)
        {
            return Err("invalid packet binding/range".into());
        }
        let mut previous = self.seq_from;
        let mut ids = std::collections::HashSet::new();
        for doc in &self.docs {
            if doc.id.is_empty()
                || !ids.insert(&doc.id)
                || doc.seq <= previous
                || doc.seq > self.seq_to
                || (doc.op == DocumentOp::Upsert
                    && !doc.body.as_ref().is_some_and(Value::is_object))
                || (doc.op != DocumentOp::Upsert && doc.body.is_some())
            {
                return Err("invalid packet document".into());
            }
            previous = doc.seq;
        }
        Ok(())
    }
    pub fn sign(&mut self, key: &dyn InstanceSigner) -> Result<(), String> {
        self.instance_id = key.instance_id().into();
        self.key_id = key.key_id().into();
        self.validate()?;
        self.sig = key.sign(&self.signing_bytes()?)?;
        if self.sig.is_empty() {
            return Err("empty instance signature".into());
        }
        Ok(())
    }
    pub fn verify(&self, key: &dyn InstanceVerifier) -> Result<(), String> {
        self.validate()?;
        if self.sig.is_empty() {
            return Err("unsigned packet".into());
        }
        key.verify(
            &self.instance_id,
            &self.key_id,
            &self.signing_bytes()?,
            &self.sig,
        )
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WriteOp {
    Put,
    Patch,
    Delete,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Write {
    pub op_id: String,
    pub prev_op_id: Option<String>,
    pub collection: String,
    pub id: String,
    pub base_rev: Option<String>,
    pub base_seq: Option<Seq>,
    pub gen: u64,
    pub schema_ver: u32,
    pub app_ver: String,
    pub op: WriteOp,
    pub body: Option<Value>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WriteStatus {
    Accepted,
    Merged,
    Rejected,
    Migrated,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WriteResult {
    pub op_id: String,
    pub status: WriteStatus,
    pub seq: Option<Seq>,
    pub rev: Option<String>,
    pub gen: u64,
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    pub archived_ref: Option<String>,
}
/// Participant identity is supplied by the admitted native connection, never this payload.
/// client_seq starts at 1 for an admitted participant/session identity and is a decimal string.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrderedWrite {
    pub client_seq: Seq,
    #[serde(flatten)]
    pub write: Write,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrderedWriteResult {
    pub client_seq: Seq,
    /// Merged/migrated results carry no unmasked business payload. The consumer MUST
    /// wait for the authoritative normal page (seq >= result.seq), rather than stamp
    /// its own optimistic body with this revision/seq. Keep the outbox until then.
    pub requires_page: bool,
    #[serde(flatten)]
    pub result: WriteResult,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    AppActivation(AppActivation),
    Subscribe(Subscribe),
    Write(OrderedWrite),
    ClockSync(ClockProbe),
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CoreMessage {
    AppActivationResult(AppActivationReceipt),
    RequiresPage(RequiresPage),
    WriteResult(OrderedWriteResult),
    HelloOk(HelloOk),
}
/// Restart this collection in its own generation namespace, retaining the old
/// local view until complete. gen=None means the collection was removed.
/// Scope admission remains mandatory for the replacement subscription.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequiresPage {
    pub collection: String,
    pub scope_hash: String,
    pub previous_gen: u64,
    pub gen: Option<u64>,
    pub checkpoint_seq: Seq,
    pub migrated_at_seq: Option<Seq>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppActivationState {
    Received,
    Activated,
    Failed,
}
/// An authenticated CLIENT acknowledgement, never inferred from sending a manifest
/// or page. The Kernel commits its local pointer before reporting Activated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppActivation {
    pub app_id: String,
    pub app_ver: String,
    pub state: AppActivationState,
    pub at_seq: Seq,
    pub checkpoints: Vec<Checkpoint>,
    pub reason: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppActivationReceipt {
    pub app_id: String,
    pub app_ver: String,
    pub state: AppActivationState,
    pub at_seq: Seq,
    pub reason: Option<String>,
    pub recorded_at_ms: u64,
}

/// Scope token values are issued/validated by P6, never minted by this codec.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HelloOk {
    pub epoch: String,
    pub head_seq: Seq,
    pub scope_tokens: Vec<Value>,
    pub rights_ver: u64,
    pub clock: ClockReply,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClockProbe {
    pub nonce: String,
    pub client_sent_at_ms: u64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClockReply {
    pub nonce: String,
    pub client_sent_at_ms: u64,
    pub core_received_at_ms: u64,
    pub core_sent_at_ms: u64,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClockEstimate {
    pub round_trip_ms: u64,
    pub core_minus_client_ms: i64,
    pub uncertainty_ms: u64,
}
impl ClockReply {
    pub fn estimate(
        &self,
        probe: &ClockProbe,
        client_received_at_ms: u64,
    ) -> Result<ClockEstimate, String> {
        if self.nonce != probe.nonce
            || self.client_sent_at_ms != probe.client_sent_at_ms
            || client_received_at_ms < probe.client_sent_at_ms
            || self.core_sent_at_ms < self.core_received_at_ms
        {
            return Err("invalid clock exchange".into());
        }
        let elapsed = client_received_at_ms - probe.client_sent_at_ms;
        let processing = self.core_sent_at_ms - self.core_received_at_ms;
        let rtt = elapsed
            .checked_sub(processing)
            .ok_or("clock changed during exchange")?;
        let offset = ((self.core_received_at_ms as i128 - probe.client_sent_at_ms as i128)
            + (self.core_sent_at_ms as i128 - client_received_at_ms as i128))
            / 2;
        Ok(ClockEstimate {
            round_trip_ms: rtt,
            core_minus_client_ms: i64::try_from(offset).map_err(|_| "clock offset overflow")?,
            uncertainty_ms: rtt.div_ceil(2),
        })
    }
}
/// Exact signed bytes for the P5 reliable message payload. Receivers verify these
/// bytes BEFORE dispatching business values; JS number/stringify rules cannot change the signature.
/// The domain prefix is part of payload. P5 owns framing and negotiated byte budgets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedFrame {
    pub payload: Vec<u8>,
    pub sig: Vec<u8>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamBinding {
    pub instance_id: String,
    pub participant: String,
    pub collection: String,
    pub scope_hash: String,
    pub gen: u64,
    pub epoch: String,
}
#[derive(Debug, Clone, PartialEq)]
pub struct VerifiedPacket(StreamPacket);
impl VerifiedPacket {
    pub fn packet(&self) -> &StreamPacket {
        &self.0
    }
}
impl StreamPacket {
    pub fn signed_frame(&self) -> Result<SignedFrame, String> {
        self.validate()?;
        if self.sig.is_empty() {
            return Err("unsigned packet".into());
        }
        Ok(SignedFrame {
            payload: self.signing_bytes()?,
            sig: self.sig.clone(),
        })
    }
}
impl SignedFrame {
    pub fn verify(
        &self,
        expected: &StreamBinding,
        maximum_bytes: usize,
        key: &dyn InstanceVerifier,
    ) -> Result<VerifiedPacket, String> {
        if self.payload.len() > maximum_bytes || self.sig.is_empty() {
            return Err("invalid signed frame size/signature".into());
        }
        let bytes = self
            .payload
            .strip_prefix(b"ctox.sync.v4.stream\0")
            .ok_or("wrong signature domain")?;
        let mut value: Value = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        let object = value.as_object_mut().ok_or("packet object required")?;
        if object.contains_key("sig") {
            return Err("signature must be detached".into());
        }
        let instance = object
            .get("instance_id")
            .and_then(Value::as_str)
            .ok_or("missing instance")?;
        let key_id = object
            .get("key_id")
            .and_then(Value::as_str)
            .ok_or("missing key id")?;
        if instance != expected.instance_id {
            return Err("wrong admitted instance".into());
        }
        key.verify(instance, key_id, &self.payload, &self.sig)?;
        object.insert(
            "sig".into(),
            serde_json::to_value(&self.sig).map_err(|e| e.to_string())?,
        );
        let packet: StreamPacket = serde_json::from_value(value).map_err(|e| e.to_string())?;
        packet.validate()?;
        if packet.participant != expected.participant
            || packet.collection != expected.collection
            || packet.scope_hash != expected.scope_hash
            || packet.gen != expected.gen
            || packet.epoch != expected.epoch
        {
            return Err("wrong subscription binding".into());
        }
        Ok(VerifiedPacket(packet))
    }
}

/// Per-document admission gate. The durable client must persist this version,
/// the document effect and packet checkpoint in the SAME local transaction.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DocumentVersion(pub Seq);
impl DocumentVersion {
    pub fn admit(&mut self, incoming: Seq) -> bool {
        if incoming > self.0 {
            self.0 = incoming;
            true
        } else {
            false
        }
    }
}
