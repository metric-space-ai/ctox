//! C++ aria2 XML-RPC (`POST /rpc`) — methodCall/methodResponse, no extra crate.
#![forbid(unsafe_code)]

use crate::error::{Error, Result};
use serde_json::{Map, Number, Value};

pub fn parse_method_call(xml: &str) -> Result<(String, Value)> {
    let method = extract_tag(xml, "methodName")
        .map(|(c, _)| xml_unescape(c.trim()))
        .ok_or_else(|| Error::Rpc("xml-rpc methodName".into()))?;
    let params_inner = extract_tag(xml, "params")
        .map(|(c, _)| c)
        .unwrap_or("");
    let mut params = Vec::new();
    let mut rest = params_inner;
    while let Some((param, next)) = extract_tag(rest, "param") {
        let (val, _) = parse_value(param)?;
        params.push(val);
        rest = next;
    }
    Ok((method, Value::Array(params)))
}

pub fn encode_response(result: &Value) -> String {
    format!(
        "<?xml version=\"1.0\"?>\n<methodResponse><params><param>{}</param></params></methodResponse>\n",
        encode_value(result)
    )
}

pub fn encode_fault(code: i32, msg: &str) -> String {
    format!(
        "<?xml version=\"1.0\"?>\n<methodResponse><fault><value><struct><member><name>faultCode</name><value><int>{code}</int></value></member><member><name>faultString</name><value><string>{}</string></value></member></struct></value></fault></methodResponse>\n",
        xml_escape(msg)
    )
}

fn parse_value(s: &str) -> Result<(Value, &str)> {
    let s = s.trim_start();
    let (inner, rest) = extract_tag(s, "value").ok_or_else(|| Error::Rpc("xml-rpc value".into()))?;
    let inner = inner.trim();
    if inner.is_empty() {
        return Ok((Value::String(String::new()), rest));
    }
    if !inner.starts_with('<') {
        return Ok((Value::String(xml_unescape(inner)), rest));
    }
    if let Some((c, _)) = extract_tag_here(inner, "string") {
        return Ok((Value::String(xml_unescape(c)), rest));
    }
    if let Some((c, _)) = extract_tag_here(inner, "int").or_else(|| extract_tag_here(inner, "i4")) {
        let n: i64 = c.trim().parse().unwrap_or(0);
        return Ok((Value::Number(n.into()), rest));
    }
    if let Some((c, _)) = extract_tag_here(inner, "i8") {
        let n: i64 = c.trim().parse().unwrap_or(0);
        return Ok((Value::Number(n.into()), rest));
    }
    if let Some((c, _)) = extract_tag_here(inner, "boolean") {
        let b = c.trim() == "1" || c.trim().eq_ignore_ascii_case("true");
        return Ok((Value::Bool(b), rest));
    }
    if let Some((c, _)) = extract_tag_here(inner, "double") {
        if let Ok(f) = c.trim().parse::<f64>() {
            if let Some(n) = Number::from_f64(f) {
                return Ok((Value::Number(n), rest));
            }
        }
        return Ok((Value::String(c.trim().to_string()), rest));
    }
    if let Some((c, _)) = extract_tag_here(inner, "base64") {
        return Ok((Value::String(c.trim().to_string()), rest));
    }
    if let Some((arr, _)) = extract_tag_here(inner, "array") {
        let data = extract_tag_here(arr, "data").map(|(c, _)| c).unwrap_or(arr);
        let mut items = Vec::new();
        let mut rest_d = data;
        while let Ok((v, next)) = parse_value(rest_d) {
            items.push(v);
            rest_d = next;
            if rest_d.trim().is_empty() {
                break;
            }
        }
        return Ok((Value::Array(items), rest));
    }
    if let Some((st, _)) = extract_tag_here(inner, "struct") {
        let mut map = Map::new();
        let mut rest_m = st;
        while let Some((member, next)) = extract_tag(rest_m, "member") {
            let name = extract_tag(member, "name")
                .map(|(c, _)| xml_unescape(c.trim()))
                .unwrap_or_default();
            let (val, _) = parse_value(member)?;
            if !name.is_empty() {
                map.insert(name, val);
            }
            rest_m = next;
        }
        return Ok((Value::Object(map), rest));
    }
    Ok((Value::String(xml_unescape(inner)), rest))
}

fn encode_value(v: &Value) -> String {
    match v {
        Value::Null => "<value><string></string></value>".into(),
        Value::Bool(b) => format!("<value><boolean>{}</boolean></value>", if *b { 1 } else { 0 }),
        Value::Number(n) => {
            if n.is_i64() || n.is_u64() {
                format!("<value><int>{}</int></value>", n)
            } else {
                format!("<value><double>{}</double></value>", n)
            }
        }
        Value::String(s) => format!("<value><string>{}</string></value>", xml_escape(s)),
        Value::Array(a) => {
            let mut o = String::from("<value><array><data>");
            for x in a {
                o.push_str(&encode_value(x));
            }
            o.push_str("</data></array></value>");
            o
        }
        Value::Object(m) => {
            let mut o = String::from("<value><struct>");
            for (k, val) in m {
                o.push_str("<member><name>");
                o.push_str(&xml_escape(k));
                o.push_str("</name>");
                o.push_str(&encode_value(val));
                o.push_str("</member>");
            }
            o.push_str("</struct></value>");
            o
        }
    }
}

fn tag_boundary(b: Option<u8>) -> bool {
    matches!(b, Some(b'>' | b'/' | b' ' | b'\t' | b'\n' | b'\r') | None)
}

fn is_open_tag_at(s: &str, tag: &str) -> bool {
    if s.len() < tag.len() + 1 || s.as_bytes()[0] != b'<' {
        return false;
    }
    let n = &s[1..];
    n.len() >= tag.len()
        && n[..tag.len()].eq_ignore_ascii_case(tag)
        && tag_boundary(n.as_bytes().get(tag.len()).copied())
}

fn is_close_tag_at(s: &str, tag: &str) -> bool {
    let close = format!("</{tag}>");
    s.len() >= close.len() && s[..close.len()].eq_ignore_ascii_case(&close)
}

fn extract_tag_here<'a>(input: &'a str, tag: &str) -> Option<(&'a str, &'a str)> {
    let s = input.trim_start();
    if !is_open_tag_at(s, tag) {
        return None;
    }
    extract_open_tag(s, tag)
}

fn extract_tag<'a>(input: &'a str, tag: &str) -> Option<(&'a str, &'a str)> {
    let s = input.trim_start();
    if s.is_empty() {
        return None;
    }
    if is_open_tag_at(s, tag) {
        return extract_open_tag(s, tag);
    }
    let mut idx = 1;
    while idx < s.len() {
        if is_open_tag_at(&s[idx..], tag) {
            return extract_open_tag(&s[idx..], tag);
        }
        idx += 1;
    }
    None
}

fn extract_open_tag<'a>(s: &'a str, tag: &str) -> Option<(&'a str, &'a str)> {
    let after_lt = &s[1..];
    let after_name = &after_lt[tag.len()..];
    let gt = after_name.find('>')?;
    let open_inner = &after_name[..gt];
    let start = 1 + tag.len() + gt + 1;
    if open_inner.trim_end().ends_with('/') {
        return Some(("", &s[start..]));
    }
    let close_len = tag.len() + 3;
    let content = &s[start..];
    let mut depth = 1i32;
    let mut i = 0;
    while i < content.len() {
        let rest = &content[i..];
        if is_close_tag_at(rest, tag) {
            depth -= 1;
            if depth == 0 {
                return Some((&content[..i], &content[i + close_len..]));
            }
            i += close_len;
            continue;
        }
        if is_open_tag_at(rest, tag) {
            depth += 1;
        }
        i += 1;
    }
    None
}

fn xml_escape(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("\u{0026}amp;"),
            '<' => o.push_str("\u{0026}lt;"),
            '>' => o.push_str("\u{0026}gt;"),
            '"' => o.push_str("\u{0026}quot;"),
            '\'' => o.push_str("\u{0026}apos;"),
            _ => o.push(c),
        }
    }
    o
}

fn xml_unescape(s: &str) -> String {
    s.replace("\u{0026}lt;", "<")
        .replace("\u{0026}gt;", ">")
        .replace("\u{0026}quot;", "\"")
        .replace("\u{0026}apos;", "'")
        .replace("\u{0026}amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_add_uri_call() {
        let xml = r#"<?xml version="1.0"?>
<methodCall>
<methodName>aria2.addUri</methodName>
<params>
<param><value><array><data>
<value><string>http://127.0.0.1/a.bin</string></value>
</data></array></value></param>
<param><value><struct>
<member><name>out</name><value><string>a.bin</string></value></member>
</struct></value></param>
</params>
</methodCall>"#;
        let (m, p) = parse_method_call(xml).unwrap();
        assert_eq!(m, "aria2.addUri");
        let arr = p.as_array().unwrap();
        assert_eq!(arr[0][0], json!("http://127.0.0.1/a.bin"));
        assert_eq!(arr[1]["out"], json!("a.bin"));
    }

    #[test]
    fn encode_string_roundtrip_shape() {
        let xml = encode_response(&json!("gidhex"));
        assert!(xml.contains("<string>gidhex</string>"));
        assert!(xml.contains("<methodResponse>"));
    }
}
