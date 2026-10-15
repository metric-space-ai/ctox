//! Native PDF/OCR, DOM parsing, web read and Parquet window adapters.
use super::*;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Extract {
    #[serde(alias = "blob_id")]
    blob: blob::BlobRef,
    #[serde(default = "pdf_kind")]
    kind: String,
    #[serde(default = "enable_ocr")]
    ocr: bool,
}
fn pdf_kind() -> String {
    "pdf".into()
}
fn enable_ocr() -> bool {
    true
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HtmlQuery {
    #[serde(alias = "blob_id")]
    blob: blob::BlobRef,
    selector: String,
    attribute: Option<String>,
    #[serde(default = "max_matches")]
    limit: usize,
}
fn max_matches() -> usize {
    1000
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Window {
    table: String,
    #[serde(default, alias = "offset")]
    skip: usize,
    limit: usize,
    filter: Option<Value>,
    #[serde(default)]
    sort: Vec<Value>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Fetch {
    url: String,
    #[serde(default)]
    find: Vec<String>,
}
pub(super) fn execute(
    root: &Path,
    blobs: &dyn blob::BlobAccess,
    ctx: &CapCallContext,
    name: &str,
    function: &str,
    args: &Value,
) -> Result<Value, CtoxError> {
    match (name, function) {
        ("doc", "extract") | ("doc.extract", "extract") => {
            let request: Extract = decode(args)?;
            if !["pdf", "document", "spreadsheet", "text"].contains(&request.kind.as_str()) {
                return Err(invalid("$.kind"));
            }
            let bytes = blob::read(blobs, ctx, &request.blob)?;
            match request.kind.as_str() {
                "pdf" => {
                    let parsed = ctox_pdf_parse::parse_pdf_bytes(
                        &bytes,
                        ctox_pdf_parse::LiteParseConfigOverrides {
                            ocr_enabled: Some(request.ocr),
                            ..Default::default()
                        },
                    )
                    .map_err(native_error)?;
                    Ok(json!({"text":parsed.text,"result":parsed}))
                }
                "document" | "spreadsheet" => {
                    let kind = if request.kind == "document" {
                        OfficeKind::Document
                    } else {
                        OfficeKind::Spreadsheet
                    };
                    let manifest = engine::inspect(kind, &bytes).map_err(native_error)?;
                    Ok(json!({"text":manifest.primary_text,"manifest":manifest}))
                }
                _ => Ok(json!({"text":String::from_utf8(bytes).map_err(native_error)?})),
            }
        }
        ("doc", "html") | ("doc.html", "query") => {
            let request: HtmlQuery = decode(args)?;
            if request.selector.len() > 4096 || request.limit == 0 || request.limit > 1000 {
                return Err(invalid("$.selector"));
            }
            let selector =
                scraper::Selector::parse(&request.selector).map_err(|_| invalid("$.selector"))?;
            if request
                .attribute
                .as_ref()
                .is_some_and(|s| s.len() > 128 || s.chars().any(char::is_control))
            {
                return Err(invalid("$.attribute"));
            }
            let bytes = blob::read(blobs, ctx, &request.blob)?;
            let document =
                scraper::Html::parse_document(&String::from_utf8(bytes).map_err(native_error)?);
            let values = document
                .select(&selector)
                .take(request.limit)
                .map(|node| match &request.attribute {
                    Some(attribute) => node.value().attr(attribute).map(str::to_owned),
                    None => Some(node.text().collect::<Vec<_>>().join("")),
                })
                .collect::<Vec<_>>();
            Ok(json!({"matches":values}))
        }
        ("dataframe", "window") => {
            let request: Window = decode(args)?;
            crate::app_capabilities::protocol::identifier(&request.table, "$.table")?;
            // Existing source only accepts ranges: never silently ignore selectors.
            if request.limit == 0
                || request.limit > 1000
                || request
                    .filter
                    .as_ref()
                    .is_some_and(|v| !v.is_null() && v != &json!({}))
                || !request.sort.is_empty()
            {
                return Err(invalid("$.window"));
            }
            let window = crate::knowledge::knowledge_table_row_window(
                root,
                &request.table,
                request.skip,
                request.limit,
            )
            .map_err(|error| {
                if crate::knowledge::is_knowledge_table_not_found(&error) {
                    CtoxError::new(ErrorCode::NotFound, "dataframe table not found")
                } else {
                    native_error(error)
                }
            })?;
            Ok(
                json!({"rows":window.rows,"row_count":window.row_count,"content_hash":window.content_hash,"schema_hash":window.schema_hash}),
            )
        }
        ("web", "fetch") => {
            let request: Fetch = decode(args)?;
            let url = url::Url::parse(&request.url).map_err(|_| invalid("$.url"))?;
            if !["http", "https"].contains(&url.scheme())
                || !url.username().is_empty()
                || url.password().is_some()
                || request.url.len() > 8192
                || request.find.len() > 64
            {
                return Err(invalid("$.url"));
            }
            let result = ctox_web_stack::run_ctox_web_read_tool(
                root,
                &ctox_web_stack::DirectWebReadRequest {
                    url: request.url,
                    find: request.find,
                    include_full_text: true,
                    timeout_cap_ms: Some(30000),
                    max_artifact_bytes: Some(blob::MAX_BLOB_BYTES as usize),
                    ..Default::default()
                },
            )
            .map_err(native_error)?;
            if result["ok"] != true {
                return Err(native_error("native web read failed"));
            }
            let reference = blob::publish(
                blobs,
                ctx,
                result["page_text"].as_str().unwrap_or("").as_bytes(),
                "text/plain",
            )?;
            Ok(
                json!({"blob_id":reference,"sha256":reference.sha256,"url":result["url"],
            "title":result["title"],"is_pdf":result["is_pdf"],"summary":result["summary"]}),
            )
        }
        _ => Err(CtoxError::new(
            ErrorCode::NotFound,
            "unknown extraction function",
        )),
    }
}
