use reqwest::StatusCode;
use serde::Serialize;
use serde_json::{json, Value};

use crate::{
    error::{AppError, AppResult},
    models::{CompletionMetadata, Resolution},
};

const OPENROUTER_ENDPOINT: &str = "https://openrouter.ai/api/v1/chat/completions";
const DEFAULT_MODEL: &str = "google/gemini-3-pro-image-preview";

#[derive(Debug, Clone)]
pub struct OpenRouterConfig {
    pub api_key: Option<String>,
    pub model: String,
    pub referer: Option<String>,
    pub title: Option<String>,
}

impl OpenRouterConfig {
    pub fn from_env() -> Self {
        let api_key = std::env::var("OPENROUTER_API_KEY")
            .ok()
            .filter(|v| !v.trim().is_empty());
        let model = std::env::var("OPENROUTER_MODEL")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_MODEL.to_string());
        let referer = std::env::var("OPENROUTER_REFERER")
            .ok()
            .filter(|v| !v.trim().is_empty());
        let title = std::env::var("OPENROUTER_TITLE")
            .ok()
            .filter(|v| !v.trim().is_empty());

        Self {
            api_key,
            model,
            referer,
            title,
        }
    }

    fn require_api_key(&self) -> AppResult<&str> {
        self.api_key.as_deref().ok_or_else(|| {
            AppError::msg("OPENROUTER_API_KEY is missing. Add it to apps/desktop/.env")
        })
    }
}

#[derive(Debug, Clone)]
pub struct OpenRouterClient {
    http_client: reqwest::Client,
    config: OpenRouterConfig,
}

#[derive(Debug, Clone)]
pub struct GenerateImageRequest {
    pub prompt: String,
    pub image_data_url: Option<String>,
    pub aspect_ratio: Option<String>,
    pub resolution: Resolution,
}

#[derive(Debug, Clone)]
pub struct OpenRouterResponse {
    pub model: String,
    pub text: Option<String>,
    pub image_data_urls: Vec<String>,
    pub sanitized_payload: Value,
    pub completion: Option<CompletionMetadata>,
}

impl OpenRouterClient {
    pub fn new(config: OpenRouterConfig) -> Self {
        Self {
            http_client: reqwest::Client::new(),
            config,
        }
    }

    pub async fn generate_image(
        &self,
        request: GenerateImageRequest,
    ) -> AppResult<OpenRouterResponse> {
        let payload = build_payload(&self.config.model, &request);
        let payload_value = serde_json::to_value(&payload)?;
        let sanitized_payload = sanitize_payload(payload_value.clone());

        let mut req = self
            .http_client
            .post(OPENROUTER_ENDPOINT)
            .header(
                "Authorization",
                format!("Bearer {}", self.config.require_api_key()?),
            )
            .header("Content-Type", "application/json")
            .json(&payload_value);

        if let Some(referer) = &self.config.referer {
            req = req.header("HTTP-Referer", referer);
        }

        if let Some(title) = &self.config.title {
            req = req.header("X-Title", title);
        }

        let response = req.send().await?;
        let status = response.status();
        let body = response.text().await?;

        if !status.is_success() {
            return Err(parse_openrouter_http_error(status, &body));
        }

        let response_json: Value = serde_json::from_str(&body)?;
        let image_data_urls = extract_image_data_urls(&response_json);

        let text = extract_text(&response_json);
        let completion = extract_completion_metadata(&response_json);
        let model = response_json
            .get("model")
            .and_then(Value::as_str)
            .unwrap_or(&self.config.model)
            .to_string();

        Ok(OpenRouterResponse {
            model,
            text,
            image_data_urls,
            sanitized_payload,
            completion,
        })
    }
}

#[derive(Debug, Serialize)]
struct ChatPayload {
    model: String,
    modalities: Vec<&'static str>,
    messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    image_config: Option<ImageConfig>,
}

#[derive(Debug, Serialize)]
struct ChatMessage {
    role: &'static str,
    content: Vec<ContentPart>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ContentPart {
    Text { text: String },
    ImageUrl { image_url: ImageUrlPayload },
}

#[derive(Debug, Serialize)]
struct ImageUrlPayload {
    url: String,
}

#[derive(Debug, Serialize)]
struct ImageConfig {
    image_size: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    aspect_ratio: Option<String>,
}

fn build_payload(model: &str, request: &GenerateImageRequest) -> ChatPayload {
    let mut content = vec![ContentPart::Text {
        text: request.prompt.clone(),
    }];

    if let Some(image_data_url) = &request.image_data_url {
        content.push(ContentPart::ImageUrl {
            image_url: ImageUrlPayload {
                url: image_data_url.clone(),
            },
        });
    }

    ChatPayload {
        model: model.to_string(),
        modalities: vec!["image", "text"],
        messages: vec![ChatMessage {
            role: "user",
            content,
        }],
        image_config: Some(ImageConfig {
            image_size: request.resolution.as_openrouter_value().to_string(),
            aspect_ratio: request.aspect_ratio.clone(),
        }),
    }
}

fn parse_openrouter_http_error(status: StatusCode, body: &str) -> AppError {
    let openrouter_error = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|json| {
            json.get("error")
                .and_then(|v| v.get("message").or_else(|| v.get("metadata")))
                .and_then(|v| v.as_str())
                .map(str::to_string)
                .or_else(|| {
                    json.get("message")
                        .and_then(|v| v.as_str())
                        .map(str::to_string)
                })
        })
        .unwrap_or_else(|| body.to_string());

    AppError::msg(format!(
        "OpenRouter request failed ({status}): {openrouter_error}"
    ))
}

fn extract_text(response: &Value) -> Option<String> {
    let message = response.pointer("/choices/0/message")?;

    if let Some(content) = message.get("content") {
        if let Some(text) = content.as_str() {
            if !text.trim().is_empty() {
                return Some(text.to_string());
            }
        }

        if let Some(parts) = content.as_array() {
            let merged = parts
                .iter()
                .filter_map(|part| part.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("\n");
            if !merged.trim().is_empty() {
                return Some(merged);
            }
        }
    }

    None
}

fn extract_image_data_urls(response: &Value) -> Vec<String> {
    let mut images = Vec::new();

    if let Some(message) = response.pointer("/choices/0/message") {
        if let Some(image_array) = message.get("images").and_then(Value::as_array) {
            for image in image_array {
                match image {
                    Value::String(url) if url.starts_with("data:image") => images.push(url.clone()),
                    Value::Object(obj) => {
                        if let Some(url) = obj
                            .get("url")
                            .or_else(|| obj.get("data"))
                            .and_then(Value::as_str)
                        {
                            if url.starts_with("data:image") {
                                images.push(url.to_string());
                            }
                        }

                        if let Some(url) = obj
                            .get("image_url")
                            .or_else(|| obj.get("imageUrl"))
                            .and_then(|value| value.get("url"))
                            .and_then(Value::as_str)
                        {
                            if url.starts_with("data:image") {
                                images.push(url.to_string());
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        if images.is_empty() {
            if let Some(parts) = message.get("content").and_then(Value::as_array) {
                for part in parts {
                    let direct_url = part
                        .get("url")
                        .or_else(|| part.get("image_url"))
                        .and_then(Value::as_str);
                    if let Some(url) = direct_url {
                        if url.starts_with("data:image") {
                            images.push(url.to_string());
                        }
                    }

                    let nested_url = part
                        .get("image_url")
                        .or_else(|| part.get("imageUrl"))
                        .and_then(|value| value.get("url"))
                        .and_then(Value::as_str);
                    if let Some(url) = nested_url {
                        if url.starts_with("data:image") {
                            images.push(url.to_string());
                        }
                    }
                }
            }
        }
    }

    images
}

fn extract_completion_metadata(response: &Value) -> Option<CompletionMetadata> {
    let finish_reason = response
        .pointer("/choices/0/finish_reason")
        .and_then(Value::as_str)
        .map(str::to_string);
    let message = response.pointer("/choices/0/message");

    let refusal = message
        .and_then(|value| value.get("refusal"))
        .and_then(to_string_value);
    let reasoning = message
        .and_then(|value| value.get("reasoning"))
        .and_then(to_string_value);
    let reasoning_details = message
        .and_then(|value| {
            value
                .get("reasoning_details")
                .or_else(|| value.get("reasoningDetails"))
        })
        .and_then(extract_reasoning_details_text);

    if finish_reason.is_none()
        && refusal.is_none()
        && reasoning.is_none()
        && reasoning_details.is_none()
    {
        return None;
    }

    Some(CompletionMetadata {
        finish_reason,
        refusal,
        reasoning,
        reasoning_details,
    })
}

fn to_string_value(value: &Value) -> Option<String> {
    match value {
        Value::Null => None,
        Value::String(text) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        _ => serde_json::to_string_pretty(value).ok(),
    }
}

fn extract_reasoning_details_text(value: &Value) -> Option<String> {
    match value {
        Value::Null => None,
        Value::String(text) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        _ => {
            let mut snippets = Vec::new();
            collect_reasoning_text_snippets(value, &mut snippets);
            if snippets.is_empty() {
                None
            } else {
                Some(snippets.join("\n\n"))
            }
        }
    }
}

fn collect_reasoning_text_snippets(value: &Value, snippets: &mut Vec<String>) {
    match value {
        Value::Array(items) => {
            for item in items {
                collect_reasoning_text_snippets(item, snippets);
            }
        }
        Value::Object(map) => {
            if let Some(text) = map.get("text").and_then(Value::as_str) {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    snippets.push(trimmed.to_string());
                }
            }

            for (key, nested) in map {
                if key == "text" {
                    continue;
                }

                if matches!(nested, Value::Array(_) | Value::Object(_)) {
                    collect_reasoning_text_snippets(nested, snippets);
                }
            }
        }
        _ => {}
    }
}

fn sanitize_payload(payload: Value) -> Value {
    fn walk(value: &mut Value) {
        match value {
            Value::Object(map) => {
                for value in map.values_mut() {
                    walk(value);
                }
            }
            Value::Array(array) => {
                for value in array.iter_mut() {
                    walk(value);
                }
            }
            Value::String(text) => {
                if text.starts_with("data:image") {
                    *value = json!("[omitted image data URL]");
                }
            }
            _ => {}
        }
    }

    let mut sanitized = payload;
    walk(&mut sanitized);
    sanitized
}
