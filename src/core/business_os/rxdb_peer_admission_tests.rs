use super::*;

#[test]
fn collection_permission_distinguishes_issuer_contention_from_invalid_credentials() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let (token, _) = store::issue_business_os_capability_token_for_managed_user(
        root.path(), "reader", "Reader", "admin", now_ms() as i64,
    )?;
    let check = || store::check_webrtc_collection_permission(
        root.path(), &token, "business_commands", policy::BusinessOsPermission::DataRead,
    );
    assert!(check()?);
    store::with_current_webrtc_capability_signer(root.path(), |_| {
        assert!(check().is_err());
        assert!(!store::webrtc_capability_allows_collection_permission(
            root.path(), &token, "business_commands", policy::BusinessOsPermission::DataRead,
        ));
        Ok(())
    })?;
    assert!(check()?);
    assert!(!store::check_webrtc_collection_permission(
        root.path(), "invalid", "business_commands", policy::BusinessOsPermission::DataRead,
    )?);
    assert!(!store::check_webrtc_collection_permission(
        root.path(), "", "business_commands", policy::BusinessOsPermission::DataRead,
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
