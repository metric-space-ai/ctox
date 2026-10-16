//! SharePoint port: one recorded case per action, expectations derived from the
//! Activepieces `run()` code (Microsoft Graph v1.0 via `@microsoft/microsoft-graph-client`).
//! Actions with several modes (find file, folder contents) get one case per mode.
mod support;

use ctox_toolhub::toolkits::sharepoint::SharePoint;
use serde_json::{json, Value};
use support::{run, Case};

const BASE: &str = "https://graph.test/v1.0";
const SITE: &str = "2c712604-1370-44e7-a1f5-426573fda80a";
/// Graph drive ids read `b!…`; `!` is percent-encoded into the path (`b%21…`).
const DRIVE: &str = "b!eJ6Fp1eSREiB-Lx7zQ9b2fSd7nBCtFpEQjvlDoKGz1f";
const DEST_DRIVE: &str = "b!8fJ6Fp1eSREiB-Lx7zQ9b2fSd7nBCtFpEQjvlDoKGzX9";
const FOLDER: &str = "01BYE5RZ5XALP3W7B6ZVDO5QCDK7VWX3JQ";
const FILE: &str = "01BYE5RZ6QN3ZWBTUFOFD3GJ2MLQOHRZM5";
const LIST: &str = "3a17c440-8b8b-4d37-9f6e-5d1a9a3b2c11";
const ITEM: &str = "7";
const PAGE: &str = "36dd9a4b-6ae7-4b1b-9a1e-7d1a2b3c4d5e";

fn adapter() -> SharePoint {
    SharePoint::with_api_base("https://graph.test")
}

/// `&[(ptr, value)]` for a `Case` field: leaked so it lives as long as the case.
fn ptrs<const N: usize>(pairs: [(&'static str, Value); N]) -> &'static [(&'static str, Value)] {
    Box::leak(Box::new(pairs))
}

fn drive_item(id: &str, name: &str, folder: bool) -> Value {
    let mut v = json!({
        "id": id, "name": name, "webUrl": format!("https://contoso.sharepoint.com/{name}"),
        "size": 19321, "createdDateTime": "2026-08-01T09:00:00Z",
        "lastModifiedDateTime": "2026-09-21T14:12:00Z",
        "file": {"mimeType": "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
                 "hashes": {"quickXorHash": "aBcDeFgHiJkLmNoPqRsTuVwXyZ="}},
        "folder": Value::Null,
    });
    if folder {
        v["file"] = Value::Null;
        v["folder"] = json!({"childCount": 3});
        v["size"] = json!(0);
    }
    v
}

fn children(items: Value) -> Value {
    json!({"@odata.nextLink": Value::Null, "value": items})
}

#[test]
fn sharepoint_actions_are_ported() {
    let a = adapter();
    let cases: Vec<Case> = vec![
        // ---------------------------------------------------------------- search/read
        Case {
            tool: "SHAREPOINT_FIND_SITE",
            args: json!({"search_term": "team site"}),
            method: "GET",
            url_contains: &[BASE, "/sites?search=team%20site&$select=displayName,id,name,webUrl"],
            body_eq: &[],
            response_status: 200,
            response: json!({"value": [
                {"id": SITE, "displayName": "Team Site", "name": "TeamSite",
                 "webUrl": "https://contoso.sharepoint.com/sites/TeamSite"}
            ], "@odata.nextLink": Value::Null}),
            result_eq: ptrs([
                ("/count", json!(1)),
                ("/sites/0/id", json!(SITE)),
                ("/sites/0/display_name", json!("Team Site")),
                ("/sites/0/web_url", json!("https://contoso.sharepoint.com/sites/TeamSite")),
            ]),
        },
        Case {
            tool: "SHAREPOINT_GET_SITE",
            args: json!({"site_id": SITE, "select": "id, displayName, webUrl, description"}),
            method: "GET",
            url_contains: &[
                "/sites/2c712604-1370-44e7-a1f5-426573fda80a?$select=id%2CdisplayName%2CwebUrl%2Cdescription",
            ],
            body_eq: &[],
            response_status: 200,
            response: json!({
                "id": SITE, "displayName": "Team Site", "name": "TeamSite",
                "webUrl": "https://contoso.sharepoint.com/sites/TeamSite",
                "description": "Everything the team shares",
                "createdDateTime": "2024-03-01T08:00:00Z"}),
            result_eq: ptrs([
                ("/displayName", json!("Team Site")),
                ("/description", json!("Everything the team shares")),
            ]),
        },
        Case {
            tool: "SHAREPOINT_LIST_SITES",
            args: json!({}),
            method: "GET",
            url_contains: &["/sites?search=*&$select=displayName,id,name"],
            body_eq: &[],
            response_status: 200,
            response: json!({"value": [
                {"id": SITE, "displayName": "Team Site", "name": "TeamSite"}
            ]}),
            result_eq: ptrs([
                ("/count", json!(1)),
                ("/items/0/id", json!(SITE)),
                ("/items/0/display_name", json!("Team Site")),
            ]),
        },
        Case {
            tool: "SHAREPOINT_LIST_DRIVES",
            args: json!({"site_id": SITE}),
            method: "GET",
            url_contains: &["/sites/2c712604-1370-44e7-a1f5-426573fda80a/drives?$select=id,name"],
            body_eq: &[],
            response_status: 200,
            response: json!({"value": [
                {"id": DRIVE, "name": "Documents", "driveType": "documentLibrary"}
            ]}),
            result_eq: ptrs([
                ("/count", json!(1)),
                ("/items/0/id", json!(DRIVE)),
                ("/items/0/name", json!("Documents")),
            ]),
        },
        Case {
            tool: "SHAREPOINT_LIST_FOLDERS",
            args: json!({"site_id": SITE, "drive_id": DRIVE}),
            method: "GET",
            url_contains: &[
                "/sites/2c712604-1370-44e7-a1f5-426573fda80a/drives/b%21eJ6Fp1eSREiB-Lx7zQ9b2fSd7nBCtFpEQjvlDoKGz1f/root/children?$filter=folder%20ne%20null&$select=id%2Cname",
            ],
            body_eq: &[],
            response_status: 200,
            response: children(json!([
                drive_item(FOLDER, "General", true),
                drive_item("01BYE5RZOTHERFOLDER000000000000", "Reports", true)
            ])),
            result_eq: ptrs([
                ("/count", json!(2)),
                ("/items/0/name", json!("General")),
                ("/items/1/name", json!("Reports")),
            ]),
        },
        Case {
            tool: "SHAREPOINT_LIST_FILES",
            args: json!({"drive_id": DRIVE}),
            method: "GET",
            url_contains: &["/drives/b%21eJ6Fp1eSREiB-Lx7zQ9b2fSd7nBCtFpEQjvlDoKGz1f/root/children?$select=id,name,file"],
            body_eq: &[],
            response_status: 200,
            response: children(json!([
                drive_item(FILE, "Q1-Report.docx", false),
                drive_item(FOLDER, "Reports", true)
            ])),
            result_eq: ptrs([
                ("/count", json!(1)),
                ("/items/0/id", json!(FILE)),
                ("/items/0/name", json!("Q1-Report.docx")),
            ]),
        },
        Case {
            tool: "SHAREPOINT_GET_DRIVE_ROOT",
            args: json!({"drive_id": DRIVE}),
            method: "GET",
            url_contains: &["/drives/b%21eJ6Fp1eSREiB-Lx7zQ9b2fSd7nBCtFpEQjvlDoKGz1f/root"],
            body_eq: &[],
            response_status: 200,
            response: drive_item("01BYE5RZROOTFOLDERID000000000000", "root", true),
            result_eq: ptrs([
                ("/id", json!("01BYE5RZROOTFOLDERID000000000000")),
                ("/folder/childCount", json!(3)),
                ("/name", json!("root")),
            ]),
        },
        Case {
            tool: "SHAREPOINT_GET_FOLDER_CONTENTS",
            args: json!({"site_id": SITE, "folder_id": FOLDER, "top": 2,
                "orderby": "name asc", "select": "id,name,size"}),
            method: "GET",
            url_contains: &[
                "/sites/2c712604-1370-44e7-a1f5-426573fda80a/drive/items/01BYE5RZ5XALP3W7B6ZVDO5QCDK7VWX3JQ/children",
                "?$top=2&$select=id%2Cname%2Csize&$orderby=name%20asc",
            ],
            body_eq: &[],
            response_status: 200,
            response: children(json!([
                drive_item(FILE, "Q1-Report.docx", false),
                drive_item("01BYE5RZSUBFOLDER0000000000000", "Archive", true)
            ])),
            result_eq: ptrs([
                ("/count", json!(2)),
                ("/items/0/name", json!("Q1-Report.docx")),
                ("/items/0/file/mimeType", json!("application/vnd.openxmlformats-officedocument.wordprocessingml.document")),
                ("/items/1/folder/childCount", json!(3)),
            ]),
        },
        Case {
            tool: "SHAREPOINT_GET_FOLDER_CONTENTS",
            args: json!({"site_id": SITE, "folder_id": "root", "include_custom_fields": true}),
            method: "GET",
            url_contains: &[
                "/sites/2c712604-1370-44e7-a1f5-426573fda80a/drive/root/children?$expand=listItem(expand=fields)",
            ],
            body_eq: &[],
            response_status: 200,
            response: children(json!([])),
            result_eq: ptrs([("/count", json!(0))]),
        },
        Case {
            tool: "SHAREPOINT_FIND_FILE",
            args: json!({"drive_id": DRIVE, "find_method": "path",
                "file_path": "Documents/Reports/Q1-Report.docx"}),
            method: "GET",
            url_contains: &["/drives/b%21eJ6Fp1eSREiB-Lx7zQ9b2fSd7nBCtFpEQjvlDoKGz1f/root:/Documents/Reports/Q1-Report.docx"],
            body_eq: &[],
            response_status: 200,
            response: drive_item(FILE, "Q1-Report.docx", false),
            result_eq: ptrs([
                ("/success", json!(true)),
                ("/files/0/id", json!(FILE)),
            ]),
        },
        Case {
            tool: "SHAREPOINT_FIND_FILE",
            args: json!({"drive_id": DRIVE, "find_method": "path", "file_path": "/Gone.docx"}),
            method: "GET",
            url_contains: &["/root:/Gone.docx"],
            body_eq: &[],
            response_status: 404,
            response: json!({"error": {"code": "itemNotFound",
                "message": "The provided path does not exist"}}),
            result_eq: ptrs([("/success", json!(true)), ("/files", json!([]))]),
        },
        Case {
            tool: "SHAREPOINT_FIND_FILE",
            args: json!({"drive_id": DRIVE, "find_method": "search",
                "search_query": "Q1 Report.docx"}),
            method: "GET",
            url_contains: &["/drives/b%21eJ6Fp1eSREiB-Lx7zQ9b2fSd7nBCtFpEQjvlDoKGz1f/root/search(q='Q1%20Report.docx')"],
            body_eq: &[],
            response_status: 200,
            response: json!({"searchTerms": ["Q1 Report.docx"], "value": [
                drive_item(FILE, "Q1-Report.docx", false)
            ]}),
            result_eq: ptrs([
                ("/success", json!(true)),
                ("/files/0/name", json!("Q1-Report.docx")),
            ]),
        },
        Case {
            tool: "SHAREPOINT_FIND_FILE",
            args: json!({"drive_id": DRIVE, "find_method": "search",
                "search_query": "report'", "search_folder_id": FOLDER}),
            method: "GET",
            url_contains: &["/drives/b%21eJ6Fp1eSREiB-Lx7zQ9b2fSd7nBCtFpEQjvlDoKGz1f/items/01BYE5RZ5XALP3W7B6ZVDO5QCDK7VWX3JQ/search(q='report%27')"],
            body_eq: &[],
            response_status: 200,
            response: json!({"value": [drive_item(FILE, "Q1-Report.docx", false)]}),
            result_eq: ptrs([("/files/0/id", json!(FILE))]),
        },
        // ---------------------------------------------------------------- lists
        Case {
            tool: "SHAREPOINT_LIST_LISTS",
            args: json!({"site_id": SITE}),
            method: "GET",
            url_contains: &["/sites/2c712604-1370-44e7-a1f5-426573fda80a/lists?$select=displayName,id"],
            body_eq: &[],
            response_status: 200,
            response: json!({"value": [
                {"id": LIST, "displayName": "Project tracker",
                 "webUrl": "https://contoso.sharepoint.com/sites/TeamSite/Lists/Project tracker"}
            ]}),
            result_eq: ptrs([
                ("/count", json!(1)),
                ("/items/0/id", json!(LIST)),
                ("/items/0/display_name", json!("Project tracker")),
            ]),
        },
        Case {
            tool: "SHAREPOINT_LIST_COLUMNS",
            args: json!({"site_id": SITE, "list_id": LIST}),
            method: "GET",
            url_contains: &["/sites/2c712604-1370-44e7-a1f5-426573fda80a/lists/3a17c440-8b8b-4d37-9f6e-5d1a9a3b2c11/columns"],
            body_eq: &[],
            response_status: 200,
            response: json!({"value": [
                {"id": "c1", "name": "Title", "displayName": "Title",
                 "description": Value::Null, "readOnly": false, "required": false,
                 "text": {"allowMultipleLines": false}},
                {"id": "c2", "name": "Status", "displayName": "Status",
                 "description": "Current state", "readOnly": false, "required": false,
                 "choice": {"displayAs": "dropDownMenu",
                            "choices": ["Not started", "In progress", "Done"]}},
                {"id": "c3", "name": "Approved", "displayName": "Approved",
                 "description": Value::Null, "readOnly": true, "required": false,
                 "boolean": {}}
            ]}),
            result_eq: ptrs([
                ("/count", json!(3)),
                ("/items/0/name", json!("Title")),
                ("/items/0/type", json!("text")),
                ("/items/1/type", json!("choice")),
                ("/items/1/choices/2", json!("Done")),
                ("/items/2/type", json!("boolean")),
                ("/items/2/read_only", json!(true)),
            ]),
        },
        Case {
            tool: "SHAREPOINT_LIST_LIST_ITEMS",
            args: json!({"site_id": SITE, "list_id": LIST}),
            method: "GET",
            url_contains: &["/sites/2c712604-1370-44e7-a1f5-426573fda80a/lists/3a17c440-8b8b-4d37-9f6e-5d1a9a3b2c11/items?$select=id&$expand=fields($select=Title)"],
            body_eq: &[],
            response_status: 200,
            response: json!({"value": [
                {"id": ITEM, "webUrl": "https://contoso.sharepoint.com/_layouts/15/listform.aspx",
                 "createdDateTime": "2026-09-01T10:00:00Z",
                 "lastModifiedDateTime": "2026-09-20T08:30:00Z",
                 "fields": {"Title": "Kickoff deck", "Status": "In progress"}}
            ]}),
            result_eq: ptrs([
                ("/count", json!(1)),
                ("/items/0/id", json!(ITEM)),
                ("/items/0/title", json!("Kickoff deck")),
                ("/items/0/fields/Status", json!("In progress")),
            ]),
        },
        Case {
            tool: "SHAREPOINT_FIND_LIST_ITEM",
            args: json!({"site_id": SITE, "list_id": LIST, "search_value": "Kickoff deck"}),
            method: "GET",
            url_contains: &["/sites/2c712604-1370-44e7-a1f5-426573fda80a/lists/3a17c440-8b8b-4d37-9f6e-5d1a9a3b2c11/items?$expand=fields&filter=fields%2FTitle%20eq%20%27Kickoff%20deck%27"],
            body_eq: &[],
            response_status: 200,
            response: json!({"value": [
                {"id": ITEM, "fields": {"Title": "Kickoff deck"}}
            ]}),
            result_eq: ptrs([
                ("/count", json!(1)),
                ("/items/0/id", json!(ITEM)),
                ("/items/0/title", json!("Kickoff deck")),
            ]),
        },
        Case {
            tool: "SHAREPOINT_LIST_PAGES",
            args: json!({"site_id": SITE}),
            method: "GET",
            url_contains: &["/sites/2c712604-1370-44e7-a1f5-426573fda80a/pages?$select=id,title"],
            body_eq: &[],
            response_status: 200,
            response: json!({"value": [
                {"id": PAGE, "title": "Home"},
                {"id": "44ee9a4buntitledpage000000000000", "title": Value::Null}
            ]}),
            result_eq: ptrs([
                ("/count", json!(2)),
                ("/items/0/title", json!("Home")),
                ("/items/1/title", json!("Page 44ee9a4buntitledpage000000000000")),
            ]),
        },
        // ---------------------------------------------------------------- write
        Case {
            tool: "SHAREPOINT_CREATE_FOLDER",
            args: json!({"drive_id": DRIVE, "parent_folder": "/General/Projects/",
                "folder_name": "Apollo"}),
            method: "PATCH",
            url_contains: &["/drives/b%21eJ6Fp1eSREiB-Lx7zQ9b2fSd7nBCtFpEQjvlDoKGz1f/root:/General/Projects/Apollo"],
            body_eq: ptrs([("/folder", json!({}))]),
            response_status: 201,
            response: drive_item("01BYE5RZNEWFOLDER00000000000000", "Apollo", true),
            result_eq: ptrs([
                ("/name", json!("Apollo")),
                ("/folder/childCount", json!(3)),
            ]),
        },
        Case {
            tool: "SHAREPOINT_CREATE_LIST",
            args: json!({"site_id": SITE, "display_name": "Risk register",
                "description": "Risks with owner and mitigation"}),
            method: "POST",
            url_contains: &["/sites/2c712604-1370-44e7-a1f5-426573fda80a/lists"],
            body_eq: ptrs([
                ("/displayName", json!("Risk register")),
                ("/description", json!("Risks with owner and mitigation")),
            ]),
            response_status: 201,
            response: json!({
                "id": "9b2c1f5e-1111-4d37-9f6e-5d1a9a3b2c99",
                "displayName": "Risk register",
                "description": "Risks with owner and mitigation",
                "webUrl": "https://contoso.sharepoint.com/sites/TeamSite/Lists/Risk register"}),
            result_eq: ptrs([
                ("/displayName", json!("Risk register")),
                ("/id", json!("9b2c1f5e-1111-4d37-9f6e-5d1a9a3b2c99")),
            ]),
        },
        Case {
            tool: "SHAREPOINT_CREATE_LIST_ITEM",
            args: json!({"site_id": SITE, "list_id": LIST, "list_columns": {
                "Title": "Status report September",
                "Status": "In progress",
                "Owners": ["ada@contoso.com", "grace@contoso.com"]}}),
            method: "POST",
            url_contains: &["/sites/2c712604-1370-44e7-a1f5-426573fda80a/lists/3a17c440-8b8b-4d37-9f6e-5d1a9a3b2c11/items"],
            body_eq: ptrs([
                ("/fields/Title", json!("Status report September")),
                ("/fields/Status", json!("In progress")),
                ("/fields/Owners", json!(["ada@contoso.com", "grace@contoso.com"])),
                ("/fields/Owners@odata.type", json!("Collection(Edm.String)")),
            ]),
            response_status: 201,
            response: json!({
                "id": "9", "fields": {"Title": "Status report September",
                                      "Status": "In progress",
                                      "Owners": ["ada@contoso.com", "grace@contoso.com"]}}),
            result_eq: ptrs([
                ("/id", json!("9")),
                ("/fields/Title", json!("Status report September")),
            ]),
        },
        Case {
            tool: "SHAREPOINT_UPDATE_LIST_ITEM",
            args: json!({"site_id": SITE, "list_id": LIST, "list_item_id": ITEM,
                "list_columns": {"Status": "Done", "Points": 8}}),
            method: "PATCH",
            url_contains: &["/sites/2c712604-1370-44e7-a1f5-426573fda80a/lists/3a17c440-8b8b-4d37-9f6e-5d1a9a3b2c11/items/7/fields"],
            body_eq: ptrs([
                ("/Status", json!("Done")),
                ("/Points", json!(8)),
            ]),
            response_status: 200,
            response: json!({"Title": "Status report September", "Status": "Done", "Points": 8}),
            result_eq: ptrs([("/Status", json!("Done")), ("/Points", json!(8))]),
        },
        Case {
            tool: "SHAREPOINT_DELETE_LIST_ITEM",
            args: json!({"site_id": SITE, "list_id": LIST, "list_item_id": ITEM}),
            method: "DELETE",
            url_contains: &["/sites/2c712604-1370-44e7-a1f5-426573fda80a/lists/3a17c440-8b8b-4d37-9f6e-5d1a9a3b2c11/items/7"],
            body_eq: &[],
            response_status: 204,
            response: Value::Null,
            result_eq: ptrs([("/deleted", json!(true))]),
        },
        Case {
            tool: "SHAREPOINT_MOVE_FILE",
            args: json!({"drive_id": DRIVE, "file_id": FILE,
                "destination_folder_id": FOLDER, "new_name": "Q1-Report-final.docx"}),
            method: "PATCH",
            url_contains: &["/drives/b%21eJ6Fp1eSREiB-Lx7zQ9b2fSd7nBCtFpEQjvlDoKGz1f/items/01BYE5RZ6QN3ZWBTUFOFD3GJ2MLQOHRZM5"],
            body_eq: ptrs([
                ("/parentReference/id", json!(FOLDER)),
                ("/name", json!("Q1-Report-final.docx")),
            ]),
            response_status: 200,
            response: drive_item(FILE, "Q1-Report-final.docx", false),
            result_eq: ptrs([
                ("/success", json!(true)),
                ("/message", json!("File moved successfully")),
                ("/item/name", json!("Q1-Report-final.docx")),
            ]),
        },
        Case {
            tool: "SHAREPOINT_COPY_ITEM_WITHIN_SITE",
            args: json!({"drive_id": DRIVE, "item_id": FILE, "destination_folder_id": FOLDER,
                "conflict_behavior": "replace", "new_name": "Q1-Report-copy.docx"}),
            method: "POST",
            url_contains: &["/drives/b%21eJ6Fp1eSREiB-Lx7zQ9b2fSd7nBCtFpEQjvlDoKGz1f/items/01BYE5RZ6QN3ZWBTUFOFD3GJ2MLQOHRZM5/copy?@microsoft.graph.conflictBehavior=replace"],
            body_eq: ptrs([
                ("/parentReference/id", json!(FOLDER)),
                ("/name", json!("Q1-Report-copy.docx")),
            ]),
            response_status: 202,
            response: Value::Null,
            result_eq: ptrs([
                ("/accepted", json!(true)),
                ("/status", json!("in_progress")),
            ]),
        },
        Case {
            tool: "SHAREPOINT_COPY_ITEM_ACROSS_SITES",
            args: json!({"drive_id": DRIVE, "item_id": FILE,
                "destination_drive_id": DEST_DRIVE, "destination_folder_id": FOLDER,
                "conflict_behavior": "rename"}),
            method: "POST",
            url_contains: &["/drives/b%21eJ6Fp1eSREiB-Lx7zQ9b2fSd7nBCtFpEQjvlDoKGz1f/items/01BYE5RZ6QN3ZWBTUFOFD3GJ2MLQOHRZM5/copy?@microsoft.graph.conflictBehavior=rename"],
            body_eq: ptrs([
                ("/parentReference/driveId", json!(DEST_DRIVE)),
                ("/parentReference/id", json!(FOLDER)),
            ]),
            response_status: 202,
            response: Value::Null,
            result_eq: ptrs([("/accepted", json!(true))]),
        },
        Case {
            tool: "SHAREPOINT_PUBLISH_PAGE",
            args: json!({"site_id": SITE, "page_id": PAGE}),
            method: "POST",
            url_contains: &["/sites/2c712604-1370-44e7-a1f5-426573fda80a/pages/36dd9a4b-6ae7-4b1b-9a1e-7d1a2b3c4d5e/microsoft.graph.sitePage/publish"],
            body_eq: ptrs([("", json!({}))]),
            response_status: 200,
            response: json!({"id": PAGE, "title": "Home", "publishState": {"checkedOutBy": Value::Null}}),
            result_eq: ptrs([
                ("/success", json!(true)),
                ("/message", json!("Page published successfully")),
            ]),
        },
    ];
    for c in &cases {
        run(&a, c);
    }
    support::assert_full_coverage(&a, &cases);
}

/// Live calls against Microsoft Graph; set CTOX_TOOLHUB_LIVE_SHAREPOINT_TOKEN=…
/// (a Graph access token with Sites.Read.All) to run, otherwise skipped silently.
#[test]
#[ignore]
fn live_read_only_calls() {
    let a = SharePoint::default();
    for (tool, args) in [
        ("SHAREPOINT_LIST_SITES", json!({})),
        ("SHAREPOINT_FIND_SITE", json!({"search_term": "team"})),
    ] {
        let Some(result) = support::live(&a, tool, args) else {
            continue; // no token configured
        };
        let out = result.unwrap_or_else(|e| panic!("{tool}: {e}"));
        assert!(out.is_object(), "{tool}: unexpected output {out}");
    }

    // Resolve one site id from the listing and read it back.
    let Some(result) = support::live(&a, "SHAREPOINT_LIST_SITES", json!({})) else {
        return;
    };
    let sites = result.unwrap_or_else(|e| panic!("SHAREPOINT_LIST_SITES: {e}"));
    let site_id = support::find_str(&sites, "id").expect("at least one accessible site");
    let out = support::live(&a, "SHAREPOINT_GET_SITE", json!({"site_id": site_id}))
        .expect("token configured")
        .unwrap_or_else(|e| panic!("SHAREPOINT_GET_SITE: {e}"));
    assert_eq!(
        support::find_str(&out, "id").as_deref(),
        Some(site_id.as_str())
    );
}
