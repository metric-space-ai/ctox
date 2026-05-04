//! Tiny demo: build a `<iq type="get" />` node and round-trip it through the
//! WhatsApp binary codec. Run with `cargo run --example encode_iq`.

use whatsapp::binary::{Attrs, Node, Value};

fn main() {
    let node = Node::new(
        "iq",
        Attrs::from([
            ("type".into(), Value::String("get".into())),
            ("id".into(), Value::String("abc123".into())),
            ("to".into(), Value::String("s.whatsapp.net".into())),
        ]),
        None,
    );
    let bytes = whatsapp::binary::marshal(&node).expect("encode");
    let decoded = whatsapp::binary::unmarshal(&bytes).expect("decode");
    println!("encoded {} bytes; decoded tag = {}", bytes.len(), decoded.tag);
    assert_eq!(decoded.tag, "iq");
}
