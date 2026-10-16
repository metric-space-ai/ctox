//! Trello toolkit (REST v1, `https://api.trello.com/1`).
//! Action set modelled on Activepieces `piece-trello` (MIT).
//!
//! Auth: the piece declares `PieceAuth.BasicAuth` (username = API key,
//! password = token) and the Trello API accepts that pair as `key`/`token`
//! query parameters or as an HTTP Basic credential. This adapter builds
//! credential-free requests; the hub injects the stored credential
//! (`api_key:token`, i.e. what the piece holds as username:password) through
//! its Basic scheme, so the recorded request URLs carry no auth material.
//!
//! Every tool's request arm cites the Activepieces source file it mirrors.
//! Requests follow the piece exactly: ids travel in the path, action
//! parameters as query fields (`text`, `value`, `name`, …) and — for create /
//! update card — as a JSON body.
use anyhow::{bail, Result};
use serde_json::{json, Map, Value};

use super::common::*;
use super::{path_segment, ToolkitAdapter};
use crate::http::{HttpRequest, HttpResponse};
use crate::model::*;

pub const SLUG: &str = "trello";

/// Trello tokens are granted per Power-Up without granular scopes; the scope
/// of every tool is the token capability the call needs (`read`/`write`).
const READ: &str = "read";
const WRITE: &str = "write";

// ---------------------------------------------------------------- schema fragments

fn choice(desc: &str, options: &[&str]) -> Value {
    json!({"type": "string", "description": desc, "enum": options})
}

/// Checkbox with a default (Activepieces `Property.Checkbox` + `defaultValue`).
fn flag(desc: &str, default: bool) -> Value {
    json!({"type": "boolean", "description": desc, "default": default})
}

/// Checkbox without a default: unset stays unset.
fn bool_prop(desc: &str) -> Value {
    json!({"type": "boolean", "description": desc})
}

fn limit_prop(default: u64, max: u64) -> Value {
    json!({"type": "integer",
        "description": format!("Maximale Anzahl Treffer (1-{max}), Standard {default}. / Max results (1-{max}), default {default}."),
        "minimum": 1, "maximum": max, "default": default})
}

// ---------------------------------------------------------------- query helpers

fn q_str(args: &Value, key: &str) -> Option<String> {
    opt_str(args, key).map(str::to_string)
}

fn q_u64(args: &Value, key: &str) -> Option<String> {
    args.get(key).and_then(Value::as_u64).map(|v| v.to_string())
}

fn q_bool(args: &Value, key: &str) -> Option<String> {
    args.get(key).and_then(Value::as_bool).map(|b| b.to_string())
}

/// Required query value (percent-encoded by `query`).
fn q_req(args: &Value, key: &str) -> Result<String> {
    Ok(req_str(args, key)?.to_string())
}

/// `filter` dropdown; rejected here so a wrong value never reaches the API.
fn q_filter(args: &Value, allowed: &[&str]) -> Result<Option<String>> {
    match opt_str(args, "filter") {
        Some(f) if allowed.contains(&f) => Ok(Some(f.to_string())),
        Some(_) => bail!("`filter` must be one of {}", allowed.join(", ")),
        None => Ok(None),
    }
}

// ---------------------------------------------------------------- adapter

pub struct Trello {
    spec: ToolkitSpec,
    api_base: String,
}

impl Default for Trello {
    fn default() -> Self {
        Self::with_api_base("https://api.trello.com/1")
    }
}

#[allow(clippy::too_many_lines)]
impl Trello {
    pub fn with_api_base(api_base: &str) -> Self {
        let t = |slug, title, desc, props: Props<'_>, req: &[&str], scopes: &[&str], m| {
            tool(SLUG, slug, title, desc, props, req, scopes, m)
        };
        let spec = ToolkitSpec {
            slug: SLUG.into(),
            name: "Trello".into(),
            description: LocalizedText::new(
                "Boards, Listen, Karten, Labels, Checklisten und Kommentare in Trello lesen und bearbeiten.",
                "Read and change boards, lists, cards, labels, checklists and comments in Trello.",
            ),
            auth_schemes: vec![AuthScheme::Basic],
            tools: vec![
                t(
                    "TRELLO_GET_MY_MEMBER",
                    ("Eigenes Profil lesen", "Get my member"),
                    (
                        "Liest den verbundenen Trello-Nutzer (ID, Benutzername, E-Mail, Workspaces).",
                        "Reads the connected Trello user (id, username, email, workspaces).",
                    ),
                    &[],
                    &[],
                    &[READ],
                    false,
                ),
                t(
                    "TRELLO_GET_MEMBER",
                    ("Mitglied lesen", "Get member"),
                    (
                        "Liest ein Trello-Mitglied per ID oder Benutzername.",
                        "Reads a Trello member by id or username.",
                    ),
                    &[("member_id", s("Mitglieds-ID oder Benutzername (aus TRELLO_SEARCH_MEMBERS)"))],
                    &["member_id"],
                    &[READ],
                    false,
                ),
                t(
                    "TRELLO_SEARCH_MEMBERS",
                    ("Mitglieder suchen", "Search members"),
                    (
                        "Findet Trello-Mitglieder nach Name oder Benutzername.",
                        "Finds Trello members by name or username.",
                    ),
                    &[
                        ("query", s("Name oder Benutzername")),
                        ("board_id", s("Suche auf Mitglieder dieses Boards einschränken")),
                        ("only_org_members", flag("Nur Mitglieder der Organisation", false)),
                        ("limit", limit_prop(8, 20)),
                    ],
                    &["query"],
                    &[READ],
                    false,
                ),
                t(
                    "TRELLO_SEARCH_CARDS",
                    ("Karten durchsuchen", "Search cards"),
                    (
                        "Volltextsuche über Trello-Karten; liefert Karten-IDs für die weiteren Tools.",
                        "Full-text search over Trello cards; returns card ids for the other tools.",
                    ),
                    &[
                        ("query", s("Suchtext gegen Name, Beschreibung und Kommentare")),
                        ("board_ids", str_list("Suche auf diese Board-IDs einschränken (leer = alle)")),
                        ("list_id", s("Suche auf eine einzige Liste einschränken")),
                        ("limit", limit_prop(50, 1000)),
                    ],
                    &["query"],
                    &[READ],
                    false,
                ),
                t(
                    "TRELLO_LIST_BOARDS",
                    ("Boards auflisten", "List boards"),
                    (
                        "Listet die Boards des verbundenen Nutzers (Standard) oder eines Mitglieds.",
                        "Lists the boards of the connected user (default) or of one member.",
                    ),
                    &[
                        ("member_id", s("Board dieses Mitglieds; leer = verbundener Nutzer (\"me\")")),
                        ("filter", choice("Welche Boards", &["open", "closed", "all"])),
                    ],
                    &[],
                    &[READ],
                    false,
                ),
                t(
                    "TRELLO_GET_BOARD",
                    ("Board lesen", "Get board"),
                    ("Liest die Details eines Boards.", "Reads the details of one board."),
                    &[("board_id", s("Board-ID (aus TRELLO_LIST_BOARDS)"))],
                    &["board_id"],
                    &[READ],
                    false,
                ),
                t(
                    "TRELLO_LIST_BOARD_LABELS",
                    ("Board-Labels auflisten", "List board labels"),
                    ("Listet die Labels eines Boards.", "Lists the labels of a board."),
                    &[
                        ("board_id", s("Board-ID (aus TRELLO_LIST_BOARDS)")),
                        ("limit", limit_prop(50, 1000)),
                    ],
                    &["board_id"],
                    &[READ],
                    false,
                ),
                t(
                    "TRELLO_LIST_BOARD_MEMBERS",
                    ("Board-Mitglieder auflisten", "List board members"),
                    (
                        "Listet die Mitglieder eines Boards; liefert Mitglieds-IDs für Zuweisungen.",
                        "Lists the members of a board; returns member ids for assignments.",
                    ),
                    &[("board_id", s("Board-ID (aus TRELLO_LIST_BOARDS)"))],
                    &["board_id"],
                    &[READ],
                    false,
                ),
                t(
                    "TRELLO_LIST_LISTS",
                    ("Listen auflisten", "List lists"),
                    ("Listet die Listen (Spalten) eines Boards.", "Lists the lists (columns) of a board."),
                    &[
                        ("board_id", s("Board-ID (aus TRELLO_LIST_BOARDS)")),
                        ("filter", choice("Welche Listen", &["open", "closed", "all"])),
                    ],
                    &["board_id"],
                    &[READ],
                    false,
                ),
                t(
                    "TRELLO_GET_LIST",
                    ("Liste lesen", "Get list"),
                    ("Liest die Details einer Liste.", "Reads the details of one list."),
                    &[("list_id", s("Listen-ID (aus TRELLO_LIST_LISTS)"))],
                    &["list_id"],
                    &[READ],
                    false,
                ),
                t(
                    "TRELLO_LIST_CARDS_IN_LIST",
                    ("Karten einer Liste auflisten", "List cards in list"),
                    ("Listet alle Karten einer Liste.", "Lists all cards in a list."),
                    &[
                        ("list_id", s("Listen-ID (aus TRELLO_LIST_LISTS)")),
                        ("filter", choice("Welche Karten", &["open", "closed", "all"])),
                    ],
                    &["list_id"],
                    &[READ],
                    false,
                ),
                t(
                    "TRELLO_GET_CARD",
                    ("Karte lesen", "Get card"),
                    ("Liest alle Details einer Karte.", "Reads the full details of one card."),
                    &[("card_id", s("Karten-ID (aus TRELLO_SEARCH_CARDS oder TRELLO_LIST_CARDS_IN_LIST)"))],
                    &["card_id"],
                    &[READ],
                    false,
                ),
                t(
                    "TRELLO_LIST_CARD_CHECKLISTS",
                    ("Karten-Checklisten auflisten", "List card checklists"),
                    (
                        "Listet die Checklisten einer Karte samt Abhakpunkten.",
                        "Lists a card's checklists including their items.",
                    ),
                    &[("card_id", s("Karten-ID (aus TRELLO_SEARCH_CARDS)"))],
                    &["card_id"],
                    &[READ],
                    false,
                ),
                t(
                    "TRELLO_LIST_CARD_COMMENTS",
                    ("Karten-Kommentare auflisten", "List card comments"),
                    ("Listet die Kommentare einer Karte.", "Lists a card's comments."),
                    &[
                        ("card_id", s("Karten-ID (aus TRELLO_SEARCH_CARDS)")),
                        ("limit", limit_prop(50, 1000)),
                    ],
                    &["card_id"],
                    &[READ],
                    false,
                ),
                t(
                    "TRELLO_LIST_CARD_ATTACHMENTS",
                    ("Karten-Anhänge auflisten", "List card attachments"),
                    ("Listet die Anhänge einer Karte.", "Lists a card's attachments."),
                    &[("card_id", s("Karten-ID (aus TRELLO_SEARCH_CARDS)"))],
                    &["card_id"],
                    &[READ],
                    false,
                ),
                t(
                    "TRELLO_LIST_ORGANIZATION_BOARDS",
                    ("Workspace-Boards auflisten", "List organization boards"),
                    ("Listet die Boards eines Trello-Workspace (Organization).", "Lists the boards of a Trello workspace (organization)."),
                    &[("org_id", s("Organisations-ID (aus TRELLO_GET_MY_MEMBER)"))],
                    &["org_id"],
                    &[READ],
                    false,
                ),
                t(
                    "TRELLO_CREATE_CARD",
                    ("Karte anlegen", "Create card"),
                    (
                        "Legt eine Karte in einer Liste an; braucht --confirm.",
                        "Creates a card in a list; requires --confirm.",
                    ),
                    &[
                        ("list_id", s("Listen-ID der Zielliste (aus TRELLO_LIST_LISTS)")),
                        ("name", s("Name der Karte")),
                        ("description", s("Beschreibung der Karte")),
                        ("position", choice("\"top\" oder \"bottom\" in der Liste", &["top", "bottom"])),
                        ("labels", str_list("Label-IDs, die angehängt werden (aus TRELLO_LIST_BOARD_LABELS)")),
                    ],
                    &["list_id", "name"],
                    &[WRITE],
                    true,
                ),
                t(
                    "TRELLO_UPDATE_CARD",
                    ("Karte ändern", "Update card"),
                    (
                        "Ändert nur die übergebenen Felder einer Karte; braucht --confirm.",
                        "Changes only the card fields you pass; requires --confirm.",
                    ),
                    &[
                        ("card_id", s("Karten-ID (aus TRELLO_SEARCH_CARDS)")),
                        ("name", s("Neuer Name")),
                        ("description", s("Neue Beschreibung")),
                        ("list_id", s("Karte in diese Liste verschieben")),
                        ("position", choice("Neue Position", &["top", "bottom"])),
                        ("labels", str_list("Ersetzt alle Label-IDs der Karte")),
                        ("closed", bool_prop("Karte archivieren (true) oder wiederherstellen (false)")),
                        ("due", s("Fälligkeitsdatum als ISO-Zeitpunkt, z. B. 2026-03-13T16:00:00.000Z")),
                    ],
                    &["card_id"],
                    &[WRITE],
                    true,
                ),
                t(
                    "TRELLO_DELETE_CARD",
                    ("Karte löschen", "Delete card"),
                    (
                        "Löscht eine Karte endgültig; braucht --confirm.",
                        "Permanently deletes a card; requires --confirm.",
                    ),
                    &[("card_id", s("Karten-ID (aus TRELLO_SEARCH_CARDS)"))],
                    &["card_id"],
                    &[WRITE],
                    true,
                ),
                t(
                    "TRELLO_ADD_COMMENT_TO_CARD",
                    ("Karte kommentieren", "Add comment to card"),
                    (
                        "Kommentiert eine Karte; braucht --confirm.",
                        "Adds a comment to a card; requires --confirm.",
                    ),
                    &[
                        ("card_id", s("Karten-ID (aus TRELLO_SEARCH_CARDS)")),
                        ("text", s("Kommentartext")),
                    ],
                    &["card_id", "text"],
                    &[WRITE],
                    true,
                ),
                t(
                    "TRELLO_DELETE_COMMENT",
                    ("Kommentar löschen", "Delete comment"),
                    (
                        "Löscht einen Kommentar einer Karte; braucht --confirm.",
                        "Deletes a comment on a card; requires --confirm.",
                    ),
                    &[
                        ("card_id", s("Karten-ID (aus TRELLO_SEARCH_CARDS)")),
                        ("comment_id", s("Kommentar-ID (aus TRELLO_LIST_CARD_COMMENTS)")),
                    ],
                    &["card_id", "comment_id"],
                    &[WRITE],
                    true,
                ),
                t(
                    "TRELLO_ADD_LABEL_TO_CARD",
                    ("Label anhängen", "Add label to card"),
                    (
                        "Hängt ein bestehendes Label an eine Karte; braucht --confirm.",
                        "Attaches an existing label to a card; requires --confirm.",
                    ),
                    &[
                        ("card_id", s("Karten-ID (aus TRELLO_SEARCH_CARDS)")),
                        ("label_id", s("Label-ID (aus TRELLO_LIST_BOARD_LABELS)")),
                    ],
                    &["card_id", "label_id"],
                    &[WRITE],
                    true,
                ),
                t(
                    "TRELLO_REMOVE_LABEL_FROM_CARD",
                    ("Label entfernen", "Remove label from card"),
                    (
                        "Nimmt ein Label von einer Karte; braucht --confirm.",
                        "Detaches a label from a card; requires --confirm.",
                    ),
                    &[
                        ("card_id", s("Karten-ID (aus TRELLO_SEARCH_CARDS)")),
                        ("label_id", s("Label-ID (aus TRELLO_GET_CARD oder TRELLO_LIST_BOARD_LABELS)")),
                    ],
                    &["card_id", "label_id"],
                    &[WRITE],
                    true,
                ),
                t(
                    "TRELLO_ADD_MEMBER_TO_CARD",
                    ("Mitglied zuweisen", "Add member to card"),
                    (
                        "Weist ein Mitglied einer Karte zu; braucht --confirm.",
                        "Assigns a member to a card; requires --confirm.",
                    ),
                    &[
                        ("card_id", s("Karten-ID (aus TRELLO_SEARCH_CARDS)")),
                        ("member_id", s("Mitglieds-ID (aus TRELLO_SEARCH_MEMBERS oder TRELLO_LIST_BOARD_MEMBERS)")),
                    ],
                    &["card_id", "member_id"],
                    &[WRITE],
                    true,
                ),
                t(
                    "TRELLO_CREATE_BOARD_LABEL",
                    ("Board-Label anlegen", "Create board label"),
                    (
                        "Legt ein Label auf einem Board an; braucht --confirm.",
                        "Creates a label on a board; requires --confirm.",
                    ),
                    &[
                        ("board_id", s("Board-ID (aus TRELLO_LIST_BOARDS)")),
                        ("name", s("Label-Name")),
                        ("color", s("Label-Farbe: green, yellow, orange, red, purple, blue, sky, lime, pink oder black")),
                    ],
                    &["board_id"],
                    &[WRITE],
                    true,
                ),
                t(
                    "TRELLO_MOVE_CARD",
                    ("Karte verschieben", "Move card"),
                    (
                        "Verschiebt eine Karte in eine andere Liste; braucht --confirm.",
                        "Moves a card to another list; requires --confirm.",
                    ),
                    &[
                        ("card_id", s("Karten-ID (aus TRELLO_SEARCH_CARDS)")),
                        ("list_id", s("Ziel-Listen-ID (aus TRELLO_LIST_LISTS)")),
                    ],
                    &["card_id", "list_id"],
                    &[WRITE],
                    true,
                ),
                t(
                    "TRELLO_ARCHIVE_CARD",
                    ("Karte archivieren", "Archive card"),
                    (
                        "Archiviert eine Karte oder holt sie zurück; braucht --confirm.",
                        "Archives a card or restores it; requires --confirm.",
                    ),
                    &[
                        ("card_id", s("Karten-ID (aus TRELLO_SEARCH_CARDS)")),
                        ("archived", flag("true = archivieren, false = wiederherstellen", true)),
                    ],
                    &["card_id"],
                    &[WRITE],
                    true,
                ),
                t(
                    "TRELLO_CREATE_LIST",
                    ("Liste anlegen", "Create list"),
                    (
                        "Legt eine Liste (Spalte) auf einem Board an; braucht --confirm.",
                        "Creates a list (column) on a board; requires --confirm.",
                    ),
                    &[
                        ("board_id", s("Board-ID (aus TRELLO_LIST_BOARDS)")),
                        ("name", s("Name der Liste")),
                        ("position", choice("\"top\" oder \"bottom\" am Board", &["top", "bottom"])),
                    ],
                    &["board_id", "name"],
                    &[WRITE],
                    true,
                ),
                t(
                    "TRELLO_CREATE_BOARD",
                    ("Board anlegen", "Create board"),
                    (
                        "Legt ein neues Board an, optional in einem Workspace; braucht --confirm.",
                        "Creates a new board, optionally in a workspace; requires --confirm.",
                    ),
                    &[
                        ("name", s("Name des Boards")),
                        ("description", s("Beschreibung des Boards")),
                        ("org_id", s("Organisations-ID des Workspaces; leer = privates Board")),
                    ],
                    &["name"],
                    &[WRITE],
                    true,
                ),
                t(
                    "TRELLO_ADD_CHECKLIST_TO_CARD",
                    ("Checkliste anhängen", "Add checklist to card"),
                    (
                        "Legt eine Checkliste auf einer Karte an; braucht --confirm.",
                        "Creates a checklist on a card; requires --confirm.",
                    ),
                    &[
                        ("card_id", s("Karten-ID (aus TRELLO_SEARCH_CARDS)")),
                        ("name", s("Name der Checkliste")),
                    ],
                    &["card_id"],
                    &[WRITE],
                    true,
                ),
            ],
        };
        Self {
            spec,
            api_base: api_base.trim_end_matches('/').to_string(),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}/{}", self.api_base, path.trim_start_matches('/'))
    }

    /// GET with optional query pairs (`key`/`token` auth params come from the hub).
    fn get(&self, path: &str, pairs: &[(&str, Option<String>)]) -> HttpRequest {
        HttpRequest::new("GET", format!("{}{}", self.url(path), query(pairs)))
            .header("accept", "application/json")
    }

    /// POST/PUT/DELETE with query params and no body, as the piece sends them.
    fn send(&self, method: &str, path: &str, pairs: &[(&str, Option<String>)]) -> HttpRequest {
        HttpRequest::new(method, format!("{}{}", self.url(path), query(pairs)))
            .header("accept", "application/json")
    }

    /// POST/PUT with query params and a JSON body (`Content-Type: application/json`).
    fn send_json(
        &self,
        method: &str,
        path: &str,
        pairs: &[(&str, Option<String>)],
        body: Value,
    ) -> HttpRequest {
        self.send(method, path, pairs).json(&body)
    }

    /// Whose boards to list: the given member or the connected user (`me`).
    fn member_ref(&self, args: &Value) -> Result<String> {
        match opt_str(args, "member_id") {
            Some(_) => Ok(path_segment(args, "member_id")?.to_string()),
            None => Ok("me".to_string()),
        }
    }
}

impl ToolkitAdapter for Trello {
    fn spec(&self) -> &ToolkitSpec {
        &self.spec
    }

    #[allow(clippy::too_many_lines)]
    fn build_request(&self, tool: &str, args: &Value) -> Result<HttpRequest> {
        Ok(match tool {
            // AP: src/lib/actions/ai/get-my-member.ts
            "TRELLO_GET_MY_MEMBER" => self.get("members/me", &[]),
            // AP: src/lib/actions/ai/get-member.ts
            "TRELLO_GET_MEMBER" => self.get(&format!("members/{}", path_segment(args, "member_id")?), &[]),
            // AP: src/lib/actions/ai/search-members.ts
            "TRELLO_SEARCH_MEMBERS" => self.get(
                "search/members",
                &[
                    ("query", q_req(args, "query")?.into()),
                    ("idBoard", q_str(args, "board_id")),
                    ("onlyOrgMembers", q_bool(args, "only_org_members")),
                    ("limit", q_u64(args, "limit")),
                ],
            ),
            // AP: src/lib/actions/ai/search-cards.ts
            "TRELLO_SEARCH_CARDS" => {
                let boards = str_vec(args, "board_ids");
                self.get(
                    "search",
                    &[
                        ("query", q_req(args, "query")?.into()),
                        ("modelTypes", Some("cards".into())),
                        ("idBoards", (!boards.is_empty()).then(|| boards.join(","))),
                        ("idList", q_str(args, "list_id")),
                        ("cards_limit", q_u64(args, "limit")),
                    ],
                )
            }
            // AP: src/lib/actions/ai/list-boards.ts
            "TRELLO_LIST_BOARDS" => self.get(
                &format!("members/{}/boards", self.member_ref(args)?),
                &[("filter", q_filter(args, &["open", "closed", "all"])?)],
            ),
            // AP: src/lib/actions/ai/get-board.ts
            "TRELLO_GET_BOARD" => self.get(&format!("boards/{}", path_segment(args, "board_id")?), &[]),
            // AP: src/lib/actions/ai/list-board-labels.ts
            "TRELLO_LIST_BOARD_LABELS" => self.get(
                &format!("boards/{}/labels", path_segment(args, "board_id")?),
                &[("limit", q_u64(args, "limit"))],
            ),
            // AP: src/lib/actions/ai/list-board-members.ts
            "TRELLO_LIST_BOARD_MEMBERS" => {
                self.get(&format!("boards/{}/members", path_segment(args, "board_id")?), &[])
            }
            // AP: src/lib/actions/ai/list-lists.ts
            "TRELLO_LIST_LISTS" => self.get(
                &format!("boards/{}/lists", path_segment(args, "board_id")?),
                &[("filter", q_filter(args, &["open", "closed", "all"])?)],
            ),
            // AP: src/lib/actions/ai/get-list.ts
            "TRELLO_GET_LIST" => self.get(&format!("lists/{}", path_segment(args, "list_id")?), &[]),
            // AP: src/lib/actions/ai/list-cards-in-list.ts
            "TRELLO_LIST_CARDS_IN_LIST" => self.get(
                &format!("lists/{}/cards", path_segment(args, "list_id")?),
                &[("filter", q_filter(args, &["open", "closed", "all"])?)],
            ),
            // AP: src/lib/actions/ai/get-card-ai.ts
            "TRELLO_GET_CARD" => self.get(&format!("cards/{}", path_segment(args, "card_id")?), &[]),
            // AP: src/lib/actions/ai/list-card-checklists.ts
            "TRELLO_LIST_CARD_CHECKLISTS" => self.get(
                &format!("cards/{}/checklists", path_segment(args, "card_id")?),
                &[("checkItems", Some("all".into()))],
            ),
            // AP: src/lib/actions/ai/list-card-comments.ts
            "TRELLO_LIST_CARD_COMMENTS" => self.get(
                &format!("cards/{}/actions", path_segment(args, "card_id")?),
                &[("filter", Some("commentCard".into())), ("limit", q_u64(args, "limit"))],
            ),
            // AP: src/lib/actions/ai/list-card-attachments-ai.ts
            "TRELLO_LIST_CARD_ATTACHMENTS" => {
                self.get(&format!("cards/{}/attachments", path_segment(args, "card_id")?), &[])
            }
            // AP: src/lib/actions/ai/list-organization-boards.ts
            "TRELLO_LIST_ORGANIZATION_BOARDS" => self.get(
                &format!("organizations/{}/boards", path_segment(args, "org_id")?),
                &[],
            ),
            // AP: src/lib/actions/ai/create-card-ai.ts
            "TRELLO_CREATE_CARD" => {
                let mut body = Map::new();
                body.insert("name".into(), json!(req_str(args, "name")?));
                if let Some(v) = opt_str(args, "description") {
                    body.insert("desc".into(), json!(v));
                }
                if let Some(v) = opt_str(args, "position") {
                    if !["top", "bottom"].contains(&v) {
                        bail!("`position` must be top or bottom");
                    }
                    body.insert("pos".into(), json!(v));
                }
                let labels = str_vec(args, "labels");
                if !labels.is_empty() {
                    body.insert("idLabels".into(), json!(labels));
                }
                self.send_json(
                    "POST",
                    "cards",
                    &[("idList", Some(path_segment(args, "list_id")?.to_string()))],
                    Value::Object(body),
                )
            }
            // AP: src/lib/actions/ai/update-card-ai.ts
            "TRELLO_UPDATE_CARD" => {
                let mut body = Map::new();
                if let Some(v) = opt_str(args, "name") {
                    body.insert("name".into(), json!(v));
                }
                if let Some(v) = opt_str(args, "description") {
                    body.insert("desc".into(), json!(v));
                }
                if opt_str(args, "list_id").is_some() {
                    body.insert("idList".into(), json!(path_segment(args, "list_id")?));
                }
                if let Some(v) = opt_str(args, "position") {
                    if !["top", "bottom"].contains(&v) {
                        bail!("`position` must be top or bottom");
                    }
                    body.insert("pos".into(), json!(v));
                }
                let labels = str_vec(args, "labels");
                if !labels.is_empty() {
                    body.insert("idLabels".into(), json!(labels));
                }
                if let Some(closed) = args.get("closed").and_then(Value::as_bool) {
                    body.insert("closed".into(), json!(closed));
                }
                if let Some(v) = opt_str(args, "due") {
                    body.insert("due".into(), json!(v));
                }
                if body.is_empty() {
                    bail!("nothing to update: pass at least one card field to change");
                }
                self.send_json(
                    "PUT",
                    &format!("cards/{}", path_segment(args, "card_id")?),
                    &[],
                    Value::Object(body),
                )
            }
            // AP: src/lib/actions/ai/delete-card-ai.ts
            "TRELLO_DELETE_CARD" => {
                self.send("DELETE", &format!("cards/{}", path_segment(args, "card_id")?), &[])
            }
            // AP: src/lib/actions/ai/add-comment-to-card.ts
            "TRELLO_ADD_COMMENT_TO_CARD" => self.send(
                "POST",
                &format!("cards/{}/actions/comments", path_segment(args, "card_id")?),
                &[("text", q_req(args, "text")?.into())],
            ),
            // AP: src/lib/actions/ai/delete-comment.ts
            "TRELLO_DELETE_COMMENT" => self.send(
                "DELETE",
                &format!(
                    "cards/{}/actions/{}/comments",
                    path_segment(args, "card_id")?,
                    path_segment(args, "comment_id")?
                ),
                &[],
            ),
            // AP: src/lib/actions/ai/add-label-to-card.ts
            "TRELLO_ADD_LABEL_TO_CARD" => self.send(
                "POST",
                &format!("cards/{}/idLabels", path_segment(args, "card_id")?),
                &[("value", Some(path_segment(args, "label_id")?.to_string()))],
            ),
            // AP: src/lib/actions/ai/remove-label-from-card.ts
            "TRELLO_REMOVE_LABEL_FROM_CARD" => self.send(
                "DELETE",
                &format!(
                    "cards/{}/idLabels/{}",
                    path_segment(args, "card_id")?,
                    path_segment(args, "label_id")?
                ),
                &[],
            ),
            // AP: src/lib/actions/ai/add-member-to-card.ts
            "TRELLO_ADD_MEMBER_TO_CARD" => self.send(
                "POST",
                &format!("cards/{}/idMembers", path_segment(args, "card_id")?),
                &[("value", Some(path_segment(args, "member_id")?.to_string()))],
            ),
            // AP: src/lib/actions/ai/create-board-label.ts
            "TRELLO_CREATE_BOARD_LABEL" => self.send(
                "POST",
                "labels",
                &[
                    ("idBoard", Some(path_segment(args, "board_id")?.to_string())),
                    ("name", q_str(args, "name")),
                    ("color", q_str(args, "color")),
                ],
            ),
            // AP: src/lib/actions/ai/move-card.ts
            "TRELLO_MOVE_CARD" => self.send(
                "PUT",
                &format!("cards/{}/idList", path_segment(args, "card_id")?),
                &[("value", Some(path_segment(args, "list_id")?.to_string()))],
            ),
            // AP: src/lib/actions/ai/archive-card.ts
            "TRELLO_ARCHIVE_CARD" => {
                let archived = args.get("archived").and_then(Value::as_bool).unwrap_or(true);
                self.send(
                    "PUT",
                    &format!("cards/{}/closed", path_segment(args, "card_id")?),
                    &[("value", Some(archived.to_string()))],
                )
            }
            // AP: src/lib/actions/ai/create-list.ts
            "TRELLO_CREATE_LIST" => {
                let position = match opt_str(args, "position") {
                    Some(p) if ["top", "bottom"].contains(&p) => Some(p.to_string()),
                    Some(_) => bail!("`position` must be top or bottom"),
                    None => None,
                };
                self.send(
                    "POST",
                    "lists",
                    &[
                        ("name", q_req(args, "name")?.into()),
                        ("idBoard", Some(path_segment(args, "board_id")?.to_string())),
                        ("pos", position),
                    ],
                )
            }
            // AP: src/lib/actions/ai/create-board.ts
            "TRELLO_CREATE_BOARD" => self.send(
                "POST",
                "boards",
                &[
                    ("name", q_req(args, "name")?.into()),
                    ("desc", q_str(args, "description")),
                    ("idOrganization", q_str(args, "org_id")),
                ],
            ),
            // AP: src/lib/actions/ai/add-checklist-to-card.ts
            "TRELLO_ADD_CHECKLIST_TO_CARD" => self.send(
                "POST",
                &format!("cards/{}/checklists", path_segment(args, "card_id")?),
                &[("name", q_str(args, "name"))],
            ),
            other => bail!("unknown Trello tool `{other}`"),
        })
    }

    /// List endpoints are reduced to `{<name>, count}`; everything else is the
    /// provider body verbatim (what the Activepieces actions return).
    fn parse_response(&self, tool: &str, response: &HttpResponse) -> Result<Value> {
        let v = response.json()?;
        let list = |key: &str, items: &Value| -> Value {
            let mut out = Map::new();
            out.insert(key.to_string(), items.clone());
            out.insert("count".into(), json!(items.as_array().map_or(0, Vec::len)));
            Value::Object(out)
        };
        Ok(match tool {
            "TRELLO_SEARCH_MEMBERS" => list("members", &v),
            "TRELLO_SEARCH_CARDS" => list("cards", &v["cards"]),
            "TRELLO_LIST_BOARDS" | "TRELLO_LIST_ORGANIZATION_BOARDS" => list("boards", &v),
            "TRELLO_LIST_BOARD_LABELS" => list("labels", &v),
            "TRELLO_LIST_BOARD_MEMBERS" => list("members", &v),
            "TRELLO_LIST_LISTS" => list("lists", &v),
            "TRELLO_LIST_CARDS_IN_LIST" => list("cards", &v),
            "TRELLO_LIST_CARD_CHECKLISTS" => list("checklists", &v),
            "TRELLO_LIST_CARD_COMMENTS" => list("comments", &v),
            "TRELLO_LIST_CARD_ATTACHMENTS" => list("attachments", &v),
            _ => v,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_and_validation() {
        let t = Trello::with_api_base("https://trello.test/1");
        let r = t.build_request("TRELLO_GET_MY_MEMBER", &json!({})).unwrap();
        assert_eq!(r.method, "GET");
        assert_eq!(r.url, "https://trello.test/1/members/me");

        // Query values are percent-encoded.
        let r = t
            .build_request(
                "TRELLO_SEARCH_CARDS",
                &json!({"query": "login bug", "board_ids": ["614d8a9b1c2d3e4f5a6b7c8d"]}),
            )
            .unwrap();
        assert_eq!(
            r.url,
            "https://trello.test/1/search?query=login+bug&modelTypes=cards\
             &idBoards=614d8a9b1c2d3e4f5a6b7c8d"
        );

        // list_boards defaults to the connected member.
        assert_eq!(
            t.build_request("TRELLO_LIST_BOARDS", &json!({})).unwrap().url,
            "https://trello.test/1/members/me/boards"
        );

        // Path params are guarded.
        assert!(t.build_request("TRELLO_GET_CARD", &json!({"card_id": "../other"})).is_err());
        assert!(t.build_request("TRELLO_GET_CARD", &json!({})).is_err());

        // Known enum values only.
        assert!(t.build_request("TRELLO_LIST_BOARDS", &json!({"filter": "weird"})).is_err());
        assert!(t
            .build_request("TRELLO_CREATE_CARD", &json!({"list_id": "624d8a9b1c2d3e4f5a6b7c8d", "name": "N", "position": "middle"}))
            .is_err());

        // Mutations that write nothing are rejected.
        assert!(t.build_request("TRELLO_UPDATE_CARD", &json!({"card_id": "5f4d8a9b1c2d3e4f5a6b7c8d"})).is_err());

        // Create card: idList in the query, payload in the JSON body.
        let r = t
            .build_request(
                "TRELLO_CREATE_CARD",
                &json!({"list_id": "624d8a9b1c2d3e4f5a6b7c8d", "name": "N", "position": "top"}),
            )
            .unwrap();
        assert_eq!(r.url, "https://trello.test/1/cards?idList=624d8a9b1c2d3e4f5a6b7c8d");
        assert_eq!(r.method, "POST");
        let body: Value = serde_json::from_slice(r.body.as_deref().unwrap()).unwrap();
        assert_eq!(body, json!({"name": "N", "pos": "top"}));
    }

    #[test]
    fn list_results_are_counted() {
        let t = Trello::with_api_base("https://trello.test/1");
        let resp = HttpResponse {
            status: 200,
            body: serde_json::to_vec(&json!([{"id": "624d8a9b1c2d3e4f5a6b7c8d", "name": "Todo"}])).unwrap(),
            session_id: None,
        };
        let out = t.parse_response("TRELLO_LIST_LISTS", &resp).unwrap();
        assert_eq!(out["count"], json!(1));
        assert_eq!(out["lists"][0]["name"], json!("Todo"));

        // Search returns the card slice of the provider body.
        let resp = HttpResponse {
            status: 200,
            body: serde_json::to_vec(&json!({"cards": [{"id": "5f4d8a9b1c2d3e4f5a6b7c8d"}], "members": []}))
                .unwrap(),
            session_id: None,
        };
        let out = t.parse_response("TRELLO_SEARCH_CARDS", &resp).unwrap();
        assert_eq!(out["count"], json!(1));
        assert_eq!(out["cards"][0]["id"], json!("5f4d8a9b1c2d3e4f5a6b7c8d"));
    }
}
