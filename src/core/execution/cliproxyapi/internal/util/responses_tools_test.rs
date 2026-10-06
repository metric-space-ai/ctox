// ref: internal/util/responses_tools_test.go:1-269
// Upstream: d7914afdedca7af95ee974a42453dc49fc1388ce
// License: MIT (upstream); modifications AGPL-3.0-only
use super::{
    collect_responses_tool_descriptors, collect_responses_tool_winners,
    qualify_responses_namespace_tool_name, responses_tool_reverse_identity_map,
};
use crate::internal::runtime::executor::helps::{
    apply_patch_original_request, apply_patch_requested, is_apply_patch_upstream_tool,
};
use crate::sdk::pluginapi::ExecutorRequest;

#[test]
fn candidate_responses_tools_preserves_namespace_sources_and_owned_raw_descriptors() {
    let original = br#"{
      "tools":[
        {"type":"namespace","name":" functions ","tools":[
          {"type":"function","name":" exec ","parameters":{"n":9007199254740993,"big":1e400}},
          {"type":"custom","name":"apply_patch"},
          {"type":"namespace","name":"nested","tools":[{"name":"ignored"}]}
        ]},
        {"type":"namespace","name":"empty","tools":[],"children":[{"name":"ignored"}]},
        {"type":"namespace","name":"fallback","tools":null,"children":[{"function":{"name":" child "}}]},
        {"type":"web_search","name":"ignored"}
      ],
      "input":[
        {"type":"additional_tools","tools":[{"type":"function","name":"late"}]},
        {"type":" additional_tools ","tools":[{"name":"ignored"}]}
      ]
    }"#;
    let root = gjson::parse(std::str::from_utf8(original).unwrap());
    let descriptors = collect_responses_tool_descriptors(&root);
    assert_eq!(
        descriptors
            .iter()
            .map(|tool| tool.name.as_str())
            .collect::<Vec<_>>(),
        [
            "functions__exec",
            "functions__apply_patch",
            "fallback__child",
            "late"
        ]
    );
    assert_eq!(descriptors[0].local_name, "exec");
    assert_eq!(descriptors[0].namespace, "functions");
    assert_eq!(descriptors[0].source_priority, 0);
    assert!(!descriptors[0].direct);
    assert_eq!(descriptors[3].source_priority, 1);
    assert!(descriptors[3].direct);
    assert!(descriptors[1].apply_patch);
    let raw = std::str::from_utf8(&descriptors[0].tool_json).unwrap();
    assert_eq!(gjson::get(raw, "parameters.n").json(), "9007199254740993");
    assert_eq!(gjson::get(raw, "parameters.big").json(), "1e400");
    for (namespace, child, expected) in [
        (" ns ", " tool ", "ns__tool"),
        ("ns", "ns__tool", "ns__tool"),
        ("ns", "mcp__server__tool", "mcp__server__tool"),
        ("ns__", "tool", "ns__tool"),
        ("ns", "ns", "ns"),
        ("", " tool ", "tool"),
        ("ns", "", ""),
    ] {
        assert_eq!(
            qualify_responses_namespace_tool_name(namespace, child),
            expected
        );
    }
}

#[test]
fn candidate_responses_tools_winner_priority_controls_original_patch_identity() {
    let original = br#"{"tools":[
      {"type":"namespace","name":"ns","tools":[{"type":"custom","name":"apply_patch"}]},
      {"type":"function","name":"ns__apply_patch"},
      {"type":"function","name":"apply_patch"}
    ],"input":[{"type":"additional_tools","tools":[
      {"type":"custom","name":"apply_patch"},
      {"type":"custom","name":"ns__apply_patch"}
    ]}]}"#;
    let root = gjson::parse(std::str::from_utf8(original).unwrap());
    let winners = collect_responses_tool_winners(&root);
    assert!(winners["ns__apply_patch"].direct);
    assert_eq!(winners["apply_patch"].source_priority, 0);
    assert!(!apply_patch_requested(original));
    assert!(!is_apply_patch_upstream_tool(original, "apply_patch"));
    assert!(!is_apply_patch_upstream_tool(original, "ns__apply_patch"));

    let custom_first = br#"{"tools":[{"type":"custom","name":"apply_patch"},{"type":"function","name":"apply_patch"}]}"#;
    let function_first = br#"{"tools":[{"type":"function","name":"apply_patch"},{"type":"custom","name":"apply_patch"}]}"#;
    assert!(apply_patch_requested(custom_first));
    assert!(!apply_patch_requested(function_first));
    let namespace_first = br#"{"tools":[{"type":"namespace","name":"ns","tools":[{"type":"function","name":"apply_patch"}]},{"type":"custom","name":"ns__apply_patch"}]}"#;
    // A custom tool merely called ns__apply_patch is not the original apply_patch contract.
    assert!(!apply_patch_requested(namespace_first));
    let top_namespace = br#"{"tools":[{"type":"namespace","name":"ns","tools":[{"type":"custom","name":"apply_patch"}]}],"input":[{"type":"additional_tools","tools":[{"type":"function","name":"ns__apply_patch"}]}]}"#;
    assert!(is_apply_patch_upstream_tool(
        top_namespace,
        "ns__apply_patch"
    ));
    let additional_only = br#"{"input":[{"type":"additional_tools","tools":[{"type":"custom","name":"apply_patch"}]}]}"#;
    assert!(apply_patch_requested(additional_only));
}

#[test]
fn candidate_responses_tools_collision_aliases_are_stable_and_bounded() {
    let original = br#"{"tools":[
      {"type":"function","name":"a/b"},{"type":"function","name":"a_b"},
      {"type":"namespace","name":"patch/space","tools":[{"type":"custom","name":"apply_patch"}]}
    ]}"#;
    let reordered = br#"{"tools":[
      {"type":"namespace","name":"patch/space","tools":[{"type":"custom","name":"apply_patch"}]},
      {"type":"function","name":"a_b"},{"type":"function","name":"a/b"}
    ]}"#;
    let map = responses_tool_reverse_identity_map(original);
    assert_eq!(map, responses_tool_reverse_identity_map(reordered));
    let aliases = map
        .iter()
        .filter(|(wire, identity)| wire.as_str() != identity.name && identity.namespace.is_empty())
        .collect::<Vec<_>>();
    assert_eq!(aliases.len(), 2);
    for (wire, _) in aliases {
        assert!(wire.starts_with("a_b_"));
        assert_eq!(wire.len(), 16);
    }
    assert!(is_apply_patch_upstream_tool(
        original,
        "patch_space__apply_patch"
    ));
    assert!(is_apply_patch_upstream_tool(
        original,
        "patch/space__apply_patch"
    ));
    assert!(!is_apply_patch_upstream_tool(original, "apply_patch"));
    let prefix = "x".repeat(70);
    let long_a = format!("{prefix}/a");
    let long_b = format!("{prefix}/b");
    let raw = serde_json::to_vec(&serde_json::json!({"tools":[
        {"type":"function","name":long_a},{"type":"function","name":long_b}
    ]}))
    .unwrap();
    let names = responses_tool_reverse_identity_map(&raw);
    let wire = names
        .keys()
        .filter(|name| name.len() <= 64)
        .collect::<Vec<_>>();
    assert_eq!(wire.len(), 2);
    assert!(wire.iter().all(|name| name.len() == 64));
    assert_ne!(wire[0], wire[1]);
    assert!(names.contains_key(&long_a));
    assert!(names.contains_key(&long_b));
}

#[test]
fn candidate_responses_tools_wrapper_and_exact_custom_declaration_are_required() {
    let wrapped = br#"{"tools":[{"type":"function","name":"apply_patch"}],"request":{"model":null,"tools":[{"type":"custom","name":"apply_patch"}]}}"#;
    assert!(apply_patch_requested(wrapped));
    let irrelevant_wrapper =
        br#"{"tools":[{"type":"custom","name":"apply_patch"}],"request":{"other":true}}"#;
    assert!(apply_patch_requested(irrelevant_wrapper));
    for raw in [
        br#"{"tools":[{"type":" custom ","name":"apply_patch"}]}"#.as_slice(),
        br#"{"tools":[{"type":"custom","function":{"name":"apply_patch"}}]}"#.as_slice(),
        br#"{"tools":[{"type":"function","name":"apply_patch"}]}"#.as_slice(),
        br#"{"tools":[{"type":"custom","name":"not_apply_patch"}]}"#.as_slice(),
        br#"{"tools":[{"type":"custom","name":"apply_patch"}]}"#.split_last().unwrap().1,
        br#"{"request":null}"#.as_slice(),
        b"null",
    ] {
        assert!(!apply_patch_requested(raw), "{raw:?}");
    }
    assert!(apply_patch_requested(
        br#"{"tools":[{"type":"custom","name":" apply_patch "}]}"#
    ));
    let request = ExecutorRequest {
        payload: br#"{"tools":[{"type":"custom","name":"apply_patch"}]}"#.to_vec(),
        ..ExecutorRequest::default()
    };
    assert!(apply_patch_requested(apply_patch_original_request(
        &request
    )));
    let request = ExecutorRequest {
        original_request: br#"{"tools":[{"type":"function","name":"apply_patch"}]}"#.to_vec(),
        ..request
    };
    assert!(!apply_patch_requested(apply_patch_original_request(
        &request
    )));
}

#[test]
fn candidate_responses_tools_deep_schema_keeps_raw_bytes_and_private_debug() {
    let depth = 20_000;
    let mut original = br#"{"tools":[{"type":"custom","name":"apply_patch","description":"private-marker-123","parameters":{"n":1e400,"schema":"#.to_vec();
    original.extend(std::iter::repeat_n(b'[', depth));
    original.extend_from_slice(b"null");
    original.extend(std::iter::repeat_n(b']', depth));
    original.extend_from_slice(b"}}]}");
    let map = responses_tool_reverse_identity_map(&original);
    assert!(map["apply_patch"].apply_patch);
    let root = gjson::parse(std::str::from_utf8(&original).unwrap());
    let descriptors = collect_responses_tool_descriptors(&root);
    let owned = descriptors[0].clone();
    assert!(std::str::from_utf8(&owned.tool_json)
        .unwrap()
        .contains("\"n\":1e400"));
    assert!(!format!("{owned:?}").contains("private-marker-123"));
    drop(descriptors);
    drop(root);
    original.fill(b' ');
    assert!(std::str::from_utf8(&owned.tool_json)
        .unwrap()
        .contains("private-marker-123"));
    assert!(owned.apply_patch);
}
