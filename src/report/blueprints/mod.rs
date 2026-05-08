//! Static blueprint registry. Blueprints are TOML files compiled into the
//! binary via `include_str!` so a release build always carries the full set
//! shipped with that build.

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use serde::Deserialize;
use std::collections::BTreeMap;

const FEASIBILITY_TOML: &str = include_str!("feasibility.toml");

#[derive(Debug, Clone, Deserialize)]
pub struct Blueprint {
    pub schema_version: String,
    pub preset: String,
    pub title_en: String,
    pub title_de: String,
    #[serde(default = "default_language")]
    pub default_language: String,
    pub bounds: Bounds,
    pub sections: Vec<Section>,
    #[serde(default)]
    pub matrices: BTreeMap<String, MatrixDef>,
    pub validators: BTreeMap<String, String>,
    #[serde(default)]
    pub disclaimer: Disclaimer,
}

fn default_language() -> String {
    "en".to_string()
}

#[derive(Debug, Clone, Deserialize)]
pub struct Bounds {
    pub min_options: usize,
    pub min_scenarios: usize,
    pub min_evidence_count: usize,
    pub min_leading_questions: usize,
    pub min_disclaimer_chars: usize,
    pub max_revise_iterations: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Section {
    pub id: String,
    pub heading_level: u32,
    pub kind: SectionKind,
    #[serde(default)]
    pub requires_claim: bool,
    #[serde(default)]
    pub min_claims: usize,
    #[serde(default)]
    pub min_claims_per_option: usize,
    #[serde(default)]
    pub matrix_kind: Option<String>,
    #[serde(default)]
    pub require_primary_recommendation: bool,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SectionKind {
    Deterministic,
    Claims,
    Matrix,
    RiskRegister,
    CitationRegister,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MatrixDef {
    pub label_en: String,
    #[serde(default)]
    pub label_de: Option<String>,
    #[serde(default)]
    pub axis_codes: Vec<String>,
    #[serde(default)]
    pub axis_labels_en: Vec<String>,
    pub required: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Disclaimer {
    #[serde(default)]
    pub must_contain_all: Vec<String>,
    #[serde(default)]
    pub must_contain_any: Vec<String>,
}

pub fn load(preset: &str) -> Result<Blueprint> {
    let raw = match preset {
        "feasibility" => FEASIBILITY_TOML,
        other => bail!("unknown preset '{other}'; supported: feasibility"),
    };
    toml::from_str::<Blueprint>(raw)
        .with_context(|| format!("failed to parse blueprint TOML for preset '{preset}'"))
}

pub fn list() -> Vec<&'static str> {
    vec!["feasibility"]
}

pub fn validator_severity(blueprint: &Blueprint, validator: &str) -> ValidatorSeverity {
    match blueprint.validators.get(validator).map(String::as_str) {
        Some("hard") => ValidatorSeverity::Hard,
        Some("soft") => ValidatorSeverity::Soft,
        _ => ValidatorSeverity::Disabled,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidatorSeverity {
    Hard,
    Soft,
    Disabled,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feasibility_blueprint_parses() {
        let bp = load("feasibility").unwrap();
        assert_eq!(bp.preset, "feasibility");
        assert!(bp.bounds.min_options >= 3);
        assert!(bp.sections.iter().any(|s| s.id == "recommendation"));
        assert!(bp.matrices.contains_key("main"));
        assert_eq!(
            validator_severity(&bp, "every_claim_has_fk_evidence"),
            ValidatorSeverity::Hard
        );
        assert_eq!(
            validator_severity(&bp, "urls_resolve"),
            ValidatorSeverity::Soft
        );
    }

    #[test]
    fn unknown_preset_errors() {
        assert!(load("market_research_v0").is_err());
    }

    #[test]
    fn list_contains_feasibility() {
        assert!(list().contains(&"feasibility"));
    }
}
