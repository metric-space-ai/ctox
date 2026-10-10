//! Port of `nektos/act` `pkg/schema`, part 1: the schema model.
//!
//! act does not use JSON Schema proper. It uses a small custom schema language
//! expressed in JSON: a set of named definitions, each of which is a mapping,
//! a sequence, a scalar, or a `one-of` choice over those. The definitions
//! live in `schemas/workflow_schema.json` (86 KB) and
//! `schemas/action_schema.json` (5.7 KB), copied verbatim from upstream.
//!
//! This module covers everything that does not need a parsed YAML tree: the
//! schema structures, the implicit definitions, and the function table.
//!
//! **Not yet ported.** Upstream's `schema.Node` walks a `yaml.Node` and checks
//! it against these definitions, and it validates `${{ ... }}` expressions with
//! `github.com/rhysd/actionlint`. Both need the YAML backend and the expression
//! parser, which are the two open architectural decisions. See
//! `docs/dev/ctox-actions-runner-port.md`.
//!
//! Upstream: <https://github.com/nektos/act> (MIT, Copyright (c) Christoph Schitt).

use std::collections::BTreeMap;
use std::fmt;
use std::sync::OnceLock;

use regex::Regex;
use serde::{Deserialize, Deserializer};

const WORKFLOW_SCHEMA: &str = include_str!("../schemas/workflow_schema.json");
const ACTION_SCHEMA: &str = include_str!("../schemas/action_schema.json");

/// The complete set of named definitions.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Schema {
    #[serde(default)]
    pub definitions: BTreeMap<String, Definition>,
}

/// One named definition: exactly one of the shape arms is present.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct Definition {
    #[serde(default)]
    pub context: Vec<String>,
    pub mapping: Option<MappingDefinition>,
    pub sequence: Option<SequenceDefinition>,
    #[serde(rename = "one-of")]
    pub one_of: Option<Vec<String>>,
    #[serde(rename = "allowed-values")]
    pub allowed_values: Option<Vec<String>>,
    pub string: Option<StringDefinition>,
    pub number: Option<NumberDefinition>,
    pub boolean: Option<BooleanDefinition>,
    pub null: Option<NullDefinition>,
}

/// A mapping shape: named properties plus a loose key/value type.
///
/// Upstream leaves both loose types as the Go zero value when absent, and 48
/// of the definitions in `workflow_schema.json` omit them entirely, so both
/// default to the empty string here.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct MappingDefinition {
    #[serde(default)]
    pub properties: BTreeMap<String, MappingProperty>,
    #[serde(default, rename = "loose-key-type")]
    pub loose_key_type: String,
    #[serde(default, rename = "loose-value-type")]
    pub loose_value_type: String,
}

/// A named property: either a bare type name or `{ type, required }`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MappingProperty {
    /// The definition name the property's value must match.
    pub type_name: String,
    /// Whether the property may be omitted.
    pub required: bool,
}

impl<'de> Deserialize<'de> for MappingProperty {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Repr {
            Bare(String),
            Detailed {
                #[serde(rename = "type")]
                type_name: String,
                #[serde(default)]
                required: bool,
            },
        }

        match Repr::deserialize(deserializer)? {
            Repr::Bare(type_name) => Ok(Self {
                type_name,
                required: false,
            }),
            Repr::Detailed { type_name, required } => Ok(Self { type_name, required }),
        }
    }
}

/// A sequence shape.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct SequenceDefinition {
    #[serde(default, rename = "item-type")]
    pub item_type: String,
}

/// A string shape, optionally pinned to a constant or an expression.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct StringDefinition {
    #[serde(default)]
    pub constant: String,
    #[serde(rename = "is-expression", default)]
    pub is_expression: bool,
}

/// Number shape. Carries no fields upstream either, and always appears as
/// `{}` in the schema documents, so the marker accepts and ignores any object.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NumberDefinition;

/// Boolean shape. As [`NumberDefinition`], always `{}`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BooleanDefinition;

/// Null shape. As [`NumberDefinition`], always `{}`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NullDefinition;

/// Accepts any YAML/JSON value and discards it, matching Go's behaviour of
/// ignoring unknown fields in these empty structs.
fn accept_ignored<'de, D: Deserializer<'de>, T: Default>(
    deserializer: D,
) -> Result<T, D::Error> {
    serde::de::IgnoredAny::deserialize(deserializer)?;
    Ok(T::default())
}

impl<'de> Deserialize<'de> for NumberDefinition {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        accept_ignored(d)
    }
}

impl<'de> Deserialize<'de> for BooleanDefinition {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        accept_ignored(d)
    }
}

impl<'de> Deserialize<'de> for NullDefinition {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        accept_ignored(d)
    }
}

/// A built-in function and its accepted argument count.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionInfo {
    /// Function name, compared case-insensitively.
    pub name: String,
    /// Minimum argument count.
    pub min: usize,
    /// Maximum argument count.
    pub max: usize,
}

impl Schema {
    /// Looks up a definition, falling back to act's implicit built-ins.
    pub fn definition(&self, name: &str) -> Definition {
        if let Some(found) = self.definitions.get(name) {
            return found.clone();
        }
        match name {
            "any" => Definition {
                one_of: Some(vec![
                    "sequence".into(),
                    "mapping".into(),
                    "number".into(),
                    "boolean".into(),
                    "string".into(),
                    "null".into(),
                ]),
                ..Definition::default()
            },
            "sequence" => Definition {
                sequence: Some(SequenceDefinition {
                    item_type: "any".into(),
                }),
                ..Definition::default()
            },
            "mapping" => Definition {
                mapping: Some(MappingDefinition {
                    loose_key_type: "any".into(),
                    loose_value_type: "any".into(),
                    ..MappingDefinition::default()
                }),
                ..Definition::default()
            },
            "number" => Definition {
                number: Some(NumberDefinition),
                ..Definition::default()
            },
            "string" => Definition {
                string: Some(StringDefinition::default()),
                ..Definition::default()
            },
            "boolean" => Definition {
                boolean: Some(BooleanDefinition),
                ..Definition::default()
            },
            "null" => Definition {
                null: Some(NullDefinition),
                ..Definition::default()
            },
            _ => Definition::default(),
        }
    }
}

/// The workflow schema, parsed once.
pub fn workflow_schema() -> &'static Schema {
    static SCHEMA: OnceLock<Schema> = OnceLock::new();
    SCHEMA.get_or_init(|| {
        serde_json::from_str(WORKFLOW_SCHEMA)
            .expect("embedded workflow_schema.json must be a valid schema document")
    })
}

/// The action (`action.yml`) schema, parsed once.
pub fn action_schema() -> &'static Schema {
    static SCHEMA: OnceLock<Schema> = OnceLock::new();
    SCHEMA.get_or_init(|| {
        serde_json::from_str(ACTION_SCHEMA)
            .expect("embedded action_schema.json must be a valid schema document")
    })
}

/// `name(min,max)` declarations embedded in a definition's `context` list.
fn function_declaration() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^([a-zA-Z0-9_]+)\(([0-9]+),([0-9]+|MAX)\)$")
            .expect("function declaration regex must compile")
    })
}

/// The functions valid in a given context: the built-in set plus any declared
/// through the definition's `context` list.
pub fn functions_for(context: &[String]) -> Vec<FunctionInfo> {
    let mut funcs = vec![
        function("contains", 2, 2),
        function("endsWith", 2, 2),
        function("format", 1, 255),
        function("join", 1, 2),
        function("startsWith", 2, 2),
        function("toJson", 1, 1),
        function("fromJson", 1, 1),
    ];

    for entry in context {
        let Some(captured) = function_declaration().captures(entry) else {
            continue;
        };
        let name = captured.get(1).map(|m| m.as_str()).unwrap_or_default();
        let min: usize = captured
            .get(2)
            .and_then(|m| m.as_str().parse().ok())
            .unwrap_or_default();
        let max_text = captured.get(3).map(|m| m.as_str()).unwrap_or_default();
        let max = if max_text.eq_ignore_ascii_case("MAX") {
            i32::MAX as usize
        } else {
            max_text.parse().unwrap_or_default()
        };
        funcs.push(FunctionInfo {
            name: name.to_string(),
            min,
            max,
        });
    }

    funcs
}

fn function(name: &str, min: usize, max: usize) -> FunctionInfo {
    FunctionInfo {
        name: name.to_string(),
        min,
        max,
    }
}

/// A schema node under validation, together with its context path.
#[derive(Debug, Clone)]
pub struct SchemaNode<'a> {
    /// Name of the definition being checked.
    pub definition: String,
    /// The schema the definition belongs to.
    pub schema: &'a Schema,
    /// Enclosing context path, innermost last.
    pub context: Vec<String>,
}

impl fmt::Display for SchemaNode<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.definition)
    }
}

/// Parses a schema document, surfacing JSON errors as a message.
///
/// Upstream swallows the unmarshal error (`_ = json.Unmarshal(...)`); that is
/// only safe because the input is an embedded constant. This variant is used
/// by the tests and by any future caller that loads a schema from disk.
pub fn parse_schema(document: &str) -> Result<Schema, String> {
    serde_json::from_str(document).map_err(|err| err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workflow_schema_parses() {
        let schema = workflow_schema();
        assert!(
            !schema.definitions.is_empty(),
            "workflow schema must define entries"
        );
        assert!(schema.definitions.contains_key("workflow-root"));
    }

    #[test]
    fn action_schema_parses() {
        let schema = action_schema();
        assert!(schema.definitions.contains_key("action-root"));
    }

    #[test]
    fn schemas_parse_identically_each_call() {
        // The OnceLock must hand back the same instance, not re-parse.
        assert!(std::ptr::eq(workflow_schema(), workflow_schema()));
        assert!(std::ptr::eq(action_schema(), action_schema()));
    }

    #[test]
    fn bare_string_property_is_not_required() {
        let json = r#"{"properties":{"name":"string","x":{"type":"string","required":true}}}"#;
        let mapping: MappingDefinition = serde_json::from_str(json).expect("mapping");
        assert_eq!(mapping.properties["name"].type_name, "string");
        assert!(!mapping.properties["name"].required);
        assert_eq!(mapping.properties["x"].type_name, "string");
        assert!(mapping.properties["x"].required);
    }

    #[test]
    fn definition_arms_use_kebab_case_keys() {
        let json = r#"{
            "one-of": ["string", "null"],
            "allowed-values": ["a", "b"],
            "sequence": {"item-type": "string"},
            "mapping": {"loose-key-type": "non-empty-string", "loose-value-type": "any"},
            "string": {"is-expression": true, "constant": "x"}
        }"#;
        let def: Definition = serde_json::from_str(json).expect("definition");
        assert_eq!(def.one_of.as_deref(), Some(["string".to_string(), "null".to_string()].as_slice()));
        assert_eq!(def.allowed_values.as_ref().map(|v| v.len()), Some(2));
        assert_eq!(def.sequence.as_ref().unwrap().item_type, "string");
        assert_eq!(def.mapping.as_ref().unwrap().loose_key_type, "non-empty-string");
        assert!(def.string.as_ref().unwrap().is_expression);
    }

    #[test]
    fn implicit_definitions_are_available() {
        let schema = Schema::default();
        assert!(schema.definition("any").one_of.is_some());
        assert!(schema.definition("sequence").sequence.is_some());
        assert!(schema.definition("mapping").mapping.is_some());
        assert!(schema.definition("number").number.is_some());
        assert!(schema.definition("string").string.is_some());
        assert!(schema.definition("boolean").boolean.is_some());
        assert!(schema.definition("null").null.is_some());
    }

    #[test]
    fn explicit_definitions_win_over_implicit() {
        let mut schema = Schema::default();
        schema.definitions.insert(
            "string".into(),
            Definition {
                string: Some(StringDefinition {
                    constant: "pinned".into(),
                    is_expression: false,
                }),
                ..Definition::default()
            },
        );

        let resolved = schema.definition("string");
        assert_eq!(resolved.string.unwrap().constant, "pinned");
    }

    #[test]
    fn unknown_definition_is_empty() {
        assert_eq!(Schema::default().definition("nope"), Definition::default());
    }

    #[test]
    fn built_in_functions_are_always_present() {
        let names: Vec<String> = functions_for(&[])
            .into_iter()
            .map(|f| f.name)
            .collect();
        for expected in [
            "contains",
            "endsWith",
            "format",
            "join",
            "startsWith",
            "toJson",
            "fromJson",
        ] {
            assert!(names.contains(&expected.to_string()), "missing {expected}");
        }
    }

    #[test]
    fn context_declares_extra_functions() {
        // `hashFiles(1,255)` is a real signature from workflow_schema.json.
        let context = vec!["hashFiles(1,255)".to_string()];
        let funcs = functions_for(&context);
        let hash = funcs.iter().find(|f| f.name == "hashFiles").expect("hashFiles");
        assert_eq!(hash.min, 1);
        assert_eq!(hash.max, 255);
    }

    #[test]
    fn every_real_context_signature_parses() {
        // The signatures actually present in the embedded schema. `MAX`
        // saturates at i32, matching Go's `math.MaxInt32`.
        for (signature, min, max) in [
            ("always(0,0)", 0, 0),
            ("cancelled(0,0)", 0, 0),
            ("failure(0,0)", 0, 0),
            ("failure(0,MAX)", 0, i32::MAX as usize),
            ("success(0,0)", 0, 0),
            ("success(0,MAX)", 0, i32::MAX as usize),
            ("hashFiles(1,255)", 1, 255),
        ] {
            let funcs = functions_for(&[signature.to_string()]);
            let name = signature.split('(').next().unwrap();
            let parsed = funcs
                .iter()
                .find(|f| f.name == name)
                .unwrap_or_else(|| panic!("{signature} must parse"));
            assert_eq!(parsed.min, min, "{signature} min");
            assert_eq!(parsed.max, max, "{signature} max");
        }
    }

    #[test]
    fn non_numeric_signatures_are_not_declarations() {
        // The upstream regex only accepts digit counts, so `hashFiles(path)`
        // is not a declaration and is silently skipped.
        let context = vec!["hashFiles(path)".to_string()];
        let funcs = functions_for(&context);
        assert!(!funcs.iter().any(|f| f.name == "hashFiles"));
    }

    #[test]
    fn max_declaration_is_unbounded() {
        let context = vec!["gigant(1,MAX)".to_string()];
        let funcs = functions_for(&context);
        let giant = funcs.iter().find(|f| f.name == "gigant").expect("gigant");
        assert_eq!(giant.max, i32::MAX as usize);
    }

    #[test]
    fn malformed_context_entries_are_ignored() {
        let context = vec![
            "not a function".to_string(),
            "bad(1)".to_string(),
            "bad(x,2)".to_string(),
            "ok(1,2)".to_string(),
        ];
        let funcs = functions_for(&context);
        let declared: Vec<&str> = funcs
            .iter()
            .skip(7)
            .map(|f| f.name.as_str())
            .collect();
        assert_eq!(declared, vec!["ok"]);
    }

    #[test]
    fn parse_schema_reports_broken_documents() {
        assert!(parse_schema("{ not json").is_err());
        assert!(parse_schema("{}").is_ok());
    }

    #[test]
    fn schema_node_reports_its_definition() {
        let schema = workflow_schema();
        let node = SchemaNode {
            definition: "workflow-root".into(),
            schema,
            context: vec!["needs".into()],
        };
        assert_eq!(node.to_string(), "workflow-root");
        assert_eq!(node.context, vec!["needs".to_string()]);
    }
}
