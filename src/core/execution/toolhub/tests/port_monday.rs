//! Recorded port tests for the monday toolkit.
//!
//! Every case is derived from the TypeScript piece: the request assertions describe
//! exactly what the Activepieces `run()` (via `monday-sdk-js`) sends — one
//! `POST https://api.monday.com/v2` carrying `{"query": …, "variables": …}` — and the
//! response bodies are realistic monday.com GraphQL payloads.
mod support;

use ctox_toolhub::toolkits::monday::Monday;
use ctox_toolhub::toolkits::ToolkitAdapter;
use serde_json::{json, Value};
use support::{assert_full_coverage, live, Case};

/// `monday-sdk-js` posts every call here (`src/lib/common/client.ts`).
const ENDPOINT: &str = "https://api.monday.com/v2";

fn adapter() -> Monday {
    Monday::with_api_base(ENDPOINT)
}

fn ptrs<const N: usize>(pairs: [(&'static str, Value); N]) -> &'static [(&'static str, Value)] {
    Box::leak(Box::new(pairs))
}

fn url() -> &'static [&'static str] {
    &["api.monday.com/v2"]
}

/// Item fields as `ITEM_WITH_VALUES_FIELDS` returns them.
fn item(id: &str, name: &str) -> Value {
    json!({
        "id": id, "name": name, "state": "normal",
        "url": format!("https://example.monday.com/boards/1234567890/items/{id}"),
        "created_at": "2026-09-02T08:15:31.482Z",
        "updated_at": "2026-10-01T11:02:44.101Z",
        "board": {"id": "1234567890"},
        "group": {"id": "topics", "title": "New requests"},
        "column_values": [
            {"id": "status", "type": "status", "text": "Working on it",
             "value": "{\"label\":\"Working on it\"}"},
            {"id": "date4", "type": "date", "text": "01-10-2026",
             "value": "{\"date\":\"2026-10-01\"}"}
        ]
    })
}

#[test]
fn monday_actions_are_ported() {
    let a = adapter();

    let cases: Vec<Case> = vec![
        // ------------------------------------------------------------- users
        Case {
            tool: "MONDAY_GET_ME",
            args: json!({}),
            method: "POST",
            url_contains: url(),
            body_eq: ptrs([("/variables", json!({}))]),
            response_status: 200,
            response: json!({
                "data": {"me": {
                    "id": "4456778899",
                    "name": "Alex Berger",
                    "email": "alex.berger@example.com",
                    "title": "Head of Ops",
                    "status": "active",
                    "url": "https://example.monday.com/people/4456778899",
                    "time_zone_identifier": "Europe/Berlin",
                    "country_code": "DE",
                    "created_at": "2021-03-04T10:00:00.000Z",
                    "user_config": {"kind": "admin"},
                    "photo_url": {"small": "https://images.monday.com/avatar_small.png"},
                    "account": {"id": "1122334455", "name": "Example GmbH", "slug": "example"}
                }}
            }),
            result_eq: ptrs([
                ("/id", json!("4456778899")),
                ("/email", json!("alex.berger@example.com")),
                ("/kind", json!("admin")),
                ("/time_zone", json!("Europe/Berlin")),
                ("/account_slug", json!("example")),
            ]),
        },
        Case {
            tool: "MONDAY_GET_ACCOUNT",
            args: json!({}),
            method: "POST",
            url_contains: url(),
            body_eq: ptrs([("/variables", json!({}))]),
            response_status: 200,
            response: json!({
                "data": {"account": {
                    "id": "1122334455", "name": "Example GmbH", "slug": "example",
                    "tier": "enterprise", "country_code": "DE",
                    "active_members_count": 42, "first_day_of_the_week": "monday",
                    "is_during_trial": false, "is_trial_expired": false,
                    "show_timeline_weekends": true,
                    "plan": {"tier": "enterprise", "period": "annual", "max_users": 100,
                             "version": "default"},
                    "products": [
                        {"id": "pro", "kind": "work_management", "tier": "enterprise"},
                        {"id": "dev", "kind": "developers", "tier": "enterprise"}
                    ]
                }}
            }),
            result_eq: ptrs([
                ("/slug", json!("example")),
                ("/plan_tier", json!("enterprise")),
                ("/products", json!("work_management:enterprise, developers:enterprise")),
            ]),
        },
        Case {
            tool: "MONDAY_LIST_USERS",
            args: json!({"emails": ["alex.berger@example.com"], "status": ["ACTIVE"],
                         "limit": 10, "page": 1}),
            method: "POST",
            url_contains: url(),
            body_eq: ptrs([
                ("/variables/emails", json!(["alex.berger@example.com"])),
                ("/variables/status", json!(["ACTIVE"])),
                ("/variables/limit", json!(10)),
            ]),
            response_status: 200,
            response: json!({
                "data": {"users": [{
                    "id": "4456778899", "name": "Alex Berger",
                    "email": "alex.berger@example.com",
                    "title": "Head of Ops", "status": "active",
                    "url": "https://example.monday.com/people/4456778899",
                    "time_zone_identifier": "Europe/Berlin", "country_code": "DE",
                    "location": "Berlin", "phone": "+49 30 1234567",
                    "created_at": "2021-03-04T10:00:00.000Z",
                    "user_config": {"kind": "admin"}
                }]}
            }),
            result_eq: ptrs([
                ("/count", json!(1)),
                ("/users/0/email", json!("alex.berger@example.com")),
                ("/users/0/location", json!("Berlin")),
            ]),
        },
        Case {
            tool: "MONDAY_LIST_TEAMS",
            args: json!({}),
            method: "POST",
            url_contains: url(),
            body_eq: ptrs([("/variables", json!({}))]),
            response_status: 200,
            response: json!({
                "data": {"teams": [{
                    "id": "7788990011", "name": "Operations",
                    "picture_url": "https://images.monday.com/team.png",
                    "users": [
                        {"id": "4456778899", "name": "Alex Berger",
                         "email": "alex.berger@example.com"},
                        {"id": "4456778900", "name": "Sam Rivera",
                         "email": "sam.rivera@example.com"}
                    ],
                    "owners": [{"id": "4456778899", "name": "Alex Berger"}]
                }]}
            }),
            result_eq: ptrs([
                ("/teams/0/member_count", json!(2)),
                ("/teams/0/member_emails",
                 json!("alex.berger@example.com, sam.rivera@example.com")),
                ("/teams/0/owner_names", json!("Alex Berger")),
            ]),
        },
        // --------------------------------------------------------- workspaces
        Case {
            tool: "MONDAY_LIST_WORKSPACES",
            args: json!({"kind": "open"}),
            method: "POST",
            url_contains: url(),
            body_eq: ptrs([
                ("/variables/kind", json!("open")),
                ("/variables/state", json!("active")),
                ("/variables/membership_kind", json!("all")),
                ("/variables/limit", json!(50)),
            ]),
            response_status: 200,
            response: json!({
                "data": {"workspaces": [{
                    "id": "5566778899", "name": "Example Workspace", "kind": "open",
                    "description": "Main workspace", "state": "active",
                    "created_at": "2021-01-04T09:00:00.000Z",
                    "is_default_workspace": true
                }]}
            }),
            result_eq: ptrs([
                ("/count", json!(1)),
                ("/workspaces/0/name", json!("Example Workspace")),
                ("/workspaces/0/is_default_workspace", json!(true)),
            ]),
        },
        // ------------------------------------------------------------- boards
        Case {
            tool: "MONDAY_LIST_BOARDS",
            args: json!({"workspace_ids": ["5566778899"]}),
            method: "POST",
            url_contains: url(),
            body_eq: ptrs([
                ("/variables/workspace_ids", json!(["5566778899"])),
                ("/variables/state", json!("active")),
            ]),
            response_status: 200,
            response: json!({
                "data": {"boards": [{
                    "id": "1234567890", "name": "Product Roadmap",
                    "description": "Quarterly planning", "state": "active",
                    "board_kind": "public",
                    "url": "https://example.monday.com/boards/1234567890",
                    "workspace_id": "5566778899", "board_folder_id": "9900112233",
                    "items_count": 128, "updated_at": "2026-10-10T07:45:00.000Z",
                    "owners": [{"id": "4456778899", "name": "Alex Berger"}]
                }]}
            }),
            result_eq: ptrs([
                ("/count", json!(1)),
                ("/boards/0/items_count", json!(128)),
                ("/boards/0/folder_id", json!("9900112233")),
                ("/boards/0/owner_names", json!("Alex Berger")),
            ]),
        },
        Case {
            tool: "MONDAY_LIST_BOARD_VIEWS",
            args: json!({"board_id": "1234567890"}),
            method: "POST",
            url_contains: url(),
            body_eq: ptrs([("/variables/ids", json!(["1234567890"]))]),
            response_status: 200,
            response: json!({
                "data": {"boards": [{"views": [
                    {"id": "11223344", "name": "Default view", "type": "default",
                     "access_level": "write", "source_view_id": null}
                ]}]}
            }),
            result_eq: ptrs([
                ("/count", json!(1)),
                ("/views/0/name", json!("Default view")),
                ("/views/0/access_level", json!("write")),
            ]),
        },
        // ------------------------------------------------------------ columns
        Case {
            tool: "MONDAY_LIST_COLUMNS",
            args: json!({"board_id": "1234567890"}),
            method: "POST",
            url_contains: url(),
            body_eq: ptrs([("/variables/boardIds", json!(["1234567890"]))]),
            response_status: 200,
            response: json!({
                "data": {"boards": [{"id": "1234567890", "columns": [
                    {"id": "status", "title": "Status", "type": "status",
                     "description": null,
                     "settings": "{\"labels\":[{\"id\":1,\"name\":\"Done\"}]}",
                     "archived": false, "width": 150},
                    {"id": "date4", "title": "Date", "type": "date",
                     "description": null, "settings": null,
                     "archived": false, "width": 130}
                ]}]}
            }),
            result_eq: ptrs([
                ("/board_id", json!("1234567890")),
                ("/count", json!(2)),
                ("/columns/0/type", json!("status")),
                ("/columns/0/width", json!(150)),
            ]),
        },
        Case {
            tool: "MONDAY_GET_COLUMN_TYPE_SCHEMA",
            args: json!({"column_type": "numbers"}),
            method: "POST",
            url_contains: url(),
            body_eq: ptrs([("/variables/type", json!("numbers"))]),
            response_status: 200,
            response: json!({
                "data": {"get_column_type_schema":
                    "{\n  \"title\": \"Numbers\",\n  \"type\": \"number\"\n}"}
            }),
            result_eq: ptrs([("/schema",
                json!("{\n  \"title\": \"Numbers\",\n  \"type\": \"number\"\n}"))]),
        },
        // ------------------------------------------------------------- groups
        Case {
            tool: "MONDAY_LIST_GROUPS",
            args: json!({"board_id": "1234567890"}),
            method: "POST",
            url_contains: url(),
            body_eq: ptrs([("/variables/boardIds", json!(["1234567890"]))]),
            response_status: 200,
            response: json!({
                "data": {"boards": [{"id": "1234567890", "groups": [
                    {"id": "topics", "title": "New requests", "color": "blue",
                     "archived": false, "deleted": false},
                    {"id": "archive", "title": "Done", "color": "green",
                     "archived": false, "deleted": false}
                ]}]}
            }),
            result_eq: ptrs([
                ("/board_id", json!("1234567890")),
                ("/groups/1/title", json!("Done")),
                ("/groups/0/color", json!("blue")),
            ]),
        },
        // -------------------------------------------------------------- items
        Case {
            tool: "MONDAY_LIST_BOARD_ITEMS",
            args: json!({"board_id": "1234567890", "limit": 25, "column_ids": ["status"]}),
            method: "POST",
            url_contains: url(),
            body_eq: ptrs([
                ("/variables/boardId", json!(["1234567890"])),
                ("/variables/limit", json!(25)),
                ("/variables/columnIds", json!(["status"])),
            ]),
            response_status: 200,
            response: json!({
                "data": {"boards": [{"id": "1234567890", "items_page": {
                    "cursor": "MjAyNi0xMC0wMQ==",
                    "items": [item("9876543210", "Ship the pricing page"),
                              item("9876543211", "Fix login redirect")]
                }}]}
            }),
            result_eq: ptrs([
                ("/count", json!(2)),
                ("/cursor", json!("MjAyNi0xMC0wMQ==")),
                ("/items/0/name", json!("Ship the pricing page")),
                ("/items/0/group_title", json!("New requests")),
                ("/items/0/column_values/0/column_id", json!("status")),
            ]),
        },
        Case {
            tool: "MONDAY_GET_ITEMS",
            args: json!({"item_ids": ["9876543210", "9876543211"]}),
            method: "POST",
            url_contains: url(),
            body_eq: ptrs([("/variables/ids", json!(["9876543210", "9876543211"]))]),
            response_status: 200,
            response: json!({
                "data": {"items": [{
                    "id": "9876543210", "name": "Ship the pricing page", "state": "normal",
                    "created_at": "2026-09-02T08:15:31.482Z",
                    "updated_at": "2026-10-01T11:02:44.101Z",
                    "url": "https://example.monday.com/boards/1234567890/items/9876543210",
                    "board": {"id": "1234567890", "name": "Product Roadmap"},
                    "group": {"id": "topics", "title": "New requests"},
                    "parent_item": null,
                    "column_values": [
                        {"id": "status", "type": "status", "text": "Working on it",
                         "value": "{\"label\":\"Working on it\"}"}
                    ]
                }]}
            }),
            result_eq: ptrs([
                ("/count", json!(1)),
                ("/items/0/board_name", json!("Product Roadmap")),
                ("/items/0/group_title", json!("New requests")),
                ("/items/0/column_values/0/text", json!("Working on it")),
            ]),
        },
        Case {
            tool: "MONDAY_SEARCH_ITEMS_BY_COLUMN_VALUES",
            args: json!({
                "board_id": "1234567890",
                "filters": [
                    {"column_id": "status", "value": "Done"},
                    {"column_id": "status", "value": "Stuck"},
                    {"column_id": "owner", "value": "4456778899"}
                ],
                "limit": 50
            }),
            method: "POST",
            url_contains: url(),
            body_eq: ptrs([
                ("/variables/boardId", json!("1234567890")),
                ("/variables/columns/0",
                 json!({"column_id": "status", "column_values": ["Done", "Stuck"]})),
                ("/variables/columns/1",
                 json!({"column_id": "owner", "column_values": ["4456778899"]})),
            ]),
            response_status: 200,
            response: json!({
                "data": {"items_page_by_column_values": {
                    "cursor": null,
                    "items": [item("9876543212", "Write the release notes")]
                }}
            }),
            result_eq: ptrs([
                ("/count", json!(1)),
                ("/items/0/id", json!("9876543212")),
                ("/cursor", json!(null)),
            ]),
        },
        Case {
            tool: "MONDAY_LIST_SUBITEMS",
            args: json!({"item_id": "9876543210", "column_ids": ["status"]}),
            method: "POST",
            url_contains: url(),
            body_eq: ptrs([
                ("/variables/ids", json!(["9876543210"])),
                ("/variables/columnIds", json!(["status"])),
            ]),
            response_status: 200,
            response: json!({
                "data": {"items": [{"id": "9876543210",
                    "subitems": [item("9876543300", "Design the banner")]}]}
            }),
            result_eq: ptrs([
                ("/count", json!(1)),
                ("/subitems/0/name", json!("Design the banner")),
                ("/subitems/0/parent_item_id", json!("9876543210")),
            ]),
        },
        Case {
            tool: "MONDAY_SEARCH",
            args: json!({"entity_type": "items", "query": "pricing", "limit": 5,
                         "board_ids": ["1234567890"]}),
            method: "POST",
            url_contains: url(),
            body_eq: ptrs([
                ("/variables/query", json!("pricing")),
                ("/variables/limit", json!(5)),
                ("/variables/board_ids", json!(["1234567890"])),
            ]),
            response_status: 200,
            response: json!({
                "data": {"search": {"items": {"results": [
                    {"id": "9876543210", "indexed_data": {
                        "name": "Ship the pricing page",
                        "url": "https://example.monday.com/boards/1234567890/items/9876543210",
                        "board_id": "1234567890", "workspace_id": "5566778899"
                    }}
                ]}}}
            }),
            result_eq: ptrs([
                ("/count", json!(1)),
                ("/results/0/id", json!("9876543210")),
                ("/results/0/entity_type", json!("items")),
                ("/results/0/board_id", json!("1234567890")),
            ]),
        },
        // ------------------------------------------------------------ updates
        Case {
            tool: "MONDAY_LIST_ITEM_UPDATES",
            args: json!({"item_id": "9876543210"}),
            method: "POST",
            url_contains: url(),
            body_eq: ptrs([
                ("/variables/ids", json!(["9876543210"])),
                ("/variables/limit", json!(25)),
                ("/variables/page", json!(1)),
            ]),
            response_status: 200,
            response: json!({
                "data": {"items": [{"id": "9876543210", "updates": [{
                    "id": "2244667788", "body": "<p>Blocked on legal</p>",
                    "text_body": "Blocked on legal",
                    "creator_id": "4456778899", "creator": {"name": "Alex Berger"},
                    "created_at": "2026-10-02T09:00:00.000Z",
                    "updated_at": "2026-10-02T09:00:00.000Z",
                    "replies": [{
                        "id": "2244667789", "body": "<p>Thanks, picking it up</p>",
                        "text_body": "Thanks, picking it up", "creator_id": "4456778900",
                        "created_at": "2026-10-02T10:00:00.000Z"
                    }]
                }]}]}
            }),
            result_eq: ptrs([
                ("/count", json!(1)),
                ("/updates/0/item_id", json!("9876543210")),
                ("/updates/0/creator_name", json!("Alex Berger")),
                ("/updates/0/reply_count", json!(1)),
                ("/updates/0/replies/0/text_body", json!("Thanks, picking it up")),
            ]),
        },
        Case {
            tool: "MONDAY_LIST_BOARD_UPDATES",
            args: json!({"board_id": "1234567890", "from_date": "2026-09-01T00:00:00Z"}),
            method: "POST",
            url_contains: url(),
            body_eq: ptrs([
                ("/variables/ids", json!(["1234567890"])),
                ("/variables/fromDate", json!("2026-09-01T00:00:00Z")),
                ("/variables/limit", json!(25)),
            ]),
            response_status: 200,
            response: json!({
                "data": {"boards": [{"id": "1234567890", "updates": [{
                    "id": "2244667788", "body": "<p>Moved to next sprint</p>",
                    "text_body": "Moved to next sprint", "item_id": "9876543211",
                    "creator_id": "4456778900", "creator": {"name": "Sam Rivera"},
                    "created_at": "2026-09-20T09:00:00.000Z",
                    "updated_at": "2026-09-20T09:00:00.000Z"
                }]}]}
            }),
            result_eq: ptrs([
                ("/updates/0/item_id", json!("9876543211")),
                ("/updates/0/creator_name", json!("Sam Rivera")),
            ]),
        },
        // ------------------------------------------------------------ folders
        Case {
            tool: "MONDAY_LIST_FOLDERS",
            args: json!({"workspace_ids": ["5566778899"]}),
            method: "POST",
            url_contains: url(),
            body_eq: ptrs([
                ("/variables/workspace_ids", json!(["5566778899"])),
                ("/variables/limit", json!(25)),
            ]),
            response_status: 200,
            response: json!({
                "data": {"folders": [{
                    "id": "9900112233", "name": "2026", "color": "dark_green",
                    "created_at": "2026-01-02T08:00:00.000Z", "owner_id": "4456778899",
                    "workspace": {"id": "5566778899", "name": "Example Workspace"},
                    "parent": null,
                    "sub_folders": [{"id": "9900112244", "name": "Q1"}],
                    "children": [{"id": "1234567890", "name": "Product Roadmap"}]
                }]}
            }),
            result_eq: ptrs([
                ("/count", json!(1)),
                ("/folders/0/workspace_name", json!("Example Workspace")),
                ("/folders/0/sub_folder_ids", json!("9900112244")),
                ("/folders/0/board_names", json!("Product Roadmap")),
            ]),
        },
        // ---------------------------------------- classic read actions 2023-10
        Case {
            tool: "MONDAY_GET_BOARD_VALUES",
            args: json!({"board_id": "1234567890", "column_ids": ["status", "numbers"]}),
            method: "POST",
            url_contains: url(),
            body_eq: ptrs([
                ("/variables/boardId", json!("1234567890")),
                ("/variables/columnIds", json!(["status", "numbers"])),
            ]),
            response_status: 200,
            response: json!({
                "data": {"boards": [{"items_page": {"items": [
                    {
                        "id": "9876543210", "name": "Ship the pricing page",
                        "column_values": [
                            {"id": "status", "type": "status", "text": "Done",
                             "value": "{\"label\":\"Done\"}", "label": "Done"},
                            {"id": "numbers", "type": "numbers", "text": "8", "value": "8"}
                        ]
                    }
                ]}}]}
            }),
            result_eq: ptrs([
                ("/0/id", json!("9876543210")),
                ("/0/status", json!("Done")),
                ("/0/numbers", json!(8.0)),
            ]),
        },
        Case {
            tool: "MONDAY_GET_ITEM_COLUMN_VALUES",
            args: json!({"board_id": "1234567890", "item_id": "9876543210"}),
            method: "POST",
            url_contains: url(),
            body_eq: ptrs([
                ("/variables/boardId", json!("1234567890")),
                ("/variables/itemId", json!("9876543210")),
            ]),
            response_status: 200,
            response: json!({
                "data": {"boards": [{"items_page": {"items": [
                    {
                        "id": "9876543210", "name": "Ship the pricing page",
                        "column_values": [
                            {"id": "date4", "type": "date", "text": "01-10-2026",
                             "value": "{\"date\":\"2026-10-01\",\"time\":\"10:00:00\"}"}
                        ]
                    }
                ]}}]}
            }),
            result_eq: ptrs([
                ("/id", json!("9876543210")),
                ("/date4", json!("2026-10-01 10:00:00")),
            ]),
        },
        // -------------------------------------------------------- item writes
        Case {
            tool: "MONDAY_CREATE_BOARD_ITEM",
            args: json!({
                "board_id": "1234567890",
                "item_name": "Draft the changelog",
                "group_id": "topics",
                "column_values": {"status": {"label": "Working on it"}}
            }),
            method: "POST",
            url_contains: url(),
            body_eq: ptrs([
                ("/variables/boardId", json!("1234567890")),
                ("/variables/itemName", json!("Draft the changelog")),
                ("/variables/groupId", json!("topics")),
                ("/variables/columnValues", json!("{\"status\":{\"label\":\"Working on it\"}}")),
                ("/variables/createLabels", json!(false)),
            ]),
            response_status: 200,
            response: json!({
                "data": {"create_item": {
                    "id": "9876543213", "name": "Draft the changelog", "state": "normal",
                    "url": "https://example.monday.com/boards/1234567890/items/9876543213",
                    "board": {"id": "1234567890"}, "group": {"id": "topics"}
                }}
            }),
            result_eq: ptrs([
                ("/id", json!("9876543213")),
                ("/name", json!("Draft the changelog")),
                ("/group_id", json!("topics")),
            ]),
        },
        Case {
            tool: "MONDAY_RENAME_ITEM",
            args: json!({"board_id": "1234567890", "item_id": "9876543210",
                         "name": "Ship pricing page v2"}),
            method: "POST",
            url_contains: url(),
            body_eq: ptrs([
                ("/variables/boardId", json!("1234567890")),
                ("/variables/itemId", json!("9876543210")),
                ("/variables/columnValues", json!("{\"name\":\"Ship pricing page v2\"}")),
            ]),
            response_status: 200,
            response: json!({
                "data": {"change_multiple_column_values": {
                    "id": "9876543210", "name": "Ship pricing page v2", "state": "normal",
                    "url": "https://example.monday.com/boards/1234567890/items/9876543210",
                    "board": {"id": "1234567890"}, "group": {"id": "topics"}
                }}
            }),
            result_eq: ptrs([
                ("/name", json!("Ship pricing page v2")),
                ("/id", json!("9876543210")),
            ]),
        },
        Case {
            tool: "MONDAY_SET_ITEM_COLUMN_VALUES",
            args: json!({
                "board_id": "1234567890",
                "item_id": "9876543210",
                "column_values": {"status": {"label": "Done"},
                                  "date4": {"date": "2026-10-01"}},
                "create_labels_if_missing": true
            }),
            method: "POST",
            url_contains: url(),
            body_eq: ptrs([
                ("/variables/columnValues",
                 json!("{\"date4\":{\"date\":\"2026-10-01\"},\"status\":{\"label\":\"Done\"}}")),
                ("/variables/createLabels", json!(true)),
            ]),
            response_status: 200,
            response: json!({
                "data": {"change_multiple_column_values": {
                    "id": "9876543210", "name": "Ship the pricing page", "state": "normal",
                    "url": "https://example.monday.com/boards/1234567890/items/9876543210",
                    "board": {"id": "1234567890"}, "group": {"id": "topics"}
                }}
            }),
            result_eq: ptrs([("/id", json!("9876543210"))]),
        },
        Case {
            tool: "MONDAY_ARCHIVE_ITEM",
            args: json!({"item_id": "9876543211"}),
            method: "POST",
            url_contains: url(),
            body_eq: ptrs([("/variables/itemId", json!("9876543211"))]),
            response_status: 200,
            response: json!({
                "data": {"archive_item": {
                    "id": "9876543211", "name": "Fix login redirect", "state": "archived",
                    "url": "https://example.monday.com/boards/1234567890/items/9876543211",
                    "board": {"id": "1234567890"}, "group": {"id": "topics"}
                }}
            }),
            result_eq: ptrs([
                ("/state", json!("archived")),
                ("/id", json!("9876543211")),
            ]),
        },
        Case {
            tool: "MONDAY_CREATE_SUBITEM",
            args: json!({"parent_item_id": "9876543210", "item_name": "Collect screenshots"}),
            method: "POST",
            url_contains: url(),
            body_eq: ptrs([
                ("/variables/parentItemId", json!("9876543210")),
                ("/variables/itemName", json!("Collect screenshots")),
                ("/variables/createLabels", json!(false)),
            ]),
            response_status: 200,
            response: json!({
                "data": {"create_subitem": {
                    "id": "9876543400", "name": "Collect screenshots", "state": "normal",
                    "url": "https://example.monday.com/boards/1234567890/items/9876543400",
                    "board": {"id": "1234567890"}, "group": {"id": "topics"}
                }}
            }),
            result_eq: ptrs([
                ("/id", json!("9876543400")),
                ("/name", json!("Collect screenshots")),
            ]),
        },
        // ----------------------------------------------------------- updates
        Case {
            tool: "MONDAY_POST_ITEM_UPDATE",
            args: json!({"item_id": "9876543210", "body": "Ready for review"}),
            method: "POST",
            url_contains: url(),
            body_eq: ptrs([
                ("/variables/itemId", json!("9876543210")),
                ("/variables/body", json!("Ready for review")),
            ]),
            response_status: 200,
            response: json!({
                "data": {"create_update": {
                    "id": "2244667790", "body": "<p>Ready for review</p>",
                    "text_body": "Ready for review", "item_id": "9876543210",
                    "creator_id": "4456778899", "created_at": "2026-10-11T12:00:00.000Z"
                }}
            }),
            result_eq: ptrs([
                ("/id", json!("2244667790")),
                ("/item_id", json!("9876543210")),
                ("/text_body", json!("Ready for review")),
            ]),
        },
        Case {
            tool: "MONDAY_SEND_NOTIFICATION",
            args: json!({
                "user_id": "4456778899", "target_id": "9876543210",
                "target_type": "item", "text": "Please take a look"
            }),
            method: "POST",
            url_contains: url(),
            body_eq: ptrs([
                ("/variables/userId", json!("4456778899")),
                ("/variables/targetId", json!("9876543210")),
                ("/variables/targetType", json!("Project")),
                ("/variables/text", json!("Please take a look")),
            ]),
            response_status: 200,
            response: json!({"data": {"create_notification": {"text": "Please take a look"}}}),
            result_eq: ptrs([
                ("/sent", json!(true)),
                ("/text", json!("Please take a look")),
            ]),
        },
        // --------------------------------------- classic write actions 2023-10
        Case {
            tool: "MONDAY_CREATE_COLUMN",
            args: json!({"board_id": "1234567890", "column_title": "Story Points",
                         "column_type": "numbers"}),
            method: "POST",
            url_contains: url(),
            body_eq: ptrs([
                ("/variables/boardId", json!("1234567890")),
                ("/variables/columnTitle", json!("Story Points")),
                ("/variables/columnType", json!("numbers")),
            ]),
            response_status: 200,
            response: json!({"data": {"create_column": {"id": "numbers7"}}}),
            result_eq: ptrs([("/id", json!("numbers7"))]),
        },
        Case {
            tool: "MONDAY_CREATE_GROUP",
            args: json!({"board_id": "1234567890", "group_name": "In progress"}),
            method: "POST",
            url_contains: url(),
            body_eq: ptrs([
                ("/variables/boardId", json!("1234567890")),
                ("/variables/groupName", json!("In progress")),
            ]),
            response_status: 200,
            response: json!({"data": {"create_group": {"id": "in_progress"}}}),
            result_eq: ptrs([("/id", json!("in_progress"))]),
        },
        Case {
            tool: "MONDAY_CREATE_UPDATE",
            args: json!({"item_id": "9876543210", "body": "Started working on it"}),
            method: "POST",
            url_contains: url(),
            body_eq: ptrs([
                ("/variables/itemId", json!("9876543210")),
                ("/variables/body", json!("Started working on it")),
            ]),
            response_status: 200,
            response: json!({"data": {"create_update": {"id": "2244667791"}}}),
            result_eq: ptrs([("/id", json!("2244667791"))]),
        },
    ];

    assert_full_coverage(&a, &cases);

    support::spec_parity("monday", &a, &cases);
    for c in &cases {
        support::run(&a, c);
    }
}

#[test]
fn monday_graphql_errors_are_reported() {
    let a = adapter();
    let resp = ctox_toolhub::http::HttpResponse {
        status: 200,
        body: serde_json::to_vec(&json!({"errors": [
            {"message": "Item name is too long"},
            {"message": "Board is read only"}
        ]}))
        .unwrap(),
        session_id: None,
    };
    let err = a.parse_response("MONDAY_ARCHIVE_ITEM", &resp).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("Item name is too long"), "{msg}");
    assert!(msg.contains("Board is read only"), "{msg}");
}

#[test]
fn auth_is_a_verbatim_token_and_writes_are_marked() {
    use ctox_toolhub::AuthScheme;
    let a = adapter();
    assert_eq!(a.spec().slug, "monday");
    let scheme = &a.spec().auth_schemes[0];
    assert!(
        matches!(scheme, AuthScheme::ApiKey { header_name, prefix }
            if header_name == "authorization" && prefix.is_empty()),
        "{scheme:?}"
    );
    let writes = [
        "MONDAY_CREATE_BOARD_ITEM", "MONDAY_RENAME_ITEM", "MONDAY_SET_ITEM_COLUMN_VALUES",
        "MONDAY_ARCHIVE_ITEM", "MONDAY_CREATE_SUBITEM", "MONDAY_POST_ITEM_UPDATE",
        "MONDAY_SEND_NOTIFICATION", "MONDAY_CREATE_COLUMN", "MONDAY_CREATE_GROUP",
        "MONDAY_CREATE_UPDATE",
    ];
    for slug in writes {
        let t = a.spec().tools.iter().find(|t| t.slug == slug).unwrap_or_else(|| panic!("{slug}"));
        assert!(t.mutating, "{slug} must be mutating (needs --confirm)");
    }
    // Nothing talks anywhere but the pinned GraphQL endpoint.
    for t in &a.spec().tools {
        if let Ok(r) = a.build_request(&t.slug, &json!({})) {
            assert_eq!(r.method, "POST", "{}", t.slug);
            assert_eq!(r.url, ENDPOINT, "{}", t.slug);
        }
    }
}

#[test]
#[ignore]
fn live_read_only_actions() {
    let a = adapter();
    for (tool, args) in [
        ("MONDAY_GET_ME", json!({})),
        ("MONDAY_LIST_BOARDS", json!({"limit": 3})),
        ("MONDAY_LIST_BOARD_ITEMS", json!({"limit": 3})),
    ] {
        let Some(result) = live(&a, tool, args) else { return };
        let v = result.unwrap_or_else(|e| panic!("{tool}: {e}"));
        assert!(v.is_object(), "{tool}: expected an object, got {v}");
    }
}