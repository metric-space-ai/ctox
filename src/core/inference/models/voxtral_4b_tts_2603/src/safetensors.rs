//! Minimal safetensors reader without serde or external crates.
//!
//! Format summary:
//! - 8-byte little-endian `u64` JSON header length;
//! - UTF-8 JSON header;
//! - raw tensor data block. `data_offsets` are relative to the data block.

use crate::mmap::Mmap;
use crate::tensor::{DType, TensorBytes, TensorInfo};
use crate::{Error, Result};
use std::fs::File;
use std::path::Path;

pub struct SafeTensors {
    mmap: Mmap,
    data_base: usize,
    tensors: Vec<TensorInfo>,
}

impl SafeTensors {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let file = File::open(path)?;
        let mmap = Mmap::map_readonly(&file)?;
        let bytes = mmap.as_slice();
        if bytes.len() < 8 {
            return Err(Error::InvalidFormat("safetensors file too short"));
        }
        let mut len_bytes = [0u8; 8];
        len_bytes.copy_from_slice(&bytes[..8]);
        let header_len = u64::from_le_bytes(len_bytes) as usize;
        let header_start = 8usize;
        let header_end = header_start
            .checked_add(header_len)
            .ok_or(Error::InvalidFormat("header length overflow"))?;
        if header_end > bytes.len() {
            return Err(Error::InvalidFormat("header exceeds file size"));
        }
        let header = std::str::from_utf8(&bytes[header_start..header_end])?;
        let data_base = header_end;
        let mut tensors = parse_header(header, data_base)?;
        tensors.sort_by(|a, b| a.name.cmp(&b.name));
        for t in &tensors {
            t.validate_byte_len()?;
            if t.data_end > bytes.len() {
                return Err(Error::InvalidFormat("tensor data exceeds file size"));
            }
        }
        Ok(Self {
            mmap,
            data_base,
            tensors,
        })
    }

    pub fn data_base(&self) -> usize {
        self.data_base
    }
    pub fn tensors(&self) -> &[TensorInfo] {
        &self.tensors
    }

    pub fn find(&self, name: &str) -> Option<&TensorInfo> {
        self.tensors
            .binary_search_by(|t| t.name.as_str().cmp(name))
            .ok()
            .map(|i| &self.tensors[i])
    }

    pub fn tensor(&self, name: &str) -> Result<TensorBytes<'_>> {
        let info = self
            .find(name)
            .ok_or_else(|| Error::MissingTensor(name.to_owned()))?;
        Ok(TensorBytes {
            info,
            bytes: &self.mmap.as_slice()[info.data_start..info.data_end],
        })
    }
}

fn parse_header(header: &str, data_base: usize) -> Result<Vec<TensorInfo>> {
    let bytes = header.as_bytes();
    let mut out = Vec::new();
    let mut pos = skip_ws(bytes, 0);
    if bytes.get(pos) != Some(&b'{') {
        return Err(Error::InvalidFormat("safetensors header is not object"));
    }
    pos += 1;

    loop {
        pos = skip_ws(bytes, pos);
        if pos >= bytes.len() {
            return Err(Error::InvalidFormat("unterminated header object"));
        }
        if bytes[pos] == b'}' {
            break;
        }
        if bytes[pos] == b',' {
            pos += 1;
            continue;
        }
        if bytes[pos] != b'"' {
            return Err(Error::InvalidFormat("expected tensor name string"));
        }
        let (name, next) = parse_json_string(bytes, pos)?;
        pos = skip_ws(bytes, next);
        if bytes.get(pos) != Some(&b':') {
            return Err(Error::InvalidFormat("expected ':' after tensor name"));
        }
        pos = skip_ws(bytes, pos + 1);
        if name == "__metadata__" {
            pos = skip_json_value(bytes, pos)?;
            continue;
        }
        if bytes.get(pos) != Some(&b'{') {
            return Err(Error::InvalidFormat("expected tensor metadata object"));
        }
        let end = matching_brace(bytes, pos)?;
        let obj = std::str::from_utf8(&bytes[pos..=end])?;
        let dtype = parse_string_field(obj, "dtype")?;
        let shape = parse_usize_array_field(obj, "shape")?;
        let offsets = parse_usize_array_field(obj, "data_offsets")?;
        if offsets.len() != 2 {
            return Err(Error::InvalidFormat("data_offsets must have two entries"));
        }
        let data_start = data_base
            .checked_add(offsets[0])
            .ok_or(Error::InvalidFormat("data offset overflow"))?;
        let data_end = data_base
            .checked_add(offsets[1])
            .ok_or(Error::InvalidFormat("data offset overflow"))?;
        out.push(TensorInfo {
            name,
            dtype: DType::from_safetensors(&dtype),
            shape,
            data_start,
            data_end,
        });
        pos = end + 1;
    }
    Ok(out)
}

fn skip_ws(bytes: &[u8], mut pos: usize) -> usize {
    while let Some(b) = bytes.get(pos) {
        if matches!(*b, b' ' | b'\n' | b'\r' | b'\t') {
            pos += 1;
        } else {
            break;
        }
    }
    pos
}

fn parse_json_string(bytes: &[u8], start: usize) -> Result<(String, usize)> {
    if bytes.get(start) != Some(&b'"') {
        return Err(Error::InvalidFormat("expected JSON string"));
    }
    let mut out = String::new();
    let mut i = start + 1;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => return Ok((out, i + 1)),
            b'\\' => {
                i += 1;
                if i >= bytes.len() {
                    return Err(Error::InvalidFormat("unterminated escape"));
                }
                let ch = match bytes[i] {
                    b'"' => '"',
                    b'\\' => '\\',
                    b'/' => '/',
                    b'b' => '\u{0008}',
                    b'f' => '\u{000c}',
                    b'n' => '\n',
                    b'r' => '\r',
                    b't' => '\t',
                    // Tensor names in Voxtral do not need unicode escapes; keep parser small.
                    b'u' => {
                        return Err(Error::Unsupported("unicode escapes in safetensors header"))
                    }
                    _ => return Err(Error::InvalidFormat("invalid JSON escape")),
                };
                out.push(ch);
                i += 1;
            }
            b => {
                out.push(b as char);
                i += 1;
            }
        }
    }
    Err(Error::InvalidFormat("unterminated JSON string"))
}

fn matching_brace(bytes: &[u8], start: usize) -> Result<usize> {
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escape = false;
    for (i, b) in bytes.iter().enumerate().skip(start) {
        if in_string {
            if escape {
                escape = false;
            } else if *b == b'\\' {
                escape = true;
            } else if *b == b'"' {
                in_string = false;
            }
            continue;
        }
        match *b {
            b'"' => in_string = true,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Ok(i);
                }
            }
            _ => {}
        }
    }
    Err(Error::InvalidFormat("unterminated object"))
}

fn skip_json_value(bytes: &[u8], pos: usize) -> Result<usize> {
    let pos = skip_ws(bytes, pos);
    match bytes.get(pos).copied() {
        Some(b'{') => matching_brace(bytes, pos).map(|e| e + 1),
        Some(b'[') => matching_bracket(bytes, pos).map(|e| e + 1),
        Some(b'"') => parse_json_string(bytes, pos).map(|(_, next)| next),
        Some(_) => {
            let mut i = pos;
            while i < bytes.len() && !matches!(bytes[i], b',' | b'}' | b']') {
                i += 1;
            }
            Ok(i)
        }
        None => Err(Error::InvalidFormat("unexpected EOF while skipping value")),
    }
}

fn matching_bracket(bytes: &[u8], start: usize) -> Result<usize> {
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escape = false;
    for (i, b) in bytes.iter().enumerate().skip(start) {
        if in_string {
            if escape {
                escape = false;
            } else if *b == b'\\' {
                escape = true;
            } else if *b == b'"' {
                in_string = false;
            }
            continue;
        }
        match *b {
            b'"' => in_string = true,
            b'[' => depth += 1,
            b']' => {
                depth -= 1;
                if depth == 0 {
                    return Ok(i);
                }
            }
            _ => {}
        }
    }
    Err(Error::InvalidFormat("unterminated array"))
}

fn parse_string_field(obj: &str, field: &str) -> Result<String> {
    let needle = format!("\"{field}\"");
    let idx = obj
        .find(&needle)
        .ok_or_else(|| Error::Parse(format!("missing string field {field}")))?;
    let rest = &obj[idx + needle.len()..];
    let colon = rest
        .find(':')
        .ok_or(Error::InvalidFormat("missing ':' in string field"))?;
    let bytes = rest[colon + 1..].as_bytes();
    let start = skip_ws(bytes, 0);
    let (s, _) = parse_json_string(bytes, start)?;
    Ok(s)
}

fn parse_usize_array_field(obj: &str, field: &str) -> Result<Vec<usize>> {
    let needle = format!("\"{field}\"");
    let idx = obj
        .find(&needle)
        .ok_or_else(|| Error::Parse(format!("missing array field {field}")))?;
    let rest = &obj[idx + needle.len()..];
    let colon = rest
        .find(':')
        .ok_or(Error::InvalidFormat("missing ':' in array field"))?;
    let bytes = rest[colon + 1..].as_bytes();
    let start = skip_ws(bytes, 0);
    if bytes.get(start) != Some(&b'[') {
        return Err(Error::InvalidFormat("expected array"));
    }
    let end = matching_bracket(bytes, start)?;
    let body = std::str::from_utf8(&bytes[start + 1..end])?;
    let mut out = Vec::new();
    for part in body.split(',') {
        let p = part.trim();
        if p.is_empty() {
            continue;
        }
        let v: usize = p
            .parse()
            .map_err(|_| Error::Parse(format!("invalid usize in {field}: {p}")))?;
        out.push(v);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_header() {
        let h = r#"{"x":{"dtype":"BF16","shape":[2,3],"data_offsets":[0,12]},"__metadata__":{"foo":"bar"}}"#;
        let ts = parse_header(h, 8).unwrap();
        assert_eq!(ts.len(), 1);
        assert_eq!(ts[0].name, "x");
        assert_eq!(ts[0].shape, vec![2, 3]);
        assert_eq!(ts[0].data_start, 8);
        assert_eq!(ts[0].data_end, 20);
    }
}
