mod agent;
mod mcp_config;
mod storage;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use agent::{AgentSession, UserAttachment};
use storage::{ConversationSummary, LoadedConversation, Storage, Workspace};
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

/// Send a user message to claude.
///
/// `conversation_id` (Story 09's reopen flow): when set, the message
/// continues an existing persisted conversation — events append to its
/// jsonl and `claude` is spawned with `--resume <stored_session_id>`.
///
/// `force_new_session` (Story 11's resume-failure recovery): when true,
/// the conversation's stored session id is cleared first so the next
/// spawn starts a fresh claude session. Events still append to the
/// same conversation file; the new session id replaces the old one in
/// the index on first init.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
async fn send_message(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    text: String,
    attachments: Option<Vec<UserAttachment>>,
    working_dir: Option<String>,
    permission_mode: Option<String>,
    conversation_id: Option<String>,
    force_new_session: Option<bool>,
) -> Result<String, String> {
    // Working-dir resolution order:
    //   1. Explicit `working_dir` argument (legacy callers; tests).
    //   2. The active workspace's path (the workspaces flow).
    //   3. $HOME (fallback for users with no workspaces yet).
    let active_workspace = if working_dir.is_none() {
        let storage = storage_for(&app, &state).await?;
        tokio::task::spawn_blocking(move || storage.get_active_workspace())
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?
    } else {
        None
    };
    let cwd_from_active = active_workspace.as_ref().map(|w| w.path.clone());

    let mut guard = state.session.lock().await;
    if guard.is_none() {
        let server_bin = mcp_config::locate_server_binary().map_err(|e| e.to_string())?;
        let mcp_cfg = mcp_config::write_default_config(&server_bin).map_err(|e| e.to_string())?;
        let cwd = working_dir
            .map(PathBuf::from)
            .or(cwd_from_active)
            .or_else(home_dir)
            .unwrap_or_else(|| PathBuf::from("."));
        let mode = permission_mode.as_deref().unwrap_or("bypassPermissions");

        let mut session = AgentSession::new(app.clone(), cwd, mcp_cfg, mode);

        // If there's an active workspace, attach a conversation log so
        // every streamed event gets persisted to disk for later replay.
        if let Some(ws) = active_workspace.as_ref() {
            let storage = storage_for(&app, &state).await?;
            // Pick the conversation id: continue an existing one if the
            // caller (reopen flow) supplied it, else use AgentSession's
            // local_id to start a fresh one.
            let convo_id = conversation_id
                .clone()
                .unwrap_or_else(|| session.local_id.clone());

            // If this conversation already has a persisted Claude session
            // id, seed the AgentSession so the next spawn uses
            // `--resume <sid>`. Story 11's recovery path can force a
            // fresh session by setting `force_new_session=true`.
            let resume_sid = if conversation_id.is_some() && !force_new_session.unwrap_or(false) {
                let storage_for_lookup = storage.clone();
                let ws_id = ws.id.clone();
                let convo_for_lookup = convo_id.clone();
                tokio::task::spawn_blocking(move || {
                    storage_for_lookup
                        .list_conversations(&ws_id)
                        .map(|list| list.into_iter().find(|c| c.id == convo_for_lookup))
                })
                .await
                .map_err(|e| e.to_string())?
                .map_err(|e| e.to_string())?
                .and_then(|c| c.claude_session_id)
            } else {
                None
            };
            if let Some(sid) = resume_sid {
                session = session.with_resume_session_id(sid);
            }

            let workspace_id = ws.id.clone();
            let log = tokio::task::spawn_blocking(move || {
                storage.open_conversation(&workspace_id, &convo_id)
            })
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?;
            session = session.with_conversation_log(log);
        }

        *guard = Some(session);
    }
    let session = guard.as_ref().unwrap();
    let atts = attachments.unwrap_or_default();
    session
        .send_user_message(&text, &atts)
        .await
        .map_err(|e| e.to_string())?;
    Ok(session.local_id.clone())
}

/// Shared helper: cancel any in-flight turn AND drop the AgentSession so the
/// next send rebuilds it (picking up a fresh cwd from the new active
/// workspace). Used by set_active_workspace and delete_workspace.
async fn end_current_session(state: &Arc<AppState>) {
    let mut guard = state.session.lock().await;
    if let Some(s) = guard.take() {
        let _ = s.end().await;
    }
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
) -> Result<Option<String>, String> {
    let storage = storage_for(&app, &state).await?;
    // If the workspace being deleted is active, end the current session up
    // front so its child process isn't left holding a working-dir we're
    // about to remove.
    let active = tokio::task::spawn_blocking({
        let storage = storage.clone();
        move || storage.get_active_workspace()
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;
    if active.as_ref().map(|w| w.id.as_str()) == Some(id.as_str()) {
        end_current_session(&state).await;
    }
    tokio::task::spawn_blocking(move || storage.delete_workspace(&id))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn set_active_workspace(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    id: String,
) -> Result<Workspace, String> {
    // Switching workspaces ends the in-flight turn (and the whole session)
    // so the next send_message spawns claude with the new working directory.
    end_current_session(&state).await;
    let storage = storage_for(&app, &state).await?;
    tokio::task::spawn_blocking(move || storage.set_active_workspace(&id))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_active_workspace(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<Option<Workspace>, String> {
    let storage = storage_for(&app, &state).await?;
    tokio::task::spawn_blocking(move || storage.get_active_workspace())
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn list_conversations(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    workspace_id: String,
) -> Result<Vec<ConversationSummary>, String> {
    let storage = storage_for(&app, &state).await?;
    tokio::task::spawn_blocking(move || storage.list_conversations(&workspace_id))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn load_conversation(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    workspace_id: String,
    conversation_id: String,
) -> Result<LoadedConversation, String> {
    let storage = storage_for(&app, &state).await?;
    tokio::task::spawn_blocking(move || storage.load_conversation(&workspace_id, &conversation_id))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn delete_conversation(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    workspace_id: String,
    conversation_id: String,
) -> Result<(), String> {
    let storage = storage_for(&app, &state).await?;
    tokio::task::spawn_blocking(move || {
        storage.delete_conversation(&workspace_id, &conversation_id)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())
}

#[tauri::command]
async fn rename_conversation(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    workspace_id: String,
    conversation_id: String,
    new_title: String,
) -> Result<ConversationSummary, String> {
    let storage = storage_for(&app, &state).await?;
    tokio::task::spawn_blocking(move || {
        storage.pin_conversation_title(&workspace_id, &conversation_id, &new_title)
    })
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
            set_active_workspace,
            get_active_workspace,
            list_conversations,
            load_conversation,
            delete_conversation,
            rename_conversation,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
