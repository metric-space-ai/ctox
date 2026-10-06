//! Metalink 4 (RFC 5854) and Metalink 3: pick HTTP(S)/FTP/SFTP URLs and dest names.
#![forbid(unsafe_code)]

use crate::error::{Error, Result};
use crate::options::OptionSet;

#[derive(Clone, Debug)]
pub struct MetalinkUrl {
    pub url: String,
    pub priority: u32,
    pub location: Option<String>,
    pub proto: Option<String>,
}

/// C++ Metalink chunk checksums (`<pieces length= type=>`).
#[derive(Clone, Debug)]
pub struct PieceChecksum {
    pub hash_type: String,
    pub length: u64,
    pub hashes: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct MetalinkFile {
    pub name: String,
    pub size: Option<u64>,
    pub hash_type: Option<String>,
    pub hash: Option<String>,
    pub language: Option<String>,
    pub os: Option<String>,
    pub version: Option<String>,
    pub urls: Vec<MetalinkUrl>,
    pub pieces: Option<PieceChecksum>,
}

impl MetalinkFile {
    pub fn preferred_urls(&self) -> Vec<String> {
        let mut u = self.urls.clone();
        u.sort_by_key(|x| x.priority);
        u.into_iter().map(|x| x.url).collect()
    }

    pub fn matches_meta(&self, opts: &OptionSet) -> bool {
        if let Some(want) = opts.get("metalink-version").filter(|s| !s.is_empty()) {
            if self.version.as_deref() != Some(want) {
                return false;
            }
        }
        if let Some(want) = opts.get("metalink-language").filter(|s| !s.is_empty()) {
            if !lang_match(self.language.as_deref(), want) {
                return false;
            }
        }
        if let Some(want) = opts.get("metalink-os").filter(|s| !s.is_empty()) {
            if !self
                .os
                .as_deref()
                .unwrap_or("")
                .eq_ignore_ascii_case(want)
            {
                return false;
            }
        }
        true
    }

    pub fn select_urls(&self, opts: &OptionSet) -> Vec<String> {
        let mut urls = self.urls.clone();
        if let Some(base) = opts.get("metalink-base-uri").filter(|s| !s.is_empty()) {
            for u in &mut urls {
                u.url = resolve_metalink_base(base, &u.url);
                if u.proto.is_none() {
                    u.proto = u
                        .url
                        .split("://")
                        .next()
                        .map(|s| s.to_ascii_lowercase());
                }
            }
        }
        if let Some(loc) = opts.get("metalink-location").filter(|s| !s.is_empty()) {
            let matched: Vec<_> = urls
                .iter()
                .filter(|u| {
                    u.location
                        .as_deref()
                        .is_some_and(|l| l.eq_ignore_ascii_case(loc))
                })
                .cloned()
                .collect();
            if !matched.is_empty() {
                urls = matched;
            }
        }
        let proto = opts
            .get("metalink-preferred-protocol")
            .filter(|s| !s.is_empty())
            .map(|s| s.to_ascii_lowercase());
        if let Some(p) = proto.as_deref() {
            let matched: Vec<_> = urls
                .iter()
                .filter(|u| url_proto(u).eq_ignore_ascii_case(p))
                .cloned()
                .collect();
            if opts.bool("metalink-enable-unique-protocol", true) && !matched.is_empty() {
                urls = matched;
            }
        }
        urls.sort_by(|a, b| {
            let pa = proto
                .as_deref()
                .map(|p| if url_proto(a).eq_ignore_ascii_case(p) { 0 } else { 1 })
                .unwrap_or(0);
            let pb = proto
                .as_deref()
                .map(|p| if url_proto(b).eq_ignore_ascii_case(p) { 0 } else { 1 })
                .unwrap_or(0);
            pa.cmp(&pb).then(a.priority.cmp(&b.priority))
        });
        urls.into_iter()
            .map(|x| x.url)
            .filter(|u| u.contains("://"))
            .collect()
    }

    pub fn checksum_spec(&self) -> Option<String> {
        let ty = self.hash_type.as_deref()?;
        let dig = self.hash.as_deref()?;
        let canon = crate::checksum::canonicalize_hash_type(ty)?;
        Some(format!("{canon}={dig}"))
    }

    /// Encode for `--piece-checksum` (sha-1:LEN:hex,hex).
    pub fn piece_checksum_spec(&self) -> Option<String> {
        let p = self.pieces.as_ref()?;
        if p.hashes.is_empty() || p.length == 0 {
            return None;
        }
        let ty = crate::checksum::canonicalize_hash_type(&p.hash_type)?;
        Some(format!("{ty}:{}:{}", p.length, p.hashes.join(",")))
    }
}

pub fn looks_like_metalink_uri(uri: &str) -> bool {
    let u = uri.split('?').next().unwrap_or(uri).to_ascii_lowercase();
    u.ends_with(".meta4") || u.ends_with(".metalink")
}

pub fn parse(xml: &str) -> Result<Vec<MetalinkFile>> {
    let mut files = Vec::new();
    let mut rest = xml;
    while let Some(rel) = find_open_tag(rest, "file") {
        let after = &rest[rel..];
        let end = find_close_tag(after, "file").unwrap_or(after.len());
        let block = &after[..end];
        if let Some(f) = parse_file_block(block) {
            files.push(f);
        }
        rest = if end < after.len() { &after[end..] } else { "" };
    }
    if files.is_empty() {
        return Err(Error::Other("metalink: no files".into()));
    }
    Ok(files)
}

pub fn parse_bytes(bytes: &[u8]) -> Result<Vec<MetalinkFile>> {
    let xml = std::str::from_utf8(bytes).map_err(|_| Error::Other("metalink not utf-8".into()))?;
    parse(xml)
}

/// C++ `--show-files` listing for .meta4/.metalink (print and do not download).
pub fn format_show_files(files: &[MetalinkFile]) -> String {
    let mut o = String::from(
        "Files:\nidx|path/length\n===+===========================================================================\n",
    );
    for (i, f) in files.iter().enumerate() {
        let sz = f.size.map(|n| format!("{n}B")).unwrap_or_else(|| "-".into());
        o.push_str(&format!(
            "{:>3}.|{}\n   |{}\n---+---------------------------------------------------------------------------\n",
            i + 1,
            f.name,
            sz
        ));
    }
    o
}

pub fn filter_files(files: Vec<MetalinkFile>, opts: &OptionSet) -> Vec<MetalinkFile> {
    let chosen: Vec<_> = files
        .iter()
        .filter(|f| f.matches_meta(opts))
        .cloned()
        .collect();
    if chosen.is_empty() {
        files
    } else {
        chosen
    }
}

fn is_metalink_url(url: &str) -> bool {
    if url.is_empty() {
        return false;
    }
    url.starts_with("http://")
        || url.starts_with("https://")
        || url.starts_with("ftp://")
        || url.starts_with("sftp://")
        || !url.contains("://")
}

/// C++ `--metalink-base-uri`: resolve relative metalink URLs (directory URI must end with /).
pub fn resolve_metalink_base(base: &str, url: &str) -> String {
    if url.contains("://") {
        return url.to_string();
    }
    if base.is_empty() {
        return url.to_string();
    }
    let rel = url.trim_start_matches('/');
    if base.ends_with('/') {
        format!("{base}{rel}")
    } else {
        format!("{base}/{rel}")
    }
}

fn lang_match(file: Option<&str>, want: &str) -> bool {
    let Some(f) = file else {
        return false;
    };
    let f = f.to_ascii_lowercase();
    let w = want.to_ascii_lowercase();
    f == w || f.starts_with(&format!("{w}-"))
}

fn url_proto(u: &MetalinkUrl) -> String {
    u.proto
        .clone()
        .or_else(|| u.url.split("://").next().map(|s| s.to_ascii_lowercase()))
        .unwrap_or_default()
}

fn parse_file_block(block: &str) -> Option<MetalinkFile> {
    let head_end = block.find('>').unwrap_or(block.len());
    let head = &block[..head_end];
    let name = attr(head, "name").or_else(|| text_of(block, "name"))?;
    if name.is_empty() {
        return None;
    }
    let size = text_of(block, "size").and_then(|s| s.parse().ok());
    let pieces = pieces_of(block);
    let (hash_type, hash) = hash_of(block, pieces.as_ref());
    let mut urls = Vec::new();
    let mut rest = block;
    while let Some(rel) = find_open_tag(rest, "url") {
        let tag = &rest[rel..];
        let gt = tag.find('>')?;
        let open = &tag[..gt];
        let after = &tag[gt + 1..];
        let close = find_close_tag(after, "url").unwrap_or(after.len());
        let body = unescape(&after[..close.min(after.len())]);
        let url = body.trim().to_string();
        if is_metalink_url(&url) {
            let priority = attr(open, "priority")
                .and_then(|s| s.parse().ok())
                .or_else(|| {
                    attr(open, "preference")
                        .and_then(|s| s.parse::<u32>().ok())
                        .map(|p| 100u32.saturating_sub(p.min(100)))
                })
                .unwrap_or(1);
            let location = attr(open, "location");
            let proto = attr(open, "type").or_else(|| {
                url.split("://").next().map(|s| s.to_ascii_lowercase())
            });
            urls.push(MetalinkUrl {
                url,
                priority,
                location,
                proto,
            });
        }
        rest = if close < after.len() { &after[close..] } else { "" };
    }
    if urls.is_empty() {
        return None;
    }
    Some(MetalinkFile {
        name,
        size,
        hash_type,
        hash,
        language: text_of(block, "language"),
        os: text_of(block, "os"),
        version: text_of(block, "version"),
        urls,
        pieces,
    })
}

fn pieces_span(block: &str) -> Option<(usize, usize)> {
    let rel = find_open_tag(block, "pieces")?;
    let after = &block[rel..];
    let gt = after.find('>')?;
    let inner = &after[gt + 1..];
    let close = find_close_tag(inner, "pieces")?;
    Some((rel, rel + gt + 1 + close))
}

fn pieces_of(block: &str) -> Option<PieceChecksum> {
    let rel = find_open_tag(block, "pieces")?;
    let tag = &block[rel..];
    let gt = tag.find('>')?;
    let open = &tag[..gt];
    let ty = attr(open, "type")?;
    let length: u64 = attr(open, "length")?.parse().ok()?;
    if length == 0 {
        return None;
    }
    let after = &tag[gt + 1..];
    let close = find_close_tag(after, "pieces").unwrap_or(after.len());
    let inner = &after[..close];
    let mut hashes = Vec::new();
    let mut rest = inner;
    while let Some(hr) = find_open_tag(rest, "hash") {
        let htag = &rest[hr..];
        let Some(hgt) = htag.find('>') else {
            break;
        };
        let hafter = &htag[hgt + 1..];
        let hclose = find_close_tag(hafter, "hash").unwrap_or(hafter.len());
        let dig = hafter[..hclose.min(hafter.len())]
            .trim()
            .to_ascii_lowercase();
        if !dig.is_empty() {
            hashes.push(dig);
        }
        rest = if hclose < hafter.len() {
            &hafter[hclose..]
        } else {
            ""
        };
    }
    if hashes.is_empty() {
        return None;
    }
    Some(PieceChecksum {
        hash_type: ty,
        length,
        hashes,
    })
}

fn hash_of(block: &str, pieces: Option<&PieceChecksum>) -> (Option<String>, Option<String>) {
    let span = if pieces.is_some() {
        pieces_span(block)
    } else {
        None
    };
    let mut offset = 0usize;
    while offset < block.len() {
        let search = &block[offset..];
        let rel = match find_open_tag(search, "hash") {
            Some(r) => r,
            None => return (None, None),
        };
        let abs = offset + rel;
        if let Some((ps, pe)) = span {
            if abs >= ps && abs < pe {
                offset = abs + 1;
                continue;
            }
        }
        let tag = &block[abs..];
        let Some(gt) = tag.find('>') else {
            return (None, None);
        };
        let ty = attr(&tag[..gt], "type");
        let after = &tag[gt + 1..];
        let close = find_close_tag(after, "hash").unwrap_or(after.len());
        let dig = after[..close.min(after.len())].trim().to_ascii_lowercase();
        if dig.is_empty() {
            return (ty, None);
        }
        return (ty, Some(dig));
    }
    (None, None)
}

fn text_of(block: &str, tag: &str) -> Option<String> {
    let rel = find_open_tag(block, tag)?;
    let after_open = &block[rel..];
    let gt = after_open.find('>')?;
    let after = &after_open[gt + 1..];
    let close = find_close_tag(after, tag)?;
    let s = unescape(after[..close].trim());
    if s.is_empty() { None } else { Some(s) }
}

fn attr(open: &str, name: &str) -> Option<String> {
    let pat = format!("{name}=");
    let l = open.to_ascii_lowercase();
    let i = l.find(&pat)?;
    let rest = &open[i + pat.len()..];
    let q = rest.chars().next()?;
    if q != '"' && q != '\'' {
        return None;
    }
    let inner = &rest[1..];
    let end = inner.find(q)?;
    Some(unescape(&inner[..end]))
}

fn unescape(s: &str) -> String {
    let amp: &[u8] = &[b'&', b'a', b'm', b'p', b';'];
    let lt: &[u8] = &[b'&', b'l', b't', b';'];
    let gt: &[u8] = &[b'&', b'g', b't', b';'];
    let quot: &[u8] = &[b'&', b'q', b'u', b'o', b't', b';'];
    let apos: &[u8] = &[b'&', b'a', b'p', b'o', b's', b';'];
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'&' {
            if bytes[i..].starts_with(amp) {
                out.push('&');
                i += amp.len();
                continue;
            }
            if bytes[i..].starts_with(lt) {
                out.push('<');
                i += lt.len();
                continue;
            }
            if bytes[i..].starts_with(gt) {
                out.push('>');
                i += gt.len();
                continue;
            }
            if bytes[i..].starts_with(quot) {
                out.push('\u{22}');
                i += quot.len();
                continue;
            }
            if bytes[i..].starts_with(apos) {
                out.push('\'');
                i += apos.len();
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

fn find_open_tag(hay: &str, name: &str) -> Option<usize> {
    let h = hay.as_bytes();
    let n = name.as_bytes();
    let mut i = 0;
    while i + n.len() + 1 < h.len() {
        if h[i] == b'<' {
            let mut j = i + 1;
            while j < h.len() && h[j] != b'/' && h[j] != b'>' && h[j] != b' ' && h[j] != b'\t' && h[j] != b'\n' {
                j += 1;
            }
            let tag = &hay[i + 1..j];
            let local = tag.rsplit(':').next().unwrap_or(tag);
            if local.eq_ignore_ascii_case(name) {
                return Some(i);
            }
        }
        i += 1;
    }
    None
}

fn find_close_tag(hay: &str, name: &str) -> Option<usize> {
    let h = hay.as_bytes();
    let mut i = 0;
    while i + name.len() + 3 < h.len() {
        if h[i] == b'<' && h[i + 1] == b'/' {
            let mut j = i + 2;
            while j < h.len() && h[j] != b'>' && h[j] != b' ' {
                j += 1;
            }
            let tag = &hay[i + 2..j];
            let local = tag.rsplit(':').next().unwrap_or(tag);
            if local.eq_ignore_ascii_case(name) {
                return Some(i);
            }
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_metalink4_http_url() {
        let xml = r#"<?xml version="1.0"?>
<metalink xmlns="urn:ietf:params:xml:ns:metalink">
  <file name="ml.bin">
    <size>4</size>
    <hash type="sha-1">da39a3ee5e6b4b0d3255bfef95601890afd80709</hash>
    <url priority="2">http://b.example/ml.bin</url>
    <url priority="1">http://a.example/ml.bin</url>
  </file>
</metalink>"#;
        let files = parse(xml).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].name, "ml.bin");
        assert_eq!(files[0].preferred_urls()[0], "http://a.example/ml.bin");
        assert_eq!(
            files[0].checksum_spec().unwrap(),
            "sha-1=da39a3ee5e6b4b0d3255bfef95601890afd80709"
        );
    }

    #[test]
    fn parse_metalink3_url() {
        let xml = r#"<metalink version="3.0" xmlns="http://www.metalinker.org/">
  <files>
    <file name="x.bin">
      <resources>
        <url type="http">http://ex/x.bin</url>
      </resources>
    </file>
  </files>
</metalink>"#;
        let files = parse(xml).unwrap();
        assert_eq!(files[0].preferred_urls()[0], "http://ex/x.bin");
    }

    #[test]
    fn parse_metalink4_pieces_skips_chunk_hashes_for_file() {
        let xml = r#"<metalink xmlns="urn:ietf:params:xml:ns:metalink">
  <file name="p.bin">
    <size>16</size>
    <hash type="sha-1">aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa</hash>
    <pieces length="8" type="sha-1">
      <hash>1111111111111111111111111111111111111111</hash>
      <hash>2222222222222222222222222222222222222222</hash>
    </pieces>
    <url>http://ex/p.bin</url>
  </file>
</metalink>"#;
        let files = parse(xml).unwrap();
        assert_eq!(
            files[0].checksum_spec().unwrap(),
            "sha-1=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        );
        assert_eq!(
            files[0].piece_checksum_spec().unwrap(),
            "sha-1:8:1111111111111111111111111111111111111111,2222222222222222222222222222222222222222"
        );
    }

    #[test]
    fn parse_metalink4_sha256_checksum_spec() {
        let xml = r#"<metalink xmlns="urn:ietf:params:xml:ns:metalink">
  <file name="s.bin">
    <size>3</size>
    <hash type="sha-256">ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad</hash>
    <pieces length="3" type="sha-256">
      <hash>ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad</hash>
    </pieces>
    <url>http://ex/s.bin</url>
  </file>
</metalink>"#;
        let files = parse(xml).unwrap();
        assert_eq!(
            files[0].checksum_spec().unwrap(),
            "sha-256=ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            files[0].piece_checksum_spec().unwrap(),
            "sha-256:3:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn metalink_base_uri_resolves_relative() {
        let xml = r#"<metalink xmlns="urn:ietf:params:xml:ns:metalink">
  <file name="rel.bin">
    <url>rel.bin</url>
  </file>
</metalink>"#;
        let files = parse(xml).unwrap();
        let mut opts = OptionSet::new();
        assert!(files[0].select_urls(&opts).is_empty());
        opts.set("metalink-base-uri", "http://127.0.0.1:9/");
        assert_eq!(
            files[0].select_urls(&opts),
            vec!["http://127.0.0.1:9/rel.bin".to_string()]
        );
        assert_eq!(
            resolve_metalink_base("http://ex/dir", "x.bin"),
            "http://ex/dir/x.bin"
        );
        assert_eq!(
            resolve_metalink_base("http://ex/keep.bin", "http://abs/a.bin"),
            "http://abs/a.bin"
        );
    }
}
