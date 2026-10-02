// ref: internal/util/claude_schema_test.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only

use super::{
    has_unsupported_unicode_property_escape, normalize_claude_tool_input_schema,
    strip_unsupported_schema_patterns,
};

#[test]
fn pinned_schema_cases_match_upstream() {
    let cases = [
        (
            r#"{"anyOf":[{"type":"object","properties":{"a":{"type":"string"}}},{"type":"object","properties":{"b":{"type":"integer"}}}]}"#,
            r#"{"type":"object","properties":{"a":{"type":"string"},"b":{"type":"integer"}}}"#,
        ),
        (
            r#"{"type":"object","properties":{"nested":{"oneOf":[{"type":"string"},{"type":"number"}]}},"oneOf":[{"properties":{"a":{"type":"string"}},"required":["a"]},{"properties":{"b":{"type":"string"}},"required":["b"]}]}"#,
            r#"{"type":"object","properties":{"nested":{"oneOf":[{"type":"string"},{"type":"number"}]},"a":{"type":"string"},"b":{"type":"string"}}}"#,
        ),
        (
            r#"{"type":"object","properties":{"a":{"type":"string"},"b":{"type":"string"}},"anyOf":[{"required":["a"]},{"required":["b"]}]}"#,
            r#"{"type":"object","properties":{"a":{"type":"string"},"b":{"type":"string"}}}"#,
        ),
        (
            r#"{"type":"object","properties":{"base":{"type":"boolean"}},"required":["base"],"allOf":[{"type":"object","properties":{"a":{"type":"string"}},"required":["a"]},{"properties":{"b":{"type":"integer"}},"required":["a","b"]}]}"#,
            r#"{"type":"object","properties":{"base":{"type":"boolean"},"a":{"type":"string"},"b":{"type":"integer"}},"required":["base","a","b"]}"#,
        ),
        (
            r#"{"type":"object","properties":{"query":{"type":"string"}},"required":["query"],"additionalProperties":false}"#,
            r#"{"type":"object","properties":{"query":{"type":"string"}},"required":["query"],"additionalProperties":false}"#,
        ),
        (r#"{"type":"#, r#"{"type":"object","properties":{}}"#),
        ("true", r#"{"type":"object","properties":{}}"#),
    ];

    for (input, expected) in cases {
        let actual = normalize_claude_tool_input_schema(input.as_bytes());
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&actual).unwrap(),
            serde_json::from_str::<serde_json::Value>(expected).unwrap(),
            "input={input}"
        );
    }
}

#[test]
fn bytes_key_order_and_go_escaping_are_exact() {
    let input = br#" { "z": "<>&\u2028", "description" : "x y", "x-num":1.00, "properties" : { "b" : { "type" : "string", "x":"<" }, "a":{"type":"integer"} } } "#;
    assert_eq!(
        normalize_claude_tool_input_schema(input),
        br#"{"description":"x y","properties":{"a":{"type":"integer"},"b":{"type":"string","x":"\u003c"}},"type":"object","x-num":1.00,"z":"\u003c\u003e\u0026\u2028"}"#
    );

    let input = br#"{"properties":{"a":{"type":"string","default":1.00}},"anyOf":[ { "properties" : {"b":{"const":"<&"}}} ]}"#;
    assert_eq!(
        normalize_claude_tool_input_schema(input),
        br#"{"properties":{"a":{"type":"string","default":1.00},"b":{"const":"\u003c\u0026"}},"type":"object"}"#
    );

    let input = br#"{"required":["<root>"],"allOf":[{"required":["a&b\u2028"]}]}"#;
    assert_eq!(
        normalize_claude_tool_input_schema(input),
        br#"{"properties":{},"required":["\u003croot\u003e","a\u0026b\u2028"],"type":"object"}"#
    );
}

#[test]
fn union_filtering_collision_and_required_rules_are_exact() {
    let input = br#"{
      "properties":{"same":{"from":"root"}},
      "required":"invalid",
      "anyOf":[
        {"type":"string","properties":{"skip":{}}},
        {"type":["null","object"],"properties":{"same":{"from":"branch"},"a":{}}},
        false
      ],
      "oneOf":{"not":"an array"},
      "allOf":[
        {"properties":{"b":{}},"required":[]},
        {"type":["object",1],"properties":{"skip2":{}}},
        {"properties":{"c":{}},"required":["a","a","b"]},
        {"properties":{"d":{}},"required":["b","c"]}
      ]
    }"#;
    assert_eq!(
        normalize_claude_tool_input_schema(input),
        br#"{"properties":{"a":{},"b":{},"c":{},"d":{},"same":{"from":"root"}},"required":["a","b","c"],"type":"object"}"#
    );
}

#[test]
fn properties_and_unions_do_not_add_placeholder_or_change_additional_properties() {
    let input = br#"{
      "additionalProperties":false,
      "properties":null,
      "allOf":[
        {"additionalProperties":true,"properties":{"nested":{"type":"object","properties":{},"additionalProperties":false}}}
      ]
    }"#;
    let output = normalize_claude_tool_input_schema(input);
    assert_eq!(
        output,
        br#"{"additionalProperties":false,"properties":{"nested":{"type":"object","properties":{},"additionalProperties":false}},"type":"object"}"#
    );
    let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert!(value.pointer("/properties/reason").is_none());
    assert!(value.pointer("/properties/_").is_none());
}

#[test]
fn empty_non_object_and_lossy_utf8_boundaries_fall_back_like_go() {
    for input in [&b""[..], b"null", b"[]", b"false", b"123", b"{broken"] {
        assert_eq!(
            normalize_claude_tool_input_schema(input),
            br#"{"type":"object","properties":{}}"#
        );
    }
    assert_eq!(
        normalize_claude_tool_input_schema(b"{\"description\":\"\xff\"}"),
        "{\"description\":\"�\",\"properties\":{},\"type\":\"object\"}".as_bytes()
    );
}

#[test]
fn unsupported_unicode_property_escapes_match_upstream() {
    let cases = [
        (
            r#"^(?!__.*__$)[^\p{Cc}\p{Cf}\p{Zl}\p{Zp}"\./[\]]{1,200}$"#,
            true,
        ),
        (r#"^\P{L}+$"#, true),
        (r#"^\\p{Cc}$"#, false),
        (r#"^\\\p{Cc}$"#, true),
        (r#"^\\\\p{Cc}$"#, false),
        (r#"^[0-9a-f]{32}$"#, false),
        (r#"^(?!__.*__$).{1,200}$"#, false),
        (r#"^(a)\1$"#, false),
        (r#"^[^\0]*$"#, true),
        (r#"^[^\x00]*$"#, false),
        (r#"^\\0$"#, false),
        (r#"abc\"#, false),
        (r#"\p"#, false),
        (r#"\p{"#, true),
        ("", false),
    ];
    for (pattern, want) in cases {
        assert_eq!(
            has_unsupported_unicode_property_escape(pattern),
            want,
            "{pattern:?}"
        );
    }
}

#[test]
fn pattern_strip_is_schema_aware() {
    let mut value = serde_json::json!({
        "properties": {
            "real_schema": {"type": "string", "pattern": r"\p{L}+"},
            "regex_config": {
                "type": "object",
                "default": {"pattern": r"\p{L}+"},
                "enum": [{"pattern": r"\p{N}+"}]
            }
        },
        "patternProperties": {
            r"^\p{L}+$": {"type": "string"},
            "^[a-z]+$": {"type": "number", "pattern": r"^[^\0]*$"}
        }
    });
    assert!(strip_unsupported_schema_patterns(&mut value));
    assert!(value["properties"]["real_schema"].get("pattern").is_none());
    assert_eq!(
        value["properties"]["regex_config"]["default"]["pattern"],
        r"\p{L}+"
    );
    assert_eq!(
        value["properties"]["regex_config"]["enum"][0]["pattern"],
        r"\p{N}+"
    );
    assert!(value["patternProperties"].get(r"^\p{L}+$").is_none());
    assert!(value["patternProperties"]["^[a-z]+$"].get("pattern").is_none());
    assert_eq!(value["patternProperties"]["^[a-z]+$"]["type"], "number");
}
