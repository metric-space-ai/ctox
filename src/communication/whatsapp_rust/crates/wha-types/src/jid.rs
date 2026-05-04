use std::fmt;
use std::str::FromStr;

use crate::error::ParseJidError;

/// Well-known WhatsApp JID servers — see `whatsmeow/types/jid.go`.
pub mod server {
    pub const DEFAULT_USER: &str = "s.whatsapp.net";
    pub const GROUP: &str = "g.us";
    pub const LEGACY_USER: &str = "c.us";
    pub const BROADCAST: &str = "broadcast";
    pub const HIDDEN_USER: &str = "lid";
    pub const MESSENGER: &str = "msgr";
    pub const INTEROP: &str = "interop";
    pub const NEWSLETTER: &str = "newsletter";
    pub const HOSTED: &str = "hosted";
    pub const HOSTED_LID: &str = "hosted.lid";
    pub const BOT: &str = "bot";
}

/// Domain-type discriminators used inside binary AD JIDs.
pub const WA_DOMAIN: u8 = 0;
pub const LID_DOMAIN: u8 = 1;
pub const HOSTED_DOMAIN: u8 = 128;
pub const HOSTED_LID_DOMAIN: u8 = 129;

/// A WhatsApp JID (Jabber ID).
///
/// There are two flavours of JID: regular pairs (`user@server`) and AD JIDs that
/// carry an extra `agent` + `device` to address a specific linked device.
/// Interop and Messenger JIDs additionally carry an `integrator` field.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct Jid {
    pub user: String,
    pub raw_agent: u8,
    pub device: u16,
    pub integrator: u16,
    pub server: String,
}

pub type MessageId = String;
pub type MessageServerId = i64;

/// Re-export so callers can write `Server::DEFAULT_USER` if they prefer.
pub use server as Server;

impl Jid {
    pub fn new(user: impl Into<String>, server: impl Into<String>) -> Self {
        Jid { user: user.into(), server: server.into(), ..Default::default() }
    }

    /// AD JID constructor. The `agent` parameter doubles as a domain-type
    /// discriminator: `LID_DOMAIN` or `HOSTED_DOMAIN` rewrite the server.
    pub fn new_ad(user: impl Into<String>, agent: u8, device: u8) -> Self {
        let user = user.into();
        let (server, raw_agent) = match agent {
            LID_DOMAIN => (server::HIDDEN_USER.to_owned(), 0),
            HOSTED_DOMAIN => (server::HOSTED.to_owned(), 0),
            HOSTED_LID_DOMAIN => (server::HOSTED_LID.to_owned(), 0),
            WA_DOMAIN => (server::DEFAULT_USER.to_owned(), 0),
            other => (server::DEFAULT_USER.to_owned(), other),
        };
        Jid { user, raw_agent, device: device.into(), integrator: 0, server }
    }

    /// Returns the actual domain-type byte used in binary AD JIDs.
    pub fn actual_agent(&self) -> u8 {
        match self.server.as_str() {
            server::DEFAULT_USER => WA_DOMAIN,
            server::HIDDEN_USER => LID_DOMAIN,
            server::HOSTED => HOSTED_DOMAIN,
            server::HOSTED_LID => HOSTED_LID_DOMAIN,
            _ => self.raw_agent,
        }
    }

    /// Numeric phone number, valid only for user JIDs.
    pub fn user_int(&self) -> u64 {
        self.user.parse().unwrap_or(0)
    }

    /// Strip the agent + device, e.g. for storing as a "contact JID".
    pub fn to_non_ad(&self) -> Self {
        Self {
            user: self.user.clone(),
            server: self.server.clone(),
            integrator: self.integrator,
            ..Default::default()
        }
    }

    /// JID stringified in AD form: `user.agent:device@server`.
    pub fn ad_string(&self) -> String {
        format!("{}.{}:{}@{}", self.user, self.raw_agent, self.device, self.server)
    }

    /// Address used for Signal sessions. AD JIDs from non-default servers
    /// suffix the agent so they get distinct Signal addresses.
    pub fn signal_address_user(&self) -> String {
        let agent = self.actual_agent();
        if agent == 0 { self.user.clone() } else { format!("{}_{}", self.user, agent) }
    }

    pub fn is_empty(&self) -> bool {
        self.server.is_empty()
    }

    pub fn is_user(&self) -> bool {
        matches!(self.server.as_str(), server::DEFAULT_USER | server::HIDDEN_USER | server::HOSTED | server::HOSTED_LID | server::MESSENGER | server::INTEROP)
    }

    pub fn is_group(&self) -> bool {
        self.server == server::GROUP
    }

    pub fn is_broadcast_list(&self) -> bool {
        self.server == server::BROADCAST && self.user != "status"
    }

    /// Parse a JID string. Recognises:
    /// * `server`              → empty user
    /// * `user@server`         → regular pair
    /// * `user:device@server`  → device-suffixed
    /// * `user.agent:device@server` → full AD JID
    pub fn parse(s: &str) -> Result<Self, ParseJidError> {
        let (user_part, server_part) = match s.split_once('@') {
            Some(t) => t,
            None => return Ok(Jid::new("", s)),
        };
        let mut jid = Jid::new(user_part, server_part);

        if jid.user.contains('.') {
            let original = std::mem::take(&mut jid.user);
            let parts: Vec<&str> = original.splitn(3, '.').collect();
            if parts.len() != 2 {
                return Err(ParseJidError::TooManyDots);
            }
            let user = parts[0];
            let ad = parts[1];

            let ad_parts: Vec<&str> = ad.splitn(3, ':').collect();
            if ad_parts.len() > 2 {
                return Err(ParseJidError::TooManyColons);
            }
            jid.user = user.to_owned();
            jid.raw_agent = ad_parts[0]
                .parse::<u8>()
                .map_err(|e| ParseJidError::BadAgent(e.to_string()))?;
            if let Some(d) = ad_parts.get(1) {
                jid.device = d
                    .parse()
                    .map_err(|e: std::num::ParseIntError| ParseJidError::BadDevice(e.to_string()))?;
            }
        } else if jid.user.contains(':') {
            let original = std::mem::take(&mut jid.user);
            let parts: Vec<&str> = original.splitn(3, ':').collect();
            if parts.len() != 2 {
                return Err(ParseJidError::TooManyColons);
            }
            jid.user = parts[0].to_owned();
            jid.device = parts[1]
                .parse()
                .map_err(|e: std::num::ParseIntError| ParseJidError::BadDevice(e.to_string()))?;
        }

        Ok(jid)
    }
}

impl fmt::Display for Jid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.raw_agent > 0 {
            write!(f, "{}.{}:{}@{}", self.user, self.raw_agent, self.device, self.server)
        } else if self.device > 0 {
            write!(f, "{}:{}@{}", self.user, self.device, self.server)
        } else if !self.user.is_empty() {
            write!(f, "{}@{}", self.user, self.server)
        } else {
            f.write_str(&self.server)
        }
    }
}

impl FromStr for Jid {
    type Err = ParseJidError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Jid::parse(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_user() {
        let j: Jid = "1234@s.whatsapp.net".parse().unwrap();
        assert_eq!(j.user, "1234");
        assert_eq!(j.server, "s.whatsapp.net");
        assert_eq!(j.device, 0);
        assert!(j.is_user());
    }

    #[test]
    fn parse_device_suffix() {
        let j: Jid = "1234:5@s.whatsapp.net".parse().unwrap();
        assert_eq!(j.user, "1234");
        assert_eq!(j.device, 5);
    }

    #[test]
    fn parse_full_ad() {
        let j: Jid = "1234.7:9@s.whatsapp.net".parse().unwrap();
        assert_eq!(j.user, "1234");
        assert_eq!(j.raw_agent, 7);
        assert_eq!(j.device, 9);
    }

    #[test]
    fn display_round_trip() {
        for s in ["g.us", "1234@s.whatsapp.net", "1234:5@s.whatsapp.net", "1234.7:9@s.whatsapp.net"] {
            let j: Jid = s.parse().unwrap();
            assert_eq!(j.to_string(), s, "round trip failed for {s}");
        }
    }

    #[test]
    fn ad_jid_constructor_lid_domain() {
        let j = Jid::new_ad("12", LID_DOMAIN, 4);
        assert_eq!(j.server, server::HIDDEN_USER);
        assert_eq!(j.raw_agent, 0);
        assert_eq!(j.actual_agent(), LID_DOMAIN);
    }

    #[test]
    fn signal_address_user_strips_default_agent() {
        let j: Jid = "1234@s.whatsapp.net".parse().unwrap();
        assert_eq!(j.signal_address_user(), "1234");
        let j2: Jid = "1234@lid".parse().unwrap();
        assert_eq!(j2.signal_address_user(), format!("1234_{}", LID_DOMAIN));
    }

    #[test]
    fn empty_jid() {
        assert!(Jid::default().is_empty());
    }
}
