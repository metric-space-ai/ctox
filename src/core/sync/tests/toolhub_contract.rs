//! Wire round-trip for the toolhub contract types (generated from
//! `src/core/rxdb/tests/fixtures/ctox_execution_contract.json`).
use ctox_sync::contracts::*;
use serde_json::json;

#[test]
fn toolhub_events_roundtrip_camel_case() {
    let event = ToolhubEvent::CredentialRequired {
        toolkit: "github".into(),
        session_id: "s1".into(),
        call_id: "c1".into(),
        reason: ToolhubLocalizedText {
            de: "Verbinden".into(),
            en: "Connect".into(),
        },
        account_id: None,
    };
    let wire = serde_json::to_value(&event).unwrap();
    assert_eq!(wire["type"], "credentialRequired");
    assert_eq!(wire["sessionId"], "s1");
    assert_eq!(serde_json::from_value::<ToolhubEvent>(wire).unwrap(), event);

    let status = json!({"type": "connectionStatus", "connection": {
        "accountId": "ca_1", "toolkit": "github", "authConfigId": "gh", "status": "expired",
        "scopes": ["repo"], "expiresAt": 10, "reason": null}});
    let parsed: ToolhubEvent = serde_json::from_value(status.clone()).unwrap();
    let ToolhubEvent::ConnectionStatus { connection } = &parsed else {
        panic!()
    };
    assert_eq!(connection.status, ToolhubConnectionStatus::Expired);

    let catalog = ToolhubEvent::Catalog {
        toolkits: vec![ToolhubToolkitSummary {
            slug: "github".into(),
            name: "GitHub".into(),
            description: ToolhubLocalizedText {
                de: "d".into(),
                en: "e".into(),
            },
            auth_schemes: vec![ToolhubAuthSchemeKind::Oauth2, ToolhubAuthSchemeKind::Bearer],
            tools: vec![ToolhubToolSummary {
                slug: "GITHUB_GET_REPO".into(),
                title: ToolhubLocalizedText {
                    de: "a".into(),
                    en: "b".into(),
                },
                description: ToolhubLocalizedText {
                    de: "a".into(),
                    en: "b".into(),
                },
                input_schema: json!({"type": "object"}),
                mutating: false,
            }],
        }],
    };
    let wire = serde_json::to_value(&catalog).unwrap();
    assert_eq!(
        wire["toolkits"][0]["authSchemes"],
        json!(["oauth2", "bearer"])
    );
    assert_eq!(
        serde_json::from_value::<ToolhubEvent>(wire).unwrap(),
        catalog
    );
}

#[test]
fn toolhub_contract_rejects_unknown_fields() {
    let forged =
        json!({"toolkit": "github", "authConfigId": "gh", "owner": "m", "accessToken": "x"});
    assert!(serde_json::from_value::<ToolhubConnectRequest>(forged).is_err());
}
