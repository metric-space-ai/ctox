//! Recorded port tests for the Google Drive toolkit (one case per tool).
//! Expectations are derived from the Activepieces `run()` code; responses are
//! realistic Drive API v3 payloads.
#[path = "support/owned.rs"]
mod support;

use ctox_toolhub::toolkits::google_drive::GoogleDrive;
use ctox_toolhub::toolkits::ToolkitAdapter;
use serde_json::{json, Value};
use support::Case;

fn cases() -> Vec<Case<'static>> {
    vec![
        // --- read / list / search ---
        Case {
            tool: "GOOGLEDRIVE_SEARCH_FILES",
            args: json!({"query_term": "name", "operator": "contains", "value": "Report",
                "type": "file", "parent_folder_id": "fld1"}),
            method: "GET",
            url_contains: &[
                "/drive/v3/files?",
                "q=name+contains+%27Report%27",
                "and+%27fld1%27+in+parents",
                "and+mimeType%21%3D%27application%2Fvnd.google-apps.folder%27",
                "corpora=user",
                "pageSize=1000",
                "fields=nextPageToken%2C+files",
            ],
            body_eq: vec![],
            response_status: 200,
            response: json!({"kind": "drive#fileList", "incompleteSearch": false, "files": [
                {"id": "1Az9xQ", "name": "Report Q1", "mimeType": "application/pdf",
                 "createdTime": "2026-01-05T10:00:00.000Z", "modifiedTime": "2026-01-06T09:12:00.000Z"}],
                "nextPageToken": null}),
            result_eq: vec![("/files/0/id", json!("1Az9xQ")), ("/files/0/mimeType", json!("application/pdf")),
                ("/next_page_token", Value::Null)],
        },
        Case {
            tool: "GOOGLEDRIVE_GET_FILE",
            args: json!({"file_id": "1Az9xQ"}),
            method: "GET",
            url_contains: &["/drive/v3/files/1Az9xQ?supportsAllDrives=true"],
            body_eq: vec![],
            response_status: 200,
            response: json!({"id": "1Az9xQ", "name": "Report Q1",
                "mimeType": "application/vnd.google-apps.document", "parents": ["fld1"],
                "webViewLink": "https://docs.google.com/document/d/1Az9xQ/edit"}),
            result_eq: vec![("/name", json!("Report Q1")), ("/parents/0", json!("fld1"))],
        },
        Case {
            tool: "GOOGLEDRIVE_LIST_FILES",
            args: json!({"folder_id": "fld1", "include_trashed": false}),
            method: "GET",
            url_contains: &[
                "/drive/v3/files?",
                "q=%27fld1%27+in+parents+and+trashed%3Dfalse",
                "corpora=user",
                "pageSize=1000",
                "fields=nextPageToken%2Cfiles%28id%2Ckind%2CmimeType%2Cname%2Ctrashed%2Cparents%29",
            ],
            body_eq: vec![],
            response_status: 200,
            response: json!({"kind": "drive#fileList", "incompleteSearch": false, "files": [
                {"id": "f1", "kind": "drive#file", "name": "notes.txt", "mimeType": "text/plain",
                 "trashed": false, "parents": ["fld1"]}],
                "nextPageToken": null}),
            result_eq: vec![("/incomplete_search", json!(false)), ("/files/0/name", json!("notes.txt"))],
        },
        Case {
            tool: "GOOGLEDRIVE_GET_ABOUT",
            args: json!({}),
            method: "GET",
            url_contains: &[
                "/drive/v3/about?fields=user%2CstorageQuota%2CmaxImportSizes%2CexportFormats",
            ],
            body_eq: vec![],
            response_status: 200,
            response: json!({"user": {"displayName": "Ada Lovelace", "emailAddress": "ada@example.com",
                "kind": "drive#user"},
                "storageQuota": {"limit": "16106127360", "usage": "1234567", "usageInDrive": "1000000"},
                "maxImportSizes": {},
                "exportFormats": {"application/vnd.google-apps.document": ["application/pdf", "text/plain"]}}),
            result_eq: vec![("/user/emailAddress", json!("ada@example.com")),
                ("/storageQuota/limit", json!("16106127360"))],
        },
        Case {
            tool: "GOOGLEDRIVE_LIST_PERMISSIONS",
            args: json!({"file_id": "fld1"}),
            method: "GET",
            url_contains: &[
                "/drive/v3/files/fld1/permissions?",
                "supportsAllDrives=true",
                "fields=permissions%28id%2Ctype%2Crole%2CemailAddress%2Cdomain%29",
            ],
            body_eq: vec![],
            response_status: 200,
            response: json!({"permissions": [
                {"id": "101", "type": "user", "role": "writer", "emailAddress": "bob@example.com"},
                {"id": "anyoneWithLink", "type": "anyone", "role": "reader"}]}),
            result_eq: vec![("/count", json!(2)), ("/permissions/0/role", json!("writer"))],
        },
        Case {
            tool: "GOOGLEDRIVE_LIST_COMMENTS",
            args: json!({"file_id": "f1"}),
            method: "GET",
            url_contains: &["/drive/v3/files/f1/comments?", "fields=*", "includeDeleted=false"],
            body_eq: vec![],
            response_status: 200,
            response: json!({"comments": [{"id": "c1", "kind": "drive#comment",
                "content": "Please review by Friday",
                "author": {"kind": "drive#user", "displayName": "Ada"}, "replies": []}]}),
            result_eq: vec![("/count", json!(1)), ("/comments/0/content", json!("Please review by Friday"))],
        },
        Case {
            tool: "GOOGLEDRIVE_LIST_REPLIES",
            args: json!({"file_id": "f1", "comment_id": "c1"}),
            method: "GET",
            url_contains: &["/drive/v3/files/f1/comments/c1/replies?", "fields=*"],
            body_eq: vec![],
            response_status: 200,
            response: json!({"replies": [{"id": "r1", "kind": "drive#reply", "content": "Done, see v2",
                "author": {"displayName": "Bob"}}]}),
            result_eq: vec![("/count", json!(1)), ("/replies/0/id", json!("r1"))],
        },
        Case {
            tool: "GOOGLEDRIVE_GET_REPLY",
            args: json!({"file_id": "f1", "comment_id": "c1", "reply_id": "r1"}),
            method: "GET",
            url_contains: &["/drive/v3/files/f1/comments/c1/replies/r1?fields=*"],
            body_eq: vec![],
            response_status: 200,
            response: json!({"id": "r1", "kind": "drive#reply", "content": "Done, see v2",
                "modifiedTime": "2026-02-01T08:00:00.000Z"}),
            result_eq: vec![("/content", json!("Done, see v2"))],
        },
        Case {
            tool: "GOOGLEDRIVE_LIST_SHARED_DRIVES",
            args: json!({"query": "name contains 'marketing'", "page_size": 50}),
            method: "GET",
            url_contains: &["/drive/v3/drives?", "q=name+contains+%27marketing%27", "pageSize=50"],
            body_eq: vec![],
            response_status: 200,
            response: json!({"drives": [{"id": "sd1", "kind": "drive#drive", "name": "Marketing"}]}),
            result_eq: vec![("/count", json!(1)), ("/drives/0/name", json!("Marketing"))],
        },
        Case {
            tool: "GOOGLEDRIVE_GET_SHARED_DRIVE",
            args: json!({"drive_id": "sd1"}),
            method: "GET",
            url_contains: &["/drive/v3/drives/sd1"],
            body_eq: vec![],
            response_status: 200,
            response: json!({"id": "sd1", "name": "Marketing"}),
            result_eq: vec![("/name", json!("Marketing"))],
        },
        // --- create / update / write ---
        Case {
            tool: "GOOGLEDRIVE_CREATE_FOLDER",
            args: json!({"folder_name": "Reports 2026", "parent_folder_id": "fld1"}),
            method: "POST",
            url_contains: &["/drive/v3/files?supportsAllDrives=true"],
            body_eq: vec![
                ("/name", json!("Reports 2026")),
                ("/mimeType", json!("application/vnd.google-apps.folder")),
                ("/parents/0", json!("fld1")),
            ],
            response_status: 200,
            response: json!({"id": "nf1", "name": "Reports 2026",
                "mimeType": "application/vnd.google-apps.folder", "parents": ["fld1"]}),
            result_eq: vec![("/id", json!("nf1")), ("/mimeType", json!("application/vnd.google-apps.folder"))],
        },
        Case {
            tool: "GOOGLEDRIVE_CREATE_FILE_FROM_TEXT",
            args: json!({"file_name": "metrics.csv", "text": "date,signups\n2026-02-01,42\n",
                "content_type": "text/csv", "parent_folder_id": "fld1"}),
            method: "POST",
            url_contains: &[
                "/upload/drive/v3/files?uploadType=multipart",
                "supportsAllDrives=true",
            ],
            // multipart/related body; covered in the adapter unit tests
            body_eq: vec![],
            response_status: 200,
            response: json!({"id": "nf2", "name": "metrics.csv", "mimeType": "text/csv",
                "parents": ["fld1"]}),
            result_eq: vec![("/id", json!("nf2")), ("/mimeType", json!("text/csv"))],
        },
        Case {
            tool: "GOOGLEDRIVE_COPY_FILE",
            args: json!({"file_id": "doc1", "name": "Report (Kopie)", "folder_id": "fld1",
                "convert_to": "application/vnd.google-apps.document"}),
            method: "POST",
            url_contains: &["/drive/v3/files/doc1/copy?supportsAllDrives=true"],
            body_eq: vec![
                ("/name", json!("Report (Kopie)")),
                ("/parents/0", json!("fld1")),
                ("/mimeType", json!("application/vnd.google-apps.document")),
            ],
            response_status: 200,
            response: json!({"id": "copy1", "name": "Report (Kopie)",
                "mimeType": "application/vnd.google-apps.document", "parents": ["fld1"]}),
            result_eq: vec![("/id", json!("copy1"))],
        },
        Case {
            tool: "GOOGLEDRIVE_MOVE_FILE",
            args: json!({"file_id": "f1", "folder_id": "fld2", "current_parents": "fld1"}),
            method: "PATCH",
            url_contains: &[
                "/drive/v3/files/f1?",
                "addParents=fld2",
                "removeParents=fld1",
                "fields=*",
                "supportsAllDrives=true",
            ],
            body_eq: vec![],
            response_status: 200,
            response: json!({"id": "f1", "name": "notes.txt", "parents": ["fld2"]}),
            result_eq: vec![("/parents/0", json!("fld2"))],
        },
        Case {
            tool: "GOOGLEDRIVE_TRASH_FILE",
            args: json!({"file_id": "f1"}),
            method: "PATCH",
            url_contains: &["/drive/v3/files/f1?supportsAllDrives=true"],
            body_eq: vec![("/trashed", json!(true))],
            response_status: 200,
            response: json!({"id": "f1", "name": "notes.txt", "trashed": true}),
            result_eq: vec![("/trashed", json!(true))],
        },
        Case {
            tool: "GOOGLEDRIVE_UNTRASH_FILE",
            args: json!({"file_id": "f1"}),
            method: "PATCH",
            url_contains: &["/drive/v3/files/f1?supportsAllDrives=true"],
            body_eq: vec![("/trashed", json!(false))],
            response_status: 200,
            response: json!({"id": "f1", "name": "notes.txt", "trashed": false}),
            result_eq: vec![("/trashed", json!(false))],
        },
        Case {
            tool: "GOOGLEDRIVE_DELETE_FILE",
            args: json!({"file_id": "f1"}),
            method: "DELETE",
            url_contains: &["/drive/v3/files/f1?supportsAllDrives=true"],
            body_eq: vec![],
            response_status: 204,
            response: json!({}),
            result_eq: vec![("/deleted", json!(true))],
        },
        Case {
            tool: "GOOGLEDRIVE_UPDATE_FILE_METADATA",
            args: json!({"file_id": "f1", "name": "Quarterly notes", "description": "Q1 numbers",
                "starred": true}),
            method: "PATCH",
            url_contains: &["/drive/v3/files/f1?supportsAllDrives=true"],
            body_eq: vec![
                ("/name", json!("Quarterly notes")),
                ("/description", json!("Q1 numbers")),
                ("/starred", json!(true)),
            ],
            response_status: 200,
            response: json!({"id": "f1", "name": "Quarterly notes", "description": "Q1 numbers",
                "starred": true}),
            result_eq: vec![("/name", json!("Quarterly notes")), ("/starred", json!(true))],
        },
        Case {
            tool: "GOOGLEDRIVE_SHARE_FILE",
            args: json!({"file_id": "fld1", "user_email": "carol@example.com", "role": "writer",
                "send_invitation_email": true}),
            method: "POST",
            url_contains: &[
                "/drive/v3/files/fld1/permissions?",
                "supportsAllDrives=true",
                "sendNotificationEmail=true",
            ],
            body_eq: vec![
                ("/type", json!("user")),
                ("/role", json!("writer")),
                ("/emailAddress", json!("carol@example.com")),
            ],
            response_status: 200,
            response: json!({"id": "p9", "type": "user", "role": "writer",
                "emailAddress": "carol@example.com"}),
            result_eq: vec![("/id", json!("p9")), ("/emailAddress", json!("carol@example.com"))],
        },
        Case {
            tool: "GOOGLEDRIVE_SET_PUBLIC_ACCESS",
            args: json!({"file_id": "f1", "role": "reader"}),
            method: "POST",
            url_contains: &["/drive/v3/files/f1/permissions?supportsAllDrives=true"],
            body_eq: vec![("/type", json!("anyone")), ("/role", json!("reader"))],
            response_status: 200,
            response: json!({"id": "anyoneWithLink", "type": "anyone", "role": "reader"}),
            result_eq: vec![("/id", json!("anyoneWithLink")), ("/type", json!("anyone"))],
        },
        Case {
            tool: "GOOGLEDRIVE_UPDATE_PERMISSION",
            args: json!({"file_id": "fld1", "permission_id": "101", "new_role": "reader"}),
            method: "PATCH",
            url_contains: &["/drive/v3/files/fld1/permissions/101?supportsAllDrives=true"],
            body_eq: vec![("/role", json!("reader"))],
            response_status: 200,
            response: json!({"id": "101", "role": "reader", "emailAddress": "bob@example.com"}),
            result_eq: vec![("/role", json!("reader"))],
        },
        Case {
            tool: "GOOGLEDRIVE_REMOVE_PERMISSION",
            args: json!({"file_id": "fld1", "permission_id": "101"}),
            method: "DELETE",
            url_contains: &["/drive/v3/files/fld1/permissions/101?supportsAllDrives=true"],
            body_eq: vec![],
            response_status: 204,
            response: json!({}),
            result_eq: vec![("/removed", json!(true))],
        },
        Case {
            tool: "GOOGLEDRIVE_CREATE_COMMENT",
            args: json!({"file_id": "f1", "content": "Please review by Friday"}),
            method: "POST",
            url_contains: &["/drive/v3/files/f1/comments?fields=*"],
            body_eq: vec![("/content", json!("Please review by Friday"))],
            response_status: 200,
            response: json!({"id": "c2", "content": "Please review by Friday",
                "author": {"displayName": "Ada"}, "replies": []}),
            result_eq: vec![("/id", json!("c2"))],
        },
        Case {
            tool: "GOOGLEDRIVE_CREATE_REPLY",
            args: json!({"file_id": "f1", "comment_id": "c2", "content": "Done", "action": "resolve"}),
            method: "POST",
            url_contains: &["/drive/v3/files/f1/comments/c2/replies?fields=*"],
            body_eq: vec![("/content", json!("Done")), ("/action", json!("resolve"))],
            response_status: 200,
            response: json!({"id": "r2", "content": "Done", "action": "resolve"}),
            result_eq: vec![("/id", json!("r2")), ("/action", json!("resolve"))],
        },
        Case {
            tool: "GOOGLEDRIVE_UPDATE_REPLY",
            args: json!({"file_id": "f1", "comment_id": "c2", "reply_id": "r2", "content": "Done, see v3"}),
            method: "PATCH",
            url_contains: &["/drive/v3/files/f1/comments/c2/replies/r2?fields=*"],
            body_eq: vec![("/content", json!("Done, see v3"))],
            response_status: 200,
            response: json!({"id": "r2", "content": "Done, see v3"}),
            result_eq: vec![("/content", json!("Done, see v3"))],
        },
        Case {
            tool: "GOOGLEDRIVE_DELETE_REPLY",
            args: json!({"file_id": "f1", "comment_id": "c2", "reply_id": "r2"}),
            method: "DELETE",
            url_contains: &["/drive/v3/files/f1/comments/c2/replies/r2"],
            body_eq: vec![],
            response_status: 204,
            response: json!({}),
            result_eq: vec![("/deleted", json!(true))],
        },
        Case {
            tool: "GOOGLEDRIVE_CREATE_SHARED_DRIVE",
            args: json!({"name": "Neues Drive"}),
            method: "POST",
            url_contains: &["/drive/v3/drives?requestId="],
            body_eq: vec![("/name", json!("Neues Drive"))],
            response_status: 200,
            response: json!({"id": "sd2", "name": "Neues Drive"}),
            result_eq: vec![("/id", json!("sd2")), ("/name", json!("Neues Drive"))],
        },
        Case {
            tool: "GOOGLEDRIVE_UPDATE_SHARED_DRIVE",
            args: json!({"drive_id": "sd1", "name": "Marketing EMEA"}),
            method: "PATCH",
            url_contains: &["/drive/v3/drives/sd1"],
            body_eq: vec![("/name", json!("Marketing EMEA"))],
            response_status: 200,
            response: json!({"id": "sd1", "name": "Marketing EMEA"}),
            result_eq: vec![("/name", json!("Marketing EMEA"))],
        },
        Case {
            tool: "GOOGLEDRIVE_DELETE_SHARED_DRIVE",
            args: json!({"drive_id": "sd1"}),
            method: "DELETE",
            url_contains: &["/drive/v3/drives/sd1"],
            body_eq: vec![],
            response_status: 204,
            response: json!({}),
            result_eq: vec![("/deleted", json!(true))],
        },
        Case {
            tool: "GOOGLEDRIVE_EMPTY_TRASH",
            args: json!({"drive_id": "sd1"}),
            method: "DELETE",
            url_contains: &["/drive/v3/files/trash?driveId=sd1"],
            body_eq: vec![],
            response_status: 204,
            response: json!({}),
            result_eq: vec![("/success", json!(true))],
        },
    ]
}

#[test]
fn recorded_actions() {
    let d = GoogleDrive::default();
    let cases = cases();
    for c in &cases {
        support::run(&d, c);
    }
    support::assert_full_coverage(&d, &cases);
}

#[test]
fn mutating_tools_require_the_drive_scope_and_flag() {
    let d = GoogleDrive::default();
    let read_only = [
        "GOOGLEDRIVE_SEARCH_FILES",
        "GOOGLEDRIVE_GET_FILE",
        "GOOGLEDRIVE_LIST_FILES",
        "GOOGLEDRIVE_GET_ABOUT",
        "GOOGLEDRIVE_LIST_PERMISSIONS",
        "GOOGLEDRIVE_LIST_COMMENTS",
        "GOOGLEDRIVE_LIST_REPLIES",
        "GOOGLEDRIVE_GET_REPLY",
        "GOOGLEDRIVE_LIST_SHARED_DRIVES",
        "GOOGLEDRIVE_GET_SHARED_DRIVE",
    ];
    for t in &d.spec().tools {
        assert_eq!(t.mutating, !read_only.contains(&t.slug.as_str()), "{}", t.slug);
        assert_eq!(t.scopes, vec!["https://www.googleapis.com/auth/drive"], "{}", t.slug);
    }
}

/// Live smoke test (read-only). Enable with
/// `CTOX_TOOLHUB_LIVE_GOOGLEDRIVE_TOKEN=<access token> cargo test -- --ignored`.
#[test]
#[ignore]
fn live_read_only_calls() {
    let d = GoogleDrive::default();
    for (tool, args) in [
        ("GOOGLEDRIVE_GET_ABOUT", json!({})),
        (
            "GOOGLEDRIVE_SEARCH_FILES",
            json!({"query_term": "name", "operator": "contains", "value": "a"}),
        ),
        ("GOOGLEDRIVE_LIST_SHARED_DRIVES", json!({"page_size": 5})),
    ] {
        if let Some(result) = support::live(&d, tool, args) {
            let out = result.unwrap_or_else(|e| panic!("{tool}: live call failed: {e}"));
            assert!(out.is_object(), "{tool}: expected an object result");
        }
    }
}
