//! Strict validation for portable Codex rollout journals.
//!
//! This boundary is intentionally separate from local rollout recovery: it
//! accepts one exact byte string, requires the artifact/transport boundary to
//! name the supported format/version, binds those bytes to a SHA-256 artifact
//! identity, and fails closed on malformed, truncated, duplicate, unknown,
//! unsupported, empty, or identity-conflicting content. Diagnostics report
//! safe structural facts only and never include journal bytes.
//!
//! A successful result proves syntax and session identity only. It does not
//! prove that a provider can resume from provider state or that recorded
//! external effects have been reconciled.

use crate::ThreadId;
use crate::protocol::RolloutItem;
use crate::protocol::RolloutLine;
use crate::protocol::SessionMetaLine;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fmt;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

/// Stable format carried by the validated byte string.
pub const PORTABLE_CODEX_JOURNAL_FORMAT: &str = "ctox-codex-rollout-jsonl";
/// Version of the strict validation contract in this crate.
pub const PORTABLE_CODEX_JOURNAL_VERSION: u32 = 1;

/// Upper bounds enforced before any journal record is decoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PortableJournalLimits {
    pub max_bytes: u64,
    pub max_line_bytes: u64,
    pub max_records: usize,
}

impl Default for PortableJournalLimits {
    fn default() -> Self {
        Self {
            max_bytes: 64 * 1024 * 1024,
            max_line_bytes: 16 * 1024 * 1024,
            max_records: 100_000,
        }
    }
}

/// Format/version metadata carried by the artifact or transport boundary.
///
/// This is supplied by the caller from an explicit manifest or envelope field.
/// Validation never infers it from journal bytes and never accepts another
/// version merely because those bytes happen to decode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PortableJournalFormat {
    pub format: &'static str,
    pub format_version: u32,
}

impl PortableJournalFormat {
    pub fn current() -> Self {
        Self {
            format: PORTABLE_CODEX_JOURNAL_FORMAT,
            format_version: PORTABLE_CODEX_JOURNAL_VERSION,
        }
    }
}

/// Content-addressed identity of the exact journal bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortableArtifactRef {
    pub sha256: String,
    pub size_bytes: u64,
}

/// Session identity expected by the portable transport.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PortableJournalExpectation {
    pub format: PortableJournalFormat,
    pub session_id: ThreadId,
}

/// Explicit statement of what validation did and did not establish.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderContinuationState {
    /// No provider adapter was executed and provider resume is not certified.
    Unresolved,
}

/// Explicit statement of external-effect evidence in the journal itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExternalEffectState {
    /// Journal syntax carries no proof that external effects are reconciled.
    Unknown,
}

/// Safe, bounded result of validation.
#[derive(Debug)]
pub struct ValidatedPortableJournal {
    pub format: &'static str,
    pub format_version: u32,
    pub artifact: PortableArtifactRef,
    pub session_id: ThreadId,
    pub record_count: usize,
    pub limits: PortableJournalLimits,
    pub provider_continuation: ProviderContinuationState,
    pub external_effects: ExternalEffectState,
    /// Decoded records for the caller's bounded import path.
    pub items: Vec<RolloutItem>,
}

/// Safe diagnostics. Line numbers are ordinal; no payload or source text is kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum PortableJournalError {
    UnsupportedInputFormat,
    ArtifactSizeMismatch,
    ArtifactHashMismatch,
    OversizedArtifact,
    OversizedLine,
    TooManyRecords,
    EmptyJournal,
    UnterminatedFinalRecord,
    InvalidUtf8,
    InvalidLine,
    UnsupportedRecord,
    UnsupportedRecordField,
    DuplicateJsonKey,
    MissingMetadataField,
    InvalidMetadata,
    DuplicateSessionMetadata,
    MissingInitialMetadata,
    EmptyHistory,
    IdentityMismatch,
}

impl fmt::Display for PortableJournalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::UnsupportedInputFormat => "portable journal input format is unsupported",
            Self::ArtifactSizeMismatch => "portable journal artifact length mismatch",
            Self::ArtifactHashMismatch => "portable journal artifact hash mismatch",
            Self::OversizedArtifact => "portable journal exceeds its byte budget",
            Self::OversizedLine => "portable journal contains an oversized record",
            Self::TooManyRecords => "portable journal exceeds its record budget",
            Self::EmptyJournal => "portable journal is empty",
            Self::UnterminatedFinalRecord => "portable journal final record is not terminated",
            Self::InvalidUtf8 => "portable journal is not valid UTF-8",
            Self::InvalidLine => "portable journal contains a malformed record",
            Self::UnsupportedRecord => "portable journal contains an unsupported record variant",
            Self::UnsupportedRecordField => "portable journal contains an unsupported record field",
            Self::DuplicateJsonKey => "portable journal contains duplicate JSON keys",
            Self::MissingMetadataField => "portable journal metadata is incomplete",
            Self::InvalidMetadata => "portable journal metadata is invalid",
            Self::DuplicateSessionMetadata => "portable journal has conflicting session metadata",
            Self::MissingInitialMetadata => "portable journal does not start with session metadata",
            Self::EmptyHistory => "portable journal has no history records",
            Self::IdentityMismatch => "portable journal session identity mismatch",
        };
        f.write_str(message)
    }
}

impl std::error::Error for PortableJournalError {}

fn artifact_hash(raw: &[u8]) -> String {
    format!("{:x}", Sha256::digest(raw))
}

fn safe_hex(hash: &str) -> bool {
    hash.len() == 64
        && hash
            .bytes()
            .all(|byte| byte.is_ascii_digit() || ('a'..='f').contains(&byte))
}

fn validate_input_format(format: &PortableJournalFormat) -> Result<(), PortableJournalError> {
    if *format != PortableJournalFormat::current() {
        return Err(PortableJournalError::UnsupportedInputFormat);
    }
    Ok(())
}

/// Validate exact bytes against an explicit format, artifact identity, and expected session.
///
/// Parsing is bounded before decoding: format, artifact size, line size, record
/// count, line termination, UTF-8, duplicate JSON keys, and the strict record
/// shape are checked before typed decoding.
pub fn validate_portable_journal(
    raw: &[u8],
    artifact: &PortableArtifactRef,
    expected: &PortableJournalExpectation,
    limits: &PortableJournalLimits,
) -> Result<ValidatedPortableJournal, PortableJournalError> {
    validate_input_format(&expected.format)?;
    if raw.len() as u64 != artifact.size_bytes {
        return Err(PortableJournalError::ArtifactSizeMismatch);
    }
    if raw.len() as u64 > limits.max_bytes {
        return Err(PortableJournalError::OversizedArtifact);
    }
    if !safe_hex(&artifact.sha256) || artifact_hash(raw) != artifact.sha256 {
        return Err(PortableJournalError::ArtifactHashMismatch);
    }
    if raw.is_empty() {
        return Err(PortableJournalError::EmptyJournal);
    }
    if raw.last() != Some(&b'\n') {
        return Err(PortableJournalError::UnterminatedFinalRecord);
    }

    let mut items = Vec::new();
    let mut metadata: Option<SessionMetaLine> = None;
    let mut record_count = 0_usize;
    let mut offset = 0_usize;

    while offset < raw.len() {
        if record_count == limits.max_records {
            return Err(PortableJournalError::TooManyRecords);
        }
        let line_end = raw[offset..]
            .iter()
            .position(|byte| *byte == b'\n')
            .ok_or(PortableJournalError::UnterminatedFinalRecord)?;
        let line = &raw[offset..line_end];
        offset = line_end + 1;
        if line.len() as u64 > limits.max_line_bytes {
            return Err(PortableJournalError::OversizedLine);
        }
        record_count += 1;

        let text = std::str::from_utf8(line).map_err(|_| PortableJournalError::InvalidUtf8)?;
        if text.ends_with('\r') {
            return Err(PortableJournalError::InvalidLine);
        }
        detect_duplicate_json_keys(text)?;
        let value: Value =
            serde_json::from_str(text).map_err(|_| PortableJournalError::InvalidLine)?;
        require_exact_keys(&value, ["timestamp", "type", "payload"])?;
        let record_type = value
            .get("type")
            .and_then(Value::as_str)
            .ok_or(PortableJournalError::InvalidLine)?;

        let is_metadata = record_type == "session_meta";
        if record_count == 1 && !is_metadata {
            return Err(PortableJournalError::MissingInitialMetadata);
        }
        if is_metadata {
            if metadata.is_some() {
                return Err(PortableJournalError::DuplicateSessionMetadata);
            }
            validate_required_metadata_shape(&value)?;
        } else if !matches!(
            record_type,
            "response_item" | "compacted" | "turn_context" | "event_msg"
        ) {
            return Err(PortableJournalError::UnsupportedRecord);
        }

        let mut unknown_field = false;
        let mut deserializer = serde_json::Deserializer::from_str(text);
        let parsed: Result<RolloutLine, _> =
            serde_ignored::deserialize(&mut deserializer, |_path: serde_ignored::Path| {
                unknown_field = true
            });
        if unknown_field {
            return Err(PortableJournalError::UnsupportedRecordField);
        }
        let line = parsed.map_err(|_| PortableJournalError::InvalidLine)?;
        deserializer
            .end()
            .map_err(|_| PortableJournalError::InvalidLine)?;
        validate_line_timestamp(&line.timestamp)?;

        match line.item {
            RolloutItem::SessionMeta(session_meta) => {
                validate_metadata(&session_meta, expected)?;
                metadata = Some(session_meta);
            }
            RolloutItem::ResponseItem(_)
            | RolloutItem::Compacted(_)
            | RolloutItem::TurnContext(_)
            | RolloutItem::EventMsg(_) => {
                items.push(line.item);
            }
        }
    }

    let session_meta = metadata.ok_or(PortableJournalError::MissingInitialMetadata)?;
    if session_meta.meta.id != expected.session_id {
        return Err(PortableJournalError::IdentityMismatch);
    }
    if items.is_empty() {
        return Err(PortableJournalError::EmptyHistory);
    }

    Ok(ValidatedPortableJournal {
        format: expected.format.format,
        format_version: expected.format.format_version,
        artifact: PortableArtifactRef {
            sha256: artifact.sha256.clone(),
            size_bytes: artifact.size_bytes,
        },
        session_id: session_meta.meta.id,
        record_count,
        limits: *limits,
        provider_continuation: ProviderContinuationState::Unresolved,
        external_effects: ExternalEffectState::Unknown,
        items,
    })
}

fn validate_line_timestamp(timestamp: &str) -> Result<(), PortableJournalError> {
    OffsetDateTime::parse(timestamp, &Rfc3339)
        .map(|_| ())
        .map_err(|_| PortableJournalError::InvalidMetadata)
}

fn require_exact_keys<const COUNT: usize>(
    value: &Value,
    keys: [&str; COUNT],
) -> Result<(), PortableJournalError> {
    let object = value.as_object().ok_or(PortableJournalError::InvalidLine)?;
    if object.len() != COUNT || keys.iter().any(|key| !object.contains_key(*key)) {
        return Err(PortableJournalError::UnsupportedRecordField);
    }
    Ok(())
}

/// Strict required-field check is separate from typed decoding so an omitted
/// `Option` cannot silently become `None` and pass portable validation.
fn validate_required_metadata_shape(line: &Value) -> Result<(), PortableJournalError> {
    let object = line.as_object().ok_or(PortableJournalError::InvalidLine)?;
    let payload = object
        .get("payload")
        .and_then(Value::as_object)
        .ok_or(PortableJournalError::InvalidLine)?;
    const REQUIRED: [&str; 9] = [
        "id",
        "timestamp",
        "cwd",
        "originator",
        "cli_version",
        "source",
        "model_provider",
        "base_instructions",
        "capability_profile",
    ];
    if REQUIRED
        .iter()
        .any(|field| payload.get(*field).is_none_or(|value| value.is_null()))
    {
        return Err(PortableJournalError::MissingMetadataField);
    }
    Ok(())
}

fn validate_metadata(
    session_meta: &SessionMetaLine,
    expected: &PortableJournalExpectation,
) -> Result<(), PortableJournalError> {
    validate_line_timestamp(&session_meta.meta.timestamp)?;
    if session_meta.meta.id != expected.session_id
        || session_meta.meta.id.to_string() == "00000000-0000-0000-0000-000000000000"
        || session_meta.meta.cwd.as_os_str().is_empty()
        || session_meta.meta.originator.is_empty()
        || session_meta.meta.cli_version.is_empty()
        || session_meta
            .meta
            .model_provider
            .as_deref()
            .is_none_or(str::is_empty)
        || session_meta.meta.base_instructions.is_none()
        || session_meta.meta.capability_profile.is_none()
    {
        return Err(PortableJournalError::InvalidMetadata);
    }
    if let Some(forked_from_id) = session_meta.meta.forked_from_id
        && forked_from_id == session_meta.meta.id
    {
        return Err(PortableJournalError::InvalidMetadata);
    }
    Ok(())
}

/// Decode JSON strings while preserving escaped/unescaped key equivalence.
struct JsonDuplicateKeyScanner<'a> {
    input: &'a [u8],
    offset: usize,
    depth: usize,
}

impl<'a> JsonDuplicateKeyScanner<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            input: input.as_bytes(),
            offset: 0,
            depth: 0,
        }
    }

    fn validate(mut self) -> Result<(), PortableJournalError> {
        self.skip_whitespace();
        self.parse_value()?;
        self.skip_whitespace();
        if self.offset != self.input.len() {
            return Err(PortableJournalError::InvalidLine);
        }
        Ok(())
    }

    fn peek(&self) -> Option<u8> {
        self.input.get(self.offset).copied()
    }

    fn skip_whitespace(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\r' | b'\n')) {
            self.offset += 1;
        }
    }

    fn expect(&mut self, byte: u8) -> Result<(), PortableJournalError> {
        if self.peek() == Some(byte) {
            self.offset += 1;
            Ok(())
        } else {
            Err(PortableJournalError::InvalidLine)
        }
    }

    fn parse_value(&mut self) -> Result<(), PortableJournalError> {
        self.skip_whitespace();
        match self.peek() {
            Some(b'{') => self.parse_object(),
            Some(b'[') => self.parse_array(),
            Some(b'"') => {
                self.parse_string()?;
                Ok(())
            }
            Some(_) => self.parse_scalar(),
            None => Err(PortableJournalError::InvalidLine),
        }
    }

    fn enter(&mut self) -> Result<(), PortableJournalError> {
        self.depth += 1;
        if self.depth <= 128 {
            Ok(())
        } else {
            Err(PortableJournalError::InvalidLine)
        }
    }

    fn parse_object(&mut self) -> Result<(), PortableJournalError> {
        self.enter()?;
        self.expect(b'{')?;
        let mut keys = HashSet::new();
        self.skip_whitespace();
        if self.peek() == Some(b'}') {
            self.offset += 1;
            self.depth -= 1;
            return Ok(());
        }
        loop {
            self.skip_whitespace();
            let key = self.parse_string()?;
            if !keys.insert(key) {
                return Err(PortableJournalError::DuplicateJsonKey);
            }
            self.skip_whitespace();
            self.expect(b':')?;
            self.parse_value()?;
            self.skip_whitespace();
            match self.peek() {
                Some(b',') => self.offset += 1,
                Some(b'}') => {
                    self.offset += 1;
                    self.depth -= 1;
                    return Ok(());
                }
                _ => return Err(PortableJournalError::InvalidLine),
            }
        }
    }

    fn parse_array(&mut self) -> Result<(), PortableJournalError> {
        self.enter()?;
        self.expect(b'[')?;
        self.skip_whitespace();
        if self.peek() == Some(b']') {
            self.offset += 1;
            self.depth -= 1;
            return Ok(());
        }
        loop {
            self.parse_value()?;
            self.skip_whitespace();
            match self.peek() {
                Some(b',') => self.offset += 1,
                Some(b']') => {
                    self.offset += 1;
                    self.depth -= 1;
                    return Ok(());
                }
                _ => return Err(PortableJournalError::InvalidLine),
            }
        }
    }

    fn parse_string(&mut self) -> Result<String, PortableJournalError> {
        self.expect(b'"')?;
        let mut output = Vec::new();
        loop {
            let byte = self.peek().ok_or(PortableJournalError::InvalidLine)?;
            self.offset += 1;
            match byte {
                b'"' => {
                    return String::from_utf8(output)
                        .map_err(|_| PortableJournalError::InvalidLine);
                }
                b'\\' => self.parse_escape(&mut output)?,
                0..=0x1f => return Err(PortableJournalError::InvalidLine),
                _ => output.push(byte),
            }
        }
    }

    fn parse_escape(&mut self, output: &mut Vec<u8>) -> Result<(), PortableJournalError> {
        let byte = self.peek().ok_or(PortableJournalError::InvalidLine)?;
        self.offset += 1;
        match byte {
            b'"' | b'\\' | b'/' => output.push(byte),
            b'b' => output.push(0x08),
            b'f' => output.push(0x0c),
            b'n' => output.push(b'\n'),
            b'r' => output.push(b'\r'),
            b't' => output.push(b'\t'),
            b'u' => {
                let value = self.parse_hex4()?;
                let character = if (0xd800..=0xdbff).contains(&value) {
                    if self.peek() != Some(b'\\') {
                        return Err(PortableJournalError::InvalidLine);
                    }
                    self.offset += 1;
                    if self.peek() != Some(b'u') {
                        return Err(PortableJournalError::InvalidLine);
                    }
                    self.offset += 1;
                    let low = self.parse_hex4()?;
                    let combined =
                        0x10000 + ((u32::from(value) - 0xd800) << 10) + (u32::from(low) - 0xdc00);
                    char::from_u32(combined).ok_or(PortableJournalError::InvalidLine)?
                } else {
                    char::from_u32(u32::from(value)).ok_or(PortableJournalError::InvalidLine)?
                };
                let mut encoded = [0_u8; 4];
                output.extend_from_slice(character.encode_utf8(&mut encoded).as_bytes());
            }
            _ => return Err(PortableJournalError::InvalidLine),
        }
        Ok(())
    }

    fn parse_hex4(&mut self) -> Result<u16, PortableJournalError> {
        let mut value = 0_u16;
        for _ in 0..4 {
            let digit = self.peek().ok_or(PortableJournalError::InvalidLine)?;
            self.offset += 1;
            let digit = char::from(digit)
                .to_digit(16)
                .ok_or(PortableJournalError::InvalidLine)? as u16;
            value = value
                .checked_mul(16)
                .and_then(|value| value.checked_add(digit))
                .ok_or(PortableJournalError::InvalidLine)?;
        }
        Ok(value)
    }

    fn parse_scalar(&mut self) -> Result<(), PortableJournalError> {
        let start = self.offset;
        while matches!(
            self.peek(),
            Some(byte) if !matches!(byte, b' ' | b'\t' | b'\r' | b'\n' | b',' | b'}' | b']')
        ) {
            self.offset += 1;
        }
        if self.offset == start {
            return Err(PortableJournalError::InvalidLine);
        }
        serde_json::from_str::<Value>(
            std::str::from_utf8(&self.input[start..self.offset])
                .map_err(|_| PortableJournalError::InvalidLine)?,
        )
        .map(|_| ())
        .map_err(|_| PortableJournalError::InvalidLine)
    }
}

fn detect_duplicate_json_keys(text: &str) -> Result<(), PortableJournalError> {
    JsonDuplicateKeyScanner::new(text).validate()
}

/// Convenience constructor used by capture paths after hashing exact bytes.
pub fn artifact_ref_for(raw: &[u8]) -> PortableArtifactRef {
    PortableArtifactRef {
        sha256: artifact_hash(raw),
        size_bytes: raw.len() as u64,
    }
}

#[cfg(test)]
#[path = "portable_journal_tests.rs"]
mod tests;
