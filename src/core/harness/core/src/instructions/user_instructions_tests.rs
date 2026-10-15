use super::*;
use ctox_protocol::models::ContentItem;
use pretty_assertions::assert_eq;


#[test]
fn test_is_user_instructions() {
    assert!(AGENTS_MD_FRAGMENT.matches_text(
        "# AGENTS.md instructions for test_directory\n\n<INSTRUCTIONS>\ntest_text\n</INSTRUCTIONS>"
    ));
    assert!(!AGENTS_MD_FRAGMENT.matches_text("test_text"));
}


#[test]
fn test_is_skill_instructions() {
    assert!(SKILL_FRAGMENT.matches_text(
        "<skill>\n<name>demo-skill</name>\n<path>skills/demo/SKILL.md</path>\nbody\n</skill>"
    ));
    assert!(!SKILL_FRAGMENT.matches_text("regular text"));
}
