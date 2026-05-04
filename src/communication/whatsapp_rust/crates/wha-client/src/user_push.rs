//! User-info / push-name / privacy / push-token query helpers.
//!
//! Ported (selectively) from `whatsmeow/user.go`, `whatsmeow/push.go` and
//! `whatsmeow/privacysettings.go`. Out of scope for this module: full contact
//! list sync (lives in appstate), profile-picture upload, status broadcast.

use std::collections::HashMap;

use base64::Engine;

use wha_binary::{Attrs, Node, Value};
use wha_types::jid::server;
use wha_types::Jid;

use crate::client::Client;
use crate::error::ClientError;
use crate::request::{InfoQuery, IqType};

// -- public types -------------------------------------------------------------

/// Mirrors `types.UserInfo` from `whatsmeow/types/user.go` minus the protobuf
/// `VerifiedName` which lives in `wha-proto` and is out of scope here.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UserInfo {
    pub status: String,
    pub picture_id: String,
    pub devices: Vec<Jid>,
    /// Linked-Identity JID, if the server returned one.
    pub lid: Option<Jid>,
    /// Raw verified-name certificate bytes (caller decodes with the proto crate).
    pub verified_name_cert: Option<Vec<u8>>,
}

/// Mirrors `types.ProfilePictureInfo` from upstream — the metadata returned
/// from the `<iq xmlns="w:profile:picture">` query.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProfilePictureInfo {
    /// Server-assigned id; pass it as `existing_id` to skip downloads when
    /// the picture has not changed.
    pub id: String,
    /// Direct CDN URL to the picture bytes.
    pub url: String,
    /// `image` for full-size, `preview` for low-res.
    pub kind: String,
    /// CDN direct path (alternative to the absolute `url`).
    pub direct_path: String,
    /// Optional SHA-256 hash of the picture bytes (base64-decoded into raw).
    pub hash: Vec<u8>,
}

/// Mirrors `types.PrivacySetting` — a single value applied to a category.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrivacySettingValue {
    Undefined,
    All,
    Contacts,
    ContactAllowlist,
    ContactBlacklist,
    MatchLastSeen,
    Known,
    None,
    OnStandard,
    Off,
    /// Catch-all for forward compatibility.
    Other(String),
}

impl PrivacySettingValue {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Undefined => "",
            Self::All => "all",
            Self::Contacts => "contacts",
            Self::ContactAllowlist => "contact_allowlist",
            Self::ContactBlacklist => "contact_blacklist",
            Self::MatchLastSeen => "match_last_seen",
            Self::Known => "known",
            Self::None => "none",
            Self::OnStandard => "on_standard",
            Self::Off => "off",
            Self::Other(s) => s.as_str(),
        }
    }

    pub fn from_wire(s: &str) -> Self {
        match s {
            "" => Self::Undefined,
            "all" => Self::All,
            "contacts" => Self::Contacts,
            "contact_allowlist" => Self::ContactAllowlist,
            "contact_blacklist" => Self::ContactBlacklist,
            "match_last_seen" => Self::MatchLastSeen,
            "known" => Self::Known,
            "none" => Self::None,
            "on_standard" => Self::OnStandard,
            "off" => Self::Off,
            other => Self::Other(other.to_owned()),
        }
    }
}

impl Default for PrivacySettingValue {
    fn default() -> Self {
        Self::Undefined
    }
}

/// Mirrors `types.PrivacySettingType` — the named privacy category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PrivacySettingName {
    GroupAdd,
    LastSeen,
    Status,
    Profile,
    ReadReceipts,
    Online,
    CallAdd,
    Messages,
    Defense,
    Stickers,
}

impl PrivacySettingName {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::GroupAdd => "groupadd",
            Self::LastSeen => "last",
            Self::Status => "status",
            Self::Profile => "profile",
            Self::ReadReceipts => "readreceipts",
            Self::Online => "online",
            Self::CallAdd => "calladd",
            Self::Messages => "messages",
            Self::Defense => "defense",
            Self::Stickers => "stickers",
        }
    }

    pub fn from_wire(s: &str) -> Option<Self> {
        Some(match s {
            "groupadd" => Self::GroupAdd,
            "last" => Self::LastSeen,
            "status" => Self::Status,
            "profile" => Self::Profile,
            "readreceipts" => Self::ReadReceipts,
            "online" => Self::Online,
            "calladd" => Self::CallAdd,
            "messages" => Self::Messages,
            "defense" => Self::Defense,
            "stickers" => Self::Stickers,
            _ => return None,
        })
    }
}

/// Mirrors `types.PrivacySettings`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PrivacySettings {
    pub group_add: PrivacySettingValue,
    pub last_seen: PrivacySettingValue,
    pub status: PrivacySettingValue,
    pub profile: PrivacySettingValue,
    pub read_receipts: PrivacySettingValue,
    pub call_add: PrivacySettingValue,
    pub online: PrivacySettingValue,
    pub messages: PrivacySettingValue,
    pub defense: PrivacySettingValue,
    pub stickers: PrivacySettingValue,
}

impl PrivacySettings {
    fn set(&mut self, name: PrivacySettingName, value: PrivacySettingValue) {
        match name {
            PrivacySettingName::GroupAdd => self.group_add = value,
            PrivacySettingName::LastSeen => self.last_seen = value,
            PrivacySettingName::Status => self.status = value,
            PrivacySettingName::Profile => self.profile = value,
            PrivacySettingName::ReadReceipts => self.read_receipts = value,
            PrivacySettingName::CallAdd => self.call_add = value,
            PrivacySettingName::Online => self.online = value,
            PrivacySettingName::Messages => self.messages = value,
            PrivacySettingName::Defense => self.defense = value,
            PrivacySettingName::Stickers => self.stickers = value,
        }
    }
}

/// Push-token registration — mirrors `whatsmeow/push.go` `PushConfig` impls.
#[derive(Debug, Clone)]
pub enum PushToken {
    /// Firebase Cloud Messaging (Android).
    Fcm { token: String },
    /// Apple Push Notification service.
    Apns {
        token: String,
        voip_token: Option<String>,
        msg_id_enc_key: [u8; 32],
    },
    /// Web push (browser endpoints).
    Web {
        endpoint: String,
        auth: Vec<u8>,
        p256dh: Vec<u8>,
    },
}

impl PushToken {
    fn into_attrs(self) -> Attrs {
        let mut a = Attrs::new();
        match self {
            PushToken::Fcm { token } => {
                a.insert("id".into(), Value::String(token));
                a.insert("num_acc".into(), Value::String("1".into()));
                a.insert("platform".into(), Value::String("gcm".into()));
            }
            PushToken::Apns { token, voip_token, msg_id_enc_key } => {
                let pkey = base64::engine::general_purpose::URL_SAFE_NO_PAD
                    .encode(msg_id_enc_key);
                a.insert("id".into(), Value::String(token));
                a.insert("platform".into(), Value::String("apple".into()));
                a.insert("version".into(), Value::String("2".into()));
                a.insert("reg_push".into(), Value::String("1".into()));
                a.insert("preview".into(), Value::String("1".into()));
                a.insert("pkey".into(), Value::String(pkey));
                a.insert("background_location".into(), Value::String("1".into()));
                a.insert("call".into(), Value::String("Opening.m4r".into()));
                a.insert("default".into(), Value::String("note.m4r".into()));
                a.insert("groups".into(), Value::String("note.m4r".into()));
                a.insert("lg".into(), Value::String("en".into()));
                a.insert("lc".into(), Value::String("US".into()));
                a.insert("nse_call".into(), Value::String("0".into()));
                a.insert("nse_ver".into(), Value::String("2".into()));
                a.insert("nse_read".into(), Value::String("0".into()));
                a.insert("voip_payload_type".into(), Value::String("2".into()));
                if let Some(voip) = voip_token {
                    if !voip.is_empty() {
                        a.insert("voip".into(), Value::String(voip));
                    }
                }
            }
            PushToken::Web { endpoint, auth, p256dh } => {
                let auth_b64 = base64::engine::general_purpose::STANDARD.encode(auth);
                let p256_b64 = base64::engine::general_purpose::STANDARD.encode(p256dh);
                a.insert("platform".into(), Value::String("web".into()));
                a.insert("endpoint".into(), Value::String(endpoint));
                a.insert("auth".into(), Value::String(auth_b64));
                a.insert("p256dh".into(), Value::String(p256_b64));
            }
        }
        a
    }
}

// -- node-builder helpers (separated so they can be unit-tested) --------------

/// Build the `<iq>` body for `GetUserInfo`. Mirrors the usync emitted by
/// `whatsmeow.usync(jids, "full", "background", …)` in `user.go`.
pub(crate) fn build_user_info_iq(
    jids: &[Jid],
    sid: &str,
) -> InfoQuery {
    // Inner <query> children — mirrors GetUserInfo's query list verbatim.
    let business_inner = Node::new(
        "business",
        Attrs::new(),
        Some(Value::Nodes(vec![Node::tag_only("verified_name")])),
    );
    let status = Node::tag_only("status");
    let picture = Node::tag_only("picture");
    let mut devices_attrs = Attrs::new();
    devices_attrs.insert("version".into(), Value::String("2".into()));
    let devices = Node::new("devices", devices_attrs, None);
    let lid_node = Node::tag_only("lid");
    let query =
        Node::new("query", Attrs::new(), Some(Value::Nodes(vec![business_inner, status, picture, devices, lid_node])));

    // <list> children — one <user jid="..."/> per input.
    let user_list: Vec<Node> = jids
        .iter()
        .map(|j| {
            let jid = j.to_non_ad();
            let mut attrs = Attrs::new();
            attrs.insert("jid".into(), Value::Jid(jid));
            Node::new("user", attrs, None)
        })
        .collect();
    let list = Node::new("list", Attrs::new(), Some(Value::Nodes(user_list)));

    let mut usync_attrs = Attrs::new();
    usync_attrs.insert("sid".into(), Value::String(sid.to_owned()));
    usync_attrs.insert("mode".into(), Value::String("full".into()));
    usync_attrs.insert("last".into(), Value::String("true".into()));
    usync_attrs.insert("index".into(), Value::String("0".into()));
    usync_attrs.insert("context".into(), Value::String("background".into()));
    let usync = Node::new("usync", usync_attrs, Some(Value::Nodes(vec![query, list])));

    InfoQuery::new("usync", IqType::Get)
        .to(Jid::new("", server::DEFAULT_USER))
        .content(Value::Nodes(vec![usync]))
}

/// Walk a usync `<iq>` response and pull out the `<usync><list>` element.
fn usync_list<'a>(resp: &'a Node) -> Result<&'a Node, ClientError> {
    resp.child_by_tag(&["usync", "list"])
        .ok_or_else(|| ClientError::Malformed("usync response missing <usync><list>".into()))
}

/// Mirrors `parseDeviceList` in `whatsmeow/user.go`. The input is the inner
/// `<devices>` node returned by usync; we walk its `<device-list>` child.
fn parse_device_list(user: &Jid, devices_node: Option<&Node>) -> Vec<Jid> {
    let Some(node) = devices_node else { return Vec::new() };
    if node.tag != "devices" {
        return Vec::new();
    }
    let Some(list) = node.children().iter().find(|c| c.tag == "device-list") else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for child in list.children() {
        if child.tag != "device" {
            continue;
        }
        let mut ag = child.attr_getter();
        let id = ag.i64("id");
        if !ag.ok() {
            continue;
        }
        let is_hosted = child.attr_getter().optional_bool("is_hosted");
        let mut copy = user.clone();
        copy.device = id as u16;
        if is_hosted {
            copy.server = if user.server == server::HIDDEN_USER {
                server::HOSTED_LID.to_owned()
            } else {
                server::HOSTED.to_owned()
            };
        }
        out.push(copy);
    }
    out
}

/// Parse the `<user …>` children of a usync `<list>` into the public
/// `HashMap<Jid, UserInfo>` we hand back from `get_user_info`.
pub(crate) fn parse_user_info_list(list: &Node) -> Result<HashMap<Jid, UserInfo>, ClientError> {
    let mut out = HashMap::new();
    for child in list.children() {
        if child.tag != "user" {
            continue;
        }
        let jid = match child.get_attr_jid("jid") {
            Some(j) => j.clone(),
            None => {
                return Err(ClientError::Malformed(
                    "user node missing required `jid` attr".into(),
                ));
            }
        };
        let mut info = UserInfo::default();

        // <status> contains free-form text content.
        if let Some(status) = child.children().iter().find(|c| c.tag == "status") {
            if let Some(b) = status.content.as_bytes() {
                info.status = String::from_utf8_lossy(b).into_owned();
            } else if let Some(s) = status.content.as_str() {
                info.status = s.to_owned();
            }
        }

        // <picture id="…"/>
        if let Some(pic) = child.children().iter().find(|c| c.tag == "picture") {
            info.picture_id = pic.get_attr_str("id").unwrap_or("").to_owned();
        }

        // <devices version="2"><device-list>…
        let devices_node = child.children().iter().find(|c| c.tag == "devices");
        info.devices = parse_device_list(&jid, devices_node);

        // <lid val="...@lid"/>
        if let Some(lid_tag) = child.children().iter().find(|c| c.tag == "lid") {
            if let Some(j) = lid_tag.get_attr_jid("val") {
                if !j.is_empty() {
                    info.lid = Some(j.clone());
                }
            }
        }

        // <business><verified_name>…cert bytes…</verified_name></business>
        if let Some(biz) = child.children().iter().find(|c| c.tag == "business") {
            if let Some(vn) = biz.children().iter().find(|c| c.tag == "verified_name") {
                if let Some(b) = vn.content.as_bytes() {
                    info.verified_name_cert = Some(b.to_vec());
                }
            }
        }

        out.insert(jid, info);
    }
    Ok(out)
}

/// Build the `<presence type="available" name="…"/>` node sent by
/// `set_push_name`. WhatsApp has no dedicated "set push name" IQ — the
/// pushname is communicated server-side via the `name` attribute on
/// `<presence>` (see `whatsmeow/presence.go::SendPresence`).
pub(crate) fn build_presence_with_name(name: &str) -> Node {
    let mut attrs = Attrs::new();
    attrs.insert("type".into(), Value::String("available".into()));
    attrs.insert("name".into(), Value::String(name.to_owned()));
    Node::new("presence", attrs, None)
}

/// Build the `<iq xmlns="privacy" type="get"…><privacy/></iq>` node.
pub(crate) fn build_get_privacy_iq() -> InfoQuery {
    InfoQuery::new("privacy", IqType::Get)
        .to(Jid::new("", server::DEFAULT_USER))
        .content(Value::Nodes(vec![Node::tag_only("privacy")]))
}

/// Build the `<iq xmlns="privacy" type="set"…><privacy><category …/></privacy></iq>` node.
pub(crate) fn build_set_privacy_iq(name: PrivacySettingName, value: &PrivacySettingValue) -> InfoQuery {
    let mut cat_attrs = Attrs::new();
    cat_attrs.insert("name".into(), Value::String(name.as_str().into()));
    cat_attrs.insert("value".into(), Value::String(value.as_str().into()));
    let category = Node::new("category", cat_attrs, None);
    let privacy = Node::new("privacy", Attrs::new(), Some(Value::Nodes(vec![category])));
    InfoQuery::new("privacy", IqType::Set)
        .to(Jid::new("", server::DEFAULT_USER))
        .content(Value::Nodes(vec![privacy]))
}

/// Mirrors `parsePrivacySettings` in `whatsmeow/privacysettings.go`.
pub(crate) fn parse_privacy_settings(privacy_node: &Node) -> PrivacySettings {
    let mut out = PrivacySettings::default();
    for child in privacy_node.children() {
        if child.tag != "category" {
            continue;
        }
        let name = child.get_attr_str("name").unwrap_or("");
        let value = child.get_attr_str("value").unwrap_or("");
        if let Some(name) = PrivacySettingName::from_wire(name) {
            out.set(name, PrivacySettingValue::from_wire(value));
        }
    }
    out
}

/// Build the push-registration `<iq xmlns="urn:xmpp:whatsapp:push" type="set">…</iq>`.
pub(crate) fn build_register_push_iq(token: PushToken) -> InfoQuery {
    let config = Node::new("config", token.into_attrs(), None);
    InfoQuery::new("urn:xmpp:whatsapp:push", IqType::Set)
        .to(Jid::new("", server::DEFAULT_USER))
        .content(Value::Nodes(vec![config]))
}

/// Build the `<iq xmlns="status" type="set">` body emitted by
/// `SetStatusMessage` upstream.
///
/// Wire shape:
/// ```xml
/// <iq id="…" xmlns="status" type="set" to="s.whatsapp.net">
///   <status>my status text</status>
/// </iq>
/// ```
pub(crate) fn build_set_status_iq(message: &str) -> InfoQuery {
    let status = Node::new(
        "status",
        Attrs::new(),
        Some(Value::Bytes(message.as_bytes().to_vec())),
    );
    InfoQuery::new("status", IqType::Set)
        .to(Jid::new("", server::DEFAULT_USER))
        .content(Value::Nodes(vec![status]))
}

/// Build the `<iq xmlns="w:profile:picture" type="get">` body emitted by
/// `GetProfilePictureInfo` upstream — the simple non-community, non-preview
/// case (`type="image"`, `query="url"`).
///
/// Wire shape:
/// ```xml
/// <iq id="…" xmlns="w:profile:picture" type="get" to="s.whatsapp.net" target="<jid>">
///   <picture type="image" query="url"/>
/// </iq>
/// ```
pub(crate) fn build_get_profile_picture_iq(target: &Jid) -> InfoQuery {
    let mut attrs = Attrs::new();
    attrs.insert("query".into(), Value::String("url".into()));
    attrs.insert("type".into(), Value::String("image".into()));
    let picture = Node::new("picture", attrs, None);
    let mut q = InfoQuery::new("w:profile:picture", IqType::Get)
        .to(Jid::new("", server::DEFAULT_USER))
        .content(Value::Nodes(vec![picture]));
    q.target = Some(target.clone());
    q
}

/// Parse a `<picture>` node returned by the profile-picture IQ. Returns
/// `None` when the server signals the picture is unset (`status="204"`) or
/// unchanged (`status="304"`); otherwise decodes the metadata.
pub(crate) fn parse_profile_picture(picture_node: &Node) -> Option<ProfilePictureInfo> {
    if picture_node.tag != "picture" {
        return None;
    }
    if let Some(s) = picture_node.get_attr_str("status") {
        if s == "304" || s == "204" {
            return None;
        }
    }
    let id = picture_node.get_attr_str("id").unwrap_or("").to_owned();
    let url = picture_node.get_attr_str("url").unwrap_or("").to_owned();
    let kind = picture_node.get_attr_str("type").unwrap_or("").to_owned();
    let direct_path = picture_node
        .get_attr_str("direct_path")
        .unwrap_or("")
        .to_owned();
    let hash = picture_node
        .get_attr_str("hash")
        .and_then(|s| {
            base64::engine::general_purpose::STANDARD
                .decode(s)
                .ok()
        })
        .unwrap_or_default();
    Some(ProfilePictureInfo {
        id,
        url,
        kind,
        direct_path,
        hash,
    })
}

// -- Client impl --------------------------------------------------------------

impl Client {
    /// Get basic user info (status, profile-pic id, devices, LID,
    /// verified-name cert bytes) for the given JIDs. Mirrors
    /// `whatsmeow.GetUserInfo`.
    ///
    /// Returns a map keyed on the JID echoed back by the server. Bots are
    /// returned with an empty `devices` list (their usync child has no
    /// `<devices>`); the server may omit users it can't resolve.
    pub async fn get_user_info(
        &self,
        jids: &[Jid],
    ) -> Result<HashMap<Jid, UserInfo>, ClientError> {
        let sid = self.generate_request_id();
        let query = build_user_info_iq(jids, &sid);
        let resp = self.send_iq(query).await?;
        let list = usync_list(&resp)?;
        parse_user_info_list(list)
    }

    /// Push the given push name to the server. WhatsApp uses
    /// `<presence type="available" name="…"/>` for this — there is no
    /// dedicated IQ. Mutates `Device.push_name` so subsequent
    /// `Client::send_presence` calls echo the same name (mirrors
    /// `cli.Store.PushName = name` in `whatsmeow/presence.go`).
    pub async fn set_push_name(&mut self, name: &str) -> Result<(), ClientError> {
        if name.is_empty() {
            return Err(ClientError::Malformed("push name must be non-empty".into()));
        }
        let node = build_presence_with_name(name);
        self.send_node(&node).await?;
        self.device.push_name = name.to_owned();
        Ok(())
    }

    /// Update the user's status / about text — mirrors
    /// `Client.SetStatusMessage` in `whatsmeow/user.go`.
    pub async fn set_status_message(&self, status: &str) -> Result<(), ClientError> {
        let _ = self.send_iq(build_set_status_iq(status)).await?;
        Ok(())
    }

    /// Look up the URL for a user / group profile picture — mirrors the
    /// non-community, non-preview branch of `Client.GetProfilePictureInfo`
    /// in `whatsmeow/user.go`. Returns `None` if the picture is unset
    /// (`status=204`) or unchanged (`status=304`).
    pub async fn get_profile_picture(
        &self,
        jid: &Jid,
    ) -> Result<Option<ProfilePictureInfo>, ClientError> {
        let resp = self.send_iq(build_get_profile_picture_iq(jid)).await?;
        let Some(picture) = resp.children().iter().find(|c| c.tag == "picture") else {
            return Ok(None);
        };
        Ok(parse_profile_picture(picture))
    }

    /// Look up the push name we know about for `jid`. For our own JID this
    /// returns the locally cached `Device.push_name`; for others we don't run
    /// a usync (whatsmeow doesn't expose that either) — the caller must rely
    /// on the appstate / message-event push-name updates.
    ///
    /// Returns an empty string if no push name is known.
    pub async fn get_push_name(&self, jid: &Jid) -> Result<String, ClientError> {
        if let Some(self_jid) = self.device.id.as_ref() {
            if self_jid.to_non_ad() == jid.to_non_ad() {
                return Ok(self.device.push_name.clone());
            }
        }
        Ok(String::new())
    }

    /// Fetch the user's privacy settings. Mirrors
    /// `whatsmeow.TryFetchPrivacySettings`. Doesn't cache — this port omits
    /// the `privacySettingsCache` for now.
    pub async fn get_privacy_settings(&self) -> Result<PrivacySettings, ClientError> {
        let resp = self.send_iq(build_get_privacy_iq()).await?;
        let privacy_node = resp
            .children()
            .iter()
            .find(|c| c.tag == "privacy")
            .ok_or_else(|| {
                ClientError::Malformed(
                    "privacy settings response missing <privacy> child".into(),
                )
            })?;
        Ok(parse_privacy_settings(privacy_node))
    }

    /// Update one privacy category. Mirrors `whatsmeow.SetPrivacySetting`.
    pub async fn set_privacy_setting(
        &self,
        name: PrivacySettingName,
        value: PrivacySettingValue,
    ) -> Result<(), ClientError> {
        let _ = self.send_iq(build_set_privacy_iq(name, &value)).await?;
        Ok(())
    }

    /// Register a push token with the server. Mirrors
    /// `whatsmeow.RegisterForPushNotifications`.
    pub async fn register_push(&self, token: PushToken) -> Result<(), ClientError> {
        let _ = self.send_iq(build_register_push_iq(token)).await?;
        Ok(())
    }
}

// -- free-function mirrors ----------------------------------------------------
//
// Mirroring the upstream API surface, callers can use either the methods on
// `Client` or these free helpers. Required by the public port spec.

/// Push the given push name to the server and persist it locally on
/// `Device.push_name`. Mirrors `whatsmeow.SendPresence` upstream — WA uses
/// `<presence type="available" name="…"/>` to communicate the push name.
pub async fn set_push_name(client: &mut Client, name: &str) -> Result<(), ClientError> {
    client.set_push_name(name).await
}

/// Update the user's status text. Mirrors `whatsmeow.SetStatusMessage`.
pub async fn set_status_message(client: &Client, status: &str) -> Result<(), ClientError> {
    client.set_status_message(status).await
}

/// Fetch usync info for one or more users — status, picture id, devices,
/// LID, verified-name cert. Mirrors `whatsmeow.GetUserInfo`.
pub async fn get_user_info(
    client: &Client,
    jids: &[Jid],
) -> Result<HashMap<Jid, UserInfo>, ClientError> {
    client.get_user_info(jids).await
}

/// Fetch profile-picture metadata for `jid`. Mirrors
/// `whatsmeow.GetProfilePictureInfo` with default options (full image, no
/// existing-id short-circuit).
pub async fn get_profile_picture(
    client: &Client,
    jid: &Jid,
) -> Result<Option<ProfilePictureInfo>, ClientError> {
    client.get_profile_picture(jid).await
}

/// Register a device for FCM push notifications, with an optional VoIP
/// (APNs) token alongside. Mirrors
/// `_upstream/whatsmeow/push.go::RegisterForPushNotifications` +
/// `SendFBPushConfig` shape: a single `<config>` child of an
/// `<iq xmlns="urn:xmpp:whatsapp:push" type="set">`.
///
/// When `voip_token` is empty the IQ uses a plain FCM (`PushToken::Fcm`)
/// configuration. When `voip_token` is non-empty we ship an APNs config
/// — which is the layout upstream's mobile clients send — using
/// `fcm_token` as the device id. This isn't a perfect 1:1 with upstream
/// (which uses a struct with both fields together), but the wire output
/// matches what the server expects in either case.
pub async fn register_for_push_notifications(
    client: &Client,
    fcm_token: &str,
    voip_token: &str,
) -> Result<(), ClientError> {
    if fcm_token.is_empty() {
        return Err(ClientError::Malformed("fcm_token must be non-empty".into()));
    }
    let token = if voip_token.is_empty() {
        PushToken::Fcm {
            token: fcm_token.to_owned(),
        }
    } else {
        // Random msg-id encryption key (mirrors upstream's
        // `random.Bytes(32)` in `APNsPushConfig.GetPushConfigAttrs`).
        let mut msg_id_enc_key = [0u8; 32];
        use rand::RngCore;
        rand::thread_rng().fill_bytes(&mut msg_id_enc_key);
        PushToken::Apns {
            token: fcm_token.to_owned(),
            voip_token: Some(voip_token.to_owned()),
            msg_id_enc_key,
        }
    };
    client.register_push(token).await
}

// -- tests --------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn jid(s: &str) -> Jid {
        s.parse().expect("parse jid")
    }

    #[test]
    fn build_user_info_iq_shape() {
        let jids = vec![jid("1234@s.whatsapp.net"), jid("5678@s.whatsapp.net")];
        let q = build_user_info_iq(&jids, "sid-1");
        let node = q.into_node("iq-id-1".into());

        assert_eq!(node.tag, "iq");
        assert_eq!(node.get_attr_str("id"), Some("iq-id-1"));
        assert_eq!(node.get_attr_str("xmlns"), Some("usync"));
        assert_eq!(node.get_attr_str("type"), Some("get"));
        assert_eq!(node.get_attr_jid("to").unwrap().server, server::DEFAULT_USER);

        let usync = node
            .children()
            .iter()
            .find(|c| c.tag == "usync")
            .expect("<usync> child present");
        assert_eq!(usync.get_attr_str("sid"), Some("sid-1"));
        assert_eq!(usync.get_attr_str("mode"), Some("full"));
        assert_eq!(usync.get_attr_str("context"), Some("background"));

        let list = usync
            .children()
            .iter()
            .find(|c| c.tag == "list")
            .expect("<list> child present");
        let users: Vec<&Node> = list.children().iter().filter(|c| c.tag == "user").collect();
        assert_eq!(users.len(), 2);
        assert_eq!(users[0].get_attr_jid("jid").unwrap().user, "1234");
        assert_eq!(users[1].get_attr_jid("jid").unwrap().user, "5678");

        let query = usync
            .children()
            .iter()
            .find(|c| c.tag == "query")
            .expect("<query> child present");
        // The five expected sub-queries: business, status, picture, devices, lid.
        let tags: Vec<&str> = query.children().iter().map(|c| c.tag.as_str()).collect();
        assert!(tags.contains(&"business"));
        assert!(tags.contains(&"status"));
        assert!(tags.contains(&"picture"));
        assert!(tags.contains(&"devices"));
        assert!(tags.contains(&"lid"));
    }

    #[test]
    fn parse_user_info_list_basic() {
        // Hand-build a synthetic <list> matching the shape of a real usync
        // response: one <user>, with status / picture / devices / lid children.
        let user_jid = jid("1234@s.whatsapp.net");
        let mut user_attrs = Attrs::new();
        user_attrs.insert("jid".into(), Value::Jid(user_jid.clone()));

        let status = Node::new(
            "status",
            Attrs::new(),
            Some(Value::Bytes(b"hello world".to_vec())),
        );

        let mut pic_attrs = Attrs::new();
        pic_attrs.insert("id".into(), Value::String("pic-id-99".into()));
        let picture = Node::new("picture", pic_attrs, None);

        let mut dev_attrs = Attrs::new();
        dev_attrs.insert("id".into(), Value::String("7".into()));
        let device_node = Node::new("device", dev_attrs, None);
        let dev_list = Node::new(
            "device-list",
            Attrs::new(),
            Some(Value::Nodes(vec![device_node])),
        );
        let mut devices_attrs = Attrs::new();
        devices_attrs.insert("version".into(), Value::String("2".into()));
        let devices = Node::new(
            "devices",
            devices_attrs,
            Some(Value::Nodes(vec![dev_list])),
        );

        let mut lid_attrs = Attrs::new();
        lid_attrs.insert("val".into(), Value::Jid(jid("999@lid")));
        let lid = Node::new("lid", lid_attrs, None);

        let user = Node::new(
            "user",
            user_attrs,
            Some(Value::Nodes(vec![status, picture, devices, lid])),
        );
        let list = Node::new("list", Attrs::new(), Some(Value::Nodes(vec![user])));

        let parsed = parse_user_info_list(&list).expect("parse ok");
        assert_eq!(parsed.len(), 1);
        let info = parsed.get(&user_jid).expect("user present");
        assert_eq!(info.status, "hello world");
        assert_eq!(info.picture_id, "pic-id-99");
        assert_eq!(info.devices.len(), 1);
        assert_eq!(info.devices[0].user, "1234");
        assert_eq!(info.devices[0].device, 7);
        assert_eq!(info.lid, Some(jid("999@lid")));
        assert!(info.verified_name_cert.is_none());
    }

    #[test]
    fn parse_user_info_list_rejects_missing_jid() {
        // <user> with no jid attr should yield an error.
        let user = Node::new("user", Attrs::new(), None);
        let list = Node::new("list", Attrs::new(), Some(Value::Nodes(vec![user])));
        let err = parse_user_info_list(&list).expect_err("should err");
        match err {
            ClientError::Malformed(_) => {}
            other => panic!("unexpected err: {other:?}"),
        }
    }

    #[test]
    fn parse_privacy_settings_extracts_categories() {
        // Build <privacy><category name="last" value="contacts"/>…
        fn cat(name: &str, value: &str) -> Node {
            let mut a = Attrs::new();
            a.insert("name".into(), Value::String(name.into()));
            a.insert("value".into(), Value::String(value.into()));
            Node::new("category", a, None)
        }
        let privacy = Node::new(
            "privacy",
            Attrs::new(),
            Some(Value::Nodes(vec![
                cat("last", "contacts"),
                cat("readreceipts", "all"),
                cat("online", "match_last_seen"),
                cat("groupadd", "none"),
                cat("calladd", "known"),
                cat("status", "contact_blacklist"),
                cat("profile", "all"),
                // unknown category is ignored gracefully:
                cat("future_unknown_thing", "all"),
            ])),
        );
        let s = parse_privacy_settings(&privacy);
        assert_eq!(s.last_seen, PrivacySettingValue::Contacts);
        assert_eq!(s.read_receipts, PrivacySettingValue::All);
        assert_eq!(s.online, PrivacySettingValue::MatchLastSeen);
        assert_eq!(s.group_add, PrivacySettingValue::None);
        assert_eq!(s.call_add, PrivacySettingValue::Known);
        assert_eq!(s.status, PrivacySettingValue::ContactBlacklist);
        assert_eq!(s.profile, PrivacySettingValue::All);
        // unset categories stay Undefined:
        assert_eq!(s.messages, PrivacySettingValue::Undefined);
        assert_eq!(s.defense, PrivacySettingValue::Undefined);
        assert_eq!(s.stickers, PrivacySettingValue::Undefined);
    }

    #[test]
    fn build_set_privacy_iq_shape() {
        let q = build_set_privacy_iq(
            PrivacySettingName::LastSeen,
            &PrivacySettingValue::Contacts,
        );
        let n = q.into_node("id-x".into());
        assert_eq!(n.get_attr_str("xmlns"), Some("privacy"));
        assert_eq!(n.get_attr_str("type"), Some("set"));
        let cat = n
            .child_by_tag(&["privacy", "category"])
            .expect("category present");
        assert_eq!(cat.get_attr_str("name"), Some("last"));
        assert_eq!(cat.get_attr_str("value"), Some("contacts"));
    }

    #[test]
    fn build_register_push_iq_fcm() {
        let q = build_register_push_iq(PushToken::Fcm {
            token: "abc-token".into(),
        });
        let n = q.into_node("id-y".into());
        assert_eq!(n.get_attr_str("xmlns"), Some("urn:xmpp:whatsapp:push"));
        assert_eq!(n.get_attr_str("type"), Some("set"));
        let cfg = n
            .children()
            .iter()
            .find(|c| c.tag == "config")
            .expect("config present");
        assert_eq!(cfg.get_attr_str("id"), Some("abc-token"));
        assert_eq!(cfg.get_attr_str("platform"), Some("gcm"));
    }

    #[test]
    fn build_presence_with_name_shape() {
        let n = build_presence_with_name("Alice");
        assert_eq!(n.tag, "presence");
        assert_eq!(n.get_attr_str("type"), Some("available"));
        assert_eq!(n.get_attr_str("name"), Some("Alice"));
    }

    // -------- set_status_message ------------------------------------------

    #[test]
    fn build_set_status_iq_shape() {
        let q = build_set_status_iq("hello world");
        let n = q.into_node("id-st".into());
        assert_eq!(n.tag, "iq");
        assert_eq!(n.get_attr_str("xmlns"), Some("status"));
        assert_eq!(n.get_attr_str("type"), Some("set"));
        let server = n.get_attr_jid("to").unwrap().server.clone();
        assert_eq!(server, server::DEFAULT_USER);
        let st = n
            .children()
            .iter()
            .find(|c| c.tag == "status")
            .expect("status child");
        match &st.content {
            Value::Bytes(b) => assert_eq!(b, b"hello world"),
            other => panic!("expected bytes content, got {other:?}"),
        }
    }

    // -------- get_profile_picture -----------------------------------------

    #[test]
    fn build_get_profile_picture_iq_shape() {
        let target = jid("4444@s.whatsapp.net");
        let q = build_get_profile_picture_iq(&target);
        let n = q.into_node("id-pp".into());
        assert_eq!(n.get_attr_str("xmlns"), Some("w:profile:picture"));
        assert_eq!(n.get_attr_str("type"), Some("get"));
        // `to` server-jid + `target` of the user we're querying.
        assert_eq!(n.get_attr_jid("to").unwrap().server, server::DEFAULT_USER);
        assert_eq!(n.get_attr_jid("target").unwrap().user, "4444");
        let pic = n
            .children()
            .iter()
            .find(|c| c.tag == "picture")
            .expect("picture child");
        assert_eq!(pic.get_attr_str("query"), Some("url"));
        assert_eq!(pic.get_attr_str("type"), Some("image"));
    }

    #[test]
    fn parse_profile_picture_extracts_metadata() {
        let mut a = Attrs::new();
        a.insert("id".into(), Value::String("pic-7".into()));
        a.insert("type".into(), Value::String("image".into()));
        a.insert(
            "url".into(),
            Value::String("https://cdn.example/abc".into()),
        );
        a.insert(
            "direct_path".into(),
            Value::String("/v/abc".into()),
        );
        // 4 bytes "test" → base64 dGVzdA== — decoded to b"test"
        a.insert("hash".into(), Value::String("dGVzdA==".into()));
        let picture = Node::new("picture", a, None);
        let info = parse_profile_picture(&picture).expect("some");
        assert_eq!(info.id, "pic-7");
        assert_eq!(info.kind, "image");
        assert_eq!(info.url, "https://cdn.example/abc");
        assert_eq!(info.direct_path, "/v/abc");
        assert_eq!(info.hash, b"test");
    }

    #[test]
    fn parse_profile_picture_status_204_means_unset() {
        let mut a = Attrs::new();
        a.insert("status".into(), Value::String("204".into()));
        let picture = Node::new("picture", a, None);
        assert!(parse_profile_picture(&picture).is_none());
    }

    #[test]
    fn privacy_setting_value_round_trip() {
        for s in [
            "all",
            "contacts",
            "contact_allowlist",
            "contact_blacklist",
            "match_last_seen",
            "known",
            "none",
            "on_standard",
            "off",
            "",
        ] {
            assert_eq!(PrivacySettingValue::from_wire(s).as_str(), s);
        }
        // unknown wire value preserves its text.
        let custom = PrivacySettingValue::from_wire("brand_new_value");
        assert_eq!(custom.as_str(), "brand_new_value");
    }

    /// `register_for_push_notifications` with an empty FCM token errors
    /// out with `Malformed` before any IO.
    #[tokio::test]
    async fn register_for_push_notifications_rejects_empty_fcm() {
        use std::sync::Arc;
        use wha_store::MemoryStore;
        let store = Arc::new(MemoryStore::new());
        let device = store.new_device();
        let (client, _evt) = Client::new(device);
        let r = register_for_push_notifications(&client, "", "voip").await;
        assert!(matches!(r, Err(ClientError::Malformed(_))));
    }

    /// `register_for_push_notifications` with FCM-only and FCM+VoIP both
    /// route to the same `<iq xmlns="urn:xmpp:whatsapp:push" type="set">`
    /// shape — verified via `build_register_push_iq` for each underlying
    /// `PushToken` variant. The pure FCM path emits `platform="gcm"`; the
    /// FCM+VoIP path emits `platform="apple"` + `voip="<token>"`.
    #[test]
    fn register_for_push_notifications_iq_shape() {
        // FCM only.
        let q1 = build_register_push_iq(PushToken::Fcm {
            token: "fcm-1".into(),
        });
        let n1 = q1.into_node("R1".into());
        let cfg1 = n1
            .children()
            .iter()
            .find(|c| c.tag == "config")
            .expect("config");
        assert_eq!(cfg1.get_attr_str("platform"), Some("gcm"));
        assert_eq!(cfg1.get_attr_str("id"), Some("fcm-1"));

        // FCM + VoIP (APNs).
        let q2 = build_register_push_iq(PushToken::Apns {
            token: "fcm-2".into(),
            voip_token: Some("voip-2".into()),
            msg_id_enc_key: [0u8; 32],
        });
        let n2 = q2.into_node("R2".into());
        assert_eq!(n2.get_attr_str("xmlns"), Some("urn:xmpp:whatsapp:push"));
        assert_eq!(n2.get_attr_str("type"), Some("set"));
        let cfg2 = n2
            .children()
            .iter()
            .find(|c| c.tag == "config")
            .expect("config");
        assert_eq!(cfg2.get_attr_str("platform"), Some("apple"));
        assert_eq!(cfg2.get_attr_str("id"), Some("fcm-2"));
        assert_eq!(cfg2.get_attr_str("voip"), Some("voip-2"));
    }
}
