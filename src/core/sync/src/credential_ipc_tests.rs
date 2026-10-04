use super::*;

#[tokio::test]
async fn private_frames_round_trip_over_a_bounded_stream() {
    use crate::business_data_contract::NativeBusinessDataHostFrame as Frame;
    let (mut sender, mut receiver) = tokio::io::duplex(64);
    let challenge = Challenge {
        version: 1,
        request_id: "r".into(),
        connection_id: "c".into(),
        target_id: "t".into(),
        session_epoch: 0,
        nonce: None,
    };
    let frame = Frame::CredentialChallenge { challenge };
    let send = write_host_frame(&mut sender, &frame);
    let read = read_host_frame(&mut receiver);
    let (written, received) = tokio::join!(send, read);
    written.unwrap();
    assert!(
        matches!(received.unwrap(), Frame::CredentialChallenge { challenge } if challenge.request_id == "r")
    );
}

#[tokio::test]
async fn excessive_frame_header_is_rejected_before_reading_body() {
    use tokio::io::AsyncWriteExt;
    let (mut sender, mut receiver) = tokio::io::duplex(16);
    sender
        .write_u32(crate::ipc::IPC_MAX_FRAME_BYTES as u32 + 1)
        .await
        .unwrap();
    assert!(read_host_frame(&mut receiver).await.is_err());
}

#[tokio::test]
async fn parser_errors_do_not_echo_credentials() {
    use tokio::io::AsyncWriteExt;
    let (mut sender, mut receiver) = tokio::io::duplex(1024);
    let bytes = br#"{"type":"SECRET_TEST_VALUE"}"#;
    sender.write_u32(bytes.len() as u32).await.unwrap();
    sender.write_all(bytes).await.unwrap();
    let error = match read_host_frame(&mut receiver).await {
        Err(error) => error,
        Ok(_) => panic!("invalid frame accepted"),
    };
    assert_eq!(error.to_string(), "invalid private BusinessData frame");
}

fn answer(challenge: &Challenge) -> Reply {
    Reply {
        version: 1,
        request_id: challenge.request_id.clone(),
        connection_id: challenge.connection_id.clone(),
        session_epoch: challenge.session_epoch,
        capability_token: Some("test-capability".into()),
        device_proof: None,
    }
}

#[tokio::test]
async fn reply_is_correlated_to_its_live_connection() {
    let (mut owner, requester) = credential_channel();
    let request = requester.request("target", "connection", 7, None);
    let deliver = async {
        let challenge = owner.next_challenge().await.unwrap();
        assert_eq!(challenge.target_id, "target");
        assert_eq!(challenge.session_epoch, 7);
        owner.accept_reply(answer(&challenge)).unwrap();
    };
    let (result, ()) = tokio::join!(request, deliver);
    assert_eq!(
        result.unwrap().capability_token.as_deref(),
        Some("test-capability")
    );
}

#[tokio::test]
async fn wrong_epoch_fences_pending_and_future_requests() {
    let (mut owner, requester) = credential_channel();
    let request = requester.request("target", "connection", 7, None);
    let deliver = async {
        let challenge = owner.next_challenge().await.unwrap();
        let mut reply = answer(&challenge);
        reply.session_epoch = 8;
        assert!(owner.accept_reply(reply).is_err());
    };
    let (result, ()) = tokio::join!(request, deliver);
    assert!(result.is_err());
    assert!(requester
        .request("target", "connection", 7, None)
        .await
        .is_err());
}

#[tokio::test]
async fn owner_drop_cancels_waiter_without_a_detached_task() {
    let (mut owner, requester) = credential_channel();
    let request = requester.request("target", "connection", 0, None);
    let close = async move {
        owner.next_challenge().await.unwrap();
        drop(owner);
    };
    let (result, ()) = tokio::join!(request, close);
    assert!(result.is_err());
}

#[tokio::test(start_paused = true)]
async fn deadline_removes_pending_request_and_late_reply_is_rejected() {
    let (mut owner, requester) = credential_channel();
    let request = requester.request("target", "connection", 0, None);
    let late = async {
        let challenge = owner.next_challenge().await.unwrap();
        tokio::time::sleep(DEADLINE + Duration::from_secs(1)).await;
        assert!(owner.accept_reply(answer(&challenge)).is_err());
    };
    let (result, ()) = tokio::join!(request, late);
    assert!(result.is_err());
    assert!(owner.shared.pending.lock().unwrap().is_empty());
}
