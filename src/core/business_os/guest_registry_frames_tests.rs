//! State-machine component fixtures; these do not prove a Linux guest or peer.
#[test]
fn native_frame_coordinates_use_the_observed_viewport_before_consumption() {
    use super::super::super::guest_runtime::MouseButton;
    let mut frame = observation();
    complete(&mut frame);
    let outside = GuestInput::Click {
        x: 2,
        y: 1,
        button: MouseButton::Left,
    };
    assert!(frame
        .consume_input("frame", "actual-turn", &outside)
        .is_err());
    assert!(!frame.consumed);
    let inside = GuestInput::Click {
        x: 1,
        y: 1,
        button: MouseButton::Left,
    };
    assert!(frame.consume_input("frame", "actual-turn", &inside).is_ok());
    assert!(frame.consumed);
}

use super::*;
fn observation() -> Observation {
    let mut permit = FrameBudget::new().reserve().unwrap();
    permit.retain_bytes(3).unwrap();
    Observation {
        id: "frame".into(),
        turn: "actual-turn".into(),
        endpoint: GuestLiveEndpoint {
            process_instance_id: "fixture-process".into(),
            guest_session_id: "fixture-session".into(),
            endpoint_id: "fixture-endpoint".into(),
        },
        transport: Weak::new(),
        frame: GuestFrame {
            png: vec![1, 2, 3],
            width: 2,
            height: 2,
        },
        metadata: json!({"owner_user_id":"actual-owner"}),
        deadline: Instant::now() + FRAME_LIFETIME,
        delivered: false,
        delivery_offset: 0,
        consumed: false,
        pending: None,
        transfer_cancelled: None,
        _permit: permit,
    }
}
fn complete(frame: &mut Observation) {
    frame.send_chunk(0, 3, false, &mut |_, _| Ok(())).unwrap();
    frame.send_chunk(3, 0, true, &mut |_, _| Ok(())).unwrap();
}
#[test]
fn native_frame_budget_releases_failed_reservation_and_shrinks_capture() {
    let budget = FrameBudget::new();
    let mut first = budget.reserve().unwrap();
    let second = budget.reserve().unwrap();
    assert!(budget.reserve().is_err());
    assert_eq!(budget.frames.load(Ordering::Acquire), 2);
    first.retain_bytes(3).unwrap();
    assert_eq!(
        budget.bytes.load(Ordering::Acquire),
        CAPTURE_RESERVATION + 3
    );
    drop(second);
    drop(first);
    assert_eq!(budget.bytes.load(Ordering::Acquire), 0);
    assert_eq!(budget.frames.load(Ordering::Acquire), 0);
}
#[test]
fn native_frame_count_limit_survives_small_capture_reservations() {
    let budget = FrameBudget::new();
    let mut retained = Vec::new();
    for _ in 0..128 {
        let mut permit = budget.reserve().unwrap();
        permit.retain_bytes(1).unwrap();
        retained.push(permit);
    }
    assert!(budget.reserve().is_err());
    assert_eq!(budget.bytes.load(Ordering::Acquire), 128);
    drop(retained);
    assert_eq!(budget.frames.load(Ordering::Acquire), 0);
}
#[test]
fn native_frame_terminal_requires_every_contiguous_byte() {
    let mut frame = observation();
    let mut sends = 0;
    assert!(frame
        .send_chunk(3, 0, true, &mut |_, _| {
            sends += 1;
            Ok(())
        })
        .is_err());
    assert_eq!(sends, 0);
    assert!(!frame.delivered);
    complete(&mut frame);
    assert!(frame.delivered);
    assert!(frame.send_chunk(0, 3, false, &mut |_, _| Ok(())).is_err());
}
#[test]
fn native_frame_failed_send_never_becomes_input_or_retry_permit() {
    let mut frame = observation();
    assert!(frame
        .send_chunk(0, 3, false, &mut |_, _| Err(new_rx_error(
            "SEND_FAILED",
            None
        )))
        .is_err());
    assert!(frame.consumed);
    assert!(!frame.delivered);
    assert!(frame.send_chunk(0, 3, false, &mut |_, _| Ok(())).is_err());
    assert!(frame
        .consume_input(
            "frame",
            "actual-turn",
            &GuestInput::Type { text: "a".into() }
        )
        .is_err());
}
#[test]
fn native_frame_input_requires_current_identity_turn_and_delivery_once() {
    let mut frame = observation();
    let input = GuestInput::Type { text: "a".into() };
    assert!(frame.consume_input("frame", "actual-turn", &input).is_err());
    complete(&mut frame);
    assert!(frame
        .consume_input("old-frame", "actual-turn", &input)
        .is_err());
    assert!(frame.consume_input("frame", "old-turn", &input).is_err());
    assert!(frame.consume_input("frame", "actual-turn", &input).is_ok());
    assert!(frame.consume_input("frame", "actual-turn", &input).is_err());
}
#[test]
fn native_frame_expiry_blocks_delivery_and_input_without_callback() {
    let mut frame = observation();
    frame.deadline = Instant::now() - Duration::from_millis(1);
    assert!(frame
        .send_chunk(0, 3, false, &mut |_, _| panic!("expired pixels sent"))
        .is_err());
    let mut delivered = observation();
    complete(&mut delivered);
    delivered.deadline = Instant::now() - Duration::from_millis(1);
    assert!(delivered
        .consume_input(
            "frame",
            "actual-turn",
            &GuestInput::Type { text: "a".into() }
        )
        .is_err());
}

#[test]
fn native_pending_chunk_is_not_input_or_another_send_permit() {
    let mut frame = observation();
    let pending = frame.begin_chunk(0, 3, false).unwrap();
    assert!(frame.consumed);
    assert!(frame.begin_chunk(0, 3, false).is_err());
    assert!(frame
        .consume_input(
            "frame",
            "actual-turn",
            &GuestInput::Type { text: "a".into() }
        )
        .is_err());
    frame
        .finish_chunk(
            &pending,
            &[1, 2, 3],
            &json!({"owner_user_id":"actual-owner"}),
        )
        .unwrap();
    assert!(!frame.delivered);
    let terminal = frame.begin_chunk(3, 0, true).unwrap();
    frame
        .finish_chunk(&terminal, &[], &json!({"owner_user_id":"actual-owner"}))
        .unwrap();
    assert!(frame.delivered && !frame.consumed);
    assert!(frame
        .validate_chunk(&terminal, &[], &frame.metadata)
        .is_err());
}
#[test]
fn native_pending_chunk_rejects_replaced_bytes_owner_and_generation() {
    let mut frame = observation();
    let pending = frame.begin_chunk(0, 3, false).unwrap();
    assert!(frame
        .validate_chunk(&pending, &[1, 2, 4], &frame.metadata)
        .is_err());
    assert!(frame
        .validate_chunk(&pending, &[1, 2, 3], &json!({"owner_user_id":"foreign"}))
        .is_err());
    let mut foreign = pending.clone();
    foreign.nonce = "other-queue-generation".into();
    assert!(frame
        .validate_chunk(&foreign, &[1, 2, 3], &frame.metadata)
        .is_err());
    frame.deadline = Instant::now() - Duration::from_millis(1);
    assert!(frame
        .finish_chunk(&pending, &[1, 2, 3], &frame.metadata.clone())
        .is_err());
    assert!(frame.consumed && !frame.delivered);
}

#[test]
fn native_frame_aborted_completion_cannot_grant_input_after_terminal_bytes() {
    let mut frame = observation();
    let cancelled = Arc::new(std::sync::atomic::AtomicBool::new(false));
    frame.transfer_cancelled = Some(cancelled.clone());
    complete(&mut frame);
    assert!(frame.delivered && !frame.consumed);
    cancelled.store(true, Ordering::SeqCst);
    assert!(frame
        .consume_input(
            "frame",
            "actual-turn",
            &GuestInput::Type { text: "a".into() }
        )
        .is_err());
}
