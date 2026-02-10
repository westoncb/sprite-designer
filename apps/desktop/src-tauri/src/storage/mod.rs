use std::{
    collections::VecDeque,
    fs,
    path::{Path, PathBuf},
};

use base64::{engine::general_purpose::STANDARD, Engine};
use chrono::Utc;
use image::{
    codecs::png::{CompressionType, FilterType, PngEncoder},
    ColorType, ImageEncoder, RgbaImage,
};
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
    apply_chromakey: bool,
    sprite_grid: Option<(u32, u32)>,
) -> AppResult<String> {
    let image_bytes = parse_data_url(data_url)?;
    let mut image = image::load_from_memory(&image_bytes.bytes)?.into_rgba8();
    if apply_chromakey {
        apply_chromakey_transparency(&mut image, sprite_grid);
    }
    let image_path = images_dir(app, project_id)?.join(format!("{child_id}_{index}.png"));

    let png_bytes = encode_png_optimized(image.as_raw(), image.width(), image.height())?;
    fs::write(&image_path, png_bytes)?;

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

pub fn export_image_to_path(source_image_path: &Path, destination_path: &Path) -> AppResult<String> {
    if !source_image_path.exists() {
        return Err(AppError::msg(format!(
            "source image path not found: {}",
            source_image_path.display()
        )));
    }

    let mut output_path = destination_path.to_path_buf();
    if output_path.extension().is_none() {
        output_path.set_extension("png");
    }

    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)?;
    }

    fs::copy(source_image_path, &output_path)?;
    Ok(output_path.to_string_lossy().to_string())
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

fn apply_chromakey_transparency(image: &mut RgbaImage, sprite_grid: Option<(u32, u32)>) {
    let (width, height) = image.dimensions();
    if width == 0 || height == 0 {
        return;
    }

    let mut visited = vec![false; (width * height) as usize];
    let mut queue = VecDeque::new();

    let seeded = sprite_grid
        .filter(|(rows, cols)| *rows > 0 && *cols > 0)
        .map(|(rows, cols)| enqueue_chromakey_cell_borders(rows, cols, image, &mut visited, &mut queue))
        .unwrap_or(false);

    if !seeded {
        enqueue_chromakey_borders(image, &mut visited, &mut queue);
    }

    while let Some((x, y)) = queue.pop_front() {
        image.put_pixel(x, y, image::Rgba([0, 0, 0, 0]));

        let neighbors = [
            (x.wrapping_sub(1), y, x > 0),
            (x + 1, y, x + 1 < width),
            (x, y.wrapping_sub(1), y > 0),
            (x, y + 1, y + 1 < height),
        ];

        for (nx, ny, in_bounds) in neighbors {
            if in_bounds {
                enqueue_if_chromakey(
                    nx,
                    ny,
                    image,
                    &mut visited,
                    &mut queue,
                    ChromaMatchMode::Expand,
                );
            }
        }
    }

    clear_strong_chromakey_anywhere(image);
    clear_chromakey_fringe(image, 2);
}

fn enqueue_chromakey_borders(
    image: &RgbaImage,
    visited: &mut [bool],
    queue: &mut VecDeque<(u32, u32)>,
) {
    let (width, height) = image.dimensions();

    for x in 0..width {
        let _ = enqueue_if_chromakey(x, 0, image, visited, queue, ChromaMatchMode::Seed);
        if height > 1 {
            let _ = enqueue_if_chromakey(
                x,
                height - 1,
                image,
                visited,
                queue,
                ChromaMatchMode::Seed,
            );
        }
    }

    for y in 0..height {
        let _ = enqueue_if_chromakey(0, y, image, visited, queue, ChromaMatchMode::Seed);
        if width > 1 {
            let _ = enqueue_if_chromakey(
                width - 1,
                y,
                image,
                visited,
                queue,
                ChromaMatchMode::Seed,
            );
        }
    }
}

fn enqueue_chromakey_cell_borders(
    rows: u32,
    cols: u32,
    image: &RgbaImage,
    visited: &mut [bool],
    queue: &mut VecDeque<(u32, u32)>,
) -> bool {
    let (width, height) = image.dimensions();
    let mut seeded = false;

    for row in 0..rows {
        let y_start = (row * height) / rows;
        let y_end = (((row + 1) * height) / rows).saturating_sub(1);
        if y_start > y_end {
            continue;
        }
        let (top, bottom) = inner_span(y_start, y_end);

        for col in 0..cols {
            let x_start = (col * width) / cols;
            let x_end = (((col + 1) * width) / cols).saturating_sub(1);
            if x_start > x_end {
                continue;
            }
            let (left, right) = inner_span(x_start, x_end);

            for x in left..=right {
                seeded |= enqueue_if_chromakey(
                    x,
                    top,
                    image,
                    visited,
                    queue,
                    ChromaMatchMode::Seed,
                );
                seeded |= enqueue_if_chromakey(
                    x,
                    bottom,
                    image,
                    visited,
                    queue,
                    ChromaMatchMode::Seed,
                );
            }
            for y in top..=bottom {
                seeded |= enqueue_if_chromakey(
                    left,
                    y,
                    image,
                    visited,
                    queue,
                    ChromaMatchMode::Seed,
                );
                seeded |= enqueue_if_chromakey(
                    right,
                    y,
                    image,
                    visited,
                    queue,
                    ChromaMatchMode::Seed,
                );
            }
        }
    }

    seeded
}

fn inner_span(start: u32, end: u32) -> (u32, u32) {
    if end > start + 1 {
        (start + 1, end - 1)
    } else {
        (start, end)
    }
}

fn enqueue_if_chromakey(
    x: u32,
    y: u32,
    image: &RgbaImage,
    visited: &mut [bool],
    queue: &mut VecDeque<(u32, u32)>,
    mode: ChromaMatchMode,
) -> bool {
    let width = image.width();
    let index = (y * width + x) as usize;
    if visited[index] {
        return false;
    }

    let pixel = image.get_pixel(x, y).0;
    if matches_chromakey(pixel[0], pixel[1], pixel[2], mode) {
        visited[index] = true;
        queue.push_back((x, y));
        return true;
    }

    false
}

#[derive(Copy, Clone)]
enum ChromaMatchMode {
    Seed,
    Expand,
}

fn matches_chromakey(r: u8, g: u8, b: u8, mode: ChromaMatchMode) -> bool {
    let max_rb = r.max(b);
    let green_lead = g.saturating_sub(max_rb);
    let dist_sq = chroma_green_distance_sq(r, g, b);

    match mode {
        ChromaMatchMode::Seed => {
            if g < 80 || green_lead < 18 {
                return false;
            }
            dist_sq <= 30_000
        }
        ChromaMatchMode::Expand => {
            if g < 40 || green_lead < 6 {
                return false;
            }
            dist_sq <= 45_000
        }
    }
}

fn chroma_green_distance_sq(r: u8, g: u8, b: u8) -> u32 {
    let dr = r as i32;
    let dg = 255_i32 - g as i32;
    let db = b as i32;

    (dr * dr + dg * dg + db * db) as u32
}

fn clear_strong_chromakey_anywhere(image: &mut RgbaImage) {
    for pixel in image.pixels_mut() {
        if pixel[3] == 0 {
            continue;
        }

        if matches_chromakey_global_strong(pixel[0], pixel[1], pixel[2]) {
            *pixel = image::Rgba([0, 0, 0, 0]);
        }
    }
}

fn matches_chromakey_global_strong(r: u8, g: u8, b: u8) -> bool {
    let max_rb = r.max(b);
    let green_lead = g.saturating_sub(max_rb);
    if g < 95 || green_lead < 20 {
        return false;
    }

    chroma_green_distance_sq(r, g, b) <= 36_000
}

fn clear_chromakey_fringe(image: &mut RgbaImage, passes: usize) {
    let (width, height) = image.dimensions();
    for _ in 0..passes {
        let mut to_clear = Vec::new();

        for y in 0..height {
            for x in 0..width {
                let pixel = image.get_pixel(x, y).0;
                if pixel[3] == 0 {
                    continue;
                }

                if !matches_chromakey_fringe(pixel[0], pixel[1], pixel[2]) {
                    continue;
                }

                if has_transparent_neighbor(image, x, y, width, height) {
                    to_clear.push((x, y));
                }
            }
        }

        if to_clear.is_empty() {
            break;
        }

        for (x, y) in to_clear {
            image.put_pixel(x, y, image::Rgba([0, 0, 0, 0]));
        }
    }
}

fn matches_chromakey_fringe(r: u8, g: u8, b: u8) -> bool {
    let max_rb = r.max(b);
    let green_lead = g.saturating_sub(max_rb);
    if g < 35 || green_lead < 2 {
        return false;
    }

    chroma_green_distance_sq(r, g, b) <= 55_000
}

fn has_transparent_neighbor(image: &RgbaImage, x: u32, y: u32, width: u32, height: u32) -> bool {
    let x_min = x.saturating_sub(1);
    let y_min = y.saturating_sub(1);
    let x_max = (x + 1).min(width.saturating_sub(1));
    let y_max = (y + 1).min(height.saturating_sub(1));

    for ny in y_min..=y_max {
        for nx in x_min..=x_max {
            if nx == x && ny == y {
                continue;
            }

            if image.get_pixel(nx, ny).0[3] == 0 {
                return true;
            }
        }
    }

    false
}

fn encode_png_optimized(rgba: &[u8], width: u32, height: u32) -> AppResult<Vec<u8>> {
    let mut png_bytes = Vec::new();
    {
        let encoder = PngEncoder::new_with_quality(
            &mut png_bytes,
            CompressionType::Best,
            FilterType::Adaptive,
        );
        encoder
            .write_image(rgba, width, height, ColorType::Rgba8)
            .map_err(|error| AppError::msg(format!("failed to encode png: {error}")))?;
    }

    let mut options = oxipng::Options::from_preset(3);
    options.strip = oxipng::StripChunks::Safe;

    oxipng::optimize_from_memory(&png_bytes, &options)
        .map_err(|error| AppError::msg(format!("failed to optimize png: {error}")))
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
