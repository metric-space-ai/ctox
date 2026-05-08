//! Compiled-in deep-research asset pack.
//!
//! The asset pack is the single source of truth for report-type catalogue,
//! domain profiles, depth profiles, document blueprints, block-library
//! rubrics, reference resources, verdict patterns and style guidance. It
//! lives at
//! `skills/system/research/deep-research/references/asset_pack.json` and is
//! embedded into the binary via `include_str!` so the deep-research
//! backend cannot ship without it.
//!
//! The structs are deliberately permissive:
//! - every named field carries `#[serde(default)]` so an asset-pack
//!   addition does not break compilation;
//! - free-form sub-trees use `serde_json::Value` rather than a typed
//!   struct;
//! - [`AssetPack::validate`] does the cheap sanity checks the schema
//!   alone cannot enforce (cross-references between `report_types`,
//!   `block_library`, `document_blueprints`, `reference_resources`,
//!   `optional_modules` and `style_profiles`).

use std::collections::HashSet;
use std::sync::OnceLock;

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

const ASSET_PACK_JSON: &str =
    include_str!("../../skills/system/research/deep-research/references/asset_pack.json");

static ASSET_PACK: OnceLock<AssetPack> = OnceLock::new();

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Manifest {
    #[serde(default)]
    pub asset_pack_id: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub purpose: String,
    #[serde(default)]
    pub derived_from: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ReportType {
    pub id: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub purpose: String,
    #[serde(default)]
    pub block_library_keys: Vec<String>,
    #[serde(default)]
    pub document_blueprint_id: String,
    #[serde(default)]
    pub default_modules: Vec<String>,
    #[serde(default)]
    pub verdict_vocabulary: Vec<String>,
    #[serde(default)]
    pub verdict_line_pattern: Option<String>,
    #[serde(default)]
    pub min_sections: u32,
    #[serde(default)]
    pub typical_chars: u32,
    #[serde(default)]
    pub reference_archetype_ids: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ReferenceArchetype {
    pub id: String,
    #[serde(default)]
    pub report_type_id: String,
    #[serde(default)]
    pub source_doc: String,
    #[serde(default)]
    pub domain_profile_id: String,
    #[serde(default)]
    pub structural_summary: String,
    #[serde(default)]
    pub uses_resource_ids: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DomainProfile {
    pub id: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub study_heading: String,
    #[serde(default)]
    pub study_doc_title: String,
    #[serde(default)]
    pub default_modules: Vec<String>,
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DepthProfile {
    pub id: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub description: String,
    /// Free-form sub-tree — the JSON exposes `evidence_floor.min_sources`,
    /// `min_methods_screened`, and so on. The check-tools wave will
    /// surface specific fields; we keep it untyped here so the asset
    /// pack can grow without forcing a schema change.
    #[serde(default)]
    pub evidence_floor: Value,
    #[serde(default)]
    pub generation_notes: Vec<String>,
    /// The spec doc references `min_evidence_count` and `research_budget`
    /// as conceptual fields. The asset pack currently expresses them
    /// inside `evidence_floor`; these aliases are populated where present
    /// so the workspace builder can read them uniformly.
    #[serde(default)]
    pub min_evidence_count: Option<u32>,
    #[serde(default)]
    pub research_budget: Option<u32>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ReferenceProfile {
    pub id: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub style_profile_id: String,
    #[serde(default)]
    pub recommended_reference_ids: Vec<String>,
    #[serde(default)]
    pub suggested_modules: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OptionalModule {
    pub id: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub block_ids: Vec<String>,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DocumentBaseDoc {
    pub id: String,
    #[serde(default)]
    pub title: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BlueprintSequenceEntry {
    pub block_id: String,
    pub doc_id: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub order: i64,
    #[serde(default)]
    pub module: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DocumentBlueprint {
    #[serde(default)]
    pub base_docs: Vec<DocumentBaseDoc>,
    #[serde(default)]
    pub sequence: Vec<BlueprintSequenceEntry>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BlockLibraryEntry {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub goal: String,
    #[serde(default)]
    pub must_have: Vec<String>,
    #[serde(default)]
    pub style_rules: Vec<String>,
    #[serde(default)]
    pub min_chars: u32,
    #[serde(default)]
    pub min_chars_per_option: Option<u32>,
    #[serde(default)]
    pub min_rows: Option<u32>,
    #[serde(default)]
    pub reference_ids: Vec<String>,
    #[serde(default)]
    pub repeatable: bool,
    #[serde(default)]
    pub typical_count_range: Option<Value>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ReferenceResource {
    pub id: String,
    #[serde(default)]
    pub source_file: String,
    #[serde(default)]
    pub story_role: String,
    #[serde(default)]
    pub block_template_ids: Vec<String>,
    #[serde(default)]
    pub excerpt: String,
    #[serde(default)]
    pub why_it_works: Vec<String>,
    #[serde(default)]
    pub reuse_moves: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ReferencePattern {
    pub id: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub source_files: Vec<String>,
    #[serde(default)]
    pub best_for: Vec<String>,
    #[serde(default)]
    pub structure_signals: Vec<String>,
    #[serde(default)]
    pub content_cues: Vec<String>,
    #[serde(default)]
    pub resource_ids: Vec<String>,
    #[serde(default)]
    pub cautions: Vec<String>,
    #[serde(default)]
    pub style_samples: Vec<Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ReferenceLengthStats {
    #[serde(default)]
    pub source_corpus: String,
    #[serde(default)]
    pub document_count: u32,
    #[serde(default)]
    pub average_chars: u32,
    #[serde(default)]
    pub min_chars: u32,
    #[serde(default)]
    pub max_chars: u32,
    #[serde(default)]
    pub total_chars: u32,
    #[serde(default)]
    pub calculation_note: String,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct VerdictPattern {
    pub report_type_id: String,
    pub block_id: String,
    #[serde(default)]
    pub pattern: Option<String>,
    #[serde(default)]
    pub vocabulary: Vec<String>,
    #[serde(default)]
    pub notes: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StyleProfile {
    pub id: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub use_when: Vec<String>,
    #[serde(default)]
    pub directives: Vec<String>,
}

/// The full 20-list bundle from `style_guidance`. Fields that are not yet
/// consumed are kept as `Vec<Value>` to absorb additions without forcing
/// a schema change.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct StyleGuidance {
    #[serde(default)]
    pub reader_effect: Vec<String>,
    #[serde(default)]
    pub preferred_moves: Vec<String>,
    #[serde(default)]
    pub document_arc: Vec<String>,
    #[serde(default)]
    pub section_bridging: Vec<String>,
    #[serde(default)]
    pub reference_handling: Vec<String>,
    #[serde(default)]
    pub dossier_story_model: Vec<String>,
    #[serde(default)]
    pub section_role_guidance: Vec<String>,
    #[serde(default)]
    pub no_reference_strategy: Vec<String>,
    #[serde(default)]
    pub internal_perspective_rules: Vec<String>,
    #[serde(default)]
    pub evidence_gap_policy: Vec<String>,
    #[serde(default)]
    pub domain_tone_rules: Vec<String>,
    #[serde(default)]
    pub terminology_consistency_rules: Vec<String>,
    #[serde(default)]
    pub numbers_freshness_rules: Vec<String>,
    #[serde(default)]
    pub consultant_phrases_to_soften: Vec<String>,
    #[serde(default)]
    pub dead_phrases_to_avoid: Vec<String>,
    #[serde(default)]
    pub forbidden_meta_phrases: Vec<String>,
    #[serde(default)]
    pub revision_checklist: Vec<String>,
    #[serde(default)]
    pub micro_examples: Vec<Value>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AssetPack {
    #[serde(default)]
    pub manifest: Manifest,
    #[serde(default)]
    pub report_types: Vec<ReportType>,
    #[serde(default)]
    pub reference_archetypes: Vec<ReferenceArchetype>,
    #[serde(default)]
    pub domain_profiles: Vec<DomainProfile>,
    #[serde(default)]
    pub depth_profiles: Vec<DepthProfile>,
    #[serde(default)]
    pub reference_profiles: Vec<ReferenceProfile>,
    #[serde(default)]
    pub optional_modules: Vec<OptionalModule>,
    #[serde(default)]
    pub document_blueprints: serde_json::Map<String, Value>,
    #[serde(default)]
    pub block_library: serde_json::Map<String, Value>,
    #[serde(default)]
    pub reference_resources: Vec<ReferenceResource>,
    #[serde(default)]
    pub reference_patterns: Vec<ReferencePattern>,
    #[serde(default)]
    pub reference_length_stats: ReferenceLengthStats,
    #[serde(default)]
    pub verdict_patterns: Vec<VerdictPattern>,
    #[serde(default)]
    pub style_profiles: Vec<StyleProfile>,
    #[serde(default)]
    pub style_guidance: StyleGuidance,
}

impl Default for Manifest {
    fn default() -> Self {
        Manifest {
            asset_pack_id: String::new(),
            version: String::new(),
            title: String::new(),
            purpose: String::new(),
            derived_from: Vec::new(),
        }
    }
}

impl Default for ReferenceLengthStats {
    fn default() -> Self {
        ReferenceLengthStats {
            source_corpus: String::new(),
            document_count: 0,
            average_chars: 0,
            min_chars: 0,
            max_chars: 0,
            total_chars: 0,
            calculation_note: String::new(),
            extra: serde_json::Map::new(),
        }
    }
}

impl AssetPack {
    /// Parse the embedded JSON exactly once and return the cached instance.
    /// Subsequent calls are O(1) pointer reads.
    pub fn load() -> Result<&'static AssetPack> {
        if let Some(pack) = ASSET_PACK.get() {
            return Ok(pack);
        }
        let parsed: AssetPack = serde_json::from_str(ASSET_PACK_JSON)
            .context("failed to parse embedded deep-research asset_pack.json")?;
        // Best-effort store: if another thread won the race, return that.
        let _ = ASSET_PACK.set(parsed);
        Ok(ASSET_PACK
            .get()
            .expect("asset pack must be set after successful load"))
    }

    /// Lookup a report type by id.
    pub fn report_type(&self, id: &str) -> Result<&ReportType> {
        self.report_types
            .iter()
            .find(|r| r.id == id)
            .ok_or_else(|| anyhow!("unknown report_type_id: {id}"))
    }

    /// Resolve every block-library entry referenced by the report type's
    /// `block_library_keys[]`. Order is preserved.
    pub fn block_library_for(&self, report_type_id: &str) -> Result<Vec<&Value>> {
        let report_type = self.report_type(report_type_id)?;
        let mut out: Vec<&Value> = Vec::with_capacity(report_type.block_library_keys.len());
        for key in &report_type.block_library_keys {
            let entry = self
                .block_library
                .get(key)
                .ok_or_else(|| anyhow!("block_library entry {key} missing for {report_type_id}"))?;
            out.push(entry);
        }
        Ok(out)
    }

    /// Typed accessor for one block-library entry. The asset pack uses a
    /// JSON object map for `block_library`, so we deserialise on demand.
    pub fn block_library_entry(&self, block_id: &str) -> Result<BlockLibraryEntry> {
        let value = self
            .block_library
            .get(block_id)
            .ok_or_else(|| anyhow!("block_library entry {block_id} not found"))?;
        let entry: BlockLibraryEntry = serde_json::from_value(value.clone())
            .with_context(|| format!("failed to decode block_library entry {block_id}"))?;
        Ok(entry)
    }

    /// Lookup a document blueprint by id.
    pub fn document_blueprint(&self, blueprint_id: &str) -> Result<DocumentBlueprint> {
        let value = self
            .document_blueprints
            .get(blueprint_id)
            .ok_or_else(|| anyhow!("document_blueprint {blueprint_id} not found"))?;
        let blueprint: DocumentBlueprint = serde_json::from_value(value.clone())
            .with_context(|| format!("failed to decode document_blueprint {blueprint_id}"))?;
        Ok(blueprint)
    }

    pub fn style_guidance(&self) -> &StyleGuidance {
        &self.style_guidance
    }

    pub fn style_profile(&self, id: &str) -> Result<&StyleProfile> {
        self.style_profiles
            .iter()
            .find(|p| p.id == id)
            .ok_or_else(|| anyhow!("unknown style_profile_id: {id}"))
    }

    pub fn domain_profile(&self, id: &str) -> Result<&DomainProfile> {
        self.domain_profiles
            .iter()
            .find(|p| p.id == id)
            .ok_or_else(|| anyhow!("unknown domain_profile_id: {id}"))
    }

    pub fn depth_profile(&self, id: &str) -> Result<&DepthProfile> {
        self.depth_profiles
            .iter()
            .find(|p| p.id == id)
            .ok_or_else(|| anyhow!("unknown depth_profile_id: {id}"))
    }

    pub fn reference_profile(&self, id: &str) -> Option<&ReferenceProfile> {
        self.reference_profiles.iter().find(|p| p.id == id)
    }

    pub fn optional_module(&self, id: &str) -> Option<&OptionalModule> {
        self.optional_modules.iter().find(|m| m.id == id)
    }

    /// Reference resources whose `block_template_ids[]` overlap with the
    /// requested template ids. Order: input order, dedup-stable.
    pub fn references_for_blocks(&self, block_template_ids: &[String]) -> Vec<&ReferenceResource> {
        let wanted: HashSet<&str> = block_template_ids.iter().map(String::as_str).collect();
        let mut out: Vec<&ReferenceResource> = Vec::new();
        for resource in &self.reference_resources {
            if resource
                .block_template_ids
                .iter()
                .any(|tpl| wanted.contains(tpl.as_str()))
            {
                out.push(resource);
            }
        }
        out
    }

    pub fn reference_pattern(&self, id: &str) -> Option<&ReferencePattern> {
        self.reference_patterns.iter().find(|p| p.id == id)
    }

    pub fn verdict_pattern(&self, report_type_id: &str, block_id: &str) -> Option<&VerdictPattern> {
        self.verdict_patterns
            .iter()
            .find(|v| v.report_type_id == report_type_id && v.block_id == block_id)
    }

    /// Cheap structural sanity checks. Run from the host bootstrap to
    /// fail fast on a malformed asset pack instead of later when the
    /// manager picks an unresolved id.
    pub fn validate(&self) -> Result<()> {
        // Every report_type's block_library_keys[] entry must exist in
        // block_library.
        for report_type in &self.report_types {
            for key in &report_type.block_library_keys {
                if !self.block_library.contains_key(key) {
                    return Err(anyhow!(
                        "report_type {} references unknown block_library entry {key}",
                        report_type.id
                    ));
                }
            }
            // The blueprint id must exist as a key in document_blueprints
            // (or be empty, in which case a downstream wave will reject it
            // when the run tries to use it).
            if !report_type.document_blueprint_id.is_empty()
                && !self
                    .document_blueprints
                    .contains_key(&report_type.document_blueprint_id)
            {
                return Err(anyhow!(
                    "report_type {} references unknown document_blueprint_id {}",
                    report_type.id,
                    report_type.document_blueprint_id
                ));
            }
        }

        // Every reference_archetype.uses_resource_ids[] entry must exist
        // in reference_resources.
        let resource_ids: HashSet<&str> = self
            .reference_resources
            .iter()
            .map(|r| r.id.as_str())
            .collect();
        for archetype in &self.reference_archetypes {
            for resource_id in &archetype.uses_resource_ids {
                if !resource_ids.contains(resource_id.as_str()) {
                    return Err(anyhow!(
                        "reference_archetype {} references unknown resource_id {resource_id}",
                        archetype.id
                    ));
                }
            }
        }

        // Every optional_module.block_ids[] entry must exist in block_library.
        for module in &self.optional_modules {
            for block_id in &module.block_ids {
                if !self.block_library.contains_key(block_id) {
                    return Err(anyhow!(
                        "optional_module {} references unknown block_library entry {block_id}",
                        module.id
                    ));
                }
            }
        }

        // Every reference_profile.style_profile_id must resolve.
        let style_ids: HashSet<&str> = self.style_profiles.iter().map(|p| p.id.as_str()).collect();
        for profile in &self.reference_profiles {
            if !profile.style_profile_id.is_empty()
                && !style_ids.contains(profile.style_profile_id.as_str())
            {
                return Err(anyhow!(
                    "reference_profile {} references unknown style_profile_id {}",
                    profile.id,
                    profile.style_profile_id
                ));
            }
        }

        // Every verdict_pattern.report_type_id and .block_id must resolve.
        let report_type_ids: HashSet<&str> =
            self.report_types.iter().map(|r| r.id.as_str()).collect();
        for pattern in &self.verdict_patterns {
            if !report_type_ids.contains(pattern.report_type_id.as_str()) {
                return Err(anyhow!(
                    "verdict_pattern references unknown report_type_id {}",
                    pattern.report_type_id
                ));
            }
            if !self.block_library.contains_key(&pattern.block_id) {
                return Err(anyhow!(
                    "verdict_pattern for {} references unknown block_id {}",
                    pattern.report_type_id,
                    pattern.block_id
                ));
            }
        }

        Ok(())
    }
}
