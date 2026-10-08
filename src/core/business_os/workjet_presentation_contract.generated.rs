// Generated from src/core/rxdb/tests/fixtures/workjet-presentation-v1.json. Do not edit.
#![allow(dead_code)]
use serde::{Deserialize, Serialize};

pub(crate) const CONTRACT_VERSION: u64 = 1;
pub(crate) const CONTRACT_SCHEMA: &str = "ctox.workjet.presentation.v1";

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
pub(crate) enum PresentationSource {
    #[serde(rename = "agent")]
    Agent,
    #[serde(rename = "owner")]
    Owner,
}
impl WireValidate for PresentationSource {
    fn validate(&self) -> Result<(), String> {
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PresentationManifest {
    pub(crate) presentation_id: String,
    pub(crate) project_id: String,
    pub(crate) meeting_id: String,
    pub(crate) owner_user_id: String,
    pub(crate) title: String,
    pub(crate) revision: u64,
    pub(crate) document_schema: String,
    pub(crate) document_file_id: String,
    pub(crate) document_generation_id: String,
    pub(crate) document_sha256: String,
    pub(crate) document_bytes: u64,
    pub(crate) slide_ids: Vec<String>,
    pub(crate) source: PresentationSource,
    pub(crate) updated_by: String,
    pub(crate) updated_at_ms: i64,
}
impl WireValidate for PresentationManifest {
    fn validate(&self) -> Result<(), String> {
        {
            let value = &self.presentation_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("PresentationManifest.presentation_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("PresentationManifest.presentation_id violates max_chars".into());
            }
        }
        {
            let value = &self.project_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("PresentationManifest.project_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("PresentationManifest.project_id violates max_chars".into());
            }
        }
        {
            let value = &self.meeting_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("PresentationManifest.meeting_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("PresentationManifest.meeting_id violates max_chars".into());
            }
        }
        {
            let value = &self.owner_user_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("PresentationManifest.owner_user_id violates min_chars".into());
            }
            if value.chars().count() > 256 {
                return Err("PresentationManifest.owner_user_id violates max_chars".into());
            }
        }
        {
            let value = &self.title;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("PresentationManifest.title violates min_chars".into());
            }
            if value.chars().count() > 180 {
                return Err("PresentationManifest.title violates max_chars".into());
            }
        }
        {
            let value = &self.revision;
            value.validate()?;
            if *value < 1 {
                return Err("PresentationManifest.revision violates minimum".into());
            }
        }
        {
            let value = &self.document_schema;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("PresentationManifest.document_schema violates min_chars".into());
            }
            if value.chars().count() > 64 {
                return Err("PresentationManifest.document_schema violates max_chars".into());
            }
        }
        {
            let value = &self.document_file_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("PresentationManifest.document_file_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("PresentationManifest.document_file_id violates max_chars".into());
            }
        }
        {
            let value = &self.document_generation_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err(
                    "PresentationManifest.document_generation_id violates min_chars".into(),
                );
            }
            if value.chars().count() > 128 {
                return Err(
                    "PresentationManifest.document_generation_id violates max_chars".into(),
                );
            }
        }
        {
            let value = &self.document_sha256;
            value.validate()?;
            if value.chars().count() < 64 {
                return Err("PresentationManifest.document_sha256 violates min_chars".into());
            }
            if value.chars().count() > 64 {
                return Err("PresentationManifest.document_sha256 violates max_chars".into());
            }
        }
        {
            let value = &self.document_bytes;
            value.validate()?;
            if *value < 2 {
                return Err("PresentationManifest.document_bytes violates minimum".into());
            }
            if *value > 8388608 {
                return Err("PresentationManifest.document_bytes violates maximum".into());
            }
        }
        {
            let value = &self.slide_ids;
            value.validate()?;
            if value.is_empty() {
                return Err("PresentationManifest.slide_ids violates min_items".into());
            }
            if value.len() > 160 {
                return Err("PresentationManifest.slide_ids violates max_items".into());
            }
        }
        {
            let value = &self.source;
            value.validate()?;
        }
        {
            let value = &self.updated_by;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("PresentationManifest.updated_by violates min_chars".into());
            }
            if value.chars().count() > 256 {
                return Err("PresentationManifest.updated_by violates max_chars".into());
            }
        }
        {
            let value = &self.updated_at_ms;
            value.validate()?;
            if *value < 0 {
                return Err("PresentationManifest.updated_at_ms violates minimum".into());
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReadPresentationRequest {
    pub(crate) project_id: String,
    pub(crate) meeting_id: String,
}
impl WireValidate for ReadPresentationRequest {
    fn validate(&self) -> Result<(), String> {
        {
            let value = &self.project_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("ReadPresentationRequest.project_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("ReadPresentationRequest.project_id violates max_chars".into());
            }
        }
        {
            let value = &self.meeting_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("ReadPresentationRequest.meeting_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("ReadPresentationRequest.meeting_id violates max_chars".into());
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReadPresentationResponse {
    pub(crate) contract: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) presentation: Option<PresentationManifest>,
}
impl WireValidate for ReadPresentationResponse {
    fn validate(&self) -> Result<(), String> {
        {
            let value = &self.contract;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("ReadPresentationResponse.contract violates min_chars".into());
            }
            if value.chars().count() > 64 {
                return Err("ReadPresentationResponse.contract violates max_chars".into());
            }
        }
        if let Some(value) = &self.presentation {
            value.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReadPresentationContentRequest {
    pub(crate) project_id: String,
    pub(crate) meeting_id: String,
    pub(crate) presentation_id: String,
    pub(crate) revision: u64,
    pub(crate) offset: u64,
    pub(crate) length: u64,
}
impl WireValidate for ReadPresentationContentRequest {
    fn validate(&self) -> Result<(), String> {
        {
            let value = &self.project_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("ReadPresentationContentRequest.project_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("ReadPresentationContentRequest.project_id violates max_chars".into());
            }
        }
        {
            let value = &self.meeting_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("ReadPresentationContentRequest.meeting_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("ReadPresentationContentRequest.meeting_id violates max_chars".into());
            }
        }
        {
            let value = &self.presentation_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err(
                    "ReadPresentationContentRequest.presentation_id violates min_chars".into(),
                );
            }
            if value.chars().count() > 128 {
                return Err(
                    "ReadPresentationContentRequest.presentation_id violates max_chars".into(),
                );
            }
        }
        {
            let value = &self.revision;
            value.validate()?;
            if *value < 1 {
                return Err("ReadPresentationContentRequest.revision violates minimum".into());
            }
        }
        {
            let value = &self.offset;
            value.validate()?;
            if *value > 8388608 {
                return Err("ReadPresentationContentRequest.offset violates maximum".into());
            }
        }
        {
            let value = &self.length;
            value.validate()?;
            if *value < 1 {
                return Err("ReadPresentationContentRequest.length violates minimum".into());
            }
            if *value > 131072 {
                return Err("ReadPresentationContentRequest.length violates maximum".into());
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PresentationContentRange {
    pub(crate) presentation_id: String,
    pub(crate) revision: u64,
    pub(crate) offset: u64,
    pub(crate) length: u64,
    pub(crate) total_bytes: u64,
    pub(crate) document_sha256: String,
    pub(crate) data_base64: String,
}
impl WireValidate for PresentationContentRange {
    fn validate(&self) -> Result<(), String> {
        {
            let value = &self.presentation_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("PresentationContentRange.presentation_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("PresentationContentRange.presentation_id violates max_chars".into());
            }
        }
        {
            let value = &self.revision;
            value.validate()?;
            if *value < 1 {
                return Err("PresentationContentRange.revision violates minimum".into());
            }
        }
        {
            let value = &self.offset;
            value.validate()?;
            if *value > 8388608 {
                return Err("PresentationContentRange.offset violates maximum".into());
            }
        }
        {
            let value = &self.length;
            value.validate()?;
            if *value < 1 {
                return Err("PresentationContentRange.length violates minimum".into());
            }
            if *value > 131072 {
                return Err("PresentationContentRange.length violates maximum".into());
            }
        }
        {
            let value = &self.total_bytes;
            value.validate()?;
            if *value < 2 {
                return Err("PresentationContentRange.total_bytes violates minimum".into());
            }
            if *value > 8388608 {
                return Err("PresentationContentRange.total_bytes violates maximum".into());
            }
        }
        {
            let value = &self.document_sha256;
            value.validate()?;
            if value.chars().count() < 64 {
                return Err("PresentationContentRange.document_sha256 violates min_chars".into());
            }
            if value.chars().count() > 64 {
                return Err("PresentationContentRange.document_sha256 violates max_chars".into());
            }
        }
        {
            let value = &self.data_base64;
            value.validate()?;
            if value.chars().count() < 4 {
                return Err("PresentationContentRange.data_base64 violates min_chars".into());
            }
            if value.chars().count() > 174764 {
                return Err("PresentationContentRange.data_base64 violates max_chars".into());
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SavePresentationCanvasRequest {
    pub(crate) operation_id: String,
    pub(crate) presentation_id: String,
    pub(crate) expected_revision: u64,
    pub(crate) slide_id: String,
    pub(crate) scene_json: String,
}
impl WireValidate for SavePresentationCanvasRequest {
    fn validate(&self) -> Result<(), String> {
        {
            let value = &self.operation_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("SavePresentationCanvasRequest.operation_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("SavePresentationCanvasRequest.operation_id violates max_chars".into());
            }
        }
        {
            let value = &self.presentation_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err(
                    "SavePresentationCanvasRequest.presentation_id violates min_chars".into(),
                );
            }
            if value.chars().count() > 128 {
                return Err(
                    "SavePresentationCanvasRequest.presentation_id violates max_chars".into(),
                );
            }
        }
        {
            let value = &self.expected_revision;
            value.validate()?;
            if *value < 1 {
                return Err(
                    "SavePresentationCanvasRequest.expected_revision violates minimum".into(),
                );
            }
        }
        {
            let value = &self.slide_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("SavePresentationCanvasRequest.slide_id violates min_chars".into());
            }
            if value.chars().count() > 120 {
                return Err("SavePresentationCanvasRequest.slide_id violates max_chars".into());
            }
        }
        {
            let value = &self.scene_json;
            value.validate()?;
            if value.chars().count() < 2 {
                return Err("SavePresentationCanvasRequest.scene_json violates min_chars".into());
            }
            if value.chars().count() > 4194304 {
                return Err("SavePresentationCanvasRequest.scene_json violates max_chars".into());
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ApplyPresentationEditsRequest {
    pub(crate) operation_id: String,
    pub(crate) presentation_id: String,
    pub(crate) expected_revision: u64,
    pub(crate) operations_json: String,
}
impl WireValidate for ApplyPresentationEditsRequest {
    fn validate(&self) -> Result<(), String> {
        {
            let value = &self.operation_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("ApplyPresentationEditsRequest.operation_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("ApplyPresentationEditsRequest.operation_id violates max_chars".into());
            }
        }
        {
            let value = &self.presentation_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err(
                    "ApplyPresentationEditsRequest.presentation_id violates min_chars".into(),
                );
            }
            if value.chars().count() > 128 {
                return Err(
                    "ApplyPresentationEditsRequest.presentation_id violates max_chars".into(),
                );
            }
        }
        {
            let value = &self.expected_revision;
            value.validate()?;
            if *value < 1 {
                return Err(
                    "ApplyPresentationEditsRequest.expected_revision violates minimum".into(),
                );
            }
        }
        {
            let value = &self.operations_json;
            value.validate()?;
            if value.chars().count() < 2 {
                return Err(
                    "ApplyPresentationEditsRequest.operations_json violates min_chars".into(),
                );
            }
            if value.chars().count() > 1048576 {
                return Err(
                    "ApplyPresentationEditsRequest.operations_json violates max_chars".into(),
                );
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PresentationMutationReceipt {
    pub(crate) operation_id: String,
    pub(crate) presentation_id: String,
    pub(crate) project_id: String,
    pub(crate) meeting_id: String,
    pub(crate) revision: u64,
    pub(crate) document_sha256: String,
    pub(crate) document_bytes: u64,
    pub(crate) slide_ids: Vec<String>,
}
impl WireValidate for PresentationMutationReceipt {
    fn validate(&self) -> Result<(), String> {
        {
            let value = &self.operation_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("PresentationMutationReceipt.operation_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("PresentationMutationReceipt.operation_id violates max_chars".into());
            }
        }
        {
            let value = &self.presentation_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err(
                    "PresentationMutationReceipt.presentation_id violates min_chars".into(),
                );
            }
            if value.chars().count() > 128 {
                return Err(
                    "PresentationMutationReceipt.presentation_id violates max_chars".into(),
                );
            }
        }
        {
            let value = &self.project_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("PresentationMutationReceipt.project_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("PresentationMutationReceipt.project_id violates max_chars".into());
            }
        }
        {
            let value = &self.meeting_id;
            value.validate()?;
            if value.chars().count() < 1 {
                return Err("PresentationMutationReceipt.meeting_id violates min_chars".into());
            }
            if value.chars().count() > 128 {
                return Err("PresentationMutationReceipt.meeting_id violates max_chars".into());
            }
        }
        {
            let value = &self.revision;
            value.validate()?;
            if *value < 1 {
                return Err("PresentationMutationReceipt.revision violates minimum".into());
            }
        }
        {
            let value = &self.document_sha256;
            value.validate()?;
            if value.chars().count() < 64 {
                return Err(
                    "PresentationMutationReceipt.document_sha256 violates min_chars".into(),
                );
            }
            if value.chars().count() > 64 {
                return Err(
                    "PresentationMutationReceipt.document_sha256 violates max_chars".into(),
                );
            }
        }
        {
            let value = &self.document_bytes;
            value.validate()?;
            if *value < 2 {
                return Err("PresentationMutationReceipt.document_bytes violates minimum".into());
            }
            if *value > 8388608 {
                return Err("PresentationMutationReceipt.document_bytes violates maximum".into());
            }
        }
        {
            let value = &self.slide_ids;
            value.validate()?;
            if value.is_empty() {
                return Err("PresentationMutationReceipt.slide_ids violates min_items".into());
            }
            if value.len() > 160 {
                return Err("PresentationMutationReceipt.slide_ids violates max_items".into());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
pub(crate) fn validate_fixture(kind: &str, value: serde_json::Value) -> Result<(), String> {
    match kind {
        "PresentationSource" => serde_json::from_value::<PresentationSource>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "PresentationManifest" => serde_json::from_value::<PresentationManifest>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "ReadPresentationRequest" => serde_json::from_value::<ReadPresentationRequest>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "ReadPresentationResponse" => serde_json::from_value::<ReadPresentationResponse>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "ReadPresentationContentRequest" => {
            serde_json::from_value::<ReadPresentationContentRequest>(value)
                .map_err(|e| e.to_string())?
                .validate()
        }
        "PresentationContentRange" => serde_json::from_value::<PresentationContentRange>(value)
            .map_err(|e| e.to_string())?
            .validate(),
        "SavePresentationCanvasRequest" => {
            serde_json::from_value::<SavePresentationCanvasRequest>(value)
                .map_err(|e| e.to_string())?
                .validate()
        }
        "ApplyPresentationEditsRequest" => {
            serde_json::from_value::<ApplyPresentationEditsRequest>(value)
                .map_err(|e| e.to_string())?
                .validate()
        }
        "PresentationMutationReceipt" => {
            serde_json::from_value::<PresentationMutationReceipt>(value)
                .map_err(|e| e.to_string())?
                .validate()
        }
        _ => Err("unknown contract type".into()),
    }
}
