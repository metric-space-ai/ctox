//! Privacy settings — port of `_upstream/whatsmeow/privacysettings.go`.
//!
//! Wire shape mirrors upstream verbatim. Fetch is a `<iq xmlns="privacy"
//! type="get"><privacy/></iq>` to `s.whatsapp.net`; the response's
//! `<privacy>` child carries one `<category name="..." value="..."/>` per
//! settable axis. Set is the same envelope with `type="set"` and a single
//! `<category>` child.
//!
//! Enum values match `types.PrivacySetting` and `types.PrivacySettingType`
//! upstream byte-for-byte (see `_upstream/whatsmeow/types/user.go`). Tests
//! pin both the IQ shape and the round-trip for every category.

use tracing::warn;
use wha_binary::{Attrs, Node, Value};
use wha_types::Jid;

use crate::client::Client;
use crate::error::ClientError;
use crate::request::{InfoQuery, IqType};

// ---------------------------------------------------------------------------
// Wire enums.
// ---------------------------------------------------------------------------

/// The category-name dimension of a privacy IQ — `name="..."`.
///
/// Matches `types.PrivacySettingType` upstream — see
/// `_upstream/whatsmeow/types/user.go`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PrivacySetting {
    /// `groupadd` — who can add you to a group.
    GroupAdd,
    /// `last` — who sees last-seen.
    LastSeen,
    /// `online` — who sees you as online.
    Online,
    /// `profile` — who sees your profile picture.
    Profile,
    /// `status` — who sees your status updates.
    Status,
    /// `readreceipts` — whether to send read receipts.
    ReadReceipts,
    /// `calladd` — who can call you.
    CallAdd,
    /// `disappearing_mode` (used as a category attribute name in some
    /// notifications). The upstream IQ for setting the timer uses a
    /// separate namespace; `DisappearingMode` here is kept for parity with
    /// the parser's switch arms in upstream `parsePrivacySettings`.
    DisappearingMode,
    /// `messages` — who can send you messages.
    Messages,
    /// `defense` — anti-stalker / link defense flag.
    Defense,
    /// `stickers` — who can send you stickers.
    Stickers,
}

impl PrivacySetting {
    pub fn as_str(self) -> &'static str {
        match self {
            PrivacySetting::GroupAdd => "groupadd",
            PrivacySetting::LastSeen => "last",
            PrivacySetting::Online => "online",
            PrivacySetting::Profile => "profile",
            PrivacySetting::Status => "status",
            PrivacySetting::ReadReceipts => "readreceipts",
            PrivacySetting::CallAdd => "calladd",
            PrivacySetting::DisappearingMode => "disappearing_mode",
            PrivacySetting::Messages => "messages",
            PrivacySetting::Defense => "defense",
            PrivacySetting::Stickers => "stickers",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        Some(match s {
            "groupadd" => PrivacySetting::GroupAdd,
            "last" => PrivacySetting::LastSeen,
            "online" => PrivacySetting::Online,
            "profile" => PrivacySetting::Profile,
            "status" => PrivacySetting::Status,
            "readreceipts" => PrivacySetting::ReadReceipts,
            "calladd" => PrivacySetting::CallAdd,
            // The category-name wire form for the default disappearing-mode
            // duration is `disappearing` in the privacy-settings IQ response,
            // but `disappearing_mode` in change-notifications. Accept both so
            // the parser handles either shape transparently.
            "disappearing" | "disappearing_mode" => PrivacySetting::DisappearingMode,
            "messages" => PrivacySetting::Messages,
            "defense" => PrivacySetting::Defense,
            "stickers" => PrivacySetting::Stickers,
            _ => return None,
        })
    }
}

// ---------------------------------------------------------------------------
// Settings struct.
// ---------------------------------------------------------------------------

/// Snapshot of the user's privacy settings — mirrors `types.PrivacySettings`
/// upstream. Each field stores the wire value (`"all"`, `"contacts"`, …) as
/// a plain `String` for forward compatibility; valid value sets are
/// documented per-field in upstream.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PrivacySettings {
    /// `groupadd` — `all` | `contacts` | `contact_blacklist` | `none`.
    pub group_add: String,
    /// `last` — same set as `group_add`.
    pub last_seen: String,
    /// `online` — `all` | `match_last_seen`.
    pub online: String,
    /// `profile` — same set as `group_add`.
    pub profile: String,
    /// `status` — same set as `group_add`.
    pub status: String,
    /// `readreceipts` — `all` | `none`.
    pub read_receipts: String,
    /// `calladd` — `all` | `known`.
    pub call_add: String,
    /// `disappearing_mode` default duration in seconds, as a string.
    pub default_disappearing_mode: String,
    /// `messages` — `all` | `contacts`.
    pub messages: String,
    /// `defense` — `on_standard` | `off`.
    pub defense: String,
    /// `stickers` — `contacts` | `contact_allowlist` | `none`.
    pub stickers: String,
}

impl PrivacySettings {
    /// Pre-populated snapshot used as a starting point for parsing.
    ///
    /// Categories absent from the server's `<privacy>` response keep these
    /// defaults rather than empty strings — `"all"` for the open-by-default
    /// settings, `"0"` for the disappearing-mode timer (no default timer).
    /// These mirror the behaviour the WhatsApp clients exhibit when a fresh
    /// account has never explicitly toggled a setting.
    fn with_defaults() -> Self {
        Self {
            group_add: "all".into(),
            last_seen: "all".into(),
            online: "all".into(),
            profile: "all".into(),
            status: "all".into(),
            read_receipts: "all".into(),
            call_add: "all".into(),
            default_disappearing_mode: "0".into(),
            messages: "all".into(),
            defense: "off".into(),
            stickers: "all".into(),
        }
    }

    pub fn set(&mut self, name: PrivacySetting, value: &str) {
        let v = value.to_owned();
        match name {
            PrivacySetting::GroupAdd => self.group_add = v,
            PrivacySetting::LastSeen => self.last_seen = v,
            PrivacySetting::Online => self.online = v,
            PrivacySetting::Profile => self.profile = v,
            PrivacySetting::Status => self.status = v,
            PrivacySetting::ReadReceipts => self.read_receipts = v,
            PrivacySetting::CallAdd => self.call_add = v,
            PrivacySetting::DisappearingMode => self.default_disappearing_mode = v,
            PrivacySetting::Messages => self.messages = v,
            PrivacySetting::Defense => self.defense = v,
            PrivacySetting::Stickers => self.stickers = v,
        }
    }
}

// ---------------------------------------------------------------------------
// Builders.
// ---------------------------------------------------------------------------

fn server_jid() -> Jid {
    Jid::new("", wha_types::jid::server::DEFAULT_USER)
}

/// Build the `<iq xmlns="privacy" type="get"><privacy/></iq>` IQ.
pub fn build_get_privacy_settings_iq() -> InfoQuery {
    InfoQuery::new("privacy", IqType::Get)
        .to(server_jid())
        .content(Value::Nodes(vec![Node::tag_only("privacy")]))
}

/// Build the `<iq xmlns="privacy" type="set"><privacy><category .../></privacy></iq>` IQ.
pub fn build_set_privacy_setting_iq(name: PrivacySetting, value: &str) -> InfoQuery {
    let mut category_attrs = Attrs::new();
    category_attrs.insert("name".into(), Value::String(name.as_str().to_owned()));
    category_attrs.insert("value".into(), Value::String(value.to_owned()));
    let category = Node::new("category", category_attrs, None);
    let privacy = Node::new("privacy", Attrs::new(), Some(Value::Nodes(vec![category])));

    InfoQuery::new("privacy", IqType::Set)
        .to(server_jid())
        .content(Value::Nodes(vec![privacy]))
}

// ---------------------------------------------------------------------------
// Parsing.
// ---------------------------------------------------------------------------

/// Parse a privacy IQ response (or its inner `<privacy>` node) into a
/// [`PrivacySettings`].
///
/// Mirrors `Client.parsePrivacySettings` upstream with two ergonomic
/// extensions:
///
/// 1. The caller may pass either the `<iq>` envelope (in which case the
///    function descends to the first `<privacy>` child) or the bare
///    `<privacy>` node directly. This makes it trivial to plug into both the
///    `TryFetchPrivacySettings` flow (which already extracts the child) and
///    notification handlers that hand us the inner node.
/// 2. Unknown category names are warn-logged for visibility but never error
///    — forward-compatibility with new server-side categories.
///
/// Categories absent from the response are filled with sane defaults
/// ([`PrivacySettings::with_defaults`]), so callers never see an
/// empty-string axis for a setting the server simply chose to omit.
pub fn parse_privacy_settings(node: &Node) -> PrivacySettings {
    // Accept either the <iq> wrapper or the bare <privacy> node. If we were
    // handed the wrapper, descend to its first <privacy> child; if we were
    // handed the <privacy> node itself, use it as-is. Anything else (e.g. a
    // pure <error> response) yields the default snapshot.
    let privacy: &Node = if node.tag == "privacy" {
        node
    } else {
        match node.children().iter().find(|c| c.tag == "privacy") {
            Some(p) => p,
            None => {
                warn!(
                    tag = %node.tag,
                    "parse_privacy_settings: no <privacy> child found, returning defaults"
                );
                return PrivacySettings::with_defaults();
            }
        }
    };

    let mut out = PrivacySettings::with_defaults();
    for child in privacy.children() {
        if child.tag != "category" {
            continue;
        }
        let raw_name = child.get_attr_str("name").unwrap_or("");
        let name = match PrivacySetting::from_str(raw_name) {
            Some(n) => n,
            None => {
                warn!(
                    category = raw_name,
                    "parse_privacy_settings: ignoring unknown privacy category"
                );
                continue;
            }
        };
        let value = child.get_attr_str("value").unwrap_or("");
        out.set(name, value);
    }
    out
}

// ---------------------------------------------------------------------------
// Public client API.
// ---------------------------------------------------------------------------

/// Fetch the user's privacy settings from the server.
///
/// Mirrors upstream's `TryFetchPrivacySettings` minus the in-memory cache —
/// callers that want a cache should layer it themselves.
pub async fn get_privacy_settings(client: &Client) -> Result<PrivacySettings, ClientError> {
    let resp = client.send_iq(build_get_privacy_settings_iq()).await?;
    // Hard-fail the public API when the server omitted <privacy>: that is
    // genuinely malformed for a successful IQ result, and surfacing it lets
    // callers retry. The bare-<privacy>/IQ-wrapper flexibility lives on the
    // pure parser (`parse_privacy_settings`) for notification handlers and
    // tests; here we want the strict contract.
    if !resp.children().iter().any(|c| c.tag == "privacy") {
        return Err(ClientError::Malformed(
            "privacy IQ response missing <privacy> child".into(),
        ));
    }
    Ok(parse_privacy_settings(&resp))
}

/// Set a single privacy category on the server. Returns `()` on success;
/// upstream additionally re-fetches the settings — callers can call
/// [`get_privacy_settings`] themselves to mirror that behaviour.
pub async fn set_privacy_setting(
    client: &Client,
    name: PrivacySetting,
    value: &str,
) -> Result<(), ClientError> {
    let _ = client.send_iq(build_set_privacy_setting_iq(name, value)).await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_get_iq_has_expected_shape() {
        let iq = build_get_privacy_settings_iq();
        let node = iq.into_node("REQ-1".into());
        assert_eq!(node.tag, "iq");
        assert_eq!(node.get_attr_str("xmlns"), Some("privacy"));
        assert_eq!(node.get_attr_str("type"), Some("get"));
        // to=s.whatsapp.net is encoded as a Jid with empty user.
        let to = node.get_attr_jid("to").expect("to");
        assert_eq!(to.server, wha_types::jid::server::DEFAULT_USER);
        // single <privacy/> child.
        let kids = node.children();
        assert_eq!(kids.len(), 1);
        assert_eq!(kids[0].tag, "privacy");
        assert!(kids[0].children().is_empty());
    }

    #[test]
    fn build_set_iq_carries_category() {
        let iq = build_set_privacy_setting_iq(PrivacySetting::LastSeen, "contacts");
        let node = iq.into_node("REQ-2".into());
        assert_eq!(node.get_attr_str("type"), Some("set"));
        let privacy = &node.children()[0];
        assert_eq!(privacy.tag, "privacy");
        let category = &privacy.children()[0];
        assert_eq!(category.tag, "category");
        assert_eq!(category.get_attr_str("name"), Some("last"));
        assert_eq!(category.get_attr_str("value"), Some("contacts"));
    }

    #[test]
    fn parse_privacy_settings_collects_known_categories() {
        // Build a synthetic <privacy> response.
        let mk_cat = |name: &str, value: &str| {
            let mut a = Attrs::new();
            a.insert("name".into(), Value::String(name.into()));
            a.insert("value".into(), Value::String(value.into()));
            Node::new("category", a, None)
        };
        let privacy = Node::new(
            "privacy",
            Attrs::new(),
            Some(Value::Nodes(vec![
                mk_cat("last", "contacts"),
                mk_cat("groupadd", "contact_blacklist"),
                mk_cat("readreceipts", "all"),
                mk_cat("online", "match_last_seen"),
                mk_cat("calladd", "known"),
                mk_cat("status", "contacts"),
                mk_cat("profile", "all"),
                mk_cat("messages", "contacts"),
                mk_cat("defense", "off"),
                mk_cat("stickers", "contact_allowlist"),
                // Unknown category: silently ignored, must not error.
                mk_cat("totally_new_thing", "yes"),
            ])),
        );
        let settings = parse_privacy_settings(&privacy);
        assert_eq!(settings.last_seen, "contacts");
        assert_eq!(settings.group_add, "contact_blacklist");
        assert_eq!(settings.read_receipts, "all");
        assert_eq!(settings.online, "match_last_seen");
        assert_eq!(settings.call_add, "known");
        assert_eq!(settings.status, "contacts");
        assert_eq!(settings.profile, "all");
        assert_eq!(settings.messages, "contacts");
        assert_eq!(settings.defense, "off");
        assert_eq!(settings.stickers, "contact_allowlist");
    }

    #[test]
    fn privacy_setting_str_round_trip() {
        for s in [
            PrivacySetting::GroupAdd,
            PrivacySetting::LastSeen,
            PrivacySetting::Online,
            PrivacySetting::Profile,
            PrivacySetting::Status,
            PrivacySetting::ReadReceipts,
            PrivacySetting::CallAdd,
            PrivacySetting::DisappearingMode,
            PrivacySetting::Messages,
            PrivacySetting::Defense,
            PrivacySetting::Stickers,
        ] {
            assert_eq!(PrivacySetting::from_str(s.as_str()), Some(s));
        }
        assert!(PrivacySetting::from_str("nonexistent").is_none());
    }

    // -----------------------------------------------------------------------
    // Helpers for the response-shape tests below.
    // -----------------------------------------------------------------------

    fn cat(name: &str, value: &str) -> Node {
        let mut a = Attrs::new();
        a.insert("name".into(), Value::String(name.into()));
        a.insert("value".into(), Value::String(value.into()));
        Node::new("category", a, None)
    }

    /// Construct the canonical `<privacy>` payload with all eight categories
    /// the server emits for a fully-configured account, exactly as documented
    /// at the top of `privacy_settings.rs`.
    fn full_privacy_node() -> Node {
        Node::new(
            "privacy",
            Attrs::new(),
            Some(Value::Nodes(vec![
                cat("last", "all"),
                cat("online", "match_last_seen"),
                cat("profile", "all"),
                cat("status", "contacts"),
                cat("readreceipts", "all"),
                cat("groupadd", "all"),
                cat("calladd", "all"),
                cat("disappearing", "86400"),
            ])),
        )
    }

    /// Wrap a `<privacy>` node in an `<iq type="result">` envelope so we can
    /// exercise both code paths in `parse_privacy_settings`.
    fn iq_wrapping(privacy: Node) -> Node {
        let mut a = Attrs::new();
        a.insert("type".into(), Value::String("result".into()));
        Node::new("iq", a, Some(Value::Nodes(vec![privacy])))
    }

    #[test]
    fn parse_full_privacy_response_extracts_all_eight_categories() {
        let settings = parse_privacy_settings(&full_privacy_node());
        assert_eq!(settings.last_seen, "all");
        assert_eq!(settings.online, "match_last_seen");
        assert_eq!(settings.profile, "all");
        assert_eq!(settings.status, "contacts");
        assert_eq!(settings.read_receipts, "all");
        assert_eq!(settings.group_add, "all");
        assert_eq!(settings.call_add, "all");
        // The server uses `disappearing` (no underscore) as the category name
        // in the IQ response, but the field stores the raw timer in seconds.
        assert_eq!(settings.default_disappearing_mode, "86400");
    }

    #[test]
    fn parse_handles_iq_wrapper_correctly() {
        // Path 1: caller hands us the bare <privacy> node.
        let bare = parse_privacy_settings(&full_privacy_node());

        // Path 2: caller hands us the <iq> envelope; the parser must descend
        // to the inner <privacy>. The two should be byte-identical.
        let wrapped = parse_privacy_settings(&iq_wrapping(full_privacy_node()));

        assert_eq!(bare, wrapped);
        assert_eq!(wrapped.last_seen, "all");
        assert_eq!(wrapped.default_disappearing_mode, "86400");

        // Sanity: a stray node with no <privacy> child falls back to the
        // defaults rather than panicking.
        let empty_iq = Node::new("iq", Attrs::new(), None);
        let defaults = parse_privacy_settings(&empty_iq);
        assert_eq!(defaults.last_seen, "all");
        assert_eq!(defaults.default_disappearing_mode, "0");
    }

    #[test]
    fn parse_unknown_category_does_not_panic_or_error() {
        // Mix of known and unknown names + a category with a missing `name`
        // attribute. None of these may panic; known categories must still be
        // applied and unknowns silently dropped (warn-logged in production).
        let mut name_missing = Attrs::new();
        name_missing.insert("value".into(), Value::String("whatever".into()));
        let nameless = Node::new("category", name_missing, None);

        let privacy = Node::new(
            "privacy",
            Attrs::new(),
            Some(Value::Nodes(vec![
                cat("last", "contacts"),
                cat("totally_new_thing", "yes"),
                cat("future_axis", "future_value"),
                nameless,
                // Non-category sibling is also tolerated.
                Node::new("noise", Attrs::new(), None),
            ])),
        );

        let settings = parse_privacy_settings(&privacy);
        // Known category was applied.
        assert_eq!(settings.last_seen, "contacts");
        // Unknown categories did not corrupt other fields — defaults remain.
        assert_eq!(settings.profile, "all");
        assert_eq!(settings.online, "all");
        assert_eq!(settings.default_disappearing_mode, "0");
    }

    #[test]
    fn parse_disappearing_alias_maps_to_disappearing_mode() {
        // The IQ-response wire form is `disappearing`; the change-notification
        // form is `disappearing_mode`. Both must land in the same field.
        let with_alias = Node::new(
            "privacy",
            Attrs::new(),
            Some(Value::Nodes(vec![cat("disappearing", "604800")])),
        );
        let with_full = Node::new(
            "privacy",
            Attrs::new(),
            Some(Value::Nodes(vec![cat("disappearing_mode", "604800")])),
        );
        assert_eq!(
            parse_privacy_settings(&with_alias).default_disappearing_mode,
            "604800",
        );
        assert_eq!(
            parse_privacy_settings(&with_full).default_disappearing_mode,
            "604800",
        );
    }
}
