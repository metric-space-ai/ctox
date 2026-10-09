// Origin: CTOX
// License: AGPL-3.0-only
//! Jour fixe presentations: one learnordie `SlideDocument` deck per meeting.
//! Every accepted change is a new immutable revision. The document of each
//! revision is stored natively and published as its own `desktop_files` record
//! with demand chunks, which Workjet reads in bounded, hash-checked ranges.
//! Schema, edit and canvas semantics come from the vendored slide-engine
//! validator, so CTOX and Workjet accept exactly the same documents.
use super::super::{
    workjet_jour_fixe_contract as meeting_wire, workjet_presentation_contract as wire,
};
use super::*;
use crate::business_os::store;
use base64::Engine;
use rusqlite::{params, OptionalExtension};
use wire::WireValidate;

pub(in crate::business_os) const READ: &str = "ctox.workjet.presentation.read";
pub(in crate::business_os) const CANVAS_SAVE: &str = "ctox.workjet.presentation.canvas.save";
pub(in crate::business_os) const EDITS_APPLY: &str = "ctox.workjet.presentation.edits.apply";
pub(in crate::business_os) const DOCUMENT_SCHEMA: &str = "learnordie.slide.v1";
pub(in crate::business_os) const MAX_DOCUMENT_BYTES: usize = 8 * 1024 * 1024;
const FILE_SOURCE: &str = "ctox-workjet-presentation";
const LINKED_COLLECTION: &str = "workjet_presentations";
const CHUNK_CHARS: usize = 16 * 1024;
const MAX_ISSUES_IN_ERROR: usize = 6;

pub(in crate::business_os) const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS workjet_presentations (
 presentation_id TEXT PRIMARY KEY, project_id TEXT NOT NULL, meeting_id TEXT NOT NULL UNIQUE,
 owner_user_id TEXT NOT NULL, manifest_json TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS workjet_presentation_revisions (
 presentation_id TEXT NOT NULL, revision INTEGER NOT NULL, document_sha256 TEXT NOT NULL,
 document_json TEXT NOT NULL, source TEXT NOT NULL, actor TEXT NOT NULL, operation_id TEXT NOT NULL,
 created_at_ms INTEGER NOT NULL, PRIMARY KEY(presentation_id, revision));
CREATE TABLE IF NOT EXISTS workjet_presentation_operations (
 presentation_id TEXT NOT NULL, operation_id TEXT NOT NULL, owner_user_id TEXT NOT NULL,
 intent_hash TEXT NOT NULL, receipt_json TEXT NOT NULL, projections_json TEXT NOT NULL,
 PRIMARY KEY(presentation_id, operation_id));";

pub(in crate::business_os) fn is_command(kind: &str) -> bool {
    matches!(kind, READ | CANVAS_SAVE | EDITS_APPLY)
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn canonical(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let mut sorted = serde_json::Map::new();
            for key in keys {
                sorted.insert(key.clone(), canonical(&map[key]));
            }
            Value::Object(sorted)
        }
        Value::Array(items) => Value::Array(items.iter().map(canonical).collect()),
        other => other.clone(),
    }
}

/// Intent identity of a request, independent of JSON key order.
pub(in crate::business_os) fn intent_hash(value: &Value) -> anyhow::Result<String> {
    Ok(sha256_hex(&serde_json::to_vec(&canonical(value))?))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::business_os) enum Writer {
    Owner,
    Supervisor,
}

/// A presentation is written only for the project's latest meeting, and only
/// while that meeting is being prepared, held or (Owner) reviewed. Earlier
/// meetings keep what they showed; later decks compare against it.
pub(in crate::business_os) fn ensure_writable(
    conn: &Connection,
    meeting: &meeting_wire::Meeting,
    writer: Writer,
) -> anyhow::Result<()> {
    use meeting_wire::MeetingState as State;
    let open = match writer {
        Writer::Supervisor => matches!(
            meeting.state,
            State::Planned | State::Preparing | State::Ready | State::Live
        ),
        Writer::Owner => matches!(
            meeting.state,
            State::Preparing | State::Ready | State::Live | State::Review
        ),
    };
    ensure!(
        open,
        "the presentation of a {:?} meeting cannot be changed",
        meeting.state
    );
    let newer: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM workjet_jour_fixe_meetings WHERE project_id=?1 AND owner_user_id=?2 AND meeting_id<>?3 AND scheduled_at_ms>?4)",
        params![meeting.project_id, meeting.owner_user_id, meeting.id, meeting.scheduled_at_ms],
        |row| row.get(0),
    )?;
    ensure!(
        !newer,
        "a later meeting exists; earlier presentations stay as they were shown"
    );
    Ok(())
}

fn table_exists(conn: &Connection) -> anyhow::Result<bool> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='workjet_presentations')",
        [],
        |row| row.get(0),
    )?)
}

fn decode_manifest(raw: &str) -> anyhow::Result<wire::PresentationManifest> {
    let manifest: wire::PresentationManifest = serde_json::from_str(raw)?;
    manifest.validate().map_err(anyhow::Error::msg)?;
    Ok(manifest)
}

pub(in crate::business_os) fn load_by_meeting(
    conn: &Connection,
    meeting_id: &str,
) -> anyhow::Result<Option<wire::PresentationManifest>> {
    if !table_exists(conn)? {
        return Ok(None);
    }
    conn.query_row(
        "SELECT manifest_json FROM workjet_presentations WHERE meeting_id=?1",
        [meeting_id],
        |row| row.get::<_, String>(0),
    )
    .optional()?
    .map(|raw| decode_manifest(&raw))
    .transpose()
}

pub(in crate::business_os) fn load_by_id(
    conn: &Connection,
    presentation_id: &str,
) -> anyhow::Result<Option<wire::PresentationManifest>> {
    if !table_exists(conn)? {
        return Ok(None);
    }
    conn.query_row(
        "SELECT manifest_json FROM workjet_presentations WHERE presentation_id=?1",
        [presentation_id],
        |row| row.get::<_, String>(0),
    )
    .optional()?
    .map(|raw| decode_manifest(&raw))
    .transpose()
}

/// The stored document of the manifest's current revision, hash-checked.
pub(in crate::business_os) fn current_document(
    conn: &Connection,
    manifest: &wire::PresentationManifest,
) -> anyhow::Result<String> {
    let raw: String = conn
        .query_row(
            "SELECT document_json FROM workjet_presentation_revisions WHERE presentation_id=?1 AND revision=?2",
            params![manifest.presentation_id, manifest.revision as i64],
            |row| row.get(0),
        )
        .optional()?
        .context("presentation revision is unavailable")?;
    ensure!(
        sha256_hex(raw.as_bytes()) == manifest.document_sha256,
        "presentation revision does not match its manifest hash"
    );
    Ok(raw)
}

/// Turns a validator `ok:false` answer into one readable error with repair hints.
pub(in crate::business_os) fn rejection(answer: &Value) -> anyhow::Error {
    let issues = answer["issues"].as_array().cloned().unwrap_or_default();
    let mut lines = Vec::new();
    for issue in issues.iter().take(MAX_ISSUES_IN_ERROR) {
        lines.push(format!(
            "{} at {}: {}{}",
            issue["code"].as_str().unwrap_or("issue"),
            issue["path"].as_str().unwrap_or("$"),
            issue["message"].as_str().unwrap_or(""),
            issue["repairHint"]
                .as_str()
                .map(|hint| format!(" Repair: {hint}"))
                .unwrap_or_default()
        ));
    }
    if issues.len() > MAX_ISSUES_IN_ERROR {
        lines.push(format!(
            "… {} more issues",
            issues.len() - MAX_ISSUES_IN_ERROR
        ));
    }
    if lines.is_empty() {
        lines.push("the slide engine rejected the presentation".into());
    }
    anyhow::anyhow!("presentation rejected: {}", lines.join(" | "))
}

/// Runs one document-producing engine operation (applyEdits, updateCanvas) and
/// returns the resulting document, or the engine's issues as one error.
pub(in crate::business_os) fn engine_document(
    root: &Path,
    request: Value,
) -> anyhow::Result<Value> {
    let answer = super::presentation_validator::run(root, &request)?;
    if answer["ok"] != true {
        return Err(rejection(&answer));
    }
    answer
        .get("document")
        .cloned()
        .context("slide engine returned no document")
}

/// Validates a whole document with the shared engine and returns it as stored.
pub(in crate::business_os) fn validated_document(
    root: &Path,
    document: Value,
) -> anyhow::Result<Value> {
    let answer = validate_report(root, document.clone())?;
    if answer["ok"] != true {
        return Err(rejection(&answer));
    }
    // A no-op edit batch re-validates and returns the engine's own copy.
    engine_document(
        root,
        json!({"op":"applyEdits","document":document,"operations":[]}),
    )
}

/// Dry-run validation report (issues and warnings with repair hints).
pub(in crate::business_os) fn validate_report(
    root: &Path,
    document: Value,
) -> anyhow::Result<Value> {
    super::presentation_validator::run(root, &json!({"op":"validate","document":document}))
}

/// Compact outline of a stored document for agents.
pub(in crate::business_os) fn outline(root: &Path, document: &Value) -> anyhow::Result<Value> {
    let mut answer =
        super::presentation_validator::run(root, &json!({"op":"outline","document":document}))?;
    ensure!(
        answer["ok"] == true,
        "slide engine could not outline the presentation"
    );
    if let Some(object) = answer.as_object_mut() {
        object.remove("ok");
    }
    Ok(answer)
}

/// The meeting deck derived from a presentation: one markdown slide per
/// presentation slide, same ids and order. The narration text (speaker notes,
/// or the slide's text without markdown) becomes `body_markdown`, because the
/// meeting deck is what the native narration reads aloud; the visual slide
/// itself is rendered from the presentation.
pub(in crate::business_os) fn meeting_slides(
    root: &Path,
    document: &Value,
    meeting_id: &str,
) -> anyhow::Result<Vec<Value>> {
    let answer = super::presentation_validator::run(
        root,
        &json!({"op":"meetingSlides","document":document}),
    )?;
    if answer["ok"] != true {
        return Err(rejection(&answer));
    }
    let slides = answer["slides"]
        .as_array()
        .context("slide engine returned no meeting slides")?;
    ensure!(
        !slides.is_empty() && slides.len() <= 100,
        "a meeting deck has between 1 and 100 slides"
    );
    slides
        .iter()
        .enumerate()
        .map(|(position, slide)| {
            let id = slide["id"].as_str().context("meeting slide id missing")?;
            let title = slide["title"].as_str().context("meeting slide title missing")?;
            let narration = slide["narration"].as_str().unwrap_or("").trim();
            let body = if narration.is_empty() {
                slide["body_markdown"].as_str().unwrap_or(title)
            } else {
                narration
            };
            ensure!(
                body.len() <= 4096 && !body.trim().is_empty(),
                "meeting slide text must be 1 to 4096 UTF-8 bytes"
            );
            Ok(json!({"id":id,"position":position,"title":title.chars().take(256).collect::<String>(),
                "body_markdown":body,"meeting_id":meeting_id}))
        })
        .collect()
}

/// Every business scene with data in a stored document: scene3d blocks and,
/// for slides the Owner drew on, scene3d embeds in the canvas. This is what a
/// meeting actually showed; later decks compare against it.
pub(in crate::business_os) fn presented_scene_data(document: &Value) -> Vec<Value> {
    let mut scenes = Vec::new();
    for slide in document["slides"].as_array().into_iter().flatten() {
        let slide_id = slide["id"].clone();
        let title = slide["title"].clone();
        let canvas = slide["canvas"]["elements"].as_array();
        if let Some(elements) = canvas {
            for element in elements {
                let embed = &element["customData"]["learnordie"];
                if embed["type"] == "scene3d" && embed["data"].is_object() {
                    scenes.push(json!({"slide_id":slide_id,"slide_title":title,
                        "scene_id":embed["sceneId"],"caption":embed["caption"],"data":embed["data"]}));
                }
            }
        } else {
            for block in slide["blocks"].as_array().into_iter().flatten() {
                if block["type"] == "scene3d" && block["data"].is_object() {
                    scenes.push(json!({"slide_id":slide_id,"slide_title":title,
                        "scene_id":block["sceneId"],"caption":block["caption"],"data":block["data"]}));
                }
            }
        }
    }
    scenes
}

pub(in crate::business_os) struct NewRevision<'a> {
    pub(in crate::business_os) meeting: &'a meeting_wire::Meeting,
    pub(in crate::business_os) prior: Option<&'a wire::PresentationManifest>,
    pub(in crate::business_os) document: &'a Value,
    pub(in crate::business_os) source: wire::PresentationSource,
    pub(in crate::business_os) actor: &'a str,
    pub(in crate::business_os) operation_id: &'a str,
}

pub(in crate::business_os) fn presentation_id_for(meeting_id: &str) -> String {
    let id = stable_id("workjet_presentation", &[meeting_id]);
    id[..id.len().min(85)].to_owned()
}

/// Persists one revision and its file custody inside the caller's transaction.
/// Returns the new manifest and the business records to project into RxDB.
pub(in crate::business_os) fn commit_revision(
    root: &Path,
    tx: &Connection,
    new: NewRevision<'_>,
) -> anyhow::Result<(wire::PresentationManifest, Vec<DomainRecordRef>)> {
    tx.execute_batch(SCHEMA)?;
    ensure!(
        new.document["schemaVersion"] == DOCUMENT_SCHEMA,
        "presentation must use {DOCUMENT_SCHEMA}"
    );
    let raw = serde_json::to_string(new.document)?;
    ensure!(
        raw.len() <= MAX_DOCUMENT_BYTES,
        "presentation exceeds {MAX_DOCUMENT_BYTES} bytes"
    );
    let sha = sha256_hex(raw.as_bytes());
    let title: String = new.document["title"]
        .as_str()
        .context("presentation title missing")?
        .chars()
        .take(180)
        .collect();
    let slide_ids = new.document["slides"]
        .as_array()
        .context("presentation slides missing")?
        .iter()
        .map(|slide| slide["id"].as_str().map(str::to_owned))
        .collect::<Option<Vec<_>>>()
        .context("presentation slide without id")?;
    let presentation_id = new
        .prior
        .map(|prior| prior.presentation_id.clone())
        .unwrap_or_else(|| presentation_id_for(&new.meeting.id));
    let revision = new
        .prior
        .map(|prior| prior.revision.checked_add(1))
        .unwrap_or(Some(1))
        .context("presentation revision overflow")?;
    let instance = store::existing_instance_id(root)?;
    let file_id = format!(
        "workjet_presentation_{:.32}",
        sha256_hex(format!("{instance}:{presentation_id}:{revision}").as_bytes())
    );
    let generation = format!(
        "presentation_generation_{:.32}",
        sha256_hex(format!("{file_id}:{sha}").as_bytes())
    );
    let now = store::now_ms() as i64;
    let file_name: String = format!("{title} r{revision}.slides.json");
    let mut refs = Vec::new();
    refs.push(super::jour_fixe_local_narration::persist(
        tx,
        "desktop_files",
        &file_id,
        now,
        json!({"id":file_id,"name":file_name,"kind":"file","mime_type":"application/json",
            "extension":"json","size_bytes":raw.len(),"owner_id":new.meeting.owner_user_id,
            "source":FILE_SOURCE,"linked_collection":LINKED_COLLECTION,
            "linked_record_id":presentation_id,"content_state":"available","content_hash":sha,
            "content_hash_scheme":"sha256-bytes-v1","content_generation_id":generation,
            "is_deleted":false,"created_at_ms":now,"updated_at_ms":now}),
    )?);
    let encoded = base64::engine::general_purpose::STANDARD.encode(raw.as_bytes());
    let total = encoded.len().div_ceil(CHUNK_CHARS);
    for (idx, chunk) in encoded.as_bytes().chunks(CHUNK_CHARS).enumerate() {
        let data = std::str::from_utf8(chunk)?;
        let id = format!("{file_id}_{generation}_{idx:06}");
        refs.push(super::jour_fixe_local_narration::persist(
            tx,
            "desktop_file_chunks",
            &id,
            now,
            json!({"id":id,"file_id":file_id,"generation_id":generation,"content_hash":sha,
                "content_hash_scheme":"sha256-bytes-v1","idx":idx,"total":total,
                "encoding":"base64","data":data,"chunk_hash":sha256_hex(data.as_bytes()),
                "chunk_hash_scheme":"sha256-base64-chunk-v1","size_bytes":data.len(),
                "created_at_ms":now}),
        )?);
    }
    let manifest = wire::PresentationManifest {
        presentation_id: presentation_id.clone(),
        project_id: new.meeting.project_id.clone(),
        meeting_id: new.meeting.id.clone(),
        owner_user_id: new.meeting.owner_user_id.clone(),
        title,
        revision,
        document_schema: DOCUMENT_SCHEMA.into(),
        document_file_id: file_id,
        document_generation_id: generation,
        document_sha256: sha.clone(),
        document_bytes: raw.len() as u64,
        slide_ids,
        source: new.source,
        updated_by: new.actor.chars().take(256).collect(),
        updated_at_ms: now,
    };
    manifest.validate().map_err(anyhow::Error::msg)?;
    let manifest_json = serde_json::to_string(&manifest)?;
    match new.prior {
        Some(prior) => ensure!(
            tx.execute(
                "UPDATE workjet_presentations SET manifest_json=?2 WHERE presentation_id=?1 AND meeting_id=?3 AND json_extract(manifest_json,'$.revision')=?4",
                params![presentation_id, manifest_json, new.meeting.id, prior.revision as i64],
            )? == 1,
            "presentation revision changed; read the current presentation"
        ),
        None => {
            tx.execute(
                "INSERT INTO workjet_presentations(presentation_id,project_id,meeting_id,owner_user_id,manifest_json) VALUES(?1,?2,?3,?4,?5)",
                params![presentation_id, new.meeting.project_id, new.meeting.id, new.meeting.owner_user_id, manifest_json],
            )
            .context("this meeting already has a presentation")?;
        }
    }
    let source = match new.source {
        wire::PresentationSource::Agent => "agent",
        wire::PresentationSource::Owner => "owner",
    };
    tx.execute(
        "INSERT INTO workjet_presentation_revisions(presentation_id,revision,document_sha256,document_json,source,actor,operation_id,created_at_ms) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
        params![presentation_id, revision as i64, sha, raw, source, new.actor, new.operation_id, now],
    )?;
    Ok((manifest, refs))
}

pub(in crate::business_os) fn receipt(
    operation_id: &str,
    manifest: &wire::PresentationManifest,
) -> anyhow::Result<Value> {
    let receipt = wire::PresentationMutationReceipt {
        operation_id: operation_id.to_owned(),
        presentation_id: manifest.presentation_id.clone(),
        project_id: manifest.project_id.clone(),
        meeting_id: manifest.meeting_id.clone(),
        revision: manifest.revision,
        document_sha256: manifest.document_sha256.clone(),
        document_bytes: manifest.document_bytes,
        slide_ids: manifest.slide_ids.clone(),
    };
    receipt.validate().map_err(anyhow::Error::msg)?;
    Ok(
        json!({"ok":true,"contract":wire::CONTRACT_SCHEMA,"mutation":receipt,"presentation":manifest}),
    )
}

/// A stored operation with the same intent is replayed; a different intent conflicts.
pub(in crate::business_os) fn replay(
    conn: &Connection,
    presentation_id: &str,
    operation_id: &str,
    owner: &str,
    intent: &str,
) -> anyhow::Result<Option<(Value, Vec<DomainRecordRef>)>> {
    if !table_exists(conn)? {
        return Ok(None);
    }
    let prior: Option<(String, String, String, String)> = conn
        .query_row(
            "SELECT owner_user_id,intent_hash,receipt_json,projections_json FROM workjet_presentation_operations WHERE presentation_id=?1 AND operation_id=?2",
            params![presentation_id, operation_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()?;
    let Some((stored_owner, stored_intent, receipt, projections)) = prior else {
        return Ok(None);
    };
    ensure!(
        stored_owner == owner && stored_intent == intent,
        "this operation_id was already used for a different request on this presentation; use a new operation_id"
    );
    Ok(Some((
        serde_json::from_str(&receipt)?,
        serde_json::from_str(&projections)?,
    )))
}

pub(in crate::business_os) fn record_operation(
    tx: &Connection,
    operation_id: &str,
    owner: &str,
    presentation_id: &str,
    intent: &str,
    receipt: &Value,
    projections: &[DomainRecordRef],
) -> anyhow::Result<()> {
    tx.execute(
        "INSERT INTO workjet_presentation_operations(presentation_id,operation_id,owner_user_id,intent_hash,receipt_json,projections_json) VALUES(?1,?2,?3,?4,?5,?6)",
        params![presentation_id, operation_id, owner, intent, serde_json::to_string(receipt)?, serde_json::to_string(projections)?],
    )?;
    Ok(())
}

fn strip_inbound_channel(command: &BusinessCommand) -> anyhow::Result<Value> {
    let mut payload = command.payload.clone();
    let object = payload
        .as_object_mut()
        .context("presentation request must be an object")?;
    if let Some(channel) = object.remove("inbound_channel") {
        let value = channel.as_str().context("inbound_channel must be text")?;
        ensure!(
            !value.trim().is_empty() && value.chars().count() <= 256,
            "invalid inbound_channel"
        );
    }
    Ok(payload)
}

/// Owner read: the manifest of the meeting's presentation, or null.
pub(in crate::business_os) fn read(
    root: &Path,
    command: &BusinessCommand,
    actor: &str,
) -> anyhow::Result<Value> {
    let query: wire::ReadPresentationRequest =
        serde_json::from_value(strip_inbound_channel(command)?)?;
    query.validate().map_err(anyhow::Error::msg)?;
    ensure!(
        command
            .record_id
            .as_deref()
            .is_none_or(|id| id == query.project_id),
        "presentation read routing conflicts with project"
    );
    let mut reader = Connection::open_with_flags(
        store::business_os_store_path(root),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    reader.busy_timeout(crate::persistence::sqlite_busy_timeout_duration())?;
    let conn = reader.transaction()?;
    let meeting =
        super::jour_fixe_owner::owned(&conn, actor, Some(&query.project_id), &query.meeting_id)?;
    let manifest = load_by_meeting(&conn, &meeting.id)?;
    if let Some(manifest) = &manifest {
        ensure!(
            manifest.project_id == meeting.project_id
                && manifest.owner_user_id == meeting.owner_user_id,
            "presentation ownership binding conflicts"
        );
    }
    Ok(json!({"ok":true,"contract":wire::CONTRACT_SCHEMA,"presentation":manifest}))
}

enum OwnerEdit {
    Canvas(wire::SavePresentationCanvasRequest),
    Edits(wire::ApplyPresentationEditsRequest),
}

impl OwnerEdit {
    fn identity(&self) -> (&str, &str, u64) {
        match self {
            Self::Canvas(v) => (&v.operation_id, &v.presentation_id, v.expected_revision),
            Self::Edits(v) => (&v.operation_id, &v.presentation_id, v.expected_revision),
        }
    }
    fn validator_request(&self, document: Value) -> anyhow::Result<Value> {
        Ok(match self {
            Self::Canvas(v) => {
                let scene: Value =
                    serde_json::from_str(&v.scene_json).context("scene_json is not JSON")?;
                json!({"op":"updateCanvas","document":document,"slideId":v.slide_id,"scene":scene})
            }
            Self::Edits(v) => {
                let operations: Value = serde_json::from_str(&v.operations_json)
                    .context("operations_json is not JSON")?;
                ensure!(
                    operations.is_array(),
                    "operations_json must be a JSON array"
                );
                json!({"op":"applyEdits","document":document,"operations":operations})
            }
        })
    }
}

fn parse_owner_edit(command: &BusinessCommand) -> anyhow::Result<(OwnerEdit, Value)> {
    let payload = strip_inbound_channel(command)?;
    let edit = match command.command_type.as_str() {
        CANVAS_SAVE => {
            let v: wire::SavePresentationCanvasRequest = serde_json::from_value(payload.clone())?;
            v.validate().map_err(anyhow::Error::msg)?;
            OwnerEdit::Canvas(v)
        }
        EDITS_APPLY => {
            let v: wire::ApplyPresentationEditsRequest = serde_json::from_value(payload.clone())?;
            v.validate().map_err(anyhow::Error::msg)?;
            OwnerEdit::Edits(v)
        }
        _ => anyhow::bail!("unsupported presentation edit"),
    };
    let (operation, presentation, _) = edit.identity();
    ensure!(
        operation.trim() == operation && presentation.trim() == presentation,
        "presentation operation identity must be canonical"
    );
    Ok((edit, payload))
}

/// Owner canvas saves and edit batches. The engine runs before the writer lock;
/// the expected revision is re-checked inside the domain transaction.
pub(in crate::business_os) fn handle(
    root: &Path,
    command: &BusinessCommand,
    actor: &str,
    admission: &DomainEffectAdmission,
) -> anyhow::Result<Value> {
    let (edit, payload) = parse_owner_edit(command)?;
    let (operation, presentation_id, expected) = edit.identity();
    let intent = intent_hash(&json!({"kind":command.command_type,"payload":payload}))?;
    let (meeting, manifest, document) = {
        let conn = open_store(root)?;
        let manifest = load_by_id(&conn, presentation_id)?
            .context("presentation unavailable to this owner")?;
        let meeting = super::jour_fixe_owner::owned(
            &conn,
            actor,
            command.record_id.as_deref(),
            &manifest.meeting_id,
        )?;
        ensure!(
            manifest.project_id == meeting.project_id
                && manifest.owner_user_id == meeting.owner_user_id,
            "presentation ownership binding conflicts"
        );
        if let Some((result, _)) = replay(
            &conn,
            presentation_id,
            operation,
            &meeting.owner_user_id,
            &intent,
        )? {
            return Ok(result);
        }
        ensure!(
            manifest.revision == expected,
            "presentation revision changed; read the current presentation"
        );
        let document = current_document(&conn, &manifest)?;
        (meeting, manifest, document)
    };
    {
        let conn = open_store(root)?;
        ensure_writable(&conn, &meeting, Writer::Owner)?;
    }
    let answer = super::presentation_validator::run(
        root,
        &edit.validator_request(serde_json::from_str(&document)?)?,
    )?;
    if answer["ok"] != true {
        return Err(rejection(&answer));
    }
    let next = answer
        .get("document")
        .cloned()
        .context("slide engine returned no document")?;
    let mut conn = open_store(root)?;
    let applied = admission.apply(&mut conn, |tx| {
        let current = load_by_id(tx, presentation_id)?.context("presentation disappeared")?;
        let meeting = super::jour_fixe_owner::owned(
            tx,
            actor,
            command.record_id.as_deref(),
            &current.meeting_id,
        )?;
        if let Some((result, projections)) = replay(
            tx,
            presentation_id,
            operation,
            &meeting.owner_user_id,
            &intent,
        )? {
            return Ok(AppliedDomainEffect {
                result,
                projections,
            });
        }
        ensure!(
            current.revision == expected && current.document_sha256 == manifest.document_sha256,
            "presentation revision changed; read the current presentation"
        );
        ensure_writable(tx, &meeting, Writer::Owner)?;
        let (next_manifest, projections) = commit_revision(
            root,
            tx,
            NewRevision {
                meeting: &meeting,
                prior: Some(&current),
                document: &next,
                source: wire::PresentationSource::Owner,
                actor: &meeting.owner_user_id,
                operation_id: operation,
            },
        )?;
        let result = receipt(operation, &next_manifest)?;
        record_operation(
            tx,
            operation,
            &meeting.owner_user_id,
            presentation_id,
            &intent,
            &result,
            &projections,
        )?;
        Ok(AppliedDomainEffect {
            result,
            projections,
        })
    })?;
    Ok(applied.result)
}
