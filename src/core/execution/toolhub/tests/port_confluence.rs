//! Recorded cases for the Confluence Cloud toolkit, derived from the Activepieces
//! `confluence` run() code (MIT): input -> expected provider request -> recorded
//! response -> parsed result. Every request arm is the `confluenceApiCall` /
//! `confluencePaginatedApiCall` of the piece (basic auth, `{domain}/wiki/api/v2` or
//! `{domain}/wiki/rest/api`).
mod support;

use ctox_toolhub::toolkits::confluence::Confluence;
use ctox_toolhub::AuthSchemeKind;
use serde_json::{json, Value};
use support::{assert_full_coverage, Case};

const SLUG: &str = "confluence";

#[test]
fn recorded_cases_and_full_coverage() {
    let confluence = Confluence::with_api_base("https://acme.atlassian.net");
    let cases = [
        // --- read / search ---
        Case {
            tool: "CONFLUENCE_LIST_SPACES",
            args: json!({"type": "global", "status": "current"}),
            method: "GET",
            url_contains: &["/wiki/api/v2/spaces", "limit=200", "type=global", "status=current"],
            body_eq: &[],
            response_status: 200,
            response: json!({"results": [
                {"id": "4242", "key": "DOCS", "name": "Docs", "type": "global", "status": "current",
                 "homepageId": "1000", "_links": {"webui": "/spaces/DOCS"}},
                {"id": "4243", "key": "ENG", "name": "Engineering", "type": "collaboration",
                 "status": "current", "homepageId": "1001"}],
                "_links": {"next": "/wiki/api/v2/spaces?cursor=abc&limit=200"}}),
            result_eq: &[("/count", json!(2)), ("/results/0/key", json!("DOCS")),
                ("/results/0/homepage_id", json!("1000")), ("/results/1/type", json!("collaboration"))],
        },
        Case {
            tool: "CONFLUENCE_GET_SPACE",
            args: json!({"space_id": "4242"}),
            method: "GET",
            url_contains: &["/wiki/api/v2/spaces/4242"],
            body_eq: &[],
            response_status: 200,
            response: json!({"id": "4242", "key": "DOCS", "name": "Docs", "type": "global",
                "status": "current", "homepageId": "1000"}),
            result_eq: &[("/key", json!("DOCS")), ("/name", json!("Docs")), ("/homepage_id", json!("1000"))],
        },
        Case {
            tool: "CONFLUENCE_LIST_SPACE_PAGES",
            args: json!({"space_id": "4242"}),
            method: "GET",
            url_contains: &["/wiki/api/v2/spaces/4242/pages", "limit=200"],
            body_eq: &[],
            response_status: 200,
            response: json!({"results": [{"id": "12345", "title": "Release notes", "status": "current",
                "spaceId": "4242"}]}),
            result_eq: &[("/count", json!(1)), ("/results/0/title", json!("Release notes"))],
        },
        Case {
            tool: "CONFLUENCE_FIND_PAGE_BY_TITLE",
            args: json!({"title": "Release notes", "space_id": "4242"}),
            method: "GET",
            url_contains: &["/wiki/api/v2/pages", "title=Release+notes", "limit=25", "space-id=4242"],
            body_eq: &[],
            response_status: 200,
            response: json!({"results": [{"id": "12345", "title": "Release notes", "spaceId": "4242",
                "status": "current"}]}),
            result_eq: &[("/found", json!(true)), ("/count", json!(1)), ("/results/0/id", json!("12345"))],
        },
        Case {
            tool: "CONFLUENCE_GET_PAGE_CONTENT",
            args: json!({"page_id": "12345"}),
            method: "GET",
            url_contains: &["/wiki/api/v2/pages/12345", "body-format=storage"],
            body_eq: &[],
            response_status: 200,
            response: json!({"id": "12345", "title": "Release notes", "status": "current", "spaceId": "4242",
                "authorId": "5b10a2", "createdDate": "2026-01-05T10:00:00.000Z",
                "version": {"number": 4, "createdAt": "2026-02-01T08:30:00.000Z"},
                "body": {"representation": "storage", "value": "<p>Shipped 2.0</p>"},
                "_links": {"webui": "/spaces/DOCS/pages/12345"}}),
            result_eq: &[("/title", json!("Release notes")), ("/version_number", json!(4)),
                ("/body_value", json!("<p>Shipped 2.0</p>"))],
        },
        Case {
            tool: "CONFLUENCE_GET_PAGE_BY_URL",
            args: json!({"url": "https://acme.atlassian.net/wiki/spaces/DOCS/pages/12345/Release+notes",
                         "body_format": "view"}),
            method: "GET",
            url_contains: &["/wiki/api/v2/pages/12345", "body-format=view"],
            body_eq: &[],
            response_status: 200,
            response: json!({"id": "12345", "title": "Release notes", "status": "current", "spaceId": "4242",
                "version": {"number": 1}, "body": {"representation": "view", "value": "<p>x</p>"}}),
            result_eq: &[("/id", json!("12345")), ("/body_representation", json!("view"))],
        },
        Case {
            tool: "CONFLUENCE_LIST_PAGE_CHILDREN",
            args: json!({"page_id": "12345"}),
            method: "GET",
            url_contains: &["/wiki/api/v2/pages/12345/children"],
            body_eq: &[],
            response_status: 200,
            response: json!({"results": [{"id": "12346", "title": "2.0", "status": "current", "spaceId": "4242"},
                {"id": "12347", "title": "1.0", "status": "archived", "spaceId": "4242"}]}),
            result_eq: &[("/count", json!(2)), ("/results/1/title", json!("1.0")),
                ("/results/1/status", json!("archived"))],
        },
        Case {
            tool: "CONFLUENCE_LIST_FOOTER_COMMENTS",
            args: json!({"page_id": "12345", "body_format": "storage"}),
            method: "GET",
            url_contains: &["/wiki/api/v2/pages/12345/footer-comments", "body-format=storage", "limit=100"],
            body_eq: &[],
            response_status: 200,
            response: json!({"results": [{"id": "9001", "title": "Looks good", "type": "comment",
                "status": "current", "pageId": "12345", "spaceId": "4242", "parentId": "9000",
                "footerId": "9001", "authorId": "5b10a2", "createdDate": "2026-02-02T09:00:00.000Z",
                "body": {"representation": "storage", "value": "<p>Looks good</p>"}}]}),
            result_eq: &[("/count", json!(1)), ("/results/0/body_value", json!("<p>Looks good</p>")),
                ("/results/0/footer_id", json!("9001"))],
        },
        Case {
            tool: "CONFLUENCE_LIST_INLINE_COMMENTS",
            args: json!({"page_id": "12345", "body_format": "atlas_doc_format"}),
            method: "GET",
            url_contains: &["/wiki/api/v2/pages/12345/inline-comments", "body-format=atlas_doc_format"],
            body_eq: &[],
            response_status: 200,
            response: json!({"results": [{"id": "9002", "pageId": "12345", "inlineId": "ii-1",
                "body": {"representation": "atlas_doc_format", "value": "{}"}}]}),
            result_eq: &[("/count", json!(1)), ("/results/0/inline_id", json!("ii-1"))],
        },
        Case {
            tool: "CONFLUENCE_LIST_ATTACHMENTS",
            args: json!({"page_id": "12345"}),
            method: "GET",
            url_contains: &["/wiki/api/v2/pages/12345/attachments", "limit=100"],
            body_eq: &[],
            response_status: 200,
            response: json!({"results": [{"id": "att123456", "title": "spec.pdf", "mediaType": "application/pdf",
                "fileSize": 20480, "comment": "", "createdDate": "2026-02-03T12:00:00.000Z", "version": 1,
                "downloadLink": "/wiki/download/attachments/12345/spec.pdf",
                "_links": {"webui": "/pages/12345/preview/spec.pdf"}}]}),
            result_eq: &[("/count", json!(1)), ("/results/0/file_size", json!(20480)),
                ("/results/0/download_link", json!("/wiki/download/attachments/12345/spec.pdf"))],
        },
        Case {
            tool: "CONFLUENCE_SEARCH_PAGES",
            args: json!({"cql": "type = \"page\" AND space = \"DOCS\" AND title ~ \"release\"",
                         "max_results": 25, "expand": "body.storage,version"}),
            method: "GET",
            url_contains: &["/wiki/rest/api/content/search", "cql=", "start=0", "limit=25",
                "expand=body.storage%2Cversion"],
            body_eq: &[],
            response_status: 200,
            response: json!({"results": [{"id": "12345", "type": "page", "title": "Release notes",
                "status": "current", "space": {"key": "DOCS", "name": "Docs"},
                "version": {"number": 4, "when": "2026-02-01T08:30:00.000Z"},
                "_links": {"webui": "/spaces/DOCS/pages/12345"}}],
                "size": 1, "start": 0, "limit": 25, "totalSize": 1}),
            result_eq: &[("/count", json!(1)), ("/results/0/space_key", json!("DOCS")),
                ("/results/0/version_number", json!(4))],
        },
        Case {
            tool: "CONFLUENCE_FIND_USER",
            args: json!({"keyword": "ada@acme.test", "limit": 5}),
            method: "GET",
            url_contains: &["/wiki/rest/api/search/user", "cql=user.fullname", "limit=5"],
            body_eq: &[],
            response_status: 200,
            response: json!({"results": [{"type": "known", "user": {"type": "known",
                "accountId": "557058:1", "userkey": "ada", "username": "ada",
                "displayName": "Ada Lovelace", "email": "ada@acme.test",
                "isExternalCollaborator": false}}], "size": 1, "limit": 5}),
            result_eq: &[("/found", json!(true)), ("/count", json!(1)),
                ("/results/0/display_name", json!("Ada Lovelace")), ("/results/0/email", json!("ada@acme.test"))],
        },
        Case {
            tool: "CONFLUENCE_LIST_PAGE_TEMPLATES",
            args: json!({"space_key": "DOCS"}),
            method: "GET",
            url_contains: &["/wiki/rest/api/template/page", "start=0", "limit=100", "spaceKey=DOCS"],
            body_eq: &[],
            response_status: 200,
            response: json!({"results": [{"templateId": "9f8e7d6c", "name": "Release notes",
                "description": "Notes template", "templateType": "page", "space": "DOCS"}]}),
            result_eq: &[("/count", json!(1)), ("/results/0/template_id", json!("9f8e7d6c")),
                ("/results/0/name", json!("Release notes"))],
        },
        Case {
            tool: "CONFLUENCE_GET_TEMPLATE",
            args: json!({"template_id": "9f8e7d6c"}),
            method: "GET",
            url_contains: &["/wiki/rest/api/template/9f8e7d6c"],
            body_eq: &[],
            response_status: 200,
            response: json!({"templateId": "9f8e7d6c", "name": "Release notes", "description": "Notes template",
                "templateType": "page", "space": "DOCS",
                "body": {"storage": {"representation": "storage",
                    "value": "<p>Version <at:var at:name=\"version\"/></p>"}, "view": {"value": "<p>Version</p>"}}}),
            result_eq: &[("/template_id", json!("9f8e7d6c")), ("/space", json!("DOCS")),
                ("/body_representation", json!("storage"))],
        },
        Case {
            tool: "CONFLUENCE_LIST_FOLDERS",
            args: json!({"homepage_id": "1000"}),
            method: "GET",
            url_contains: &["/wiki/rest/api/content/1000/descendant/folder", "start=0", "limit=100"],
            body_eq: &[],
            response_status: 200,
            response: json!({"results": [{"id": "555", "title": "Archive", "type": "folder"},
                {"id": "556", "title": "Templates", "type": "folder"}]}),
            result_eq: &[("/count", json!(2)), ("/results/0/id", json!("555")),
                ("/results/1/title", json!("Templates"))],
        },
        // --- write ---
        Case {
            tool: "CONFLUENCE_CREATE_PAGE",
            args: json!({"space_id": "4242", "parent_id": "1000", "title": "Roadmap 2026",
                         "body": "<p>Plan</p>", "body_format": "storage", "status": "current"}),
            method: "POST",
            url_contains: &["/wiki/api/v2/pages"],
            body_eq: &[("/spaceId", json!("4242")), ("/parentId", json!("1000")),
                ("/title", json!("Roadmap 2026")), ("/status", json!("current")),
                ("/body/representation", json!("storage")), ("/body/value", json!("<p>Plan</p>"))],
            response_status: 201,
            response: json!({"id": "13000", "title": "Roadmap 2026", "status": "current", "spaceId": "4242",
                "version": {"number": 1}, "body": {"representation": "storage", "value": "<p>Plan</p>"}}),
            result_eq: &[("/id", json!("13000")), ("/title", json!("Roadmap 2026")),
                ("/version_number", json!(1))],
        },
        Case {
            tool: "CONFLUENCE_UPDATE_PAGE",
            args: json!({"page_id": "12345", "current_version": 4, "title": "Release notes",
                         "body": "<p>Shipped 2.1</p>", "body_format": "storage", "status": "draft"}),
            method: "PUT",
            url_contains: &["/wiki/api/v2/pages/12345"],
            body_eq: &[("/id", json!("12345")), ("/status", json!("draft")),
                ("/title", json!("Release notes")), ("/body/value", json!("<p>Shipped 2.1</p>")),
                ("/version/number", json!(5))],
            response_status: 200,
            response: json!({"id": "12345", "title": "Release notes", "status": "draft", "spaceId": "4242",
                "version": {"number": 5}, "body": {"representation": "storage", "value": "<p>Shipped 2.1</p>"}}),
            result_eq: &[("/status", json!("draft")), ("/version_number", json!(5))],
        },
        Case {
            tool: "CONFLUENCE_PUBLISH_DRAFT",
            args: json!({"page_id": "13001", "title": "Draft page", "body": "<p>hi</p>"}),
            method: "PUT",
            url_contains: &["/wiki/api/v2/pages/13001"],
            body_eq: &[("/id", json!("13001")), ("/status", json!("current")),
                ("/title", json!("Draft page")), ("/version/number", json!(1))],
            response_status: 200,
            response: json!({"id": "13001", "title": "Draft page", "status": "current", "spaceId": "4242",
                "version": {"number": 1}, "body": {"representation": "storage", "value": "<p>hi</p>"}}),
            result_eq: &[("/status", json!("current")), ("/title", json!("Draft page"))],
        },
        Case {
            tool: "CONFLUENCE_MOVE_PAGE",
            args: json!({"page_id": "13001", "target_page_id": "12345", "position": "append"}),
            method: "PUT",
            url_contains: &["/wiki/rest/api/content/13001/move/append/12345"],
            body_eq: &[],
            response_status: 200,
            response: Value::Null,
            result_eq: &[("/success", json!(true))],
        },
        Case {
            tool: "CONFLUENCE_DELETE_PAGE",
            args: json!({"page_id": "13001", "purge": true}),
            method: "DELETE",
            url_contains: &["/wiki/api/v2/pages/13001", "purge=true"],
            body_eq: &[],
            response_status: 204,
            response: Value::Null,
            result_eq: &[("/success", json!(true))],
        },
        Case {
            tool: "CONFLUENCE_ADD_COMMENT",
            args: json!({"page_id": "12345", "body": "Thanks!"}),
            method: "POST",
            url_contains: &["/wiki/api/v2/footer-comments"],
            body_eq: &[("/pageId", json!("12345")), ("/body/representation", json!("storage")),
                ("/body/value", json!("Thanks!"))],
            response_status: 201,
            response: json!({"id": "9100", "pageId": "12345", "footerId": "9100", "title": "Thanks!",
                "body": {"representation": "storage", "value": "Thanks!"}}),
            result_eq: &[("/id", json!("9100")), ("/title", json!("Thanks!"))],
        },
        Case {
            tool: "CONFLUENCE_REPLY_TO_COMMENT",
            args: json!({"parent_comment_id": "9001", "body": "<p>Agreed</p>"}),
            method: "POST",
            url_contains: &["/wiki/api/v2/footer-comments"],
            body_eq: &[("/parentCommentId", json!("9001")), ("/body/value", json!("<p>Agreed</p>"))],
            response_status: 201,
            response: json!({"id": "9101", "parentId": "9000", "footerId": "9101",
                "body": {"representation": "storage", "value": "<p>Agreed</p>"}}),
            result_eq: &[("/id", json!("9101")), ("/parent_id", json!("9000"))],
        },
        Case {
            tool: "CONFLUENCE_ADD_LABEL",
            args: json!({"page_id": "12345", "labels": ["release", "docs"], "prefix": "global"}),
            method: "POST",
            url_contains: &["/wiki/rest/api/content/12345/label"],
            body_eq: &[("/0", json!({"prefix": "global", "name": "release"})),
                ("/1", json!({"prefix": "global", "name": "docs"}))],
            response_status: 200,
            response: json!({"results": [{"prefix": "global", "name": "release", "label": "123456"},
                {"prefix": "global", "name": "docs", "label": "123457"}]}),
            result_eq: &[("/count", json!(2)), ("/results/0/name", json!("release")),
                ("/results/1/id", json!("123457"))],
        },
        Case {
            tool: "CONFLUENCE_REMOVE_LABEL",
            args: json!({"page_id": "12345", "label": "docs"}),
            method: "DELETE",
            url_contains: &["/wiki/rest/api/content/12345/label", "name=docs"],
            body_eq: &[],
            response_status: 204,
            response: Value::Null,
            result_eq: &[("/success", json!(true))],
        },
    ];
    assert_full_coverage(&confluence, &cases);
    for c in &cases {
        support::run(&confluence, c);
    }
}

/// Live smoke test: read-only calls against the real site through the hub exec path.
/// `CTOX_TOOLHUB_LIVE_CONFLUENCE_TOKEN` must hold the basic-auth credential of the
/// piece, i.e. `account-email:api-token`; it is never printed. The call is skipped
/// silently when the variable is unset. Note that the built-in adapter is wired to the
/// placeholder site of [`Confluence::default`] — a live run therefore needs the token's
/// account to live on `https://your-domain.atlassian.net`, as for the other site-bound
/// toolkits (jira).
#[test]
#[ignore]
fn live_read_only() {
    for (tool, args) in [
        ("CONFLUENCE_LIST_SPACES", json!({})),
        (
            "CONFLUENCE_FIND_PAGE_BY_TITLE",
            json!({"title": "Release notes"}),
        ),
        (
            "CONFLUENCE_SEARCH_PAGES",
            json!({"cql": "type = \"page\" ORDER BY lastmodified DESC", "max_results": 3}),
        ),
    ] {
        if let Some(r) = support::live_exec(SLUG, AuthSchemeKind::Basic, tool, args) {
            let out = r.unwrap_or_else(|e| panic!("live call {tool} failed: {e}"));
            assert!(!out.is_null(), "{tool}: empty result");
        }
    }
}
