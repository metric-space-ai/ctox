use wha_types::{jid::server, Jid};

use crate::error::BinaryError;
use crate::node::{count_attrs, Attrs, Node, Value};
use crate::token;

pub(crate) struct Encoder {
    data: Vec<u8>,
}

impl Encoder {
    pub fn new() -> Self {
        // Leading byte is the "compression flag" — always 0 for marshal.
        Encoder { data: vec![0] }
    }

    pub fn finish(self) -> Vec<u8> {
        self.data
    }

    pub fn write_node(&mut self, n: &Node) -> Result<(), BinaryError> {
        if n.tag == "0" {
            self.push(token::LIST_8);
            self.push(token::LIST_EMPTY);
            return Ok(());
        }

        let has_content = if matches!(&n.content, Value::None) { 0 } else { 1 };
        let attr_count = count_attrs(&n.attrs);
        // Tag + 2*attrs + maybe content
        self.write_list_start(1 + 2 * attr_count + has_content);
        self.write_string(&n.tag)?;
        self.write_attributes(&n.attrs)?;
        if !matches!(&n.content, Value::None) {
            self.write_value(&n.content)?;
        }
        Ok(())
    }

    fn write_attributes(&mut self, attrs: &Attrs) -> Result<(), BinaryError> {
        for (k, v) in attrs {
            // Skip empty/None values to match Go encoder behaviour.
            match v {
                Value::String(s) if s.is_empty() => continue,
                Value::None => continue,
                _ => {}
            }
            self.write_string(k)?;
            self.write_value(v)?;
        }
        Ok(())
    }

    fn write_value(&mut self, v: &Value) -> Result<(), BinaryError> {
        match v {
            Value::None => self.push(token::LIST_EMPTY),
            Value::String(s) => self.write_string(s)?,
            Value::Jid(j) => self.write_jid(j)?,
            Value::Bytes(b) => self.write_bytes(b),
            Value::Nodes(nodes) => {
                self.write_list_start(nodes.len());
                for n in nodes {
                    self.write_node(n)?;
                }
            }
        }
        Ok(())
    }

    fn write_string(&mut self, s: &str) -> Result<(), BinaryError> {
        if let Some(idx) = token::index_of_single(s) {
            self.push(idx);
        } else if let Some((dict, idx)) = token::index_of_double(s) {
            self.push(token::DICTIONARY_0 + dict);
            self.push(idx);
        } else if validate_nibble(s) {
            self.write_packed_bytes(s, token::NIBBLE_8)?;
        } else if validate_hex(s) {
            self.write_packed_bytes(s, token::HEX_8)?;
        } else {
            self.write_string_raw(s);
        }
        Ok(())
    }

    fn write_string_raw(&mut self, s: &str) {
        self.write_byte_length(s.len());
        self.data.extend_from_slice(s.as_bytes());
    }

    fn write_bytes(&mut self, b: &[u8]) {
        self.write_byte_length(b.len());
        self.data.extend_from_slice(b);
    }

    fn write_byte_length(&mut self, length: usize) {
        if length < 256 {
            self.push(token::BINARY_8);
            self.push(length as u8);
        } else if length < (1 << 20) {
            self.push(token::BINARY_20);
            self.push(((length >> 16) & 0x0F) as u8);
            self.push(((length >> 8) & 0xFF) as u8);
            self.push((length & 0xFF) as u8);
        } else if length < i32::MAX as usize {
            self.push(token::BINARY_32);
            self.push(((length >> 24) & 0xFF) as u8);
            self.push(((length >> 16) & 0xFF) as u8);
            self.push(((length >> 8) & 0xFF) as u8);
            self.push((length & 0xFF) as u8);
        } else {
            // Should never happen on the WhatsApp wire format
            panic!("length too large: {length}");
        }
    }

    fn write_jid(&mut self, j: &Jid) -> Result<(), BinaryError> {
        if ((j.server == server::DEFAULT_USER || j.server == server::HIDDEN_USER) && j.device > 0)
            || j.server == server::HOSTED
            || j.server == server::HOSTED_LID
        {
            self.push(token::AD_JID);
            self.push(j.actual_agent());
            self.push(j.device as u8);
            self.write_string(&j.user)?;
        } else if j.server == server::MESSENGER {
            self.push(token::FB_JID);
            self.write_string(&j.user)?;
            self.push((j.device >> 8) as u8);
            self.push(j.device as u8);
            self.write_string(&j.server)?;
        } else if j.server == server::INTEROP {
            self.push(token::INTEROP_JID);
            self.write_string(&j.user)?;
            self.push((j.device >> 8) as u8);
            self.push(j.device as u8);
            self.push((j.integrator >> 8) as u8);
            self.push(j.integrator as u8);
            self.write_string(&j.server)?;
        } else {
            self.push(token::JID_PAIR);
            if j.user.is_empty() {
                self.push(token::LIST_EMPTY);
            } else {
                self.write_string(&j.user)?;
            }
            self.write_string(&j.server)?;
        }
        Ok(())
    }

    fn write_list_start(&mut self, size: usize) {
        if size == 0 {
            self.push(token::LIST_EMPTY);
        } else if size < 256 {
            self.push(token::LIST_8);
            self.push(size as u8);
        } else {
            self.push(token::LIST_16);
            self.push((size >> 8) as u8);
            self.push(size as u8);
        }
    }

    fn write_packed_bytes(&mut self, value: &str, kind: u8) -> Result<(), BinaryError> {
        if value.len() > token::PACKED_MAX {
            return Err(BinaryError::PackedTooLong(value.len()));
        }
        self.push(kind);
        let bytes = value.as_bytes();
        let mut rounded = bytes.len() / 2;
        if bytes.len() % 2 != 0 {
            rounded += 1;
        }
        let mut header = rounded as u8;
        if bytes.len() % 2 != 0 {
            header |= 0x80;
        }
        self.push(header);
        let packer: fn(u8) -> Result<u8, BinaryError> = match kind {
            token::NIBBLE_8 => pack_nibble,
            token::HEX_8 => pack_hex,
            _ => unreachable!(),
        };
        let pairs = bytes.len() / 2;
        for i in 0..pairs {
            let p1 = packer(bytes[2 * i])?;
            let p2 = packer(bytes[2 * i + 1])?;
            self.push((p1 << 4) | p2);
        }
        if bytes.len() % 2 != 0 {
            let p1 = packer(bytes[bytes.len() - 1])?;
            let p2 = packer(0)?;
            self.push((p1 << 4) | p2);
        }
        Ok(())
    }

    #[inline]
    fn push(&mut self, b: u8) {
        self.data.push(b);
    }
}

fn validate_nibble(s: &str) -> bool {
    if s.len() > token::PACKED_MAX {
        return false;
    }
    s.bytes().all(|c| matches!(c, b'0'..=b'9' | b'-' | b'.'))
}

fn validate_hex(s: &str) -> bool {
    if s.len() > token::PACKED_MAX {
        return false;
    }
    s.bytes().all(|c| matches!(c, b'0'..=b'9' | b'A'..=b'F'))
}

fn pack_nibble(c: u8) -> Result<u8, BinaryError> {
    Ok(match c {
        b'-' => 10,
        b'.' => 11,
        0 => 15,
        b'0'..=b'9' => c - b'0',
        _ => return Err(BinaryError::InvalidPackedValue(c)),
    })
}

fn pack_hex(c: u8) -> Result<u8, BinaryError> {
    Ok(match c {
        b'0'..=b'9' => c - b'0',
        b'A'..=b'F' => 10 + c - b'A',
        0 => 15,
        _ => return Err(BinaryError::InvalidPackedValue(c)),
    })
}
