//! Dropbox port: one recorded case per action, expectations derived from the
//! Activepieces `run()` code. Every Dropbox call is a POST with the arguments
//! in the JSON body (`api.` host) or in the `Dropbox-API-Arg` header plus a raw
//! byte body (`content.` host).
mod support;

use ctox_toolhub::http::HttpResponse;
use ctox_toolhub::toolkits::dropbox::Dropbox;
use ctox_toolhub::toolkits::ToolkitAdapter;
use serde_json::{json, Value};
use support::{live, run, Case};

const API: &str = "https://api.dropbox.test/2";
const CONTENT: &str = "https://content.dropbox.test/2";

const FILE: &str = "/reports/q4-summary.pdf";
const FILE_COPY: &str = "/archive/q4-summary.pdf";
const FOLDER: &str = "/reports";
const REV: &str = "5f9a1c2d3e4f";
const JOB: &str = "dbjid:AAAbyC8pB-CAAAAAAAAV_g";
const LINK: &str =
    "https://uc7f81d95df6ec7ea9b1a.dl.dropboxusercontent.com/get/reports/q4-summary.pdf?_dl=1";
const UPLOAD_LINK: &str = "https://content.dropboxapi.com/apitul/2/b1c2d3e4f5a6";
const COPY_REF: &str = "z1X6ATl6aWtzOGq0c3g5Ng";

fn adapter() -> Dropbox {
    Dropbox::with_api_base("https://api.dropbox.test")
}

/// `&[(ptr, value)]` for a `Case` field: leaked so it lives as long as the case.
fn ptrs<const N: usize>(pairs: [(&'static str, Value); N]) -> &'static [(&'static str, Value)] {
    Box::leak(Box::new(pairs))
}

/// Expected URL substring for a `Case`, leaked so it lives as long as the case.
fn u(url: String) -> &'static [&'static str] {
    let leaked: &'static str = Box::leak(url.into_boxed_str());
    Box::leak(Box::new([leaked]))
}

fn base_name(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_string()
}

/// Recorded file metadata, exactly the fields Dropbox returns.
fn file_meta(path: &str) -> Value {
    json!({
        ".tag": "file",
        "name": base_name(path),
        "path_lower": path.to_ascii_lowercase(),
        "path_display": path,
        "id": "id:9XyZAbCdEfGhIjKlMnOp",
        "client_modified": "2026-09-28T10:12:44Z",
        "server_modified": "2026-09-28T10:12:44Z",
        "rev": REV,
        "size": 482113,
        "content_hash":
            "db1d0f1d2f3e4a5b6c7d8e9f0a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b",
        "is_downloadable": true,
    })
}

fn folder_meta(path: &str) -> Value {
    json!({
        ".tag": "folder",
        "name": base_name(path),
        "path_lower": path.to_ascii_lowercase(),
        "path_display": path,
        "id": "id:QrStUvWxYzAbCdEfGhIj",
    })
}

/// `{".tag": "success", "metadata": …}` batch result entry.
fn success(path: &str) -> Value {
    json!({".tag": "success", "metadata": file_meta(path)})
}

/// Batch result entry for a deleted/copied folder.
fn folder_success(path: &str) -> Value {
    json!({".tag": "success", "metadata": folder_meta(path)})
}

fn lock_entry(holder: &str) -> Value {
    json!({"entries": [{
        ".tag": "success",
        "lock": {
            ".tag": "lock",
            "lockholder_path": FILE,
            "lockholder_display_name": holder,
            "created": "2026-09-30T09:00:00Z",
        }
    }]})
}

#[test]
fn dropbox_actions_are_ported() {
    let a = adapter();
    let cases: Vec<Case> = vec![
        Case {
            tool: "DROPBOX_SEARCH_FILES",
            args: json!({"query": "q4 summary", "path": "/reports", "max_results": 10,
                         "file_extensions": "pdf,docx", "filename_only": true}),
            method: "POST",
            url_contains: u(format!("{API}/files/search_v2")),
            body_eq: ptrs([
                ("/query", json!("q4 summary")),
                ("/options/path", json!("/reports")),
                ("/options/max_results", json!(10)),
                ("/options/file_status", json!("active")),
                ("/options/filename_only", json!(true)),
                ("/options/file_extensions", json!(["pdf", "docx"])),
            ]),
            response_status: 200,
            response: json!({
                "matches": [{"match_type": {".tag": "filename"},
                             "metadata": {".tag": "metadata",
                                          "metadata": file_meta(FILE)}}],
                "has_more": false,
                "cursor": "AAEOp9xQ2Z1c3g5Ng",
            }),
            result_eq: ptrs([
                ("/count", json!(1)),
                ("/items/0/name", json!("q4-summary.pdf")),
                ("/items/0/path_display", json!(FILE)),
                ("/has_more", json!(false)),
                ("/cursor", json!("AAEOp9xQ2Z1c3g5Ng")),
            ]),
        },
        Case {
            tool: "DROPBOX_LIST_FOLDER",
            args: json!({"path": "", "limit": 2}),
            method: "POST",
            url_contains: u(format!("{API}/files/list_folder")),
            body_eq: ptrs([
                ("/path", json!("")),
                ("/recursive", json!(false)),
                ("/limit", json!(2)),
            ]),
            response_status: 200,
            response: json!({
                "entries": [folder_meta(FOLDER), file_meta(FILE)],
                "cursor": "lSt9mLnOoPqRsTuV",
                "has_more": true,
            }),
            result_eq: ptrs([
                ("/count", json!(2)),
                ("/items/0/kind", json!("folder")),
                ("/items/0/name", json!("reports")),
                ("/items/1/size", json!(482113)),
                ("/has_more", json!(true)),
                ("/cursor", json!("lSt9mLnOoPqRsTuV")),
            ]),
        },
        Case {
            tool: "DROPBOX_GET_FILE_METADATA",
            args: json!({"path": FILE, "include_deleted": true, "include_media_info": true}),
            method: "POST",
            url_contains: u(format!("{API}/files/get_metadata")),
            body_eq: ptrs([
                ("/path", json!(FILE)),
                ("/include_deleted", json!(true)),
                ("/include_media_info", json!(true)),
                ("/include_has_explicit_shared_members", json!(false)),
            ]),
            response_status: 200,
            response: file_meta(FILE),
            result_eq: ptrs([
                ("/kind", json!("file")),
                ("/name", json!("q4-summary.pdf")),
                ("/rev", json!(REV)),
                ("/size", json!(482113)),
                ("/is_downloadable", json!(true)),
            ]),
        },
        Case {
            tool: "DROPBOX_LIST_FILE_REVISIONS",
            args: json!({"path": FILE, "mode": "id", "limit": 5}),
            method: "POST",
            url_contains: u(format!("{API}/files/list_revisions")),
            body_eq: ptrs([
                ("/path", json!(FILE)),
                ("/mode", json!("id")),
                ("/limit", json!(5)),
            ]),
            response_status: 200,
            response: json!({
                "is_deleted": false,
                "entries": [
                    {".tag": "file", "name": "q4-summary.pdf", "path_lower": FILE,
                     "path_display": FILE, "id": "id:9XyZAbCdEfGhIjKlMnOp",
                     "rev": "a1b2c3d4e5f6", "size": 480001,
                     "server_modified": "2026-09-20T08:00:00Z", "is_downloadable": true},
                    {".tag": "file", "name": "q4-summary.pdf", "path_lower": FILE,
                     "path_display": FILE, "id": "id:9XyZAbCdEfGhIjKlMnOp",
                     "rev": REV, "size": 482113,
                     "server_modified": "2026-09-28T10:12:44Z", "is_downloadable": true},
                ],
                "has_more": false,
                "cursor": "UmV2aXNpb25z",
            }),
            result_eq: ptrs([
                ("/count", json!(2)),
                ("/items/0/rev", json!("a1b2c3d4e5f6")),
                ("/items/1/rev", json!(REV)),
                ("/is_deleted", json!(false)),
            ]),
        },
        Case {
            tool: "DROPBOX_GET_TEMPORARY_LINK",
            args: json!({"path": FILE}),
            method: "POST",
            url_contains: u(format!("{API}/files/get_temporary_link")),
            body_eq: ptrs([("/path", json!(FILE))]),
            response_status: 200,
            response: json!({"metadata": file_meta(FILE), "link": LINK}),
            result_eq: ptrs([
                ("/link", json!(LINK)),
                ("/metadata/name", json!("q4-summary.pdf")),
            ]),
        },
        Case {
            tool: "DROPBOX_GET_FILE_LOCK",
            args: json!({"path": FILE}),
            method: "POST",
            url_contains: u(format!("{API}/files/get_file_lock_batch")),
            body_eq: ptrs([("/entries/0/path", json!(FILE))]),
            response_status: 200,
            response: lock_entry("Ada Lovelace"),
            result_eq: ptrs([
                ("/status", json!("success")),
                ("/lock/lockholder_display_name", json!("Ada Lovelace")),
                ("/lock/lockholder_path", json!(FILE)),
            ]),
        },
        Case {
            tool: "DROPBOX_GET_THUMBNAILS_BATCH",
            args: json!({"paths": ["/img/a.jpg", "/img/b.png"], "format": "png",
                         "size": "w128h128"}),
            method: "POST",
            url_contains: u(format!("{CONTENT}/files/get_thumbnail_batch")),
            body_eq: ptrs([
                ("/entries/0/path", json!("/img/a.jpg")),
                ("/entries/0/format", json!("png")),
                ("/entries/0/size", json!("w128h128")),
                ("/entries/1/path", json!("/img/b.png")),
            ]),
            response_status: 200,
            response: json!({"entries": [
                {".tag": "success", "metadata": file_meta("/img/a.jpg"),
                 "thumbnail": "iVBORw0KGgoAAAANSUhEUg=="},
                {".tag": "success", "metadata": file_meta("/img/b.png"),
                 "thumbnail": "iVBORw0KGgoAAAANSUhEUgAA"},
            ]}),
            result_eq: ptrs([
                ("/count", json!(2)),
                ("/items/0/status", json!("success")),
                ("/items/0/thumbnail_base64", json!("iVBORw0KGgoAAAANSUhEUg==")),
                ("/items/1/metadata/name", json!("b.png")),
            ]),
        },
        Case {
            tool: "DROPBOX_GET_COPY_BATCH_STATUS",
            args: json!({"async_job_id": JOB}),
            method: "POST",
            url_contains: u(format!("{API}/files/copy_batch/check_v2")),
            body_eq: ptrs([("/async_job_id", json!(JOB))]),
            response_status: 200,
            response: json!({".tag": "complete", "entries": [success(FILE_COPY)]}),
            result_eq: ptrs([
                ("/.tag", json!("complete")),
                ("/entries/0/metadata/path_display", json!(FILE_COPY)),
            ]),
        },
        Case {
            tool: "DROPBOX_GET_MOVE_BATCH_STATUS",
            args: json!({"async_job_id": JOB}),
            method: "POST",
            url_contains: u(format!("{API}/files/move_batch/check_v2")),
            body_eq: ptrs([("/async_job_id", json!(JOB))]),
            response_status: 200,
            response: json!({".tag": "in_progress"}),
            result_eq: ptrs([("/.tag", json!("in_progress"))]),
        },
        Case {
            tool: "DROPBOX_GET_DELETE_BATCH_STATUS",
            args: json!({"async_job_id": JOB}),
            method: "POST",
            url_contains: u(format!("{API}/files/delete_batch/check")),
            body_eq: ptrs([("/async_job_id", json!(JOB))]),
            response_status: 200,
            response: json!({".tag": "complete", "entries": [folder_success(FOLDER)]}),
            result_eq: ptrs([
                ("/.tag", json!("complete")),
                ("/entries/0/metadata/.tag", json!("folder")),
            ]),
        },
        Case {
            tool: "DROPBOX_GET_CREATE_FOLDER_BATCH_STATUS",
            args: json!({"async_job_id": JOB}),
            method: "POST",
            url_contains: u(format!("{API}/files/create_folder_batch/check")),
            body_eq: ptrs([("/async_job_id", json!(JOB))]),
            response_status: 200,
            response: json!({".tag": "complete", "entries": [
                {".tag": "success", "metadata": folder_meta("/reports/2026")}
            ]}),
            result_eq: ptrs([
                ("/.tag", json!("complete")),
                ("/entries/0/metadata/path_display", json!("/reports/2026")),
            ]),
        },
        Case {
            tool: "DROPBOX_GET_SAVE_URL_STATUS",
            args: json!({"async_job_id": JOB}),
            method: "POST",
            url_contains: u(format!("{API}/files/save_url/check_job_status")),
            body_eq: ptrs([("/async_job_id", json!(JOB))]),
            response_status: 200,
            response: json!({".tag": "complete", "metadata": file_meta(FILE)}),
            result_eq: ptrs([
                ("/.tag", json!("complete")),
                ("/metadata/name", json!("q4-summary.pdf")),
            ]),
        },
        Case {
            tool: "DROPBOX_CREATE_FOLDER",
            args: json!({"path": "/Hausaufgaben/math", "autorename": true}),
            method: "POST",
            url_contains: u(format!("{API}/files/create_folder_v2")),
            body_eq: ptrs([
                ("/path", json!("/Hausaufgaben/math")),
                ("/autorename", json!(true)),
            ]),
            response_status: 200,
            response: json!({"metadata": folder_meta("/Hausaufgaben/math")}),
            result_eq: ptrs([
                ("/kind", json!("folder")),
                ("/name", json!("math")),
                ("/path_display", json!("/Hausaufgaben/math")),
            ]),
        },
        Case {
            tool: "DROPBOX_CREATE_FOLDER_BATCH",
            args: json!({"paths": ["/reports/2026", "/reports/2027"]}),
            method: "POST",
            url_contains: u(format!("{API}/files/create_folder_batch")),
            body_eq: ptrs([
                ("/paths", json!(["/reports/2026", "/reports/2027"])),
                ("/autorename", json!(false)),
            ]),
            response_status: 200,
            response: json!({".tag": "complete", "entries": [
                {".tag": "success", "metadata": folder_meta("/reports/2026")},
                {".tag": "success", "metadata": folder_meta("/reports/2027")},
            ]}),
            result_eq: ptrs([
                ("/.tag", json!("complete")),
                ("/entries/1/metadata/name", json!("2027")),
            ]),
        },
        Case {
            tool: "DROPBOX_COPY_ENTRY",
            args: json!({"from_path": FILE, "to_path": FILE_COPY, "autorename": true}),
            method: "POST",
            url_contains: u(format!("{API}/files/copy_v2")),
            body_eq: ptrs([
                ("/from_path", json!(FILE)),
                ("/to_path", json!(FILE_COPY)),
                ("/autorename", json!(true)),
            ]),
            response_status: 200,
            response: json!({"metadata": file_meta(FILE_COPY)}),
            result_eq: ptrs([
                ("/name", json!("q4-summary.pdf")),
                ("/path_display", json!(FILE_COPY)),
                ("/rev", json!(REV)),
            ]),
        },
        Case {
            tool: "DROPBOX_COPY_BATCH",
            args: json!({"entries": [
                {"from_path": "/a.txt", "to_path": "/backup/a.txt"},
                {"from_path": "/b.txt", "to_path": "/backup/b.txt"}
            ], "autorename": true}),
            method: "POST",
            url_contains: u(format!("{API}/files/copy_batch_v2")),
            body_eq: ptrs([
                ("/entries/0/from_path", json!("/a.txt")),
                ("/entries/0/to_path", json!("/backup/a.txt")),
                ("/entries/1/from_path", json!("/b.txt")),
                ("/entries/1/to_path", json!("/backup/b.txt")),
                ("/autorename", json!(true)),
            ]),
            response_status: 200,
            response: json!({".tag": "async_job_id", "async_job_id": JOB}),
            result_eq: ptrs([
                ("/.tag", json!("async_job_id")),
                ("/async_job_id", json!(JOB)),
            ]),
        },
        Case {
            tool: "DROPBOX_MOVE_ENTRY",
            args: json!({"from_path": FILE, "to_path": "/reports/2026/q4-summary.pdf"}),
            method: "POST",
            url_contains: u(format!("{API}/files/move_v2")),
            body_eq: ptrs([
                ("/from_path", json!(FILE)),
                ("/to_path", json!("/reports/2026/q4-summary.pdf")),
                ("/autorename", json!(false)),
            ]),
            response_status: 200,
            response: json!({"metadata": file_meta("/reports/2026/q4-summary.pdf")}),
            result_eq: ptrs([
                ("/name", json!("q4-summary.pdf")),
                ("/path_display", json!("/reports/2026/q4-summary.pdf")),
            ]),
        },
        Case {
            tool: "DROPBOX_MOVE_BATCH",
            args: json!({"entries": [
                {"from_path": "/a.txt", "to_path": "/archiv/a.txt"},
                {"from_path": "/b.txt", "to_path": "/archiv/b.txt"}
            ]}),
            method: "POST",
            url_contains: u(format!("{API}/files/move_batch_v2")),
            body_eq: ptrs([
                ("/entries/0/from_path", json!("/a.txt")),
                ("/entries/1/to_path", json!("/archiv/b.txt")),
                ("/autorename", json!(false)),
            ]),
            response_status: 200,
            response: json!({".tag": "async_job_id", "async_job_id": JOB}),
            result_eq: ptrs([("/async_job_id", json!(JOB))]),
        },
        Case {
            tool: "DROPBOX_DELETE_ENTRY",
            args: json!({"path": FOLDER}),
            method: "POST",
            url_contains: u(format!("{API}/files/delete_v2")),
            body_eq: ptrs([("/path", json!(FOLDER))]),
            response_status: 200,
            response: json!({"metadata": folder_meta(FOLDER)}),
            result_eq: ptrs([
                ("/kind", json!("folder")),
                ("/name", json!("reports")),
                ("/path_display", json!(FOLDER)),
            ]),
        },
        Case {
            tool: "DROPBOX_DELETE_BATCH",
            args: json!({"paths": [FILE, FOLDER]}),
            method: "POST",
            url_contains: u(format!("{API}/files/delete_batch")),
            body_eq: ptrs([
                ("/entries/0/path", json!(FILE)),
                ("/entries/1/path", json!(FOLDER)),
            ]),
            response_status: 200,
            response: json!({".tag": "complete", "entries": [
                folder_success(FOLDER), success(FILE)
            ]}),
            result_eq: ptrs([
                ("/.tag", json!("complete")),
                ("/entries/0/metadata/.tag", json!("folder")),
                ("/entries/1/metadata/name", json!("q4-summary.pdf")),
            ]),
        },
        Case {
            tool: "DROPBOX_RESTORE_FILE_REVISION",
            args: json!({"path": FILE, "rev": "a1b2c3d4e5f6"}),
            method: "POST",
            url_contains: u(format!("{API}/files/restore")),
            body_eq: ptrs([
                ("/path", json!(FILE)),
                ("/rev", json!("a1b2c3d4e5f6")),
            ]),
            response_status: 200,
            response: file_meta(FILE),
            result_eq: ptrs([
                ("/name", json!("q4-summary.pdf")),
                ("/rev", json!(REV)),
                ("/size", json!(482113)),
            ]),
        },
        Case {
            tool: "DROPBOX_CREATE_COPY_REFERENCE",
            args: json!({"path": FOLDER}),
            method: "POST",
            url_contains: u(format!("{API}/files/copy_reference/get")),
            body_eq: ptrs([("/path", json!(FOLDER))]),
            response_status: 200,
            response: json!({
                "metadata": folder_meta(FOLDER),
                "copy_reference": COPY_REF,
                "expires": "2045-05-12T15:50:38Z",
            }),
            result_eq: ptrs([
                ("/copy_reference", json!(COPY_REF)),
                ("/expires", json!("2045-05-12T15:50:38Z")),
                ("/metadata/name", json!("reports")),
            ]),
        },
        Case {
            tool: "DROPBOX_SAVE_COPY_REFERENCE",
            args: json!({"copy_reference": COPY_REF, "path": FILE_COPY}),
            method: "POST",
            url_contains: u(format!("{API}/files/copy_reference/save")),
            body_eq: ptrs([
                ("/copy_reference", json!(COPY_REF)),
                ("/path", json!(FILE_COPY)),
            ]),
            response_status: 200,
            response: json!({"metadata": file_meta(FILE_COPY)}),
            result_eq: ptrs([
                ("/name", json!("q4-summary.pdf")),
                ("/path_display", json!(FILE_COPY)),
            ]),
        },
        Case {
            tool: "DROPBOX_LOCK_FILE",
            args: json!({"path": FILE}),
            method: "POST",
            url_contains: u(format!("{API}/files/lock_file_batch")),
            body_eq: ptrs([("/entries/0/path", json!(FILE))]),
            response_status: 200,
            response: lock_entry("Ada Lovelace"),
            result_eq: ptrs([
                ("/status", json!("success")),
                ("/lock/lockholder_path", json!(FILE)),
            ]),
        },
        Case {
            tool: "DROPBOX_UNLOCK_FILE",
            args: json!({"path": FILE}),
            method: "POST",
            url_contains: u(format!("{API}/files/unlock_file_batch")),
            body_eq: ptrs([("/entries/0/path", json!(FILE))]),
            response_status: 200,
            response: json!({"entries": [{".tag": "success"}]}),
            result_eq: ptrs([
                ("/status", json!("success")),
                ("/lock", Value::Null),
                ("/failure", Value::Null),
            ]),
        },
        Case {
            tool: "DROPBOX_SAVE_URL",
            args: json!({"path": "/Von-URLs/whitepaper.pdf",
                         "url": "https://cdn.example.org/whitepaper.pdf"}),
            method: "POST",
            url_contains: u(format!("{API}/files/save_url")),
            body_eq: ptrs([
                ("/path", json!("/Von-URLs/whitepaper.pdf")),
                ("/url", json!("https://cdn.example.org/whitepaper.pdf")),
            ]),
            response_status: 200,
            response: json!({".tag": "async_job_id", "async_job_id": JOB}),
            result_eq: ptrs([
                ("/.tag", json!("async_job_id")),
                ("/async_job_id", json!(JOB)),
            ]),
        },
        Case {
            tool: "DROPBOX_CREATE_UPLOAD_LINK",
            args: json!({"path": "/Inbox/fax.pdf", "mode": "overwrite", "duration": 3600,
                         "autorename": true}),
            method: "POST",
            url_contains: u(format!("{API}/files/get_temporary_upload_link")),
            body_eq: ptrs([
                ("/commit_info/path", json!("/Inbox/fax.pdf")),
                ("/commit_info/mode", json!("overwrite")),
                ("/commit_info/autorename", json!(true)),
                ("/duration", json!(3600)),
            ]),
            response_status: 200,
            response: json!({"link": UPLOAD_LINK, "expires": "2026-10-14T18:00:00Z"}),
            result_eq: ptrs([
                ("/link", json!(UPLOAD_LINK)),
                ("/expires", json!("2026-10-14T18:00:00Z")),
            ]),
        },
        Case {
            // The text upload carries its arguments in the Dropbox-API-Arg
            // header and the UTF-8 text as the raw body; checked below.
            tool: "DROPBOX_CREATE_TEXT_FILE",
            args: json!({"path": "/Notizen/heute.md", "text": "# Einkauf\n- Milch\n",
                         "mute": true}),
            method: "POST",
            url_contains: u(format!("{CONTENT}/files/upload")),
            body_eq: ptrs([]),
            response_status: 200,
            response: file_meta("/Notizen/heute.md"),
            result_eq: ptrs([
                ("/name", json!("heute.md")),
                ("/path_display", json!("/Notizen/heute.md")),
                ("/rev", json!(REV)),
            ]),
        },
    ];
    support::assert_full_coverage(&a, &cases);
    for c in &cases {
        run(&a, c);
    }
}

/// The text upload goes to the content host with the commit info in the
/// `Dropbox-API-Arg` header and the text as the raw request body.
#[test]
fn text_file_uses_the_content_host() {
    let a = adapter();
    let req = a
        .build_request(
            "DROPBOX_CREATE_TEXT_FILE",
            &json!({"path": "/Notizen/heute.md", "text": "# Einkauf\n- Milch\n", "mute": true}),
        )
        .unwrap();
    assert_eq!(req.method, "POST");
    assert_eq!(req.url, format!("{CONTENT}/files/upload"));
    assert_eq!(req.headers["content-type"], "application/octet-stream");
    assert_eq!(
        req.headers["dropbox-api-arg"],
        r#"{"autorename":false,"mode":"add","mute":true,"path":"/Notizen/heute.md","strict_conflict":false}"#
    );
    assert_eq!(req.body.as_deref(), Some("# Einkauf\n- Milch\n".as_bytes()));
}

/// A failed single-entry batch result becomes a tool error (unwrapEntry).
#[test]
fn failed_lock_entry_is_an_error() {
    let a = adapter();
    let resp = HttpResponse {
        status: 200,
        body: serde_json::to_vec(&json!({"entries": [{
            ".tag": "failure", "failure": {".tag": "file_locks_not_available"}
        }]}))
        .unwrap(),
        session_id: None,
    };
    let err = a.parse_response("DROPBOX_LOCK_FILE", &resp).unwrap_err();
    assert!(err.to_string().contains("file_locks_not_available"), "{err}");
}

/// Live read-only calls through the hub exec path (bearer access token).
#[test]
#[ignore]
fn live_read_only_actions() {
    let a = adapter();
    let mut path = None;
    if let Some(result) = live(&a, "DROPBOX_LIST_FOLDER", json!({"path": "", "limit": 5})) {
        let listing = result.unwrap_or_else(|e| panic!("DROPBOX_LIST_FOLDER: {e}"));
        path = support::find_str(&listing, "path_display");
    }
    if let Some(result) = live(&a, "DROPBOX_SEARCH_FILES", json!({"query": "test", "max_results": 5})) {
        result.unwrap_or_else(|e| panic!("DROPBOX_SEARCH_FILES: {e}"));
    }
    if let Some(path) = path {
        if let Some(result) = live(&a, "DROPBOX_GET_FILE_METADATA", json!({"path": path})) {
            result.unwrap_or_else(|e| panic!("DROPBOX_GET_FILE_METADATA: {e}"));
        }
    }
}
