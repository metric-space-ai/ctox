// ref: internal/runtime/executor/helps/devin_wire_test.go @ d7914afdedca7af95ee974a42453dc49fc1388ce
// License: MIT (upstream); modifications AGPL-3.0-only

use super::devin_wire::*;
use flate2::{write::GzEncoder, Compression};
use std::io::{Cursor, Write};

fn gzip(bytes: &[u8]) -> Vec<u8> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(bytes).unwrap();
    encoder.finish().unwrap()
}

#[test]
fn candidate_devin_wire_fragmented_multiple_and_empty_frames() {
    let raw = "hello 🙂\0β".as_bytes();
    let mut wire = wrap_connect_envelope(raw).unwrap();
    wire.extend(wrap_connect_envelope(b"").unwrap());
    wire.extend(wrap_connect_envelope_with_flag(CONNECT_FLAG_END_STREAM, b"{}").unwrap());
    let expected = vec![
        ConnectFrame {
            flag: 0,
            payload: raw.to_vec(),
        },
        ConnectFrame {
            flag: 0,
            payload: Vec::new(),
        },
        ConnectFrame {
            flag: 2,
            payload: b"{}".to_vec(),
        },
    ];
    for split in 0..=wire.len() {
        let mut decoder = ConnectFrameDecoder::default();
        let mut frames = Vec::new();
        decoder
            .feed(&wire[..split], |frame| frames.push(frame))
            .unwrap();
        decoder.feed(&[], |frame| frames.push(frame)).unwrap();
        decoder
            .feed(&wire[split..], |frame| frames.push(frame))
            .unwrap();
        assert_eq!(decoder.finish(), Ok(()), "split {split}");
        assert_eq!(frames, expected, "split {split}");
    }
    for width in 1..=wire.len() {
        let mut decoder = ConnectFrameDecoder::default();
        let mut frames = Vec::new();
        for chunk in wire.chunks(width) {
            decoder.feed(chunk, |frame| frames.push(frame)).unwrap();
        }
        assert_eq!(decoder.finish(), Ok(()));
        assert_eq!(frames, expected);
        assert!(!format!("{decoder:?}").contains("hello"));
    }
}

#[test]
fn candidate_devin_wire_compression_and_trailer_flags() {
    let mut compressed = gzip(b"one");
    compressed.extend(gzip(b"two"));
    for flag in [
        CONNECT_FLAG_COMPRESSED,
        CONNECT_FLAG_COMPRESSED | CONNECT_FLAG_END_STREAM,
    ] {
        let bytes = wrap_connect_envelope_with_flag(flag, &compressed).unwrap();
        let mut decoder = ConnectFrameDecoder::default();
        let mut frames = Vec::new();
        for chunk in bytes.chunks(3) {
            decoder.feed(chunk, |frame| frames.push(frame)).unwrap();
        }
        decoder.finish().unwrap();
        assert_eq!(
            frames,
            vec![ConnectFrame {
                flag,
                payload: b"onetwo".to_vec()
            }]
        );
        let frame = read_connect_frame(&mut Cursor::new(&bytes)).unwrap();
        assert_eq!(frame.flag, flag);
        assert_eq!(frame.payload, b"onetwo");
        assert!(!format!("{frame:?}").contains("onetwo"));
    }
}

#[test]
fn candidate_devin_wire_limits_corruption_and_failed_stream() {
    let too_large = (MAX_CONNECT_FRAME_SIZE as u32 + 1).to_be_bytes();
    let mut header = vec![0];
    header.extend(too_large);
    assert_eq!(
        read_connect_frame(&mut Cursor::new(&header)),
        Err(ConnectFrameError::FrameTooLarge)
    );
    let mut decoder = ConnectFrameDecoder::default();
    assert_eq!(
        decoder.feed(&header, |_| panic!("invalid frame emitted")),
        Err(ConnectFrameError::FrameTooLarge)
    );
    assert_eq!(
        decoder.feed(&wrap_connect_envelope(b"ok").unwrap(), |_| panic!(
            "failed decoder resurrected"
        )),
        Err(ConnectFrameError::FrameTooLarge)
    );
    assert_eq!(decoder.finish(), Err(ConnectFrameError::FrameTooLarge));

    for flag in [4, 128, 255] {
        let invalid = [flag, 0, 0, 0, 0];
        assert_eq!(
            read_connect_frame(&mut Cursor::new(invalid)),
            Err(ConnectFrameError::InvalidFlag(flag))
        );
        assert_eq!(
            ConnectFrameDecoder::default().feed(&invalid, |_| panic!("bad flag emitted")),
            Err(ConnectFrameError::InvalidFlag(flag))
        );
    }

    for length in [32, 33] {
        let compressed = gzip(&vec![b'x'; length]);
        let wire = wrap_connect_envelope_with_flag(1, &compressed).unwrap();
        let mut bounded = ConnectFrameDecoder::with_limits(256, 32);
        let mut frames = Vec::new();
        let result = bounded.feed(&wire, |frame| frames.push(frame));
        if length == 32 {
            result.unwrap();
            assert_eq!(frames[0].payload.len(), 32);
        } else {
            assert_eq!(result, Err(ConnectFrameError::DecompressedFrameTooLarge));
            assert!(frames.is_empty());
        }
    }
    let mut corrupt = gzip(b"CRC protected");
    let position = corrupt.len() - 8;
    corrupt[position] ^= 0xff;
    let wire = wrap_connect_envelope_with_flag(1, &corrupt).unwrap();
    assert_eq!(
        read_connect_frame(&mut Cursor::new(wire)),
        Err(ConnectFrameError::InvalidCompression)
    );
    let invalid_gzip = wrap_connect_envelope_with_flag(1, b"not gzip").unwrap();
    assert_eq!(
        ConnectFrameDecoder::default().feed(&invalid_gzip, |_| panic!("invalid gzip emitted")),
        Err(ConnectFrameError::InvalidCompression)
    );
}

#[test]
fn candidate_devin_wire_keeps_emitted_frames_before_later_protocol_failure() {
    let mut wire = wrap_connect_envelope(b"already emitted").unwrap();
    wire.extend([8, 0, 0, 0, 0]);
    let mut frames = Vec::new();
    let result = ConnectFrameDecoder::default().feed(&wire, |frame| frames.push(frame));
    assert_eq!(result, Err(ConnectFrameError::InvalidFlag(8)));
    assert_eq!(
        frames,
        vec![ConnectFrame {
            flag: 0,
            payload: b"already emitted".to_vec()
        }]
    );
}

#[test]
fn candidate_devin_wire_reader_preserves_frame_boundaries() {
    let first = wrap_connect_envelope(b"first").unwrap();
    let second = wrap_connect_envelope_with_flag(2, b"{}").unwrap();
    let mut wire = first.clone();
    wire.extend(&second);
    let mut reader = Cursor::new(&wire);
    assert_eq!(read_connect_frame(&mut reader).unwrap().payload, b"first");
    assert_eq!(reader.position(), first.len() as u64);
    assert_eq!(read_connect_frame(&mut reader).unwrap().flag, 2);
    assert_eq!(
        read_connect_frame(&mut reader),
        Err(ConnectFrameError::EndOfInput)
    );
    for end in 1..first.len() {
        let truncated = &first[..end];
        assert_eq!(
            read_connect_frame(&mut Cursor::new(truncated)),
            Err(ConnectFrameError::Truncated)
        );
        let mut decoder = ConnectFrameDecoder::default();
        decoder
            .feed(truncated, |_| panic!("truncated frame emitted"))
            .unwrap();
        assert_eq!(
            decoder.finish(),
            Err(ConnectFrameError::Truncated),
            "length {end}"
        );
    }
}

#[test]
fn candidate_devin_wire_trailer_error_classification() {
    for (code, message, expected) in [
        ("invalid_argument", "bad request", 400),
        ("INVALID_ARGUMENT", "Internal Error", 502),
        ("internal", "backend failed", 502),
        ("unauthenticated", "login required", 401),
        ("permission_denied", "not allowed", 403),
        ("permission_denied", "High Demand", 429),
        ("resource_exhausted", "slow down", 429),
        ("unavailable", "retry later", 503),
        ("canceled", "canceled", 499),
        ("deadline_exceeded", "timeout", 504),
        ("failed_precondition", "not configured", 400),
        ("failed_precondition", "QUOTA", 429),
        ("failed_precondition", "credits", 429),
        ("failed_precondition", "ACU", 429),
        ("failed_precondition", "exhausted", 429),
        ("failed_precondition", "limit reached", 429),
        ("unknown", "unknown", 502),
    ] {
        let body =
            serde_json::to_vec(&serde_json::json!({"error":{"code":code,"message":message}}))
                .unwrap();
        let error = parse_devin_trailer_error(&body).unwrap();
        assert_eq!(error.status_code, expected);
        assert_eq!(error.code, code);
        assert_eq!(error.message, message);
        assert_eq!(
            error.to_string(),
            format!("devin upstream error ({code}): {message}")
        );
    }
    for body in [
        b"".as_slice(),
        b"  {} ",
        b"not json",
        b"[]",
        b"null",
        b"{\"error\":null}",
        b"{\"error\":{\"code\":12}}",
        b"{\"error\":{\"message\":[]}}",
        b"{\"metadata\":{}}",
    ] {
        assert!(parse_devin_trailer_error(body).is_none(), "{body:?}");
    }
    for body in [
        b"{\"error\":{}}".as_slice(),
        b"{\"error\":{\"code\":null,\"message\":null}}",
    ] {
        let error = parse_devin_trailer_error(body).unwrap();
        assert_eq!(error.status_code, 502);
        assert_eq!(error.to_string(), "devin upstream error (): ");
    }
}
