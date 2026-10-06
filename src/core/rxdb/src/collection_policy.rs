//! Collection-specific data-plane behavior.
//!
//! Generic storage and replication code must consult this module instead of
//! embedding collection names or collection-specific transfer/write rules.
//! Applications may install a replacement policy before the first policy read;
//! the CTOX-layer registration itself is intentionally left to a follow-up.

use std::sync::OnceLock;

use serde_json::Value;

/// Generic ceiling for one master pull response before transport framing.
///
/// The WebRTC wire allows transfers up to 8 MiB, but a count-only pull batch can
/// combine many individually valid documents into one response above that hard
/// limit. Keep ordinary collections comfortably below the wire ceiling; known
/// chunk-heavy collections may install tighter exact-name overrides below.
///
/// 4 MiB (half the wire limit): with ~60 KB documents a 1 MiB ceiling paid a
/// full pull round-trip per ~15 documents (on-prem deployment, 06.10.2026).
pub const DEFAULT_MASTER_RESPONSE_CEILING_BYTES: usize = 4 * 1024 * 1024;

/// A collection/context-specific write decision applied by a storage backend.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WriteGuard {
    /// No collection-specific write filtering applies.
    Allow,
    /// Reject the incoming value when the stored value is non-empty and differs.
    ///
    /// The stored value is trimmed before comparison, while the incoming value
    /// must match exactly. This preserves the pre-policy command status guard.
    RejectIncomingValueWhenStoredNonEmptyAndDifferent {
        field: String,
        incoming_value: String,
    },
}

impl WriteGuard {
    /// Returns whether this guard performs any check at all — callers use this
    /// to decide whether the pre-write stored-document lookup is needed.
    pub fn is_active(&self) -> bool {
        !matches!(self, Self::Allow)
    }

    /// Returns whether this guard rejects an incoming document against stored state.
    pub fn rejects(&self, document: &Value, document_in_db: &Value) -> bool {
        match self {
            Self::Allow => false,
            Self::RejectIncomingValueWhenStoredNonEmptyAndDifferent {
                field,
                incoming_value,
            } => {
                document.get(field).and_then(Value::as_str) == Some(incoming_value.as_str())
                    && document_in_db
                        .get(field)
                        .and_then(Value::as_str)
                        .is_some_and(|stored| {
                            let stored = stored.trim();
                            !stored.is_empty() && stored != incoming_value
                        })
            }
        }
    }
}

#[derive(Clone, Debug)]
struct WriteGuardRule {
    collection: String,
    context: String,
    guard: WriteGuard,
}

#[derive(Clone, Debug)]
struct PollBatchLimitRule {
    collection: String,
    limit: usize,
}

#[derive(Clone, Debug)]
struct TransferCeilingRule {
    collection: String,
    ceiling_bytes: usize,
}

/// Registry of collection-specific data-plane behavior.
#[derive(Clone, Debug)]
pub struct CollectionPolicy {
    write_guards: Vec<WriteGuardRule>,
    poll_batch_limits: Vec<PollBatchLimitRule>,
    transfer_ceilings: Vec<TransferCeilingRule>,
    demand_only_chunk_collections: Vec<String>,
}

impl CollectionPolicy {
    /// Creates an empty policy. Rules are exact collection-name matches.
    pub fn new() -> Self {
        Self {
            write_guards: Vec::new(),
            poll_batch_limits: Vec::new(),
            transfer_ceilings: Vec::new(),
            demand_only_chunk_collections: Vec::new(),
        }
    }

    pub fn with_write_guard(
        mut self,
        collection: impl Into<String>,
        context: impl Into<String>,
        guard: WriteGuard,
    ) -> Self {
        self.write_guards.push(WriteGuardRule {
            collection: collection.into(),
            context: context.into(),
            guard,
        });
        self
    }

    pub fn with_poll_batch_limit(mut self, collection: impl Into<String>, limit: usize) -> Self {
        self.poll_batch_limits.push(PollBatchLimitRule {
            collection: collection.into(),
            limit,
        });
        self
    }

    pub fn with_transfer_ceiling_bytes(
        mut self,
        collection: impl Into<String>,
        ceiling_bytes: usize,
    ) -> Self {
        self.transfer_ceilings.push(TransferCeilingRule {
            collection: collection.into(),
            ceiling_bytes,
        });
        self
    }

    pub fn with_demand_only_chunk_collection(mut self, collection: impl Into<String>) -> Self {
        self.demand_only_chunk_collections.push(collection.into());
        self
    }

    pub fn write_guard(&self, collection: &str, context: &str) -> WriteGuard {
        self.write_guards
            .iter()
            .find(|rule| rule.collection == collection && rule.context == context)
            .map_or(WriteGuard::Allow, |rule| rule.guard.clone())
    }

    /// Returns a collection-specific SQLite external-poll limit.
    ///
    /// SQLite table names encode the database, exact collection name, and schema
    /// version as `<database>__<collection>__v<version>`. We parse the final
    /// collection segment instead of using substring matching, so similarly
    /// named tables retain the generic poll limit.
    pub fn poll_batch_limit(&self, table_name: &str) -> Option<usize> {
        let collection = collection_name_from_table_name(table_name);
        self.poll_batch_limits
            .iter()
            .find(|rule| rule.collection == collection)
            .map(|rule| rule.limit)
    }

    pub fn transfer_ceiling_bytes(&self, collection: &str) -> Option<usize> {
        self.transfer_ceilings
            .iter()
            .find(|rule| rule.collection == collection)
            .map(|rule| rule.ceiling_bytes)
    }

    /// Returns the byte ceiling for every master pull response.
    ///
    /// Exact collection rules only tighten the generic ceiling. Returning a
    /// concrete default here is load-bearing: otherwise an ordinary collection
    /// with many large JSON documents can exceed the framed WebRTC transfer
    /// limit even though every individual document is valid.
    pub fn master_response_ceiling_bytes(&self, collection: &str) -> usize {
        self.transfer_ceiling_bytes(collection)
            .unwrap_or(DEFAULT_MASTER_RESPONSE_CEILING_BYTES)
    }

    pub fn is_demand_only_chunk_collection(&self, collection: &str) -> bool {
        self.demand_only_chunk_collections
            .iter()
            .any(|configured| configured == collection)
    }
}

impl Default for CollectionPolicy {
    fn default() -> Self {
        Self::new()
            .with_write_guard(
                "business_commands",
                "replication-master-write",
                WriteGuard::RejectIncomingValueWhenStoredNonEmptyAndDifferent {
                    field: "status".to_string(),
                    incoming_value: "pending_sync".to_string(),
                },
            )
            .with_poll_batch_limit("desktop_file_chunks", 2)
            .with_transfer_ceiling_bytes("desktop_file_chunks", 96 * 1024)
            // Knowledge table chunks are large row-bearing documents (the SKF
            // dataset is roughly 380-414 KiB per document). Keep one response
            // below the WebRTC transfer ceiling when older peers ask for more.
            .with_transfer_ceiling_bytes("knowledge_tables", 512 * 1024)
            .with_demand_only_chunk_collection("desktop_file_chunks")
            .with_demand_only_chunk_collection("document_blob_chunks")
            .with_demand_only_chunk_collection("spreadsheet_blob_chunks")
    }
}

fn collection_name_from_table_name(table_name: &str) -> &str {
    let Some((without_version, _version)) = table_name.rsplit_once("__v") else {
        return table_name;
    };
    without_version
        .rsplit_once("__")
        .map_or(table_name, |(_, collection)| collection)
}

static COLLECTION_POLICY: OnceLock<CollectionPolicy> = OnceLock::new();

/// Installs the process-wide policy before its first use.
///
/// Returns the supplied policy unchanged when a policy was already installed or
/// the default policy was already observed.
pub fn register_collection_policy(policy: CollectionPolicy) -> Result<(), CollectionPolicy> {
    COLLECTION_POLICY.set(policy)
}

/// Returns the registered policy, initializing the built-in defaults on first use.
pub fn collection_policy() -> &'static CollectionPolicy {
    COLLECTION_POLICY.get_or_init(CollectionPolicy::default)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    use serde_json::json;

    use super::{CollectionPolicy, WriteGuard, DEFAULT_MASTER_RESPONSE_CEILING_BYTES};

    #[test]
    fn default_write_guard_is_exact_and_preserves_status_semantics() {
        let policy = CollectionPolicy::default();
        let guard = policy.write_guard("business_commands", "replication-master-write");
        assert!(guard.rejects(
            &json!({"status": "pending_sync"}),
            &json!({"status": "completed"})
        ));
        assert!(!guard.rejects(
            &json!({"status": "pending_sync"}),
            &json!({"status": " pending_sync "})
        ));
        assert_eq!(
            policy.write_guard("other_commands", "replication-master-write"),
            WriteGuard::Allow
        );
        assert_eq!(
            policy.write_guard("business_commands", "local-write"),
            WriteGuard::Allow
        );
    }

    #[test]
    fn default_poll_limit_uses_exact_derived_collection_segment() {
        let policy = CollectionPolicy::default();
        assert_eq!(
            policy.poll_batch_limit("workspace__desktop_file_chunks__v0"),
            Some(2)
        );
        assert_eq!(
            policy.poll_batch_limit("workspace__archived_desktop_file_chunks__v0"),
            None
        );
        assert_eq!(policy.poll_batch_limit("workspace__documents__v0"), None);
    }

    #[test]
    fn default_transfer_ceilings_and_demand_only_membership_are_exact() {
        let policy = CollectionPolicy::default();
        assert_eq!(
            policy.transfer_ceiling_bytes("desktop_file_chunks"),
            Some(96 * 1024)
        );
        assert_eq!(
            policy.transfer_ceiling_bytes("knowledge_tables"),
            Some(512 * 1024)
        );
        assert_eq!(policy.transfer_ceiling_bytes("documents"), None);
        assert_eq!(
            policy.master_response_ceiling_bytes("documents"),
            DEFAULT_MASTER_RESPONSE_CEILING_BYTES
        );
        assert_eq!(
            policy.master_response_ceiling_bytes("desktop_file_chunks"),
            96 * 1024
        );
        assert!(policy.is_demand_only_chunk_collection("desktop_file_chunks"));
        assert!(policy.is_demand_only_chunk_collection("document_blob_chunks"));
        assert!(policy.is_demand_only_chunk_collection("spreadsheet_blob_chunks"));
        assert!(!policy.is_demand_only_chunk_collection("archived_desktop_file_chunks"));
    }

    #[test]
    fn production_collection_literals_are_centralized() {
        const CENTRALIZED_LITERALS: [&str; 3] = [
            "business_commands",
            "desktop_file_chunks",
            "knowledge_tables",
        ];

        let src_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut rust_files = Vec::new();
        collect_rust_files(&src_root, &mut rust_files);
        let policy_file = src_root.join("collection_policy.rs");
        let mut violations = Vec::new();

        for path in rust_files {
            if path == policy_file || is_allowed_non_production_path(&path) {
                continue;
            }
            let source = fs::read_to_string(&path).expect("read Rust source for policy guard");
            let production_source = before_cfg_test_module(&source);
            for (line_index, line) in production_source.lines().enumerate() {
                for literal in CENTRALIZED_LITERALS {
                    if line.contains(&format!("\"{literal}\"")) {
                        violations.push(format!(
                            "{}:{} contains production literal {literal:?}",
                            path.strip_prefix(&src_root).unwrap_or(&path).display(),
                            line_index + 1
                        ));
                    }
                }
            }
        }

        assert!(
            violations.is_empty(),
            "collection-specific production literals must live only in collection_policy.rs:\n{}",
            violations.join("\n")
        );
    }

    fn collect_rust_files(directory: &Path, files: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(directory).expect("read source directory") {
            let path = entry.expect("read source entry").path();
            if path.is_dir() {
                collect_rust_files(&path, files);
            } else if path.extension().and_then(|extension| extension.to_str()) == Some("rs") {
                files.push(path);
            }
        }
    }

    fn is_allowed_non_production_path(path: &Path) -> bool {
        path.components().any(|component| {
            matches!(
                component.as_os_str().to_str(),
                Some("tests" | "fixtures" | "generated" | "contracts")
            )
        })
    }

    fn before_cfg_test_module(source: &str) -> &str {
        let marker = "#[cfg(test)]\nmod tests";
        source
            .find(marker)
            .map_or(source, |test_module_start| &source[..test_module_start])
    }
}
