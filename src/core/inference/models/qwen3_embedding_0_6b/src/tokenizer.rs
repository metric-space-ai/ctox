use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenizerInspection {
    pub model_type: String,
    pub vocab_size: usize,
    pub merges_count: usize,
    pub added_tokens_count: usize,
    pub added_tokens: BTreeMap<String, usize>,
}

impl TokenizerInspection {
    pub fn token_id(&self, token: &str) -> Option<usize> {
        self.added_tokens.get(token).copied()
    }

    pub fn total_token_slots(&self) -> usize {
        self.vocab_size + self.added_tokens_count
    }
}

pub fn inspect_tokenizer(path: &Path) -> Result<TokenizerInspection, String> {
    let raw = std::fs::read_to_string(path)
        .map_err(|err| format!("failed to read tokenizer {}: {err}", path.display()))?;
    inspect_tokenizer_json(&raw)
}

pub fn inspect_tokenizer_json(raw: &str) -> Result<TokenizerInspection, String> {
    let model = object_for_key(raw, "model")?;
    let model_type = string_field(model, "type")?;
    let vocab = object_for_key(model, "vocab")?;
    let merges = array_for_key(model, "merges")?;
    let added_tokens = parse_added_tokens(raw)?;
    Ok(TokenizerInspection {
        model_type,
        vocab_size: count_object_entries(vocab)?,
        merges_count: count_top_level_arrays(merges)?,
        added_tokens_count: added_tokens.len(),
        added_tokens,
    })
}

fn parse_added_tokens(raw: &str) -> Result<BTreeMap<String, usize>, String> {
    let array = array_for_key(raw, "added_tokens")?;
    let mut out = BTreeMap::new();
    for object in top_level_objects(array)? {
        out.insert(string_field(object, "content")?, usize_field(object, "id")?);
    }
    Ok(out)
}

fn object_for_key<'a>(raw: &'a str, key: &str) -> Result<&'a str, String> {
    let value = raw_value_for_key(raw, key)?;
    let bytes = value.as_bytes();
    let start = skip_ws(bytes, 0);
    if bytes.get(start) != Some(&b'{') {
        return Err(format!("JSON field `{key}` is not an object"));
    }
    let end = matching_container(bytes, start, b'{', b'}')?;
    Ok(&value[start..=end])
}

fn array_for_key<'a>(raw: &'a str, key: &str) -> Result<&'a str, String> {
    let value = raw_value_for_key(raw, key)?;
    let bytes = value.as_bytes();
    let start = skip_ws(bytes, 0);
    if bytes.get(start) != Some(&b'[') {
        return Err(format!("JSON field `{key}` is not an array"));
    }
    let end = matching_container(bytes, start, b'[', b']')?;
    Ok(&value[start..=end])
}

fn raw_value_for_key<'a>(raw: &'a str, key: &str) -> Result<&'a str, String> {
    let needle = format!("\"{key}\"");
    let start = raw
        .find(&needle)
        .ok_or_else(|| format!("missing JSON field `{key}`"))?;
    let after_key = &raw[start + needle.len()..];
    let colon = after_key
        .find(':')
        .ok_or_else(|| format!("missing JSON colon after `{key}`"))?;
    Ok(after_key[colon + 1..].trim_start())
}

fn string_field(raw: &str, key: &str) -> Result<String, String> {
    let value = raw_value_for_key(raw, key)?;
    let start = skip_ws(value.as_bytes(), 0);
    parse_json_string_at(value.as_bytes(), start).map(|(value, _)| value)
}

fn usize_field(raw: &str, key: &str) -> Result<usize, String> {
    let value = raw_value_for_key(raw, key)?;
    let digits = value
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>();
    if digits.is_empty() {
        return Err(format!("JSON field `{key}` is not an unsigned integer"));
    }
    digits
        .parse::<usize>()
        .map_err(|err| format!("JSON field `{key}` is out of range: {err}"))
}

fn count_object_entries(raw: &str) -> Result<usize, String> {
    let bytes = raw.as_bytes();
    let mut index = skip_ws(bytes, 0);
    if bytes.get(index) != Some(&b'{') {
        return Err("not a JSON object".to_string());
    }
    index += 1;
    let mut count = 0_usize;
    loop {
        index = skip_ws(bytes, index);
        match bytes.get(index) {
            Some(b'}') => return Ok(count),
            Some(b',') => {
                index += 1;
                continue;
            }
            Some(b'"') => {}
            _ => return Err("malformed JSON object".to_string()),
        }
        let (_, next) = parse_json_string_at(bytes, index)?;
        index = skip_ws(bytes, next);
        if bytes.get(index) != Some(&b':') {
            return Err("missing colon in JSON object".to_string());
        }
        index = skip_json_value(bytes, skip_ws(bytes, index + 1))?;
        count += 1;
    }
}

fn count_top_level_arrays(raw: &str) -> Result<usize, String> {
    let bytes = raw.as_bytes();
    let mut index = skip_ws(bytes, 0);
    if bytes.get(index) != Some(&b'[') {
        return Err("not a JSON array".to_string());
    }
    index += 1;
    let mut count = 0_usize;
    loop {
        index = skip_ws(bytes, index);
        match bytes.get(index) {
            Some(b']') => return Ok(count),
            Some(b',') => index += 1,
            Some(b'[') => {
                index = matching_container(bytes, index, b'[', b']')? + 1;
                count += 1;
            }
            _ => return Err("malformed nested JSON array".to_string()),
        }
    }
}

fn top_level_objects(raw: &str) -> Result<Vec<&str>, String> {
    let bytes = raw.as_bytes();
    let mut index = skip_ws(bytes, 0);
    if bytes.get(index) != Some(&b'[') {
        return Err("not a JSON array".to_string());
    }
    index += 1;
    let mut out = Vec::new();
    loop {
        index = skip_ws(bytes, index);
        match bytes.get(index) {
            Some(b']') => return Ok(out),
            Some(b',') => index += 1,
            Some(b'{') => {
                let end = matching_container(bytes, index, b'{', b'}')?;
                out.push(&raw[index..=end]);
                index = end + 1;
            }
            _ => return Err("malformed JSON object array".to_string()),
        }
    }
}

fn skip_json_value(bytes: &[u8], start: usize) -> Result<usize, String> {
    match bytes.get(start) {
        Some(b'{') => matching_container(bytes, start, b'{', b'}').map(|index| index + 1),
        Some(b'[') => matching_container(bytes, start, b'[', b']').map(|index| index + 1),
        Some(b'"') => parse_json_string_at(bytes, start).map(|(_, index)| index),
        Some(_) => {
            let mut index = start;
            while !matches!(bytes.get(index), None | Some(b',' | b'}' | b']')) {
                index += 1;
            }
            Ok(index)
        }
        None => Err("missing JSON value".to_string()),
    }
}

fn skip_ws(bytes: &[u8], mut index: usize) -> usize {
    while matches!(bytes.get(index), Some(b' ' | b'\n' | b'\r' | b'\t')) {
        index += 1;
    }
    index
}

fn parse_json_string_at(bytes: &[u8], start: usize) -> Result<(String, usize), String> {
    if bytes.get(start) != Some(&b'"') {
        return Err("expected JSON string".to_string());
    }
    let mut out = String::new();
    let mut index = start + 1;
    let mut escaped = false;
    while let Some(&byte) = bytes.get(index) {
        if escaped {
            out.push(match byte {
                b'n' => '\n',
                b'r' => '\r',
                b't' => '\t',
                b'"' => '"',
                b'\\' => '\\',
                _ => byte as char,
            });
            escaped = false;
        } else if byte == b'\\' {
            escaped = true;
        } else if byte == b'"' {
            return Ok((out, index + 1));
        } else {
            out.push(byte as char);
        }
        index += 1;
    }
    Err("unterminated JSON string".to_string())
}

fn matching_container(bytes: &[u8], start: usize, open: u8, close: u8) -> Result<usize, String> {
    let mut depth = 0_i32;
    let mut in_string = false;
    let mut escaped = false;
    for (offset, &byte) in bytes[start..].iter().enumerate() {
        if in_string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            }
            continue;
        }
        if byte == b'"' {
            in_string = true;
        } else if byte == open {
            depth += 1;
        } else if byte == close {
            depth -= 1;
            if depth == 0 {
                return Ok(start + offset);
            }
        }
    }
    Err("unterminated JSON container".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inspects_bpe_tokenizer_shape() {
        let raw = r#"{
          "added_tokens": [
            {"id": 151643, "content": "<|endoftext|>", "special": true},
            {"id": 151644, "content": "<|im_start|>", "special": true}
          ],
          "model": {
            "type": "BPE",
            "vocab": {"!": 0, "hello": 14990},
            "merges": [["h", "e"], ["he", "llo"]]
          }
        }"#;
        let inspected = inspect_tokenizer_json(raw).unwrap();
        assert_eq!(inspected.model_type, "BPE");
        assert_eq!(inspected.vocab_size, 2);
        assert_eq!(inspected.merges_count, 2);
        assert_eq!(inspected.added_tokens_count, 2);
        assert_eq!(inspected.token_id("<|endoftext|>"), Some(151643));
        assert_eq!(inspected.total_token_slots(), 4);
    }
}
