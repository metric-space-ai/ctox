//! YAML node tree for GitHub Actions workflows.
//!
//! act deserialises workflows into `gopkg.in/yaml.v3` node trees and keeps
//! whole subtrees as raw `yaml.Node` values, deferring decoding until the
//! field is actually needed. That matters because `${{ ... }}` expressions must
//! survive as *unevaluated source text* until the expression evaluator runs,
//! and because `on:`, `runs-on:`, `if:`, `matrix:` and friends accept either a
//! scalar or a collection depending on the workflow.
//!
//! This module rebuilds that layer on `saphyr`. It parses the event stream into
//! an arena of nodes that mirror `yaml.Node`:
//!
//! * [`NodeKind`] matches `yaml.Kind`.
//! * [`Node::content`] matches `yaml.Node.Content`.
//! * [`Node::alias`] matches `yaml.Node.Alias`, which act needs because it
//!   resolves anchors itself and detects circular ones.
//!
//! Two details drive the whole design. Aliases stay in the tree as
//! [`NodeKind::Alias`] nodes instead of being substituted during parsing, so
//! [`Document::resolve_aliases`] can reproduce act's `model/anchors.go`
//! behaviour including its `circular alias` error. And scalar text is kept raw,
//! because expression detection scans the literal text for `${{`.
//!
//! One difference worth naming: `saphyr` validates anchors while scanning, so
//! an alias to an anchor that was never defined fails as a [`YamlError::Syntax`]
//! during parsing. act leaves `node.Alias == nil` and reports
//! `unresolved alias node` from its own resolution pass. Both reject the
//! document; the port rejects it earlier. [`YamlError::UnresolvedAlias`]
//! therefore only applies to hand-built trees.
//!
//! Upstream dependency: <https://github.com/ntoml2/yaml-rust> replaced here by
//! <https://github.com/biojppm/saphyr>.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;

use saphyr_parser::{Event, Parser, ScalarStyle, StrInput};

/// Index of a node inside a [`Document`].
pub type NodeId = usize;

/// Anchor id `0` means "this node carries no anchor".
const NO_ANCHOR: usize = 0;

/// Node kind, mirroring `yaml.Kind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    /// A whole document.
    Document,
    /// A sequence.
    Sequence,
    /// A mapping.
    Mapping,
    /// A scalar value.
    Scalar,
    /// A reference to an anchored node.
    Alias,
}

/// One node of a parsed document.
#[derive(Debug, Clone)]
pub struct Node {
    /// What this node holds.
    pub kind: NodeKind,
    /// Raw scalar text. Empty for collections, as in `yaml.Node.Value`.
    pub value: String,
    /// Scalar presentation style.
    pub style: ScalarStyle,
    /// Explicit tag, when the document carried one.
    pub tag: Option<String>,
    /// Anchor name this node defines, when it defines one.
    pub anchor: Option<String>,
    /// Target of an [`NodeKind::Alias`] node.
    pub alias: Option<NodeId>,
    /// Child nodes: items for a sequence, alternating key/value for a mapping.
    pub content: Vec<NodeId>,
    /// 1-based line, for error messages.
    pub line: usize,
    /// 1-based column, for error messages.
    pub column: usize,
}

impl Node {
    fn empty(kind: NodeKind) -> Self {
        Self {
            kind,
            value: String::new(),
            style: ScalarStyle::Plain,
            tag: None,
            anchor: None,
            alias: None,
            content: Vec::new(),
            line: 0,
            column: 0,
        }
    }

    /// True when this node is a mapping.
    pub fn is_mapping(&self) -> bool {
        self.kind == NodeKind::Mapping
    }

    /// True when this node is a sequence.
    pub fn is_sequence(&self) -> bool {
        self.kind == NodeKind::Sequence
    }

    /// True when this node is a scalar.
    pub fn is_scalar(&self) -> bool {
        self.kind == NodeKind::Scalar
    }

    /// A detached node of the given kind, ready to hand to [`Document::alloc`].
    pub fn detached(kind: NodeKind) -> Self {
        Self::empty(kind)
    }

    /// Position rendered the way act's `formatLocation` does.
    pub fn location(&self) -> String {
        format!("{}:{}", self.line, self.column)
    }
}

/// A parsed YAML document, stored as an arena of nodes.
#[derive(Debug, Clone, Default)]
pub struct Document {
    nodes: Vec<Node>,
    root: Option<NodeId>,
}

/// Something went wrong while parsing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum YamlError {
    /// The scanner or parser rejected the document.
    Syntax(String),
    /// An alias referenced an anchor that was never defined.
    UnresolvedAlias,
    /// An anchor refers to itself, directly or through a chain.
    CircularAlias,
}

impl fmt::Display for YamlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Syntax(msg) => write!(f, "yaml: {msg}"),
            Self::UnresolvedAlias => f.write_str("yaml: unresolved alias node"),
            Self::CircularAlias => f.write_str("yaml: circular alias"),
        }
    }
}

impl std::error::Error for YamlError {}

impl Document {
    /// Parses a single YAML document.
    ///
    /// A multi-document stream yields its first document, matching how act
    /// unmarshals one workflow file at a time.
    pub fn parse(input: &str) -> Result<Self, YamlError> {
        let mut doc = Document::default();
        let mut parser = Parser::new_from_str(input);
        let mut anchors: HashMap<usize, NodeId> = HashMap::new();

        while let Some(event) = parser.next_event() {
            match event {
                Err(err) => return Err(YamlError::Syntax(err.to_string())),
                Ok((event, span)) => {
                    if matches!(event, Event::StreamEnd) {
                        break;
                    }
                    if matches!(event, Event::DocumentEnd) {
                        break;
                    }
                    if matches!(event, Event::Nothing | Event::StreamStart | Event::DocumentStart(_))
                    {
                        continue;
                    }
                    let id = doc.build_node(
                        &mut parser,
                        &event,
                        span.start.line(),
                        span.start.col(),
                        &mut anchors,
                        0,
                    )?;
                    if doc.root.is_none() {
                        doc.root = Some(id);
                    }
                    break;
                }
            }
        }

        Ok(doc)
    }

    /// The document's root node, if it had content.
    pub fn root(&self) -> Option<NodeId> {
        self.root
    }

    /// Borrows a node by id.
    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(id)
    }

    /// Adds a node to the arena and returns its id.
    ///
    /// `EvaluateYamlNode` replaces a whole subtree, so it needs somewhere to
    /// put the replacement. Upstream gets that from a fresh `&yaml.Node{}` that
    /// `Encode` fills in; here the replacement has to be an arena entry. An id
    /// handed out here is not reachable from the root until a caller splices
    /// it into a parent's `content`, so allocating one is invisible on its own.
    pub fn alloc(&mut self, node: Node) -> NodeId {
        self.nodes.push(node);
        self.nodes.len() - 1
    }

    /// Overwrites a node in place, keeping its arena id valid.
    ///
    /// `evaluate_yaml_node` rebuilds a subtree and has to write it back where
    /// the old one was: every `NodeId` a caller captured from the parsed
    /// document — a `Job`'s `if:`, a `Step`'s `run:` — stays meaningful after
    /// evaluation, which a fresh id would break.
    ///
    /// A missing id is ignored, matching the "nothing to replace" case the
    /// walk already reports.
    pub fn replace(&mut self, id: NodeId, node: Node) {
        if let Some(target) = self.nodes.get_mut(id) {
            *target = node;
        }
    }

    /// Points the document at a different root.
    ///
    /// Only [`crate::runner::expression::evaluate_yaml_node`]'s callers need
    /// this; the walk itself replaces the root's *contents* in place rather
    /// than re-rooting, so the document's identity survives the evaluation.
    pub fn set_root(&mut self, id: Option<NodeId>) {
        self.root = id;
    }

    /// Number of nodes in the arena.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// True when the document is empty.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    fn push(&mut self, node: Node) -> NodeId {
        self.nodes.push(node);
        self.nodes.len() - 1
    }

    /// Replaces every [`NodeKind::Alias`] node with the node it points at.
    ///
    /// Port of act's `model.resolveAliases`. act runs this before decoding a
    /// workflow so that anchors behave as if they had been written out in full.
    /// An alias currently being expanded cannot appear inside its own
    /// expansion, which is reported as [`YamlError::CircularAlias`] rather
    /// than looping forever.
    pub fn resolve_aliases(&mut self) -> Result<(), YamlError> {
        let Some(root) = self.root else {
            return Ok(());
        };
        let mut path = HashSet::new();
        self.resolve_aliases_ext(root, &mut path, false)
    }

    fn resolve_aliases_ext(
        &mut self,
        id: NodeId,
        path: &mut HashSet<NodeId>,
        skip_check: bool,
    ) -> Result<(), YamlError> {
        if !skip_check && path.contains(&id) {
            return Err(YamlError::CircularAlias);
        }

        match self.nodes[id].kind {
            NodeKind::Alias => {
                let Some(target) = self.nodes[id].alias else {
                    return Err(YamlError::UnresolvedAlias);
                };
                path.insert(id);
                // Mirrors Go's `*node = *aliasTarget`: the alias node takes
                // over the target's content, keeping its own identity so the
                // `path` set still tracks the expansion in progress.
                let target_node = self.nodes[target].clone();
                self.nodes[id] = target_node;
                self.resolve_aliases_ext(id, path, true)?;
                path.remove(&id);
            }
            NodeKind::Document | NodeKind::Mapping | NodeKind::Sequence => {
                for child in self.nodes[id].content.clone() {
                    self.resolve_aliases_ext(child, path, false)?;
                }
            }
            NodeKind::Scalar => {}
        }

        Ok(())
    }

    /// Decodes a scalar node as a string, whatever its YAML type.
    pub fn scalar(&self, id: NodeId) -> Option<String> {
        self.node(id).filter(|n| n.is_scalar()).map(|n| n.value.clone())
    }

    /// Decodes a scalar or sequence node as a list of strings.
    ///
    /// Mirrors act's `nodeAsStringSlice`: a scalar becomes a single-element
    /// list, a sequence is taken element-wise, anything else yields `None`.
    pub fn string_slice(&self, id: NodeId) -> Option<Vec<String>> {
        let node = self.node(id)?;
        match node.kind {
            NodeKind::Scalar => Some(vec![node.value.clone()]),
            NodeKind::Sequence => {
                let mut out = Vec::with_capacity(node.content.len());
                for child in &node.content {
                    out.push(self.scalar(*child)?);
                }
                Some(out)
            }
            _ => None,
        }
    }

    /// Decodes a mapping node as `string -> string`.
    pub fn string_map(&self, id: NodeId) -> Option<BTreeMap<String, String>> {
        let node = self.node(id)?;
        if !node.is_mapping() {
            return None;
        }
        let mut out = BTreeMap::new();
        for (key, value) in self.map_entries(id) {
            out.insert(self.scalar(key)?, self.scalar(value)?);
        }
        Some(out)
    }

    /// Decodes a scalar as a boolean, accepting YAML's `true`/`false` words.
    pub fn boolean(&self, id: NodeId) -> Option<bool> {
        match self.scalar(id)?.as_str() {
            "true" | "True" | "TRUE" | "yes" | "on" => Some(true),
            "false" | "False" | "FALSE" | "no" | "off" => Some(false),
            _ => None,
        }
    }

    /// Looks up a mapping entry by key.
    pub fn map_get(&self, id: NodeId, key: &str) -> Option<NodeId> {
        let node = self.node(id)?;
        if !node.is_mapping() {
            return None;
        }
        node.content.chunks_exact(2).find_map(|pair| {
            let key_node = self.node(pair[0])?;
            (key_node.value == key).then_some(pair[1])
        })
    }

    /// The mapping entries as `(key, value)` node id pairs.
    pub fn map_entries(&self, id: NodeId) -> Vec<(NodeId, NodeId)> {
        match self.node(id) {
            Some(node) if node.is_mapping() => node
                .content
                .chunks_exact(2)
                .map(|pair| (pair[0], pair[1]))
                .collect(),
            _ => Vec::new(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn build_node(
        &mut self,
        parser: &mut Parser<'_, StrInput<'_>>,
        event: &Event<'_>,
        line: usize,
        column: usize,
        anchors: &mut HashMap<usize, NodeId>,
        depth: usize,
    ) -> Result<NodeId, YamlError> {
        // Guards against stack exhaustion on adversarially nested input.
        if depth > MAX_DEPTH {
            return Err(YamlError::Syntax(format!(
                "document nests deeper than {MAX_DEPTH} levels"
            )));
        }

        match event {
            Event::Scalar(value, style, anchor_id, tag) => {
                let mut node = Node::empty(NodeKind::Scalar);
                node.value = value.to_string();
                node.style = *style;
                node.tag = tag.as_ref().map(|t| t.to_string());
                node.line = line;
                node.column = column;
                let id = self.push(node);
                if *anchor_id != NO_ANCHOR {
                    anchors.insert(*anchor_id, id);
                }
                Ok(id)
            }
            Event::Alias(anchor_id) => {
                let Some(target) = anchors.get(anchor_id).copied() else {
                    return Err(YamlError::UnresolvedAlias);
                };
                let mut node = Node::empty(NodeKind::Alias);
                node.line = line;
                node.column = column;
                node.alias = Some(target);
                Ok(self.push(node))
            }
            Event::SequenceStart(anchor_id, tag) => {
                let mut node = Node::empty(NodeKind::Sequence);
                node.tag = tag.as_ref().map(|t| t.to_string());
                node.line = line;
                node.column = column;
                let id = self.push(node);
                if *anchor_id != NO_ANCHOR {
                    anchors.insert(*anchor_id, id);
                }
                while let Some(child) = self.next_node(parser, anchors, depth)? {
                    self.nodes[id].content.push(child);
                }
                Ok(id)
            }
            Event::MappingStart(anchor_id, tag) => {
                let mut node = Node::empty(NodeKind::Mapping);
                node.tag = tag.as_ref().map(|t| t.to_string());
                node.line = line;
                node.column = column;
                let id = self.push(node);
                if *anchor_id != NO_ANCHOR {
                    anchors.insert(*anchor_id, id);
                }
                let mut key = None;
                while let Some(child) = self.next_node(parser, anchors, depth)? {
                    match key.take() {
                        None => key = Some(child),
                        Some(key) => {
                            self.nodes[id].content.push(key);
                            self.nodes[id].content.push(child);
                        }
                    }
                }
                if key.is_some() {
                    // A mapping key with no value: YAML treats it as null.
                    let mut null_node = Node::empty(NodeKind::Scalar);
                    null_node.tag = Some("!!null".to_string());
                    let null_id = self.push(null_node);
                    self.nodes[id].content.push(null_id);
                }
                Ok(id)
            }
            other => Err(YamlError::Syntax(format!("unexpected event {other:?}"))),
        }
    }

    /// Pulls the next content node, descending into nested collections.
    ///
    /// Returns `None` when the enclosing collection closes or the stream ends.
    fn next_node(
        &mut self,
        parser: &mut Parser<'_, StrInput<'_>>,
        anchors: &mut HashMap<usize, NodeId>,
        depth: usize,
    ) -> Result<Option<NodeId>, YamlError> {
        loop {
            let Some(event) = parser.next_event() else {
                return Ok(None);
            };
            let (event, span) = event.map_err(|err| YamlError::Syntax(err.to_string()))?;
            match &event {
                Event::Nothing | Event::StreamStart | Event::DocumentStart(_) => continue,
                Event::StreamEnd | Event::DocumentEnd => return Ok(None),
                Event::SequenceEnd | Event::MappingEnd => return Ok(None),
                _ => {
                    return self
                        .build_node(
                            parser,
                            &event,
                            span.start.line(),
                            span.start.col(),
                            anchors,
                            depth + 1,
                        )
                        .map(Some);
                }
            }
        }
    }
}

/// Maximum nesting depth accepted while parsing.
const MAX_DEPTH: usize = 256;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_has_no_root() {
        let doc = Document::parse("").expect("empty input parses");
        assert!(doc.is_empty());
        assert_eq!(doc.root(), None);
    }

    #[test]
    fn scalar_root() {
        let doc = Document::parse("hello").expect("parses");
        let root = doc.root().expect("root");
        let node = doc.node(root).expect("node");
        assert!(node.is_scalar());
        assert_eq!(node.value, "hello");
    }

    #[test]
    fn mapping_root_keeps_key_value_pairs() {
        let doc = Document::parse("name: build\non: push\n").expect("parses");
        let root = doc.root().expect("root");
        let node = doc.node(root).expect("node");
        assert!(node.is_mapping());
        assert_eq!(node.content.len(), 4);
    }

    #[test]
    fn sequence_root() {
        let doc = Document::parse("- a\n- b\n").expect("parses");
        let root = doc.root().expect("root");
        let node = doc.node(root).expect("node");
        assert!(node.is_sequence());
        assert_eq!(node.content.len(), 2);
    }

    #[test]
    fn raw_expression_text_is_preserved() {
        let doc = Document::parse(r#"if: ${{ github.event_name == 'push' }}"#).expect("parses");
        let root = doc.root().expect("root");
        let mapping = doc.node(root).expect("node");
        assert!(mapping.is_mapping());

        let key = doc.node(mapping.content[0]).expect("key");
        assert_eq!(key.value, "if");

        let value = doc.node(mapping.content[1]).expect("value");
        assert_eq!(value.value, "${{ github.event_name == 'push' }}");
        assert!(value.value.contains("${{"));
    }

    #[test]
    fn expression_inside_a_longer_scalar_is_preserved() {
        let doc = Document::parse("run: echo ${{ matrix.os }}").expect("parses");
        let root = doc.root().expect("root");
        let mapping = doc.node(root).expect("node");
        let value = doc.node(mapping.content[1]).expect("value");
        assert_eq!(value.value, "echo ${{ matrix.os }}");
    }

    #[test]
    fn quoted_expression_yields_the_same_text() {
        let doc = Document::parse(r#"if: "${{ github.event_name == 'push' }}""#).expect("parses");
        let root = doc.root().expect("root");
        let mapping = doc.node(root).expect("node");
        let value = doc.node(mapping.content[1]).expect("value");
        assert_eq!(value.value, "${{ github.event_name == 'push' }}");
    }

    #[test]
    fn quoted_scalar_keeps_quotes_out_of_value() {
        let doc = Document::parse(r#"name: "0.2.89""#).expect("parses");
        let root = doc.root().expect("root");
        let node = doc.node(root).expect("node");
        let value = doc.node(node.content[1]).expect("value");
        assert_eq!(value.value, "0.2.89");
    }

    #[test]
    fn nested_structures_are_built() {
        let doc = Document::parse("jobs:\n  build:\n    steps:\n      - run: echo hi\n")
            .expect("parses");
        assert!(doc.len() >= 7, "expected a populated arena, got {}", doc.len());
    }

    #[test]
    fn syntax_error_is_reported() {
        let err = Document::parse("a: [1, 2\nb: }").expect_err("must reject");
        assert!(matches!(err, YamlError::Syntax(_)));
    }
}

#[cfg(test)]
mod alias_tests {
    use super::*;

    fn resolved(source: &str) -> Document {
        let mut doc = Document::parse(source).expect("must parse");
        doc.resolve_aliases().expect("must resolve");
        doc
    }

    #[test]
    fn alias_to_scalar_is_substituted() {
        let doc = resolved("defaults: &shared\n  value: 7\njob: *shared\n");
        let root = doc.root().expect("root");
        let job = doc.map_get(root, "job").expect("job");
        let job_node = doc.node(job).expect("job node");
        assert!(job_node.is_mapping());
        let value = doc.map_get(job, "value").expect("value");
        assert_eq!(doc.node(value).unwrap().value, "7");
    }

    #[test]
    fn aliases_resolve_to_the_same_content() {
        let doc = resolved("a: &x [1, 2]\nb: *x\n");
        let root = doc.root().expect("root");
        let a = doc.map_get(root, "a").expect("a");
        let b = doc.map_get(root, "b").expect("b");
        assert_eq!(doc.node(a).unwrap().content.len(), 2);
        assert_eq!(doc.node(b).unwrap().content.len(), 2);
    }

    #[test]
    fn no_alias_nodes_remain_after_resolution() {
        let doc = resolved("a: &x hello\nb: *x\n");
        let root = doc.root().expect("root");
        let b = doc.map_get(root, "b").expect("b");
        assert!(doc.node(b).unwrap().is_scalar());
    }

    #[test]
    fn anchors_without_aliases_are_untouched() {
        let doc = resolved("a: &x hello\nb: plain\n");
        let root = doc.root().expect("root");
        let a = doc.map_get(root, "a").expect("a");
        assert_eq!(doc.node(a).unwrap().value, "hello");
    }

    #[test]
    fn circular_alias_is_rejected() {
        // `a` anchors a sequence that contains an alias back to itself.
        let mut doc = Document::parse("a: &x\n  - *x\n").expect("must parse");
        let err = doc.resolve_aliases().expect_err("circular alias must fail");
        assert_eq!(err, YamlError::CircularAlias);
    }

    #[test]
    fn indirect_circular_alias_is_rejected() {
        let mut doc = Document::parse("a: &x\n  b: &y\n    c: *x\n").expect("must parse");
        let err = doc.resolve_aliases().expect_err("circular alias must fail");
        assert_eq!(err, YamlError::CircularAlias);
    }

    #[test]
    fn empty_document_resolves_trivially() {
        let mut doc = Document::parse("").expect("parses");
        assert!(doc.resolve_aliases().is_ok());
    }

    #[test]
    fn map_get_returns_none_for_missing_key() {
        let doc = Document::parse("a: 1\n").expect("parses");
        let root = doc.root().expect("root");
        assert!(doc.map_get(root, "missing").is_none());
    }

    #[test]
    fn map_get_returns_none_for_non_mapping() {
        let doc = Document::parse("- a\n").expect("parses");
        let root = doc.root().expect("root");
        assert!(doc.map_get(root, "a").is_none());
    }

    #[test]
    fn map_entries_pairs_keys_with_values() {
        let doc = Document::parse("a: 1\nb: 2\n").expect("parses");
        let root = doc.root().expect("root");
        let entries = doc.map_entries(root);
        assert_eq!(entries.len(), 2);
        let keys: Vec<String> = entries
            .iter()
            .map(|(k, _)| doc.node(*k).unwrap().value.clone())
            .collect();
        assert_eq!(keys, vec!["a".to_string(), "b".to_string()]);
    }
}
