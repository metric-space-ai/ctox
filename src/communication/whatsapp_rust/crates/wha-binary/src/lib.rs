//! Encoder/decoder for WhatsApp's binary XML wire format.
//!
//! This is a direct, dependency-free port of `whatsmeow/binary/`. The wire
//! format itself is documented inline in [`token`]; the high-level entry points
//! are [`marshal`] and [`unmarshal`] (which speak [`Node`]s).

pub mod attrs;
mod decoder;
mod encoder;
mod error;
pub mod node;
pub mod token;
mod unpack;

pub use attrs::{Attrs, AttrUtility};
pub use error::BinaryError;
pub use node::{Node, Value};
pub use unpack::unpack;

#[cfg(test)]
mod tests {
    use super::*;
    use wha_types::Jid;

    fn round_trip(n: Node) {
        let bytes = marshal(&n).expect("marshal");
        let back = unmarshal(&bytes).expect("unmarshal");
        assert_eq!(n, back, "round-trip failed");
    }

    #[test]
    fn empty_node() {
        round_trip(Node::tag_only("iq"));
    }

    #[test]
    fn iq_with_attrs() {
        let mut attrs = Attrs::new();
        attrs.insert("type".into(), Value::String("get".into()));
        attrs.insert("id".into(), Value::String("abc".into()));
        attrs.insert("to".into(), Value::String("s.whatsapp.net".into()));
        round_trip(Node::new("iq", attrs, None));
    }

    #[test]
    fn nested_children() {
        let inner = Node::new(
            "child",
            {
                let mut a = Attrs::new();
                a.insert("k".into(), Value::String("v".into()));
                a
            },
            None,
        );
        let parent = Node::new("parent", Attrs::new(), Some(Value::Nodes(vec![inner])));
        round_trip(parent);
    }

    #[test]
    fn node_with_jid_attr() {
        let mut attrs = Attrs::new();
        attrs.insert(
            "to".into(),
            Value::Jid(Jid::new("12345", "s.whatsapp.net")),
        );
        round_trip(Node::new("message", attrs, None));
    }

    #[test]
    fn ad_jid_round_trip_keeps_device() {
        let mut attrs = Attrs::new();
        let jid = Jid::new_ad("12345", 0, 7);
        attrs.insert("from".into(), Value::Jid(jid.clone()));
        let node = Node::new("ack", attrs, None);
        let bytes = marshal(&node).unwrap();
        let back = unmarshal(&bytes).unwrap();
        assert_eq!(back.get_attr_jid("from").unwrap().device, 7);
        assert_eq!(back.get_attr_jid("from").unwrap().user, "12345");
    }

    #[test]
    fn binary_content() {
        let node = Node::new(
            "enc",
            Attrs::new(),
            Some(Value::Bytes(vec![1, 2, 3, 4, 5])),
        );
        round_trip(node);
    }

    #[test]
    fn nibble_encodes_phone_number() {
        // Strings of digits/-/. encode as packed nibbles. The decoded value
        // should round-trip but the bytes are noticeably shorter.
        let s = "1234567890";
        let mut attrs = Attrs::new();
        attrs.insert("to".into(), Value::String(s.into()));
        let bytes = marshal(&Node::new("x", attrs, None)).unwrap();
        // Heuristic: nibble-packed should be smaller than raw-encoded length
        let mut attrs2 = Attrs::new();
        attrs2.insert("to".into(), Value::String("not-a-nibble-because-letters".into()));
        let bytes2 = marshal(&Node::new("x", attrs2, None)).unwrap();
        assert!(bytes.len() < bytes2.len());
    }
}

/// Encode a [`Node`] into WhatsApp's binary XML format. The returned buffer
/// always starts with a leading zero byte (uncompressed indicator), matching
/// `whatsmeow/binary.Marshal`.
pub fn marshal(n: &Node) -> Result<Vec<u8>, BinaryError> {
    let mut enc = encoder::Encoder::new();
    enc.write_node(n)?;
    Ok(enc.finish())
}

/// Decode a WhatsApp binary XML buffer back into a [`Node`].
///
/// Symmetric to [`marshal`]: `unmarshal(marshal(n)?)` returns `n`. Internally
/// it routes through [`unpack`] so it also accepts zlib-compressed payloads
/// (the second-bit-set flavour of the leading framing byte).
pub fn unmarshal(data: &[u8]) -> Result<Node, BinaryError> {
    let unpacked = unpack(data)?;
    unmarshal_raw(&unpacked)
}

/// Like [`unmarshal`] but the input is expected to *already* be unpacked, i.e.
/// no leading framing byte. Used by the noise socket layer.
pub fn unmarshal_raw(data: &[u8]) -> Result<Node, BinaryError> {
    let mut dec = decoder::Decoder::new(data);
    let node = dec.read_node()?;
    if !dec.exhausted() {
        return Err(BinaryError::LeftoverBytes(dec.remaining()));
    }
    Ok(node)
}
