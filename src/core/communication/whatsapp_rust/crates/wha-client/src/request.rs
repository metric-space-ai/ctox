use wha_binary::{Attrs, Node, Value};
use wha_types::Jid;

/// Mirrors the `infoQuery` struct in `whatsmeow/request.go`.
#[derive(Debug, Clone)]
pub struct InfoQuery {
    pub namespace: String,
    pub iq_type: IqType,
    pub to: Option<Jid>,
    pub target: Option<Jid>,
    pub id: Option<String>,
    pub content: Option<Value>,
    pub timeout: Option<std::time::Duration>,
    pub no_retry: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IqType {
    Get,
    Set,
}

impl IqType {
    pub fn as_str(&self) -> &'static str {
        match self {
            IqType::Get => "get",
            IqType::Set => "set",
        }
    }
}

impl InfoQuery {
    pub fn new(namespace: impl Into<String>, iq_type: IqType) -> Self {
        Self {
            namespace: namespace.into(),
            iq_type,
            to: None,
            target: None,
            id: None,
            content: None,
            timeout: None,
            no_retry: false,
        }
    }

    pub fn to(mut self, jid: Jid) -> Self {
        self.to = Some(jid);
        self
    }

    pub fn content(mut self, value: Value) -> Self {
        self.content = Some(value);
        self
    }

    /// Override the per-IQ response timeout. Defaults to
    /// [`crate::client::DEFAULT_REQUEST_TIMEOUT`] when left unset. Useful for
    /// short-lived health pings (e.g. the keepalive loop's 10-second budget).
    pub fn with_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    pub(crate) fn into_node(self, id: String) -> Node {
        let mut attrs = Attrs::new();
        attrs.insert("id".into(), Value::String(id));
        attrs.insert("xmlns".into(), Value::String(self.namespace));
        attrs.insert("type".into(), Value::String(self.iq_type.as_str().into()));
        if let Some(to) = self.to {
            attrs.insert("to".into(), Value::Jid(to));
        }
        if let Some(target) = self.target {
            attrs.insert("target".into(), Value::Jid(target));
        }
        Node::new("iq", attrs, self.content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn into_node_carries_required_attrs() {
        let n = InfoQuery::new("w:p", IqType::Set)
            .to(Jid::new("", "s.whatsapp.net"))
            .into_node("xyz".into());
        assert_eq!(n.tag, "iq");
        assert_eq!(n.get_attr_str("id"), Some("xyz"));
        assert_eq!(n.get_attr_str("xmlns"), Some("w:p"));
        assert_eq!(n.get_attr_str("type"), Some("set"));
    }
}
