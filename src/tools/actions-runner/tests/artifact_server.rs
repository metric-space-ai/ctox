//! The upstream acceptance tests for `pkg/artifacts`.
//!
//! Nine of act's ten test functions are ported. Every assertion comes from
//! `server_test.go`; `TestArtifactFlow` is the tenth and needs Docker plus the
//! whole runner, so it waits for `container` and `runner`.
//!
//! Two different filesystems appear here on purpose.
//!
//! The seven route tests are ported against [`MapFs`], the Rust translation of
//! the `fstest.MapFS` + `writeMapFS` double act's own tests use, so their
//! assertions hold unchanged. That double is *not* production: its
//! `OpenAppendable` replaces the contents instead of appending, and its names
//! are relative map keys rather than host paths. So the V4 flow and the append
//! behaviour are tested a second time against [`OsFs`], the `readWriteFSImpl`
//! that `Serve` actually builds. A double that only resembles the real thing is
//! not a test of the real thing.
//!
//! One structural difference is unavoidable. Upstream registers `uploads` and
//! `downloads` on separate routers per test, so a request to the other's route
//! is a 404 there and answered here. No upstream assertion depends on it.
//!
//! The path-safety tests and the V4 wire shapes are pinned against Go: the
//! `safeResolve`, `buildSignature`, `artifactNameToID`, expiry-layout and
//! protojson-timestamp tables were all captured by running act's own code.

use std::sync::Arc;
use std::time::{Duration, SystemTime};

use ctox_actions_runner::artifacts::{
    base64_url_encode, build_signature, format_signed_expiry, MapFs, OsFs, Service,
    SIGNED_URL_LIFETIME_SECONDS,
};
use ctox_actions_runner::http::Call;

const BASE: &str = "artifact/server/path";
const HOST: &str = "localhost";

/// One request, the way `httptest` does it: no socket, straight into the
/// router.
fn call(method: &str, target: &str, body: &[u8]) -> Call {
    Call::new(method, target, body).with_host(HOST)
}

/// A service over the test double.
fn map_service(fs: &Arc<MapFs>) -> Service {
    Service::new(std::path::Path::new(BASE), fs.clone()).expect("the path is not empty")
}

/// The body parsed as JSON.
fn json(reply: &ctox_actions_runner::http::Reply) -> serde_json::Value {
    serde_json::from_slice(&reply.body).expect("a JSON body")
}

fn route(service: &Service, call: &Call) -> Result<Option<ctox_actions_runner::http::Reply>, std::io::Error> {
    service.route(call)
}

// server_test.go: TestNewArtifactUploadPrepare
#[test]
fn new_artifact_upload_prepare() {
    let fs = Arc::new(MapFs::new());
    let service = map_service(&fs);

    let reply = route(&service, &call("POST", "/_apis/pipelines/workflows/1/artifacts", b""))
        .expect("no io failure")
        .expect("a route");

    assert_eq!(reply.status, 200);
    assert_eq!(
        json(&reply)["fileContainerResourceUrl"],
        "http://localhost/upload/1"
    );
}

// server_test.go: TestArtifactUploadBlob
#[test]
fn artifact_upload_blob() {
    let fs = Arc::new(MapFs::new());
    let service = map_service(&fs);

    let reply = route(
        &service,
        &call("PUT", "/upload/1?itemPath=some/file", b"content"),
    )
    .expect("no io failure")
    .expect("a route");

    assert_eq!(reply.status, 200);
    assert_eq!(json(&reply)["message"], "success");
    assert_eq!(fs.get("artifact/server/path/1/some/file").as_deref(), Some(&b"content"[..]));
}

// server_test.go: TestFinalizeArtifactUpload
#[test]
fn finalize_artifact_upload() {
    let fs = Arc::new(MapFs::new());
    let service = map_service(&fs);

    let reply = route(
        &service,
        &call("PATCH", "/_apis/pipelines/workflows/1/artifacts", b""),
    )
    .expect("no io failure")
    .expect("a route");

    assert_eq!(reply.status, 200);
    assert_eq!(json(&reply)["message"], "success");
}

// server_test.go: TestListArtifacts
#[test]
fn list_artifacts() {
    let fs = Arc::new(MapFs::new());
    fs.insert("artifact/server/path/1/file.txt", b"");
    let service = map_service(&fs);

    let reply = route(&service, &call("GET", "/_apis/pipelines/workflows/1/artifacts", b""))
        .expect("no io failure")
        .expect("a route");

    assert_eq!(reply.status, 200);
    let body = json(&reply);
    assert_eq!(body["count"], 1);
    assert_eq!(body["value"][0]["name"], "file.txt");
    assert_eq!(
        body["value"][0]["fileContainerResourceUrl"],
        "http://localhost/download/1"
    );
}

// server_test.go: TestListArtifactContainer
#[test]
fn list_artifact_container() {
    let fs = Arc::new(MapFs::new());
    fs.insert("artifact/server/path/1/some/file", b"");
    let service = map_service(&fs);

    let reply = route(&service, &call("GET", "/download/1?itemPath=some/file", b""))
        .expect("no io failure")
        .expect("a route");

    assert_eq!(reply.status, 200);
    let body = json(&reply);
    assert_eq!(body["value"].as_array().expect("an array").len(), 1);
    assert_eq!(body["value"][0]["path"], "some/file");
    assert_eq!(body["value"][0]["itemType"], "file");
    // The trailing `/.` is an upstream artefact: the walk is rooted at the
    // file itself, so `filepath.Rel` returns "." and act interpolates it.
    assert_eq!(
        body["value"][0]["contentLocation"],
        "http://localhost/artifact/1/some/file/."
    );
}

// server_test.go: TestDownloadArtifactFile
#[test]
fn download_artifact_file() {
    let fs = Arc::new(MapFs::new());
    fs.insert("artifact/server/path/1/some/file", b"content");
    let service = map_service(&fs);

    let reply = route(&service, &call("GET", "/artifact/1/some/file", b""))
        .expect("no io failure")
        .expect("a route");

    assert_eq!(reply.status, 200);
    assert_eq!(reply.body, b"content");
}

// server_test.go: TestDownloadArtifactFileUnsafePath
#[test]
fn download_artifact_file_unsafe_path() {
    let fs = Arc::new(MapFs::new());
    fs.insert("artifact/server/path/some/file", b"content");
    let service = map_service(&fs);

    let reply = route(&service, &call("GET", "/artifact/2/../../some/file", b""))
        .expect("no io failure")
        .expect("a route");

    assert_eq!(reply.status, 200);
    assert_eq!(reply.body, b"content");
}

// server_test.go: TestArtifactUploadBlobUnsafePath
#[test]
fn artifact_upload_blob_unsafe_path() {
    let fs = Arc::new(MapFs::new());
    let service = map_service(&fs);

    let reply = route(
        &service,
        &call("PUT", "/upload/1?itemPath=../../some/file", b"content"),
    )
    .expect("no io failure")
    .expect("a route");

    assert_eq!(reply.status, 200);
    assert_eq!(json(&reply)["message"], "success");
    assert_eq!(fs.get("artifact/server/path/1/some/file").as_deref(), Some(&b"content"[..]));
}

// server_test.go: TestMkdirFsImplSafeResolve
//
// The whole table also lives in `artifacts::path` as unit tests next to the
// implementation; it is repeated here because it is one of the nine upstream
// functions and belongs with them.
#[test]
fn safe_resolve_table() {
    use ctox_actions_runner::artifacts::safe_resolve;
    if !cfg!(windows) {
        let table = [
            ("baz", "/foo/bar/baz"),
            ("baz/blue", "/foo/bar/baz/blue"),
            ("baz/../../blue", "/foo/bar/blue"),
            ("../../parent", "/foo/bar/parent"),
            ("/root", "/foo/bar/root"),
            ("/", "/foo/bar"),
            ("", "/foo/bar"),
        ];
        for (input, want) in table {
            assert_eq!(safe_resolve("/foo/bar", input), want, "safeResolve({input:?})");
        }
    } else {
        // Go's own test table is Unix-shaped; on Windows the same inputs
        // resolve with the native separator. The unit tests cover the
        // separator-independent form of every case.
        assert!(safe_resolve("\\foo\\bar", "baz").ends_with("baz"));
        assert!(safe_resolve("\\foo\\bar", "../../parent").contains("foo"));
    }
}

// -- The parts upstream cannot test without Docker -----------------------

/// `TestArtifactFlow`, in the only form that is possible before `container`
/// and `runner` exist: the V4 protocol driven by hand over a **real**
/// directory, which is the filesystem `Serve` builds.
///
/// Create, upload in two blocks, finalize, list, sign, download, delete.
#[test]
fn the_v4_flow_works_over_a_real_directory() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let service = Service::new(dir.path(), Arc::new(OsFs)).expect("the path is not empty");

    let create = call(
        "POST",
        "/twirp/github.actions.results.api.v1.ArtifactService/CreateArtifact",
        br#"{"workflow_run_backend_id":"21","workflow_job_run_backend_id":"49","name":"test","version":4}"#,
    );
    let created = route(&service, &create)
        .expect("no io failure")
        .expect("a route");
    assert_eq!(created.status, 200);
    assert_eq!(created.content_type, "application/json;charset=utf-8");
    let body = json(&created);
    assert_eq!(body["ok"], true);
    let signed_upload = body["signedUploadUrl"]
        .as_str()
        .expect("a signed url")
        .to_string();

    // The reservation created the directory and an empty archive inside it.
    let archive = dir.path().join("21").join("test").join("test.zip");
    assert!(archive.is_file(), "CreateArtifact reserves the archive");
    assert_eq!(std::fs::read(&archive).expect("readable").len(), 0);

    // Two blocks, the way the client splits a large file.
    for (comp, chunk) in [("block", &b"first"[..]), ("appendBlock", &b"-second"[..])] {
        let reply = route(
            &service,
            &call("PUT", &format!("{signed_upload}&comp={comp}"), chunk),
        )
        .expect("no io failure")
        .expect("a route");
        // `ctx.JSON` sets the status and writes nothing.
        assert_eq!(reply.status, 201);
        assert!(reply.body.is_empty());
    }
    // The real filesystem appends, which the test double does not.
    assert_eq!(
        std::fs::read(&archive).expect("readable"),
        b"first-second",
        "OpenAppendable appends in production",
    );

    // The block-list step. act matches `blocklist` while the client sends
    // `blockList`, so it falls through every case and the handler returns
    // without writing — a bare 200 with no body at all.
    let reply = route(
        &service,
        &call("PUT", &format!("{signed_upload}&comp=blockList"), b""),
    )
    .expect("no io failure")
    .expect("a route");
    assert_eq!(reply.status, 200);
    assert!(reply.body.is_empty());

    let finalize = call(
        "POST",
        "/twirp/github.actions.results.api.v1.ArtifactService/FinalizeArtifact",
        br#"{"workflow_run_backend_id":"21","workflow_job_run_backend_id":"49","name":"test","size":"11","hash":"sha256:00"}"#,
    );
    let finalized = json(&route(&service, &finalize).expect("no io failure").expect("a route"));
    assert_eq!(finalized["ok"], true);
    assert_eq!(finalized["artifactId"], "2949673445");

    let list = call(
        "POST",
        "/twirp/github.actions.results.api.v1.ArtifactService/ListArtifacts",
        br#"{"workflow_run_backend_id":"21","workflow_job_run_backend_id":"49"}"#,
    );
    let listed = json(&route(&service, &list).expect("no io failure").expect("a route"));
    let artifacts = listed["artifacts"].as_array().expect("an array");
    assert_eq!(artifacts.len(), 1);
    assert_eq!(artifacts[0]["name"], "test");
    assert_eq!(artifacts[0]["databaseId"], "2949673445");
    assert_eq!(artifacts[0]["workflowRunBackendId"], "21");
    assert_eq!(artifacts[0]["workflowJobRunBackendId"], "49");
    // int64 fields are JSON strings.
    assert!(artifacts[0]["size"].is_string());
    assert!(artifacts[0]["createdAt"].is_string());

    // The name filter narrows the list.
    let filtered = call(
        "POST",
        "/twirp/github.actions.results.api.v1.ArtifactService/ListArtifacts",
        br#"{"workflow_run_backend_id":"21","workflow_job_run_backend_id":"49","name_filter":"other"}"#,
    );
    let listed = json(&route(&service, &filtered).expect("no io failure").expect("a route"));
    assert_eq!(listed["artifacts"].as_array().expect("an array").len(), 0);

    let sign = call(
        "POST",
        "/twirp/github.actions.results.api.v1.ArtifactService/GetSignedArtifactURL",
        br#"{"workflow_run_backend_id":"21","workflow_job_run_backend_id":"49","name":"test"}"#,
    );
    let signed = json(&route(&service, &sign).expect("no io failure").expect("a route"));
    let download_url = signed["signedUrl"].as_str().expect("a signed url").to_string();

    let downloaded = route(&service, &call("GET", &download_url, b""))
        .expect("no io failure")
        .expect("a route");
    assert_eq!(downloaded.status, 200);
    assert_eq!(downloaded.body, b"first-second");

    let delete = call(
        "POST",
        "/twirp/github.actions.results.api.v1.ArtifactService/DeleteArtifact",
        br#"{"workflow_run_backend_id":"21","workflow_job_run_backend_id":"49","name":"test"}"#,
    );
    let deleted = json(&route(&service, &delete).expect("no io failure").expect("a route"));
    assert_eq!(deleted["ok"], true);
    assert_eq!(deleted["artifactId"], "2949673445");
    assert!(!archive.exists(), "DeleteArtifact removes the whole directory");
}

/// The signed URL is only as good as its HMAC, and the check happens before
/// the expiry check — so an edited query is a 401 either way.
#[test]
fn a_tampered_signed_url_is_rejected() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let service = Service::new(dir.path(), Arc::new(OsFs)).expect("the path is not empty");

    let create = call(
        "POST",
        "/twirp/github.actions.results.api.v1.ArtifactService/CreateArtifact",
        br#"{"workflow_run_backend_id":"21","workflow_job_run_backend_id":"49","name":"test"}"#,
    );
    let signed_upload = json(&route(&service, &create).expect("no io failure").expect("a route"))
        ["signedUploadUrl"]
        .as_str()
        .expect("a signed url")
        .to_string();

    // Each of these edits a query field that is inside the HMAC, leaving the
    // path alone so the request still reaches the handler.
    let edits = [
        signed_upload.replace("artifactName=test", "artifactName=evil"),
        signed_upload.replace("taskID=21", "taskID=22"),
        // Re-signing nothing and simply dropping the signature is the same
        // failure: `hmac.Equal` rejects a length mismatch.
        {
            let index = signed_upload.find("sig=").expect("a sig") + "sig=".len();
            let end = signed_upload[index..].find('&').expect("a following field") + index;
            format!("{}&{}", &signed_upload[..index], &signed_upload[end + 1..])
        },
    ];
    for edited in &edits {
        assert_ne!(edited, &signed_upload, "the edit changed nothing");
        let reply = route(&service, &call("PUT", edited, b"x"))
            .expect("no io failure")
            .expect("a route");
        assert_eq!(reply.status, 401);
        assert!(reply.body.is_empty(), "a 401 carries no body");
    }

    // Swapping the endpoint in the path is a routing change rather than a
    // signature one: the upload route is a PUT, so the download route is a
    // 405 before any signature is looked at.
    let swapped = signed_upload.replace("UploadArtifact", "DownloadArtifact");
    assert_ne!(swapped, signed_upload);
    let reply = route(&service, &call("PUT", &swapped, b"x"))
        .expect("no io failure")
        .expect("a route");
    assert_eq!(reply.status, 405);

    // An expired link is rejected too. Because `expires` is inside the HMAC,
    // an expiry cannot be edited into place — it has to be *signed*, which is
    // what this does: a correctly signed URL whose expiry is an hour old.
    let expired_expiry =
        format_signed_expiry(SystemTime::now() - Duration::from_secs(SIGNED_URL_LIFETIME_SECONDS as u64));
    let signature = base64_url_encode(&build_signature(
        "UploadArtifact",
        &expired_expiry,
        "test",
        21,
    ));
    let expired = format!(
        "http://{HOST}{}/UploadArtifact?sig={signature}&expires={}&artifactName=test&taskID=21",
        "/twirp/github.actions.results.api.v1.ArtifactService",
        ctox_actions_runner::http::percent_encode(&expired_expiry),
    );
    let reply = route(&service, &call("PUT", &expired, b"x"))
        .expect("no io failure")
        .expect("a route");
    assert_eq!(reply.status, 401, "a correctly signed but stale link");
    assert!(reply.body.is_empty());
}

/// A run id that is not a number is a 400 with no body, and a body that is not
/// protojson is a 500 with no body. Both are `ctx.Error`, which writes the
/// status and nothing else.
#[test]
fn the_error_responses_carry_no_body() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let service = Service::new(dir.path(), Arc::new(OsFs)).expect("the path is not empty");

    let bad_run_id = call(
        "POST",
        "/twirp/github.actions.results.api.v1.ArtifactService/ListArtifacts",
        br#"{"workflow_run_backend_id":"not-a-number"}"#,
    );
    let reply = route(&service, &bad_run_id)
        .expect("no io failure")
        .expect("a route");
    assert_eq!(reply.status, 400);
    assert!(reply.body.is_empty());

    let bad_body = call(
        "POST",
        "/twirp/github.actions.results.api.v1.ArtifactService/ListArtifacts",
        b"{not json",
    );
    let reply = route(&service, &bad_body)
        .expect("no io failure")
        .expect("a route");
    assert_eq!(reply.status, 500);
    assert!(reply.body.is_empty());
}

/// protojson accepts the camelCase `json_name` and the original snake_case
/// field name alike, and an `int64` may arrive quoted or bare.
#[test]
fn protojson_accepts_both_field_spellings() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let service = Service::new(dir.path(), Arc::new(OsFs)).expect("the path is not empty");

    let snake = call(
        "POST",
        "/twirp/github.actions.results.api.v1.ArtifactService/CreateArtifact",
        br#"{"workflow_run_backend_id":"21","workflow_job_run_backend_id":"49","name":"test"}"#,
    );
    let first = route(&service, &snake).expect("no io failure").expect("a route");
    assert_eq!(first.status, 200);

    let camel = call(
        "POST",
        "/twirp/github.actions.results.api.v1.ArtifactService/CreateArtifact",
        br#"{"workflowRunBackendId":"21","workflowJobRunBackendId":"49","name":"other","version":4}"#,
    );
    let second = route(&service, &camel).expect("no io failure").expect("a route");
    assert_eq!(second.status, 200);

    // Both artifacts exist under the same run.
    let list = call(
        "POST",
        "/twirp/github.actions.results.api.v1.ArtifactService/ListArtifacts",
        br#"{"workflow_run_backend_id":"21","workflow_job_run_backend_id":"49"}"#,
    );
    let listed = json(&route(&service, &list).expect("no io failure").expect("a route"));
    let names: Vec<&str> = listed["artifacts"]
        .as_array()
        .expect("an array")
        .iter()
        .map(|entry| entry["name"].as_str().expect("a name"))
        .collect();
    assert_eq!(names, ["other", "test"], "listed in sorted order");
}

/// V3 uploads are stored gzip-encoded under a marker suffix and served back
/// with the encoding header, which is why `Reply` carries headers.
#[test]
fn a_gzip_encoded_upload_is_served_with_its_encoding() {
    let fs = Arc::new(MapFs::new());
    let service = map_service(&fs);

    let reply = route(
        &service,
        &call(
            "PUT",
            "/upload/1?itemPath=some/file",
            b"gzipped bytes",
        )
        .with_header("Content-Encoding", "gzip"),
    )
    .expect("no io failure")
    .expect("a route");
    assert_eq!(reply.status, 200);
    assert_eq!(
        fs.get("artifact/server/path/1/some/file.gz__").as_deref(),
        Some(&b"gzipped bytes"[..]),
    );

    let served = route(&service, &call("GET", "/artifact/1/some/file", b""))
        .expect("no io failure")
        .expect("a route");
    assert_eq!(served.status, 200);
    assert_eq!(served.body, b"gzipped bytes");
    assert_eq!(served.header("Content-Encoding"), Some("gzip"));
}

/// An empty run answers `{"count":0,"value":null}`, not an empty array: Go
/// marshals a nil slice as `null`, and act's list starts nil.
///
/// This one needs a real directory, because the map double synthesises its
/// directories from the files inside them and so cannot hold an empty one.
#[test]
fn an_empty_run_reports_a_null_list() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    std::fs::create_dir_all(dir.path().join("1")).expect("an empty run directory");
    let service = Service::new(dir.path(), Arc::new(OsFs)).expect("the path is not empty");

    let reply = route(&service, &call("GET", "/_apis/pipelines/workflows/1/artifacts", b""))
        .expect("no io failure")
        .expect("a route");
    assert_eq!(reply.status, 200);
    assert_eq!(String::from_utf8_lossy(&reply.body), r#"{"count":0,"value":null}"#);
}

/// A run that does not exist is a `panic` upstream — `fs.ReadDir` on a missing
/// directory — and `net/http` answers a panicking handler by closing the
/// connection without writing anything. That is an error here too, not a 404
/// and not a 500.
#[test]
fn a_missing_run_directory_fails_the_request() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let service = Service::new(dir.path(), Arc::new(OsFs)).expect("the path is not empty");

    let error = route(&service, &call("GET", "/_apis/pipelines/workflows/9/artifacts", b""))
        .expect_err("the directory is not there");
    assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
}

/// An unknown path is a bare 404 and a known path with the wrong method is a
/// 405, which is what `httprouter` does with its defaults.
#[test]
fn unknown_paths_are_404_and_wrong_methods_are_405() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let service = Service::new(dir.path(), Arc::new(OsFs)).expect("the path is not empty");

    // An unknown path is claimed by no route at all, which the server turns
    // into a bare 404.
    let unknown = route(&service, &call("GET", "/nothing/here", b"")).expect("no io failure");
    assert!(unknown.is_none(), "nothing claimed {unknown:?}");

    let wrong_method = route(&service, &call("DELETE", "/upload/1", b""))
        .expect("no io failure")
        .expect("a reply");
    assert_eq!(wrong_method.status, 405);

    let v4_wrong_method = route(
        &service,
        &call("GET", "/twirp/github.actions.results.api.v1.ArtifactService/ListArtifacts", b""),
    )
    .expect("no io failure")
    .expect("a reply");
    assert_eq!(v4_wrong_method.status, 405);
}

/// An empty artifact path turns the server off, without binding a socket.
#[test]
fn an_empty_path_disables_the_server() {
    assert!(Service::new(std::path::Path::new(""), Arc::new(OsFs)).is_none());
}

/// The routes over a real listener and a real client, which is the only shape
/// that exercises the transport: absolute-form targets from act's own signed
/// URLs, `Connection: close`, and the no-response-on-a-panic case.
///
/// The upstream tests stop at the router, so this is the port's own addition.
#[test]
fn the_server_answers_over_a_socket() {
    use std::io::{Read, Write};
    use std::net::TcpStream;

    let dir = tempfile::tempdir().expect("a temporary directory");
    let mut server =
        ctox_actions_runner::artifacts::Server::start(dir.path(), "127.0.0.1", "0")
            .expect("a bindable address")
            .expect("the path is not empty");
    let port = server.actual_port();
    std::thread::spawn(move || server.serve());

    fn request(port: u16, raw: &str) -> Option<(u16, String, Vec<u8>)> {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connected");
        stream.write_all(raw.as_bytes()).expect("written");
        let mut buffer = Vec::new();
        stream.read_to_end(&mut buffer).expect("readable");
        let text = String::from_utf8_lossy(&buffer).into_owned();
        let (head, body) = text.split_once("\r\n\r\n")?;
        let status: u16 = head.split_whitespace().nth(1)?.parse().ok()?;
        Some((status, head.to_string(), body.as_bytes().to_vec()))
    }

    let body = r#"{"workflow_run_backend_id":"21","workflow_job_run_backend_id":"49","name":"test"}"#;
    let (status, head, response_body) = request(
        port,
        &format!(
            "POST /twirp/github.actions.results.api.v1.ArtifactService/CreateArtifact HTTP/1.1\r\n\
             Host: 127.0.0.1:{port}\r\n\
             Content-Length: {}\r\n\
             Connection: close\r\n\r\n{body}",
            body.len(),
        ),
    )
    .expect("a response");
    assert_eq!(status, 200);
    assert!(
        head.contains("Content-Type: application/json;charset=utf-8"),
        "protojson content type, got: {head}",
    );
    assert!(head.contains("Connection: close"));

    // The signed URL is absolute, and the server has to resolve it the way
    // `net/http` does.
    let signed = serde_json::from_slice::<serde_json::Value>(&response_body)
        .expect("a JSON body")["signedUploadUrl"]
        .as_str()
        .expect("a signed url")
        .to_string();
    assert!(signed.starts_with("http://127.0.0.1:"), "{signed}");

    let upload_target = signed
        .trim_start_matches("http://127.0.0.1:")
        .split_once('/')
        .expect("a path")
        .1
        .to_string();
    let (status, _, _) = request(
        port,
        &format!(
            "PUT /{upload_target}&comp=block HTTP/1.1\r\n\
             Host: 127.0.0.1:{port}\r\n\
             Content-Length: 5\r\n\
             Connection: close\r\n\r\nhello",
        ),
    )
    .expect("a response");
    assert_eq!(status, 201);

    // A missing artifact: act panics, `net/http` closes the connection with
    // nothing written, so the client sees an empty read rather than a status.
    let missing = request(
        port,
        &format!(
            "GET /_apis/pipelines/workflows/9/artifacts HTTP/1.1\r\n\
             Host: 127.0.0.1:{port}\r\n\
             Connection: close\r\n\r\n",
        ),
    );
    assert!(missing.is_none(), "a panicking handler writes nothing");
}
