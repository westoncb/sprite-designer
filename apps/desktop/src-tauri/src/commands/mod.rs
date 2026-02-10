use std::path::Path;

use chrono::Utc;
use tauri::{AppHandle, State};
use uuid::Uuid;

use crate::{
    error::{AppError, AppResult},
    models::{
        Child, ChildInputs, ChildMode, ChildOutputs, ChildResult, ChildType, EditRequest,
        GenerateRequest, OpenRouterSnapshot, Project, ProjectSummary, Resolution,
    },
    openrouter::GenerateImageRequest,
    prompt, storage, AppState,
};

#[tauri::command]
pub fn list_projects(app: AppHandle) -> Result<Vec<ProjectSummary>, String> {
    wrap_cmd(|| {
        let projects = storage::list_project_records(&app)?
            .into_iter()
            .map(|record| record.to_summary())
            .collect();
        Ok(projects)
    })
}

#[tauri::command]
pub fn get_project(app: AppHandle, project_id: String) -> Result<Project, String> {
    wrap_cmd(|| storage::load_project(&app, &project_id))
}

#[tauri::command]
pub fn create_project(
    app: AppHandle,
    optional_name: Option<String>,
) -> Result<ProjectSummary, String> {
    wrap_cmd(|| {
        let record = storage::create_project_record(&app, optional_name)?;
        Ok(record.to_summary())
    })
}

#[tauri::command]
pub fn delete_project(app: AppHandle, project_id: String) -> Result<(), String> {
    wrap_cmd(|| storage::delete_project(&app, &project_id))
}

#[tauri::command]
pub async fn generate_image(
    app: AppHandle,
    state: State<'_, AppState>,
    req: GenerateRequest,
) -> Result<ChildResult, String> {
    wrap_cmd_async(async {
        validate_generate_request(&req)?;

        if let Some(data_url) = &req.image_prior_data_url {
            storage::validate_data_url(data_url)?;
        }

        let mut project_record = if let Some(project_id) = req.project_id.as_deref() {
            storage::load_project_record(&app, project_id)?
        } else {
            storage::create_project_record(&app, Some(default_project_name(&req)))?
        };

        if let Some(name) = req.name.as_ref().and_then(|value| non_empty(value)) {
            project_record =
                storage::update_project_name(&app, &project_record.id, Some(name.to_string()))?;
        }

        let child_name = storage::next_child_name(&app, &project_record.id, ChildType::Generate)?;

        let (mode, prompt_text, aspect_ratio) = if req.sprite_mode {
            let rows = req
                .rows
                .ok_or_else(|| AppError::msg("rows is required in sprite mode"))?;
            let cols = req
                .cols
                .ok_or_else(|| AppError::msg("cols is required in sprite mode"))?;
            (
                ChildMode::Sprite,
                prompt::build_sprite_prompt(&req)?,
                Some(prompt::choose_aspect_ratio(cols, rows).to_string()),
            )
        } else {
            (ChildMode::Normal, prompt::build_normal_prompt(&req)?, None)
        };

        let openrouter_response = state
            .openrouter
            .generate_image(GenerateImageRequest {
                prompt: prompt_text,
                image_data_url: req.image_prior_data_url.clone(),
                aspect_ratio,
                resolution: req.resolution,
            })
            .await?;

        let child_id = Uuid::new_v4().to_string();
        let mut image_paths = Vec::new();
        for (index, data_url) in openrouter_response.image_data_urls.iter().enumerate() {
            let image_path =
                storage::write_output_image(&app, &project_record.id, &child_id, index, data_url)?;
            image_paths.push(image_path);
        }

        let child = Child {
            id: child_id,
            project_id: project_record.id.clone(),
            r#type: ChildType::Generate,
            name: child_name,
            created_at: Utc::now(),
            mode,
            inputs: ChildInputs {
                rows: req.rows,
                cols: req.cols,
                object_description: req.object_description.clone(),
                style: req.style.clone(),
                camera_angle: req.camera_angle.clone(),
                prompt_text: req.prompt_text.clone(),
                edit_prompt: None,
                base_child_id: None,
                resolution: Some(req.resolution),
                image_prior_data_url: req.image_prior_data_url.clone(),
                base_image_path: None,
            },
            openrouter: OpenRouterSnapshot {
                model: openrouter_response.model,
                payload: openrouter_response.sanitized_payload,
            },
            outputs: ChildOutputs {
                text: openrouter_response.text,
                image_paths: image_paths.clone(),
                primary_image_path: image_paths.first().cloned(),
            },
        };

        storage::append_child(&app, &project_record.id, &child)?;
        project_record = storage::load_project_record(&app, &project_record.id)?;

        Ok(ChildResult {
            project: project_record.to_summary(),
            child,
        })
    })
    .await
}

#[tauri::command]
pub async fn edit_image(
    app: AppHandle,
    state: State<'_, AppState>,
    req: EditRequest,
) -> Result<ChildResult, String> {
    wrap_cmd_async(async {
        let edit_prompt = prompt::build_edit_prompt(&req.edit_prompt)?;

        let mut project_record = storage::load_project_record(&app, &req.project_id)?;
        if let Some(name) = req.name.as_ref().and_then(|value| non_empty(value)) {
            project_record =
                storage::update_project_name(&app, &project_record.id, Some(name.to_string()))?;
        }

        let base_child = storage::load_child(&app, &req.project_id, &req.base_child_id)?;
        let base_image_path = req
            .base_image_path
            .clone()
            .or_else(|| base_child.outputs.primary_image_path.clone())
            .ok_or_else(|| AppError::msg("No base image path found for edit request"))?;

        let base_image_data_url = if let Some(data_url) = req.base_image_data_url.as_ref() {
            storage::validate_data_url(data_url)?;
            data_url.clone()
        } else {
            storage::read_image_path_as_data_url(Path::new(&base_image_path))?
        };

        let openrouter_response = state
            .openrouter
            .generate_image(GenerateImageRequest {
                prompt: edit_prompt,
                image_data_url: Some(base_image_data_url),
                aspect_ratio: None,
                resolution: req.resolution.unwrap_or(Resolution::OneK),
            })
            .await?;

        let child_id = Uuid::new_v4().to_string();
        let child_name = storage::next_child_name(&app, &project_record.id, ChildType::Edit)?;

        let mut image_paths = Vec::new();
        for (index, data_url) in openrouter_response.image_data_urls.iter().enumerate() {
            let image_path =
                storage::write_output_image(&app, &project_record.id, &child_id, index, data_url)?;
            image_paths.push(image_path);
        }

        let child = Child {
            id: child_id,
            project_id: project_record.id.clone(),
            r#type: ChildType::Edit,
            name: child_name,
            created_at: Utc::now(),
            mode: ChildMode::Edit,
            inputs: ChildInputs {
                rows: None,
                cols: None,
                object_description: None,
                style: None,
                camera_angle: None,
                prompt_text: None,
                edit_prompt: Some(req.edit_prompt.clone()),
                base_child_id: Some(req.base_child_id.clone()),
                resolution: req.resolution,
                image_prior_data_url: None,
                base_image_path: Some(base_image_path),
            },
            openrouter: OpenRouterSnapshot {
                model: openrouter_response.model,
                payload: openrouter_response.sanitized_payload,
            },
            outputs: ChildOutputs {
                text: openrouter_response.text,
                image_paths: image_paths.clone(),
                primary_image_path: image_paths.first().cloned(),
            },
        };

        storage::append_child(&app, &project_record.id, &child)?;
        project_record = storage::load_project_record(&app, &project_record.id)?;

        Ok(ChildResult {
            project: project_record.to_summary(),
            child,
        })
    })
    .await
}

fn validate_generate_request(req: &GenerateRequest) -> AppResult<()> {
    if req.sprite_mode {
        let rows = req
            .rows
            .ok_or_else(|| AppError::msg("rows is required in sprite mode"))?;
        let cols = req
            .cols
            .ok_or_else(|| AppError::msg("cols is required in sprite mode"))?;
        if rows == 0 || cols == 0 {
            return Err(AppError::msg("rows and cols must be > 0"));
        }

        if non_empty_opt(req.object_description.as_deref()).is_none() {
            return Err(AppError::msg(
                "objectDescription is required in sprite mode",
            ));
        }
        if non_empty_opt(req.style.as_deref()).is_none() {
            return Err(AppError::msg("style is required in sprite mode"));
        }
        if non_empty_opt(req.camera_angle.as_deref()).is_none() {
            return Err(AppError::msg("cameraAngle is required in sprite mode"));
        }
    } else if non_empty_opt(req.prompt_text.as_deref()).is_none() {
        return Err(AppError::msg(
            "promptText is required when spriteMode=false",
        ));
    }

    Ok(())
}

fn default_project_name(req: &GenerateRequest) -> String {
    let date = Utc::now().format("%m-%d-%Y");
    if req.sprite_mode {
        let rows = req.rows.unwrap_or(1);
        let cols = req.cols.unwrap_or(1);
        format!("sprite-{rows}x{cols}-{date}")
    } else {
        format!("art-{date}")
    }
}

fn non_empty_opt(value: Option<&str>) -> Option<&str> {
    value.filter(|v| !v.trim().is_empty())
}

fn non_empty(value: &str) -> Option<&str> {
    if value.trim().is_empty() {
        None
    } else {
        Some(value.trim())
    }
}

fn wrap_cmd<T, F>(f: F) -> Result<T, String>
where
    F: FnOnce() -> AppResult<T>,
{
    f().map_err(|error| error.to_string())
}

async fn wrap_cmd_async<T, F>(f: F) -> Result<T, String>
where
    F: std::future::Future<Output = AppResult<T>>,
{
    f.await.map_err(|error| error.to_string())
}
