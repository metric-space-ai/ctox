// Origin: CTOX
// License: AGPL-3.0-only
//! Presentation tools for the registered Supervisor of a Jour fixe meeting.
//! Decks are learnordie `SlideDocument`s validated and edited by the shared
//! slide engine; every write is a new revision bound to the current meeting.
use super::super::project_chats::presentation as store_presentation;
use super::super::workjet_presentation_contract as wire;
use super::*;
use rusqlite::{params, Connection, OpenFlags, TransactionBehavior};
use serde_json::json;
use sha2::{Digest, Sha256};

pub(super) const READ_TOOL: &str = "business_os.presentation_read";
pub(super) const WRITE_TOOL: &str = "business_os.presentation_update";
const MAX_REQUEST_BYTES: usize = 4 * 1024 * 1024;
const MAX_INLINE_BYTES: usize = 192 * 1024;

const GUIDE: &str =
    include_str!("../../skills/system/mission_orchestration/jour-fix-presentation/SKILL.md");
const READ_ACTIONS: [&str; 6] = [
    "read_guide",
    "read_history",
    "read_presentation",
    "read_slide",
    "read_document",
    "validate_document",
];
const WRITE_ACTIONS: [&str; 5] = [
    "create_presentation",
    "replace_document",
    "apply_edits",
    "save_canvas",
    "publish_deck",
];

pub(super) fn allows(tool: &str, args: &Value) -> bool {
    let action = args["action"].as_str().unwrap_or("");
    (tool == READ_TOOL && READ_ACTIONS.contains(&action))
        || (tool == WRITE_TOOL && WRITE_ACTIONS.contains(&action))
}

fn id_schema() -> Value {
    json!({"type":"string","minLength":1,"maxLength":128})
}
fn revision_schema() -> Value {
    json!({"type":"integer","minimum":1,"maximum":9_007_199_254_740_991_u64})
}
fn action_schema(action: &str, properties: Value, required: &[&str]) -> Value {
    json!({"type":"object","additionalProperties":false,"required":["action","request"],
        "properties":{"action":{"type":"string","const":action},
        "request":{"type":"object","additionalProperties":false,"properties":properties,"required":required}}})
}
fn scope() -> serde_json::Map<String, Value> {
    let mut map = serde_json::Map::new();
    map.insert("project_id".into(), id_schema());
    map.insert("meeting_id".into(), id_schema());
    map
}
fn with(mut base: serde_json::Map<String, Value>, extra: Value) -> Value {
    if let Value::Object(extra) = extra {
        base.extend(extra);
    }
    Value::Object(base)
}

pub(super) fn read_descriptor() -> BusinessOsMcpToolDescriptor {
    let schema = json!({"type":"object","additionalProperties":false,"required":["action","request"],
    "properties":{"action":{"type":"string"},"request":{"type":"object"}},
    "oneOf":[
        action_schema("read_guide", json!({}), &[]),
        action_schema("read_presentation", Value::Object(scope()), &["project_id","meeting_id"]),
        action_schema("read_slide", with(scope(), json!({"slide_id":{"type":"string","minLength":1,"maxLength":120}})), &["project_id","meeting_id","slide_id"]),
        action_schema("read_document", Value::Object(scope()), &["project_id","meeting_id"]),
        action_schema("validate_document", json!({"document":{"type":"object"}}), &["document"]),
    ]});
    read_tool(READ_TOOL,
        "Call read_guide first: it returns the authoring guide for Jour fixe presentations. Read the presentation (learnordie SlideDocument, schema learnordie.slide.v1) of this registered Supervisor's current Jour fixe meeting: read_presentation returns the manifest and a compact outline, read_slide one full slide including its canvas, read_document the whole document when it is small. read_history lists this project's earlier meetings, newest first (index 1 = last, 2 = penultimate), with the scene data their presentations actually showed; it is the only source for comparisons with earlier meetings. validate_document checks a draft without storing it and returns issues with repair hints.",
        schema)
}

pub(super) fn write_descriptor() -> BusinessOsMcpToolDescriptor {
    let schema = json!({"type":"object","additionalProperties":false,"required":["action","request"],
    "properties":{"action":{"type":"string"},"request":{"type":"object"}},
    "oneOf":[
        action_schema("create_presentation", with(scope(), json!({"operation_id":id_schema(),"document":{"type":"object"}})), &["operation_id","project_id","meeting_id","document"]),
        action_schema("replace_document", with(scope(), json!({"operation_id":id_schema(),"expected_revision":revision_schema(),"document":{"type":"object"}})), &["operation_id","project_id","meeting_id","expected_revision","document"]),
        action_schema("apply_edits", with(scope(), json!({"operation_id":id_schema(),"expected_revision":revision_schema(),"operations":{"type":"array","maxItems":200,"items":{"type":"object"}}})), &["operation_id","project_id","meeting_id","expected_revision","operations"]),
        action_schema("save_canvas", with(scope(), json!({"operation_id":id_schema(),"expected_revision":revision_schema(),"slide_id":{"type":"string","minLength":1,"maxLength":120},"scene":{"type":"object"}})), &["operation_id","project_id","meeting_id","expected_revision","slide_id","scene"]),
        action_schema("publish_deck", with(scope(), json!({"operation_id":id_schema(),"presentation_revision":revision_schema(),"expected_meeting_revision":{"type":"integer","minimum":0,"maximum":9_007_199_254_740_991_u64},"deck_revision":revision_schema()})), &["operation_id","project_id","meeting_id","presentation_revision","expected_meeting_revision","deck_revision"]),
    ]});
    write_tool(WRITE_TOOL,
        "Store this Supervisor's Jour fixe presentation as a new revision. create_presentation stores the first document for the meeting; replace_document, apply_edits (learnordie edit operations) and save_canvas (one slide's learnordie.excalidraw.v1 scene) require the current expected_revision and a stable operation_id. Documents are validated by the slide engine; rejected drafts return issues with repair hints. publish_deck derives the meeting's narration deck (one markdown slide per presentation slide, same slide ids) from a stored revision, exactly like prepare_deck.",
        schema)
}

fn arg<'a>(arguments: &'a Value, field: &str) -> anyhow::Result<&'a str> {
    let value = arguments["request"][field]
        .as_str()
        .with_context(|| format!("request.{field} is required"))?;
    anyhow::ensure!(
        !value.is_empty() && value.trim() == value && value.chars().count() <= 128,
        "request.{field} must be canonical bounded text"
    );
    Ok(value)
}
fn number(arguments: &Value, field: &str) -> anyhow::Result<u64> {
    arguments["request"][field]
        .as_u64()
        .filter(|v| *v <= 9_007_199_254_740_991)
        .with_context(|| format!("request.{field} must be a safe nonnegative integer"))
}
fn bounded(value: Value, what: &str) -> anyhow::Result<Value> {
    anyhow::ensure!(
        serde_json::to_vec(&value)?.len() <= MAX_INLINE_BYTES,
        "{what} exceeds the tool response budget; use read_presentation for the outline and read_slide per slide"
    );
    Ok(value)
}

fn connections(root: &Path, writing: bool) -> anyhow::Result<(Connection, Connection)> {
    let flags = if writing {
        OpenFlags::SQLITE_OPEN_READ_WRITE
    } else {
        OpenFlags::SQLITE_OPEN_READ_ONLY
    };
    let core = Connection::open_with_flags(crate::paths::core_db(root), flags)?;
    core.busy_timeout(std::time::Duration::from_secs(5))?;
    let policy = Connection::open_with_flags(store::business_os_store_path(root), flags)?;
    policy.busy_timeout(std::time::Duration::from_secs(5))?;
    Ok((core, policy))
}

/// Meeting + current presentation, resolved through the Supervisor binding.
struct Scope {
    meeting: super::super::workjet_jour_fixe_contract::Meeting,
    presentation: Option<wire::PresentationManifest>,
    document: Option<String>,
}

fn resolve(
    core: &Connection,
    policy: &Connection,
    context: &McpChannelRequestContext,
    trusted: &Value,
    arguments: &Value,
    writing: bool,
) -> anyhow::Result<Scope> {
    let project = arg(arguments, "project_id")?;
    let meeting_id = arg(arguments, "meeting_id")?;
    let meeting =
        workjet_jour_fixe::current_meeting(core, policy, context, trusted, meeting_id, writing)?;
    anyhow::ensure!(
        meeting.project_id == project,
        "presentation project differs from the meeting"
    );
    let presentation = store_presentation::load_by_meeting(policy, &meeting.id)?;
    let document = presentation
        .as_ref()
        .map(|manifest| store_presentation::current_document(policy, manifest))
        .transpose()?;
    Ok(Scope {
        meeting,
        presentation,
        document,
    })
}

fn publish(
    root: &Path,
    refs: &[super::super::domain_effect::DomainRecordRef],
) -> anyhow::Result<()> {
    let conn = store::open_store(root)?;
    for record in refs {
        anyhow::ensure!(
            matches!(
                record.collection.as_str(),
                "desktop_files" | "desktop_file_chunks"
            ),
            "presentation projection scope differs"
        );
        let (raw, at): (String, i64) = conn.query_row(
            "SELECT payload_json,updated_at_ms FROM business_records WHERE collection=?1 AND record_id=?2 AND deleted=0",
            params![record.collection, record.id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        store::upsert_rxdb_collection_record(
            root,
            &record.collection,
            &record.id,
            at,
            serde_json::from_str(&raw)?,
        )?;
    }
    Ok(())
}

pub(super) fn execute(
    root: &Path,
    context: &McpChannelRequestContext,
    tool: &str,
    arguments: &Value,
    trusted: Option<&Value>,
) -> anyhow::Result<Value> {
    anyhow::ensure!(allows(tool, arguments), "unsupported presentation action");
    let trusted = trusted.context("native supervisor session unavailable")?;
    anyhow::ensure!(
        context.trusted_role_source.as_deref() == Some(MCP_INTERNAL_SESSION_AUTH_SOURCE)
            && trusted["workjet_supervisor_only"] == true,
        "presentation tool requires the restricted native supervisor session"
    );
    anyhow::ensure!(
        serde_json::to_vec(arguments)?.len() <= MAX_REQUEST_BYTES,
        "presentation request exceeds {MAX_REQUEST_BYTES} bytes"
    );
    let action = arguments["action"].as_str().unwrap_or_default();
    if tool == READ_TOOL {
        return read(root, context, trusted, action, arguments);
    }
    if action == "publish_deck" {
        return publish_deck(root, context, trusted, arguments);
    }
    write(root, context, trusted, action, arguments)
}

fn read(
    root: &Path,
    context: &McpChannelRequestContext,
    trusted: &Value,
    action: &str,
    arguments: &Value,
) -> anyhow::Result<Value> {
    if action == "read_guide" {
        return Ok(json!({"contract":wire::CONTRACT_SCHEMA,"guide":GUIDE}));
    }
    if action == "validate_document" {
        let document = arguments["request"]["document"].clone();
        anyhow::ensure!(document.is_object(), "request.document must be an object");
        // Dry run only; still bound to an existing supervisor session above.
        let answer = store_presentation::validate_report(root, document)?;
        return bounded(
            json!({"contract":wire::CONTRACT_SCHEMA,"validation":answer}),
            "validation report",
        );
    }
    let (mut core, mut policy) = connections(root, false)?;
    let core_tx = core.transaction_with_behavior(TransactionBehavior::Deferred)?;
    let policy_tx = policy.transaction_with_behavior(TransactionBehavior::Deferred)?;
    let scope = resolve(&core_tx, &policy_tx, context, trusted, arguments, false)?;
    if action == "read_history" {
        let limit = arguments["request"]["limit"]
            .as_u64()
            .unwrap_or(3)
            .clamp(1, 6);
        let history = history(&policy_tx, &scope.meeting, limit)?;
        return bounded(
            json!({"contract":wire::CONTRACT_SCHEMA,"meeting_id":scope.meeting.id,
                "occurrences":history}),
            "meeting history",
        );
    }
    drop(policy_tx);
    drop(core_tx);
    let Some(manifest) = scope.presentation else {
        return Ok(json!({"contract":wire::CONTRACT_SCHEMA,"presentation":null,
            "hint":"No presentation exists for this meeting yet; use business_os.presentation_update create_presentation."}));
    };
    let document: Value = serde_json::from_str(scope.document.as_deref().unwrap_or("null"))?;
    match action {
        "read_presentation" => {
            let outline = super::super::project_chats::presentation::outline(root, &document)?;
            bounded(
                json!({"contract":wire::CONTRACT_SCHEMA,"presentation":manifest,"outline":outline}),
                "presentation outline",
            )
        }
        "read_slide" => {
            let slide_id = arguments["request"]["slide_id"]
                .as_str()
                .context("request.slide_id is required")?;
            let slide = document["slides"]
                .as_array()
                .and_then(|slides| slides.iter().find(|slide| slide["id"] == slide_id))
                .cloned()
                .context("slide is not part of this presentation")?;
            bounded(
                json!({"contract":wire::CONTRACT_SCHEMA,"presentation_id":manifest.presentation_id,
                    "revision":manifest.revision,"slide":slide}),
                "slide",
            )
        }
        _ => bounded(
            json!({"contract":wire::CONTRACT_SCHEMA,"presentation":manifest,"document":document}),
            "document",
        ),
    }
}

/// Earlier meetings of the same project and owner, newest first, with the
/// scene data their stored presentations showed. Missing presentations stay
/// missing: there is no interpolation or estimate.
fn history(
    policy: &Connection,
    meeting: &super::super::workjet_jour_fixe_contract::Meeting,
    limit: u64,
) -> anyhow::Result<Vec<Value>> {
    let mut statement = policy.prepare(
        "SELECT meeting_id,scheduled_at_ms,json_extract(metadata_json,'$.state') FROM workjet_jour_fixe_meetings
         WHERE project_id=?1 AND owner_user_id=?2 AND meeting_id<>?3 AND scheduled_at_ms<?4
         ORDER BY scheduled_at_ms DESC LIMIT ?5",
    )?;
    let rows = statement
        .query_map(
            params![
                meeting.project_id,
                meeting.owner_user_id,
                meeting.id,
                meeting.scheduled_at_ms,
                limit as i64
            ],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            },
        )?
        .collect::<Result<Vec<_>, _>>()?;
    let mut occurrences = Vec::new();
    for (index, (meeting_id, scheduled_at_ms, state)) in rows.into_iter().enumerate() {
        let manifest = store_presentation::load_by_meeting(policy, &meeting_id)?;
        let (presentation, scenes) = match &manifest {
            Some(manifest) => {
                let document: Value =
                    serde_json::from_str(&store_presentation::current_document(policy, manifest)?)?;
                (
                    json!({"presentation_id":manifest.presentation_id,"revision":manifest.revision,
                        "title":manifest.title,"updated_at_ms":manifest.updated_at_ms}),
                    store_presentation::presented_scene_data(&document),
                )
            }
            None => (Value::Null, Vec::new()),
        };
        occurrences.push(json!({"index":index+1,"meeting_id":meeting_id,
            "scheduled_at_ms":scheduled_at_ms,"state":state,"presentation":presentation,
            "scenes":scenes}));
    }
    Ok(occurrences)
}

fn write(
    root: &Path,
    context: &McpChannelRequestContext,
    trusted: &Value,
    action: &str,
    arguments: &Value,
) -> anyhow::Result<Value> {
    let operation = arg(arguments, "operation_id")?.to_owned();
    let intent = format!("{:x}", Sha256::digest(serde_json::to_vec(arguments)?));
    // The slide engine runs outside any writer lock, against the revision the
    // caller named; that revision is re-checked inside the write transaction.
    let (next, expected) = {
        let (mut core, mut policy) = connections(root, false)?;
        let core_tx = core.transaction_with_behavior(TransactionBehavior::Deferred)?;
        let policy_tx = policy.transaction_with_behavior(TransactionBehavior::Deferred)?;
        let scope = resolve(&core_tx, &policy_tx, context, trusted, arguments, false)?;
        if let Some((result, refs)) = store_presentation::replay(
            &policy_tx,
            &operation,
            &scope.meeting.owner_user_id,
            &intent,
        )? {
            drop(policy_tx);
            drop(core_tx);
            publish(root, &refs)?;
            return Ok(result);
        }
        anyhow::ensure!(
            !matches!(
                scope.meeting.state,
                super::super::workjet_jour_fixe_contract::MeetingState::Cancelled
                    | super::super::workjet_jour_fixe_contract::MeetingState::Failed
            ),
            "a cancelled or failed meeting keeps its presentation unchanged"
        );
        let request = &arguments["request"];
        match action {
            "create_presentation" => {
                anyhow::ensure!(
                    scope.presentation.is_none(),
                    "this meeting already has a presentation; use replace_document, apply_edits or save_canvas with its current revision"
                );
                let document = request["document"].clone();
                anyhow::ensure!(document.is_object(), "request.document must be an object");
                (
                    store_presentation::validated_document(root, document)?,
                    None,
                )
            }
            _ => {
                let manifest = scope
                    .presentation
                    .as_ref()
                    .context("this meeting has no presentation yet; use create_presentation")?;
                let expected = number(arguments, "expected_revision")?;
                anyhow::ensure!(
                    manifest.revision == expected,
                    "presentation revision changed; read the current presentation (current revision {})",
                    manifest.revision
                );
                let current: Value =
                    serde_json::from_str(scope.document.as_deref().context("document missing")?)?;
                let next = match action {
                    "replace_document" => {
                        let document = request["document"].clone();
                        anyhow::ensure!(document.is_object(), "request.document must be an object");
                        store_presentation::validated_document(root, document)?
                    }
                    "apply_edits" => {
                        let operations = request["operations"].clone();
                        anyhow::ensure!(
                            operations.is_array(),
                            "request.operations must be an array"
                        );
                        store_presentation::engine_document(
                            root,
                            json!({"op":"applyEdits","document":current,"operations":operations}),
                        )?
                    }
                    _ => {
                        let slide_id = request["slide_id"]
                            .as_str()
                            .context("request.slide_id is required")?;
                        anyhow::ensure!(
                            request["scene"].is_object(),
                            "request.scene must be an object"
                        );
                        store_presentation::engine_document(
                            root,
                            json!({"op":"updateCanvas","document":current,"slideId":slide_id,"scene":request["scene"]}),
                        )?
                    }
                };
                (next, Some((expected, manifest.document_sha256.clone())))
            }
        }
    };
    let (mut core, mut policy) = connections(root, true)?;
    let core_tx = core.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let policy_tx = policy.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let scope = resolve(&core_tx, &policy_tx, context, trusted, arguments, true)?;
    if let Some((result, refs)) = store_presentation::replay(
        &policy_tx,
        &operation,
        &scope.meeting.owner_user_id,
        &intent,
    )? {
        drop(policy_tx);
        drop(core_tx);
        publish(root, &refs)?;
        return Ok(result);
    }
    match (&expected, &scope.presentation) {
        (None, None) => {}
        (Some((revision, sha)), Some(current)) => anyhow::ensure!(
            current.revision == *revision && &current.document_sha256 == sha,
            "presentation revision changed; read the current presentation"
        ),
        _ => anyhow::bail!("presentation changed concurrently; read the current presentation"),
    }
    let actor = format!("supervisor:{}", scope.meeting.supervisor.workjet_thread_id);
    let (manifest, refs) = store_presentation::commit_revision(
        root,
        &policy_tx,
        store_presentation::NewRevision {
            meeting: &scope.meeting,
            prior: scope.presentation.as_ref(),
            document: &next,
            source: wire::PresentationSource::Agent,
            actor: &actor,
            operation_id: &operation,
        },
    )?;
    let result = store_presentation::receipt(&operation, &manifest)?;
    store_presentation::record_operation(
        &policy_tx,
        &operation,
        &scope.meeting.owner_user_id,
        &manifest.presentation_id,
        &intent,
        &result,
        &refs,
    )?;
    policy_tx.commit()?;
    core_tx.commit()?;
    publish(root, &refs)?;
    Ok(result)
}

/// Derives the narration deck from a stored presentation revision and hands it
/// to the existing prepare_deck path, so all meeting checks stay in one place.
fn publish_deck(
    root: &Path,
    context: &McpChannelRequestContext,
    trusted: &Value,
    arguments: &Value,
) -> anyhow::Result<Value> {
    let operation = arg(arguments, "operation_id")?.to_owned();
    let presentation_revision = number(arguments, "presentation_revision")?;
    let expected_meeting_revision = number(arguments, "expected_meeting_revision")?;
    let deck_revision = number(arguments, "deck_revision")?;
    let (meeting_id, document) = {
        let (mut core, mut policy) = connections(root, false)?;
        let core_tx = core.transaction_with_behavior(TransactionBehavior::Deferred)?;
        let policy_tx = policy.transaction_with_behavior(TransactionBehavior::Deferred)?;
        let scope = resolve(&core_tx, &policy_tx, context, trusted, arguments, false)?;
        let manifest = scope
            .presentation
            .as_ref()
            .context("this meeting has no presentation yet")?;
        anyhow::ensure!(
            manifest.revision == presentation_revision,
            "presentation revision changed; read the current presentation (current revision {})",
            manifest.revision
        );
        let document: Value =
            serde_json::from_str(scope.document.as_deref().context("document missing")?)?;
        (scope.meeting.id.clone(), document)
    };
    let slides = store_presentation::meeting_slides(root, &document, &meeting_id)?;
    let request = json!({"action":"prepare_deck","request":{"operation_id":operation,"meeting_id":meeting_id,
        "expected_revision":expected_meeting_revision,"deck_revision":deck_revision,"slides":slides}});
    workjet_jour_fixe::execute(
        root,
        context,
        workjet_jour_fixe::WRITE_TOOL,
        &request,
        Some(trusted),
    )
}

#[cfg(test)]
#[path = "mcp_workjet_presentation_tests.rs"]
mod tests;
