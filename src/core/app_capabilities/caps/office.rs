//! CAP-2 adapters. The single additional parent-module declaration loads these
//! exclusively owned files; the CAP-1 registry/policy/protocol stay unchanged.
use crate::app_capabilities::caps::{NativeCall, NativeRoute};
use crate::app_capabilities::schema::PayloadShape;
use crate::app_capabilities::{
    CapCallContext, CapabilityBackend, CapabilityHost, CapabilityRegistry, CtoxError, ErrorCode,
};
use crate::business_os::office_engine::{self as engine, OfficeKind};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;
#[path = "blob.rs"]
pub mod blob;
#[path = "doc_extract.rs"]
pub mod doc_extract;
#[path = "mail.rs"]
pub mod mail;
#[path = "speech.rs"]
pub mod speech;
#[cfg(test)]
#[path = "cap2_tests.rs"]
mod tests;

fn invalid(path: &str) -> CtoxError {
    CtoxError::invalid(path)
}
fn decode<T: DeserializeOwned>(value: &Value) -> Result<T, CtoxError> {
    serde_json::from_value(value.clone()).map_err(|_| invalid("$.args"))
}
fn native_error(_: impl std::fmt::Display) -> CtoxError {
    CtoxError::new(ErrorCode::Unavailable, "native capability unavailable")
}
const fn route(
    cap: &'static str,
    function: &'static str,
    command: &'static str,
    rust: &'static str,
    source: &'static str,
) -> NativeRoute {
    NativeRoute::new(cap, function, command, rust, source, PayloadShape::Object)
}
pub static ROUTES: &[NativeRoute] = &[
    route(
        "office.edit",
        "prepare",
        "cap.office.prepare",
        "office_engine::prepare",
        "src/core/business_os/office_engine.rs:1998",
    ),
    route(
        "office.edit",
        "apply",
        "cap.office.apply",
        "office_engine::apply_changes",
        "src/core/business_os/office_engine.rs:8924",
    ),
    route(
        "office.render",
        "render",
        "cap.office.render",
        "office_engine::export / compile_document_to_email",
        "src/core/business_os/office_engine.rs:9022",
    ),
    route(
        "office.settings",
        "save",
        "ctox.office.settings.save",
        "store::handle_workspace_control_command",
        "src/core/business_os/store.rs:15623",
    ),
    route(
        "blob",
        "materialize",
        "cap.blob.materialize",
        "ctox_transfers::Store::read_completed_peer_artifact",
        "src/core/transfers/src/lib.rs:198",
    ),
    route(
        "blob",
        "export",
        "cap.blob.export",
        "ctox_transfers::Store::read_completed_peer_artifact",
        "src/core/transfers/src/lib.rs:198",
    ),
    route(
        "doc.extract",
        "extract",
        "cap.doc.extract",
        "ctox_pdf_parse::parse_pdf_bytes",
        "src/tools/pdf-parse/src/lib.rs:18",
    ),
    route(
        "doc.html",
        "query",
        "cap.doc.html",
        "scraper::Html::parse_document / Selector::parse",
        "src/core/business_os/importer.rs:3081",
    ),
    route(
        "web",
        "fetch",
        "cap.web.fetch",
        "ctox_web_stack::run_ctox_web_read_tool",
        "src/tools/web-stack/src/web_search.rs:845",
    ),
    route(
        "dataframe",
        "window",
        "cap.dataframe.window",
        "knowledge::knowledge_table_row_window",
        "src/core/knowledge/data.rs:1546",
    ),
    route(
        "speech",
        "transcribe",
        "cap.speech.transcribe",
        "communication::gateway::transcribe_audio_file",
        "src/core/communication/gateway.rs:329",
    ),
    route(
        "speech",
        "synthesize",
        "cap.speech.synthesize",
        "SpeechGateway::synthesize_verified_async",
        "src/core/execution/speech.rs:582",
    ),
    route(
        "speech.stt",
        "transcribe",
        "cap.speech.transcribe",
        "communication::gateway::transcribe_audio_file",
        "src/core/communication/gateway.rs:329",
    ),
];
pub fn register(registry: &mut CapabilityRegistry) -> Result<(), CtoxError> {
    for route in ROUTES.iter().chain(mail::ROUTES) {
        registry.register(*route)?;
    }
    Ok(())
}
pub fn registry() -> CapabilityRegistry {
    let mut registry = CapabilityRegistry::cap1();
    register(&mut registry).expect("duplicate CAP-2 function");
    registry
}
/// Resolve JS dotted methods to fixed routes before canonical manifest policy.
pub fn call_path(route: &NativeRoute) -> String {
    if (route.capability.contains('.')
        && route.capability.rsplit('.').next() == Some(route.function))
        || (route.capability == "doc.html" && route.function == "query")
    {
        route.capability.to_owned()
    } else {
        format!("{}.{}", route.capability, route.function)
    }
}
fn resolve(name: &str, function: &str) -> Option<NativeRoute> {
    let all = || {
        crate::app_capabilities::caps::all_routes()
            .chain(ROUTES)
            .chain(mail::ROUTES)
    };
    all()
        .find(|route| route.capability == name && route.function == function)
        .copied()
        .or_else(|| {
            let path = format!("{name}.{function}");
            all()
                .filter(|route| call_path(route) == path)
                .max_by_key(|route| route.capability.len())
                .copied()
        })
}
fn wire_args(name: String, function: String, args: Value) -> (String, String, Value) {
    let Some(route) = resolve(&name, &function) else {
        return (name, function, args);
    };
    let cap2 = ROUTES
        .iter()
        .chain(mail::ROUTES)
        .any(|r| r.capability == route.capability && r.function == route.function);
    let args = if cap2 { json!({"payload":args}) } else { args };
    (route.capability.into(), route.function.into(), args)
}
/// Typed JS-facing entry point. The internal CAP-1 envelope remains an
/// implementation detail; authorization receives the complete request payload.
pub fn invoke(
    host: &dyn CapabilityHost,
    backend: &dyn CapabilityBackend,
    ctx: &CapCallContext,
    name: &str,
    function: &str,
    args: Value,
) -> Result<Value, CtoxError> {
    let (name, function, args) = wire_args(name.into(), function.into(), args);
    registry().invoke(host, backend, ctx, &name, &function, args)
}
/// LocalTransport adapter retaining CAP-1 framing, errors and correlation.
pub fn handle(
    host: &dyn CapabilityHost,
    backend: &dyn CapabilityBackend,
    message: crate::app_capabilities::protocol::CapMessage,
) -> Result<crate::app_capabilities::protocol::CapMessage, CtoxError> {
    use crate::app_capabilities::protocol::CapMessage;
    let message = match message {
        CapMessage::Invoke {
            request_id,
            ctx,
            name,
            function,
            args,
        } => {
            let (name, function, args) = wire_args(name, function, args);
            CapMessage::Invoke {
                request_id,
                ctx,
                name,
                function,
                args,
            }
        }
        message => message,
    };
    registry().handle(host, backend, message)
}
/// Instantiate on the Core blocking executor, never the store writer or a Node
/// process. BlobAccess is supplied by the admitted runtime/transfer owner.
pub struct Cap2Backend<'a> {
    pub root: &'a Path,
    pub blobs: &'a dyn blob::BlobAccess,
}
impl CapabilityBackend for Cap2Backend<'_> {
    fn execute(&self, ctx: &CapCallContext, call: &NativeCall) -> Result<Value, CtoxError> {
        let name = call.route().capability;
        let function = call.route().function;
        match name {
            "office.edit" | "office.render" => office(self.blobs, ctx, function, call.payload()),
            "blob" => blob::execute(self.blobs, ctx, call.payload()),
            "doc" | "doc.extract" | "doc.html" | "web" | "dataframe" => {
                doc_extract::execute(self.root, self.blobs, ctx, name, function, call.payload())
            }
            "speech" | "speech.stt" => {
                speech::execute(self.root, self.blobs, ctx, function, call.payload())
            }
            "mail.send" => mail::send(self.root, self.blobs, ctx, call.payload()),
            "mail.read" => mail::read(self.root, call.payload()),
            "mail.delivery_log" => mail::delivery_log(self.root, call.payload()),
            "office.settings" | "mail.channel" | "mail.server" => {
                mail::validate_legacy(call.payload())?;
                crate::app_capabilities::native::NativeCommandBackend::new(self.root)
                    .execute(ctx, call)
                    .map(mail::redact_paths)
            }
            _ => crate::app_capabilities::native::NativeCommandBackend::new(self.root)
                .execute(ctx, call),
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OfficeSession {
    pub kind: OfficeKind,
    pub source: blob::BlobRef,
    pub editor: blob::BlobRef,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Prepare {
    kind: OfficeKind,
    #[serde(alias = "blob_id")]
    blob: blob::BlobRef,
    #[serde(default)]
    implemented_features: Vec<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Apply {
    session: OfficeSession,
    changes: blob::BlobRef,
    base_version: String,
    #[serde(default)]
    implemented_features: Vec<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Format {
    Docx,
    Xlsx,
    Pdf,
    EmailHtml,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Render {
    session: OfficeSession,
    to: Format,
}
fn prepared(
    blobs: &dyn blob::BlobAccess,
    ctx: &CapCallContext,
    source: blob::BlobRef,
    p: engine::PreparedEditorPayload,
) -> Result<Value, CtoxError> {
    let editor = blob::publish(blobs, ctx, &p.editor_payload, "application/octet-stream")?;
    let session = OfficeSession {
        kind: p.kind,
        source,
        editor,
    };
    Ok(
        json!({"session":session,"model":p.editor_manifest,"manifest":p.manifest,
        "protocol":p.protocol,"protocol_version":p.protocol_version,"sha256":session.editor.sha256}),
    )
}
fn office(
    blobs: &dyn blob::BlobAccess,
    ctx: &CapCallContext,
    function: &str,
    args: &Value,
) -> Result<Value, CtoxError> {
    match function {
        "prepare" => {
            let request: Prepare = decode(args)?;
            let source = blob::read(blobs, ctx, &request.blob)?;
            let source = if request.kind == OfficeKind::Spreadsheet && !source.starts_with(b"PK") {
                engine::delimited_text_to_xlsx(&source).map_err(native_error)?
            } else {
                source
            };
            let canonical = if engine::sha256_hex(&source) == request.blob.sha256 {
                request.blob
            } else {
                blob::publish(blobs, ctx, &source, request.kind.canonical_mime())?
            };
            let value = engine::prepare(
                request.kind,
                &source,
                engine::PrepareOptions {
                    implemented_features: request.implemented_features,
                },
            )
            .map_err(native_error)?;
            prepared(blobs, ctx, canonical, value)
        }
        "apply" => {
            let request: Apply = decode(args)?;
            if request.base_version != request.session.editor.sha256 {
                return Err(CtoxError::new(
                    ErrorCode::Conflict,
                    "editor base version changed",
                ));
            }
            let base = blob::read(blobs, ctx, &request.session.editor)?;
            let changes = blob::read(blobs, ctx, &request.changes)?;
            let p = engine::apply_changes(
                request.session.kind,
                &base,
                &changes,
                engine::ApplyChangesOptions {
                    expected_base_sha256: request.base_version,
                    implemented_features: request.implemented_features,
                },
            )
            .map_err(native_error)?;
            let source = blob::read(blobs, ctx, &request.session.source)?;
            let package = engine::export(request.session.kind, &p.editor_payload, Some(&source))
                .map_err(native_error)?;
            let canonical = blob::publish(blobs, ctx, &package.bytes, &package.mime_type)?;
            let mut result = prepared(blobs, ctx, canonical.clone(), p)?;
            result["blob_id"] = json!(canonical);
            result["sha256"] = json!(canonical.sha256);
            result["manifest"] = json!(package.manifest);
            Ok(result)
        }
        "render" => {
            let request: Render = decode(args)?;
            match request.to {
                Format::Pdf => {
                    return Err(CtoxError::new(
                        ErrorCode::Unavailable,
                        "native office PDF exporter is not registered",
                    ))
                }
                Format::Docx if request.session.kind != OfficeKind::Document => {
                    return Err(invalid("$.to"))
                }
                Format::Xlsx if request.session.kind != OfficeKind::Spreadsheet => {
                    return Err(invalid("$.to"))
                }
                Format::EmailHtml if request.session.kind != OfficeKind::Document => {
                    return Err(invalid("$.to"))
                }
                _ => {}
            }
            let editor = blob::read(blobs, ctx, &request.session.editor)?;
            let source = blob::read(blobs, ctx, &request.session.source)?;
            let package = engine::export(request.session.kind, &editor, Some(&source))
                .map_err(native_error)?;
            if matches!(request.to, Format::EmailHtml) {
                let frozen =
                    engine::compile_document_to_email(&package.bytes).map_err(native_error)?;
                let html = blob::publish(blobs, ctx, frozen.html.as_bytes(), "text/html")?;
                let text = blob::publish(blobs, ctx, frozen.text.as_bytes(), "text/plain")?;
                let mut assets = Vec::new();
                for asset in frozen.assets {
                    let reference = blob::publish(blobs, ctx, &asset.bytes, &asset.mime_type)?;
                    assets.push(json!({"content_id":asset.content_id,"mime_type":asset.mime_type,"blob_id":reference}));
                }
                Ok(
                    json!({"blob_id":html,"sha256":html.sha256,"text_blob":text,"assets":assets,"compiler_id":frozen.compiler_id}),
                )
            } else {
                let reference = blob::publish(blobs, ctx, &package.bytes, &package.mime_type)?;
                Ok(
                    json!({"blob_id":reference,"sha256":reference.sha256,"mime_type":package.mime_type}),
                )
            }
        }
        _ => Err(CtoxError::new(
            ErrorCode::NotFound,
            "unknown office function",
        )),
    }
}
