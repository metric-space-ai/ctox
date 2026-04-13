#[path = "../harness/mod.rs"]
mod harness;

use harness::TestRoot;

/// Build a helper that invokes the ctox binary in `--tui-smoke` mode.
/// The binary renders one frame to stdout as plain-text then exits.
fn tui_smoke(root: &TestRoot, page: &str, width: u16, height: u16) -> String {
    let output = root.run(&[
        "tui-smoke",
        page,
        &width.to_string(),
        &height.to_string(),
    ]);
    output.success();
    output.stdout()
}

#[test]
fn tui_smoke_chat_renders() {
    let root = TestRoot::new("tui-smoke-chat");
    let buf = tui_smoke(&root, "chat", 120, 40);
    assert!(buf.contains("CTOX"), "header must contain CTOX branding");
    assert!(buf.contains("Chat"), "tabs must contain Chat label");
}

#[test]
fn tui_smoke_skills_renders() {
    let root = TestRoot::new("tui-smoke-skills");
    let buf = tui_smoke(&root, "skills", 120, 40);
    assert!(buf.contains("Skills"), "tabs must contain Skills label");
}

#[test]
fn tui_smoke_settings_renders() {
    let root = TestRoot::new("tui-smoke-settings");
    let buf = tui_smoke(&root, "settings", 120, 40);
    assert!(buf.contains("Settings"), "tabs must contain Settings label");
}

#[test]
fn tui_smoke_narrow_chat_renders() {
    let root = TestRoot::new("tui-smoke-narrow");
    let buf = tui_smoke(&root, "chat", 60, 24);
    assert!(buf.contains("CTOX"), "narrow layout must still render header");
}

#[test]
fn tui_smoke_wide_chat_renders() {
    let root = TestRoot::new("tui-smoke-wide");
    let buf = tui_smoke(&root, "chat", 200, 50);
    assert!(buf.contains("Chat"), "wide layout must render tabs");
}
