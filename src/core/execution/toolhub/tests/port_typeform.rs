//! Typeform port: one recorded case per action. Every expectation is the request the
//! Activepieces `run()` code makes (`typeformCommon.typeformRequest` in
//! `src/lib/common/index.ts` and the action files under `src/lib/actions`).
mod support;

use ctox_toolhub::toolkits::typeform::Typeform;
use serde_json::{json, Value};
use support::{live, run, Case};

const BASE: &str = "https://typeform.test";

const FORM: &str = "01H9F3K2AB";
const THEME: &str = "kQ9mA7Xc";
const WORKSPACE: &str = "Ns8vQ2Lp";
const ACCOUNT: &str = "3n6Xq1Zb";
const IMAGE: &str = "mZ4pR8tY";
const RESPONSE: &str = "3a9xQ0bB1c";

const ME_URL: &str = "https://typeform.test/me";
const FORMS_URL: &str = "https://typeform.test/forms";
const FORMS_Q_URL: &str = "https://typeform.test/forms?";
const FORM_URL: &str = "https://typeform.test/forms/01H9F3K2AB";
const FORM_MESSAGES_URL: &str = "https://typeform.test/forms/01H9F3K2AB/messages";
const FORM_RESPONSES_Q_URL: &str = "https://typeform.test/forms/01H9F3K2AB/responses?";
const THEMES_URL: &str = "https://typeform.test/themes";
const THEMES_Q_URL: &str = "https://typeform.test/themes?";
const THEME_URL: &str = "https://typeform.test/themes/kQ9mA7Xc";
const WORKSPACES_URL: &str = "https://typeform.test/workspaces";
const WORKSPACES_Q_URL: &str = "https://typeform.test/workspaces?";
const ACCOUNT_WORKSPACES_URL: &str = "https://typeform.test/accounts/3n6Xq1Zb/workspaces";
const WORKSPACE_URL: &str = "https://typeform.test/workspaces/Ns8vQ2Lp";
const IMAGES_URL: &str = "https://typeform.test/images";
const IMAGE_URL: &str = "https://typeform.test/images/mZ4pR8tY";
const WS_ID_Q: &str = "workspace_id=Ns8vQ2Lp";
const RESPONSE_IDS_Q: &str = "included_response_ids=3a9xQ0bB1c%2C4b0yR1cD2d";

fn adapter() -> Typeform {
    Typeform::with_api_base(BASE)
}

/// `&[(ptr, value)]` for a `Case` field: leaked so it lives as long as the case.
fn ptrs<const N: usize>(pairs: [(&'static str, Value); N]) -> &'static [(&'static str, Value)] {
    Box::leak(Box::new(pairs))
}

fn form_record() -> Value {
    json!({
        "id": FORM,
        "title": "Kundenzufriedenheit Q3",
        "created_at": "2026-09-01T10:00:00.000Z",
        "last_updated_at": "2026-10-02T08:30:00.000Z",
        "published_at": "2026-09-05T09:00:00.000Z",
        "is_trial": false,
        "settings": {
            "is_public": true,
            "language": "de",
            "progress_bar": "percentage",
            "meta": {"allow_indexing": false},
            "enrichment_in_renderer": {"active": true, "version": "2"}
        },
        "theme": {"href": format!("{BASE}/themes/{THEME}")},
        "workspace": {"href": format!("{BASE}/workspaces/{WORKSPACE}")},
        "welcome_screens": [{"ref": "welcome", "title": "Willkommen"}],
        "thankyou_screens": [{"ref": "thanks", "title": "Danke"}],
        "logic": [],
        "variables": [],
        "hidden": ["utm_source"],
        "fields": [
            {"id": "field-1", "ref": "zufriedenheit", "title": "Wie zufrieden sind Sie?",
             "type": "multiple_choice", "validations": {"required": true},
             "properties": {"choices": [{"id": "ch-1", "ref": "sehr_gut", "label": "Sehr gut"},
                                          {"id": "ch-2", "ref": "gut", "label": "Gut"}]}},
            {"id": "field-2", "ref": "kommentar", "title": "Anmerkungen", "type": "long_text",
             "validations": {"required": false}, "properties": {}}
        ],
        "_links": {"display": "https://form.typeform.com/to/AbCd12"}
    })
}

#[test]
fn typeform_actions_are_ported() {
    let a = adapter();
    let cases: Vec<Case> = vec![
        // AP: src/lib/actions/get-me.ts -> GET /me
        Case {
            tool: "TYPEFORM_GET_ME",
            args: json!({}),
            method: "GET",
            url_contains: &[ME_URL],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({
                "alias": "Acme Feedback GmbH",
                "email": "ops@acme.de",
                "language": "de"
            }),
            result_eq: ptrs([
                ("/alias", json!("Acme Feedback GmbH")),
                ("/email", json!("ops@acme.de")),
            ]),
        },
        // AP: src/lib/actions/list-forms.ts -> GET /forms?search&workspace_id&sort_by&order_by&page&page_size
        Case {
            tool: "TYPEFORM_LIST_FORMS",
            args: json!({"search": "Q3", "workspace": WORKSPACE, "sort_by": "last_updated_at",
                        "order_by": "desc", "page": 2, "page_size": 50}),
            method: "GET",
            url_contains: &[
                FORMS_Q_URL,
                "search=Q3",
                WS_ID_Q,
                "sort_by=last_updated_at",
                "order_by=desc",
                "page=2",
                "page_size=50",
            ],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({
                "total_items": 2,
                "page_count": 2,
                "items": [
                    {"id": FORM, "title": "Kundenzufriedenheit Q3", "is_public": true,
                     "created_at": "2026-09-01T10:00:00.000Z",
                     "last_updated_at": "2026-10-02T08:30:00.000Z",
                     "_links": {"display": "https://form.typeform.com/to/AbCd12"}},
                    {"id": "02J8G4L3CD", "title": "Onboarding Survey Q3", "is_public": false,
                     "created_at": "2026-08-20T07:15:00.000Z",
                     "last_updated_at": "2026-09-30T16:45:00.000Z",
                     "_links": {"display": "https://form.typeform.com/to/EfGh34"}}
                ]
            }),
            result_eq: ptrs([
                ("/total_items", json!(2)),
                ("/page_count", json!(2)),
                ("/items/0/id", json!(FORM)),
                ("/items/1/title", json!("Onboarding Survey Q3")),
            ]),
        },
        // AP: src/lib/common/index.ts getForm() -> GET /forms/{form_id}
        Case {
            tool: "TYPEFORM_GET_FORM",
            args: json!({"form_id": FORM}),
            method: "GET",
            url_contains: &[FORM_URL],
            body_eq: ptrs([]),
            response_status: 200,
            response: form_record(),
            result_eq: ptrs([
                ("/id", json!(FORM)),
                ("/fields/0/ref", json!("zufriedenheit")),
                ("/fields/0/properties/choices/1/label", json!("Gut")),
                ("/settings/is_public", json!(true)),
                ("/hidden/0", json!("utm_source")),
            ]),
        },
        // AP: src/lib/actions/get-form-messages.ts -> GET /forms/{form_id}/messages
        Case {
            tool: "TYPEFORM_GET_FORM_MESSAGES",
            args: json!({"form_id": FORM}),
            method: "GET",
            url_contains: &[FORM_MESSAGES_URL],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({
                "label.submitBtn.default": "Weiter",
                "label.backBtn.default": "Zurück",
                "error.validation.email": "Bitte eine gültige E-Mail-Adresse eingeben."
            }),
            result_eq: ptrs([
                ("/label.submitBtn.default", json!("Weiter")),
                ("/error.validation.email", json!("Bitte eine gültige E-Mail-Adresse eingeben.")),
            ]),
        },
        // AP: src/lib/actions/list-responses.ts -> GET /forms/{form_id}/responses?...
        Case {
            tool: "TYPEFORM_LIST_RESPONSES",
            args: json!({"form_id": FORM, "query": "Safari", "response_type": "completed",
                        "included_response_ids": [RESPONSE, "4b0yR1cD2d"], "sort": "submitted_at,desc",
                        "since": "2026-09-01T00:00:00Z", "until": "2026-10-01T00:00:00Z",
                        "page_size": 25}),
            method: "GET",
            url_contains: &[
                FORM_RESPONSES_Q_URL,
                "query=Safari",
                "response_type=completed",
                RESPONSE_IDS_Q,
                "sort=submitted_at%2Cdesc",
                "since=2026-09-01T00%3A00%3A00Z",
                "until=2026-10-01T00%3A00%3A00Z",
                "page_size=25",
            ],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({
                "total_items": 1,
                "page_count": 1,
                "items": [
                    {
                        "form_id": FORM,
                        "token": RESPONSE,
                        "landed_at": "2026-09-14T12:00:00.000Z",
                        "submitted_at": "2026-09-14T12:04:31.000Z",
                        "calculated_at": "2026-09-14T12:04:35.000Z",
                        "response_type": "completed",
                        "hidden": {"utm_source": "newsletter"},
                        "answers": [
                            {"type": "choice", "field": {"id": "field-1", "type": "multiple_choice", "ref": "zufriedenheit"},
                             "choice": {"id": "ch-2", "label": "Gut", "other": false}},
                            {"type": "text", "field": {"id": "field-2", "type": "long_text", "ref": "kommentar"},
                             "text": "Läuft auf Safari sehr schnell"}
                        ]
                    }
                ]
            }),
            result_eq: ptrs([
                ("/total_items", json!(1)),
                ("/items/0/token", json!(RESPONSE)),
                ("/items/0/answers/1/text", json!("Läuft auf Safari sehr schnell")),
                ("/items/0/hidden/utm_source", json!("newsletter")),
            ]),
        },
        // AP: src/lib/actions/list-themes.ts -> GET /themes?page&page_size
        Case {
            tool: "TYPEFORM_LIST_THEMES",
            args: json!({"page": 1, "page_size": 200}),
            method: "GET",
            url_contains: &[THEMES_Q_URL, "page=1", "page_size=200"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({
                "total_items": 1,
                "page_count": 1,
                "items": [
                    {"id": THEME, "name": "Acme Brand", "font": "Montserrat", "visibility": "private",
                     "colors": {"question": "#3D3D3D", "answer": "#3D3D3D", "button": "#4A4A4A", "background": "#FFFFFF"},
                     "fields": {"alignment": "left", "font_size": "medium"},
                     "screens": {"alignment": "center", "font_size": "large"}}
                ]
            }),
            result_eq: ptrs([
                ("/items/0/id", json!(THEME)),
                ("/items/0/visibility", json!("private")),
                ("/items/0/screens/alignment", json!("center")),
            ]),
        },
        // AP: src/lib/actions/get-theme.ts -> GET /themes/{themeId}
        Case {
            tool: "TYPEFORM_GET_THEME",
            args: json!({"theme_id": THEME}),
            method: "GET",
            url_contains: &[THEME_URL],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({
                "id": THEME, "name": "Acme Brand", "font": "Montserrat", "visibility": "private",
                "background": {"type": "color", "href": "#FFFFFF"},
                "colors": {"question": "#3D3D3D", "answer": "#3D3D3D", "button": "#4A4A4A", "background": "#FFFFFF"},
                "fields": {"alignment": "left", "font_size": "medium"},
                "screens": {"alignment": "center", "font_size": "large"}
            }),
            result_eq: ptrs([
                ("/id", json!(THEME)),
                ("/font", json!("Montserrat")),
                ("/colors/button", json!("#4A4A4A")),
            ]),
        },
        // AP: src/lib/actions/list-workspaces.ts (no accountId) -> GET /workspaces?search&page&page_size
        Case {
            tool: "TYPEFORM_LIST_WORKSPACES",
            args: json!({"search": "Marketing", "page": 1, "page_size": 200}),
            method: "GET",
            url_contains: &[WORKSPACES_Q_URL, "search=Marketing", "page=1", "page_size=200"],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({
                "total_items": 1,
                "page_count": 1,
                "items": [
                    {"id": WORKSPACE, "name": "Marketing", "account_id": ACCOUNT, "default": false,
                     "shared": true, "forms": {"count": 3}}
                ]
            }),
            result_eq: ptrs([
                ("/items/0/id", json!(WORKSPACE)),
                ("/items/0/account_id", json!(ACCOUNT)),
                ("/items/0/forms/count", json!(3)),
            ]),
        },
        // AP: src/lib/actions/list-workspaces.ts (accountId set) -> GET /accounts/{accountId}/workspaces
        Case {
            tool: "TYPEFORM_LIST_ACCOUNT_WORKSPACES",
            args: json!({"account_id": ACCOUNT}),
            method: "GET",
            url_contains: &[ACCOUNT_WORKSPACES_URL],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({
                "total_items": 1,
                "page_count": 1,
                "items": [
                    {"id": WORKSPACE, "name": "Marketing", "account_id": ACCOUNT, "default": true,
                     "shared": false, "forms": {"count": 3}}
                ]
            }),
            result_eq: ptrs([
                ("/items/0/default", json!(true)),
                ("/items/0/shared", json!(false)),
            ]),
        },
        // AP: src/lib/actions/get-workspace.ts -> GET /workspaces/{workspace}
        Case {
            tool: "TYPEFORM_GET_WORKSPACE",
            args: json!({"workspace_id": WORKSPACE}),
            method: "GET",
            url_contains: &[WORKSPACE_URL],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({
                "id": WORKSPACE, "name": "Marketing", "account_id": ACCOUNT, "default": false,
                "shared": true, "forms": {"count": 3},
                "members": [
                    {"email": "ops@acme.de", "role": "owner"},
                    {"email": "marketing@acme.de", "role": "co-owner"}
                ],
                "_links": {"display": "https://admin.typeform.com/form/workspace/abc"}
            }),
            result_eq: ptrs([
                ("/id", json!(WORKSPACE)),
                ("/members/0/role", json!("owner")),
                ("/members/1/email", json!("marketing@acme.de")),
            ]),
        },
        // AP: src/lib/actions/list-images.ts -> GET /images
        Case {
            tool: "TYPEFORM_LIST_IMAGES",
            args: json!({}),
            method: "GET",
            url_contains: &[IMAGES_URL],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!([
                {"id": IMAGE, "file_name": "logo.png", "content_type": "image/png",
                 "width": 512, "height": 512, "created_at": "2026-08-01T09:00:00.000Z",
                 "src": "https://embed.typeform.com/next/images/mZ4pR8tY/logo.png"}
            ]),
            result_eq: ptrs([
                ("/count", json!(1)),
                ("/total_items", json!(1)),
                ("/items/0/file_name", json!("logo.png")),
            ]),
        },
        // AP: src/lib/actions/get-image.ts -> GET /images/{imageId}
        Case {
            tool: "TYPEFORM_GET_IMAGE",
            args: json!({"image_id": IMAGE}),
            method: "GET",
            url_contains: &[IMAGE_URL],
            body_eq: ptrs([]),
            response_status: 200,
            response: json!({
                "id": IMAGE, "file_name": "logo.png", "content_type": "image/png",
                "width": 512, "height": 512,
                "src": "https://embed.typeform.com/next/images/mZ4pR8tY/logo.png"
            }),
            result_eq: ptrs([
                ("/id", json!(IMAGE)),
                ("/content_type", json!("image/png")),
                ("/width", json!(512)),
            ]),
        },
        // AP: src/lib/actions/create-form.ts -> POST /forms
        Case {
            tool: "TYPEFORM_CREATE_FORM",
            args: json!({"title": "Neue Umfrage", "workspace": WORKSPACE, "theme": THEME,
                        "fields": [{"title": "Ihr Name", "type": "short_text", "ref": "name"}]}),
            method: "POST",
            url_contains: &[FORMS_URL],
            body_eq: ptrs([
                ("/title", json!("Neue Umfrage")),
                ("/workspace/href", json!(format!("{BASE}/workspaces/{WORKSPACE}"))),
                ("/theme/href", json!(format!("{BASE}/themes/{THEME}"))),
                ("/fields/0/ref", json!("name")),
            ]),
            response_status: 201,
            response: json!({
                "id": "03K7H5M4EF", "title": "Neue Umfrage", "created_at": "2026-10-14T10:00:00.000Z",
                "theme": {"href": format!("{BASE}/themes/{THEME}")},
                "workspace": {"href": format!("{BASE}/workspaces/{WORKSPACE}")},
                "fields": [{"title": "Ihr Name", "type": "short_text", "ref": "name"}],
                "_links": {"display": "https://form.typeform.com/to/ZzYy33"}
            }),
            result_eq: ptrs([
                ("/id", json!("03K7H5M4EF")),
                ("/title", json!("Neue Umfrage")),
            ]),
        },
        // AP: src/lib/actions/update-form.ts -> PATCH /forms/{form_id} with JSON-Patch operations
        Case {
            tool: "TYPEFORM_UPDATE_FORM",
            args: json!({"form_id": FORM, "title": "Kundenzufriedenheit Q4", "is_public": "false",
                        "workspace": WORKSPACE}),
            method: "PATCH",
            url_contains: &[FORM_URL],
            body_eq: ptrs([
                ("/0/op", json!("replace")),
                ("/0/path", json!("/title")),
                ("/0/value", json!("Kundenzufriedenheit Q4")),
                ("/1/path", json!("/settings/is_public")),
                ("/1/value", json!(false)),
                ("/2/path", json!("/workspace")),
                ("/2/value/href", json!(format!("{BASE}/workspaces/{WORKSPACE}"))),
            ]),
            response_status: 200,
            response: json!({"id": FORM, "title": "Kundenzufriedenheit Q4"}),
            result_eq: ptrs([
                ("/id", json!(FORM)),
                ("/title", json!("Kundenzufriedenheit Q4")),
            ]),
        },
        // AP: src/lib/actions/replace-form.ts -> PUT /forms/{form_id} with `toFormPayload(definition)`
        Case {
            tool: "TYPEFORM_REPLACE_FORM",
            args: json!({"form_id": FORM, "definition": form_record()}),
            method: "PUT",
            url_contains: &[FORM_URL],
            body_eq: ptrs([
                ("/title", json!("Kundenzufriedenheit Q3")),
                ("/theme/href", json!(format!("{BASE}/themes/{THEME}"))),
                ("/fields/0/id", json!("field-1")),
                ("/settings/enrichment_in_renderer/version", json!("2")),
                ("/settings/is_public", json!(true)),
            ]),
            response_status: 200,
            response: form_record(),
            result_eq: ptrs([("/id", json!(FORM)), ("/fields/1/ref", json!("kommentar"))]),
        },
        // AP: src/lib/actions/delete-form.ts -> DELETE /forms/{form_id}
        Case {
            tool: "TYPEFORM_DELETE_FORM",
            args: json!({"form_id": FORM}),
            method: "DELETE",
            url_contains: &[FORM_URL],
            body_eq: ptrs([]),
            response_status: 204,
            response: Value::Null,
            result_eq: ptrs([("/deleted", json!(true))]),
        },
        // AP: src/lib/actions/create-theme.ts -> POST /themes (themeBody)
        Case {
            tool: "TYPEFORM_CREATE_THEME",
            args: json!({"name": "Acme Brand", "question_color": "#3D3D3D", "answer_color": "#4D4D4D",
                        "button_color": "#4A4A4A", "background_color": "#FFFFFF", "font": "Montserrat",
                        "screen_alignment": "center"}),
            method: "POST",
            url_contains: &[THEMES_URL],
            body_eq: ptrs([
                ("/name", json!("Acme Brand")),
                ("/font", json!("Montserrat")),
                ("/colors/question", json!("#3D3D3D")),
                ("/colors/background", json!("#FFFFFF")),
                ("/fields/alignment", json!("left")),
                ("/fields/font_size", json!("medium")),
                ("/screens/alignment", json!("center")),
                ("/screens/font_size", json!("medium")),
            ]),
            response_status: 201,
            response: json!({
                "id": "pL2nB6yE", "name": "Acme Brand", "font": "Montserrat", "visibility": "private",
                "colors": {"question": "#3D3D3D", "answer": "#4D4D4D", "button": "#4A4A4A", "background": "#FFFFFF"},
                "fields": {"alignment": "left", "font_size": "medium"},
                "screens": {"alignment": "center", "font_size": "medium"}
            }),
            result_eq: ptrs([
                ("/id", json!("pL2nB6yE")),
                ("/visibility", json!("private")),
            ]),
        },
        // AP: src/lib/actions/create-workspace.ts (no accountId) -> POST /workspaces
        Case {
            tool: "TYPEFORM_CREATE_WORKSPACE",
            args: json!({"name": "Support"}),
            method: "POST",
            url_contains: &[WORKSPACES_URL],
            body_eq: ptrs([("/name", json!("Support"))]),
            response_status: 201,
            response: json!({
                "id": "tR9dF3xW", "name": "Support", "account_id": ACCOUNT, "default": false,
                "shared": false, "forms": {"count": 0}
            }),
            result_eq: ptrs([("/id", json!("tR9dF3xW")), ("/account_id", json!(ACCOUNT))]),
        },
        // AP: src/lib/actions/create-workspace.ts (accountId set) -> POST /accounts/{accountId}/workspaces
        Case {
            tool: "TYPEFORM_CREATE_ACCOUNT_WORKSPACE",
            args: json!({"account_id": ACCOUNT, "name": "Support DE"}),
            method: "POST",
            url_contains: &[ACCOUNT_WORKSPACES_URL],
            body_eq: ptrs([("/name", json!("Support DE"))]),
            response_status: 201,
            response: json!({
                "id": "uS1gH5aV", "name": "Support DE", "account_id": ACCOUNT, "default": false,
                "shared": false, "forms": {"count": 0}
            }),
            result_eq: ptrs([("/id", json!("uS1gH5aV")), ("/name", json!("Support DE"))]),
        },
        // AP: src/lib/actions/update-workspace.ts -> PATCH /workspaces/{workspace}
        Case {
            tool: "TYPEFORM_UPDATE_WORKSPACE",
            args: json!({"workspace_id": WORKSPACE, "name": "Marketing DACH"}),
            method: "PATCH",
            url_contains: &[WORKSPACE_URL],
            body_eq: ptrs([
                ("/0/op", json!("replace")),
                ("/0/path", json!("/name")),
                ("/0/value", json!("Marketing DACH")),
            ]),
            response_status: 200,
            response: json!({
                "id": WORKSPACE, "name": "Marketing DACH", "account_id": ACCOUNT, "default": false,
                "shared": true, "forms": {"count": 3}
            }),
            result_eq: ptrs([("/id", json!(WORKSPACE)), ("/name", json!("Marketing DACH"))]),
        },
        // AP: src/lib/actions/delete-image.ts -> DELETE /images/{imageId}
        Case {
            tool: "TYPEFORM_DELETE_IMAGE",
            args: json!({"image_id": IMAGE}),
            method: "DELETE",
            url_contains: &[IMAGE_URL],
            body_eq: ptrs([]),
            response_status: 204,
            response: Value::Null,
            result_eq: ptrs([("/deleted", json!(true))]),
        },
        // AP: src/lib/actions/delete-responses.ts -> DELETE /forms/{form_id}/responses?included_response_ids=
        Case {
            tool: "TYPEFORM_DELETE_RESPONSES",
            args: json!({"form_id": FORM, "response_ids": [RESPONSE, "4b0yR1cD2d"]}),
            method: "DELETE",
            url_contains: &[
                FORM_RESPONSES_Q_URL,
                RESPONSE_IDS_Q,
            ],
            body_eq: ptrs([]),
            response_status: 204,
            response: Value::Null,
            result_eq: ptrs([("/deleted", json!(true))]),
        },
    ];

    support::assert_full_coverage(&a, &cases);
    for c in &cases {
        run(&a, c);
    }
}

#[test]
fn empty_response_bodies_and_rejects_are_handled() {
    use ctox_toolhub::http::HttpResponse;
    use ctox_toolhub::toolkits::ToolkitAdapter;

    let a = adapter();
    // Empty list bodies must not panic and must still produce a page shape.
    let resp = HttpResponse {
        status: 200,
        body: serde_json::to_vec(&json!({"total_items": 0, "page_count": 0})).unwrap(),
        session_id: None,
    };
    let out = a.parse_response("TYPEFORM_LIST_FORMS", &resp).unwrap();
    assert_eq!(out["total_items"], 0);

    let resp = HttpResponse {
        status: 200,
        body: serde_json::to_vec(&json!([])).unwrap(),
        session_id: None,
    };
    let out = a.parse_response("TYPEFORM_LIST_IMAGES", &resp).unwrap();
    assert_eq!(out["count"], 0);

    // Path traversal and unknown tools are rejected before any request is built.
    assert!(a
        .build_request("TYPEFORM_GET_FORM", &json!({"form_id": "../../secrets"}))
        .is_err());
    assert!(a.build_request("TYPEFORM_NOPE", &json!({})).is_err());
}

/// Live read-only calls through the hub exec path (OAuth2 access token as bearer).
#[test]
#[ignore]
fn live_read_only_actions() {
    let a = adapter();
    for (tool, args) in [
        ("TYPEFORM_GET_ME", json!({})),
        ("TYPEFORM_LIST_FORMS", json!({"page_size": 1})),
        ("TYPEFORM_LIST_THEMES", json!({"page_size": 1})),
    ] {
        let Some(result) = live(&a, tool, args) else { return };
        let v = result.unwrap_or_else(|e| panic!("{tool}: {e}"));
        assert!(v.is_object(), "{tool}: expected an object, got {v}");
    }
}