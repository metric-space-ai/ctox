use super::*;
use crate::business_os::threads;

#[test]
fn collection_permission_distinguishes_issuer_contention_from_invalid_credentials(
) -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let (token, _) = store::issue_business_os_capability_token_for_managed_user(
        root.path(),
        "reader",
        "Reader",
        "admin",
        now_ms() as i64,
    )?;
    let check = || {
        store::check_webrtc_collection_permission(
            root.path(),
            &token,
            "business_commands",
            crate::business_os::policy::BusinessOsPermission::DataRead,
        )
    };
    assert!(check()?);
    store::with_current_webrtc_capability_signer(root.path(), |_| {
        // Both the warm decision and a cold preparatory read work without
        // acquiring the publication mutex. A cache hit alone cannot prove it.
        assert!(check()?);
        assert!(store::check_webrtc_collection_permission(
            root.path(),
            &token,
            "ctox_crew_members",
            crate::business_os::policy::BusinessOsPermission::DataRead,
        )?);
        // Publication still cannot reenter the non-recursive issuer fence.
        assert!(store::with_current_webrtc_capability_signer(root.path(), |_| Ok(())).is_err());
        Ok(())
    })?;
    assert!(check()?);
    // Genuine unavailable authority remains an error, not an invalid-token
    // denial. Use a fresh collection so the existing short decision cache
    // cannot hide the missing protected key during this isolated check.
    let key = root
        .path()
        .join("runtime")
        .join(crate::secrets::SECRET_MASTER_KEY_FILE);
    let held_key = key.with_extension("fixture-held");
    std::fs::rename(&key, &held_key)?;
    assert!(store::check_webrtc_collection_permission(
        root.path(),
        &token,
        "ctox_queue_tasks",
        crate::business_os::policy::BusinessOsPermission::DataRead,
    )
    .is_err());
    assert!(!store::webrtc_capability_allows_collection_permission(
        root.path(),
        &token,
        "ctox_queue_tasks",
        crate::business_os::policy::BusinessOsPermission::DataRead,
    ));
    std::fs::rename(&held_key, &key)?;
    assert!(store::check_webrtc_collection_permission(
        root.path(),
        &token,
        "ctox_queue_tasks",
        crate::business_os::policy::BusinessOsPermission::DataRead,
    )?);
    assert!(!store::check_webrtc_collection_permission(
        root.path(),
        "invalid",
        "business_commands",
        crate::business_os::policy::BusinessOsPermission::DataRead,
    )?);
    assert!(!store::check_webrtc_collection_permission(
        root.path(),
        "",
        "business_commands",
        crate::business_os::policy::BusinessOsPermission::DataRead,
    )?);
    Ok(())
}

#[test]
fn missing_revocation_store_rejects_session_without_creating_authority() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let protocol = serde_json::json!({"peerSession":{"sessionId":"peer"}});
    assert_eq!(
        validate_device_bound_peer_session(root.path(), &protocol, None),
        WebRTCPeerSessionValidation::Reject
    );
    assert!(!store::business_os_store_path(root.path()).exists());
    Ok(())
}

#[test]
fn current_revocation_and_store_failure_are_enforced_before_session_admission() -> anyhow::Result<()>
{
    let root = tempfile::tempdir()?;
    let conn = store::open_store(root.path())?;
    let protocol = serde_json::json!({"peerSession":{"sessionId":"peer"}});
    // Existing least-privilege negotiation is retained, with no data grant.
    assert_eq!(
        validate_device_bound_peer_session(root.path(), &protocol, None),
        WebRTCPeerSessionValidation::Accept
    );
    store::revoke_business_peer(root.path(), "peer", "admin", "test revocation")?;
    assert_eq!(
        validate_device_bound_peer_session(root.path(), &protocol, None),
        WebRTCPeerSessionValidation::Reject
    );
    store::clear_business_peer_revocation(root.path(), "peer")?;
    assert_eq!(
        validate_device_bound_peer_session(root.path(), &protocol, None),
        WebRTCPeerSessionValidation::Accept
    );
    conn.execute_batch("DROP TABLE business_peer_revocations")?;
    assert!(store::is_business_peer_revoked(root.path(), "peer").is_err());
    assert_eq!(
        validate_device_bound_peer_session(root.path(), &protocol, None),
        WebRTCPeerSessionValidation::Reject
    );
    Ok(())
}

#[test]
fn owner_browser_can_write_commands_but_foreign_peer_and_command_cannot() -> anyhow::Result<()> {
    use crate::business_os::policy::BusinessOsPermission;

    let root = tempfile::tempdir()?;
    let foreign_root = tempfile::tempdir()?;
    let at_ms = now_ms() as i64;
    let (owner_token, _) = store::issue_business_os_capability_token_for_managed_user(
        root.path(),
        "owner-browser",
        "Owner",
        "chef",
        at_ms,
    )?;
    // A foreign issuer cannot acquire authority by claiming the same actor/role.
    let (foreign_token, _) = store::issue_business_os_capability_token_for_managed_user(
        foreign_root.path(),
        "owner-browser",
        "Owner",
        "chef",
        at_ms,
    )?;
    for (token, expected) in [
        (&owner_token, WebRTCPeerSessionValidation::Accept),
        (&foreign_token, WebRTCPeerSessionValidation::Reject),
    ] {
        let protocol = serde_json::json!({
            "peerSession": {"sessionId": "owner-peer", "capabilityToken": token}
        });
        assert_eq!(
            validate_device_bound_peer_session(root.path(), &protocol, None),
            expected
        );
        let allowed = expected == WebRTCPeerSessionValidation::Accept;
        assert_eq!(
            store::check_webrtc_collection_permission(
                root.path(),
                token,
                "business_commands",
                BusinessOsPermission::DataRead,
            )?,
            allowed
        );
        assert_eq!(
            threads::may_accept_peer_write(root.path(), token, "business_commands"),
            allowed
        );
        let command = serde_json::json!({"client_context": {"capability_token": token}});
        assert_eq!(
            threads::may_accept_peer_document_write(
                root.path(),
                token,
                "business_commands",
                &command,
            ),
            allowed
        );
    }
    let foreign_command = serde_json::json!({
        "client_context": {"capability_token": foreign_token}
    });
    assert!(!threads::may_accept_peer_document_write(
        root.path(),
        &owner_token,
        "business_commands",
        &foreign_command,
    ));
    for collection in ["ctox_queue_tasks", "workjet_projects"] {
        assert!(!threads::may_accept_peer_write(
            root.path(),
            &owner_token,
            collection
        ));
    }
    Ok(())
}

#[test]
fn interrupted_actor_lookup_is_unavailable_then_recovers_without_widening_policy(
) -> anyhow::Result<()> {
    use crate::business_os::policy::BusinessOsPermission;

    let root = tempfile::tempdir()?;
    let at_ms = now_ms() as i64;
    let (token, _) = store::issue_business_os_capability_token_for_managed_user(
        root.path(),
        "owner-browser",
        "Owner",
        "chef",
        at_ms,
    )?;
    let conn = store::open_store(root.path())?;
    store::with_current_webrtc_capability_signer(root.path(), |secret| {
        let check = || {
            store::check_webrtc_collection_permission_from_connection(
                &conn,
                &token,
                secret,
                "business_commands",
                BusinessOsPermission::DataRead,
                at_ms,
            )
        };
        assert!(check()?);
        // Interrupt the real role/epoch SELECT, not a mocked authorization hook.
        conn.progress_handler(1, Some(|| true));
        let failure =
            check().expect_err("interrupted native lookup must not become a policy denial");
        assert!(failure.to_string().contains("interrupted"));
        assert!(
            !store::webrtc_capability_allows_collection_permission_from_connection(
                &conn,
                &token,
                secret,
                "business_commands",
                BusinessOsPermission::DataRead,
                at_ms,
            )
        );
        conn.progress_handler(0, None::<fn() -> bool>);
        assert!(check()?);
        assert!(!store::check_webrtc_collection_permission_from_connection(
            &conn,
            "foreign-or-invalid",
            secret,
            "business_commands",
            BusinessOsPermission::DataRead,
            at_ms,
        )?);
        Ok(())
    })?;
    Ok(())
}

#[test]
fn current_owner_epoch_role_and_active_state_remain_definitive_denials() -> anyhow::Result<()> {
    use crate::business_os::policy::BusinessOsPermission;

    for change in [
        "UPDATE business_users SET capability_epoch = capability_epoch + 1 WHERE user_id = ?1",
        "UPDATE business_users SET role = 'user' WHERE user_id = ?1",
        "UPDATE business_users SET active = 0 WHERE user_id = ?1",
    ] {
        let root = tempfile::tempdir()?;
        let at_ms = now_ms() as i64;
        let (token, _) = store::issue_business_os_capability_token_for_managed_user(
            root.path(),
            "owner-browser",
            "Owner",
            "chef",
            at_ms,
        )?;
        let conn = store::open_store(root.path())?;
        store::with_current_webrtc_capability_signer(root.path(), |secret| {
            let check = |when| {
                store::check_webrtc_collection_permission_from_connection(
                    &conn,
                    &token,
                    secret,
                    "business_commands",
                    BusinessOsPermission::DataRead,
                    when,
                )
            };
            assert!(check(at_ms)?);
            // Ordinary Owner tokens do not inherit the paired-device expiry exception.
            assert!(!check(at_ms + 13 * 60 * 60 * 1000)?);
            conn.execute(change, ["owner-browser"])?;
            assert!(!check(at_ms)?);
            Ok(())
        })?;
    }
    Ok(())
}
