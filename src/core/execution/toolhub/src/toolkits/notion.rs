//! Notion toolkit (Notion REST API v1, `Notion-Version 2022-02-22` as used by
//! Activepieces `piece-notion`, MIT). Ports the `notion-*` action set (search,
//! read, create, update, archive, comment); the one-request adapter seam means
//! pagination-aware TS loops run as a single (server-side) request here.
use std::collections::BTreeMap;

use anyhow::{anyhow, bail, Result};
use serde_json::{json, Map, Value};

use super::common::*;
use super::{path_segment, ToolkitAdapter};
use crate::http::{HttpRequest, HttpResponse};
use crate::model::*;

pub const SLUG: &str = "notion";
const NOTION_VERSION: &str = "2022-02-22";
/// Notion caps one rich-text object at 2000 characters.
const RICH_TEXT_LIMIT: usize = 2000;
/// Block types whose body shape is `{ <type>: { rich_text: [...] } }`.
const TEXT_BLOCK_TYPES: [&str; 10] = [
    "paragraph",
    "heading_1",
    "heading_2",
    "heading_3",
    "bulleted_list_item",
    "numbered_list_item",
    "to_do",
    "quote",
    "callout",
    "code",
];

pub struct Notion {
    spec: ToolkitSpec,
    api_base: String,
}

impl Default for Notion {
    fn default() -> Self {
        Self::with_api_base("https://api.notion.com")
    }
}

impl Notion {
    pub fn with_api_base(api_base: &str) -> Self {
        let t = |slug, title: (&str, &str), desc: (&str, &str), props: Props<'_>, req: &[&str], m| {
            // Notion integrations carry their capability set on the token; the
            // OAuth app itself declares no scopes.
            tool(SLUG, slug, title, desc, props, req, &[], m)
        };
        let obj = |desc: &str| json!({"type": "object", "description": desc});
        let txt = |desc: &str| json!({"type": "string", "description": desc});
        let boolean = |desc: &str| json!({"type": "boolean", "description": desc});
        let spec = ToolkitSpec {
            slug: SLUG.into(),
            name: "Notion".into(),
            description: LocalizedText::new(
                "Notion-Seiten und -Datenbanken durchsuchen, lesen und bearbeiten.",
                "Search, read and edit Notion pages and databases.",
            ),
            auth_schemes: vec![
                AuthScheme::OAuth2(OAuth2Scheme {
                    authorize_url: "https://api.notion.com/v1/oauth/authorize".into(),
                    token_url: "https://api.notion.com/v1/oauth/token".into(),
                    default_scopes: vec![],
                    pkce: false,
                    scope_separator: " ".into(),
                    refreshable: false,
                    extra_authorize_params: vec![("owner".into(), "user".into())],
                }),
                // Internal integration secret (`ntn_…`), sent as a bearer token.
                AuthScheme::Bearer,
            ],
            tools: vec![
                t(
                    "NOTION_SEARCH",
                    ("Seiten und Datenbanken suchen", "Search pages and databases"),
                    (
                        "Durchsucht den Workspace nach Titel (Seiten und/oder Datenbanken, teilweise übereinstimmend, neueste Änderungen zuerst).",
                        "Searches the workspace by title for pages and/or databases (partial matching, newest edits first).",
                    ),
                    &[
                        ("query", txt("Titeltext (oder Teil davon) / title text (or part of it)")),
                        (
                            "filter_type",
                            json!({"type": "string", "description": "Nur Seiten, nur Datenbanken oder beide / pages, databases, or both", "enum": ["page", "database", "all"], "default": "all"}),
                        ),
                        ("limit", int("Maximale Treffer (1-100) / max results", 1, 100, 10)),
                        ("cursor", txt("Seitencursor aus einer vorherigen Antwort / page cursor from a previous response")),
                    ],
                    &["query"],
                    false,
                ),
                t(
                    "NOTION_LIST_DATABASES",
                    ("Datenbanken auflisten", "List databases"),
                    (
                        "Listet die mit der Integration geteilten Datenbanken, neueste Änderungen zuerst.",
                        "Lists the databases shared with your integration, newest edits first.",
                    ),
                    &[
                        ("limit", int("Maximale Anzahl (1-100) / how many", 1, 100, 100)),
                        ("cursor", txt("Seitencursor aus einer vorherigen Antwort / page cursor from a previous response")),
                    ],
                    &[],
                    false,
                ),
                t(
                    "NOTION_GET_PAGE",
                    ("Seite lesen", "Get page"),
                    (
                        "Liest Metadaten und Eigenschaften einer Seite (eine Datenbankzeile ist auch eine Seite).",
                        "Reads a page's metadata and property values (a database row is a page).",
                    ),
                    &[("page_id", txt("Seiten-ID / page id"))],
                    &["page_id"],
                    false,
                ),
                t(
                    "NOTION_GET_DATABASE",
                    ("Datenbankschema lesen", "Get database schema"),
                    (
                        "Liest das Schema einer Datenbank: Feldnamen, Typen und Optionen.",
                        "Reads a database's schema: property names, field types and options.",
                    ),
                    &[("database_id", txt("Datenbank-ID / database id"))],
                    &["database_id"],
                    false,
                ),
                t(
                    "NOTION_QUERY_DATABASE",
                    ("Datenbank abfragen", "Query database"),
                    (
                        "Fragt eine Datenbank mit Notions nativen Filter- und Sortier-JSON-Objekten ab (UND/ODER, Bereiche, enthält).",
                        "Queries a database using Notion's native filter and sort JSON (AND/OR, ranges, contains).",
                    ),
                    &[
                        ("database_id", txt("Datenbank-ID / database id")),
                        ("filter", obj("Optionales Notion-Filter-Objekt / optional Notion filter object")),
                        ("sorts", json!({"type": "array", "items": {"type": "object"}, "description": "Optionale Notion-Sortierobjekte / optional Notion sort objects"})),
                        ("page_size", int("Maximale Zeilen (1-100) / max rows", 1, 100, 100)),
                        ("start_cursor", txt("Paginierungscursor / pagination cursor")),
                    ],
                    &["database_id"],
                    false,
                ),
                t(
                    "NOTION_FIND_DATABASE_ITEM",
                    ("Datenbankeintrag finden", "Find database item"),
                    (
                        "Findet Zeilen einer Datenbank anhand eines Notion-Filter-Objekts; liefert den ersten Treffer und alle Treffer.",
                        "Finds rows in a database matching a Notion filter object; returns the first match and all matches.",
                    ),
                    &[
                        ("database_id", txt("Datenbank-ID / database id")),
                        ("filter", obj("Notion-Filter-Objekt, z.B. {\"property\":\"Email\",\"email\":{\"equals\":\"a@b.com\"}} / Notion filter object")),
                        ("page_size", int("Maximale Zeilen (1-100) / max rows", 1, 100, 100)),
                    ],
                    &["database_id", "filter"],
                    false,
                ),
                t(
                    "NOTION_GET_BLOCK_CHILDREN",
                    ("Seiteninhalt lesen", "Get page or block content"),
                    (
                        "Liest den Inhalt einer Seite oder eines Blocks als Block-JSON-Baum und als Markdown.",
                        "Reads a page's or block's content as a block JSON tree and as Markdown.",
                    ),
                    &[("block_id", txt("Seiten- oder Block-ID / page or block id"))],
                    &["block_id"],
                    false,
                ),
                t(
                    "NOTION_GET_PAGE_COMMENTS",
                    ("Seitenkommentare lesen", "Get page comments"),
                    (
                        "Listet alle Kommentare einer Seite, gruppiert in Diskussionsfäden.",
                        "Lists all comments on a page, grouped into discussion threads.",
                    ),
                    &[("page_id", txt("Seiten-ID / page id"))],
                    &["page_id"],
                    false,
                ),
                t(
                    "NOTION_LIST_USERS",
                    ("Benutzer auflisten", "List users"),
                    (
                        "Listet Personen und Bots im Workspace (braucht die Fähigkeit „Benutzerinformationen lesen“).",
                        "Lists the people and bots in the workspace (needs the \"Read user information\" capability).",
                    ),
                    &[
                        ("page_size", int("Benutzer pro Seite (1-100) / users per page", 1, 100, 100)),
                        ("start_cursor", txt("Paginierungscursor / pagination cursor")),
                    ],
                    &[],
                    false,
                ),
                t(
                    "NOTION_GET_USER",
                    ("Benutzer lesen", "Get user"),
                    (
                        "Liest die Details eines Workspace-Benutzers.",
                        "Reads a workspace user's details.",
                    ),
                    &[("user_id", txt("Benutzer-ID / user id"))],
                    &["user_id"],
                    false,
                ),
                t(
                    "NOTION_CREATE_PAGE",
                    ("Unterseite erstellen", "Create sub-page"),
                    (
                        "Erstellt eine neue Seite unter einer bestehenden Seite, mit Titel und optionalem Markdown-Inhalt.",
                        "Creates a new page under an existing page, with a title and optional markdown body.",
                    ),
                    &[
                        ("parent_page_id", txt("Elternseiten-ID / parent page id")),
                        ("title", txt("Titel der neuen Seite / title of the new page")),
                        ("content", txt("Markdown-Inhalt (wird in Notion-Blöcke umgewandelt) / markdown body (converted to Notion blocks)")),
                    ],
                    &["parent_page_id"],
                    true,
                ),
                t(
                    "NOTION_CREATE_DATABASE",
                    ("Datenbank erstellen", "Create database"),
                    (
                        "Erstellt eine Inline-Datenbank unter einer Seite, mit Titel und Spaltenschema (genau eine title-Spalte).",
                        "Creates an inline database under a page, with a title and a column schema (exactly one title column).",
                    ),
                    &[
                        ("parent_page_id", txt("Elternseiten-ID (muss eine Seite sein) / parent page id (must be a page)")),
                        ("title", txt("Titel der Datenbank / database title")),
                        ("properties", obj("Spaltenschema: Name → Typkonfiguration, z.B. {\"Name\":{\"title\":{}},\"Status\":{\"select\":{\"options\":[{\"name\":\"Open\"}]}}} / column schema: name → type config")),
                    ],
                    &["parent_page_id", "title", "properties"],
                    true,
                ),
                t(
                    "NOTION_CREATE_DATABASE_ITEM",
                    ("Datenbankeintrag erstellen", "Create database item"),
                    (
                        "Fügt einer Datenbank eine Zeile hinzu und setzt ihre Felder aus einem Notion-Properties-Objekt.",
                        "Adds a row to a database and sets its fields from a Notion properties object.",
                    ),
                    &[
                        ("database_id", txt("Datenbank-ID / database id")),
                        ("properties", obj("Notion-Properties-Objekt, z.B. {\"Name\":{\"title\":[{\"text\":{\"content\":\"My task\"}}]}} / Notion properties object")),
                        ("content", txt("Optionaler Textabsatz als Seitenkörper / optional plain-text paragraph as page body")),
                    ],
                    &["database_id", "properties"],
                    true,
                ),
                t(
                    "NOTION_APPEND_TO_PAGE",
                    ("Inhalt anhängen", "Append content to page"),
                    (
                        "Hängt neue Blöcke (aus Markdown geparst) an das Ende einer Seite oder eines Blocks an.",
                        "Appends new blocks (parsed from markdown) to the end of a page or block.",
                    ),
                    &[
                        ("block_id", txt("Seiten- oder Block-ID / page or block id")),
                        ("content", txt("Anzuhängender Markdown-Inhalt / markdown content to append")),
                    ],
                    &["block_id", "content"],
                    true,
                ),
                t(
                    "NOTION_UPDATE_BLOCK",
                    ("Blocktext ändern", "Update block text"),
                    (
                        "Ersetzt den Textinhalt eines einzelnen Blocks; der Blocktyp muss zum tatsächlichen Block passen.",
                        "Replaces the text content of a single block; the block type must match the actual block.",
                    ),
                    &[
                        ("block_id", txt("Block-ID / block id")),
                        ("content", txt("Neuer Text (automatisch bei 2000 Zeichen geteilt) / new text (split automatically at 2000 characters)")),
                        (
                            "block_type",
                            json!({"type": "string", "description": "Typ des Blocks / type of the block", "enum": TEXT_BLOCK_TYPES, "default": "paragraph"}),
                        ),
                        ("checked", boolean("Nur für to_do: abgehakt? / to_do only: checked?")),
                        ("language", txt("Nur für Code: Programmiersprache / code only: programming language")),
                    ],
                    &["block_id", "content"],
                    true,
                ),
                t(
                    "NOTION_UPDATE_DATABASE_ITEM",
                    ("Datenbankeintrag ändern", "Update database item"),
                    (
                        "Setzt Feldwerte einer bestehenden Datenbankzeile; nur enthaltene Felder werden geändert.",
                        "Sets field values on an existing database row; only included properties are changed.",
                    ),
                    &[
                        ("item_id", txt("Seiten-ID der Zeile / page id of the row")),
                        ("properties", obj("Zu setzende Notion-Properties, z.B. {\"Status\":{\"status\":{\"name\":\"Done\"}}} / Notion properties to set")),
                    ],
                    &["item_id", "properties"],
                    true,
                ),
                t(
                    "NOTION_UPDATE_DATABASE_SCHEMA",
                    ("Datenbankschema ändern", "Update database schema"),
                    (
                        "Ändert Titel und/oder Spalten einer Datenbank (Hinzufügen, Umbenennen, Ändern, Entfernen).",
                        "Updates a database's title and/or columns (add, rename, retype, remove).",
                    ),
                    &[
                        ("database_id", txt("Datenbank-ID / database id")),
                        ("title", txt("Optionaler neuer Titel / optional new title")),
                        ("properties", obj("Optionaler Schema-Patch: Name → Typkonfiguration, bei Umbenennung {\"name\":\"Neu\"}, zum Entfernen null / optional schema patch: name → type config, {\"name\":\"New\"} to rename, null to remove")),
                    ],
                    &["database_id"],
                    true,
                ),
                t(
                    "NOTION_MOVE_PAGE",
                    ("Seite verschieben", "Move page"),
                    (
                        "Verschiebt eine Seite unter eine andere Seite oder in eine Datenbank als Zeile (genau ein Ziel).",
                        "Moves a page under another page or into a database as a row (exactly one target).",
                    ),
                    &[
                        ("page_id", txt("Zu verschiebende Seite / page to move")),
                        ("new_parent_page_id", txt("Neue Elternseite / new parent page")),
                        ("new_parent_database_id", txt("Neue Eltern-Datenbank / new parent database")),
                    ],
                    &["page_id"],
                    true,
                ),
                t(
                    "NOTION_ARCHIVE_PAGE",
                    ("Seite archivieren/wiederherstellen", "Archive or restore page"),
                    (
                        "Archiviert (Papierkorb) oder stellt eine Seite oder Datenbankzeile über das archived-Flag.",
                        "Archives (trashes) or restores any page or database row via its archived flag.",
                    ),
                    &[
                        ("page_id", txt("Seiten-ID / page id")),
                        ("archived", boolean("true = archivieren, false = wiederherstellen / true = archive, false = restore")),
                    ],
                    &["page_id", "archived"],
                    true,
                ),
                t(
                    "NOTION_ARCHIVE_DATABASE_ITEM",
                    ("Datenbankeintrag archivieren", "Archive database item"),
                    (
                        "Archiviert (löscht weich) eine Datenbankzeile — bleibt wiederherstellbar.",
                        "Archives (soft-deletes) a database row — it stays recoverable.",
                    ),
                    &[("item_id", txt("Seiten-ID der Zeile / page id of the row"))],
                    &["item_id"],
                    true,
                ),
                t(
                    "NOTION_RESTORE_DATABASE_ITEM",
                    ("Datenbankeintrag wiederherstellen", "Restore database item"),
                    (
                        "Stellt eine archivierte Datenbankzeile wieder auf aktiv.",
                        "Restores an archived database row back to active.",
                    ),
                    &[("item_id", txt("Seiten-ID der Zeile / page id of the row"))],
                    &["item_id"],
                    true,
                ),
                t(
                    "NOTION_DELETE_BLOCK",
                    ("Block löschen", "Delete block"),
                    (
                        "Löscht (archiviert) einen einzelnen Block und entfernt ihn von seiner Seite.",
                        "Deletes (archives) a single block, removing it from its page.",
                    ),
                    &[("block_id", txt("Block-ID / block id"))],
                    &["block_id"],
                    true,
                ),
                t(
                    "NOTION_ADD_COMMENT",
                    ("Kommentar hinzufügen", "Add comment"),
                    (
                        "Postet einen neuen Kommentarfaden auf einer Seite (braucht „Kommentare einfügen“).",
                        "Posts a new comment thread on a page (needs \"Insert comments\").",
                    ),
                    &[
                        ("page_id", txt("Seiten-ID / page id")),
                        ("comment_text", txt("Kommentartext / comment text")),
                    ],
                    &["page_id", "comment_text"],
                    true,
                ),
            ],
        };
        Self {
            spec,
            api_base: api_base.trim_end_matches('/').to_string(),
        }
    }

    fn req(&self, method: &str, path: &str) -> HttpRequest {
        HttpRequest::new(method, format!("{}/v1{path}", self.api_base))
            .header("notion-version", NOTION_VERSION)
    }
}

/// JSON object argument (Property.Json in Activepieces).
fn req_object<'a>(args: &'a Value, key: &str) -> Result<&'a Map<String, Value>> {
    args.get(key)
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow!("argument `{key}` must be a JSON object"))
}

/// `/v1/search` body: workspace search, newest edits first (the TS loop fills
/// up to `limit`; here one page of at most `limit` results is requested).
fn search_body(query: Option<&str>, object_filter: Option<&str>, limit: u64, cursor: Option<&str>) -> Value {
    let mut b = json!({
        "sort": {"direction": "descending", "timestamp": "last_edited_time"},
        "page_size": limit,
    });
    if let Some(q) = query {
        b["query"] = json!(q);
    }
    if let Some(v) = object_filter {
        b["filter"] = json!({"property": "object", "value": v});
    }
    if let Some(c) = cursor {
        b["start_cursor"] = json!(c);
    }
    b
}

/// `/v1/databases/{id}/query` body with optional native filter/sorts.
fn query_body(args: &Value, require_filter: bool) -> Result<Value> {
    let mut b = json!({"page_size": opt_u64(args, "page_size", 100, 1, 100)});
    let filter = args.get("filter").filter(|v| !v.is_null());
    if require_filter || filter.is_some() {
        let f = filter
            .and_then(Value::as_object)
            .ok_or_else(|| anyhow!("argument `filter` must be a Notion filter JSON object"))?;
        b["filter"] = Value::Object(f.clone());
    }
    if let Some(s) = args.get("sorts").filter(|v| !v.is_null()) {
        let sorts = s
            .as_array()
            .ok_or_else(|| anyhow!("argument `sorts` must be a JSON array of Notion sort objects"))?;
        b["sorts"] = Value::Array(sorts.clone());
    }
    if let Some(c) = opt_str(args, "start_cursor") {
        b["start_cursor"] = json!(c);
    }
    Ok(b)
}

/// Plain text → rich_text objects, split at Notion's 2000-character cap
/// (an empty string still yields one empty rich-text object to clear a block).
fn rich_text(content: &str) -> Vec<Value> {
    let mut out = Vec::new();
    let mut chars = content.chars();
    loop {
        let chunk: String = chars.by_ref().take(RICH_TEXT_LIMIT).collect();
        if chunk.is_empty() {
            break;
        }
        out.push(json!({"type": "text", "text": {"content": chunk}}));
    }
    if out.is_empty() {
        out.push(json!({"type": "text", "text": {"content": ""}}));
    }
    out
}

fn push_text(out: &mut Vec<Value>, text: &mut String, annotations: Option<Value>) {
    for chunk in rich_text(text) {
        let mut seg = chunk;
        if let Some(a) = &annotations {
            seg["annotations"] = a.clone();
        }
        out.push(seg);
    }
    text.clear();
}

/// Match one inline token at the start of `rest`; flush the pending plain text
/// first, push the token's segment(s) and return the consumed byte length.
fn next_segment(rest: &str, plain: &mut String, out: &mut Vec<Value>) -> Option<usize> {
    if let Some(tail) = rest.strip_prefix("**") {
        if let Some(end) = tail.find("**") {
            push_text(out, plain, None);
            push_text(out, &mut tail[..end].to_string(), Some(json!({"bold": true})));
            return Some(end + 4);
        }
    }
    if let Some(tail) = rest.strip_prefix('*') {
        if let Some(end) = tail.find('*').filter(|e| *e > 0) {
            push_text(out, plain, None);
            push_text(out, &mut tail[..end].to_string(), Some(json!({"italic": true})));
            return Some(end + 2);
        }
    }
    if let Some(tail) = rest.strip_prefix('`') {
        if let Some(end) = tail.find('`').filter(|e| *e > 0) {
            push_text(out, plain, None);
            push_text(out, &mut tail[..end].to_string(), Some(json!({"code": true})));
            return Some(end + 2);
        }
    }
    if let Some(tail) = rest.strip_prefix('[') {
        // [text](url)
        if let Some(close) = tail.find(']') {
            if tail[close + 1..].starts_with('(') {
                if let Some(paren) = tail[close + 2..].find(')') {
                    let text = &tail[..close];
                    let url = &tail[close + 2..close + 2 + paren];
                    if !text.is_empty() && !url.is_empty() {
                        push_text(out, plain, None);
                        out.push(json!({"type": "text", "text": {"content": text, "link": {"url": url}}}));
                        return Some(close + paren + 4);
                    }
                }
            }
        }
    }
    None
}

/// Inline markdown → rich_text segments. Supported subset: `**bold**`,
/// `*italic*`, `` `code` ``, `[text](url)`; everything else stays plain text.
fn inline_rich_text(md: &str) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::new();
    let mut plain = String::new();
    let mut rest = md;
    while !rest.is_empty() {
        let Some(i) = rest.find(['*', '`', '[']) else {
            plain.push_str(rest);
            break;
        };
        plain.push_str(&rest[..i]);
        rest = &rest[i..];
        match next_segment(rest, &mut plain, &mut out) {
            Some(len) => rest = &rest[len..],
            None => {
                let c = rest.chars().next().unwrap_or('*');
                plain.push(c);
                rest = &rest[c.len_utf8()..];
            }
        }
    }
    push_text(&mut out, &mut plain, None);
    out
}

fn heading(t: &str) -> Option<(usize, &str)> {
    let level = t.bytes().take_while(|b| *b == b'#').count();
    if (1..=6).contains(&level) {
        // Notion only has heading_1..3; deeper levels render as heading_3.
        t[level..].strip_prefix(' ').map(|text| (level.min(3), text))
    } else {
        None
    }
}

fn todo_item(t: &str) -> Option<(bool, &str)> {
    if let Some(r) = t
        .strip_prefix("- [x] ")
        .or_else(|| t.strip_prefix("- [X] "))
    {
        Some((true, r))
    } else {
        t.strip_prefix("- [ ] ").map(|r| (false, r))
    }
}

fn ordered_item(t: &str) -> Option<&str> {
    let digits = t.bytes().take_while(|b| b.is_ascii_digit()).count();
    if digits == 0 {
        return None;
    }
    t[digits..].strip_prefix(". ")
}

/// Markdown → Notion block objects (subset of `@tryfabric/martian`): headings
/// 1-3, paragraphs, bulleted/numbered lists, to-dos, quotes, fenced code,
/// dividers, and the inline forms of [`inline_rich_text`].
fn markdown_to_blocks(md: &str) -> Vec<Value> {
    fn flush(blocks: &mut Vec<Value>, para: &mut Vec<String>) {
        if !para.is_empty() {
            let text = para.join("\n");
            para.clear();
            blocks.push(json!({
                "object": "block", "type": "paragraph",
                "paragraph": {"rich_text": inline_rich_text(&text)},
            }));
        }
    }

    let mut blocks = Vec::new();
    let mut para: Vec<String> = Vec::new();
    let mut lines = md.lines();
    while let Some(raw) = lines.next() {
        let t = raw.trim();
        if t.is_empty() {
            flush(&mut blocks, &mut para);
        } else if let Some(info) = t.strip_prefix("```") {
            flush(&mut blocks, &mut para);
            let mut code: Vec<&str> = Vec::new();
            for l in lines.by_ref() {
                if l.trim_start().starts_with("```") {
                    break;
                }
                code.push(l);
            }
            blocks.push(json!({
                "object": "block", "type": "code",
                "code": {
                    "rich_text": rich_text(&code.join("\n")),
                    "language": if info.trim().is_empty() { "plain text" } else { info.trim() },
                },
            }));
        } else if t == "---" || t == "***" {
            flush(&mut blocks, &mut para);
            blocks.push(json!({"object": "block", "type": "divider", "divider": {}}));
        } else if let Some((level, text)) = heading(t) {
            flush(&mut blocks, &mut para);
            let rich = inline_rich_text(text);
            blocks.push(match level {
                1 => json!({"object": "block", "type": "heading_1", "heading_1": {"rich_text": rich}}),
                2 => json!({"object": "block", "type": "heading_2", "heading_2": {"rich_text": rich}}),
                _ => json!({"object": "block", "type": "heading_3", "heading_3": {"rich_text": rich}}),
            });
        } else if let Some((checked, text)) = todo_item(t) {
            flush(&mut blocks, &mut para);
            blocks.push(json!({
                "object": "block", "type": "to_do",
                "to_do": {"rich_text": inline_rich_text(text), "checked": checked},
            }));
        } else if let Some(text) = t
            .strip_prefix("- ")
            .or_else(|| t.strip_prefix("* "))
            .or_else(|| t.strip_prefix("+ "))
        {
            flush(&mut blocks, &mut para);
            blocks.push(json!({
                "object": "block", "type": "bulleted_list_item",
                "bulleted_list_item": {"rich_text": inline_rich_text(text)},
            }));
        } else if let Some(text) = ordered_item(t) {
            flush(&mut blocks, &mut para);
            blocks.push(json!({
                "object": "block", "type": "numbered_list_item",
                "numbered_list_item": {"rich_text": inline_rich_text(text)},
            }));
        } else if let Some(text) = t.strip_prefix("> ") {
            flush(&mut blocks, &mut para);
            blocks.push(json!({
                "object": "block", "type": "quote",
                "quote": {"rich_text": inline_rich_text(text)},
            }));
        } else {
            para.push(t.to_string());
        }
    }
    flush(&mut blocks, &mut para);
    blocks
}

fn plain_segment(seg: &Value) -> &str {
    seg["plain_text"]
        .as_str()
        .or_else(|| seg["text"]["content"].as_str())
        .unwrap_or_default()
}

fn join_rich_text(v: &Value) -> String {
    v.as_array()
        .map(|a| a.iter().map(plain_segment).collect::<Vec<_>>().join(""))
        .unwrap_or_default()
}

/// Title of a search result: databases carry a top-level `title` array, pages
/// carry the title inside their first `title`-type property.
fn title_of(item: &Value) -> String {
    let top = join_rich_text(&item["title"]);
    if !top.is_empty() {
        return top;
    }
    if let Some(props) = item["properties"].as_object() {
        for p in props.values() {
            if p["type"].as_str() == Some("title") {
                let t = join_rich_text(&p["title"]);
                if !t.is_empty() {
                    return t;
                }
            }
        }
    }
    String::new()
}

fn search_item(item: &Value) -> Value {
    json!({
        "id": item["id"], "object": item["object"], "title": title_of(item),
        "url": item["url"], "created_time": item["created_time"],
        "last_edited_time": item["last_edited_time"], "archived": item["archived"],
        "parent": item["parent"],
    })
}

fn database_item(d: &Value) -> Value {
    json!({
        "id": d["id"], "title": title_of(d), "url": d["url"],
        "createdTime": d["created_time"], "lastEditedTime": d["last_edited_time"],
        "archived": d["archived"].as_bool().unwrap_or(false),
        "properties": d["properties"].as_object().map(|p| p.keys().collect::<Vec<_>>())
            .unwrap_or_default(),
    })
}

fn block_url(body: &Value) -> String {
    body["external"]["url"]
        .as_str()
        .or_else(|| body["file"]["url"].as_str())
        .unwrap_or_default()
        .to_string()
}

fn rich_text_to_markdown(rt: &[Value]) -> String {
    let mut out = String::new();
    for seg in rt {
        let text = plain_segment(seg);
        let ann = &seg["annotations"];
        let mut wrapped = text.to_string();
        if ann["code"].as_bool().unwrap_or(false) {
            wrapped = format!("`{wrapped}`");
        } else {
            if ann["bold"].as_bool().unwrap_or(false) {
                wrapped = format!("**{wrapped}**");
            }
            if ann["italic"].as_bool().unwrap_or(false) {
                wrapped = format!("*{wrapped}*");
            }
        }
        match seg["text"]["link"]["url"].as_str().or_else(|| seg["href"].as_str()) {
            Some(url) => out.push_str(&format!("[{wrapped}]({url})")),
            None => out.push_str(&wrapped),
        }
    }
    out
}

/// Notion blocks → markdown (subset of `notion-to-md`): text-bearing blocks,
/// to-dos, quotes, code fences, dividers, child pages, media and embeds.
fn blocks_to_markdown(blocks: &[Value]) -> String {
    let mut out = String::new();
    let mut numbered = 0usize;
    for b in blocks {
        let btype = b["type"].as_str().unwrap_or_default();
        let body = &b[btype];
        let rt = body["rich_text"].as_array().map(Vec::as_slice).unwrap_or(&[]);
        let text = rich_text_to_markdown(rt);
        if btype != "numbered_list_item" {
            numbered = 0;
        }
        let md = match btype {
            "paragraph" | "callout" | "toggle" => text,
            "heading_1" => format!("# {text}"),
            "heading_2" => format!("## {text}"),
            "heading_3" => format!("### {text}"),
            "bulleted_list_item" => format!("- {text}"),
            "numbered_list_item" => {
                numbered += 1;
                format!("{numbered}. {text}")
            }
            "to_do" => format!(
                "- [{}] {}",
                if body["checked"].as_bool().unwrap_or(false) { "x" } else { " " },
                text
            ),
            "quote" => format!("> {text}"),
            "code" => format!("```{}\n{text}\n```", body["language"].as_str().unwrap_or_default()),
            "divider" => "---".to_string(),
            "child_page" => format!("📄 {}", body["title"].as_str().unwrap_or_default()),
            "child_database" => body["title"].as_str().unwrap_or_default().to_string(),
            "image" | "file" => format!("![{btype}]({})", block_url(body)),
            "bookmark" | "embed" | "link_preview" => block_url(body),
            _ => continue,
        };
        if !out.is_empty() {
            out.push_str("\n\n");
        }
        out.push_str(&md);
    }
    out
}

fn options_value(opts: Option<&Vec<Value>>) -> Value {
    opts.map(|a| {
        a.iter()
            .map(|o| json!({"name": o["name"], "color": o["color"], "id": o["id"]}))
            .collect::<Vec<_>>()
    })
    .into()
}

/// `notion_get_database` result: raw database + flattened schema (the TS
/// `formStructure`) so callers learn property names, types and options.
fn database_overview(db: &Value) -> Value {
    let empty = Map::new();
    let mut properties = Map::new();
    let mut property_types = Map::new();
    let mut select_options = Map::new();
    let mut status_options = Map::new();
    let mut relation_config = Map::new();
    let mut number_config = Map::new();
    let mut type_counts: BTreeMap<String, u64> = BTreeMap::new();
    for (key, p) in db["properties"].as_object().unwrap_or(&empty) {
        let ptype = p["type"].as_str().unwrap_or_default();
        properties.insert(
            key.clone(),
            json!({
                "name": p["name"].as_str().unwrap_or(key),
                "type": ptype,
                "id": p["id"],
                "description": p["description"].as_str().unwrap_or_default(),
            }),
        );
        property_types.insert(key.clone(), json!(ptype));
        *type_counts.entry(ptype.to_string()).or_default() += 1;
        match ptype {
            "select" => {
                select_options.insert(key.clone(), options_value(p["select"]["options"].as_array()));
            }
            "multi_select" => {
                select_options
                    .insert(key.clone(), options_value(p["multi_select"]["options"].as_array()));
            }
            "status" => {
                status_options.insert(
                    key.clone(),
                    json!({
                        "options": options_value(p["status"]["options"].as_array()),
                        "groups": p["status"]["groups"].as_array().map(|g| g.iter()
                            .map(|gr| json!({
                                "name": gr["name"], "color": gr["color"], "id": gr["id"],
                                "option_ids": gr["option_ids"].as_array().cloned().unwrap_or_default(),
                            }))
                            .collect::<Vec<_>>()),
                    }),
                );
            }
            "relation" => {
                relation_config.insert(
                    key.clone(),
                    json!({
                        "database_id": p["relation"]["database_id"],
                        "synced_property_id": p["relation"]["synced_property_id"],
                        "synced_property_name": p["relation"]["synced_property_name"],
                    }),
                );
            }
            "number" => {
                number_config.insert(key.clone(), json!({"format": p["number"]["format"]}));
            }
            _ => {}
        }
    }
    let title = join_rich_text(&db["title"]);
    json!({
        "success": true,
        "database": db,
        "formStructure": {
            "id": db["id"],
            "title": title,
            "description": join_rich_text(&db["description"]),
            "properties": properties,
            "propertyTypes": property_types,
            "selectOptions": select_options,
            "statusOptions": status_options,
            "relationConfig": relation_config,
            "numberConfig": number_config,
        },
        "summary": {
            "title": if title.is_empty() { "Untitled Database".to_string() } else { title.clone() },
            "totalProperties": properties.len(),
            "propertyTypes": type_counts,
        },
    })
}

/// Archive/restore database item result (the TS adds a client-timestamped
/// `archived_at`/`restored_at`; the hub clock is not part of this seam).
fn item_result(v: &Value, message: &str, item_key: &str) -> Value {
    let title = {
        let t = title_of(v);
        if t.is_empty() {
            "Untitled item".to_string()
        } else {
            t
        }
    };
    let mut out = json!({
        "success": true,
        "message": format!("\"{title}\" {message}"),
        "fullResponse": v,
    });
    out[item_key] = json!({"id": v["id"], "title": title, "url": v["url"]});
    out
}

fn created_time(c: &Value) -> String {
    c["created_time"].as_str().unwrap_or_default().to_string()
}

impl ToolkitAdapter for Notion {
    fn spec(&self) -> &ToolkitSpec {
        &self.spec
    }

    fn build_request(&self, tool: &str, args: &Value) -> Result<HttpRequest> {
        Ok(match tool {
            "NOTION_SEARCH" => {
                let filter_type = opt_str(args, "filter_type").unwrap_or("all");
                if !matches!(filter_type, "page" | "database" | "all") {
                    bail!("invalid argument `filter_type`");
                }
                self.req("POST", "/search")
                    .json(&search_body(Some(req_str(args, "query")?), (filter_type != "all").then_some(filter_type), opt_u64(args, "limit", 10, 1, 100), opt_str(args, "cursor")))
            }
            "NOTION_LIST_DATABASES" => self.req("POST", "/search").json(&search_body(
                None,
                Some("database"),
                opt_u64(args, "limit", 100, 1, 100),
                opt_str(args, "cursor"),
            )),
            "NOTION_GET_PAGE" => self.req("GET", &format!("/pages/{}", path_segment(args, "page_id")?)),
            "NOTION_GET_DATABASE" => {
                self.req("GET", &format!("/databases/{}", path_segment(args, "database_id")?))
            }
            "NOTION_QUERY_DATABASE" => {
                let id = path_segment(args, "database_id")?;
                self.req("POST", &format!("/databases/{id}/query")).json(&query_body(args, false)?)
            }
            "NOTION_FIND_DATABASE_ITEM" => {
                let id = path_segment(args, "database_id")?;
                self.req("POST", &format!("/databases/{id}/query")).json(&query_body(args, true)?)
            }
            "NOTION_GET_BLOCK_CHILDREN" => {
                let id = path_segment(args, "block_id")?;
                self.req("GET", &format!("/blocks/{id}/children?page_size=100"))
            }
            "NOTION_GET_PAGE_COMMENTS" => {
                let id = path_segment(args, "page_id")?;
                self.req("GET", &format!(
                    "/comments{}",
                    query(&[("block_id", Some(id.to_string())), ("page_size", Some("100".into()))])
                ))
            }
            "NOTION_LIST_USERS" => self.req("GET", &format!(
                "/users{}",
                query(&[
                    ("page_size", Some(opt_u64(args, "page_size", 100, 1, 100).to_string())),
                    ("start_cursor", opt_str(args, "start_cursor").map(String::from)),
                ])
            )),
            "NOTION_GET_USER" => self.req("GET", &format!("/users/{}", path_segment(args, "user_id")?)),
            "NOTION_CREATE_PAGE" => {
                let mut body = json!({
                    "parent": {"page_id": path_segment(args, "parent_page_id")?},
                    "properties": {"title": {"title": [{"text": {"content": opt_str(args, "title").unwrap_or("")}}]}},
                });
                if let Some(content) = opt_str(args, "content") {
                    body["children"] = json!(markdown_to_blocks(content));
                }
                self.req("POST", "/pages").json(&body)
            }
            "NOTION_CREATE_DATABASE" => {
                let body = json!({
                    "parent": {"type": "page_id", "page_id": path_segment(args, "parent_page_id")?},
                    "title": [{"type": "text", "text": {"content": req_str(args, "title")?}}],
                    "properties": Value::Object(req_object(args, "properties")?.clone()),
                });
                self.req("POST", "/databases").json(&body)
            }
            "NOTION_CREATE_DATABASE_ITEM" => {
                let mut body = json!({
                    "parent": {"type": "database_id", "database_id": path_segment(args, "database_id")?},
                    "properties": Value::Object(req_object(args, "properties")?.clone()),
                });
                if let Some(content) = opt_str(args, "content") {
                    body["children"] = json!([{
                        "object": "block", "type": "paragraph",
                        "paragraph": {"rich_text": [{"type": "text", "text": {"content": content}}]},
                    }]);
                }
                self.req("POST", "/pages").json(&body)
            }
            "NOTION_APPEND_TO_PAGE" => {
                let id = path_segment(args, "block_id")?;
                let body = json!({"children": markdown_to_blocks(req_str(args, "content")?)});
                self.req("PATCH", &format!("/blocks/{id}/children")).json(&body)
            }
            "NOTION_UPDATE_BLOCK" => {
                let id = path_segment(args, "block_id")?;
                let btype = opt_str(args, "block_type").unwrap_or("paragraph");
                if !TEXT_BLOCK_TYPES.contains(&btype) {
                    bail!("invalid argument `block_type`");
                }
                let mut inner = json!({"rich_text": rich_text(req_str(args, "content")?)});
                if btype == "to_do" {
                    inner["checked"] = json!(args.get("checked").and_then(Value::as_bool).unwrap_or(false));
                }
                if btype == "code" {
                    inner["language"] = json!(opt_str(args, "language").unwrap_or("plain text"));
                }
                let mut body = Map::new();
                body.insert(btype.to_string(), inner);
                self.req("PATCH", &format!("/blocks/{id}")).json(&Value::Object(body))
            }
            "NOTION_UPDATE_DATABASE_ITEM" => {
                let id = path_segment(args, "item_id")?;
                let body = json!({"properties": Value::Object(req_object(args, "properties")?.clone())});
                self.req("PATCH", &format!("/pages/{id}")).json(&body)
            }
            "NOTION_UPDATE_DATABASE_SCHEMA" => {
                let id = path_segment(args, "database_id")?;
                let title = opt_str(args, "title");
                let properties = args.get("properties").filter(|v| !v.is_null());
                if title.is_none() && properties.is_none() {
                    bail!("provide a new `title` and/or a `properties` schema patch");
                }
                let mut body = Map::new();
                if let Some(t) = title {
                    body.insert("title".into(), json!([{"type": "text", "text": {"content": t}}]));
                }
                if let Some(p) = properties {
                    // Property values may legitimately be null (= remove column).
                    let patch = p
                        .as_object()
                        .ok_or_else(|| anyhow!("argument `properties` must be a JSON object"))?;
                    body.insert("properties".into(), Value::Object(patch.clone()));
                }
                self.req("PATCH", &format!("/databases/{id}")).json(&Value::Object(body))
            }
            "NOTION_MOVE_PAGE" => {
                let id = path_segment(args, "page_id")?;
                let page = opt_str(args, "new_parent_page_id");
                let database = opt_str(args, "new_parent_database_id");
                if page.is_some() == database.is_some() {
                    bail!("supply exactly one of `new_parent_page_id` or `new_parent_database_id`");
                }
                let parent = match (page, database) {
                    (Some(p), _) => json!({"page_id": p}),
                    (_, Some(d)) => json!({"database_id": d}),
                    _ => unreachable!("checked above"),
                };
                self.req("PATCH", &format!("/pages/{id}")).json(&json!({"parent": parent}))
            }
            "NOTION_ARCHIVE_PAGE" => {
                let id = path_segment(args, "page_id")?;
                let archived = args
                    .get("archived")
                    .and_then(Value::as_bool)
                    .ok_or_else(|| anyhow!("missing argument `archived`"))?;
                self.req("PATCH", &format!("/pages/{id}")).json(&json!({"archived": archived}))
            }
            "NOTION_ARCHIVE_DATABASE_ITEM" => {
                let id = path_segment(args, "item_id")?;
                self.req("PATCH", &format!("/pages/{id}")).json(&json!({"archived": true}))
            }
            "NOTION_RESTORE_DATABASE_ITEM" => {
                let id = path_segment(args, "item_id")?;
                self.req("PATCH", &format!("/pages/{id}")).json(&json!({"archived": false}))
            }
            "NOTION_DELETE_BLOCK" => self.req("DELETE", &format!("/blocks/{}", path_segment(args, "block_id")?)),
            "NOTION_ADD_COMMENT" => {
                let body = json!({
                    "parent": {"page_id": path_segment(args, "page_id")?},
                    "rich_text": [{"type": "text", "text": {"content": req_str(args, "comment_text")?}}],
                });
                self.req("POST", "/comments").json(&body)
            }
            other => bail!("unknown Notion tool `{other}`"),
        })
    }

    fn parse_response(&self, tool: &str, response: &HttpResponse) -> Result<Value> {
        let v = response.json()?;
        Ok(match tool {
            "NOTION_SEARCH" => {
                let results: Vec<Value> = v["results"]
                    .as_array()
                    .map(|a| a.iter().map(search_item).collect())
                    .unwrap_or_default();
                json!({
                    "results": results,
                    "count": results.len(),
                    "has_more": v["has_more"],
                    "next_cursor": v["next_cursor"],
                })
            }
            "NOTION_LIST_DATABASES" => {
                let data: Vec<Value> = v["results"]
                    .as_array()
                    .map(|a| a.iter().map(database_item).collect())
                    .unwrap_or_default();
                json!({
                    "success": true,
                    "data": data,
                    "pagination": {
                        "hasMore": v["has_more"],
                        "nextCursor": v["next_cursor"],
                        "count": data.len(),
                    },
                })
            }
            "NOTION_GET_DATABASE" => database_overview(&v),
            "NOTION_QUERY_DATABASE" => {
                let results = v["results"].as_array().cloned().unwrap_or_default();
                json!({
                    "success": true,
                    "results": results,
                    "count": results.len(),
                    "has_more": v["has_more"],
                    "next_cursor": v["next_cursor"],
                })
            }
            "NOTION_FIND_DATABASE_ITEM" => {
                let results = v["results"].as_array().cloned().unwrap_or_default();
                json!({
                    "success": !results.is_empty(),
                    "results": results,
                    "count": results.len(),
                    "firstMatch": results.first().cloned().unwrap_or(Value::Null),
                    "has_more": v["has_more"],
                    "next_cursor": v["next_cursor"],
                })
            }
            "NOTION_GET_BLOCK_CHILDREN" => {
                let blocks = v["results"].as_array().cloned().unwrap_or_default();
                json!({
                    "blocks": blocks,
                    "markdown": blocks_to_markdown(&blocks),
                    "count": blocks.len(),
                })
            }
            "NOTION_GET_PAGE_COMMENTS" => {
                let all = v["results"].as_array().cloned().unwrap_or_default();
                let mut threads: BTreeMap<String, Vec<Value>> = BTreeMap::new();
                let mut page_comments = Vec::new();
                for c in &all {
                    match c["discussion_id"].as_str() {
                        Some(d) => threads.entry(d.to_string()).or_default().push(c.clone()),
                        None => page_comments.push(c.clone()),
                    }
                }
                for list in threads.values_mut() {
                    list.sort_by_key(created_time);
                }
                page_comments.sort_by_key(created_time);
                json!({
                    "totalComments": all.len(),
                    "pageComments": page_comments,
                    "discussionThreads": threads,
                    "discussionCount": threads.len(),
                    "allComments": all,
                })
            }
            "NOTION_LIST_USERS" => {
                let users: Vec<Value> = v["results"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .map(|u| {
                                json!({
                                    "id": u["id"], "name": u["name"], "type": u["type"],
                                    "avatar_url": u["avatar_url"], "email": u["person"]["email"],
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                json!({
                    "users": users,
                    "count": users.len(),
                    "has_more": v["has_more"],
                    "next_cursor": v["next_cursor"],
                })
            }
            "NOTION_GET_USER" => json!({
                "id": v["id"], "name": v["name"], "type": v["type"],
                "avatar_url": v["avatar_url"], "email": v["person"]["email"], "bot": v["bot"],
            }),
            "NOTION_CREATE_DATABASE" => json!({"success": true, "database": v}),
            "NOTION_CREATE_DATABASE_ITEM" => json!({"success": true, "item": v}),
            "NOTION_UPDATE_DATABASE_ITEM" => json!({"success": true, "item": v}),
            "NOTION_UPDATE_DATABASE_SCHEMA" => json!({"success": true, "database": v}),
            "NOTION_ADD_COMMENT" => json!({"success": true, "comment": v}),
            "NOTION_ARCHIVE_DATABASE_ITEM" => {
                item_result(&v, "has been archived successfully", "archivedItem")
            }
            "NOTION_RESTORE_DATABASE_ITEM" => item_result(
                &v,
                "has been successfully restored and is now active",
                "restoredItem",
            ),
            // GET_PAGE / CREATE_PAGE / APPEND_TO_PAGE / UPDATE_BLOCK /
            // MOVE_PAGE / ARCHIVE_PAGE / DELETE_BLOCK / ADD_COMMENT:
            // the TS actions return the raw API object.
            _ => v,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_to_blocks_subset() {
        let blocks = markdown_to_blocks(
            "# Title\n\nPara **bold** *em* `code` [l](https://x.y)\n\n- a\n1. one\n> quoted\n---\n\n- [ ] buy milk\n- [x] done\n\n```rust\nlet x = 1;\n```",
        );
        let types: Vec<&str> = blocks.iter().map(|b| b["type"].as_str().unwrap()).collect();
        assert_eq!(
            types,
            vec![
                "heading_1",
                "paragraph",
                "bulleted_list_item",
                "numbered_list_item",
                "quote",
                "divider",
                "to_do",
                "to_do",
                "code"
            ]
        );
        let para = &blocks[1]["paragraph"]["rich_text"];
        assert_eq!(para[0]["text"]["content"], "Para ");
        assert_eq!(para[1]["annotations"]["bold"], true);
        assert_eq!(para[1]["text"]["content"], "bold");
        assert_eq!(para[3]["annotations"]["italic"], true);
        assert_eq!(para[5]["text"]["content"], "code");
        assert_eq!(para[5]["annotations"]["code"], true);
        assert_eq!(para[7]["text"]["link"]["url"], "https://x.y");
        assert_eq!(blocks[6]["to_do"]["checked"], false);
        assert_eq!(blocks[7]["to_do"]["checked"], true);
        assert_eq!(blocks[8]["code"]["language"], "rust");
        assert_eq!(blocks_to_markdown(&blocks), "# Title\n\nPara **bold** *em* `code` [l](https://x.y)\n\n- a\n\n1. one\n\n> quoted\n\n---\n\n- [ ] buy milk\n\n- [x] done\n\n```rust\nlet x = 1;\n```");
    }

    #[test]
    fn rich_text_cap_and_request_validation() {
        let long = "x".repeat(RICH_TEXT_LIMIT * 2 + 1);
        let rt = rich_text(&long);
        assert_eq!(rt.len(), 3);
        assert_eq!(rt[2]["text"]["content"], "x");
        // Empty content still yields one (empty) rich-text object.
        assert_eq!(rich_text("")[0]["text"]["content"], "");

        let n = Notion::with_api_base("https://n.test");
        assert!(n
            .build_request(
                "NOTION_MOVE_PAGE",
                &json!({"page_id": "p1", "new_parent_page_id": "p2", "new_parent_database_id": "d1"})
            )
            .is_err());
        assert!(n
            .build_request(
                "NOTION_UPDATE_BLOCK",
                &json!({"block_id": "b1", "content": "x", "block_type": "bogus"})
            )
            .is_err());
        assert!(n.build_request("NOTION_GET_PAGE", &json!({"page_id": "../etc"})).is_err());
        assert!(n
            .build_request(
                "NOTION_FIND_DATABASE_ITEM",
                &json!({"database_id": "d1", "filter": "not-an-object"})
            )
            .is_err());

        let users = n
            .build_request("NOTION_LIST_USERS", &json!({"start_cursor": "a b", "page_size": 500}))
            .unwrap();
        assert_eq!(users.url, "https://n.test/v1/users?page_size=100&start_cursor=a+b");
        assert_eq!(
            users.headers.get("notion-version").map(String::as_str),
            Some(NOTION_VERSION)
        );
        let comments = n
            .build_request("NOTION_GET_PAGE_COMMENTS", &json!({"page_id": "p?q"}))
            .is_err();
        assert!(comments, "path_segment rejects query characters");
    }
}
