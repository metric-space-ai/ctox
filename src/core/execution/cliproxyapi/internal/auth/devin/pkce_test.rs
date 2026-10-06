use super::*;

#[test]
fn candidate_devin_oauth_pkce_matches_fixed_64_byte_s256_golden() {
    let bytes = std::array::from_fn(|index| index as u8);
    let codes = pkce_from_bytes(bytes);
    assert_eq!(
        codes.code_verifier,
        "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKissLS4vMDEyMzQ1Njc4OTo7PD0-Pw"
    );
    assert_eq!(
        codes.code_challenge,
        "wsNdZaf3VpLTsEDmR5gPk2C6xYVWxKb0xcaG3O6kX10"
    );
    assert_eq!(codes.code_verifier.len(), 86);
    assert_eq!(codes.code_challenge.len(), 43);
}
#[test]
fn candidate_devin_oauth_pkce_uses_fresh_entropy_and_redacts_debug() {
    let first = generate_pkce_codes().unwrap();
    let second = generate_pkce_codes().unwrap();
    assert_ne!(first.code_verifier, second.code_verifier);
    assert_eq!(first.code_verifier.len(), 86);
    assert!(first
        .code_verifier
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_')));
    let debug = format!("{first:?}");
    assert!(!debug.contains(&first.code_verifier));
    assert!(!debug.contains(&first.code_challenge));
}
