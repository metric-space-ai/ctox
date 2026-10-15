// ref: internal/util/provider.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: adapted_to_ctox
// License: MIT (upstream); modifications AGPL-3.0-only

use super::{
    get_openai_compatibility_config, get_provider_name, hide_api_key, in_array,
    is_openai_compatibility_alias, mask_authorization_header, mask_sensitive_header_value,
    mask_sensitive_query, openai_compatible_provider_key, resolve_auto_model, ModelRegistryView,
    OpenAiCompatibilityEntryView, OpenAiCompatibilityModelView,
};

struct Registry {
    providers: Vec<String>,
    first: Result<String, ()>,
}

impl ModelRegistryView for Registry {
    type Error = ();

    fn model_providers(&self, _model_name: &str) -> Vec<String> {
        self.providers.clone()
    }

    fn first_available_model(&self, handler_type: &str) -> Result<String, Self::Error> {
        assert!(handler_type.is_empty());
        self.first.clone()
    }
}

#[derive(Debug, PartialEq, Eq)]
struct Model(&'static str);

impl OpenAiCompatibilityModelView for Model {
    fn alias(&self) -> &str {
        self.0
    }
}

#[derive(Debug)]
struct Compatibility {
    disabled: bool,
    models: Vec<Model>,
}

impl OpenAiCompatibilityEntryView for Compatibility {
    type Model = Model;

    fn disabled(&self) -> bool {
        self.disabled
    }

    fn models(&self) -> &[Self::Model] {
        &self.models
    }
}

#[test]
fn compatible_provider_key_matches_normalization_and_idempotence() {
    assert_eq!(openai_compatible_provider_key(""), "openai-compatibility");
    assert_eq!(
        openai_compatible_provider_key(" OpenAI-Compatibility "),
        "openai-compatibility"
    );
    assert_eq!(
        openai_compatible_provider_key(" OpenAI-Compatible-Acme "),
        "openai-compatible-acme"
    );
    assert_eq!(
        openai_compatible_provider_key(" AcMe "),
        "openai-compatible-acme"
    );
    assert_eq!(openai_compatible_provider_key("İ"), "openai-compatible-i");
}

#[test]
fn provider_lookup_preserves_registry_order_case_and_exact_deduplication() {
    let registry = Registry {
        providers: vec![
            String::new(),
            "claude".into(),
            "Claude".into(),
            "claude".into(),
            "codex".into(),
        ],
        first: Err(()),
    };

    assert_eq!(
        get_provider_name("model", &registry),
        vec!["claude", "Claude", "codex"]
    );
    assert!(get_provider_name("", &registry).is_empty());
}

#[test]
fn auto_resolution_is_exact_and_fails_closed_to_original() {
    let found = Registry {
        providers: Vec::new(),
        first: Ok("first-model".into()),
    };
    let failed = Registry {
        providers: Vec::new(),
        first: Err(()),
    };

    assert_eq!(resolve_auto_model("auto", &found), "first-model");
    assert_eq!(resolve_auto_model("auto", &failed), "auto");
    assert_eq!(resolve_auto_model("AUTO", &found), "AUTO");
}

#[test]
fn compatibility_alias_skips_disabled_and_returns_first_enabled_match() {
    let config = vec![
        Compatibility {
            disabled: true,
            models: vec![Model("alias")],
        },
        Compatibility {
            disabled: false,
            models: vec![Model("alias"), Model("other")],
        },
        Compatibility {
            disabled: false,
            models: vec![Model("alias")],
        },
    ];

    assert!(is_openai_compatibility_alias("alias", Some(&config)));
    assert!(!is_openai_compatibility_alias("Alias", Some(&config)));
    assert!(!is_openai_compatibility_alias::<Compatibility>(
        "alias", None
    ));
    let (entry, model) =
        get_openai_compatibility_config("alias", Some(&config)).expect("enabled match");
    assert!(std::ptr::eq(entry, &config[1]));
    assert_eq!(model, &Model("alias"));
}

#[test]
fn key_masking_matches_go_byte_thresholds_including_split_utf8() {
    assert_eq!(hide_api_key(b"ab"), b"ab");
    assert_eq!(hide_api_key(b"abc"), b"a...c");
    assert_eq!(hide_api_key(b"abcde"), b"ab...de");
    assert_eq!(hide_api_key(b"123456789"), b"1234...6789");

    let split = hide_api_key("aébcde".as_bytes());
    assert_eq!(split, vec![b'a', 0xc3, b'.', b'.', b'.', b'd', b'e']);
    assert!(std::str::from_utf8(&split).is_err());
}

#[test]
fn header_masking_preserves_auth_scheme_and_go_space_behavior() {
    assert_eq!(
        mask_authorization_header(" Bearer abcdefghi "),
        b"Bearer abcd...fghi"
    );
    assert_eq!(
        mask_authorization_header("Bearer  abcdef"),
        b"Bearer  a...ef"
    );
    assert_eq!(mask_authorization_header("abcde"), b"ab...de");
    assert_eq!(
        mask_sensitive_header_value(" X-API-KEY ", "123456789"),
        b"1234...6789"
    );
    assert_eq!(
        mask_sensitive_header_value("x-refresh-token", "abcdef"),
        b"ab...ef"
    );
    assert_eq!(mask_sensitive_header_value("X-Trace", "abcdef"), b"abcdef");
}

#[test]
fn query_masking_preserves_raw_structure_and_go_url_rules() {
    assert_eq!(
        mask_sensitive_query("plain=a+b&auth_token=%20abcdef%20&x=1"),
        "plain=a+b&auth_token=ab...ef&x=1"
    );
    assert_eq!(
        mask_sensitive_query("api%5Fkey=123456789&items%5B%5D=keep"),
        "api%5Fkey=1234...6789&items%5B%5D=keep"
    );
    assert_eq!(mask_sensitive_query("token"), "token=");
    assert_eq!(mask_sensitive_query("token=a%2Fb+c"), "token=a%2F...+c");
    assert_eq!(mask_sensitive_query("token=%zz"), "token=%25...z");
    assert_eq!(mask_sensitive_query("ordinary=%zz&x=1"), "ordinary=%zz&x=1");
    assert_eq!(mask_sensitive_query("a=1&&b=2"), "a=1&&b=2");
}
