mod agent;
mod mcp_config;
mod storage;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use agent::{AgentSession, UserAttachment};
use storage::{Storage, Workspace};
use tauri::{AppHandle, Manager, State};
use tokio::sync::Mutex;

#[derive(Default)]
struct AppState {
    session: Mutex<Option<AgentSession>>,
    storage: Mutex<Option<Storage>>,
}

async fn storage_for(app: &AppHandle, state: &Arc<AppState>) -> Result<Storage, String> {
    let mut guard = state.storage.lock().await;
    if guard.is_none() {
        let dir = app
            .path()
            .app_data_dir()
            .map_err(|e| format!("app_data_dir: {e}"))?;
        *guard = Some(Storage::open(dir).map_err(|e| e.to_string())?);
    }
    Ok(guard.as_ref().unwrap().clone())
}

#[tauri::command]
async fn send_message(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    text: String,
    attachments: Option<Vec<UserAttachment>>,
    working_dir: Option<String>,
    permission_mode: Option<String>,
) -> Result<String, String> {
    let mut guard = state.session.lock().await;
    if guard.is_none() {
        let server_bin = mcp_config::locate_server_binary().map_err(|e| e.to_string())?;
        let mcp_cfg = mcp_config::write_default_config(&server_bin).map_err(|e| e.to_string())?;
        let cwd = working_dir
            .map(PathBuf::from)
            .or_else(home_dir)
            .unwrap_or_else(|| PathBuf::from("."));
        let mode = permission_mode.as_deref().unwrap_or("bypassPermissions");
        *guard = Some(AgentSession::new(app.clone(), cwd, mcp_cfg, mode));
    }
    let session = guard.as_ref().unwrap();
    let atts = attachments.unwrap_or_default();
    session
        .send_user_message(&text, &atts)
        .await
        .map_err(|e| e.to_string())?;
    Ok(session.local_id.clone())
}

#[tauri::command]
async fn end_session(state: State<'_, Arc<AppState>>) -> Result<(), String> {
    let mut guard = state.session.lock().await;
    if let Some(s) = guard.take() {
        s.end().await.map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
async fn cancel_turn(state: State<'_, Arc<AppState>>) -> Result<(), String> {
    let guard = state.session.lock().await;
    if let Some(s) = guard.as_ref() {
        s.cancel_turn().await.map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
async fn list_workspaces(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<Workspace>, String> {
    let storage = storage_for(&app, &state).await?;
    tokio::task::spawn_blocking(move || storage.list_workspaces())
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn create_workspace(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    name: String,
    path: String,
) -> Result<Workspace, String> {
    let storage = storage_for(&app, &state).await?;
    tokio::task::spawn_blocking(move || storage.create_workspace(&name, Path::new(&path)))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn rename_workspace(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    id: String,
    new_name: String,
) -> Result<Workspace, String> {
    let storage = storage_for(&app, &state).await?;
    tokio::task::spawn_blocking(move || storage.rename_workspace(&id, &new_name))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn delete_workspace(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    id: String,
) -> Result<(), String> {
    let storage = storage_for(&app, &state).await?;
    tokio::task::spawn_blocking(move || storage.delete_workspace(&id))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn read_file(path: String) -> Result<String, String> {
    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|e| format!("{path}: {e}"))?;
    match String::from_utf8(bytes.clone()) {
        Ok(s) => Ok(s),
        Err(_) => Err(format!("not a UTF-8 text file ({} bytes)", bytes.len())),
    }
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,linux_coworker_ui=debug".into()),
        )
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(Arc::new(AppState::default()))
        .invoke_handler(tauri::generate_handler![
            send_message,
            end_session,
            cancel_turn,
            read_file,
            list_workspaces,
            create_workspace,
            rename_workspace,
            delete_workspace,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
