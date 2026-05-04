use std::collections::BTreeMap;

use wha_types::Jid;

use crate::error::BinaryError;

/// A heterogenous attribute or content value. The wire format allows bare
/// strings, JIDs, byte buffers, and lists of nodes; this enum is the union of
/// what the encoder accepts and what the decoder produces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    String(String),
    Jid(Jid),
    Bytes(Vec<u8>),
    Nodes(Vec<Node>),
    None,
}

impl Value {
    pub fn as_str(&self) -> Option<&str> {
        if let Value::String(s) = self { Some(s.as_str()) } else { None }
    }
    pub fn as_jid(&self) -> Option<&Jid> {
        if let Value::Jid(j) = self { Some(j) } else { None }
    }
    pub fn as_bytes(&self) -> Option<&[u8]> {
        if let Value::Bytes(b) = self { Some(b) } else { None }
    }
    pub fn as_nodes(&self) -> Option<&[Node]> {
        if let Value::Nodes(n) = self { Some(n) } else { None }
    }
    pub fn is_none(&self) -> bool {
        matches!(self, Value::None)
    }
}

impl From<&str> for Value {
    fn from(s: &str) -> Self { Value::String(s.to_owned()) }
}
impl From<String> for Value {
    fn from(s: String) -> Self { Value::String(s) }
}
impl From<Jid> for Value {
    fn from(j: Jid) -> Self { Value::Jid(j) }
}
impl From<Vec<u8>> for Value {
    fn from(b: Vec<u8>) -> Self { Value::Bytes(b) }
}
impl From<Vec<Node>> for Value {
    fn from(n: Vec<Node>) -> Self { Value::Nodes(n) }
}

/// Map of attribute name → value. Sorted for deterministic encoding (helpful
/// for tests; the wire protocol itself is order-insensitive).
pub type Attrs = BTreeMap<String, Value>;

/// A WhatsApp XML element.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    pub tag: String,
    pub attrs: Attrs,
    pub content: Value,
}

impl Node {
    pub fn new(tag: impl Into<String>, attrs: Attrs, content: Option<Value>) -> Self {
        Node { tag: tag.into(), attrs, content: content.unwrap_or(Value::None) }
    }

    /// Empty `<tag />` shorthand.
    pub fn tag_only(tag: impl Into<String>) -> Self {
        Node { tag: tag.into(), attrs: Attrs::new(), content: Value::None }
    }

    pub fn children(&self) -> &[Node] {
        match &self.content {
            Value::Nodes(n) => n,
            _ => &[],
        }
    }

    pub fn children_by_tag(&self, tag: &str) -> Vec<&Node> {
        self.children().iter().filter(|c| c.tag == tag).collect()
    }

    pub fn child_by_tag<'a>(&'a self, tags: &[&str]) -> Option<&'a Node> {
        let mut current = self;
        for t in tags {
            current = current.children().iter().find(|c| &c.tag == t)?;
        }
        Some(current)
    }

    pub fn get_attr_str(&self, key: &str) -> Option<&str> {
        self.attrs.get(key).and_then(|v| v.as_str())
    }
    pub fn get_attr_jid(&self, key: &str) -> Option<&Jid> {
        self.attrs.get(key).and_then(|v| v.as_jid())
    }
}

/// Type-erased helper used by the encoder to validate node shapes.
pub(crate) fn count_attrs(attrs: &Attrs) -> usize {
    attrs
        .values()
        .filter(|v| match v {
            Value::String(s) => !s.is_empty(),
            Value::None => false,
            _ => true,
        })
        .count()
}

/// Errors specific to attribute validation flow downstream of the decoder.
pub type AttrResult<T> = Result<T, BinaryError>;
