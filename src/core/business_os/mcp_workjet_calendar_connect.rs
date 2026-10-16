// Origin: CTOX
// License: AGPL-3.0-only
//! OAuth control, deliberately outside durable command payloads: callbackUrl
//! carries a one-time authorization code. Only secret-free receipts are returned.
use super::*;
use crate::communication::google_calendar;

pub(crate) const METHOD: &str = "ctox.workjet.calendar.connect.v1";
#[derive(Deserialize)]
#[serde(tag = "action", deny_unknown_fields)]
enum Request {
    #[serde(rename = "project.calendar.account.connect.start")]
    Start { provider: String },
    #[serde(rename = "project.calendar.account.connect.complete")]
    Complete {
        #[serde(rename = "attemptId")]
        attempt_id: String,
        #[serde(rename = "callbackUrl")]
        callback_url: String,
    },
}

pub(crate) fn connect_webrtc(
    root: &Path,
    token: &str,
    params: Vec<Value>,
) -> Result<Value, String> {
    let run = || -> google_calendar::ConnectResult<Value> {
        if params.len() != 1
            || serde_json::to_vec(&params)
                .map_err(|_| "permission_denied")?
                .len()
                > 12_288
        {
            return Err("permission_denied");
        }
        let request: Request =
            serde_json::from_value(params[0].clone()).map_err(|_| "permission_denied")?;
        let identity =
            store::verify_webrtc_capability_actor(root, token).ok_or("permission_denied")?;
        let owner = super::super::workjet_identity::owner(root, &identity.0)
            .map_err(|_| "permission_denied")?;
        let authorize = || -> google_calendar::ConnectResult<()> {
            if store::verify_webrtc_capability_actor(root, token).as_ref() != Some(&identity)
                || !store::capability_allows_collection_permission(
                    root,
                    token,
                    "communication_accounts",
                    BusinessOsPermission::DataWrite,
                )
                || super::super::workjet_identity::owner(root, &identity.0)
                    .ok()
                    .as_deref()
                    != Some(owner.as_str())
            {
                return Err("permission_denied");
            }
            Ok(())
        };
        authorize()?;
        match request {
            Request::Start { provider } => google_calendar::start(root, &owner, &provider),
            Request::Complete {
                attempt_id,
                callback_url,
            } => google_calendar::complete(root, &owner, &attempt_id, &callback_url, &authorize),
        }
    };
    run().map_err(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn google_calendar_connect_uses_live_browser_authority() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        store::tests::seed_business_user(root.path(), "calendar-owner", "admin")?;
        let now = i64::try_from(store::now_ms())?;
        let (token, _) =
            store::issue_business_os_capability_token(root.path(), "calendar-owner", now)?;
        crate::secrets::write_secret_record(root.path(),google_calendar::CLIENT_SCOPE,google_calendar::CLIENT_NAME,&json!({"client_id":"fake-client","client_secret":"fake-client-secret","redirect_uri":"https://calendar.example.test/callback"}).to_string(),None,json!({}))?;
        let start = json!({"action":"project.calendar.account.connect.start","provider":"google"});
        assert_eq!(
            connect_webrtc(root.path(), "not-a-capability", vec![start.clone()]),
            Err("permission_denied".into())
        );
        let receipt =
            connect_webrtc(root.path(), &token, vec![start.clone()]).map_err(anyhow::Error::msg)?;
        assert!(receipt["authorizeUrl"]
            .as_str()
            .unwrap()
            .starts_with("https://accounts.google.com/"));
        assert!(!receipt.to_string().contains("fake-client-secret"));
        let mut native =
            crate::communication_store::open_channel_db(&root.path().join("runtime/ctox.sqlite3"))?;
        crate::mission::channels::upsert_communication_account(
            &mut native,
            "email:connected@example.test",
            "email",
            "connected@example.test",
            "google",
            json!({"ownerUserId":"calendar-owner","shared_user_ids":[],"displayName":"Connected Google","source":"google-oauth"}),
        )?;
        let page = super::super::workjet_calendar::read_webrtc(
            root.path(),
            &token,
            vec![json!({"action":"accounts","request_id":"google-accounts"})],
        )
        .map_err(anyhow::Error::msg)?;
        assert_eq!(page["data"]["accounts"][0]["id"], "connected@example.test");
        assert_eq!(
            page["data"]["accounts"][0]["supported"], true,
            "native Google calendar must be listed as supported"
        );
        store::tests::seed_business_user(root.path(), "foreign-admin", "admin")?;
        let (foreign, _) =
            store::issue_business_os_capability_token(root.path(), "foreign-admin", now)?;
        let page = super::super::workjet_calendar::read_webrtc(
            root.path(),
            &foreign,
            vec![json!({"action":"accounts","request_id":"foreign-accounts"})],
        )
        .map_err(anyhow::Error::msg)?;
        assert_eq!(
            page["data"]["accounts"],
            json!([]),
            "foreign admin must not inherit a Google account"
        );
        assert_eq!(
            connect_webrtc(
                root.path(),
                &token,
                vec![
                    json!({"action":"project.calendar.account.connect.start","provider":"google","actor":"foreign"})
                ]
            ),
            Err("permission_denied".into())
        );
        store::open_store(root.path())?.execute(
            "UPDATE business_users SET active=0 WHERE user_id='calendar-owner'",
            [],
        )?;
        assert_eq!(
            connect_webrtc(root.path(), &token, vec![start]),
            Err("permission_denied".into())
        );
        Ok(())
    }
}
