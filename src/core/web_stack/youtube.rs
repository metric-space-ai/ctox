//! `ctox web youtube-transcript --url <video> [--lang de]`
//!
//! Prints the caption transcript of one YouTube video as JSON. The watch page
//! only yields the public Innertube API key; the caption track comes from the
//! Innertube `player` endpoint with the Android client, whose timedtext URLs
//! work without a proof-of-origin token (the web client's URLs return empty
//! bodies since 2025). Videos without any caption track fail with
//! `no_captions` — there is no audio download/STT fallback here.

use anyhow::{bail, ensure, Context, Result};
use serde_json::{json, Value};
use std::time::Duration;

const WEB_USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0 Safari/537.36";
const ANDROID_CLIENT_VERSION: &str = "20.10.38";
const ANDROID_USER_AGENT: &str = "com.google.android.youtube/20.10.38 (Linux; U; Android 14) gzip";

pub(super) fn handle(args: &[String]) -> Result<()> {
    let url = flag(args, "--url")
        .or_else(|| {
            args.get(1)
                .filter(|value| !value.starts_with("--"))
                .map(String::as_str)
        })
        .context("usage: ctox web youtube-transcript --url <video-url-or-id> [--lang <code>]")?;
    let lang = flag(args, "--lang");
    let result = match fetch_transcript(url, lang) {
        Ok(value) => value,
        Err(error) => json!({ "ok": false, "url": url, "error": error.to_string() }),
    };
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

fn flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.windows(2)
        .find(|window| window[0] == name)
        .map(|window| window[1].as_str())
}

fn fetch_transcript(input: &str, lang: Option<&str>) -> Result<Value> {
    let video_id = video_id(input).context("not a YouTube video URL or id")?;
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(25))
        .build();
    let watch = agent
        .get(&format!("https://www.youtube.com/watch?v={video_id}&hl=en"))
        .set("User-Agent", WEB_USER_AGENT)
        .set("Accept-Language", "en-US,en;q=0.9")
        .set("Cookie", "SOCS=CAI; CONSENT=YES+1")
        .call()
        .context("watch page request failed")?
        .into_string()?;
    let api_key = innertube_api_key(&watch).context("watch page carries no Innertube API key")?;
    let body = json!({
        "context": { "client": {
            "clientName": "ANDROID",
            "clientVersion": ANDROID_CLIENT_VERSION,
            "androidSdkVersion": 34,
            "hl": "en",
        }},
        "videoId": video_id,
    });
    let player: Value = agent
        .post(&format!(
            "https://www.youtube.com/youtubei/v1/player?key={api_key}"
        ))
        .set("Content-Type", "application/json")
        .set("User-Agent", ANDROID_USER_AGENT)
        .send_string(&body.to_string())
        .context("player request failed")?
        .into_string()
        .map_err(anyhow::Error::from)
        .and_then(|text| serde_json::from_str(&text).context("player response is not JSON"))?;
    let status = player
        .pointer("/playabilityStatus/status")
        .and_then(Value::as_str)
        .unwrap_or("UNKNOWN");
    ensure!(status == "OK", "video not playable ({status})");
    let tracks = player
        .pointer("/captions/playerCaptionsTracklistRenderer/captionTracks")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let Some(track) = choose_track(&tracks, lang) else {
        bail!("no_captions");
    };
    let base_url = track
        .get("baseUrl")
        .and_then(Value::as_str)
        .context("caption track has no baseUrl")?;
    let xml = agent
        .get(&strip_format(base_url))
        .set("User-Agent", ANDROID_USER_AGENT)
        .call()
        .context("caption request failed")?
        .into_string()?;
    let segments = parse_transcript_xml(&xml);
    ensure!(!segments.is_empty(), "caption track was empty");
    let text = segments
        .iter()
        .map(|segment| segment["text"].as_str().unwrap_or_default())
        .collect::<Vec<_>>()
        .join(" ");
    let details = player.get("videoDetails").cloned().unwrap_or(Value::Null);
    Ok(json!({
        "ok": true,
        "video_id": video_id,
        "url": format!("https://www.youtube.com/watch?v={video_id}"),
        "title": details.get("title"),
        "channel": details.get("author"),
        "channel_id": details.get("channelId"),
        "length_seconds": details.get("lengthSeconds"),
        "published_at": player.pointer("/microformat/playerMicroformatRenderer/publishDate"),
        "language": track.get("languageCode"),
        "caption_kind": if track.get("kind").and_then(Value::as_str) == Some("asr") { "auto" } else { "manual" },
        "segments": segments,
        "text": text,
    }))
}

fn video_id(input: &str) -> Option<String> {
    let input = input.trim();
    let valid = |id: &str| {
        id.len() == 11
            && id
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
    };
    if valid(input) {
        return Some(input.to_string());
    }
    let rest = input
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_start_matches("www.")
        .trim_start_matches("m.");
    let candidate = if let Some(path) = rest.strip_prefix("youtu.be/") {
        path.split(['?', '&', '/', '#']).next()?
    } else if let Some(path) = rest.strip_prefix("youtube.com/") {
        if let Some(query) = path
            .strip_prefix("watch?")
            .or_else(|| path.split_once("watch?").map(|(_, q)| q))
        {
            query
                .split('&')
                .find_map(|pair| pair.strip_prefix("v="))?
                .split(['&', '#'])
                .next()?
        } else {
            let mut parts = path.split('/');
            match parts.next()? {
                "shorts" | "live" | "embed" | "v" => parts.next()?.split(['?', '&', '#']).next()?,
                _ => return None,
            }
        }
    } else {
        return None;
    };
    valid(candidate).then(|| candidate.to_string())
}

fn innertube_api_key(html: &str) -> Option<String> {
    let marker = "\"INNERTUBE_API_KEY\":\"";
    let start = html.find(marker)? + marker.len();
    let end = html[start..].find('"')? + start;
    let key = &html[start..end];
    (!key.is_empty()
        && key
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_'))
    .then(|| key.to_string())
}

/// Requested language first (manual before auto-generated), then any manual
/// track, then whatever exists.
fn choose_track<'a>(tracks: &'a [Value], lang: Option<&str>) -> Option<&'a Value> {
    let is_asr = |track: &Value| track.get("kind").and_then(Value::as_str) == Some("asr");
    let matches_lang = |track: &Value, lang: &str| {
        track
            .get("languageCode")
            .and_then(Value::as_str)
            .is_some_and(|code| code == lang || code.starts_with(&format!("{lang}-")))
    };
    if let Some(lang) = lang {
        if let Some(track) = tracks.iter().find(|t| matches_lang(t, lang) && !is_asr(t)) {
            return Some(track);
        }
        if let Some(track) = tracks.iter().find(|t| matches_lang(t, lang)) {
            return Some(track);
        }
    }
    tracks
        .iter()
        .find(|t| !is_asr(t))
        .or_else(|| tracks.first())
}

fn strip_format(url: &str) -> String {
    let (base, query) = url.split_once('?').unwrap_or((url, ""));
    let kept: Vec<&str> = query
        .split('&')
        .filter(|pair| !pair.is_empty() && !pair.starts_with("fmt="))
        .collect();
    if kept.is_empty() {
        base.to_string()
    } else {
        format!("{base}?{}", kept.join("&"))
    }
}

fn attribute(tag: &str, name: &str) -> Option<f64> {
    let marker = format!("{name}=\"");
    let start = tag.find(&marker)? + marker.len();
    let end = tag[start..].find('"')? + start;
    tag[start..end].parse().ok()
}

/// `<text start="1.2" dur="2.16">…</text>` → `{start_ms, duration_ms, text}`.
fn parse_transcript_xml(xml: &str) -> Vec<Value> {
    let mut segments = Vec::new();
    let mut rest = xml;
    while let Some(open) = rest.find("<text") {
        let after_open = &rest[open..];
        let Some(tag_end) = after_open.find('>') else {
            break;
        };
        let tag = &after_open[..tag_end];
        let body_start = &after_open[tag_end + 1..];
        let Some(close) = body_start.find("</text>") else {
            break;
        };
        let raw = &body_start[..close];
        // Captions are entity-escaped twice (`&amp;#39;`).
        let text = decode_entities(&decode_entities(raw))
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if !text.is_empty() {
            segments.push(json!({
                "start_ms": (attribute(tag, "start").unwrap_or(0.0) * 1000.0).round() as i64,
                "duration_ms": (attribute(tag, "dur").unwrap_or(0.0) * 1000.0).round() as i64,
                "text": text,
            }));
        }
        rest = &body_start[close + "</text>".len()..];
    }
    segments
}

fn decode_entities(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let candidate = &rest[amp..];
        let Some(semi) = candidate.find(';').filter(|semi| *semi <= 10) else {
            out.push('&');
            rest = &candidate[1..];
            continue;
        };
        let entity = &candidate[1..semi];
        let decoded = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ => entity
                .strip_prefix("#x")
                .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                .or_else(|| entity.strip_prefix('#').and_then(|dec| dec.parse().ok()))
                .and_then(char::from_u32),
        };
        match decoded {
            Some(ch) => {
                out.push(ch);
                rest = &candidate[semi + 1..];
            }
            None => {
                out.push('&');
                rest = &candidate[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn video_ids_from_common_url_shapes() {
        for input in [
            "jNQXAC9IVRw",
            "https://www.youtube.com/watch?v=jNQXAC9IVRw",
            "https://youtube.com/watch?feature=share&v=jNQXAC9IVRw&t=4",
            "https://youtu.be/jNQXAC9IVRw?si=abc",
            "https://www.youtube.com/shorts/jNQXAC9IVRw",
            "https://m.youtube.com/live/jNQXAC9IVRw?feature=x",
        ] {
            assert_eq!(video_id(input).as_deref(), Some("jNQXAC9IVRw"), "{input}");
        }
        assert_eq!(video_id("https://www.youtube.com/@channel"), None);
        assert_eq!(video_id("https://example.com/watch?v=jNQXAC9IVRw"), None);
    }

    #[test]
    fn transcript_xml_is_decoded_twice_and_timed() {
        let xml = r#"<?xml version="1.0"?><transcript><text start="1.2" dur="2.16">All right, so
here we are</text><text start="12.616" dur="1.751">and that&amp;#39;s cool &amp;amp; fun</text><text start="13" dur="1"> </text></transcript>"#;
        let segments = parse_transcript_xml(xml);
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0]["start_ms"], json!(1200));
        assert_eq!(segments[0]["text"], json!("All right, so here we are"));
        assert_eq!(segments[1]["text"], json!("and that's cool & fun"));
    }

    #[test]
    fn track_choice_prefers_requested_manual_captions() {
        let tracks = vec![
            json!({"languageCode": "en", "kind": "asr", "baseUrl": "a"}),
            json!({"languageCode": "de", "baseUrl": "b"}),
            json!({"languageCode": "en", "baseUrl": "c"}),
        ];
        assert_eq!(choose_track(&tracks, Some("en")).unwrap()["baseUrl"], "c");
        assert_eq!(choose_track(&tracks, Some("fr")).unwrap()["baseUrl"], "b");
        assert_eq!(choose_track(&tracks, None).unwrap()["baseUrl"], "b");
        assert!(choose_track(&[], None).is_none());
    }

    #[test]
    fn format_parameter_is_removed_and_key_is_extracted() {
        assert_eq!(
            strip_format("https://www.youtube.com/api/timedtext?v=x&fmt=srv3&lang=en"),
            "https://www.youtube.com/api/timedtext?v=x&lang=en"
        );
        assert_eq!(
            innertube_api_key(r#"..."INNERTUBE_API_KEY":"AIzaSyA-_1","x":1"#).as_deref(),
            Some("AIzaSyA-_1")
        );
        assert_eq!(innertube_api_key("nothing"), None);
    }
}
