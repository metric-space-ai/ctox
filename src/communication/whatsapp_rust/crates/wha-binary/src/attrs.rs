//! AttrUtility-style helper for reading attributes off a [`Node`].
//!
//! Mirrors `whatsmeow/binary/attrs.go`: every getter records its error
//! into `errors` instead of failing eagerly so callers can ergonomically
//! pull a bag of attrs and check `ok()` once.

use wha_types::Jid;

use crate::error::BinaryError;
use crate::node::{Node, Value};

/// Map of attribute name → typed value.
pub type Attrs = crate::node::Attrs;

pub struct AttrUtility<'a> {
    pub attrs: &'a Attrs,
    pub errors: Vec<BinaryError>,
}

impl Node {
    pub fn attr_getter(&self) -> AttrUtility<'_> {
        AttrUtility { attrs: &self.attrs, errors: Vec::new() }
    }
}

impl<'a> AttrUtility<'a> {
    pub fn ok(&self) -> bool {
        self.errors.is_empty()
    }

    pub fn into_result(self) -> Result<(), Vec<BinaryError>> {
        if self.errors.is_empty() { Ok(()) } else { Err(self.errors) }
    }

    pub fn optional_string(&mut self, key: &str) -> Option<&'a str> {
        match self.attrs.get(key) {
            Some(Value::String(s)) => Some(s.as_str()),
            None => None,
            Some(_) => {
                self.errors.push(BinaryError::Attr(format!("attr `{key}` not a string")));
                None
            }
        }
    }

    pub fn string(&mut self, key: &str) -> &'a str {
        match self.attrs.get(key) {
            Some(Value::String(s)) => s.as_str(),
            None => {
                self.errors.push(BinaryError::Attr(format!("missing required attr `{key}`")));
                ""
            }
            Some(_) => {
                self.errors.push(BinaryError::Attr(format!("attr `{key}` not a string")));
                ""
            }
        }
    }

    pub fn optional_jid(&mut self, key: &str) -> Option<&'a Jid> {
        match self.attrs.get(key) {
            Some(Value::Jid(j)) => Some(j),
            None => None,
            Some(_) => {
                self.errors.push(BinaryError::Attr(format!("attr `{key}` not a JID")));
                None
            }
        }
    }

    pub fn jid(&mut self, key: &str) -> Jid {
        match self.attrs.get(key) {
            Some(Value::Jid(j)) => j.clone(),
            None => {
                self.errors.push(BinaryError::Attr(format!("missing required JID attr `{key}`")));
                Jid::default()
            }
            Some(_) => {
                self.errors.push(BinaryError::Attr(format!("attr `{key}` not a JID")));
                Jid::default()
            }
        }
    }

    pub fn optional_i64(&mut self, key: &str) -> Option<i64> {
        let s = self.optional_string(key)?;
        match s.parse::<i64>() {
            Ok(v) => Some(v),
            Err(e) => {
                self.errors.push(BinaryError::Attr(format!("attr `{key}` not an i64: {e}")));
                None
            }
        }
    }

    pub fn i64(&mut self, key: &str) -> i64 {
        self.optional_i64(key).unwrap_or_else(|| {
            self.errors.push(BinaryError::Attr(format!("missing required i64 attr `{key}`")));
            0
        })
    }

    pub fn optional_u64(&mut self, key: &str) -> Option<u64> {
        let s = self.optional_string(key)?;
        match s.parse::<u64>() {
            Ok(v) => Some(v),
            Err(e) => {
                self.errors.push(BinaryError::Attr(format!("attr `{key}` not a u64: {e}")));
                None
            }
        }
    }

    pub fn optional_bool(&mut self, key: &str) -> bool {
        self.optional_string(key)
            .and_then(|s| s.parse::<bool>().ok())
            .unwrap_or(false)
    }
}
