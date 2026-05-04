use std::fmt;

use wha_types::Jid;

/// `(name, device_id)` — the canonical form of a Signal session address.
/// Mirrors `signalProtocol.SignalAddress` from libsignal.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SignalAddress {
    pub name: String,
    pub device_id: u32,
}

impl SignalAddress {
    pub fn new(name: impl Into<String>, device_id: u32) -> Self {
        Self { name: name.into(), device_id }
    }

    /// Canonical wire form: `name:device_id`.
    pub fn serialize(&self) -> String {
        format!("{}:{}", self.name, self.device_id)
    }

    /// Build an address from a JID via the rules in
    /// `whatsmeow/types/jid.go::SignalAddressUser`.
    pub fn from_jid(jid: &Jid) -> Self {
        Self::new(jid.signal_address_user(), jid.device as u32)
    }
}

impl fmt::Display for SignalAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.serialize())
    }
}

/// Group + sender pair for sender-key sessions. Used by the group-encryption
/// codepath where every member rotates a per-group symmetric key.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SenderKeyName {
    pub group_id: String,
    pub sender: SignalAddress,
}

impl SenderKeyName {
    pub fn new(group_id: impl Into<String>, sender: SignalAddress) -> Self {
        Self { group_id: group_id.into(), sender }
    }
    pub fn serialize(&self) -> String {
        format!("{}::{}", self.group_id, self.sender.serialize())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_jid_keeps_device() {
        let jid: Jid = "1234:5@s.whatsapp.net".parse().unwrap();
        let a = SignalAddress::from_jid(&jid);
        assert_eq!(a.name, "1234");
        assert_eq!(a.device_id, 5);
    }

    #[test]
    fn lid_jid_uses_agent_suffix() {
        let jid: Jid = "1234@lid".parse().unwrap();
        let a = SignalAddress::from_jid(&jid);
        assert_eq!(a.name, "1234_1");
    }
}
