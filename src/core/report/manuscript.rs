//! `Manuscript v1` schema. The deterministic intermediate that the renderer
//! consumes. The draft stage produces this from typed DB rows; the renderer
//! never invents content.

use serde::Deserialize;
use serde::Serialize;

pub const MANUSCRIPT_SCHEMA_VERSION: &str = "ctox.report.manuscript/v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manuscript {
    pub schema: String,
    pub run_id: String,
    pub preset: String,
    pub language: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub version_label: String,
    pub scope: ScopeBlock,
    pub sections: Vec<Section>,
    pub citation_register: Vec<Citation>,
}

impl Manuscript {
    pub fn new(
        run_id: String,
        preset: String,
        language: String,
        title: String,
        subtitle: Option<String>,
        version_label: String,
        scope: ScopeBlock,
    ) -> Self {
        Self {
            schema: MANUSCRIPT_SCHEMA_VERSION.to_string(),
            run_id,
            preset,
            language,
            title,
            subtitle,
            version_label,
            scope,
            sections: Vec::new(),
            citation_register: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScopeBlock {
    pub leading_questions: Vec<String>,
    pub out_of_scope: Vec<String>,
    pub assumptions: Vec<String>,
    pub disclaimer_md: String,
    pub success_criteria: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Section {
    pub id: String,
    pub heading_level: u32,
    pub heading: String,
    pub blocks: Vec<Block>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Block {
    Paragraph {
        text_md: String,
        evidence_ids: Vec<String>,
    },
    Bullets {
        items: Vec<BulletItem>,
    },
    Numbered {
        items: Vec<BulletItem>,
    },
    OptionsTable {
        options: Vec<OptionRow>,
    },
    RequirementsTable {
        rows: Vec<RequirementRow>,
    },
    MatrixTable {
        matrix_kind: String,
        label: String,
        axes: Vec<MatrixAxis>,
        rows: Vec<MatrixRow>,
    },
    ScenarioBlock {
        code: String,
        label: String,
        description_md: String,
    },
    RiskRegister {
        rows: Vec<RiskRow>,
    },
    CitationRegister,
    Note {
        text_md: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BulletItem {
    pub text_md: String,
    pub evidence_ids: Vec<String>,
    #[serde(default)]
    pub primary_recommendation: bool,
    #[serde(default)]
    pub assumption_note_md: Option<String>,
    #[serde(default)]
    pub scenario_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptionRow {
    pub code: String,
    pub label: String,
    pub summary_md: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequirementRow {
    pub code: String,
    pub title: String,
    pub must_have: bool,
    pub description_md: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatrixAxis {
    pub code: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatrixRow {
    pub option_code: String,
    pub option_label: String,
    pub cells: Vec<MatrixCell>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatrixCell {
    pub axis_code: String,
    pub value_label: String,
    pub value_numeric: Option<f64>,
    pub rationale_md: String,
    pub evidence_ids: Vec<String>,
    pub assumption_note_md: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskRow {
    pub code: String,
    pub title: String,
    pub description_md: String,
    pub mitigation_md: String,
    pub likelihood: Option<String>,
    pub impact: Option<String>,
    pub evidence_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Citation {
    pub evidence_id: String,
    pub display_index: usize,
    pub citation_kind: String,
    pub canonical_id: String,
    pub title: Option<String>,
    pub authors: Vec<String>,
    pub venue: Option<String>,
    pub year: Option<i64>,
    pub landing_url: Option<String>,
    pub full_text_url: Option<String>,
}

pub fn body_hash(manuscript: &Manuscript) -> String {
    use sha2::Digest;
    use sha2::Sha256;
    let canonical = serde_json::to_string(manuscript).unwrap_or_default();
    let digest = Sha256::digest(canonical.as_bytes());
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        hex.push(char::from_digit(((byte >> 4) & 0xF) as u32, 16).unwrap());
        hex.push(char::from_digit((byte & 0xF) as u32, 16).unwrap());
    }
    hex
}
