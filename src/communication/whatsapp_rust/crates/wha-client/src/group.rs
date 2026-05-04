//! Group-chat management.
//!
//! Port of the IQ-based subset of `whatsmeow/group.go`. We support reading
//! group state (`get_group_info`), mutating it (`create_group`, `leave_group`,
//! `set_group_name`, `set_group_topic`, `update_group_participants`), and the
//! invite-link flow (`get_group_invite_link`, `join_group_with_link`).
//!
//! The wire format mirrors upstream:
//!
//! ```xml
//! <iq xmlns="w:g2" type="get" to="123-456@g.us" id="...">
//!   <query request="interactive"/>
//! </iq>
//! ```
//!
//! The reply is an `<iq type="result">` carrying a `<group>` element; see
//! [`parse_group_node`] for the field map.

use wha_binary::{Attrs, Node, Value};
use wha_types::Jid;

use crate::client::Client;
use crate::error::ClientError;
use crate::request::{InfoQuery, IqType};

/// Upstream's `InviteLinkPrefix`.
pub const INVITE_LINK_PREFIX: &str = "https://chat.whatsapp.com/";

/// What an `update_group_participants` call should do. Mirrors
/// `whatsmeow.ParticipantChange`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParticipantAction {
    Add,
    Remove,
    Promote,
    Demote,
}

/// Public alias matching the task spec — same enum, more idiomatic name when
/// a caller is thinking in terms of "the action to apply to the group", not
/// "the participant change". Both names refer to the same type so no
/// migration is needed for existing call sites.
pub type GroupAction = ParticipantAction;

impl ParticipantAction {
    pub fn as_str(&self) -> &'static str {
        match self {
            ParticipantAction::Add => "add",
            ParticipantAction::Remove => "remove",
            ParticipantAction::Promote => "promote",
            ParticipantAction::Demote => "demote",
        }
    }
}

/// One participant in the IQ-result of an `update_group_participants` call.
/// Mirrors the per-row payload upstream surfaces in the
/// `[]types.GroupParticipant` return value of `Client.UpdateGroupParticipants`,
/// but pared down to the three fields that the wire format actually carries
/// for each `<participant jid="…" error="N"><add_request>…</add_request></participant>`
/// child:
///
/// - `jid`     — the participant being acted on (always present).
/// - `status`  — the `error` attribute on the participant node. `0` means the
///   action succeeded for that participant; non-zero is the server's reason
///   code (e.g. `403` if the caller lacks permission, `409` if the participant
///   was already in the group).
/// - `content` — the optional `<add_request code="…">` body the server returns
///   when adding a non-contact participant; surfaced as a single string for
///   forwarding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupParticipantUpdate {
    pub jid: Jid,
    pub status: u16,
    pub content: Option<String>,
}

/// One member of a group, as returned in `<group>` responses. Mirrors the
/// fields we actually consume off `types.GroupParticipant`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupParticipant {
    pub jid: Jid,
    pub is_admin: bool,
    pub is_super_admin: bool,
    pub display_name: String,
}

/// Snapshot of a group as returned by `<group>` in an IQ result. Mirrors a
/// trimmed subset of upstream's `types.GroupInfo` — the fields the parser can
/// actually populate from the wire format we care about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupInfo {
    pub jid: Jid,
    pub owner: Jid,
    pub name: String,
    pub name_set_at: i64,
    pub name_set_by: Jid,
    pub topic: String,
    pub topic_id: String,
    pub topic_set_at: i64,
    pub topic_set_by: Jid,
    pub created_at: i64,
    pub participants: Vec<GroupParticipant>,
    pub is_announce: bool,
    pub is_locked: bool,
    pub is_ephemeral: bool,
    pub disappearing_timer: u32,
    pub participant_count: i64,
}

impl GroupInfo {
    fn empty() -> Self {
        GroupInfo {
            jid: Jid::default(),
            owner: Jid::default(),
            name: String::new(),
            name_set_at: 0,
            name_set_by: Jid::default(),
            topic: String::new(),
            topic_id: String::new(),
            topic_set_at: 0,
            topic_set_by: Jid::default(),
            created_at: 0,
            participants: Vec::new(),
            is_announce: false,
            is_locked: false,
            is_ephemeral: false,
            disappearing_timer: 0,
            participant_count: 0,
        }
    }
}

// -----------------------------------------------------------------------------
// helpers
// -----------------------------------------------------------------------------

fn group_server_jid() -> Jid {
    Jid::new("", wha_types::Server::GROUP)
}

/// Inspect an IQ response and turn `<iq type="error">…<error code="…" text="…"/>`
/// into a [`ClientError::Iq`]. Returns `None` for normal `result` responses.
fn iq_error_from_response(resp: &Node) -> Option<ClientError> {
    if resp.get_attr_str("type") != Some("error") {
        return None;
    }
    let err = resp.child_by_tag(&["error"])?;
    let code = err
        .get_attr_str("code")
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(0);
    let text = err.get_attr_str("text").unwrap_or("").to_owned();
    Some(ClientError::Iq { code, text })
}

/// Parse a `<participant jid="…" type="…"/>` child.
fn parse_participant(node: &Node) -> GroupParticipant {
    let mut ag = node.attr_getter();
    let pcp_type = ag.optional_string("type").unwrap_or("").to_owned();
    let jid = ag.optional_jid("jid").cloned().unwrap_or_default();
    let display_name = ag.optional_string("display_name").unwrap_or("").to_owned();
    GroupParticipant {
        jid,
        is_admin: pcp_type == "admin" || pcp_type == "superadmin",
        is_super_admin: pcp_type == "superadmin",
        display_name,
    }
}

/// Parse a `<group>` element into a [`GroupInfo`]. Mirrors upstream
/// `parseGroupNode`. The parser is lenient: missing optional attributes leave
/// the corresponding [`GroupInfo`] field at its default.
pub fn parse_group_node(group_node: &Node) -> Result<GroupInfo, ClientError> {
    if group_node.tag != "group" {
        return Err(ClientError::Malformed(format!(
            "expected <group>, got <{}>",
            group_node.tag
        )));
    }

    let mut info = GroupInfo::empty();
    let mut ag = group_node.attr_getter();

    // The `id` attribute on a `<group>` is the bare numeric — upgrade it to a
    // full JID against the group server.
    let id = ag.optional_string("id").unwrap_or("").to_owned();
    info.jid = Jid::new(id, wha_types::Server::GROUP);

    info.owner = ag.optional_jid("creator").cloned().unwrap_or_default();
    info.name = ag.optional_string("subject").unwrap_or("").to_owned();
    info.name_set_at = ag.optional_i64("s_t").unwrap_or(0);
    info.name_set_by = ag.optional_jid("s_o").cloned().unwrap_or_default();
    info.created_at = ag.optional_i64("creation").unwrap_or(0);
    info.participant_count = ag.optional_i64("size").unwrap_or(0);

    for child in group_node.children() {
        match child.tag.as_str() {
            "participant" => {
                info.participants.push(parse_participant(child));
            }
            "description" => {
                let mut cag = child.attr_getter();
                if let Some(body) = child.children().iter().find(|c| c.tag == "body") {
                    let topic_bytes = body
                        .content
                        .as_bytes()
                        .map(|b| b.to_vec())
                        .unwrap_or_default();
                    info.topic = String::from_utf8_lossy(&topic_bytes).into_owned();
                    info.topic_id = cag.optional_string("id").unwrap_or("").to_owned();
                    info.topic_set_at = cag.optional_i64("t").unwrap_or(0);
                    info.topic_set_by = cag
                        .optional_jid("participant")
                        .cloned()
                        .unwrap_or_default();
                }
            }
            "announcement" => info.is_announce = true,
            "locked" => info.is_locked = true,
            "ephemeral" => {
                info.is_ephemeral = true;
                let mut cag = child.attr_getter();
                info.disappearing_timer =
                    cag.optional_u64("expiration").unwrap_or(0) as u32;
            }
            _ => {}
        }
    }

    Ok(info)
}

/// Build the body of a `<create>` group IQ — the inner content the caller
/// hands to `send_iq`. Pulled out so we can pin the wire shape in a unit test.
pub fn build_create_group_iq(name: &str, participants: &[Jid], create_key: &str) -> InfoQuery {
    let mut child_nodes: Vec<Node> = Vec::with_capacity(participants.len());
    for jid in participants {
        let mut attrs = Attrs::new();
        attrs.insert("jid".into(), Value::Jid(jid.clone()));
        child_nodes.push(Node::new("participant", attrs, None));
    }

    let mut create_attrs = Attrs::new();
    create_attrs.insert("subject".into(), Value::String(name.to_owned()));
    create_attrs.insert("key".into(), Value::String(create_key.to_owned()));

    let create_node = Node::new("create", create_attrs, Some(Value::Nodes(child_nodes)));

    InfoQuery::new("w:g2", IqType::Set)
        .to(group_server_jid())
        .content(Value::Nodes(vec![create_node]))
}

/// Build the IQ for an `update_group_participants` action.
pub fn build_update_participants_iq(
    group: &Jid,
    participants: &[Jid],
    action: ParticipantAction,
) -> InfoQuery {
    let mut child_nodes: Vec<Node> = Vec::with_capacity(participants.len());
    for jid in participants {
        let mut attrs = Attrs::new();
        attrs.insert("jid".into(), Value::Jid(jid.clone()));
        child_nodes.push(Node::new("participant", attrs, None));
    }
    let action_node = Node::new(action.as_str(), Attrs::new(), Some(Value::Nodes(child_nodes)));

    InfoQuery::new("w:g2", IqType::Set)
        .to(group.clone())
        .content(Value::Nodes(vec![action_node]))
}

/// Build the IQ for `set_group_announce` — toggles whether only admins can
/// post in the group. Mirrors upstream `SetGroupAnnounce`: the inner tag is
/// either `<announcement/>` (announce-only on) or `<not_announcement/>` (off).
pub fn build_set_group_announce_iq(group: &Jid, announce_only: bool) -> InfoQuery {
    let tag = if announce_only { "announcement" } else { "not_announcement" };
    InfoQuery::new("w:g2", IqType::Set)
        .to(group.clone())
        .content(Value::Nodes(vec![Node::tag_only(tag)]))
}

/// Build the IQ for `set_group_locked` — when locked, only admins can edit
/// group metadata. Mirrors upstream `SetGroupLocked`: `<locked/>` vs
/// `<unlocked/>`.
pub fn build_set_group_locked_iq(group: &Jid, locked: bool) -> InfoQuery {
    let tag = if locked { "locked" } else { "unlocked" };
    InfoQuery::new("w:g2", IqType::Set)
        .to(group.clone())
        .content(Value::Nodes(vec![Node::tag_only(tag)]))
}

/// Build the IQ for `get_joined_groups`. Mirrors upstream `GetJoinedGroups`:
///
/// ```xml
/// <iq xmlns="w:g2" type="get" to="g.us">
///   <participating>
///     <participants/>
///     <description/>
///   </participating>
/// </iq>
/// ```
pub fn build_get_joined_groups_iq() -> InfoQuery {
    let participating = Node::new(
        "participating",
        Attrs::new(),
        Some(Value::Nodes(vec![
            Node::tag_only("participants"),
            Node::tag_only("description"),
        ])),
    );
    InfoQuery::new("w:g2", IqType::Get)
        .to(group_server_jid())
        .content(Value::Nodes(vec![participating]))
}

/// Parse the `<participant>` children of an `<add>`/`<remove>`/`<promote>`
/// /`<demote>` action response. Each child carries the attempted JID + an
/// `error` integer (`0` = success) and optional `<add_request code="…"/>`
/// body the server returns when the operation needed an explicit invite.
pub fn parse_participant_updates(action_node: &Node) -> Vec<GroupParticipantUpdate> {
    let mut out = Vec::new();
    for child in action_node.children_by_tag("participant") {
        let jid = child.get_attr_jid("jid").cloned().unwrap_or_default();
        let status = child
            .get_attr_str("error")
            .and_then(|s| s.parse::<u16>().ok())
            .unwrap_or(0);
        let content = child
            .child_by_tag(&["add_request"])
            .and_then(|n| n.get_attr_str("code"))
            .map(|s| s.to_owned());
        out.push(GroupParticipantUpdate {
            jid,
            status,
            content,
        });
    }
    out
}

fn build_get_group_info_iq(group: &Jid) -> InfoQuery {
    let mut attrs = Attrs::new();
    attrs.insert("request".into(), Value::String("interactive".into()));
    let query = Node::new("query", attrs, None);
    InfoQuery::new("w:g2", IqType::Get)
        .to(group.clone())
        .content(Value::Nodes(vec![query]))
}

fn build_leave_group_iq(group: &Jid) -> InfoQuery {
    let mut group_attrs = Attrs::new();
    group_attrs.insert("id".into(), Value::Jid(group.clone()));
    let leave = Node::new(
        "leave",
        Attrs::new(),
        Some(Value::Nodes(vec![Node::new("group", group_attrs, None)])),
    );
    InfoQuery::new("w:g2", IqType::Set)
        .to(group_server_jid())
        .content(Value::Nodes(vec![leave]))
}

fn build_set_subject_iq(group: &Jid, name: &str) -> InfoQuery {
    let subject = Node::new(
        "subject",
        Attrs::new(),
        Some(Value::Bytes(name.as_bytes().to_vec())),
    );
    InfoQuery::new("w:g2", IqType::Set)
        .to(group.clone())
        .content(Value::Nodes(vec![subject]))
}

fn build_set_topic_iq(
    group: &Jid,
    previous_id: Option<&str>,
    new_id: &str,
    topic: &str,
) -> InfoQuery {
    let mut attrs = Attrs::new();
    attrs.insert("id".into(), Value::String(new_id.to_owned()));
    if let Some(prev) = previous_id {
        if !prev.is_empty() {
            attrs.insert("prev".into(), Value::String(prev.to_owned()));
        }
    }
    let content = if topic.is_empty() {
        attrs.insert("delete".into(), Value::String("true".into()));
        None
    } else {
        let body = Node::new(
            "body",
            Attrs::new(),
            Some(Value::Bytes(topic.as_bytes().to_vec())),
        );
        Some(Value::Nodes(vec![body]))
    };
    let description = Node::new("description", attrs, content);
    InfoQuery::new("w:g2", IqType::Set)
        .to(group.clone())
        .content(Value::Nodes(vec![description]))
}

fn build_invite_link_iq(group: &Jid, reset: bool) -> InfoQuery {
    let iq_type = if reset { IqType::Set } else { IqType::Get };
    let invite = Node::tag_only("invite");
    InfoQuery::new("w:g2", iq_type)
        .to(group.clone())
        .content(Value::Nodes(vec![invite]))
}

fn build_join_with_link_iq(code: &str) -> InfoQuery {
    let mut attrs = Attrs::new();
    attrs.insert("code".into(), Value::String(code.to_owned()));
    let invite = Node::new("invite", attrs, None);
    InfoQuery::new("w:g2", IqType::Set)
        .to(group_server_jid())
        .content(Value::Nodes(vec![invite]))
}

/// Pull `<invite code="…"/>` out of an IQ result and return the full
/// invite link.
fn parse_invite_link_response(resp: &Node) -> Result<String, ClientError> {
    let invite = resp.child_by_tag(&["invite"]).ok_or_else(|| {
        ClientError::Malformed("invite-link response missing <invite> child".into())
    })?;
    let code = invite
        .get_attr_str("code")
        .ok_or_else(|| ClientError::Malformed("invite node missing `code` attribute".into()))?;
    Ok(format!("{INVITE_LINK_PREFIX}{code}"))
}

// -----------------------------------------------------------------------------
// Client API
// -----------------------------------------------------------------------------

impl Client {
    /// Fetch metadata for a group. Mirrors `whatsmeow.GetGroupInfo`.
    pub async fn get_group_info(&self, group: &Jid) -> Result<GroupInfo, ClientError> {
        let resp = self.send_iq(build_get_group_info_iq(group)).await?;
        if let Some(err) = iq_error_from_response(&resp) {
            return Err(err);
        }
        let group_node = resp.child_by_tag(&["group"]).ok_or_else(|| {
            ClientError::Malformed("group-info response missing <group> child".into())
        })?;
        parse_group_node(group_node)
    }

    /// Create a new group with the given name and initial participants.
    /// Mirrors the IQ-set portion of `whatsmeow.CreateGroup` (community /
    /// ephemeral options are not yet ported).
    pub async fn create_group(
        &self,
        name: &str,
        participants: &[Jid],
    ) -> Result<GroupInfo, ClientError> {
        // Upstream strips the "3EB0" prefix from the create key; we follow.
        let raw_key = crate::send::generate_message_id(self);
        let create_key = raw_key
            .strip_prefix(crate::send::WEB_MESSAGE_ID_PREFIX)
            .unwrap_or(&raw_key)
            .to_owned();

        let resp = self
            .send_iq(build_create_group_iq(name, participants, &create_key))
            .await?;
        if let Some(err) = iq_error_from_response(&resp) {
            return Err(err);
        }
        let group_node = resp.child_by_tag(&["group"]).ok_or_else(|| {
            ClientError::Malformed("create-group response missing <group> child".into())
        })?;
        parse_group_node(group_node)
    }

    /// Leave the given group. Mirrors `whatsmeow.LeaveGroup`.
    pub async fn leave_group(&self, group: &Jid) -> Result<(), ClientError> {
        let resp = self.send_iq(build_leave_group_iq(group)).await?;
        if let Some(err) = iq_error_from_response(&resp) {
            return Err(err);
        }
        Ok(())
    }

    /// Update a group's name (subject). Mirrors `whatsmeow.SetGroupName`.
    pub async fn set_group_name(&self, group: &Jid, name: &str) -> Result<(), ClientError> {
        let resp = self.send_iq(build_set_subject_iq(group, name)).await?;
        if let Some(err) = iq_error_from_response(&resp) {
            return Err(err);
        }
        Ok(())
    }

    /// Update a group's topic (description). Mirrors `whatsmeow.SetGroupTopic`,
    /// minus the optional caller-supplied previous-id / new-id overrides — we
    /// always fetch the current topic id and generate a fresh new one.
    pub async fn set_group_topic(&self, group: &Jid, topic: &str) -> Result<(), ClientError> {
        let previous_id = match self.get_group_info(group).await {
            Ok(info) => Some(info.topic_id),
            Err(e) => return Err(e),
        };
        let new_id = crate::send::generate_message_id(self);
        let prev_ref = previous_id.as_deref();
        let resp = self
            .send_iq(build_set_topic_iq(group, prev_ref, &new_id, topic))
            .await?;
        if let Some(err) = iq_error_from_response(&resp) {
            return Err(err);
        }
        Ok(())
    }

    /// Get (or reset) the public invite link for a group. Mirrors
    /// `whatsmeow.GetGroupInviteLink`.
    pub async fn get_group_invite_link(
        &self,
        group: &Jid,
        reset: bool,
    ) -> Result<String, ClientError> {
        let resp = self.send_iq(build_invite_link_iq(group, reset)).await?;
        if let Some(err) = iq_error_from_response(&resp) {
            return Err(err);
        }
        parse_invite_link_response(&resp)
    }

    /// Join a group using the invite-link code. Accepts either the bare code
    /// or the full `https://chat.whatsapp.com/CODE` URL. Mirrors
    /// `whatsmeow.JoinGroupWithLink`.
    pub async fn join_group_with_link(&self, code: &str) -> Result<Jid, ClientError> {
        let trimmed = code.strip_prefix(INVITE_LINK_PREFIX).unwrap_or(code);
        let resp = self.send_iq(build_join_with_link_iq(trimmed)).await?;
        if let Some(err) = iq_error_from_response(&resp) {
            return Err(err);
        }
        // If the group requires admin approval, the server replies with a
        // <membership_approval_request jid="…"/> instead of <group>.
        if let Some(req) = resp.child_by_tag(&["membership_approval_request"]) {
            return Ok(req.get_attr_jid("jid").cloned().unwrap_or_default());
        }
        let group_node = resp.child_by_tag(&["group"]).ok_or_else(|| {
            ClientError::Malformed("join-with-link response missing <group> child".into())
        })?;
        Ok(group_node.get_attr_jid("jid").cloned().unwrap_or_default())
    }

    /// Add, remove, promote, or demote group participants. Mirrors
    /// `whatsmeow.UpdateGroupParticipants` — returns one
    /// [`GroupParticipantUpdate`] per echoed participant so callers can tell
    /// which JIDs the server accepted vs. rejected (per-row `status`/`error`).
    pub async fn update_group_participants(
        &self,
        group: &Jid,
        participants: &[Jid],
        action: ParticipantAction,
    ) -> Result<Vec<GroupParticipantUpdate>, ClientError> {
        let resp = self
            .send_iq(build_update_participants_iq(group, participants, action))
            .await?;
        if let Some(err) = iq_error_from_response(&resp) {
            return Err(err);
        }
        let action_node = resp.child_by_tag(&[action.as_str()]).ok_or_else(|| {
            ClientError::Malformed(format!(
                "update-participants response missing <{}> child",
                action.as_str()
            ))
        })?;
        Ok(parse_participant_updates(action_node))
    }

    /// Toggle a group's announce-only mode. When `announce_only` is true only
    /// admins can post; otherwise any participant can. Mirrors
    /// `whatsmeow.SetGroupAnnounce`.
    pub async fn set_group_announce(
        &self,
        group: &Jid,
        announce_only: bool,
    ) -> Result<(), ClientError> {
        let resp = self
            .send_iq(build_set_group_announce_iq(group, announce_only))
            .await?;
        if let Some(err) = iq_error_from_response(&resp) {
            return Err(err);
        }
        Ok(())
    }

    /// Toggle a group's locked state. When `locked` is true only admins can
    /// edit group metadata (subject, topic, picture); otherwise any
    /// participant can. Mirrors `whatsmeow.SetGroupLocked`.
    pub async fn set_group_locked(&self, group: &Jid, locked: bool) -> Result<(), ClientError> {
        let resp = self.send_iq(build_set_group_locked_iq(group, locked)).await?;
        if let Some(err) = iq_error_from_response(&resp) {
            return Err(err);
        }
        Ok(())
    }

    /// List groups the local device is currently a member of. Mirrors
    /// `whatsmeow.GetJoinedGroups`.
    pub async fn get_joined_groups(&self) -> Result<Vec<GroupInfo>, ClientError> {
        let resp = self.send_iq(build_get_joined_groups_iq()).await?;
        if let Some(err) = iq_error_from_response(&resp) {
            return Err(err);
        }
        let groups = resp.child_by_tag(&["groups"]).ok_or_else(|| {
            ClientError::Malformed("joined-groups response missing <groups> child".into())
        })?;
        let mut out = Vec::new();
        // Side-effect: cache LID↔PN pairs surfaced inside each group's
        // `<participant>` children (mirrors upstream's `cacheGroupInfo`).
        let mut lid_pairs: Vec<(Jid, Jid)> = Vec::new();
        for child in groups.children() {
            if child.tag != "group" {
                continue;
            }
            let parsed = parse_group_node(child)?;
            for (lid, pn) in extract_lid_pn_pairs(child) {
                lid_pairs.push((lid, pn));
            }
            out.push(parsed);
        }
        for (lid, pn) in lid_pairs {
            if let Err(e) = self.device.lids.put_lid_pn_mapping(lid, pn).await {
                tracing::warn!("get_joined_groups: failed to persist LID↔PN mapping: {e}");
            }
        }
        Ok(out)
    }
}

/// Walk a `<group>` node's `<participant>` children and emit any LID↔PN
/// pairs the server volunteered. Mirrors `cacheGroupInfo` upstream — for each
/// participant we accept either:
///
/// - `<participant jid="…@lid" phone_number="…@s.whatsapp.net"/>` (LID-first
///   addressing mode)
/// - `<participant jid="…@s.whatsapp.net" lid="…@lid"/>` (PN-first addressing
///   mode)
fn extract_lid_pn_pairs(group_node: &Node) -> Vec<(Jid, Jid)> {
    let mut out = Vec::new();
    for child in group_node.children_by_tag("participant") {
        let jid = match child.get_attr_jid("jid") {
            Some(j) => j,
            None => continue,
        };
        if jid.server == wha_types::Server::HIDDEN_USER {
            if let Some(pn) = child.get_attr_jid("phone_number") {
                out.push((jid.clone(), pn.clone()));
            }
        } else if jid.server == wha_types::Server::DEFAULT_USER {
            if let Some(lid) = child.get_attr_jid("lid") {
                out.push((lid.clone(), jid.clone()));
            }
        }
    }
    out
}

// -----------------------------------------------------------------------------
// tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn synth_jid(user: &str, server: &str) -> Jid {
        Jid::new(user, server)
    }

    #[test]
    fn parse_group_node_extracts_name_topic_creator() {
        // <group id="120-456" subject="Lunch Crew" creation="1700000000"
        //        s_o="111@s.whatsapp.net" s_t="1700000050"
        //        creator="111@s.whatsapp.net" size="2">
        //   <participant jid="111@s.whatsapp.net" type="superadmin"/>
        //   <participant jid="222@s.whatsapp.net" type="admin"/>
        //   <description id="topic-7" t="1700000123" participant="111@s.whatsapp.net">
        //     <body>Where shall we eat?</body>
        //   </description>
        //   <announcement/>
        //   <ephemeral expiration="86400"/>
        // </group>
        let creator = synth_jid("111", wha_types::Server::DEFAULT_USER);
        let p2 = synth_jid("222", wha_types::Server::DEFAULT_USER);

        let mut g_attrs = Attrs::new();
        g_attrs.insert("id".into(), Value::String("120-456".into()));
        g_attrs.insert("subject".into(), Value::String("Lunch Crew".into()));
        g_attrs.insert("creation".into(), Value::String("1700000000".into()));
        g_attrs.insert("s_t".into(), Value::String("1700000050".into()));
        g_attrs.insert("s_o".into(), Value::Jid(creator.clone()));
        g_attrs.insert("creator".into(), Value::Jid(creator.clone()));
        g_attrs.insert("size".into(), Value::String("2".into()));

        let mut sup_attrs = Attrs::new();
        sup_attrs.insert("jid".into(), Value::Jid(creator.clone()));
        sup_attrs.insert("type".into(), Value::String("superadmin".into()));
        let part_super = Node::new("participant", sup_attrs, None);

        let mut admin_attrs = Attrs::new();
        admin_attrs.insert("jid".into(), Value::Jid(p2.clone()));
        admin_attrs.insert("type".into(), Value::String("admin".into()));
        let part_admin = Node::new("participant", admin_attrs, None);

        let body = Node::new(
            "body",
            Attrs::new(),
            Some(Value::Bytes(b"Where shall we eat?".to_vec())),
        );
        let mut desc_attrs = Attrs::new();
        desc_attrs.insert("id".into(), Value::String("topic-7".into()));
        desc_attrs.insert("t".into(), Value::String("1700000123".into()));
        desc_attrs.insert("participant".into(), Value::Jid(creator.clone()));
        let description = Node::new("description", desc_attrs, Some(Value::Nodes(vec![body])));

        let announce = Node::tag_only("announcement");

        let mut eph_attrs = Attrs::new();
        eph_attrs.insert("expiration".into(), Value::String("86400".into()));
        let ephemeral = Node::new("ephemeral", eph_attrs, None);

        let group = Node::new(
            "group",
            g_attrs,
            Some(Value::Nodes(vec![
                part_super,
                part_admin,
                description,
                announce,
                ephemeral,
            ])),
        );

        let info = parse_group_node(&group).expect("parse_group_node ok");

        assert_eq!(info.jid.user, "120-456");
        assert_eq!(info.jid.server, wha_types::Server::GROUP);
        assert_eq!(info.name, "Lunch Crew");
        assert_eq!(info.name_set_at, 1700000050);
        assert_eq!(info.name_set_by, creator);
        assert_eq!(info.owner, creator);
        assert_eq!(info.created_at, 1700000000);
        assert_eq!(info.participant_count, 2);

        assert_eq!(info.participants.len(), 2);
        assert_eq!(info.participants[0].jid, creator);
        assert!(info.participants[0].is_admin);
        assert!(info.participants[0].is_super_admin);
        assert_eq!(info.participants[1].jid, p2);
        assert!(info.participants[1].is_admin);
        assert!(!info.participants[1].is_super_admin);

        assert_eq!(info.topic, "Where shall we eat?");
        assert_eq!(info.topic_id, "topic-7");
        assert_eq!(info.topic_set_at, 1700000123);
        assert_eq!(info.topic_set_by, creator);

        assert!(info.is_announce);
        assert!(info.is_ephemeral);
        assert_eq!(info.disappearing_timer, 86400);
        assert!(!info.is_locked);
    }

    #[test]
    fn build_create_group_iq_has_xmlns_and_subject() {
        let p1 = synth_jid("111", wha_types::Server::DEFAULT_USER);
        let p2 = synth_jid("222", wha_types::Server::DEFAULT_USER);
        let q = build_create_group_iq("Hiking Buddies", &[p1.clone(), p2.clone()], "ABCDEF");
        let node = q.into_node("test-id".into());

        assert_eq!(node.tag, "iq");
        assert_eq!(node.get_attr_str("xmlns"), Some("w:g2"));
        assert_eq!(node.get_attr_str("type"), Some("set"));
        assert_eq!(node.get_attr_jid("to").map(|j| j.server.as_str()), Some("g.us"));

        let create = node
            .child_by_tag(&["create"])
            .expect("iq must contain <create>");
        assert_eq!(create.get_attr_str("subject"), Some("Hiking Buddies"));
        assert_eq!(create.get_attr_str("key"), Some("ABCDEF"));

        let parts = create.children_by_tag("participant");
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0].get_attr_jid("jid"), Some(&p1));
        assert_eq!(parts[1].get_attr_jid("jid"), Some(&p2));
    }

    #[test]
    fn build_update_participants_add_iq() {
        let group = synth_jid("120-456", wha_types::Server::GROUP);
        let p = synth_jid("333", wha_types::Server::DEFAULT_USER);
        let q = build_update_participants_iq(&group, &[p.clone()], ParticipantAction::Add);
        let node = q.into_node("test-id".into());

        assert_eq!(node.get_attr_str("xmlns"), Some("w:g2"));
        assert_eq!(node.get_attr_str("type"), Some("set"));
        assert_eq!(node.get_attr_jid("to").map(|j| j.user.as_str()), Some("120-456"));
        assert_eq!(node.get_attr_jid("to").map(|j| j.server.as_str()), Some("g.us"));

        let add = node.child_by_tag(&["add"]).expect("iq must contain <add>");
        let parts = add.children_by_tag("participant");
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].get_attr_jid("jid"), Some(&p));
    }

    #[test]
    fn build_update_participants_action_string_matches_tag() {
        let group = synth_jid("120-456", wha_types::Server::GROUP);
        let p = synth_jid("333", wha_types::Server::DEFAULT_USER);
        for (action, expected) in [
            (ParticipantAction::Add, "add"),
            (ParticipantAction::Remove, "remove"),
            (ParticipantAction::Promote, "promote"),
            (ParticipantAction::Demote, "demote"),
        ] {
            assert_eq!(action.as_str(), expected);
            let q = build_update_participants_iq(&group, &[p.clone()], action);
            let node = q.into_node("x".into());
            assert!(
                node.child_by_tag(&[expected]).is_some(),
                "iq must carry <{expected}> action node"
            );
        }
    }

    #[test]
    fn parse_invite_link_response_extracts_full_url() {
        // <iq><invite code="ABCDEFG"/></iq>
        let mut invite_attrs = Attrs::new();
        invite_attrs.insert("code".into(), Value::String("ABCDEFG".into()));
        let invite = Node::new("invite", invite_attrs, None);
        let iq = Node::new(
            "iq",
            Attrs::new(),
            Some(Value::Nodes(vec![invite])),
        );
        let link = parse_invite_link_response(&iq).expect("parse ok");
        assert_eq!(link, "https://chat.whatsapp.com/ABCDEFG");
    }

    #[test]
    fn iq_error_response_is_surfaced() {
        // <iq type="error"><error code="403" text="forbidden"/></iq>
        let mut iq_attrs = Attrs::new();
        iq_attrs.insert("type".into(), Value::String("error".into()));
        let mut err_attrs = Attrs::new();
        err_attrs.insert("code".into(), Value::String("403".into()));
        err_attrs.insert("text".into(), Value::String("forbidden".into()));
        let err_node = Node::new("error", err_attrs, None);
        let iq = Node::new("iq", iq_attrs, Some(Value::Nodes(vec![err_node])));
        match iq_error_from_response(&iq) {
            Some(ClientError::Iq { code, text }) => {
                assert_eq!(code, 403);
                assert_eq!(text, "forbidden");
            }
            other => panic!("expected ClientError::Iq, got {other:?}"),
        }
    }

    #[test]
    fn build_set_group_announce_iq_toggles_tag() {
        let group = synth_jid("120-456", wha_types::Server::GROUP);
        // announce_only = true → <announcement/>
        let on = build_set_group_announce_iq(&group, true).into_node("x".into());
        assert_eq!(on.get_attr_str("xmlns"), Some("w:g2"));
        assert_eq!(on.get_attr_str("type"), Some("set"));
        assert_eq!(on.get_attr_jid("to").map(|j| j.user.as_str()), Some("120-456"));
        assert!(on.child_by_tag(&["announcement"]).is_some());
        assert!(on.child_by_tag(&["not_announcement"]).is_none());

        // announce_only = false → <not_announcement/>
        let off = build_set_group_announce_iq(&group, false).into_node("x".into());
        assert!(off.child_by_tag(&["not_announcement"]).is_some());
        assert!(off.child_by_tag(&["announcement"]).is_none());
    }

    #[test]
    fn build_set_group_locked_iq_toggles_tag() {
        let group = synth_jid("120-456", wha_types::Server::GROUP);
        let locked = build_set_group_locked_iq(&group, true).into_node("x".into());
        assert_eq!(locked.get_attr_str("xmlns"), Some("w:g2"));
        assert_eq!(locked.get_attr_str("type"), Some("set"));
        assert!(locked.child_by_tag(&["locked"]).is_some());
        assert!(locked.child_by_tag(&["unlocked"]).is_none());

        let unlocked = build_set_group_locked_iq(&group, false).into_node("x".into());
        assert!(unlocked.child_by_tag(&["unlocked"]).is_some());
        assert!(unlocked.child_by_tag(&["locked"]).is_none());
    }

    #[test]
    fn build_get_joined_groups_iq_has_participating_with_two_children() {
        let q = build_get_joined_groups_iq().into_node("x".into());
        assert_eq!(q.get_attr_str("xmlns"), Some("w:g2"));
        assert_eq!(q.get_attr_str("type"), Some("get"));
        assert_eq!(q.get_attr_jid("to").map(|j| j.server.as_str()), Some("g.us"));
        let participating = q
            .child_by_tag(&["participating"])
            .expect("iq must contain <participating>");
        assert!(participating.child_by_tag(&["participants"]).is_some());
        assert!(participating.child_by_tag(&["description"]).is_some());
    }

    #[test]
    fn parse_participant_updates_extracts_status_and_addrequest() {
        // <add>
        //   <participant jid="333@s.whatsapp.net" error="0"/>     → success
        //   <participant jid="444@s.whatsapp.net" error="409"/>   → already in group
        //   <participant jid="555@s.whatsapp.net" error="403">    → invite required
        //     <add_request code="ABC123"/>
        //   </participant>
        // </add>
        let p1 = synth_jid("333", wha_types::Server::DEFAULT_USER);
        let p2 = synth_jid("444", wha_types::Server::DEFAULT_USER);
        let p3 = synth_jid("555", wha_types::Server::DEFAULT_USER);

        let mut a1 = Attrs::new();
        a1.insert("jid".into(), Value::Jid(p1.clone()));
        a1.insert("error".into(), Value::String("0".into()));
        let n1 = Node::new("participant", a1, None);

        let mut a2 = Attrs::new();
        a2.insert("jid".into(), Value::Jid(p2.clone()));
        a2.insert("error".into(), Value::String("409".into()));
        let n2 = Node::new("participant", a2, None);

        let mut ar = Attrs::new();
        ar.insert("code".into(), Value::String("ABC123".into()));
        let add_request = Node::new("add_request", ar, None);
        let mut a3 = Attrs::new();
        a3.insert("jid".into(), Value::Jid(p3.clone()));
        a3.insert("error".into(), Value::String("403".into()));
        let n3 = Node::new("participant", a3, Some(Value::Nodes(vec![add_request])));

        let add = Node::new("add", Attrs::new(), Some(Value::Nodes(vec![n1, n2, n3])));
        let updates = parse_participant_updates(&add);
        assert_eq!(updates.len(), 3);
        assert_eq!(updates[0].jid, p1);
        assert_eq!(updates[0].status, 0);
        assert!(updates[0].content.is_none());
        assert_eq!(updates[1].jid, p2);
        assert_eq!(updates[1].status, 409);
        assert_eq!(updates[2].jid, p3);
        assert_eq!(updates[2].status, 403);
        assert_eq!(updates[2].content.as_deref(), Some("ABC123"));
    }

    #[test]
    fn extract_lid_pn_pairs_from_participants_both_addressing_modes() {
        // <group>
        //   <participant jid="X@s.whatsapp.net" lid="L1@lid"/>           ← PN-first
        //   <participant jid="L2@lid" phone_number="Y@s.whatsapp.net"/>  ← LID-first
        //   <participant jid="Z@s.whatsapp.net"/>                         ← no LID
        // </group>
        let pn1 = synth_jid("X", wha_types::Server::DEFAULT_USER);
        let lid1 = synth_jid("L1", wha_types::Server::HIDDEN_USER);
        let lid2 = synth_jid("L2", wha_types::Server::HIDDEN_USER);
        let pn2 = synth_jid("Y", wha_types::Server::DEFAULT_USER);
        let pn3 = synth_jid("Z", wha_types::Server::DEFAULT_USER);

        let mut a1 = Attrs::new();
        a1.insert("jid".into(), Value::Jid(pn1.clone()));
        a1.insert("lid".into(), Value::Jid(lid1.clone()));
        let n1 = Node::new("participant", a1, None);

        let mut a2 = Attrs::new();
        a2.insert("jid".into(), Value::Jid(lid2.clone()));
        a2.insert("phone_number".into(), Value::Jid(pn2.clone()));
        let n2 = Node::new("participant", a2, None);

        let mut a3 = Attrs::new();
        a3.insert("jid".into(), Value::Jid(pn3.clone()));
        let n3 = Node::new("participant", a3, None);

        let group = Node::new("group", Attrs::new(), Some(Value::Nodes(vec![n1, n2, n3])));
        let pairs = extract_lid_pn_pairs(&group);
        assert_eq!(pairs.len(), 2);
        assert_eq!(pairs[0], (lid1, pn1));
        assert_eq!(pairs[1], (lid2, pn2));
    }
}
