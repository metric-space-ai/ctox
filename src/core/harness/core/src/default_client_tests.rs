use super::*;
use pretty_assertions::assert_eq;





#[test]
fn test_invalid_suffix_is_sanitized() {
    let prefix = "ctox_cli_rs/0.0.0";
    let suffix = "bad\rsuffix";

    assert_eq!(
        sanitize_user_agent(format!("{prefix} ({suffix})"), prefix),
        "ctox_cli_rs/0.0.0 (bad_suffix)"
    );
}

#[test]
fn test_invalid_suffix_is_sanitized2() {
    let prefix = "ctox_cli_rs/0.0.0";
    let suffix = "bad\0suffix";

    assert_eq!(
        sanitize_user_agent(format!("{prefix} ({suffix})"), prefix),
        "ctox_cli_rs/0.0.0 (bad_suffix)"
    );
}
