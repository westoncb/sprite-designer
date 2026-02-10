use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSummary {
    pub id: String,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub child_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub children: Vec<Child>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRecord {
    pub id: String,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub child_ids: Vec<String>,
}

impl ProjectRecord {
    pub fn to_summary(&self) -> ProjectSummary {
        ProjectSummary {
            id: self.id.clone(),
            name: self.name.clone(),
            created_at: self.created_at,
            updated_at: self.updated_at,
            child_count: self.child_ids.len(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ChildType {
    Generate,
    Edit,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ChildMode {
    Sprite,
    Normal,
    Edit,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Resolution {
    #[serde(rename = "1K")]
    OneK,
    #[serde(rename = "2K")]
    TwoK,
    #[serde(rename = "4K")]
    FourK,
}

impl Resolution {
    pub fn as_openrouter_value(&self) -> &'static str {
        match self {
            Self::OneK => "1K",
            Self::TwoK => "2K",
            Self::FourK => "4K",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ChildInputs {
    pub rows: Option<u32>,
    pub cols: Option<u32>,
    pub object_description: Option<String>,
    pub style: Option<String>,
    pub camera_angle: Option<String>,
    pub prompt_text: Option<String>,
    pub edit_prompt: Option<String>,
    pub base_child_id: Option<String>,
    pub resolution: Option<Resolution>,
    pub image_prior_data_url: Option<String>,
    pub base_image_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenRouterSnapshot {
    pub model: String,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ChildOutputs {
    pub text: Option<String>,
    pub image_paths: Vec<String>,
    pub primary_image_path: Option<String>,
    pub completion: Option<CompletionMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CompletionMetadata {
    pub finish_reason: Option<String>,
    pub refusal: Option<String>,
    pub reasoning: Option<String>,
    pub reasoning_details: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Child {
    pub id: String,
    pub project_id: String,
    pub r#type: ChildType,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub mode: ChildMode,
    pub inputs: ChildInputs,
    pub openrouter: OpenRouterSnapshot,
    pub outputs: ChildOutputs,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChildResult {
    pub project: ProjectSummary,
    pub child: Child,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateRequest {
    pub project_id: Option<String>,
    pub name: Option<String>,
    pub sprite_mode: bool,
    pub rows: Option<u32>,
    pub cols: Option<u32>,
    pub object_description: Option<String>,
    pub style: Option<String>,
    pub camera_angle: Option<String>,
    pub prompt_text: Option<String>,
    pub resolution: Resolution,
    pub image_prior_data_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditRequest {
    pub project_id: String,
    pub base_child_id: String,
    pub name: Option<String>,
    pub edit_prompt: String,
    pub resolution: Option<Resolution>,
    pub base_image_data_url: Option<String>,
    pub base_image_path: Option<String>,
}
