//! Recorded port tests for the OneDrive toolkit: one case per tool, request
//! expectations derived from the Activepieces `microsoft-onedrive` run() code
//! (`lib/actions/*.ts`, `lib/common/graph-api.ts`).
mod support;

use ctox_toolhub::toolkits::onedrive::OneDrive;
use ctox_toolhub::toolkits::ToolkitAdapter;
use serde_json::{json, Value};
use support::Case;

fn strs(v: Vec<&'static str>) -> &'static [&'static str] {
    Box::leak(v.into_boxed_slice())
}
/// JSON-pointer pairs; keys are normalized to leading-`/` form (serde_json's
/// `pointer()` returns None without it).
fn slice(v: Vec<(&'static str, Value)>) -> &'static [(&'static str, Value)] {
    let mapped: Vec<(&'static str, Value)> = v
        .into_iter()
        .map(|(k, val)| (format!("/{}", k.trim_start_matches('/')).leak() as &'static str, val))
        .collect();
    Box::leak(mapped.into_boxed_slice())
}

#[allow(clippy::too_many_arguments)]
fn case(
    tool: &'static str,
    args: Value,
    method: &'static str,
    url_contains: &'static [&'static str],
    body_eq: &'static [(&'static str, Value)],
    response_status: u16,
    response: Value,
    result_eq: &'static [(&'static str, Value)],
) -> Case<'static> {
    Case { tool, args, method, url_contains, body_eq, response_status, response, result_eq }
}

/// A driveItem as Microsoft Graph returns it (`GraphDriveItem`).
fn graph_item(id: &str, name: &str) -> Value {
    json!({
        "id": id,
        "name": name,
        "size": 20481,
        "webUrl": format!("https://contoso-my.sharepoint.com/personal/ann_contoso_com/_layouts/15/onedrive.aspx?id=%2Fpersonal%2Fann_contoso_com%2FDocuments%2F{name}"),
        "createdDateTime": "2026-01-04T08:12:00Z",
        "lastModifiedDateTime": "2026-02-11T16:40:22Z",
        "file": {"mimeType": "application/vnd.openxmlformats-officedocument.wordprocessingml.document"},
        "parentReference": {"driveId": "b!contoso", "driveType": "personal", "id": "F4D50P8",
            "name": "Documents", "path": "/drive/root:"},
        "createdBy": {"user": {"displayName": "Ann Baumann", "email": "ann@contoso.com"}},
        "lastModifiedBy": {"user": {"displayName": "Ann Baumann", "email": "ann@contoso.com"}}
    })
}

fn cases() -> Vec<Case<'static>> {
    vec![
        // --- read / search ------------------------------------------------------
        case(
            "ONEDRIVE_GET_ITEM",
            json!({"item_id": "01ABC!234"}),
            "GET",
            strs(vec!["https://graph.test/v1.0/me/drive/items/01ABC!234"]),
            slice(vec![]),
            200,
            graph_item("01ABC!234", "report.docx"),
            slice(vec![
                ("id", json!("01ABC!234")),
                ("name", json!("report.docx")),
                ("type", json!("file")),
                ("mime_type", json!("application/vnd.openxmlformats-officedocument.wordprocessingml.document")),
                ("size", json!(20481)),
                ("drive_type", json!("personal")),
            ]),
        ),
        case(
            // `oneDriveApi.itemPath` path form: `/me/drive/root:/<segments>:`.
            "ONEDRIVE_GET_ITEM",
            json!({"path": "Documents/Rechnungen 2026.xlsx"}),
            "GET",
            strs(vec!["https://graph.test/v1.0/me/drive/root:/Documents/Rechnungen%202026.xlsx:"]),
            slice(vec![]),
            200,
            graph_item("01XYZ!9", "Rechnungen 2026.xlsx"),
            slice(vec![("id", json!("01XYZ!9")), ("parent_path", json!("/drive/root:"))]),
        ),
        case(
            "ONEDRIVE_GET_DRIVE",
            json!({}),
            "GET",
            strs(vec!["https://graph.test/v1.0/me/drive"]),
            slice(vec![]),
            200,
            json!({"id": "b!contoso", "name": "OneDrive", "driveType": "personal",
                "webUrl": "https://onedrive.live.com/",
                "owner": {"user": {"displayName": "Ann Baumann", "email": "ann@contoso.com"}},
                "quota": {"total": 1073741824000i64, "used": 12884901888i64,
                    "remaining": 1060856922112i64, "deleted": 0, "state": "normal"}}),
            slice(vec![
                ("drive_type", json!("personal")),
                ("owner_email", json!("ann@contoso.com")),
                ("quota_remaining", json!(1060856922112i64)),
            ]),
        ),
        case(
            "ONEDRIVE_GET_SPECIAL_FOLDER",
            json!({"folder": "documents"}),
            "GET",
            strs(vec!["https://graph.test/v1.0/me/drive/special/documents"]),
            slice(vec![]),
            200,
            json!({"id": "root-documents", "name": "Documents",
                "folder": {"childCount": 42},
                "parentReference": {"driveId": "b!contoso", "driveType": "personal"}}),
            slice(vec![("id", json!("root-documents")), ("type", json!("folder")),
                ("child_count", json!(42))]),
        ),
        case(
            "ONEDRIVE_LIST_DRIVES",
            json!({}),
            "GET",
            strs(vec!["https://graph.test/v1.0/me/drives"]),
            slice(vec![]),
            200,
            json!({"value": [
                {"id": "b!contoso", "name": "OneDrive", "driveType": "personal",
                    "quota": {"total": 1073741824000i64, "used": 12884901888i64,
                        "remaining": 1060856922112i64, "state": "normal"}},
                {"id": "b!shared", "name": "Contoso Team", "driveType": "business"}],
                "@odata.nextLink": "https://graph.test/v1.0/me/drives?$skip=2"}),
            slice(vec![("count", json!(2)), ("items/1/drive_type", json!("business")),
                ("next_page_token", json!("https://graph.test/v1.0/me/drives?$skip=2"))]),
        ),
        case(
            "ONEDRIVE_LIST_FOLDER_CHILDREN",
            json!({"page_size": 25}),
            "GET",
            strs(vec!["https://graph.test/v1.0/me/drive/root/children?$top=25"]),
            slice(vec![]),
            200,
            json!({"value": [
                {"id": "F4D50P8", "name": "Documents", "folder": {"childCount": 7},
                    "parentReference": {"driveId": "b!contoso", "driveType": "personal"}},
                {"id": "01ABC!234", "name": "report.docx",
                    "file": {"mimeType": "application/vnd.openxmlformats-officedocument.wordprocessingml.document"}}]}),
            slice(vec![("count", json!(2)), ("items/0/type", json!("folder")),
                ("items/0/child_count", json!(7)), ("items/1/type", json!("file")),
                ("next_page_token", json!(null))]),
        ),
        case(
            "ONEDRIVE_LIST_FOLDER_CHILDREN",
            json!({"folder_path": "Documents/Reports"}),
            "GET",
            strs(vec!["https://graph.test/v1.0/me/drive/root:/Documents/Reports:/children?$top=100"]),
            slice(vec![]),
            200,
            json!({"value": []}),
            slice(vec![("count", json!(0))]),
        ),
        case(
            "ONEDRIVE_SEARCH_ITEMS",
            json!({"query": "Q3 report"}),
            "GET",
            strs(vec!["https://graph.test/v1.0/me/drive/root/search(q='Q3%20report')?$top=50"]),
            slice(vec![]),
            200,
            json!({"value": [{"id": "01ABC!234", "name": "Q3 report.docx",
                "file": {"mimeType": "application/vnd.openxmlformats-officedocument.wordprocessingml.document"}}]}),
            slice(vec![("count", json!(1)), ("items/0/name", json!("Q3 report.docx"))]),
        ),
        case(
            "ONEDRIVE_SEARCH_ITEMS",
            json!({"query": "shared", "scope": "include_shared", "page_size": 10}),
            "GET",
            strs(vec!["https://graph.test/v1.0/me/drive/search(q='shared')?$top=10"]),
            slice(vec![]),
            200,
            json!({"value": [{"id": "01REMOTE!7", "name": "Team budget.xlsx",
                "remoteItem": {"id": "01OTHER!3"}}]}),
            slice(vec![("items/0/type", json!("remote")),
                ("items/0/remote_item_id", json!("01OTHER!3"))]),
        ),
        case(
            "ONEDRIVE_LIST_DRIVE_CHANGES",
            json!({"start_from_now": true}),
            "GET",
            strs(vec!["https://graph.test/v1.0/me/drive/root/delta?token=latest"]),
            slice(vec![]),
            200,
            json!({"value": [], "@odata.deltaLink": "https://graph.test/v1.0/me/drive/root/delta?token=deltatoken-1"}),
            slice(vec![("count", json!(0)),
                ("delta_token", json!("https://graph.test/v1.0/me/drive/root/delta?token=deltatoken-1"))]),
        ),
        case(
            "ONEDRIVE_LIST_DRIVE_CHANGES",
            json!({}),
            "GET",
            strs(vec!["https://graph.test/v1.0/me/drive/root/delta"]),
            slice(vec![]),
            200,
            json!({"value": [{"id": "01ABC!234", "name": "report.docx", "file": {"mimeType": "text/plain"}}],
                "@odata.nextLink": "https://graph.test/v1.0/me/drive/root/delta?token=page2",
                "@odata.deltaLink": "https://graph.test/v1.0/me/drive/root/delta?token=final"}),
            slice(vec![("count", json!(1)),
                ("next_page_token", json!("https://graph.test/v1.0/me/drive/root/delta?token=page2")),
                ("delta_token", json!("https://graph.test/v1.0/me/drive/root/delta?token=final"))]),
        ),
        case(
            "ONEDRIVE_LIST_ITEM_PERMISSIONS",
            json!({"item_id": "01ABC!234"}),
            "GET",
            strs(vec!["https://graph.test/v1.0/me/drive/items/01ABC!234/permissions"]),
            slice(vec![]),
            200,
            json!({"value": [
                {"id": "perm-owner", "roles": ["owner"],
                    "grantedToV2": {"user": {"displayName": "Ann Baumann", "email": "ann@contoso.com", "id": "u1"}}},
                {"id": "perm-link", "roles": ["read"],
                    "link": {"type": "view", "scope": "anonymous",
                        "webUrl": "https://1drv.ms/u/s!AbCdEf", "preventsDownload": false},
                    "hasPassword": true}],
                "@odata.nextLink": "https://graph.test/v1.0/me/drive/items/01ABC!234/permissions?$skiptoken=1"}),
            slice(vec![
                ("count", json!(2)),
                ("items/0/granted_to_email", json!("ann@contoso.com")),
                ("items/1/link_type", json!("view")),
                ("items/1/link_scope", json!("anonymous")),
                ("items/1/has_password", json!(true)),
                ("items/0/inherited", json!(false)),
            ]),
        ),
        case(
            "ONEDRIVE_LIST_ITEM_THUMBNAILS",
            json!({"path": "Photos/beach.jpg"}),
            "GET",
            strs(vec!["https://graph.test/v1.0/me/drive/root:/Photos/beach.jpg:/thumbnails"]),
            slice(vec![]),
            200,
            json!({"value": [{"id": "0",
                "small": {"url": "https://graph.test/v1.0/me/drive/items/01ABC!234/thumbnails/0/small/0", "width": 48, "height": 48},
                "medium": {"url": "https://graph.test/v1.0/me/drive/items/01ABC!234/thumbnails/0/medium/0", "width": 384, "height": 288},
                "large": {"url": "https://graph.test/v1.0/me/drive/items/01ABC!234/thumbnails/0/large/0", "width": 1080, "height": 810}}]}),
            slice(vec![("count", json!(1)), ("items/0/thumbnail_set_id", json!("0")),
                ("items/0/large/width", json!(1080)), ("items/0/large/height", json!(810))]),
        ),
        case(
            "ONEDRIVE_LIST_ITEM_VERSIONS",
            json!({"item_id": "01ABC!234"}),
            "GET",
            strs(vec!["https://graph.test/v1.0/me/drive/items/01ABC!234/versions"]),
            slice(vec![]),
            200,
            json!({"value": [
                {"id": "2.0", "lastModifiedDateTime": "2026-02-11T16:40:22Z", "size": 20481,
                    "lastModifiedBy": {"user": {"displayName": "Ann Baumann", "email": "ann@contoso.com"}}},
                {"id": "1.0", "lastModifiedDateTime": "2026-01-04T08:12:00Z", "size": 19000}]}),
            slice(vec![("count", json!(2)), ("items/0/version_id", json!("2.0")),
                ("items/0/modified_by_email", json!("ann@contoso.com")), ("items/1/size", json!(19000))]),
        ),
        case(
            "ONEDRIVE_RESOLVE_SHARING_LINK",
            json!({"sharing_url": "https://1drv.ms/u/s!AbCdEfGhIjKlMnOpQrStUvWxYz"}),
            "GET",
            strs(vec!["https://graph.test/v1.0/shares/u!aHR0cHM6Ly8xZHJ2Lm1zL3UvcyFBYkNkRWZHaElqS2xNbk9wUXJTdFV2V3hZeg/driveItem"]),
            slice(vec![]),
            200,
            // Graph returns the redeemed `shareId` when
            // `Prefer: redeemSharingLinkIfNecessary` is sent.
            json!({"shareId": "u!aHR0cHM6Ly8xZHJ2Lm1zL3UvcyFBYkNkRWZHaElqS2xNbk9wUXJTdFV2V3hZeg",
                "id": "01ABC!234", "name": "report.docx",
                "file": {"mimeType": "text/plain"},
                "parentReference": {"driveId": "b!contoso", "driveType": "business"}}),
            slice(vec![("id", json!("01ABC!234")),
                ("share_id", json!("u!aHR0cHM6Ly8xZHJ2Lm1zL3UvcyFBYkNkRWZHaElqS2xNbk9wUXJTdFV2V3hZeg"))]),
        ),
        case(
            // A share token is passed through unchanged (`toShareId`).
            "ONEDRIVE_RESOLVE_SHARING_LINK",
            json!({"sharing_url": "u!aHR0cHM6Ly8xZHJ2Lm1zL3UvcyFBYkNkRWZHaElqS2xNbk9wUXJTdFV2V3hZeg"}),
            "GET",
            strs(vec!["https://graph.test/v1.0/shares/u!aHR0cHM6Ly8xZHJ2Lm1zL3UvcyFBYkNkRWZHaElqS2xNbk9wUXJTdFV2V3hZeg/driveItem"]),
            slice(vec![]),
            200,
            // Graph returns the redeemed `shareId` when
            // `Prefer: redeemSharingLinkIfNecessary` is sent.
            json!({"shareId": "u!aHR0cHM6Ly8xZHJ2Lm1zL3UvcyFBYkNkRWZHaElqS2xNbk9wUXJTdFV2V3hZeg",
                "id": "01ABC!234", "name": "report.docx",
                "file": {"mimeType": "text/plain"},
                "parentReference": {"driveId": "b!contoso", "driveType": "business"}}),
            slice(vec![("id", json!("01ABC!234"))]),
        ),
        case(
            "ONEDRIVE_LIST_BUNDLES",
            json!({}),
            "GET",
            strs(vec!["https://graph.test/v1.0/me/drive/bundles"]),
            slice(vec![]),
            200,
            json!({"value": [
                {"id": "bnd-1", "name": "Summer 2025", "size": 0,
                    "bundle": {"childCount": 3, "album": {"coverSetId": "album-1"}}},
                {"id": "bnd-2", "name": "Invoices", "bundle": {"childCount": 2}}]}),
            slice(vec![("count", json!(2)), ("items/0/bundle_child_count", json!(3)),
                ("items/0/is_album", json!(true)), ("items/1/is_album", json!(false))]),
        ),
        // --- write --------------------------------------------------------------
        case(
            "ONEDRIVE_CREATE_FOLDER",
            json!({"parent_folder_id": "F4D50P8", "name": "Invoices 2026"}),
            "POST",
            strs(vec!["https://graph.test/v1.0/me/drive/items/F4D50P8/children"]),
            slice(vec![("/name", json!("Invoices 2026")), ("/folder", json!({})),
                ("/@microsoft.graph.conflictBehavior", json!("fail"))]),
            201,
            json!({"id": "F4DNEW!", "name": "Invoices 2026", "folder": {"childCount": 0},
                "parentReference": {"driveId": "b!contoso", "id": "F4D50P8"}}),
            slice(vec![("id", json!("F4DNEW!")), ("name", json!("Invoices 2026")),
                ("type", json!("folder")), ("created", json!(true))]),
        ),
        case(
            // No parent folder: the drive root is the destination
            // (`oneDriveApi.folderPath` falls back to `root`).
            "ONEDRIVE_CREATE_FOLDER",
            json!({"name": "Archive"}),
            "POST",
            strs(vec!["https://graph.test/v1.0/me/drive/root/children"]),
            slice(vec![("/name", json!("Archive"))]),
            201,
            json!({"id": "F4DARC!", "name": "Archive", "folder": {"childCount": 0}}),
            slice(vec![("created", json!(true))]),
        ),
        case(
            "ONEDRIVE_UPDATE_ITEM",
            json!({"item_id": "01ABC!234", "name": "report-2026.docx", "description": "Q3 numbers"}),
            "PATCH",
            strs(vec!["https://graph.test/v1.0/me/drive/items/01ABC!234"]),
            slice(vec![("/name", json!("report-2026.docx")), ("/description", json!("Q3 numbers"))]),
            200,
            json!({"id": "01ABC!234", "name": "report-2026.docx", "description": "Q3 numbers",
                "file": {"mimeType": "application/vnd.openxmlformats-officedocument.wordprocessingml.document"}}),
            slice(vec![("id", json!("01ABC!234")), ("name", json!("report-2026.docx")),
                ("description", json!("Q3 numbers"))]),
        ),
        case(
            "ONEDRIVE_MOVE_ITEM",
            json!({"item_id": "01ABC!234", "destination_folder_id": "F4D50P8", "new_name": "moved.docx"}),
            "PATCH",
            strs(vec!["https://graph.test/v1.0/me/drive/items/01ABC!234"]),
            slice(vec![("/parentReference/id", json!("F4D50P8")), ("/name", json!("moved.docx"))]),
            200,
            json!({"id": "01ABC!234", "name": "moved.docx",
                "parentReference": {"id": "F4D50P8", "path": "/drive/root:/Documents"},
                "file": {"mimeType": "text/plain"}}),
            slice(vec![("name", json!("moved.docx")), ("parent_id", json!("F4D50P8")),
                ("parent_path", json!("/drive/root:/Documents"))]),
        ),
        case(
            "ONEDRIVE_DELETE_ITEM",
            json!({"path": "Documents/report.docx"}),
            "DELETE",
            strs(vec!["https://graph.test/v1.0/me/drive/root:/Documents/report.docx:"]),
            slice(vec![]),
            204,
            json!(null),
            slice(vec![("success", json!(true))]),
        ),
        case(
            "ONEDRIVE_CREATE_SHARING_LINK",
            json!({"item_id": "01ABC!234", "link_type": "edit", "scope": "organization",
                "expiration_date_time": "2026-12-31T23:59:00Z"}),
            "POST",
            strs(vec!["https://graph.test/v1.0/me/drive/items/01ABC!234/createLink"]),
            slice(vec![("/type", json!("edit")), ("/scope", json!("organization")),
                ("/retainInheritedPermissions", json!(true)),
                ("/expirationDateTime", json!("2026-12-31T23:59:00Z"))]),
            200,
            json!({"id": "perm-link", "roles": ["write"],
                "link": {"scope": "organization", "type": "edit",
                    "webUrl": "https://contoso-my.sharepoint.com/:w:/g/personal/ann_contoso_com/AbCdEf"},
                "expirationDateTime": "2026-12-31T23:59:00Z"}),
            slice(vec![("id", json!("perm-link")), ("link_type", json!("edit")),
                ("link_scope", json!("organization")),
                ("link_web_url", json!("https://contoso-my.sharepoint.com/:w:/g/personal/ann_contoso_com/AbCdEf"))]),
        ),
        case(
            "ONEDRIVE_INVITE_TO_ITEM",
            json!({"path": "Documents/report.docx", "recipients": ["bob@contoso.com", "cleo@contoso.com"],
                "role": "read", "send_invitation": true, "message": "Please review"}),
            "POST",
            strs(vec!["https://graph.test/v1.0/me/drive/root:/Documents/report.docx:/invite"]),
            slice(vec![("/roles/0", json!("read")), ("/requireSignIn", json!(true)),
                ("/sendInvitation", json!(true)), ("/retainInheritedPermissions", json!(true)),
                ("/message", json!("Please review")),
                ("/recipients/0/email", json!("bob@contoso.com")),
                ("/recipients/1/email", json!("cleo@contoso.com"))]),
            200,
            json!({"value": [
                {"id": "perm-bob", "roles": ["read"],
                    "grantedToV2": {"user": {"displayName": "Bob Stone", "email": "bob@contoso.com"}}},
                {"id": "perm-cleo", "roles": ["read"],
                    "grantedToIdentitiesV2": [{"user": {"email": "cleo@contoso.com", "displayName": "Cleo Meier"}}]}]}),
            slice(vec![("count", json!(2)), ("success", json!(true)),
                ("items/0/granted_to_name", json!("Bob Stone")),
                ("items/1/granted_to_identities", json!("cleo@contoso.com"))]),
        ),
        case(
            "ONEDRIVE_UPDATE_ITEM_PERMISSION",
            json!({"item_id": "01ABC!234", "permission_id": "perm-bob", "role": "write"}),
            "PATCH",
            strs(vec!["https://graph.test/v1.0/me/drive/items/01ABC!234/permissions/perm-bob"]),
            slice(vec![("/roles/0", json!("write"))]),
            200,
            json!({"id": "perm-bob", "roles": ["write"],
                "grantedToV2": {"user": {"displayName": "Bob Stone", "email": "bob@contoso.com"}}}),
            slice(vec![("id", json!("perm-bob")), ("roles/0", json!("write")),
                ("granted_to_email", json!("bob@contoso.com"))]),
        ),
        case(
            "ONEDRIVE_DELETE_ITEM_PERMISSION",
            json!({"item_id": "01ABC!234", "permission_id": "perm-link"}),
            "DELETE",
            strs(vec!["https://graph.test/v1.0/me/drive/items/01ABC!234/permissions/perm-link"]),
            slice(vec![]),
            204,
            json!(null),
            slice(vec![("success", json!(true))]),
        ),
        case(
            "ONEDRIVE_RESTORE_ITEM_VERSION",
            json!({"item_id": "01ABC!234", "version_id": "1.0"}),
            "POST",
            strs(vec!["https://graph.test/v1.0/me/drive/items/01ABC!234/versions/1.0/restoreVersion"]),
            slice(vec![]),
            200,
            json!({"id": "01ABC!234", "name": "report.docx", "file": {"mimeType": "text/plain"}}),
            slice(vec![("success", json!(true))]),
        ),
        case(
            "ONEDRIVE_CHECKOUT_ITEM",
            json!({"path": "Documents/report.docx"}),
            "POST",
            strs(vec!["https://graph.test/v1.0/me/drive/root:/Documents/report.docx:/checkout"]),
            slice(vec![]),
            200,
            json!({"id": "01ABC!234", "name": "report.docx", "file": {"mimeType": "text/plain"}}),
            slice(vec![("success", json!(true))]),
        ),
        case(
            "ONEDRIVE_CHECKIN_ITEM",
            json!({"item_id": "01ABC!234", "comment": "Final version"}),
            "POST",
            strs(vec!["https://graph.test/v1.0/me/drive/items/01ABC!234/checkin"]),
            slice(vec![("/comment", json!("Final version"))]),
            200,
            json!({"id": "01ABC!234", "name": "report.docx", "file": {"mimeType": "text/plain"}}),
            slice(vec![("success", json!(true))]),
        ),
        case(
            "ONEDRIVE_DISCARD_CHECKOUT",
            json!({"item_id": "01ABC!234"}),
            "POST",
            strs(vec!["https://graph.test/v1.0/me/drive/items/01ABC!234/discardCheckout"]),
            slice(vec![]),
            200,
            json!({"id": "01ABC!234", "name": "report.docx", "file": {"mimeType": "text/plain"}}),
            slice(vec![("success", json!(true))]),
        ),
        case(
            "ONEDRIVE_CREATE_BUNDLE",
            json!({"name": "Summer 2026", "item_ids": ["01ABC!234", "01XYZ!9"]}),
            "POST",
            strs(vec!["https://graph.test/v1.0/me/drive/bundles"]),
            slice(vec![("/name", json!("Summer 2026")), ("/bundle", json!({})),
                ("/@microsoft.graph.conflictBehavior", json!("rename")),
                ("/children/0/id", json!("01ABC!234")), ("/children/1/id", json!("01XYZ!9"))]),
            201,
            json!({"id": "bnd-9", "name": "Summer 2026", "bundle": {"childCount": 2}}),
            slice(vec![("id", json!("bnd-9")), ("name", json!("Summer 2026")),
                ("bundle_child_count", json!(2))]),
        ),
        case(
            "ONEDRIVE_ADD_ITEM_TO_BUNDLE",
            json!({"bundle_id": "bnd-1", "item_id": "01ABC!234"}),
            "POST",
            strs(vec!["https://graph.test/v1.0/me/drive/bundles/bnd-1/children"]),
            slice(vec![("/id", json!("01ABC!234"))]),
            200,
            json!({"value": []}),
            slice(vec![("success", json!(true))]),
        ),
        case(
            "ONEDRIVE_REMOVE_ITEM_FROM_BUNDLE",
            json!({"bundle_id": "bnd-1", "item_id": "01ABC!234"}),
            "DELETE",
            strs(vec!["https://graph.test/v1.0/me/drive/bundles/bnd-1/children/01ABC!234"]),
            slice(vec![]),
            204,
            json!(null),
            slice(vec![("success", json!(true))]),
        ),
    ]
}

#[test]
fn recorded_cases_match_activepieces_requests() {
    let a = OneDrive::with_api_base("https://graph.test");
    let cs = cases();
    support::assert_full_coverage(&a, &cs);
    for c in &cs {
        support::run(&a, c);
    }
}

/// The US Government (GCC High) cloud is a different Graph host, matching the
/// piece's `cloud` prop mapping in `lib/common/microsoft-cloud.ts`.
#[test]
fn gcc_high_cloud_uses_the_us_graph_host() {
    let a = OneDrive::with_api_base("https://graph.microsoft.us");
    let r = a.build_request("ONEDRIVE_GET_DRIVE", &json!({})).unwrap();
    assert_eq!(r.url, "https://graph.microsoft.us/v1.0/me/drive");
}

/// Arguments the TypeScript run() code refuses must fail here too.
#[test]
fn rejects_invalid_arguments() {
    let a = OneDrive::with_api_base("https://graph.test");
    let b = |tool: &str, args: Value| a.build_request(tool, &args);

    // `itemPath` needs an item ID or a path.
    assert!(b("ONEDRIVE_GET_ITEM", json!({})).is_err());
    assert!(b("ONEDRIVE_GET_ITEM", json!({"item_id": "  "})).is_err());
    // `special/{folder}` is a fixed enum, enforced with `path_segment`.
    assert!(b("ONEDRIVE_GET_SPECIAL_FOLDER", json!({"folder": "root/../secrets"})).is_err());
    // `update-item.ts` refuses an empty patch body.
    assert!(b("ONEDRIVE_UPDATE_ITEM", json!({"item_id": "01ABC!234"})).is_err());
    // `create-bundle.ts` needs at least two item IDs.
    assert!(b("ONEDRIVE_CREATE_BUNDLE", json!({"name": "n", "item_ids": ["a"]})).is_err());
    // `invite-to-item.ts` needs at least one recipient.
    assert!(b(
        "ONEDRIVE_INVITE_TO_ITEM",
        json!({"item_id": "01ABC!234", "role": "read", "recipients": []})
    )
    .is_err());
    // Dropdown values are validated against the TypeScript options.
    assert!(b(
        "ONEDRIVE_CREATE_SHARING_LINK",
        json!({"item_id": "01ABC!234", "link_type": "owner"})
    )
    .is_err());
    // A page token of another tenant/host is rejected by `resolveUrl`.
    assert!(b(
        "ONEDRIVE_LIST_FOLDER_CHILDREN",
        json!({"page_token": "https://evil.example/v1.0/me/drive/root/children?$skiptoken=1"})
    )
    .is_err());

    // Graph IDs contain `!`/`=`, which `path_segment` rejects; they are
    // percent-encoded into exactly one path segment instead.
    let r = b("ONEDRIVE_GET_ITEM", json!({"item_id": "../evil"})).unwrap();
    assert_eq!(r.url, "https://graph.test/v1.0/me/drive/items/..%2Fevil");
    let r = b("ONEDRIVE_MOVE_ITEM", json!({"item_id": "x", "destination_folder_id": "a/b"})).unwrap();
    assert!(r.url.contains("/me/drive/items/x"), "{}", r.url);
    assert_eq!(
        serde_json::from_slice::<Value>(&r.body.unwrap()).unwrap()["parentReference"]["id"],
        json!("a%2Fb")
    );
}

/// Live calls against Microsoft Graph; set
/// `CTOX_TOOLHUB_LIVE_ONEDRIVE_TOKEN=<access token>` to run, otherwise skipped.
#[test]
#[ignore]
fn live_read_only_calls() {
    let a = OneDrive::default();
    for (tool, args) in [
        ("ONEDRIVE_GET_DRIVE", json!({})),
        ("ONEDRIVE_GET_ITEM", json!({"item_id": "root"})),
        ("ONEDRIVE_LIST_FOLDER_CHILDREN", json!({"page_size": 5})),
    ] {
        let Some(result) = support::live(&a, tool, args) else {
            continue; // no token configured
        };
        let out = result.unwrap_or_else(|e| panic!("{tool}: live call failed: {e}"));
        assert!(out.is_object(), "{tool}: expected an object result");
    }
}