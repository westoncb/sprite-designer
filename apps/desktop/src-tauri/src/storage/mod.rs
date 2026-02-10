use std::{
    fs,
    path::{Path, PathBuf},
};

use base64::{engine::general_purpose::STANDARD, Engine};
use chrono::Utc;
use image::ImageFormat;
use serde::{de::DeserializeOwned, Serialize};
use tauri::{AppHandle, Manager};
use uuid::Uuid;

use crate::{
    error::{AppError, AppResult},
    models::{Child, ChildType, Project, ProjectRecord},
};

const SUPPORTED_MIMES: [&str; 4] = ["image/png", "image/jpeg", "image/jpg", "image/webp"];

pub struct ParsedDataUrl {
    pub bytes: Vec<u8>,
}

pub fn ensure_projects_root(app: &AppHandle) -> AppResult<PathBuf> {
    let root = app
        .path()
        .app_data_dir()
        .map_err(|error| AppError::msg(format!("failed to resolve app data dir: {error}")))?
        .join("projects");

    fs::create_dir_all(&root)?;
    Ok(root)
}

pub fn create_project_record(app: &AppHandle, name: Option<String>) -> AppResult<ProjectRecord> {
    let now = Utc::now();
    let id = Uuid::new_v4().to_string();
    let record = ProjectRecord {
        id: id.clone(),
        name: normalize_project_name(name),
        created_at: now,
        updated_at: now,
        child_ids: Vec::new(),
    };

    ensure_project_dirs(app, &id)?;
    save_project_record(app, &record)?;

    Ok(record)
}

pub fn list_project_records(app: &AppHandle) -> AppResult<Vec<ProjectRecord>> {
    let root = ensure_projects_root(app)?;
    let mut records = Vec::new();

    for entry in fs::read_dir(&root)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }

        let project_file = entry.path().join("project.json");
        if !project_file.exists() {
            continue;
        }

        let record: ProjectRecord = read_json(&project_file)?;
        records.push(record);
    }

    records.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(records)
}

pub fn load_project_record(app: &AppHandle, project_id: &str) -> AppResult<ProjectRecord> {
    let path = project_file_path(app, project_id)?;
    if !path.exists() {
        return Err(AppError::msg(format!("project not found: {project_id}")));
    }

    read_json(&path)
}

pub fn save_project_record(app: &AppHandle, record: &ProjectRecord) -> AppResult<()> {
    ensure_project_dirs(app, &record.id)?;
    let path = project_file_path(app, &record.id)?;
    write_json(&path, record)
}

pub fn update_project_name(
    app: &AppHandle,
    project_id: &str,
    name: Option<String>,
) -> AppResult<ProjectRecord> {
    let mut record = load_project_record(app, project_id)?;
    if let Some(name) = name {
        let trimmed = name.trim();
        if !trimmed.is_empty() {
            record.name = trimmed.to_string();
            record.updated_at = Utc::now();
            save_project_record(app, &record)?;
        }
    }

    Ok(record)
}

pub fn delete_project(app: &AppHandle, project_id: &str) -> AppResult<()> {
    let project_dir = project_dir(app, project_id)?;
    if project_dir.exists() {
        fs::remove_dir_all(project_dir)?;
    }

    Ok(())
}

pub fn load_project(app: &AppHandle, project_id: &str) -> AppResult<Project> {
    let record = load_project_record(app, project_id)?;
    let children = record
        .child_ids
        .iter()
        .filter_map(|child_id| load_child(app, project_id, child_id).ok())
        .collect::<Vec<_>>();

    Ok(Project {
        id: record.id,
        name: record.name,
        created_at: record.created_at,
        updated_at: record.updated_at,
        children,
    })
}

pub fn append_child(app: &AppHandle, project_id: &str, child: &Child) -> AppResult<()> {
    save_child(app, child)?;

    let mut record = load_project_record(app, project_id)?;
    record.child_ids.push(child.id.clone());
    record.updated_at = Utc::now();
    save_project_record(app, &record)
}

pub fn save_child(app: &AppHandle, child: &Child) -> AppResult<()> {
    let child_path = child_file_path(app, &child.project_id, &child.id)?;
    write_json(&child_path, child)
}

pub fn load_child(app: &AppHandle, project_id: &str, child_id: &str) -> AppResult<Child> {
    let child_path = child_file_path(app, project_id, child_id)?;
    if !child_path.exists() {
        return Err(AppError::msg(format!(
            "child {child_id} not found in project {project_id}"
        )));
    }
    read_json(&child_path)
}

pub fn next_child_name(
    app: &AppHandle,
    project_id: &str,
    child_type: ChildType,
) -> AppResult<String> {
    let project = load_project(app, project_id)?;
    let count = project
        .children
        .iter()
        .filter(|child| child.r#type == child_type)
        .count()
        + 1;

    let prefix = match child_type {
        ChildType::Generate => "gen",
        ChildType::Edit => "edit",
    };

    Ok(format!("{prefix}-{count:04}"))
}

pub fn write_output_image(
    app: &AppHandle,
    project_id: &str,
    child_id: &str,
    index: usize,
    data_url: &str,
) -> AppResult<String> {
    let image_bytes = parse_data_url(data_url)?;
    let image = image::load_from_memory(&image_bytes.bytes)?;
    let image_path = images_dir(app, project_id)?.join(format!("{child_id}_{index}.png"));

    image.save_with_format(&image_path, ImageFormat::Png)?;

    Ok(image_path.to_string_lossy().to_string())
}

pub fn validate_data_url(data_url: &str) -> AppResult<()> {
    parse_data_url(data_url).map(|_| ())
}

pub fn parse_data_url(data_url: &str) -> AppResult<ParsedDataUrl> {
    if !data_url.starts_with("data:") {
        return Err(AppError::msg("expected a data URL with image payload"));
    }

    let (metadata, payload) = data_url
        .split_once(',')
        .ok_or_else(|| AppError::msg("invalid data URL format"))?;

    if !metadata.contains(";base64") {
        return Err(AppError::msg("data URL must be base64 encoded"));
    }

    let mime = metadata
        .trim_start_matches("data:")
        .split(';')
        .next()
        .unwrap_or_default();
    if !SUPPORTED_MIMES.contains(&mime) {
        return Err(AppError::msg(format!(
            "unsupported image mime type: {mime}. allowed: png/jpeg/webp"
        )));
    }

    let bytes = STANDARD.decode(payload.trim())?;
    Ok(ParsedDataUrl { bytes })
}

pub fn read_image_path_as_data_url(path: &Path) -> AppResult<String> {
    if !path.exists() {
        return Err(AppError::msg(format!(
            "image path not found: {}",
            path.display()
        )));
    }

    let bytes = fs::read(path)?;
    let mime = match path.extension().and_then(|ext| ext.to_str()) {
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        _ => "image/png",
    };

    Ok(format!("data:{mime};base64,{}", STANDARD.encode(bytes)))
}

fn ensure_project_dirs(app: &AppHandle, project_id: &str) -> AppResult<()> {
    fs::create_dir_all(children_dir(app, project_id)?)?;
    fs::create_dir_all(images_dir(app, project_id)?)?;
    Ok(())
}

fn project_dir(app: &AppHandle, project_id: &str) -> AppResult<PathBuf> {
    Ok(ensure_projects_root(app)?.join(project_id))
}

fn project_file_path(app: &AppHandle, project_id: &str) -> AppResult<PathBuf> {
    Ok(project_dir(app, project_id)?.join("project.json"))
}

fn children_dir(app: &AppHandle, project_id: &str) -> AppResult<PathBuf> {
    Ok(project_dir(app, project_id)?.join("children"))
}

fn images_dir(app: &AppHandle, project_id: &str) -> AppResult<PathBuf> {
    Ok(project_dir(app, project_id)?.join("images"))
}

fn child_file_path(app: &AppHandle, project_id: &str, child_id: &str) -> AppResult<PathBuf> {
    Ok(children_dir(app, project_id)?.join(format!("{child_id}.json")))
}

fn normalize_project_name(name: Option<String>) -> String {
    match name {
        Some(name) if !name.trim().is_empty() => name.trim().to_string(),
        _ => format!("sprite-project-{}", Utc::now().format("%m-%d-%Y")),
    }
}

fn read_json<T: DeserializeOwned>(path: &Path) -> AppResult<T> {
    let contents = fs::read_to_string(path)?;
    let value = serde_json::from_str::<T>(&contents)?;
    Ok(value)
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> AppResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let contents = serde_json::to_string_pretty(value)?;
    fs::write(path, contents)?;
    Ok(())
}
