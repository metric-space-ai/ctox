//! Bot ↔ persona mapping. Mirrors `_upstream/whatsmeow/types/botmap.go`.
//!
//! WhatsApp's Meta-AI / "bot" surface assigns each public bot two JIDs:
//!
//! * a stable **bot-server JID** (`<persona>@bot`) that survives backend
//!   reshuffles, used for addressing and Signal sessions, and
//! * a phone-number-shaped **owner JID** (`<phone>@s.whatsapp.net`) that
//!   identifies which Meta-AI persona owns the bot — surfaced in receipts
//!   and message contexts.
//!
//! Upstream ships a static lookup table baked at `_upstream/whatsmeow/types/botmap.go::BotJIDMap`
//! (≈210 entries; `<bot-jid> → <owner-phone-jid>`). The actual data is huge
//! and changes whenever Meta rotates the bot fleet, so the [`BotMap`] type
//! exposed here is **dynamic**: callers populate it at startup (e.g. by
//! parsing a bundled JSON, calling a server endpoint, or hand-rolling a
//! fixture for tests). The static upstream values can be loaded into the
//! map by callers that want to reproduce upstream behaviour byte-for-byte.
//!
//! Two operations are supported on the map itself:
//!
//! * [`BotMap::put_bot_owner`] — insert / overwrite a `(bot, owner)` row.
//! * [`BotMap::get_bot_owner`] — look up a bot's owner.
//!
//! Plus one classification helper that does not consult the map:
//!
//! * [`BotMap::is_bot_jid`] — `true` for any JID on the `bot` server.
//!   Mirrors `JID.IsBot()` upstream for the simple `Server == "bot"` case.

use std::collections::HashMap;

use crate::jid::{server, Jid};

/// Bidirectional bot ↔ owner mapping. Mirrors the values held statically in
/// `whatsmeow/types/botmap.go::BotJIDMap`, but populated at runtime so callers
/// can carry their own bot fleet definition.
#[derive(Clone, Debug, Default)]
pub struct BotMap {
    /// `bot JID → owner (Meta-AI persona) JID`. Mirrors upstream's
    /// `BotJIDMap` map[bot]owner direction.
    bot_to_owner: HashMap<Jid, Jid>,
}

impl BotMap {
    /// Construct an empty map. Callers populate it via
    /// [`put_bot_owner`](Self::put_bot_owner).
    pub fn new() -> Self {
        BotMap {
            bot_to_owner: HashMap::new(),
        }
    }

    /// Insert / overwrite the `(bot, owner)` mapping. Subsequent calls with
    /// the same `bot` replace the prior owner — matching the semantics of
    /// `BotJIDMap[bot] = owner` upstream.
    pub fn put_bot_owner(&mut self, bot: Jid, owner: Jid) {
        self.bot_to_owner.insert(bot, owner);
    }

    /// Look up a bot's owner. Returns `None` for unknown bot JIDs.
    pub fn get_bot_owner(&self, bot: &Jid) -> Option<&Jid> {
        self.bot_to_owner.get(bot)
    }

    /// `true` if `jid` lives on the `bot` server. Mirrors upstream's
    /// `JID.IsBot()` minus the legacy regex on `s.whatsapp.net` —
    /// the bot server itself is the canonical signal that we're addressing
    /// a bot persona.
    ///
    /// Note on the upstream regex: `IsBot()` upstream also matches a few
    /// `1313555…@s.whatsapp.net` and `131655500…@s.whatsapp.net` numbers
    /// that historically front Meta-AI bots. The Rust port keeps the simple
    /// "is on `@bot`" classifier separate; callers that want regex-style
    /// classification can compose it on top of [`Jid`] directly.
    pub fn is_bot_jid(jid: &Jid) -> bool {
        jid.server == server::BOT
    }

    /// Number of registered (bot, owner) pairs. Useful for sanity checks
    /// after bulk-loading the upstream table.
    pub fn len(&self) -> usize {
        self.bot_to_owner.len()
    }

    /// `true` when no bots have been registered yet.
    pub fn is_empty(&self) -> bool {
        self.bot_to_owner.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Round-trip: an inserted (bot, owner) pair is retrievable, and a
    /// second `put_bot_owner` for the same bot replaces the prior owner.
    #[test]
    fn put_and_get_bot_owner_round_trip() {
        let mut map = BotMap::new();
        let bot: Jid = "867051314767696@bot".parse().unwrap();
        let owner: Jid = "13135550002@s.whatsapp.net".parse().unwrap();

        // Empty map — nothing returns.
        assert!(map.get_bot_owner(&bot).is_none());
        assert!(map.is_empty());

        // Insert + retrieve.
        map.put_bot_owner(bot.clone(), owner.clone());
        assert_eq!(map.get_bot_owner(&bot), Some(&owner));
        assert_eq!(map.len(), 1);

        // Overwrite with a fresh owner.
        let new_owner: Jid = "13135559999@s.whatsapp.net".parse().unwrap();
        map.put_bot_owner(bot.clone(), new_owner.clone());
        assert_eq!(map.get_bot_owner(&bot), Some(&new_owner));
        assert_eq!(map.len(), 1, "overwrite must not grow the map");

        // Unknown bot — None.
        let other_bot: Jid = "999@bot".parse().unwrap();
        assert!(map.get_bot_owner(&other_bot).is_none());
    }

    /// `is_bot_jid` classifies any JID on the `bot` server as a bot, and
    /// rejects JIDs on every other server (default-user, group, lid, …).
    /// Mirrors the simple `Server == BotServer` arm of upstream's
    /// `JID.IsBot()`.
    #[test]
    fn is_bot_jid_classifies_bot_server_only() {
        let bot: Jid = "867051314767696@bot".parse().unwrap();
        let user: Jid = "13135550002@s.whatsapp.net".parse().unwrap();
        let group: Jid = "120363042-test@g.us".parse().unwrap();
        let lid: Jid = "9876@lid".parse().unwrap();
        let msgr: Jid = "5550001@msgr".parse().unwrap();

        assert!(BotMap::is_bot_jid(&bot), "@bot server must classify as bot");
        assert!(!BotMap::is_bot_jid(&user), "default user is not a bot");
        assert!(!BotMap::is_bot_jid(&group), "group is not a bot");
        assert!(!BotMap::is_bot_jid(&lid), "lid is not a bot");
        assert!(!BotMap::is_bot_jid(&msgr), "messenger is not a bot");
    }

    /// Loading a small slice of upstream's `BotJIDMap` and round-tripping
    /// each entry. This pins our `(bot, owner)` semantics against upstream's
    /// actual data shape — server attribution included.
    #[test]
    fn upstream_botmap_subset_round_trips() {
        // Sample from `_upstream/whatsmeow/types/botmap.go::BotJIDMap`.
        let entries = [
            ("867051314767696", "13135550002"),
            ("1061492271844689", "13135550005"),
            ("245886058483988", "13135550009"),
        ];
        let mut map = BotMap::new();
        for (bot, owner) in entries {
            let bot: Jid = format!("{bot}@bot").parse().unwrap();
            let owner: Jid = format!("{owner}@s.whatsapp.net").parse().unwrap();
            map.put_bot_owner(bot, owner);
        }
        assert_eq!(map.len(), 3);
        for (bot, owner) in entries {
            let bot: Jid = format!("{bot}@bot").parse().unwrap();
            let owner: Jid = format!("{owner}@s.whatsapp.net").parse().unwrap();
            assert_eq!(map.get_bot_owner(&bot), Some(&owner));
            // Each bot JID is classified as a bot per `is_bot_jid`.
            assert!(BotMap::is_bot_jid(&bot));
            assert!(!BotMap::is_bot_jid(&owner));
        }
    }
}
