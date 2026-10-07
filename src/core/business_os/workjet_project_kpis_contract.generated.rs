// Generated from src/core/rxdb/tests/fixtures/workjet-project-kpis-v1.json. Do not edit.
#![allow(dead_code)]
use serde::{Deserialize, Serialize};

pub(crate) const CONTRACT_VERSION: u64 = 1;
pub(crate) const CONTRACT_SCHEMA: &str = "ctox.workjet.project_kpis.v1";

pub(crate) trait WireValidate {
    fn validate(&self) -> Result<(), String>;
}
impl WireValidate for String {
    fn validate(&self) -> Result<(), String> {
        Ok(())
    }
}
impl WireValidate for bool {
    fn validate(&self) -> Result<(), String> {
        Ok(())
    }
}
impl WireValidate for u64 {
    fn validate(&self) -> Result<(), String> {
        if *self > 9_007_199_254_740_991 {
            return Err("unsafe JSON integer".into());
        }
        Ok(())
    }
}
impl WireValidate for i64 {
    fn validate(&self) -> Result<(), String> {
        if self.unsigned_abs() > 9_007_199_254_740_991 {
            return Err("unsafe JSON integer".into());
        }
        Ok(())
    }
}
impl WireValidate for f64 {
    fn validate(&self) -> Result<(), String> {
        if !self.is_finite() {
            return Err("non-finite number".into());
        }
        Ok(())
    }
}
impl<T: WireValidate> WireValidate for Vec<T> {
    fn validate(&self) -> Result<(), String> {
        for item in self {
            item.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) enum KpiState {
    #[serde(rename = "resolving")]
    Resolving,
    #[serde(rename = "ready")]
    Ready,
    #[serde(rename = "stale")]
    Stale,
    #[serde(rename = "missing_source")]
    MissingSource,
    #[serde(rename = "failed")]
    Failed,
}
impl WireValidate for KpiState {
    fn validate(&self) -> Result<(), String> {
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) enum SourceKind {
    #[serde(rename = "native_metric")]
    NativeMetric,
    #[serde(rename = "github_metric")]
    GithubMetric,
    #[serde(rename = "connected_metric")]
    ConnectedMetric,
}
impl WireValidate for SourceKind {
    fn validate(&self) -> Result<(), String> {
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) enum Calculation {
    #[serde(rename = "identity")]
    Identity,
    #[serde(rename = "sum")]
    Sum,
    #[serde(rename = "average")]
    Average,
    #[serde(rename = "percentage")]
    Percentage,
}
impl WireValidate for Calculation {
    fn validate(&self) -> Result<(), String> {
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PromptInput {
    pub(crate) kpi_id: String,
    pub(crate) prompt: String,
}
impl WireValidate for PromptInput {
    fn validate(&self) -> Result<(), String> {
        {
            let value = &self.kpi_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("PromptInput.kpi_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("PromptInput.kpi_id violates max_chars".into());
            }
        }
        {
            let value = &self.prompt;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("PromptInput.prompt violates min_chars".into());
            }
            if value.chars().count() > 1024 {
                return Err("PromptInput.prompt violates max_chars".into());
            }
        }
        validate_rules(
            "PromptInput",
            &serde_json::to_value(self).map_err(|e| e.to_string())?,
        )?;
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct KpiPrompt {
    pub(crate) kpi_id: String,
    pub(crate) prompt: String,
    pub(crate) revision: u64,
}
impl WireValidate for KpiPrompt {
    fn validate(&self) -> Result<(), String> {
        {
            let value = &self.kpi_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("KpiPrompt.kpi_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("KpiPrompt.kpi_id violates max_chars".into());
            }
        }
        {
            let value = &self.prompt;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("KpiPrompt.prompt violates min_chars".into());
            }
            if value.chars().count() > 1024 {
                return Err("KpiPrompt.prompt violates max_chars".into());
            }
        }
        {
            let value = &self.revision;
            value.validate()?;
            if *value < 1 {
                return Err("KpiPrompt.revision violates minimum".into());
            }
        }
        validate_rules(
            "KpiPrompt",
            &serde_json::to_value(self).map_err(|e| e.to_string())?,
        )?;
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SourceEvidence {
    pub(crate) source_key: String,
    pub(crate) kind: SourceKind,
    pub(crate) connection_id: String,
    pub(crate) metric_key: String,
    pub(crate) project_id: String,
    pub(crate) snapshot_revision: String,
    pub(crate) evidence_ref: String,
    pub(crate) observed_at_ms: i64,
    #[serde(serialize_with = "serialize_wire_number")]
    pub(crate) value: f64,
}
impl WireValidate for SourceEvidence {
    fn validate(&self) -> Result<(), String> {
        {
            let value = &self.source_key;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("SourceEvidence.source_key violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("SourceEvidence.source_key violates max_chars".into());
            }
        }
        {
            let value = &self.kind;
            value.validate()?;
        }
        {
            let value = &self.connection_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("SourceEvidence.connection_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("SourceEvidence.connection_id violates max_chars".into());
            }
        }
        {
            let value = &self.metric_key;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("SourceEvidence.metric_key violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("SourceEvidence.metric_key violates max_chars".into());
            }
        }
        {
            let value = &self.project_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("SourceEvidence.project_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("SourceEvidence.project_id violates max_chars".into());
            }
        }
        {
            let value = &self.snapshot_revision;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("SourceEvidence.snapshot_revision violates min_chars".into());
            }
            if value.chars().count() > 256 {
                return Err("SourceEvidence.snapshot_revision violates max_chars".into());
            }
        }
        {
            let value = &self.evidence_ref;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("SourceEvidence.evidence_ref violates min_chars".into());
            }
            if value.chars().count() > 256 {
                return Err("SourceEvidence.evidence_ref violates max_chars".into());
            }
        }
        {
            let value = &self.observed_at_ms;
            value.validate()?;
            if *value < 0 {
                return Err("SourceEvidence.observed_at_ms violates minimum".into());
            }
        }
        {
            let value = &self.value;
            value.validate()?;
        }
        validate_rules(
            "SourceEvidence",
            &serde_json::to_value(self).map_err(|e| e.to_string())?,
        )?;
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Computation {
    pub(crate) recipe_id: String,
    pub(crate) revision: u64,
    pub(crate) operation: Calculation,
    pub(crate) input_keys: Vec<String>,
    pub(crate) window_start_ms: i64,
    pub(crate) window_end_ms: i64,
}
impl WireValidate for Computation {
    fn validate(&self) -> Result<(), String> {
        {
            let value = &self.recipe_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("Computation.recipe_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("Computation.recipe_id violates max_chars".into());
            }
        }
        {
            let value = &self.revision;
            value.validate()?;
            if *value < 1 {
                return Err("Computation.revision violates minimum".into());
            }
        }
        {
            let value = &self.operation;
            value.validate()?;
        }
        {
            let value = &self.input_keys;
            value.validate()?;
            if value.len() < 1 {
                return Err("Computation.input_keys violates min_items".into());
            }
            if value.len() > 8 {
                return Err("Computation.input_keys violates max_items".into());
            }
        }
        {
            let value = &self.window_start_ms;
            value.validate()?;
            if *value < 0 {
                return Err("Computation.window_start_ms violates minimum".into());
            }
        }
        {
            let value = &self.window_end_ms;
            value.validate()?;
            if *value < 0 {
                return Err("Computation.window_end_ms violates minimum".into());
            }
        }
        validate_rules(
            "Computation",
            &serde_json::to_value(self).map_err(|e| e.to_string())?,
        )?;
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Freshness {
    pub(crate) calculated_at_ms: i64,
    pub(crate) refresh_at_ms: i64,
    pub(crate) fresh_until_ms: i64,
}
impl WireValidate for Freshness {
    fn validate(&self) -> Result<(), String> {
        {
            let value = &self.calculated_at_ms;
            value.validate()?;
            if *value < 0 {
                return Err("Freshness.calculated_at_ms violates minimum".into());
            }
        }
        {
            let value = &self.refresh_at_ms;
            value.validate()?;
            if *value < 0 {
                return Err("Freshness.refresh_at_ms violates minimum".into());
            }
        }
        {
            let value = &self.fresh_until_ms;
            value.validate()?;
            if *value < 0 {
                return Err("Freshness.fresh_until_ms violates minimum".into());
            }
        }
        validate_rules(
            "Freshness",
            &serde_json::to_value(self).map_err(|e| e.to_string())?,
        )?;
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct KpiSnapshot {
    pub(crate) project_id: String,
    pub(crate) kpi_id: String,
    pub(crate) prompt_revision: u64,
    pub(crate) label: String,
    #[serde(serialize_with = "serialize_wire_number")]
    pub(crate) value: f64,
    pub(crate) unit: String,
    pub(crate) display_value: String,
    pub(crate) sources: Vec<SourceEvidence>,
    pub(crate) computation: Computation,
    pub(crate) freshness: Freshness,
}
impl WireValidate for KpiSnapshot {
    fn validate(&self) -> Result<(), String> {
        {
            let value = &self.project_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("KpiSnapshot.project_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("KpiSnapshot.project_id violates max_chars".into());
            }
        }
        {
            let value = &self.kpi_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("KpiSnapshot.kpi_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("KpiSnapshot.kpi_id violates max_chars".into());
            }
        }
        {
            let value = &self.prompt_revision;
            value.validate()?;
            if *value < 1 {
                return Err("KpiSnapshot.prompt_revision violates minimum".into());
            }
        }
        {
            let value = &self.label;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("KpiSnapshot.label violates min_chars".into());
            }
            if value.chars().count() > 24 {
                return Err("KpiSnapshot.label violates max_chars".into());
            }
        }
        {
            let value = &self.value;
            value.validate()?;
        }
        {
            let value = &self.unit;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("KpiSnapshot.unit violates min_chars".into());
            }
            if value.chars().count() > 16 {
                return Err("KpiSnapshot.unit violates max_chars".into());
            }
        }
        {
            let value = &self.display_value;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("KpiSnapshot.display_value violates min_chars".into());
            }
            if value.chars().count() > 32 {
                return Err("KpiSnapshot.display_value violates max_chars".into());
            }
        }
        {
            let value = &self.sources;
            value.validate()?;
            if value.len() < 1 {
                return Err("KpiSnapshot.sources violates min_items".into());
            }
            if value.len() > 8 {
                return Err("KpiSnapshot.sources violates max_items".into());
            }
        }
        {
            let value = &self.computation;
            value.validate()?;
        }
        {
            let value = &self.freshness;
            value.validate()?;
        }
        validate_rules(
            "KpiSnapshot",
            &serde_json::to_value(self).map_err(|e| e.to_string())?,
        )?;
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct KpiResult {
    pub(crate) status: KpiState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) snapshot: Option<KpiSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) reason_code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) message: Option<String>,
}
impl WireValidate for KpiResult {
    fn validate(&self) -> Result<(), String> {
        {
            let value = &self.status;
            value.validate()?;
        }
        if let Some(value) = &self.snapshot {
            value.validate()?;
        }
        if let Some(value) = &self.reason_code {
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("KpiResult.reason_code violates min_chars".into());
            }
            if value.chars().count() > 64 {
                return Err("KpiResult.reason_code violates max_chars".into());
            }
        }
        if let Some(value) = &self.message {
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("KpiResult.message violates min_chars".into());
            }
            if value.chars().count() > 256 {
                return Err("KpiResult.message violates max_chars".into());
            }
        }
        validate_rules(
            "KpiResult",
            &serde_json::to_value(self).map_err(|e| e.to_string())?,
        )?;
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct KpiRecord {
    pub(crate) prompt: KpiPrompt,
    pub(crate) result: KpiResult,
}
impl WireValidate for KpiRecord {
    fn validate(&self) -> Result<(), String> {
        {
            let value = &self.prompt;
            value.validate()?;
        }
        {
            let value = &self.result;
            value.validate()?;
        }
        validate_rules(
            "KpiRecord",
            &serde_json::to_value(self).map_err(|e| e.to_string())?,
        )?;
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProjectKpis {
    pub(crate) project_id: String,
    pub(crate) revision: u64,
    pub(crate) items: Vec<KpiRecord>,
}
impl WireValidate for ProjectKpis {
    fn validate(&self) -> Result<(), String> {
        {
            let value = &self.project_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("ProjectKpis.project_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("ProjectKpis.project_id violates max_chars".into());
            }
        }
        {
            let value = &self.revision;
            value.validate()?;
        }
        {
            let value = &self.items;
            value.validate()?;
            if value.len() > 3 {
                return Err("ProjectKpis.items violates max_items".into());
            }
        }
        validate_rules(
            "ProjectKpis",
            &serde_json::to_value(self).map_err(|e| e.to_string())?,
        )?;
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ConfigureKpisRequest {
    pub(crate) operation_id: String,
    pub(crate) project_id: String,
    pub(crate) expected_revision: u64,
    pub(crate) prompts: Vec<PromptInput>,
}
impl WireValidate for ConfigureKpisRequest {
    fn validate(&self) -> Result<(), String> {
        {
            let value = &self.operation_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("ConfigureKpisRequest.operation_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("ConfigureKpisRequest.operation_id violates max_chars".into());
            }
        }
        {
            let value = &self.project_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("ConfigureKpisRequest.project_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("ConfigureKpisRequest.project_id violates max_chars".into());
            }
        }
        {
            let value = &self.expected_revision;
            value.validate()?;
        }
        {
            let value = &self.prompts;
            value.validate()?;
            if value.len() > 3 {
                return Err("ConfigureKpisRequest.prompts violates max_items".into());
            }
        }
        validate_rules(
            "ConfigureKpisRequest",
            &serde_json::to_value(self).map_err(|e| e.to_string())?,
        )?;
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ResolveKpiRequest {
    pub(crate) operation_id: String,
    pub(crate) project_id: String,
    pub(crate) kpi_id: String,
    pub(crate) prompt_revision: u64,
    pub(crate) expected_revision: u64,
}
impl WireValidate for ResolveKpiRequest {
    fn validate(&self) -> Result<(), String> {
        {
            let value = &self.operation_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("ResolveKpiRequest.operation_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("ResolveKpiRequest.operation_id violates max_chars".into());
            }
        }
        {
            let value = &self.project_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("ResolveKpiRequest.project_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("ResolveKpiRequest.project_id violates max_chars".into());
            }
        }
        {
            let value = &self.kpi_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("ResolveKpiRequest.kpi_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("ResolveKpiRequest.kpi_id violates max_chars".into());
            }
        }
        {
            let value = &self.prompt_revision;
            value.validate()?;
            if *value < 1 {
                return Err("ResolveKpiRequest.prompt_revision violates minimum".into());
            }
        }
        {
            let value = &self.expected_revision;
            value.validate()?;
        }
        validate_rules(
            "ResolveKpiRequest",
            &serde_json::to_value(self).map_err(|e| e.to_string())?,
        )?;
        Ok(())
    }
}

#[cfg(test)]
pub(crate) fn validate_fixture(kind: &str, value: serde_json::Value) -> Result<(), String> {
    match kind {
        "KpiState" => serde_json::from_value::<KpiState>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "SourceKind" => serde_json::from_value::<SourceKind>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "Calculation" => serde_json::from_value::<Calculation>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "PromptInput" => serde_json::from_value::<PromptInput>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "KpiPrompt" => serde_json::from_value::<KpiPrompt>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "SourceEvidence" => serde_json::from_value::<SourceEvidence>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "Computation" => serde_json::from_value::<Computation>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "Freshness" => serde_json::from_value::<Freshness>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "KpiSnapshot" => serde_json::from_value::<KpiSnapshot>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "KpiResult" => serde_json::from_value::<KpiResult>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "KpiRecord" => serde_json::from_value::<KpiRecord>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "ProjectKpis" => serde_json::from_value::<ProjectKpis>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "ConfigureKpisRequest" => serde_json::from_value::<ConfigureKpisRequest>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "ResolveKpiRequest" => serde_json::from_value::<ResolveKpiRequest>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        _ => Err("unknown contract type".into()),
    }
}

fn rules() -> &'static serde_json::Value {
    static RULES: std::sync::OnceLock<serde_json::Value> = std::sync::OnceLock::new();
    RULES.get_or_init(|| serde_json::from_str(r#"{"PromptInput":{"nonblank":["kpi_id","prompt"]},"KpiPrompt":{"nonblank":["kpi_id","prompt"]},"Computation":{"lte":[["window_start_ms","window_end_ms"]],"unique":[{"field":"input_keys"}]},"Freshness":{"lt":[["calculated_at_ms","refresh_at_ms"]],"lte":[["refresh_at_ms","fresh_until_ms"]]},"KpiSnapshot":{"unique":[{"field":"sources","key":"source_key"}],"every_eq":[{"field":"sources","key":"project_id","target":"project_id"}],"every_lte":[{"field":"sources","key":"observed_at_ms","target":"freshness.calculated_at_ms"}],"calculate":true},"KpiResult":{"state_fields":{"field":"status","states":{"resolving":{"forbidden":["snapshot","reason_code","message"]},"ready":{"required":["snapshot"],"forbidden":["reason_code","message"]},"stale":{"required":["snapshot","reason_code","message"]},"missing_source":{"required":["reason_code","message"],"forbidden":["snapshot"]},"failed":{"required":["reason_code","message"],"forbidden":["snapshot"]}}}},"KpiRecord":{"eq":[["prompt.kpi_id","result.snapshot.kpi_id"],["prompt.revision","result.snapshot.prompt_revision"]]},"ProjectKpis":{"unique":[{"field":"items","key":"prompt.kpi_id"}],"every_eq":[{"field":"items","key":"result.snapshot.project_id","target":"project_id"}]},"ConfigureKpisRequest":{"unique":[{"field":"prompts","key":"kpi_id"}]}}"#).expect("generated KPI rules"))
}

// JSON/JS has one numeric type. Preserve integral measurements as integers,
// avoiding a representation-only change (1284 -> 1284.0) on native persistence.
fn serialize_wire_number<S: serde::Serializer>(
    value: &f64,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    if !value.is_finite() {
        return Err(serde::ser::Error::custom("non-finite measurement"));
    }
    if value.abs() <= 9_007_199_254_740_991.0 && value.fract() == 0.0 {
        serializer.serialize_i64(*value as i64)
    } else {
        serializer.serialize_f64(*value)
    }
}
fn at<'a>(value: &'a serde_json::Value, path: &str) -> Option<&'a serde_json::Value> {
    let mut cursor = value;
    for key in path.split('.') {
        cursor = cursor.get(key)?;
    }
    if cursor.is_null() {
        None
    } else {
        Some(cursor)
    }
}
fn text(value: &serde_json::Value) -> &str {
    value.as_str().expect("generated rule string")
}
fn validate_rules(kind: &str, value: &serde_json::Value) -> Result<(), String> {
    let Some(rules) = rules().get(kind) else {
        return Ok(());
    };
    let fail = |rule: &str| format!("{kind}: {rule}");
    if let Some(fields) = rules["nonblank"].as_array() {
        for field in fields {
            if at(value, text(field))
                .and_then(|v| v.as_str())
                .is_some_and(|v| v.chars().all(|c| matches!(c, ' ' | '\t' | '\r' | '\n')))
            {
                return Err(fail("blank prompt or identity"));
            }
        }
    }
    for operation in ["eq", "lt", "lte"] {
        if let Some(pairs) = rules[operation].as_array() {
            for pair in pairs {
                let (Some(left), Some(right)) =
                    (at(value, text(&pair[0])), at(value, text(&pair[1])))
                else {
                    continue;
                };
                let ok = match operation {
                    "eq" => left == right,
                    "lt" => left.as_i64() < right.as_i64(),
                    _ => left.as_i64() <= right.as_i64(),
                };
                if !ok {
                    return Err(fail(operation));
                }
            }
        }
    }
    if let Some(items) = rules["unique"].as_array() {
        for rule in items {
            let Some(values) = at(value, text(&rule["field"])).and_then(|v| v.as_array()) else {
                continue;
            };
            let keys: Vec<_> = values
                .iter()
                .filter_map(|v| {
                    if let Some(key) = rule["key"].as_str() {
                        at(v, key)
                    } else {
                        Some(v)
                    }
                })
                .collect();
            for i in 0..keys.len() {
                if keys[i + 1..].contains(&keys[i]) {
                    return Err(fail("duplicate identity or input"));
                }
            }
        }
    }
    for operation in ["every_eq", "every_lte"] {
        if let Some(items) = rules[operation].as_array() {
            for rule in items {
                let target = at(value, text(&rule["target"]));
                if let Some(values) = at(value, text(&rule["field"])).and_then(|v| v.as_array()) {
                    for item in values {
                        if let (Some(left), Some(right)) = (at(item, text(&rule["key"])), target) {
                            let ok = if operation == "every_eq" {
                                left == right
                            } else {
                                left.as_i64() <= right.as_i64()
                            };
                            if !ok {
                                return Err(fail(operation));
                            }
                        }
                    }
                }
            }
        }
    }
    if let Some(state_rule) = rules.get("state_fields") {
        let state = at(value, text(&state_rule["field"]))
            .and_then(|v| v.as_str())
            .ok_or_else(|| fail("missing state"))?;
        let state_fields = &state_rule["states"][state];
        for (mode, required) in [("required", true), ("forbidden", false)] {
            if let Some(fields) = state_fields[mode].as_array() {
                for field in fields {
                    if at(value, text(field)).is_some() != required {
                        return Err(fail(mode));
                    }
                }
            }
        }
    }
    if rules["calculate"] == true {
        let sources = value["sources"].as_array().ok_or_else(|| fail("sources"))?;
        let keys = value["computation"]["input_keys"]
            .as_array()
            .ok_or_else(|| fail("input keys"))?;
        if sources.len() != keys.len() {
            return Err(fail("all evidence must be consumed"));
        }
        let mut inputs = Vec::new();
        for key in keys {
            let source = sources
                .iter()
                .find(|s| s["source_key"] == *key)
                .ok_or_else(|| fail("unknown input"))?;
            inputs.push(
                source["value"]
                    .as_f64()
                    .ok_or_else(|| fail("numeric input"))?,
            );
        }
        let expected = match value["computation"]["operation"].as_str() {
            Some("identity") if inputs.len() == 1 => inputs[0],
            Some("sum") => inputs.iter().sum(),
            Some("average") => inputs.iter().sum::<f64>() / (inputs.len() as f64),
            Some("percentage") if inputs.len() == 2 && inputs[1] != 0.0 => {
                100.0 * inputs[0] / inputs[1]
            }
            _ => return Err(fail("invalid calculation arity or denominator")),
        };
        if !expected.is_finite() || value["value"].as_f64() != Some(expected) {
            return Err(fail("value differs from evidence"));
        }
    }
    Ok(())
}
