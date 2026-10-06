//! Bencode for .torrent metainfo. Dicts encode with sorted keys (BEP 3).
#![forbid(unsafe_code)]

use crate::error::{Error, Result};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BVal {
    Int(i64),
    Bytes(Vec<u8>),
    List(Vec<BVal>),
    Dict(Vec<(Vec<u8>, BVal)>),
}

impl BVal {
    pub fn dict_get(&self, k: &[u8]) -> Option<&BVal> {
        match self {
            BVal::Dict(d) => d.iter().find(|(kk, _)| kk.as_slice() == k).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn as_int(&self) -> Option<i64> {
        match self {
            BVal::Int(n) => Some(*n),
            _ => None,
        }
    }

    pub fn as_bytes(&self) -> Option<&[u8]> {
        match self {
            BVal::Bytes(b) => Some(b),
            _ => None,
        }
    }

    pub fn as_list(&self) -> Option<&[BVal]> {
        match self {
            BVal::List(xs) => Some(xs),
            _ => None,
        }
    }
}

pub fn encode(v: &BVal) -> Vec<u8> {
    let mut o = Vec::new();
    write_val(&mut o, v);
    o
}

fn write_val(o: &mut Vec<u8>, v: &BVal) {
    match v {
        BVal::Int(n) => {
            o.push(b'i');
            o.extend_from_slice(n.to_string().as_bytes());
            o.push(b'e');
        }
        BVal::Bytes(b) => {
            o.extend_from_slice(b.len().to_string().as_bytes());
            o.push(b':');
            o.extend_from_slice(b);
        }
        BVal::List(xs) => {
            o.push(b'l');
            for x in xs {
                write_val(o, x);
            }
            o.push(b'e');
        }
        BVal::Dict(pairs) => {
            let mut keys: Vec<&[u8]> = pairs.iter().map(|(k, _)| k.as_slice()).collect();
            keys.sort();
            o.push(b'd');
            for k in keys {
                write_val(o, &BVal::Bytes(k.to_vec()));
                let val = pairs.iter().find(|(kk, _)| kk.as_slice() == k).unwrap();
                write_val(o, &val.1);
            }
            o.push(b'e');
        }
    }
}

pub fn decode(b: &[u8]) -> Result<BVal> {
    let (v, rest) = parse(b)?;
    if !rest.is_empty() {
        return Err(Error::Bt("trailing bencode".into()));
    }
    Ok(v)
}

/// Parse one value; returns the value and remaining bytes.
pub fn parse(b: &[u8]) -> Result<(BVal, &[u8])> {
    match b.first() {
        Some(b'i') => {
            let end = b.iter().position(|&c| c == b'e').ok_or_else(|| Error::Bt("int".into()))?;
            let n = std::str::from_utf8(&b[1..end])
                .map_err(|_| Error::Bt("int utf8".into()))?
                .parse::<i64>()
                .map_err(|_| Error::Bt("int parse".into()))?;
            Ok((BVal::Int(n), &b[end + 1..]))
        }
        Some(b'l') => {
            let mut cur = &b[1..];
            let mut xs = Vec::new();
            while cur.first() != Some(&b'e') {
                if cur.is_empty() {
                    return Err(Error::Bt("list eof".into()));
                }
                let (v, rest) = parse(cur)?;
                xs.push(v);
                cur = rest;
            }
            Ok((BVal::List(xs), &cur[1..]))
        }
        Some(b'd') => {
            let mut cur = &b[1..];
            let mut xs = Vec::new();
            while cur.first() != Some(&b'e') {
                if cur.is_empty() {
                    return Err(Error::Bt("dict eof".into()));
                }
                let (k, rest) = parse(cur)?;
                let key = k.as_bytes().ok_or_else(|| Error::Bt("dict key".into()))?.to_vec();
                let (v, rest) = parse(rest)?;
                xs.push((key, v));
                cur = rest;
            }
            Ok((BVal::Dict(xs), &cur[1..]))
        }
        Some(c) if c.is_ascii_digit() => {
            let colon = b.iter().position(|&c| c == b':').ok_or_else(|| Error::Bt("bytes :".into()))?;
            let n: usize = std::str::from_utf8(&b[..colon])
                .map_err(|_| Error::Bt("len utf8".into()))?
                .parse()
                .map_err(|_| Error::Bt("len".into()))?;
            let start = colon + 1;
            let end = start.checked_add(n).ok_or_else(|| Error::Bt("overflow".into()))?;
            if end > b.len() {
                return Err(Error::Bt("bytes eof".into()));
            }
            Ok((BVal::Bytes(b[start..end].to_vec()), &b[end..]))
        }
        _ => Err(Error::Bt("bencode tag".into())),
    }
}

/// Raw `info` dict bytes inside a torrent (for info_hash).
pub fn raw_info_dict(torrent: &[u8]) -> Result<&[u8]> {
    let (top, _) = parse(torrent)?;
    let BVal::Dict(pairs) = top else {
        return Err(Error::Bt("torrent not dict".into()));
    };
    // Re-scan original bytes for the `4:info` key so hashing matches the file.
    let needle = b"4:info";
    let pos = torrent
        .windows(needle.len())
        .position(|w| w == needle)
        .ok_or_else(|| Error::Bt("no info".into()))?;
    let start = pos + needle.len();
    let (_, rest) = parse(&torrent[start..])?;
    let consumed = torrent[start..].len() - rest.len();
    let _ = pairs;
    Ok(&torrent[start..start + consumed])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_int_bytes_list_dict() {
        let v = BVal::Dict(vec![
            (b"bar".to_vec(), BVal::Bytes(b"spam".to_vec())),
            (b"foo".to_vec(), BVal::Int(42)),
        ]);
        let enc = encode(&v);
        assert_eq!(enc, b"d3:bar4:spam3:fooi42ee");
        assert_eq!(decode(&enc).unwrap(), v);
    }
}
