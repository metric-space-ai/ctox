//! The artifact server: `actions/upload-artifact`'s backend, in both protocol
//! versions.
//!
//! This is a port of act's `pkg/artifacts`, a small HTTP service the runner
//! starts beside a job and advertises through `ACTIONS_RESULTS_URL`.
//! `upload-artifact` speaks whichever version its own `version:` input asks for,
//! so both are served from the same directory and frequently coexist: a V3
//! upload writes `<run>/<itemPath>` while a V4 upload writes
//! `<run>/<name>/<name>.zip`, and each version's listing then reports the
//! other's entries.
//!
//! # The two protocols
//!
//! **V3** (`upload-artifact@v3`) is a REST shape over four routes:
//! `POST`/`PATCH`/`GET /_apis/pipelines/workflows/:runId/artifacts` and
//! `PUT /upload/:runId`. It reports a container's contents through
//! `GET /download/:container?itemPath=…` and serves the bytes through
//! `GET /artifact/*path`.
//!
//! **V4** is twirp-over-JSON: seven methods under
//! `/twirp/github.actions.results.api.v1.ArtifactService`. Two of them are
//! unauthenticated blob transfers guarded by an HMAC over the query string,
//! and the other five are the control plane.
//!
//! # Deviations from upstream
//!
//! * **No protobuf.** protojson's output is the wire format, so the shapes are
//!   reproduced directly: fixed structs, fields in protobuf field-number order,
//!   `int64` as a JSON string, and the timestamp rendered by the rule above.
//!   Requests are parsed leniently the way `protojson.Unmarshal` is — both the
//!   `json_name` and the original snake_case spelling are accepted, an `int64`
//!   may arrive as a string or a number, and unknown fields are ignored.
//! * **The `Host` is read from the request being answered.** act stores it in
//!   a field on the shared route struct, set by three of the seven handlers and
//!   read back by those same three. That is a cross-request coupling with no
//!   upside, and `req.Host` is the same value.
//! * **A panic becomes an error.** Every act handler `panic`s on an IO failure
//!   — a missing artifact, a directory that is not there, a create that fails —
//!   and `net/http` answers a panicking handler by closing the connection
//!   without writing anything. The port returns that as an error so the
//!   connection closes the same way, rather than inventing a status code.
//! * **`httprouter`'s `Allow` header is dropped.** A wrong method on a known
//!   path is still a 405; only the header listing the accepted methods is
//!   missing.
//! * The port runs a blocking server on its own thread, like the cache.

mod fs;
mod path;
mod signature;

pub use fs::{ArtifactFs, EntryInfo, MapFs, OsFs};
/// The Go lexical path helpers, re-exported because a container back-end needs
/// the same `Clean` and relative-path rules for its bind mounts.
pub use path::{clean, join, rel, to_slash};
pub use signature::{
    build_signature, format_signed_expiry, parse_signed_expiry, proto_timestamp, signatures_equal,
    SIGNED_URL_LIFETIME_SECONDS,
};

/// act signs with Go's `base64.URLEncoding`, which is padded. The JWT in
/// [`crate::common::auth`] needs the raw variant instead, so both live in
/// [`crate::base64url`] and are re-exported here under the names this module
/// used.
pub use crate::base64url::{decode as base64_url_decode, encode_padded as base64_url_encode};

use std::io;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, SystemTime};

use serde::Serialize;

use crate::http::{self, Call, Reply, Service as HttpService, NO_CONTENT_TYPE};

/// The V4 route prefix.
pub const ARTIFACT_V4_ROUTE_BASE: &str = "/twirp/github.actions.results.api.v1.ArtifactService";
/// The suffix act appends to a file uploaded gzip-encoded.
pub const ARTIFACT_V4_CONTENT_ENCODING: &str = "application/zip";
/// The marker for a gzip-encoded upload, and the name the client asks for
/// without it.
pub const GZIP_EXTENSION: &str = ".gz__";

/// Why the service could not be started.
#[derive(Debug)]
pub enum ServerError {
    /// The address could not be bound.
    Bind(io::Error),
}

impl std::fmt::Display for ServerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Bind(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for ServerError {}

impl From<io::Error> for ServerError {
    fn from(err: io::Error) -> Self {
        Self::Bind(err)
    }
}

/// The request state the handlers share: the artifact directory, the
/// filesystem, and nothing else.
pub struct Service {
    base_dir: String,
    fs: Arc<dyn ArtifactFs>,
}

impl Service {
    /// A service over `base_dir`, writing through `fs`.
    ///
    /// act's `Serve` returns immediately without registering a single route
    /// when the path is empty — the server is off — so this takes the same
    /// decision rather than serving a service with no directory.
    pub fn new(base_dir: &Path, fs: Arc<dyn ArtifactFs>) -> Option<Self> {
        if base_dir.as_os_str().is_empty() {
            return None;
        }
        Some(Service {
            base_dir: base_dir.to_string_lossy().into_owned(),
            fs,
        })
    }

    /// A service over a real directory, the shape `Serve` builds.
    pub fn on_disk(base_dir: &Path) -> Option<Self> {
        Service::new(base_dir, Arc::new(OsFs))
    }

    /// The artifact directory.
    pub fn base_dir(&self) -> &str {
        &self.base_dir
    }

    /// The filesystem.
    pub fn fs(&self) -> &Arc<dyn ArtifactFs> {
        &self.fs
    }

    /// Routes one request. `Ok(None)` is a path that is not ours at all, which
    /// `httprouter` answers with a bare 404.
    pub fn route(&self, call: &Call) -> Result<Option<Reply>, io::Error> {
        let method = call.method.as_str();
        let path = call.path.as_str();

        // The V4 routes are all literals below the prefix, so they are checked
        // first and cost one comparison each.
        if let Some(method_name) = path.strip_prefix(ARTIFACT_V4_ROUTE_BASE) {
            let method_name = method_name.trim_start_matches('/');
            return match (method, method_name) {
                ("POST", "CreateArtifact") => Ok(Some(self.create_artifact(call)?)),
                ("POST", "FinalizeArtifact") => Ok(Some(self.finalize_artifact(call)?)),
                ("POST", "ListArtifacts") => Ok(Some(self.list_artifacts(call)?)),
                ("POST", "GetSignedArtifactURL") => Ok(Some(self.get_signed_artifact_url(call)?)),
                ("POST", "DeleteArtifact") => Ok(Some(self.delete_artifact(call)?)),
                ("PUT", "UploadArtifact") => Ok(Some(self.upload_artifact(call)?)),
                ("GET", "DownloadArtifact") => Ok(Some(self.download_artifact(call)?)),
                // A known V4 path, wrong method: `httprouter`'s 405.
                (_, name) if is_v4_method_name(name) => Ok(Some(Reply::status(405))),
                _ => Ok(None),
            };
        }

        // V3. Four routes; the three methods on the artifacts path are
        // registered by two functions upstream, `uploads` taking POST and
        // PATCH and `downloads` taking GET, which is why all three coexist.
        let artifacts = match_route(path, "/_apis/pipelines/workflows/:runId/artifacts");
        let upload = match_route(path, "/upload/:runId");
        let download = match_route(path, "/download/:container");
        let artifact = match_route(path, "/artifact/*path");
        if artifacts.is_none() && upload.is_none() && download.is_none() && artifact.is_none() {
            return Ok(None);
        }

        if let Some(params) = artifacts {
            let run_id = &params[0];
            return Ok(Some(match method {
                "POST" => self.v3_create(call, run_id),
                "PATCH" => v3_message("success"),
                "GET" => self.v3_list(call, run_id)?,
                // A registered path with the wrong method is httprouter's 405.
                _ => Reply::status(405),
            }));
        }
        if let Some(params) = upload {
            return Ok(Some(match method {
                "PUT" => self.v3_upload(call, &params[0])?,
                _ => Reply::status(405),
            }));
        }
        if let Some(params) = download {
            return Ok(Some(match method {
                "GET" => self.v3_container(call, &params[0])?,
                _ => Reply::status(405),
            }));
        }
        // httprouter's catch-all keeps the leading separator, and act drops it
        // before resolving.
        let params = artifact.expect("checked above");
        Ok(Some(match method {
            "GET" => self.v3_artifact(&params[0][1..])?,
            _ => Reply::status(405),
        }))
    }

    // -- V3 ---------------------------------------------------------------

    /// `POST /_apis/pipelines/workflows/:runId/artifacts` — where to PUT the
    /// files.
    fn v3_create(&self, call: &Call, run_id: &str) -> Reply {
        Reply::new(
            200,
            http::SNIFFED,
            encode(serde_json::json!({
                "fileContainerResourceUrl": format!("http://{}/upload/{}", call.host, run_id),
            })),
        )
    }

    /// `PUT /upload/:runId?itemPath=…` — store one item.
    ///
    /// The write is append-or-truncate on `Content-Range`, which is how the
    /// client splits a large file. A gzip-encoded body is stored under a name
    /// ending in `.gz__` and served back with the encoding header.
    fn v3_upload(&self, call: &Call, run_id: &str) -> Result<Reply, io::Error> {
        let mut item_path = call.query_get("itemPath").to_string();
        if call.header("Content-Encoding") == Some("gzip") {
            item_path.push_str(GZIP_EXTENSION);
        }

        let run_path = safe_resolve(&self.base_dir, run_id);
        let path = safe_resolve(&run_path, &item_path);

        let content_range = call.header("Content-Range").unwrap_or("");
        if !content_range.is_empty() && !content_range.starts_with("bytes 0-") {
            self.fs.append(&path, &call.body)?;
        } else {
            self.fs.create(&path)?;
            self.fs.append(&path, &call.body)?;
        }

        Ok(v3_message("success"))
    }

    /// `GET /_apis/pipelines/workflows/:runId/artifacts` — the item names
    /// stored under this run.
    ///
    /// Every entry carries the *same* URL: one container per run, and the
    /// client fetches the whole thing through `/download/:container` and
    /// asks it what is inside.
    fn v3_list(&self, call: &Call, run_id: &str) -> Result<Reply, io::Error> {
        let path = safe_resolve(&self.base_dir, run_id);
        let entries = self.fs.read_dir(&path)?;
        // A nil slice, not an empty one: Go marshals nil as `null`, so a run
        // with nothing in it answers `{"count":0,"value":null}`.
        let mut list: Option<Vec<serde_json::Value>> = None;
        let count = entries.len();
        for entry in &entries {
            list.get_or_insert_with(Vec::new).push(serde_json::json!({
                "name": entry.name,
                "fileContainerResourceUrl": format!("http://{}/download/{}", call.host, run_id),
            }));
        }
        Ok(Reply::new(
            200,
            http::SNIFFED,
            encode(serde_json::json!({
                "count": count,
                "value": list,
            })),
        ))
    }

    /// `GET /download/:container?itemPath=…` — one item's metadata.
    fn v3_container(&self, call: &Call, container: &str) -> Result<Reply, io::Error> {
        let item_path = call.query_get("itemPath").to_string();
        let path = safe_resolve(&self.base_dir, &join(&[container, &item_path]));

        let mut files: Vec<serde_json::Value> = Vec::new();
        for (walked, is_dir) in self.fs.walk(&path)? {
            if is_dir {
                continue;
            }
            // act asks `filepath.Rel` for the offset, which is `.` when the
            // walk was rooted at that very file — which is the normal case
            // here, and the reason the `contentLocation` ends in `/.`.
            let relative = rel(&path, &walked).unwrap_or_else(|| ".".to_string());
            let relative = relative
                .strip_suffix(GZIP_EXTENSION)
                .unwrap_or(&relative)
                .to_string();
            let full = to_slash(&join(&[&item_path, &relative]));
            files.push(serde_json::json!({
                "path": full,
                "itemType": "file",
                "contentLocation": format!(
                    "http://{}/artifact/{}/{}/{}",
                    call.host, container, item_path, relative,
                ),
            }));
        }

        Ok(Reply::new(
            200,
            http::SNIFFED,
            encode(serde_json::json!({ "value": files })),
        ))
    }

    /// `GET /artifact/*path` — the bytes.
    ///
    /// A stored `.gz__` is served with `Content-Encoding: gzip` so the client
    /// inflates it. `artifactcache`'s equivalent does not need the distinction;
    /// this one is why `Reply` carries headers at all.
    fn v3_artifact(&self, path: &str) -> Result<Reply, io::Error> {
        let safe = safe_resolve(&self.base_dir, path);
        let (body, gzipped) = match self.fs.read(&safe) {
            Ok(bytes) => (bytes, false),
            Err(err) => match self.fs.read(&format!("{safe}{GZIP_EXTENSION}")) {
                Ok(bytes) => (bytes, true),
                Err(_) => return Err(err),
            },
        };
        let reply = Reply::new(200, http::SNIFFED, body);
        Ok(if gzipped {
            reply.with_header("Content-Encoding", "gzip")
        } else {
            reply
        })
    }

    // -- V4 ---------------------------------------------------------------

    /// `POST …/CreateArtifact` — reserve a name, and hand back a signed upload
    /// URL.
    ///
    /// The file is created empty: the directory is the reservation, and the
    /// listing in `list_artifacts` reports directories as artifacts.
    fn create_artifact(&self, call: &Call) -> Result<Reply, io::Error> {
        let body = match call.json_body() {
            Ok(value) => value,
            Err(_) => return Ok(Reply::status(500)),
        };
        let Some(run_id) = validate_run_id(&body) else {
            return Ok(Reply::status(400));
        };
        let name = string_field(&body, "name", "name");
        let path = v4_zip_path(&self.base_dir, run_id, &name);
        self.fs.create(&path)?;

        Ok(Reply::protojson(
            200,
            serde_json::json!({
                "ok": true,
                "signedUploadUrl": self.build_artifact_url(call, "UploadArtifact", &name, run_id),
            }),
        ))
    }

    /// `POST …/FinalizeArtifact` — the upload is complete.
    ///
    /// Nothing is verified: act never compares the reported size or hash
    /// against the file, so neither does the port.
    fn finalize_artifact(&self, call: &Call) -> Result<Reply, io::Error> {
        let body = match call.json_body() {
            Ok(value) => value,
            Err(_) => return Ok(Reply::status(500)),
        };
        if validate_run_id(&body).is_none() {
            return Ok(Reply::status(400));
        }
        Ok(Reply::protojson(
            200,
            serde_json::json!({
                "ok": true,
                "artifactId": artifact_name_to_id(&string_field(&body, "name", "name")).to_string(),
            }),
        ))
    }

    /// `POST …/ListArtifacts` — what this run holds.
    fn list_artifacts(&self, call: &Call) -> Result<Reply, io::Error> {
        let body = match call.json_body() {
            Ok(value) => value,
            Err(_) => return Ok(Reply::status(500)),
        };
        let Some(run_id) = validate_run_id(&body) else {
            return Ok(Reply::status(400));
        };
        let job_run_id = string_field(
            &body,
            "workflow_job_run_backend_id",
            "workflowJobRunBackendId",
        );
        let run_backend_id = string_field(&body, "workflow_run_backend_id", "workflowRunBackendId");
        let name_filter = optional_string_field(&body, "name_filter", "nameFilter");
        let id_filter = optional_int_field(&body, "id_filter", "idFilter");

        let path = safe_resolve(&self.base_dir, &run_id.to_string());
        let entries = self.fs.read_dir(&path)?;

        let now = SystemTime::now();
        let mut artifacts: Vec<MonolithArtifact> = Vec::new();
        for entry in &entries {
            let id = artifact_name_to_id(&entry.name);
            if let Some(name) = &name_filter {
                if name != &entry.name {
                    continue;
                }
            }
            if let Some(id) = id_filter {
                if id != artifact_name_to_id(&entry.name) {
                    continue;
                }
            }
            artifacts.push(MonolithArtifact {
                workflow_run_backend_id: run_backend_id.clone(),
                workflow_job_run_backend_id: job_run_id.clone(),
                database_id: id,
                name: entry.name.clone(),
                size: entry.size,
                // act seeds the entry with the current time and a zero size,
                // and overwrites both when `Info` succeeds. `EntryInfo`
                // reports `None` for the failure case.
                created_at: proto_timestamp(entry.modified.unwrap_or(now)),
            });
        }

        Ok(Reply::protojson(
            200,
            serde_json::to_value(ListArtifactsResponse { artifacts }).expect("serialises"),
        ))
    }

    /// `POST …/GetSignedArtifactURL` — a download URL for a named artifact.
    ///
    /// Nothing is checked: the name need not exist.
    fn get_signed_artifact_url(&self, call: &Call) -> Result<Reply, io::Error> {
        let body = match call.json_body() {
            Ok(value) => value,
            Err(_) => return Ok(Reply::status(500)),
        };
        let Some(run_id) = validate_run_id(&body) else {
            return Ok(Reply::status(400));
        };
        let name = string_field(&body, "name", "name");
        Ok(Reply::protojson(
            200,
            serde_json::json!({
                "signedUrl": self.build_artifact_url(call, "DownloadArtifact", &name, run_id),
            }),
        ))
    }

    /// `POST …/DeleteArtifact` — remove the artifact's whole directory.
    fn delete_artifact(&self, call: &Call) -> Result<Reply, io::Error> {
        let body = match call.json_body() {
            Ok(value) => value,
            Err(_) => {
                return Ok(Reply::status(500));
            }
        };
        let Some(run_id) = validate_run_id(&body) else {
            return Ok(Reply::status(400));
        };
        let name = string_field(&body, "name", "name");
        let path = safe_resolve(&self.base_dir, &join(&[&run_id.to_string(), &name]));
        // `os.RemoveAll` on a path that is not there is a success in Go, and
        // the error is discarded anyway.
        let _ = self.fs.remove_all(&path);

        Ok(Reply::protojson(
            200,
            serde_json::json!({
                "ok": true,
                "artifactId": artifact_name_to_id(&name).to_string(),
            }),
        ))
    }

    /// `PUT …/UploadArtifact` — the blob transfer, guarded by the signature.
    ///
    /// The `comp` switch has three cases upstream and one of them is spelled
    /// wrong: the comment and the client both say `blockList`, the code matches
    /// `blocklist`. So the block-list step falls through every case and the
    /// handler returns without writing, which `net/http` turns into a bare
    /// 200. That is reproduced rather than corrected — the client treats a
    /// 200 as success either way, and the archive is already complete.
    fn upload_artifact(&self, call: &Call) -> Result<Reply, io::Error> {
        let Some((_, _, task_id, name)) = self.verify_signature(call, "UploadArtifact") else {
            return Ok(Reply::status(401));
        };

        match call.query_get("comp") {
            "block" | "appendBlock" => {
                let path = v4_zip_path(&self.base_dir, task_id, &name);
                self.fs.append(&path, &call.body)?;
                // `ctx.JSON` ignores its arguments and only sets the status,
                // so the body is empty — and an empty body gets no
                // `Content-Type`, because `net/http` only sniffs when a
                // handler writes something.
                Ok(Reply::new(201, NO_CONTENT_TYPE, Vec::new()))
            }
            _ => Ok(Reply::new(200, NO_CONTENT_TYPE, Vec::new())),
        }
    }

    /// `GET …/DownloadArtifact` — the blob, guarded by the signature.
    fn download_artifact(&self, call: &Call) -> Result<Reply, io::Error> {
        let Some((_, _, task_id, name)) = self.verify_signature(call, "DownloadArtifact") else {
            return Ok(Reply::status(401));
        };
        let path = v4_zip_path(&self.base_dir, task_id, &name);
        // act discards the open error and copies from the nil file, which
        // panics; `net/http` then closes the connection. Same outcome here.
        let body = self.fs.read(&path)?;
        Ok(Reply::new(200, ARTIFACT_V4_CONTENT_ENCODING, body))
    }

    /// `buildArtifactURL`: `http://<host><prefix>/<endpoint>?sig=…&expires=…
    /// &artifactName=…&taskID=…`.
    ///
    /// `taskID` is the **run** id, not a job id — act passes `runID` from the
    /// request into the same slot the download side reads back.
    fn build_artifact_url(
        &self,
        call: &Call,
        endpoint: &str,
        artifact_name: &str,
        task_id: i64,
    ) -> String {
        let expires = format_signed_expiry(
            SystemTime::now() + Duration::from_secs(SIGNED_URL_LIFETIME_SECONDS as u64),
        );
        let signature = build_signature(endpoint, &expires, artifact_name, task_id);
        format!(
            "http://{}{}/{endpoint}?sig={}&expires={}&artifactName={}&taskID={task_id}",
            call.host.trim_end_matches('/'),
            ARTIFACT_V4_ROUTE_BASE.trim_end_matches('/'),
            base64_url_encode(&signature),
            http::percent_encode(&expires),
            http::percent_encode(artifact_name),
        )
    }

    /// `verifySignature`: the HMAC, then the expiry.
    ///
    /// The order matters. `expires` is inside the signed payload, so an edited
    /// expiry fails the comparison before the expiry check ever runs; the
    /// expiry test only ever rejects a URL act itself issued and that has since
    /// aged past the hour.
    fn verify_signature(&self, call: &Call, endpoint: &str) -> Option<(i64, String, i64, String)> {
        let raw_task_id = call.query_get("taskID");
        let sig = call.query_get("sig");
        let expires = call.query_get("expires");
        let artifact_name = call.query_get("artifactName");

        let presented = base64_url_decode(sig);
        // `strconv.ParseInt`'s error is discarded, so an unparsable task id is
        // simply zero.
        let task_id: i64 = raw_task_id.parse().unwrap_or(0);

        let expected = build_signature(endpoint, expires, artifact_name, task_id);
        if !signatures_equal(&presented, &expected) {
            return None;
        }

        let instant = parse_signed_expiry(expires)?;
        let now = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        if instant < now {
            return None;
        }
        Some((
            instant,
            expires.to_string(),
            task_id,
            artifact_name.to_string(),
        ))
    }
}

impl HttpService for Service {
    fn route(&self, call: &Call) -> Result<Option<Reply>, io::Error> {
        Service::route(self, call)
    }
}

/// `safeResolve`: `filepath.Join(baseDir, filepath.Clean(filepath.Join("/", relPath)))`.
///
/// The inner join roots the relative path, `Clean` collapses any `..` in it,
/// and the outer join puts it back under `baseDir`. The result is that a
/// request cannot name anything outside the artifact directory — which is what
/// `TestDownloadArtifactFileUnsafePath` and `TestArtifactUploadBlobUnsafePath`
/// are about.
pub fn safe_resolve(base_dir: &str, rel_path: &str) -> String {
    let root = path::MAIN_SEPARATOR.to_string();
    join(&[base_dir, &clean(&join(&[&root, rel_path]))])
}

/// `<base>/<runId>/<name>/<name>.zip`, the V4 artifact's shape.
///
/// act applies `safeResolve` three times rather than joining, so a name
/// containing `..` is collapsed at each step.
fn v4_zip_path(base_dir: &str, run_id: i64, name: &str) -> String {
    let run_path = safe_resolve(base_dir, &run_id.to_string());
    let safe = safe_resolve(&run_path, name);
    safe_resolve(&safe, &format!("{name}.zip"))
}

/// `artifactNameToID`: FNV-1a 32, widened to `int64`.
///
/// A hash, not a counter, so the id of an artifact is the same on every host
/// and across restarts — which is what lets a client hold on to it.
pub fn artifact_name_to_id(name: &str) -> i64 {
    let mut hash: u32 = 0x811c_9dc5;
    for byte in name.as_bytes() {
        hash ^= *byte as u32;
        // FNV prime 16777619, with the multiply wrapping as Go's does.
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash as i64
}

/// `validateRunIDV4`: the field is a protobuf `string`, and act parses it into
/// an `int64` only to use it as a path element.
fn validate_run_id(body: &serde_json::Value) -> Option<i64> {
    let raw = string_field(body, "workflow_run_backend_id", "workflowRunBackendId");
    raw.parse::<i64>().ok()
}

/// protojson accepts the message's `json_name` and its original snake_case
/// field name alike, and leaves an absent field at its zero value.
fn string_field(body: &serde_json::Value, snake: &str, camel: &str) -> String {
    for key in [camel, snake] {
        if let Some(serde_json::Value::String(value)) = body.get(key) {
            return value.clone();
        }
    }
    String::new()
}

/// A `google.protobuf.StringValue` is a bare string on the wire, and an absent
/// field is a `nil` pointer — which act distinguishes from an empty one.
fn optional_string_field(body: &serde_json::Value, snake: &str, camel: &str) -> Option<String> {
    for key in [camel, snake] {
        if let Some(serde_json::Value::String(value)) = body.get(key) {
            return Some(value.clone());
        }
    }
    None
}

/// A `google.protobuf.Int64Value` accepts a number or a quoted number.
fn optional_int_field(body: &serde_json::Value, snake: &str, camel: &str) -> Option<i64> {
    for key in [camel, snake] {
        match body.get(key) {
            Some(serde_json::Value::Number(number)) => {
                if let Some(value) = number.as_i64() {
                    return Some(value);
                }
            }
            Some(serde_json::Value::String(text)) => {
                if let Ok(value) = text.parse::<i64>() {
                    return Some(value);
                }
            }
            _ => {}
        }
    }
    None
}

/// `ResponseMessage`.
fn v3_message(message: &str) -> Reply {
    Reply::new(
        200,
        http::SNIFFED,
        encode(serde_json::json!({ "message": message })),
    )
}

/// One entry of a `ListArtifactsResponse`.
///
/// The three `int64` fields are JSON **strings** on the wire, and the fields
/// come out in protobuf field-number order rather than alphabetical order, so
/// this is serialised by hand.
#[derive(Debug)]
struct MonolithArtifact {
    workflow_run_backend_id: String,
    workflow_job_run_backend_id: String,
    database_id: i64,
    name: String,
    size: u64,
    created_at: String,
}

#[derive(Debug, Serialize)]
struct ListArtifactsResponse {
    artifacts: Vec<MonolithArtifact>,
}

impl Serialize for MonolithArtifact {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(Some(6))?;
        // protobuf field numbers, which is the order protojson emits.
        map.serialize_entry("workflowRunBackendId", &self.workflow_run_backend_id)?;
        map.serialize_entry("workflowJobRunBackendId", &self.workflow_job_run_backend_id)?;
        map.serialize_entry("databaseId", &self.database_id.to_string())?;
        map.serialize_entry("name", &self.name)?;
        map.serialize_entry("size", &self.size.to_string())?;
        // A nil `Timestamp` marshals as absent, which is what a `None`
        // modification time produces.
        map.serialize_entry("createdAt", &self.created_at)?;
        map.end()
    }
}

/// `json.Marshal`. A `panic` upstream where act itself panics; every value
/// built here is infallible, so this is a shape, not a failure path.
fn encode(value: serde_json::Value) -> Vec<u8> {
    serde_json::to_vec(&value).unwrap_or_else(|_| b"{}".to_vec())
}

/// Whether a V4 method name is one of the seven registered routes, which is
/// what turns a wrong method into a 405 rather than a 404.
fn is_v4_method_name(name: &str) -> bool {
    matches!(
        name,
        "CreateArtifact"
            | "FinalizeArtifact"
            | "ListArtifacts"
            | "GetSignedArtifactURL"
            | "DeleteArtifact"
            | "UploadArtifact"
            | "DownloadArtifact"
    )
}

/// `httprouter`'s matching: `:name` takes one segment, `*name` takes the rest
/// including its leading separator, and a literal must match exactly.
///
/// The trailing `/` on a literal segment is optional in httprouter only when
/// `RedirectTrailingSlash` is on, and `httprouter.New()` leaves it off, so
/// `/artifacts/` does not match `/artifacts`.
fn match_route(path: &str, pattern: &str) -> Option<Vec<String>> {
    if !path.starts_with('/') {
        return None;
    }
    let mut parts = path[1..].split('/');
    let mut params = Vec::new();
    for segment in pattern[1..].split('/') {
        if let Some(_name) = segment.strip_prefix(':') {
            let value = parts.next()?;
            if value.is_empty() {
                return None;
            }
            params.push(value.to_string());
        } else if let Some(name) = segment.strip_prefix('*') {
            let _ = name;
            // The catch-all swallows the separator and everything after it,
            // so `/artifact/a/b` captures `/a/b`.
            let rest = parts.collect::<Vec<_>>();
            let joined = format!("/{}", rest.join("/"));
            params.push(joined);
            return Some(params);
        } else if parts.next()? != segment {
            return None;
        }
    }
    // A leftover path element means no route matched.
    if parts.next().is_some() {
        return None;
    }
    Some(params)
}

/// The running service.
pub struct Server {
    listener: Option<TcpListener>,
    service: Arc<Service>,
    addr: String,
    port: String,
}

impl Server {
    /// `Serve`: binds `addr:port` and returns the service.
    ///
    /// An empty `artifact_path` means the server is off, and `None` is
    /// returned without touching a socket — act's `Serve` returns its cancel
    /// function at that point, having registered nothing.
    pub fn start(
        artifact_path: &Path,
        addr: &str,
        port: &str,
    ) -> Result<Option<Self>, ServerError> {
        let Some(service) = Service::on_disk(artifact_path) else {
            return Ok(None);
        };
        let listener = TcpListener::bind(format!("{addr}:{port}"))?;
        Ok(Some(Server {
            listener: Some(listener),
            service: Arc::new(service),
            addr: addr.to_string(),
            port: port.to_string(),
        }))
    }

    /// A server over an arbitrary filesystem, for driving the routes without
    /// a disk. The upstream tests work this way.
    pub fn with_fs(
        base_dir: &Path,
        fs: Arc<dyn ArtifactFs>,
        addr: &str,
        port: &str,
    ) -> Result<Option<Self>, ServerError> {
        let Some(service) = Service::new(base_dir, fs) else {
            return Ok(None);
        };
        let listener = TcpListener::bind(format!("{addr}:{port}"))?;
        Ok(Some(Server {
            listener: Some(listener),
            service: Arc::new(service),
            addr: addr.to_string(),
            port: port.to_string(),
        }))
    }

    /// The address the service listens on, `host:port`.
    pub fn bind_address(&self) -> String {
        self.listener
            .as_ref()
            .and_then(|listener| listener.local_addr().ok())
            .map(|addr| addr.to_string())
            .unwrap_or_else(|| format!("{}:{}", self.addr, self.port))
    }

    /// The port actually bound, which is what a `port` of `"0"` resolves to.
    pub fn actual_port(&self) -> u16 {
        self.listener
            .as_ref()
            .and_then(|listener| listener.local_addr().ok())
            .map(|addr| addr.port())
            .unwrap_or(0)
    }

    /// The artifact directory.
    pub fn base_dir(&self) -> PathBuf {
        PathBuf::from(self.service.base_dir())
    }

    /// The shared request state, for callers that want to drive the routing.
    pub fn service(&self) -> Arc<Service> {
        Arc::clone(&self.service)
    }

    /// Serves requests until the listener is closed.
    pub fn serve(&mut self) {
        let Some(listener) = self.listener.take() else {
            return;
        };
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            let service = Arc::clone(&self.service);
            thread::spawn(move || {
                let _ = http::serve_connection(stream, &*service);
            });
        }
    }
}
