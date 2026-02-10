use crate::{
    error::{AppError, AppResult},
    models::GenerateRequest,
};

const SUPPORTED_ASPECT_RATIOS: [(&str, f64); 7] = [
    ("1:1", 1.0),
    ("4:3", 4.0 / 3.0),
    ("3:4", 3.0 / 4.0),
    ("16:9", 16.0 / 9.0),
    ("9:16", 9.0 / 16.0),
    ("3:2", 3.0 / 2.0),
    ("2:3", 2.0 / 3.0),
];

pub fn build_sprite_prompt(request: &GenerateRequest) -> AppResult<String> {
    let rows = request
        .rows
        .ok_or_else(|| AppError::msg("rows is required in sprite mode"))?;
    let cols = request
        .cols
        .ok_or_else(|| AppError::msg("cols is required in sprite mode"))?;
    let object_description = request
        .object_description
        .as_deref()
        .filter(|v| !v.trim().is_empty())
        .ok_or_else(|| AppError::msg("objectDescription is required in sprite mode"))?;
    let style = request
        .style
        .as_deref()
        .filter(|v| !v.trim().is_empty())
        .ok_or_else(|| AppError::msg("style is required in sprite mode"))?;
    let camera_angle = request
        .camera_angle
        .as_deref()
        .filter(|v| !v.trim().is_empty())
        .ok_or_else(|| AppError::msg("cameraAngle is required in sprite mode"))?;

    let total_frames = rows * cols;
    let mut prompt = format!(
        "Sprite Sheet Spec\nFrames: {total_frames} frames total\nLayout: {cols} columns x {rows} rows\nOrder: left-to-right, top-to-bottom\nCamera: {camera_angle}; fixed camera and scale across frames\nSubject: {object_description}\nStyle: {style}\nAlignment rules: same baseline, consistent proportions, consistent lighting, even padding\nBackground: generate using a pure chromakey green background (#00FF00)\nConstraints: no text, no borders, no watermark. Generate one image file only."
    );

    if request.image_prior_data_url.is_some() {
        prompt.push_str("\nFollow the attached reference grid exactly.");
    }

    Ok(prompt)
}

pub fn build_normal_prompt(request: &GenerateRequest) -> AppResult<String> {
    let prompt = request
        .prompt_text
        .as_deref()
        .filter(|v| !v.trim().is_empty())
        .ok_or_else(|| AppError::msg("promptText is required when spriteMode=false"))?;

    Ok(prompt.to_string())
}

pub fn build_edit_prompt(edit_prompt: &str) -> AppResult<String> {
    let trimmed = edit_prompt.trim();
    if trimmed.is_empty() {
        return Err(AppError::msg("editPrompt is required"));
    }

    Ok(format!(
        "{trimmed}\n\nApply the requested changes while preserving the subject identity and overall style."
    ))
}

pub fn choose_aspect_ratio(cols: u32, rows: u32) -> &'static str {
    if rows == 0 || cols == 0 {
        return "1:1";
    }

    let target = cols as f64 / rows as f64;
    SUPPORTED_ASPECT_RATIOS
        .iter()
        .min_by(|(_, a), (_, b)| {
            let da = (target - *a).abs();
            let db = (target - *b).abs();
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(ratio, _)| *ratio)
        .unwrap_or("1:1")
}
