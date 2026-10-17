//! `draft-spec --from-openapi`: generator on two real public OpenAPI documents
//! (bund.dev Autobahn App API and PEGELONLINE, both keyless, fixtures copied
//! 2026-10-17 from github.com/bundesAPI).
use ctox_toolhub::spec::openapi::{draft_from_openapi, parse_document, Options};
use ctox_toolhub::spec::SpecAdapter;
use ctox_toolhub::toolkits::ToolkitAdapter;
use serde_json::{json, Value};

const AUTOBAHN: &str = include_str!("fixtures/openapi/autobahn.yaml");
const PEGEL: &str = include_str!("fixtures/openapi/pegel.yaml");

fn opts(src: &str) -> Options {
    Options { source: src.into(), ..Default::default() }
}

fn adapter(spec: &Value) -> SpecAdapter {
    SpecAdapter::from_json(&spec.to_string()).unwrap_or_else(|e| panic!("draft does not validate: {e:#}\n{spec:#}"))
}

#[test]
fn autobahn_document_maps_to_a_valid_keyless_spec_with_probes() {
    let d = draft_from_openapi(&parse_document(AUTOBAHN).unwrap(), &opts("https://autobahn.api.bund.dev/openapi.yaml")).unwrap();
    let a = adapter(&d.spec);
    assert_eq!(d.spec["slug"], json!("autobahn_app"));
    assert_eq!(d.spec["api_base"], json!("https://verkehr.autobahn.de/o/autobahn"));
    assert_eq!(d.spec["allowed_hosts"], json!(["verkehr.autobahn.de"]));
    assert_eq!(d.auth["scheme"], json!({"type": "none"}));
    let tools: Vec<&str> = a.spec().tools.iter().map(|t| t.slug.as_str()).collect();
    assert_eq!(tools.len(), 13, "{tools:?}");
    assert!(tools.contains(&"AUTOBAHN_APP_LIST_ROADWORKS") && tools.contains(&"AUTOBAHN_APP_GET_WEBCAM"));
    assert!(d.spec["actions"].as_array().unwrap().iter().all(|x| x["risk"] == json!("read")));
    // Every read got a probe from the doc's examples.
    assert_eq!(d.probes.len(), 13);
    assert!(d.probes_missing.is_empty() && d.skipped.is_empty());
    let p = d.probes.iter().find(|p| p["tool"] == json!("AUTOBAHN_APP_LIST_ROADWORKS")).unwrap();
    assert_eq!(p["args"], json!({"roadId": "A1"}));
    assert_eq!(p["expect"]["kind"], json!("object"));
    // Detail probes are chained to the list probes (examples go stale).
    let g = d.probes.iter().find(|p| p["tool"] == json!("AUTOBAHN_APP_GET_ROADWORK")).unwrap();
    assert_eq!(g["from"], json!({"roadworkId": "AUTOBAHN_APP_LIST_ROADWORKS#/roadworks/0/identifier"}), "{g}");
    assert_eq!(g["args"], json!({}));
    assert_eq!(d.probes.iter().filter(|p| p.get("from").is_some()).count(), 6);
    let pos = |t: &str| d.probes.iter().position(|p| p["tool"] == json!(t)).unwrap();
    assert!(pos("AUTOBAHN_APP_LIST_ROADWORKS") < pos("AUTOBAHN_APP_GET_ROADWORK"));
    let r = a.build_request("AUTOBAHN_APP_LIST_ROADWORKS", &p["args"]).unwrap();
    assert_eq!(r.url, "https://verkehr.autobahn.de/o/autobahn/A1/services/roadworks");
    assert_eq!(a.build_request("AUTOBAHN_APP_LIST_AUTOBAHNEN", &json!({})).unwrap().url, "https://verkehr.autobahn.de/o/autobahn/");
    // Path values are encoded, never traversal.
    assert!(a.build_request("AUTOBAHN_APP_GET_WEBCAM", &json!({"webcamId": ".."})).is_err());
    assert!(a.build_request("AUTOBAHN_APP_GET_WEBCAM", &json!({"webcamId": "a/b="})).unwrap().url.ends_with("/details/webcam/a%2Fb%3D"));
}

#[test]
fn pegelonline_document_skips_binary_and_maps_query_types() {
    let d = draft_from_openapi(&parse_document(PEGEL).unwrap(), &opts("https://pegelonline.api.bund.dev/openapi.yaml")).unwrap();
    let a = adapter(&d.spec);
    assert_eq!(d.spec["slug"], json!("pegel_online"));
    assert_eq!(d.spec["allowed_hosts"], json!(["www.pegelonline.wsv.de"]));
    assert_eq!(a.spec().tools.len(), 5);
    assert_eq!(d.skipped.len(), 1);
    assert!(d.skipped[0]["reason"].as_str().unwrap().contains("binary"), "{:?}", d.skipped);
    let r = a.build_request("PEGEL_ONLINE_GET_STATIONS", &json!({"waters": ["RHEIN", "ELBE"], "latitude": 52.52, "includeTimeseries": true})).unwrap();
    assert!(r.url.starts_with("https://www.pegelonline.wsv.de/webservices/rest-api/v2/stations.json?"), "{}", r.url);
    assert!(r.url.contains("waters=RHEIN%2CELBE") && r.url.contains("latitude=52.52") && r.url.contains("includeTimeseries=true"), "{}", r.url);
    // Non-empty example preferred over the doc's "no filter" example.
    let p = d.probes.iter().find(|p| p["tool"] == json!("PEGEL_ONLINE_GET_STATIONS_BY_ID")).unwrap();
    // Chained to the station list instead of the doc's example `EITZE`.
    assert_eq!(p["from"]["station"].as_str().map(|s| s.starts_with("PEGEL_ONLINE_GET_STATIONS#/0/")), Some(true), "{p}");
    assert_eq!(d.probes.len(), 5);
    assert_eq!(d.probes[0]["expect"]["kind"], json!("array"));
}

#[test]
fn synthetic_swagger2_auth_bodies_and_risks() {
    let doc = json!({
        "swagger": "2.0", "info": {"title": "Acme Shop API"}, "host": "api.acme.io", "basePath": "/v1/", "schemes": ["http"],
        "securityDefinitions": {"key": {"type": "apiKey", "in": "header", "name": "X-Acme-Key"}}, "security": [{"key": []}],
        "paths": {
            "/orders": {
                "get": {"operationId": "listOrders", "summary": "List orders", "parameters": [{"in": "query", "name": "limit", "type": "integer"}], "responses": {"200": {"description": "ok", "schema": {"type": "array", "items": {"type": "object"}}}}},
                "post": {"operationId": "createOrder", "parameters": [{"in": "body", "name": "body", "schema": {"type": "object", "required": ["sku"], "properties": {"sku": {"type": "string"}, "qty": {"type": "integer"}, "id": {"type": "string", "readOnly": true}}}}], "responses": {"201": {"description": "created"}}}
            },
            "/orders/{id}": {
                "parameters": [{"in": "path", "name": "id", "required": true, "type": "string"}],
                "get": {"operationId": "getOrder", "responses": {"200": {"description": "ok"}}},
                "delete": {"operationId": "deleteOrder", "responses": {"204": {"description": "gone"}}}
            },
            "/upload": {"post": {"operationId": "upload", "consumes": ["multipart/form-data"], "parameters": [{"in": "formData", "name": "file", "type": "file"}], "responses": {"200": {"description": "ok"}}}},
            "/secure": {"get": {"operationId": "secure", "parameters": [{"in": "header", "name": "X-Tenant", "required": true, "type": "string"}], "responses": {"200": {"description": "ok"}}}}
        }
    });
    let d = draft_from_openapi(&doc, &Options::default()).unwrap();
    let a = adapter(&d.spec);
    assert_eq!(d.spec["slug"], json!("acme_shop"));
    assert_eq!(d.spec["api_base"], json!("https://api.acme.io/v1"));
    assert!(d.notes.iter().any(|n| n.contains("http; the spec uses https")));
    assert_eq!(d.spec["auth_schemes"], json!([{"type": "api_key", "header_name": "X-Acme-Key"}]));
    let risk = |t: &str| d.spec["actions"].as_array().unwrap().iter().find(|x| x["slug"] == json!(t)).map(|x| x["risk"].clone());
    assert_eq!(risk("ACME_SHOP_LIST_ORDERS"), Some(json!("read")));
    assert_eq!(risk("ACME_SHOP_CREATE_ORDER"), Some(json!("write")));
    assert_eq!(risk("ACME_SHOP_DELETE_ORDER"), Some(json!("destructive")));
    assert_eq!(risk("ACME_SHOP_UPLOAD"), None);
    assert_eq!(risk("ACME_SHOP_SECURE"), None);
    assert_eq!(d.skipped.len(), 2, "{:?}", d.skipped);
    let r = a.build_request("ACME_SHOP_CREATE_ORDER", &json!({"sku": "x1", "qty": 2})).unwrap();
    assert_eq!(serde_json::from_slice::<Value>(r.body.as_ref().unwrap()).unwrap(), json!({"sku": "x1", "qty": 2}));
    assert!(a.build_request("ACME_SHOP_CREATE_ORDER", &json!({"qty": 2})).is_err(), "sku is required");
    // Reads first; getOrder has no example for `id`, so no probe but a hint.
    assert_eq!(d.probes_missing, vec![json!({"tool": "ACME_SHOP_GET_ORDER", "needs": ["id"]})]);
    assert_eq!(d.probes, vec![json!({"tool": "ACME_SHOP_LIST_ORDERS", "args": {}, "expect": {"kind": "array"}})]);
    // API keys in the query are refused, not silently dropped.
    let mut q = doc.clone();
    q["securityDefinitions"]["key"]["in"] = json!("query");
    let d = draft_from_openapi(&q, &Options::default()).unwrap();
    assert_eq!(d.auth["unsupported"], json!(true));
    assert!(d.notes.iter().any(|n| n.contains("keys never go into URLs")));
}

#[test]
fn not_openapi_is_refused() {
    assert!(parse_document("<html></html>").is_err());
    assert!(parse_document("{\"hello\": 1}").is_err());
}
