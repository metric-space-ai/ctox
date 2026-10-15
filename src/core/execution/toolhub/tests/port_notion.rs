//! Ported Activepieces `piece-notion` actions: one recorded case per tool
//! (request expectations derived from the TypeScript `run()` bodies, realistic
//! recorded Notion API responses, reduced results).
mod support;

use ctox_toolhub::toolkits::notion::Notion;
use serde_json::{json, Value};
use support::Case;

const PAGE: &str = "5c6a68f5-9e0a-4f0e-8b8d-3f6e1c9a7b21";
const DB: &str = "d3f2ab60-6a1e-4a2e-9a4f-0c5b8e1d2f30";
const BLOCK: &str = "b1a2c3d4-1111-2222-3333-444455556666";
const ROW: &str = "44445555-6666-7777-8888-9999aaaabbbb";
const PARENT: &str = "11112222-3333-4444-5555-666677778888";
const USER: &str = "9f8e7d6c-5b4a-4c3d-8e2f-1a2b3c4d5e6f";

fn page_row(id: &str, title: &str, url: &str) -> Value {
    json!({
        "object": "page", "id": id,
        "created_time": "2026-04-02T08:00:00.000Z",
        "last_edited_time": "2026-06-10T12:11:00.000Z",
        "archived": false, "url": url,
        "parent": {"type": "database_id", "database_id": DB},
        "properties": {
            "Name": {"id": "title", "type": "title",
                "title": [{"type": "text", "text": {"content": title}, "plain_text": title}]},
            "Status": {"id": "st", "type": "status",
                "status": {"id": "o2", "name": "Done", "color": "green"}}
        }
    })
}

#[test]
fn recorded_cases() {
    let notion = Notion::default();
    let cases: &[Case] = &[
        Case {
            tool: "NOTION_SEARCH",
            args: json!({"query": "Roadmap", "filter_type": "page", "limit": 3}),
            method: "POST",
            url_contains: &["/v1/search"],
            body_eq: &[
                ("/query", json!("Roadmap")),
                ("/filter/property", json!("object")),
                ("/filter/value", json!("page")),
                ("/sort/timestamp", json!("last_edited_time")),
                ("/sort/direction", json!("descending")),
                ("/page_size", json!(3)),
            ],
            response_status: 200,
            response: json!({
                "object": "list",
                "results": [
                    {"object": "page", "id": PAGE,
                     "created_time": "2026-05-01T10:00:00.000Z",
                     "last_edited_time": "2026-06-01T09:30:00.000Z",
                     "archived": false,
                     "url": "https://www.notion.so/Roadmap-5c6a68f59e0a4f0e8b8d3f6e1c9a7b21",
                     "parent": {"type": "workspace", "workspace": true},
                     "properties": {"Title": {"id": "title", "type": "title",
                         "title": [{"plain_text": "Roadmap"}]}}},
                    {"object": "database", "id": DB,
                     "created_time": "2026-05-02T10:00:00.000Z",
                     "last_edited_time": "2026-06-02T09:30:00.000Z",
                     "archived": false, "url": "https://www.notion.so/RoadmapDB-d3f2ab60",
                     "parent": {"type": "workspace", "workspace": true},
                     "title": [{"plain_text": "Roadmap DB"}], "properties": {}}
                ],
                "has_more": false, "next_cursor": null
            }),
            result_eq: &[
                ("/count", json!(2)),
                ("/results/0/object", json!("page")),
                ("/results/0/id", json!(PAGE)),
                ("/results/0/title", json!("Roadmap")),
                ("/results/1/object", json!("database")),
                ("/results/1/title", json!("Roadmap DB")),
                ("/has_more", json!(false)),
                ("/next_cursor", Value::Null),
            ],
        },
        Case {
            tool: "NOTION_LIST_DATABASES",
            args: json!({"limit": 5, "cursor": "cur-1"}),
            method: "POST",
            url_contains: &["/v1/search"],
            body_eq: &[
                ("/filter/property", json!("object")),
                ("/filter/value", json!("database")),
                ("/page_size", json!(5)),
                ("/start_cursor", json!("cur-1")),
                ("/sort/direction", json!("descending")),
            ],
            response_status: 200,
            response: json!({
                "object": "list",
                "results": [{
                    "object": "database", "id": DB,
                    "title": [{"plain_text": "Tasks DB"}],
                    "url": "https://www.notion.so/TasksDB-d3f2ab60",
                    "created_time": "2026-03-01T10:00:00.000Z",
                    "last_edited_time": "2026-06-05T09:30:00.000Z",
                    "archived": false,
                    "properties": {
                        "Name": {"id": "title", "name": "Name", "type": "title"},
                        "Status": {"id": "st", "name": "Status", "type": "status"}
                    }
                }],
                "has_more": true, "next_cursor": "cur-2"
            }),
            result_eq: &[
                ("/success", json!(true)),
                ("/data/0/id", json!(DB)),
                ("/data/0/title", json!("Tasks DB")),
                ("/data/0/properties/0", json!("Name")),
                ("/pagination/hasMore", json!(true)),
                ("/pagination/nextCursor", json!("cur-2")),
                ("/pagination/count", json!(1)),
            ],
        },
        Case {
            tool: "NOTION_GET_PAGE",
            args: json!({"page_id": PAGE}),
            method: "GET",
            url_contains: &["/v1/pages/", PAGE],
            body_eq: &[],
            response_status: 200,
            response: json!({
                "object": "page", "id": PAGE,
                "created_time": "2026-04-02T08:00:00.000Z",
                "last_edited_time": "2026-06-10T12:11:00.000Z",
                "archived": false, "url": "https://www.notion.so/Spec-5c6a68f5",
                "parent": {"type": "page_id", "page_id": PARENT},
                "properties": {
                    "Name": {"id": "title", "type": "title",
                        "title": [{"plain_text": "Spec"}]},
                    "Done": {"id": "done", "type": "checkbox", "checkbox": false}
                }
            }),
            result_eq: &[
                ("/id", json!(PAGE)),
                ("/object", json!("page")),
                ("/properties/Name/title/0/plain_text", json!("Spec")),
                ("/properties/Done/checkbox", json!(false)),
            ],
        },
        Case {
            tool: "NOTION_GET_DATABASE",
            args: json!({"database_id": DB}),
            method: "GET",
            url_contains: &["/v1/databases/", DB],
            body_eq: &[],
            response_status: 200,
            response: json!({
                "object": "database", "id": DB,
                "title": [{"plain_text": "Tasks"}],
                "description": [{"plain_text": "Team tasks"}],
                "properties": {
                    "Done": {"id": "done", "name": "Done", "type": "checkbox", "checkbox": {}},
                    "Estimate": {"id": "est", "name": "Estimate", "type": "number",
                        "number": {"format": "number"}},
                    "Name": {"id": "title", "name": "Name", "type": "title", "title": {}},
                    "Related": {"id": "rel", "name": "Related", "type": "relation",
                        "relation": {"database_id": "other-db", "synced_property_id": "xyz",
                            "synced_property_name": "Tasks"}},
                    "Status": {"id": "st", "name": "Status", "type": "status", "status": {
                        "options": [{"id": "o1", "name": "To do", "color": "default"},
                            {"id": "o2", "name": "Done", "color": "green"}],
                        "groups": [{"id": "g1", "name": "To-do", "color": "gray",
                            "option_ids": ["o1"]}]}},
                    "Tags": {"id": "tg", "name": "Tags", "type": "multi_select",
                        "multi_select": {"options": [{"id": "t1", "name": "urgent",
                            "color": "red"}]}}
                }
            }),
            result_eq: &[
                ("/success", json!(true)),
                ("/formStructure/title", json!("Tasks")),
                ("/formStructure/propertyTypes/Status", json!("status")),
                ("/formStructure/statusOptions/Status/options/0/name", json!("To do")),
                ("/formStructure/statusOptions/Status/groups/0/option_ids/0", json!("o1")),
                ("/formStructure/selectOptions/Tags/0/name", json!("urgent")),
                ("/formStructure/relationConfig/Related/database_id", json!("other-db")),
                ("/formStructure/numberConfig/Estimate/format", json!("number")),
                ("/summary/totalProperties", json!(6)),
                ("/summary/propertyTypes/status", json!(1)),
            ],
        },
        Case {
            tool: "NOTION_QUERY_DATABASE",
            args: json!({
                "database_id": DB,
                "filter": {"property": "Status", "status": {"equals": "Done"}},
                "sorts": [{"timestamp": "last_edited_time", "direction": "descending"}],
                "page_size": 50,
                "start_cursor": "c-in"
            }),
            method: "POST",
            url_contains: &["/v1/databases/", DB, "/query"],
            body_eq: &[
                ("/filter/property", json!("Status")),
                ("/filter/status/equals", json!("Done")),
                ("/sorts/0/direction", json!("descending")),
                ("/page_size", json!(50)),
                ("/start_cursor", json!("c-in")),
            ],
            response_status: 200,
            response: json!({
                "object": "list",
                "results": [page_row(ROW, "Shipped thing", "https://www.notion.so/Shipped-thing-44445555")],
                "has_more": true, "next_cursor": "c-out"
            }),
            result_eq: &[
                ("/success", json!(true)),
                ("/count", json!(1)),
                ("/results/0/id", json!(ROW)),
                ("/has_more", json!(true)),
                ("/next_cursor", json!("c-out")),
            ],
        },
        Case {
            tool: "NOTION_FIND_DATABASE_ITEM",
            args: json!({
                "database_id": DB,
                "filter": {"property": "Email", "email": {"equals": "a@b.com"}}
            }),
            method: "POST",
            url_contains: &["/v1/databases/", DB, "/query"],
            body_eq: &[
                ("/filter/property", json!("Email")),
                ("/filter/email/equals", json!("a@b.com")),
                ("/page_size", json!(100)),
            ],
            response_status: 200,
            response: json!({
                "object": "list",
                "results": [page_row(ROW, "a@b.com person", "https://www.notion.so/44445555")],
                "has_more": false, "next_cursor": null
            }),
            result_eq: &[
                ("/success", json!(true)),
                ("/count", json!(1)),
                ("/firstMatch/id", json!(ROW)),
                ("/has_more", json!(false)),
            ],
        },
        Case {
            tool: "NOTION_GET_BLOCK_CHILDREN",
            args: json!({"block_id": BLOCK}),
            method: "GET",
            url_contains: &["/v1/blocks/", BLOCK, "/children?page_size=100"],
            body_eq: &[],
            response_status: 200,
            response: json!({
                "object": "list",
                "results": [
                    {"object": "block", "id": "blk-1", "type": "paragraph",
                     "has_children": false,
                     "paragraph": {"rich_text": [{"type": "text",
                         "text": {"content": "Hello world"}, "plain_text": "Hello world",
                         "annotations": {"bold": false, "italic": false, "code": false}}]}},
                    {"object": "block", "id": "blk-2", "type": "heading_2",
                     "has_children": false,
                     "heading_2": {"rich_text": [{"plain_text": "Section",
                         "annotations": {"bold": false}}]}},
                    {"object": "block", "id": "blk-3", "type": "bulleted_list_item",
                     "has_children": false,
                     "bulleted_list_item": {"rich_text": [{"plain_text": "item",
                         "annotations": {}}]}}
                ],
                "has_more": false, "next_cursor": null
            }),
            result_eq: &[
                ("/count", json!(3)),
                ("/blocks/0/id", json!("blk-1")),
                ("/markdown", json!("Hello world\n\n## Section\n\n- item")),
            ],
        },
        Case {
            tool: "NOTION_GET_PAGE_COMMENTS",
            args: json!({"page_id": PAGE}),
            method: "GET",
            url_contains: &["/v1/comments?block_id=", PAGE, "&page_size=100"],
            body_eq: &[],
            response_status: 200,
            response: json!({
                "object": "list",
                "results": [
                    {"object": "comment", "id": "c2", "discussion_id": "disc-1",
                     "created_time": "2026-06-02T10:00:00.000Z",
                     "rich_text": [{"plain_text": "second"}],
                     "parent": {"type": "page_id", "page_id": PAGE}},
                    {"object": "comment", "id": "c1", "discussion_id": "disc-1",
                     "created_time": "2026-06-01T10:00:00.000Z",
                     "rich_text": [{"plain_text": "first"}]},
                    {"object": "comment", "id": "c3",
                     "created_time": "2026-06-03T10:00:00.000Z",
                     "rich_text": [{"plain_text": "page-level"}]}
                ],
                "has_more": false
            }),
            result_eq: &[
                ("/totalComments", json!(3)),
                ("/discussionCount", json!(1)),
                ("/discussionThreads/disc-1/0/id", json!("c1")),
                ("/discussionThreads/disc-1/1/id", json!("c2")),
                ("/pageComments/0/id", json!("c3")),
            ],
        },
        Case {
            tool: "NOTION_LIST_USERS",
            args: json!({"page_size": 2}),
            method: "GET",
            url_contains: &["/v1/users?page_size=2"],
            body_eq: &[],
            response_status: 200,
            response: json!({
                "object": "list",
                "results": [
                    {"object": "user", "id": USER, "name": "Ada", "type": "person",
                     "avatar_url": "https://example.com/ada.png",
                     "person": {"email": "ada@example.com"}},
                    {"object": "user", "id": "bot-1", "name": "Integration bot",
                     "type": "bot", "bot": {}}
                ],
                "has_more": false, "next_cursor": null
            }),
            result_eq: &[
                ("/count", json!(2)),
                ("/users/0/id", json!(USER)),
                ("/users/0/email", json!("ada@example.com")),
                ("/users/1/type", json!("bot")),
            ],
        },
        Case {
            tool: "NOTION_GET_USER",
            args: json!({"user_id": USER}),
            method: "GET",
            url_contains: &["/v1/users/", USER],
            body_eq: &[],
            response_status: 200,
            response: json!({
                "object": "user", "id": USER, "name": "Grace", "type": "person",
                "avatar_url": null, "person": {"email": "grace@example.com"}
            }),
            result_eq: &[
                ("/id", json!(USER)),
                ("/name", json!("Grace")),
                ("/type", json!("person")),
                ("/email", json!("grace@example.com")),
            ],
        },
        Case {
            tool: "NOTION_CREATE_PAGE",
            args: json!({
                "parent_page_id": PARENT,
                "title": "New page",
                "content": "# Heading\n\nBody **bold**"
            }),
            method: "POST",
            url_contains: &["/v1/pages"],
            body_eq: &[
                ("/parent/page_id", json!(PARENT)),
                ("/properties/title/title/0/text/content", json!("New page")),
                ("/children/0/type", json!("heading_1")),
                ("/children/0/heading_1/rich_text/0/text/content", json!("Heading")),
                ("/children/1/type", json!("paragraph")),
                ("/children/1/paragraph/rich_text/0/text/content", json!("Body ")),
                ("/children/1/paragraph/rich_text/1/text/content", json!("bold")),
                ("/children/1/paragraph/rich_text/1/annotations/bold", json!(true)),
            ],
            response_status: 200,
            response: json!({
                "object": "page", "id": "new-page-1",
                "url": "https://www.notion.so/New-page-1",
                "archived": false, "properties": {}
            }),
            result_eq: &[("/id", json!("new-page-1")), ("/object", json!("page"))],
        },
        Case {
            tool: "NOTION_CREATE_DATABASE",
            args: json!({
                "parent_page_id": PARENT,
                "title": "Tasks",
                "properties": {
                    "Name": {"title": {}},
                    "Status": {"select": {"options": [{"name": "Open"}]}}
                }
            }),
            method: "POST",
            url_contains: &["/v1/databases"],
            body_eq: &[
                ("/parent/type", json!("page_id")),
                ("/parent/page_id", json!(PARENT)),
                ("/title/0/type", json!("text")),
                ("/title/0/text/content", json!("Tasks")),
                ("/properties/Name/title", json!({})),
                ("/properties/Status/select/options/0/name", json!("Open")),
            ],
            response_status: 200,
            response: json!({
                "object": "database", "id": "db-new",
                "title": [{"plain_text": "Tasks"}], "properties": {}
            }),
            result_eq: &[
                ("/success", json!(true)),
                ("/database/id", json!("db-new")),
            ],
        },
        Case {
            tool: "NOTION_CREATE_DATABASE_ITEM",
            args: json!({
                "database_id": DB,
                "properties": {
                    "Name": {"title": [{"text": {"content": "My task"}}]},
                    "Estimate": {"number": 3}
                },
                "content": "Row body"
            }),
            method: "POST",
            url_contains: &["/v1/pages"],
            body_eq: &[
                ("/parent/type", json!("database_id")),
                ("/parent/database_id", json!(DB)),
                ("/properties/Name/title/0/text/content", json!("My task")),
                ("/properties/Estimate/number", json!(3)),
                ("/children/0/type", json!("paragraph")),
                ("/children/0/paragraph/rich_text/0/text/content", json!("Row body")),
            ],
            response_status: 200,
            response: page_row("row-1", "My task", "https://www.notion.so/My-task-row1"),
            result_eq: &[
                ("/success", json!(true)),
                ("/item/id", json!("row-1")),
            ],
        },
        Case {
            tool: "NOTION_APPEND_TO_PAGE",
            args: json!({"block_id": BLOCK, "content": "- a\n1. b"}),
            method: "PATCH",
            url_contains: &["/v1/blocks/", BLOCK, "/children"],
            body_eq: &[
                ("/children/0/type", json!("bulleted_list_item")),
                ("/children/0/bulleted_list_item/rich_text/0/text/content", json!("a")),
                ("/children/1/type", json!("numbered_list_item")),
                ("/children/1/numbered_list_item/rich_text/0/text/content", json!("b")),
            ],
            response_status: 200,
            response: json!({
                "object": "list", "results": [
                    {"object": "block", "id": "new-blk-1", "type": "bulleted_list_item"}
                ],
                "has_more": false
            }),
            result_eq: &[
                ("/object", json!("list")),
                ("/results/0/id", json!("new-blk-1")),
            ],
        },
        Case {
            tool: "NOTION_UPDATE_BLOCK",
            args: json!({
                "block_id": BLOCK,
                "content": "todo text",
                "block_type": "to_do",
                "checked": true
            }),
            method: "PATCH",
            url_contains: &["/v1/blocks/", BLOCK],
            body_eq: &[
                ("/to_do/rich_text/0/text/content", json!("todo text")),
                ("/to_do/checked", json!(true)),
            ],
            response_status: 200,
            response: json!({
                "object": "block", "id": BLOCK, "type": "to_do",
                "to_do": {"rich_text": [{"plain_text": "todo text"}], "checked": true}
            }),
            result_eq: &[("/id", json!(BLOCK)), ("/to_do/checked", json!(true))],
        },
        Case {
            tool: "NOTION_UPDATE_DATABASE_ITEM",
            args: json!({
                "item_id": ROW,
                "properties": {"Status": {"status": {"name": "Done"}},
                    "Estimate": {"number": 5}}
            }),
            method: "PATCH",
            url_contains: &["/v1/pages/", ROW],
            body_eq: &[
                ("/properties/Status/status/name", json!("Done")),
                ("/properties/Estimate/number", json!(5)),
            ],
            response_status: 200,
            response: page_row(ROW, "Shipped thing", "https://www.notion.so/Shipped-thing-44445555"),
            result_eq: &[("/success", json!(true)), ("/item/id", json!(ROW))],
        },
        Case {
            tool: "NOTION_UPDATE_DATABASE_SCHEMA",
            args: json!({
                "database_id": DB,
                "title": "Renamed",
                "properties": {"Old": null, "Extra": {"rich_text": {}}}
            }),
            method: "PATCH",
            url_contains: &["/v1/databases/", DB],
            body_eq: &[
                ("/title/0/text/content", json!("Renamed")),
                ("/properties/Old", Value::Null),
                ("/properties/Extra/rich_text", json!({})),
            ],
            response_status: 200,
            response: json!({
                "object": "database", "id": DB,
                "title": [{"plain_text": "Renamed"}], "properties": {}
            }),
            result_eq: &[
                ("/success", json!(true)),
                ("/database/id", json!(DB)),
                ("/database/title/0/plain_text", json!("Renamed")),
            ],
        },
        Case {
            tool: "NOTION_MOVE_PAGE",
            args: json!({"page_id": PAGE, "new_parent_database_id": DB}),
            method: "PATCH",
            url_contains: &["/v1/pages/", PAGE],
            body_eq: &[("/parent/database_id", json!(DB))],
            response_status: 200,
            response: json!({
                "object": "page", "id": PAGE, "archived": false,
                "parent": {"type": "database_id", "database_id": DB}, "properties": {}
            }),
            result_eq: &[("/id", json!(PAGE)), ("/parent/database_id", json!(DB))],
        },
        Case {
            tool: "NOTION_ARCHIVE_PAGE",
            args: json!({"page_id": PAGE, "archived": true}),
            method: "PATCH",
            url_contains: &["/v1/pages/", PAGE],
            body_eq: &[("/archived", json!(true))],
            response_status: 200,
            response: json!({
                "object": "page", "id": PAGE, "archived": true,
                "parent": {"type": "workspace", "workspace": true}, "properties": {}
            }),
            result_eq: &[("/id", json!(PAGE)), ("/archived", json!(true))],
        },
        Case {
            tool: "NOTION_ARCHIVE_DATABASE_ITEM",
            args: json!({"item_id": ROW}),
            method: "PATCH",
            url_contains: &["/v1/pages/", ROW],
            body_eq: &[("/archived", json!(true))],
            response_status: 200,
            response: json!({
                "object": "page", "id": ROW, "archived": true,
                "url": "https://www.notion.so/Row-1-44445555",
                "properties": {"Name": {"id": "title", "type": "title",
                    "title": [{"plain_text": "Row 1"}]}}
            }),
            result_eq: &[
                ("/success", json!(true)),
                ("/message", json!("\"Row 1\" has been archived successfully")),
                ("/archivedItem/id", json!(ROW)),
                ("/archivedItem/title", json!("Row 1")),
                ("/archivedItem/url", json!("https://www.notion.so/Row-1-44445555")),
            ],
        },
        Case {
            tool: "NOTION_RESTORE_DATABASE_ITEM",
            args: json!({"item_id": ROW}),
            method: "PATCH",
            url_contains: &["/v1/pages/", ROW],
            body_eq: &[("/archived", json!(false))],
            response_status: 200,
            response: json!({
                "object": "page", "id": ROW, "archived": false,
                "url": "https://www.notion.so/Row-1-44445555",
                "properties": {"Name": {"id": "title", "type": "title",
                    "title": [{"plain_text": "Row 1"}]}}
            }),
            result_eq: &[
                ("/success", json!(true)),
                ("/message", json!("\"Row 1\" has been successfully restored and is now active")),
                ("/restoredItem/id", json!(ROW)),
            ],
        },
        Case {
            tool: "NOTION_DELETE_BLOCK",
            args: json!({"block_id": BLOCK}),
            method: "DELETE",
            url_contains: &["/v1/blocks/", BLOCK],
            body_eq: &[],
            response_status: 200,
            response: json!({
                "object": "block", "id": BLOCK, "type": "paragraph",
                "archived": true, "deleted": true,
                "paragraph": {"rich_text": []}
            }),
            result_eq: &[
                ("/id", json!(BLOCK)),
                ("/archived", json!(true)),
                ("/deleted", json!(true)),
            ],
        },
        Case {
            tool: "NOTION_ADD_COMMENT",
            args: json!({"page_id": PAGE, "comment_text": "Looks good!"}),
            method: "POST",
            url_contains: &["/v1/comments"],
            body_eq: &[
                ("/parent/page_id", json!(PAGE)),
                ("/rich_text/0/type", json!("text")),
                ("/rich_text/0/text/content", json!("Looks good!")),
            ],
            response_status: 200,
            response: json!({
                "object": "comment", "id": "c-new", "discussion_id": "disc-9",
                "created_time": "2026-06-11T10:00:00.000Z",
                "rich_text": [{"plain_text": "Looks good!"}],
                "parent": {"type": "page_id", "page_id": PAGE},
                "created_by": {"id": "bot-1"}
            }),
            result_eq: &[
                ("/success", json!(true)),
                ("/comment/id", json!("c-new")),
                ("/comment/parent/page_id", json!(PAGE)),
            ],
        },
    ];
    support::assert_full_coverage(&notion, cases);
    support::spec_parity("notion", &notion, cases);
    for c in cases {
        support::run(&notion, c);
    }
}

/// Live read-only calls against the real API; skipped silently unless
/// `CTOX_TOOLHUB_LIVE_NOTION_TOKEN` is set (see PORTING.md).
#[test]
#[ignore]
fn live_read_only_calls() {
    let notion = Notion::default();
    let calls = [
        ("NOTION_SEARCH", json!({"query": "Notion", "limit": 3})),
        ("NOTION_LIST_DATABASES", json!({"limit": 5})),
        ("NOTION_LIST_USERS", json!({})),
    ];
    for (tool, args) in calls {
        match support::live(&notion, tool, args) {
            None => return, // no token configured: skip silently
            Some(Ok(v)) => println!("{tool}: {}", serde_json::to_string(&v).unwrap_or_default()),
            Some(Err(e)) => panic!("{tool} failed: {e}"),
        }
    }
}
