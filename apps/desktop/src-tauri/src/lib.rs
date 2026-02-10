mod commands;
mod error;
mod models;
mod openrouter;
mod prompt;
mod storage;

use openrouter::{OpenRouterClient, OpenRouterConfig};

#[derive(Clone)]
pub struct AppState {
    pub openrouter: OpenRouterClient,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    load_env_files();

    let app_state = AppState {
        openrouter: OpenRouterClient::new(OpenRouterConfig::from_env()),
    };

    tauri::Builder::default()
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            commands::list_projects,
            commands::get_project,
            commands::create_project,
            commands::delete_project,
            commands::generate_image,
            commands::edit_image,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn load_env_files() {
    let _ = dotenvy::from_filename(".env");
    let _ = dotenvy::from_filename("../.env");
    let _ = dotenvy::from_filename("../../.env");
}
