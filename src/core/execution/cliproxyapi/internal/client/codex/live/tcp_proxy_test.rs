// ref: internal/client/codex/live/tcp_proxy_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use std::io::{self, Cursor, Write};

use super::*;

const PUBLIC_TCP: &str = "1 1 TCP 2122260223 8.8.8.8 443 typ host tcptype passive";

fn stun_frame(username: &str, integrity: bool, fingerprint: bool) -> Vec<u8> {
    let mut attributes = Vec::new();
    add_attribute(&mut attributes, 0x0006, username.as_bytes());
    if integrity {
        add_attribute(&mut attributes, 0x0008, &[0; 20]);
    }
    if fingerprint {
        add_attribute(&mut attributes, 0x8028, &[0; 4]);
    }
    let mut payload = vec![0x00, 0x01];
    payload.extend_from_slice(&(attributes.len() as u16).to_be_bytes());
    payload.extend_from_slice(&[0x21, 0x12, 0xA4, 0x42]);
    payload.extend_from_slice(&[0; 12]);
    payload.extend_from_slice(&attributes);
    let mut frame = (payload.len() as u16).to_be_bytes().to_vec();
    frame.extend_from_slice(&payload);
    frame
}

fn add_attribute(output: &mut Vec<u8>, kind: u16, value: &[u8]) {
    output.extend_from_slice(&kind.to_be_bytes());
    output.extend_from_slice(&(value.len() as u16).to_be_bytes());
    output.extend_from_slice(value);
    while !output.len().is_multiple_of(4) {
        output.push(0);
    }
}

struct ShortWriter(Vec<u8>);

impl Write for ShortWriter {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        let count = data.len().min(2);
        self.0.extend_from_slice(&data[..count]);
        Ok(count)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
