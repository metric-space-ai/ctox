//! Reverse-direction interop: take bytes our Rust encoder produces and feed
//! them to upstream whatsmeow's `Unmarshal` via a small Go helper. This is
//! the test that proves our encoder isn't just internally consistent — it
//! produces wire bytes the original library accepts.
//!
//! The Go helper lives at `_upstream/verify_rust/`; build it with `go build`
//! before running. The test silently skips when the binary isn't present so
//! CI environments without Go don't fail.

use std::path::PathBuf;
use std::process::Command;

use wha_binary::{marshal, Attrs, Node, Value};
use wha_types::Jid;

fn verifier_path() -> Option<PathBuf> {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let p = workspace_root.join("_upstream/verify_rust/verify_rust");
    if p.exists() { Some(p) } else { None }
}

fn upstream_describe(bytes: &[u8]) -> Option<String> {
    let bin = verifier_path()?;
    let hex_in = bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
    let out = Command::new(bin)
        .arg(&hex_in)
        .output()
        .expect("run verify_rust");
    if !out.status.success() {
        panic!(
            "verify_rust failed: stderr=`{}` stdout=`{}` for hex `{}`",
            String::from_utf8_lossy(&out.stderr),
            String::from_utf8_lossy(&out.stdout),
            hex_in,
        );
    }
    Some(String::from_utf8(out.stdout).expect("utf8").trim().to_owned())
}

#[test]
fn upstream_decodes_our_empty_iq() {
    let Some(_) = verifier_path() else { return; };
    let bytes = marshal(&Node::tag_only("iq")).unwrap();
    assert_eq!(upstream_describe(&bytes).unwrap(), "tag=iq");
}

#[test]
fn upstream_decodes_our_iq_with_string_attrs() {
    let Some(_) = verifier_path() else { return; };
    let mut attrs = Attrs::new();
    attrs.insert("type".into(), Value::String("get".into()));
    attrs.insert("id".into(), Value::String("abc".into()));
    attrs.insert("to".into(), Value::String("s.whatsapp.net".into()));
    let bytes = marshal(&Node::new("iq", attrs, None)).unwrap();
    let desc = upstream_describe(&bytes).unwrap();
    assert_eq!(desc, "tag=iq attr[id]=str:abc attr[to]=str:s.whatsapp.net attr[type]=str:get");
}

#[test]
fn upstream_decodes_our_message_with_jid() {
    let Some(_) = verifier_path() else { return; };
    let mut attrs = Attrs::new();
    attrs.insert("to".into(), Value::Jid(Jid::new("12345", "s.whatsapp.net")));
    let bytes = marshal(&Node::new("message", attrs, None)).unwrap();
    let desc = upstream_describe(&bytes).unwrap();
    assert_eq!(desc, "tag=message attr[to]=jid:12345@s.whatsapp.net");
}

#[test]
fn upstream_decodes_our_ad_jid_with_device() {
    let Some(_) = verifier_path() else { return; };
    let mut attrs = Attrs::new();
    attrs.insert("from".into(), Value::Jid(Jid::new_ad("12345", 0, 7)));
    let bytes = marshal(&Node::new("ack", attrs, None)).unwrap();
    let desc = upstream_describe(&bytes).unwrap();
    assert_eq!(desc, "tag=ack attr[from]=jid:12345:7@s.whatsapp.net");
}

#[test]
fn upstream_decodes_our_nested_children() {
    let Some(_) = verifier_path() else { return; };
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
    let bytes = marshal(&parent).unwrap();
    let desc = upstream_describe(&bytes).unwrap();
    assert_eq!(desc, "tag=parent children=1 <tag=child attr[k]=str:v>");
}

#[test]
fn upstream_decodes_our_binary_payload() {
    let Some(_) = verifier_path() else { return; };
    let n = Node::new("enc", Attrs::new(), Some(Value::Bytes(vec![1, 2, 3, 4, 5])));
    let bytes = marshal(&n).unwrap();
    let desc = upstream_describe(&bytes).unwrap();
    assert_eq!(desc, "tag=enc bytes=0102030405");
}
