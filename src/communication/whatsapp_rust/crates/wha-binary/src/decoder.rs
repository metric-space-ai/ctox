use wha_types::{jid::server, Jid};

use crate::error::BinaryError;
use crate::node::{Attrs, Node, Value};
use crate::token;

pub(crate) struct Decoder<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Decoder<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Decoder { data, pos: 0 }
    }

    pub fn exhausted(&self) -> bool {
        self.pos == self.data.len()
    }

    pub fn remaining(&self) -> usize {
        self.data.len() - self.pos
    }

    fn check(&self, n: usize) -> Result<(), BinaryError> {
        if self.pos + n > self.data.len() { Err(BinaryError::UnexpectedEof) } else { Ok(()) }
    }

    fn read_byte(&mut self) -> Result<u8, BinaryError> {
        self.check(1)?;
        let b = self.data[self.pos];
        self.pos += 1;
        Ok(b)
    }

    fn read_int_n(&mut self, n: usize, little_endian: bool) -> Result<usize, BinaryError> {
        self.check(n)?;
        let mut ret: usize = 0;
        for i in 0..n {
            let shift = if little_endian { i } else { n - i - 1 };
            ret |= (self.data[self.pos + i] as usize) << (shift * 8);
        }
        self.pos += n;
        Ok(ret)
    }

    fn read_int8(&mut self) -> Result<usize, BinaryError> { self.read_int_n(1, false) }
    fn read_int16(&mut self) -> Result<usize, BinaryError> { self.read_int_n(2, false) }
    fn read_int32(&mut self) -> Result<usize, BinaryError> { self.read_int_n(4, false) }

    fn read_int20(&mut self) -> Result<usize, BinaryError> {
        self.check(3)?;
        let ret = ((self.data[self.pos] as usize & 15) << 16)
            + ((self.data[self.pos + 1] as usize) << 8)
            + (self.data[self.pos + 2] as usize);
        self.pos += 3;
        Ok(ret)
    }

    fn read_raw(&mut self, n: usize) -> Result<&'a [u8], BinaryError> {
        self.check(n)?;
        let s = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }

    fn read_packed8(&mut self, tag: u8) -> Result<String, BinaryError> {
        let start = self.read_byte()?;
        let mut out = String::new();
        for _ in 0..(start & 0x7F) {
            let curr = self.read_byte()?;
            let lower = unpack_byte(tag, curr >> 4)?;
            let upper = unpack_byte(tag, curr & 0x0F)?;
            out.push(lower as char);
            out.push(upper as char);
        }
        if start >> 7 != 0 {
            out.pop();
        }
        Ok(out)
    }

    fn read_list_size(&mut self, tag: u8) -> Result<usize, BinaryError> {
        match tag {
            token::LIST_EMPTY => Ok(0),
            token::LIST_8 => self.read_int8(),
            token::LIST_16 => self.read_int16(),
            _ => Err(BinaryError::InvalidToken { tag, pos: self.pos }),
        }
    }

    /// `read_value(true)` returns strings as `Value::String`; `read_value(false)`
    /// returns binary blobs as `Value::Bytes`. Lists always come back as
    /// `Value::Nodes`.
    fn read_value(&mut self, as_string: bool) -> Result<Value, BinaryError> {
        let tag = self.read_byte()?;
        match tag {
            token::LIST_EMPTY => Ok(Value::None),
            token::LIST_8 | token::LIST_16 => {
                let nodes = self.read_list(tag)?;
                Ok(Value::Nodes(nodes))
            }
            token::BINARY_8 => {
                let n = self.read_int8()?;
                self.read_bytes_or_string(n, as_string)
            }
            token::BINARY_20 => {
                let n = self.read_int20()?;
                self.read_bytes_or_string(n, as_string)
            }
            token::BINARY_32 => {
                let n = self.read_int32()?;
                self.read_bytes_or_string(n, as_string)
            }
            token::DICTIONARY_0 | token::DICTIONARY_1 | token::DICTIONARY_2 | token::DICTIONARY_3 => {
                let dict = (tag - token::DICTIONARY_0) as usize;
                let idx = self.read_int8()?;
                token::get_double_token(dict, idx)
                    .map(|s| Value::String(s.to_owned()))
                    .ok_or(BinaryError::InvalidToken { tag, pos: self.pos })
            }
            token::FB_JID => self.read_fb_jid(),
            token::INTEROP_JID => self.read_interop_jid(),
            token::JID_PAIR => self.read_jid_pair(),
            token::AD_JID => self.read_ad_jid(),
            token::NIBBLE_8 | token::HEX_8 => Ok(Value::String(self.read_packed8(tag)?)),
            other => {
                let idx = other as usize;
                if idx >= 1 && idx < token::SINGLE_BYTE_TOKENS.len() {
                    Ok(Value::String(token::SINGLE_BYTE_TOKENS[idx].to_owned()))
                } else {
                    Err(BinaryError::InvalidToken { tag: other, pos: self.pos })
                }
            }
        }
    }

    fn read_bytes_or_string(&mut self, n: usize, as_string: bool) -> Result<Value, BinaryError> {
        let raw = self.read_raw(n)?.to_vec();
        if as_string {
            Ok(Value::String(String::from_utf8_lossy(&raw).into_owned()))
        } else {
            Ok(Value::Bytes(raw))
        }
    }

    fn read_jid_pair(&mut self) -> Result<Value, BinaryError> {
        let user = self.read_value(true)?;
        let server_v = self.read_value(true)?;
        let server = match server_v {
            Value::String(s) => s,
            Value::None => return Err(BinaryError::InvalidJidType("missing server")),
            _ => return Err(BinaryError::InvalidJidType("server not string")),
        };
        match user {
            Value::None => Ok(Value::Jid(Jid::new("", server))),
            Value::String(u) => Ok(Value::Jid(Jid::new(u, server))),
            _ => Err(BinaryError::InvalidJidType("user not string")),
        }
    }

    fn read_interop_jid(&mut self) -> Result<Value, BinaryError> {
        let user = match self.read_value(true)? {
            Value::String(u) => u,
            _ => return Err(BinaryError::InvalidJidType("interop user")),
        };
        let device = self.read_int16()? as u16;
        let integrator = self.read_int16()? as u16;
        let server_v = match self.read_value(true)? {
            Value::String(s) => s,
            _ => return Err(BinaryError::InvalidJidType("interop server")),
        };
        if server_v != server::INTEROP {
            return Err(BinaryError::InvalidJidType("expected interop server"));
        }
        Ok(Value::Jid(Jid {
            user,
            device,
            integrator,
            server: server::INTEROP.to_owned(),
            raw_agent: 0,
        }))
    }

    fn read_fb_jid(&mut self) -> Result<Value, BinaryError> {
        let user = match self.read_value(true)? {
            Value::String(u) => u,
            _ => return Err(BinaryError::InvalidJidType("fb user")),
        };
        let device = self.read_int16()? as u16;
        let server_v = match self.read_value(true)? {
            Value::String(s) => s,
            _ => return Err(BinaryError::InvalidJidType("fb server")),
        };
        if server_v != server::MESSENGER {
            return Err(BinaryError::InvalidJidType("expected messenger server"));
        }
        Ok(Value::Jid(Jid { user, device, server: server_v, ..Default::default() }))
    }

    fn read_ad_jid(&mut self) -> Result<Value, BinaryError> {
        let agent = self.read_byte()?;
        let device = self.read_byte()?;
        let user = match self.read_value(true)? {
            Value::String(u) => u,
            _ => return Err(BinaryError::InvalidJidType("ad user")),
        };
        Ok(Value::Jid(Jid::new_ad(user, agent, device)))
    }

    fn read_attributes(&mut self, n: usize) -> Result<Attrs, BinaryError> {
        let mut attrs = Attrs::new();
        for _ in 0..n {
            let key = match self.read_value(true)? {
                Value::String(k) => k,
                _ => return Err(BinaryError::NonStringKey),
            };
            let value = self.read_value(true)?;
            attrs.insert(key, value);
        }
        Ok(attrs)
    }

    fn read_list(&mut self, tag: u8) -> Result<Vec<Node>, BinaryError> {
        let size = self.read_list_size(tag)?;
        let mut out = Vec::with_capacity(size);
        for _ in 0..size {
            out.push(self.read_node()?);
        }
        Ok(out)
    }

    pub fn read_node(&mut self) -> Result<Node, BinaryError> {
        let size_tag = self.read_byte()?;
        let list_size = self.read_list_size(size_tag)?;
        let raw_desc = self.read_value(true)?;
        let tag = match raw_desc {
            Value::String(t) => t,
            _ => return Err(BinaryError::InvalidNode),
        };
        if list_size == 0 || tag.is_empty() {
            return Err(BinaryError::InvalidNode);
        }
        let attrs = self.read_attributes((list_size - 1) >> 1)?;
        let content = if list_size % 2 == 1 {
            Value::None
        } else {
            self.read_value(false)?
        };
        Ok(Node { tag, attrs, content })
    }
}

fn unpack_byte(tag: u8, v: u8) -> Result<u8, BinaryError> {
    match tag {
        token::NIBBLE_8 => unpack_nibble(v),
        token::HEX_8 => unpack_hex(v),
        _ => Err(BinaryError::UnknownPackedTag(tag)),
    }
}

fn unpack_nibble(v: u8) -> Result<u8, BinaryError> {
    Ok(match v {
        0..=9 => b'0' + v,
        10 => b'-',
        11 => b'.',
        15 => 0,
        _ => return Err(BinaryError::InvalidPackedValue(v)),
    })
}

fn unpack_hex(v: u8) -> Result<u8, BinaryError> {
    Ok(match v {
        0..=9 => b'0' + v,
        10..=15 => b'A' + v - 10,
        _ => return Err(BinaryError::InvalidPackedValue(v)),
    })
}
