// On-disk schema and CRUD for Workspaces.
//
// Storage layout under `<root>`:
//   workspaces.json                                  — array of Workspace
//   active.json                                      — { active_id: Option<String> }
//   workspaces/<workspace_id>/conversations/
//                <conversation_id>.jsonl            — append-only event log
//
// Both top-level JSON files are rewritten atomically (temp file + rename)
// on every mutation so a crash leaves either the old file intact or the
// new file fully written — never a half-written one. Conversation event
// logs are append-only: every line is a complete JSON object terminated
// by `\n`, so a crash mid-turn leaves a valid prefix that replays cleanly.
//
// All operations are synchronous. The Tauri commands wrap them in
// `tokio::task::spawn_blocking` when called from async contexts.

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Workspace {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    /// Unix timestamp in milliseconds. Updated whenever the workspace is set
    /// active (covered by Story 02); on create, this matches `created_at`.
    pub last_used_at: u64,
}

/// One entry in a workspace's `conversations/index.json`. Lets the rail
/// list conversations without having to open each jsonl.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConversationSummary {
    pub id: String,
    pub title: String,
    /// Unix timestamp in milliseconds, set on first event.
    pub started_at: u64,
    /// Unix timestamp in milliseconds, bumped on every event append.
    pub last_active_at: u64,
    /// Claude Code's session id (captured from the `system/init` event).
    /// Used by Story 10 to drive `--resume`.
    #[serde(default)]
    pub claude_session_id: Option<String>,
    /// When true, the title was set explicitly by the user (e.g. through
    /// a rename in Story 05) and should NOT be auto-derived from future
    /// user-text events.
    #[serde(default)]
    pub title_pinned: bool,
}

/// Filesystem-backed workspace store. Construct one per app via
/// [`Storage::open`]. Cheap to clone (it's just a path).
#[derive(Debug, Clone)]
pub struct Storage {
    root: PathBuf,
}

impl Storage {
    /// Open a store rooted at `root`. The directory and `workspaces.json`
    /// are created if they don't exist yet.
    pub fn open(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root.into();
        fs::create_dir_all(&root).with_context(|| format!("create {:?}", root))?;
        let store = Self { root };
        if !store.workspaces_path().exists() {
            store.write_workspaces(&[])?;
        }
        Ok(store)
    }

    fn workspaces_path(&self) -> PathBuf {
        self.root.join("workspaces.json")
    }

    fn active_path(&self) -> PathBuf {
        self.root.join("active.json")
    }

    fn workspace_dir(&self, id: &str) -> PathBuf {
        self.root.join("workspaces").join(id)
    }

    pub fn list_workspaces(&self) -> Result<Vec<Workspace>> {
        let bytes = fs::read(self.workspaces_path())
            .with_context(|| format!("read {:?}", self.workspaces_path()))?;
        let list: Vec<Workspace> =
            serde_json::from_slice(&bytes).context("parse workspaces.json")?;
        Ok(list)
    }

    pub fn create_workspace(&self, name: &str, path: &Path) -> Result<Workspace> {
        let name = name.trim();
        if name.is_empty() {
            bail!("workspace name cannot be empty");
        }
        if !path.is_absolute() {
            bail!("workspace path must be absolute: {:?}", path);
        }
        if !path.is_dir() {
            bail!(
                "workspace path does not exist or is not a directory: {:?}",
                path
            );
        }

        let mut list = self.list_workspaces()?;
        if list.iter().any(|w| w.name.eq_ignore_ascii_case(name)) {
            bail!("a workspace named {:?} already exists", name);
        }

        let workspace = Workspace {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.to_string(),
            path: path.to_path_buf(),
            last_used_at: now_millis(),
        };
        list.push(workspace.clone());
        self.write_workspaces(&list)?;
        fs::create_dir_all(self.workspace_dir(&workspace.id).join("conversations"))
            .with_context(|| format!("create conversations dir for {}", workspace.id))?;
        Ok(workspace)
    }

    pub fn rename_workspace(&self, id: &str, new_name: &str) -> Result<Workspace> {
        let new_name = new_name.trim();
        if new_name.is_empty() {
            bail!("workspace name cannot be empty");
        }
        let mut list = self.list_workspaces()?;
        if list
            .iter()
            .any(|w| w.id != id && w.name.eq_ignore_ascii_case(new_name))
        {
            bail!("a workspace named {:?} already exists", new_name);
        }
        let workspace = list
            .iter_mut()
            .find(|w| w.id == id)
            .ok_or_else(|| anyhow!("workspace {id} not found"))?;
        workspace.name = new_name.to_string();
        let updated = workspace.clone();
        self.write_workspaces(&list)?;
        Ok(updated)
    }

    /// Delete the workspace entry and its on-disk conversations folder.
    /// If the folder removal fails, the entry is restored so the workspace
    /// is not orphaned. Returns the new active workspace id (the
    /// most-recently-used remaining one) when the deleted workspace was
    /// active; `None` otherwise or when no workspaces remain.
    pub fn delete_workspace(&self, id: &str) -> Result<Option<String>> {
        let mut list = self.list_workspaces()?;
        let original = list.clone();
        let pos = list
            .iter()
            .position(|w| w.id == id)
            .ok_or_else(|| anyhow!("workspace {id} not found"))?;
        list.remove(pos);
        self.write_workspaces(&list)?;

        let dir = self.workspace_dir(id);
        if dir.exists() {
            if let Err(err) = fs::remove_dir_all(&dir) {
                // Rollback the index write so the user can retry.
                self.write_workspaces(&original)?;
                return Err(err).with_context(|| format!("remove {:?}", dir));
            }
        }

        // If the deleted workspace was active, fall back to the most-
        // recently-used remaining workspace (or clear active when the list
        // is now empty).
        let current_active = self.read_active_id()?;
        if current_active.as_deref() == Some(id) {
            let next = list
                .iter()
                .max_by_key(|w| w.last_used_at)
                .map(|w| w.id.clone());
            self.write_active_id(next.as_deref())?;
            return Ok(next);
        }
        Ok(None)
    }

    /// Set the active workspace by id. Bumps the workspace's `last_used_at`
    /// to "now" so that downstream "most recent" sorts surface it.
    pub fn set_active_workspace(&self, id: &str) -> Result<Workspace> {
        let mut list = self.list_workspaces()?;
        let workspace = list
            .iter_mut()
            .find(|w| w.id == id)
            .ok_or_else(|| anyhow!("workspace {id} not found"))?;
        workspace.last_used_at = now_millis();
        let updated = workspace.clone();
        self.write_workspaces(&list)?;
        self.write_active_id(Some(id))?;
        Ok(updated)
    }

    /// Get the active workspace, if any. Returns `None` when no workspace
    /// is active or when the persisted active id no longer matches any
    /// workspace (in which case the stale id is cleared).
    pub fn get_active_workspace(&self) -> Result<Option<Workspace>> {
        let Some(active_id) = self.read_active_id()? else {
            return Ok(None);
        };
        let list = self.list_workspaces()?;
        if let Some(ws) = list.into_iter().find(|w| w.id == active_id) {
            Ok(Some(ws))
        } else {
            // Stale active id (workspace was deleted out-of-band). Clear it.
            self.write_active_id(None)?;
            Ok(None)
        }
    }

    fn read_active_id(&self) -> Result<Option<String>> {
        let path = self.active_path();
        if !path.exists() {
            return Ok(None);
        }
        let bytes = fs::read(&path).with_context(|| format!("read {:?}", path))?;
        #[derive(Deserialize)]
        struct ActiveFile {
            #[serde(default)]
            active_id: Option<String>,
        }
        let parsed: ActiveFile = serde_json::from_slice(&bytes).context("parse active.json")?;
        Ok(parsed.active_id)
    }

    fn write_active_id(&self, id: Option<&str>) -> Result<()> {
        let value = serde_json::json!({ "active_id": id });
        write_json_atomic(&self.active_path(), &value)
    }

    fn write_workspaces(&self, list: &[Workspace]) -> Result<()> {
        write_json_atomic(&self.workspaces_path(), list)
    }

    /// Open (or create) the append-only event log for a conversation under
    /// the given workspace. The conversations folder is created on demand.
    /// The returned log knows how to update this workspace's
    /// `index.json` as events arrive (Story 04).
    pub fn open_conversation(
        &self,
        workspace_id: &str,
        conversation_id: &str,
    ) -> Result<ConversationLog> {
        let dir = self.workspace_dir(workspace_id).join("conversations");
        fs::create_dir_all(&dir).with_context(|| format!("create {:?}", dir))?;
        let path = dir.join(format!("{conversation_id}.jsonl"));
        Ok(ConversationLog::open(
            path,
            self.clone(),
            workspace_id.to_string(),
            conversation_id.to_string(),
        ))
    }

    // ── conversation index ───────────────────────────────────────────────

    fn index_path(&self, workspace_id: &str) -> PathBuf {
        self.workspace_dir(workspace_id)
            .join("conversations")
            .join("index.json")
    }

    /// Read the workspace's conversation index. Falls back to reconstructing
    /// from on-disk `.jsonl` files when the index is missing or unparseable;
    /// in that case the reconstructed entries carry the file's mtime as
    /// `last_active_at` and the filename stem as `id`, with placeholder
    /// titles that future events will overwrite (unless pinned).
    pub fn load_conversation_index(&self, workspace_id: &str) -> Result<Vec<ConversationSummary>> {
        let path = self.index_path(workspace_id);
        if path.exists() {
            match fs::read(&path)
                .with_context(|| format!("read {:?}", path))
                .and_then(|b| serde_json::from_slice(&b).context("parse index.json"))
            {
                Ok(list) => return Ok(list),
                Err(err) => {
                    tracing::warn!(?err, ?path, "conversation index unreadable, rebuilding");
                }
            }
        }
        let rebuilt = self.reconstruct_index(workspace_id)?;
        if !rebuilt.is_empty() {
            self.write_conversation_index(workspace_id, &rebuilt)?;
        }
        tracing::info!(
            workspace_id,
            count = rebuilt.len(),
            "reconstructed conversation index from disk"
        );
        Ok(rebuilt)
    }

    fn reconstruct_index(&self, workspace_id: &str) -> Result<Vec<ConversationSummary>> {
        let dir = self.workspace_dir(workspace_id).join("conversations");
        if !dir.exists() {
            return Ok(Vec::new());
        }
        let mut out = Vec::new();
        for entry in fs::read_dir(&dir).with_context(|| format!("read_dir {:?}", dir))? {
            let entry = entry?;
            let path = entry.path();
            let Some(stem) = path
                .file_stem()
                .and_then(|s| s.to_str())
                .map(|s| s.to_string())
            else {
                continue;
            };
            // Skip the index file itself + anything that isn't a .jsonl.
            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            let mtime_ms = entry
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as u64)
                .unwrap_or_else(now_millis);
            out.push(ConversationSummary {
                id: stem.clone(),
                title: format!("Conversation {stem}"),
                started_at: mtime_ms,
                last_active_at: mtime_ms,
                claude_session_id: None,
                title_pinned: false,
            });
        }
        Ok(out)
    }

    fn write_conversation_index(
        &self,
        workspace_id: &str,
        list: &[ConversationSummary],
    ) -> Result<()> {
        write_json_atomic(&self.index_path(workspace_id), list)
    }

    /// Bump (or create) an index entry for a conversation in response to a
    /// streamed event. Sets `started_at` on first event, always bumps
    /// `last_active_at`, derives a title from the first user-text event
    /// unless the title is pinned, and captures Claude's session id from
    /// `system/init` events.
    pub fn update_conversation_for_event(
        &self,
        workspace_id: &str,
        conversation_id: &str,
        event: &Value,
    ) -> Result<()> {
        let workspace_name = self
            .list_workspaces()?
            .into_iter()
            .find(|w| w.id == workspace_id)
            .map(|w| w.name)
            .unwrap_or_else(|| "workspace".to_string());

        let mut list = self.load_conversation_index(workspace_id)?;
        let now = now_millis();
        let pos = list.iter().position(|c| c.id == conversation_id);
        let entry = match pos {
            Some(i) => &mut list[i],
            None => {
                list.push(ConversationSummary {
                    id: conversation_id.to_string(),
                    title: format!("{workspace_name} \u{00b7} {now}"),
                    started_at: now,
                    last_active_at: now,
                    claude_session_id: None,
                    title_pinned: false,
                });
                list.last_mut().expect("just pushed")
            }
        };
        entry.last_active_at = now;
        if !entry.title_pinned {
            if let Some(derived) = derive_title_from_event(event) {
                entry.title = derived;
                // First user-text event sets the title and acts as the
                // "we have a real title now" marker — we don't pin here
                // because Story 05's rename does that explicitly.
            }
        }
        if let Some(sid) = extract_init_session_id(event) {
            entry.claude_session_id = Some(sid);
        }
        self.write_conversation_index(workspace_id, &list)?;
        Ok(())
    }

    /// Manually set a conversation's title (used by Story 05's
    /// `rename_conversation` command). Pins the title so future
    /// user-text events don't overwrite it.
    #[allow(dead_code)] // wired up in Story 05
    pub fn pin_conversation_title(
        &self,
        workspace_id: &str,
        conversation_id: &str,
        new_title: &str,
    ) -> Result<ConversationSummary> {
        let new_title = new_title.trim();
        if new_title.is_empty() {
            bail!("conversation title cannot be empty");
        }
        let mut list = self.load_conversation_index(workspace_id)?;
        let entry = list
            .iter_mut()
            .find(|c| c.id == conversation_id)
            .ok_or_else(|| anyhow!("conversation {conversation_id} not found"))?;
        entry.title = new_title.to_string();
        entry.title_pinned = true;
        let updated = entry.clone();
        self.write_conversation_index(workspace_id, &list)?;
        Ok(updated)
    }
}

/// Pull a user-displayable title out of an Anthropic stream-json event,
/// when the event is a user-text message. Returns None for any other
/// event type so the caller can leave the existing title alone.
fn derive_title_from_event(event: &Value) -> Option<String> {
    if event.get("type").and_then(Value::as_str) != Some("user") {
        return None;
    }
    let content = event.pointer("/message/content")?.as_array()?;
    let first_text = content
        .iter()
        .find_map(|c| c.get("text").and_then(Value::as_str))?;
    derive_title(first_text)
}

/// Pure title deriver. Returns `None` for empty / whitespace-only input
/// so the caller keeps whatever fallback title was assigned.
fn derive_title(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    const MAX_LEN: usize = 50;
    if trimmed.chars().count() <= MAX_LEN {
        return Some(trimmed.to_string());
    }
    // Truncate to MAX_LEN chars, then back off to the last whitespace
    // so we don't end mid-word. Append an ellipsis.
    let mut truncated: String = trimmed.chars().take(MAX_LEN).collect();
    if let Some(last_ws) = truncated.rfind(char::is_whitespace) {
        // Only back off if we'd keep at least a third of the budget — for
        // single-long-word inputs, just truncate hard.
        if last_ws >= MAX_LEN / 3 {
            truncated.truncate(last_ws);
        }
    }
    truncated.push('\u{2026}'); // …
    Some(truncated)
}

fn extract_init_session_id(event: &Value) -> Option<String> {
    if event.get("type").and_then(Value::as_str) != Some("system") {
        return None;
    }
    if event.get("subtype").and_then(Value::as_str) != Some("init") {
        return None;
    }
    event
        .get("session_id")
        .and_then(Value::as_str)
        .map(|s| s.to_string())
}

/// Append-only event log for a single conversation. Cheap to clone (it's
/// an `Arc` internally) so the agent's stream reader and any test harness
/// can hold their own handles without contending on opens.
///
/// Writes are serialized via an internal mutex so concurrent
/// `append_event` calls produce well-formed newline-terminated JSON lines
/// in some order — never interleaved. A crash mid-write at most loses the
/// trailing line; lines committed before the crash are valid prefix.
#[derive(Debug, Clone)]
pub struct ConversationLog {
    path: PathBuf,
    writer: Arc<Mutex<()>>,
    storage: Storage,
    workspace_id: String,
    conversation_id: String,
}

impl ConversationLog {
    fn open(
        path: PathBuf,
        storage: Storage,
        workspace_id: String,
        conversation_id: String,
    ) -> Self {
        Self {
            path,
            writer: Arc::new(Mutex::new(())),
            storage,
            workspace_id,
            conversation_id,
        }
    }

    #[allow(dead_code)] // used by tests + future stories (09)
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns true if the log file has been created on disk (i.e. at
    /// least one event has been appended).
    #[allow(dead_code)] // used by tests + future stories (09)
    pub fn exists(&self) -> bool {
        self.path.exists()
    }

    /// Append a single JSON event to the log as one newline-terminated
    /// line. Creates the file on the first call. Also updates the
    /// workspace's conversation index (title, `last_active_at`, captured
    /// claude session id).
    pub fn append_event(&self, event: &Value) -> Result<()> {
        let _guard = self
            .writer
            .lock()
            .map_err(|_| anyhow!("conversation log mutex poisoned"))?;
        let mut line = serde_json::to_vec(event).context("serialize event")?;
        line.push(b'\n');
        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .with_context(|| format!("open {:?}", self.path))?;
        f.write_all(&line)
            .with_context(|| format!("append to {:?}", self.path))?;
        // Best-effort flush; we don't fsync per event (expensive in a hot
        // stream loop). A crash may lose the trailing N kB of the buffer
        // but never produces a torn line, because each write() is atomic
        // and we always end with `\n`.

        // Update the index in the same critical section so the index can't
        // claim a higher last_active_at than what's actually on disk.
        if let Err(err) = self.storage.update_conversation_for_event(
            &self.workspace_id,
            &self.conversation_id,
            event,
        ) {
            // Don't propagate — losing the index update is annoying but
            // not catastrophic (it's rebuildable from the jsonl files).
            tracing::warn!(?err, "failed to update conversation index");
        }
        Ok(())
    }
}

fn write_json_atomic<T: Serialize + ?Sized>(path: &Path, value: &T) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("path has no parent: {:?}", path))?;
    fs::create_dir_all(parent).with_context(|| format!("create {:?}", parent))?;
    let bytes = serde_json::to_vec_pretty(value).context("serialize JSON")?;
    let tmp = path.with_extension("json.tmp");
    {
        let mut f = fs::File::create(&tmp).with_context(|| format!("create {:?}", tmp))?;
        f.write_all(&bytes)
            .with_context(|| format!("write {:?}", tmp))?;
        f.sync_all().with_context(|| format!("fsync {:?}", tmp))?;
    }
    fs::rename(&tmp, path).with_context(|| format!("rename {:?} -> {:?}", tmp, path))?;
    Ok(())
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn open_store() -> (Storage, TempDir, TempDir) {
        // Two tempdirs: one is the storage root, one is a folder we'll use
        // as a candidate "workspace path" so .is_dir() returns true.
        let store_root = TempDir::new().expect("tempdir");
        let workspace_target = TempDir::new().expect("tempdir");
        let storage = Storage::open(store_root.path()).expect("open");
        (storage, store_root, workspace_target)
    }

    #[test]
    fn open_creates_workspaces_json_when_missing() {
        let (storage, root, _target) = open_store();
        assert!(root.path().join("workspaces.json").exists());
        assert_eq!(storage.list_workspaces().unwrap(), Vec::new());
    }

    #[test]
    fn create_persists_and_reads_back() {
        let (storage, root, target) = open_store();
        let ws = storage
            .create_workspace("my-app", target.path())
            .expect("create");
        assert_eq!(ws.name, "my-app");
        assert_eq!(ws.path, target.path().to_path_buf());

        // Reopen the storage and confirm persistence.
        let reopened = Storage::open(root.path()).expect("reopen");
        let list = reopened.list_workspaces().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, ws.id);
        assert!(reopened
            .workspace_dir(&ws.id)
            .join("conversations")
            .is_dir());
    }

    #[test]
    fn create_rejects_duplicate_name_case_insensitive() {
        let (storage, _root, target) = open_store();
        storage.create_workspace("my-app", target.path()).unwrap();
        let err = storage
            .create_workspace("MY-APP", target.path())
            .unwrap_err()
            .to_string();
        assert!(err.contains("already exists"), "got: {err}");
    }

    #[test]
    fn create_rejects_missing_path() {
        let (storage, _root, _target) = open_store();
        let bogus = PathBuf::from("/does/not/exist/anywhere/i/hope");
        let err = storage
            .create_workspace("ws", &bogus)
            .unwrap_err()
            .to_string();
        assert!(err.contains("does not exist"), "got: {err}");
    }

    #[test]
    fn create_rejects_relative_path() {
        let (storage, _root, _target) = open_store();
        let err = storage
            .create_workspace("ws", Path::new("relative/path"))
            .unwrap_err()
            .to_string();
        assert!(err.contains("absolute"), "got: {err}");
    }

    #[test]
    fn create_rejects_empty_name() {
        let (storage, _root, target) = open_store();
        let err = storage
            .create_workspace("   ", target.path())
            .unwrap_err()
            .to_string();
        assert!(err.contains("empty"), "got: {err}");
    }

    #[test]
    fn rename_updates_name_in_place() {
        let (storage, _root, target) = open_store();
        let ws = storage.create_workspace("old", target.path()).unwrap();
        let renamed = storage.rename_workspace(&ws.id, "new").unwrap();
        assert_eq!(renamed.name, "new");
        assert_eq!(storage.list_workspaces().unwrap()[0].name, "new");
    }

    #[test]
    fn rename_rejects_collision() {
        let (storage, _root, target) = open_store();
        let a = storage.create_workspace("a", target.path()).unwrap();
        storage.create_workspace("b", target.path()).unwrap();
        let err = storage
            .rename_workspace(&a.id, "B")
            .unwrap_err()
            .to_string();
        assert!(err.contains("already exists"), "got: {err}");
    }

    #[test]
    fn rename_allows_renaming_to_same_name() {
        // Renaming a workspace to its current name shouldn't collide with
        // itself.
        let (storage, _root, target) = open_store();
        let ws = storage.create_workspace("same", target.path()).unwrap();
        let renamed = storage.rename_workspace(&ws.id, "same").unwrap();
        assert_eq!(renamed.name, "same");
    }

    #[test]
    fn rename_unknown_id_errors() {
        let (storage, _root, _target) = open_store();
        let err = storage
            .rename_workspace("nope", "x")
            .unwrap_err()
            .to_string();
        assert!(err.contains("not found"), "got: {err}");
    }

    #[test]
    fn delete_removes_entry_and_folder() {
        let (storage, _root, target) = open_store();
        let ws = storage.create_workspace("ws", target.path()).unwrap();
        let dir = storage.workspace_dir(&ws.id);
        assert!(dir.exists());
        let new_active = storage.delete_workspace(&ws.id).unwrap();
        assert!(storage.list_workspaces().unwrap().is_empty());
        assert!(!dir.exists());
        // Workspace was never set active, so no fallback.
        assert_eq!(new_active, None);
    }

    #[test]
    fn delete_unknown_id_errors() {
        let (storage, _root, _target) = open_store();
        let err = storage.delete_workspace("nope").unwrap_err().to_string();
        assert!(err.contains("not found"), "got: {err}");
    }

    // ── active workspace ──────────────────────────────────────────────────

    #[test]
    fn get_active_returns_none_when_unset() {
        let (storage, _root, _target) = open_store();
        assert!(storage.get_active_workspace().unwrap().is_none());
    }

    #[test]
    fn set_active_persists_and_returns_workspace() {
        let (storage, root, target) = open_store();
        let ws = storage.create_workspace("ws", target.path()).unwrap();
        let active = storage.set_active_workspace(&ws.id).unwrap();
        assert_eq!(active.id, ws.id);
        // Persistence across reopen.
        let reopened = Storage::open(root.path()).unwrap();
        assert_eq!(
            reopened.get_active_workspace().unwrap().map(|w| w.id),
            Some(ws.id)
        );
    }

    #[test]
    fn set_active_bumps_last_used_at_to_newer_value() {
        let (storage, _root, target) = open_store();
        let ws = storage.create_workspace("ws", target.path()).unwrap();
        let original = ws.last_used_at;
        // Force a small wait so the millis tick advances.
        std::thread::sleep(std::time::Duration::from_millis(5));
        let active = storage.set_active_workspace(&ws.id).unwrap();
        assert!(
            active.last_used_at >= original,
            "expected last_used_at to be bumped: {} >= {}",
            active.last_used_at,
            original
        );
    }

    #[test]
    fn set_active_unknown_id_errors() {
        let (storage, _root, _target) = open_store();
        let err = storage
            .set_active_workspace("nope")
            .unwrap_err()
            .to_string();
        assert!(err.contains("not found"), "got: {err}");
    }

    #[test]
    fn get_active_clears_stale_id_when_workspace_disappeared() {
        let (storage, _root, target) = open_store();
        let ws = storage.create_workspace("ws", target.path()).unwrap();
        storage.set_active_workspace(&ws.id).unwrap();
        // Tamper: write a bogus active id manually.
        storage.write_active_id(Some("nonexistent-id")).unwrap();
        // get_active sees the mismatch and clears it.
        assert!(storage.get_active_workspace().unwrap().is_none());
        // After the clear, the on-disk active_id is None.
        assert_eq!(storage.read_active_id().unwrap(), None);
    }

    #[test]
    fn delete_active_falls_back_to_most_recently_used() {
        let (storage, _root, target) = open_store();
        let a = storage.create_workspace("a", target.path()).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(2));
        let b = storage.create_workspace("b", target.path()).unwrap();
        // Make `a` more recently used than `b`.
        std::thread::sleep(std::time::Duration::from_millis(2));
        storage.set_active_workspace(&a.id).unwrap();
        // Now active=a, last_used: a > b. Delete the currently-active a;
        // expect fallback to b (the only remaining).
        let new_active = storage.delete_workspace(&a.id).unwrap();
        assert_eq!(new_active, Some(b.id.clone()));
        assert_eq!(
            storage.get_active_workspace().unwrap().map(|w| w.id),
            Some(b.id)
        );
    }

    #[test]
    fn delete_active_clears_when_no_workspaces_remain() {
        let (storage, _root, target) = open_store();
        let ws = storage.create_workspace("ws", target.path()).unwrap();
        storage.set_active_workspace(&ws.id).unwrap();
        let new_active = storage.delete_workspace(&ws.id).unwrap();
        assert_eq!(new_active, None);
        assert!(storage.get_active_workspace().unwrap().is_none());
    }

    #[test]
    fn delete_inactive_does_not_change_active() {
        let (storage, _root, target) = open_store();
        let a = storage.create_workspace("a", target.path()).unwrap();
        let b = storage.create_workspace("b", target.path()).unwrap();
        storage.set_active_workspace(&a.id).unwrap();
        let new_active = storage.delete_workspace(&b.id).unwrap();
        // Active didn't change → no fallback id returned.
        assert_eq!(new_active, None);
        assert_eq!(
            storage.get_active_workspace().unwrap().map(|w| w.id),
            Some(a.id)
        );
    }

    // ── ConversationLog ───────────────────────────────────────────────────

    #[test]
    fn conversation_log_does_not_create_file_until_first_event() {
        let (storage, _root, target) = open_store();
        let ws = storage.create_workspace("ws", target.path()).unwrap();
        let log = storage.open_conversation(&ws.id, "conv-1").unwrap();
        assert!(!log.exists(), "no file should exist before first event");
    }

    #[test]
    fn conversation_log_appends_one_line_per_event() {
        let (storage, _root, target) = open_store();
        let ws = storage.create_workspace("ws", target.path()).unwrap();
        let log = storage.open_conversation(&ws.id, "conv-1").unwrap();
        log.append_event(&serde_json::json!({"type": "user", "text": "hi"}))
            .unwrap();
        log.append_event(&serde_json::json!({"type": "assistant", "text": "hello"}))
            .unwrap();
        let contents = fs::read_to_string(log.path()).unwrap();
        let lines: Vec<&str> = contents.lines().collect();
        assert_eq!(lines.len(), 2);
        // Every line must parse back as JSON.
        for line in &lines {
            let _: Value = serde_json::from_str(line).expect("valid JSON line");
        }
        // Each line is newline-terminated (trailing `\n` after the last).
        assert!(contents.ends_with('\n'));
    }

    #[test]
    fn conversation_log_survives_concurrent_writes_without_interleaving() {
        // Hammer the same log from N threads; assert every line is valid
        // JSON afterwards.
        let (storage, _root, target) = open_store();
        let ws = storage.create_workspace("ws", target.path()).unwrap();
        let log = storage.open_conversation(&ws.id, "conv-1").unwrap();
        let mut handles = Vec::new();
        for thread_id in 0..8 {
            let log = log.clone();
            handles.push(std::thread::spawn(move || {
                for i in 0..50 {
                    log.append_event(&serde_json::json!({
                        "thread": thread_id,
                        "i": i,
                    }))
                    .unwrap();
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        let contents = fs::read_to_string(log.path()).unwrap();
        let lines: Vec<&str> = contents.lines().collect();
        assert_eq!(lines.len(), 8 * 50);
        for (i, line) in lines.iter().enumerate() {
            serde_json::from_str::<Value>(line)
                .unwrap_or_else(|_| panic!("line {i} not valid JSON: {line:?}"));
        }
    }

    #[test]
    fn conversation_log_lives_under_workspace_conversations_folder() {
        let (storage, root, target) = open_store();
        let ws = storage.create_workspace("ws", target.path()).unwrap();
        let log = storage.open_conversation(&ws.id, "conv-1").unwrap();
        log.append_event(&serde_json::json!({"x": 1})).unwrap();
        let expected = root
            .path()
            .join("workspaces")
            .join(&ws.id)
            .join("conversations")
            .join("conv-1.jsonl");
        assert_eq!(log.path(), expected.as_path());
        assert!(expected.exists());
    }

    // ── conversation index + auto-titles ─────────────────────────────────

    fn user_text_event(text: &str) -> Value {
        serde_json::json!({
            "type": "user",
            "message": {
                "role": "user",
                "content": [{"type": "text", "text": text}],
            }
        })
    }

    #[test]
    fn derive_title_returns_short_input_verbatim() {
        assert_eq!(super::derive_title("hello"), Some("hello".to_string()));
    }

    #[test]
    fn derive_title_returns_none_for_whitespace_only() {
        assert_eq!(super::derive_title("   \n\t  "), None);
        assert_eq!(super::derive_title(""), None);
    }

    #[test]
    fn derive_title_trims_input() {
        assert_eq!(
            super::derive_title("  hi there  "),
            Some("hi there".to_string())
        );
    }

    #[test]
    fn derive_title_truncates_long_input_at_word_boundary() {
        let input = "fix the bug in the OAuth handler that drops the refresh token on retry";
        let title = super::derive_title(input).unwrap();
        assert!(title.ends_with('\u{2026}'), "got: {title}");
        assert!(title.chars().count() <= 51); // 50 + ellipsis
                                              // Should not end mid-word; the char before ellipsis must be the
                                              // last char of a word (i.e. not a letter cut from a longer word).
        let without_ellipsis = title.trim_end_matches('\u{2026}');
        assert!(
            !without_ellipsis.ends_with(|c: char| c.is_alphabetic())
                || input.starts_with(without_ellipsis),
            "expected word-boundary truncation, got: {title}"
        );
    }

    #[test]
    fn derive_title_hard_truncates_when_no_whitespace() {
        let input = "a".repeat(80);
        let title = super::derive_title(&input).unwrap();
        assert_eq!(title.chars().count(), 51); // 50 'a' + ellipsis
    }

    #[test]
    fn index_creates_entry_on_first_event() {
        let (storage, _root, target) = open_store();
        let ws = storage.create_workspace("ws", target.path()).unwrap();
        let log = storage.open_conversation(&ws.id, "c1").unwrap();
        log.append_event(&user_text_event("first message")).unwrap();
        let list = storage.load_conversation_index(&ws.id).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "c1");
        assert_eq!(list[0].title, "first message");
        assert!(!list[0].title_pinned);
        assert!(list[0].started_at > 0);
        assert!(list[0].last_active_at >= list[0].started_at);
    }

    #[test]
    fn index_bumps_last_active_on_each_event() {
        let (storage, _root, target) = open_store();
        let ws = storage.create_workspace("ws", target.path()).unwrap();
        let log = storage.open_conversation(&ws.id, "c1").unwrap();
        log.append_event(&user_text_event("hi")).unwrap();
        let first = storage.load_conversation_index(&ws.id).unwrap()[0].last_active_at;
        std::thread::sleep(std::time::Duration::from_millis(5));
        log.append_event(&serde_json::json!({"type": "assistant"}))
            .unwrap();
        let second = storage.load_conversation_index(&ws.id).unwrap()[0].last_active_at;
        assert!(second > first, "{} > {}", second, first);
    }

    #[test]
    fn index_does_not_overwrite_pinned_title() {
        let (storage, _root, target) = open_store();
        let ws = storage.create_workspace("ws", target.path()).unwrap();
        let log = storage.open_conversation(&ws.id, "c1").unwrap();
        log.append_event(&user_text_event("first")).unwrap();
        // User renames the conversation (Story 05).
        storage
            .pin_conversation_title(&ws.id, "c1", "My custom title")
            .unwrap();
        // A later user-text event must not clobber the pinned title.
        log.append_event(&user_text_event("second message"))
            .unwrap();
        let entry = &storage.load_conversation_index(&ws.id).unwrap()[0];
        assert_eq!(entry.title, "My custom title");
        assert!(entry.title_pinned);
    }

    #[test]
    fn index_uses_placeholder_when_first_event_has_no_text() {
        let (storage, _root, target) = open_store();
        let ws = storage.create_workspace("ws", target.path()).unwrap();
        let log = storage.open_conversation(&ws.id, "c1").unwrap();
        // Empty content array (e.g. attachments only that we then strip).
        log.append_event(&serde_json::json!({
            "type": "user",
            "message": {"role": "user", "content": []},
        }))
        .unwrap();
        let entry = &storage.load_conversation_index(&ws.id).unwrap()[0];
        // No user text → title is a placeholder, not empty, not derived
        // from the event. (Exact wording depends on whether the entry was
        // created via reconstruction or via the placeholder branch — both
        // are acceptable v1 placeholders.)
        assert!(!entry.title.trim().is_empty(), "got empty title");
        assert!(!entry.title_pinned);
    }

    #[test]
    fn index_captures_claude_session_id_from_init_event() {
        let (storage, _root, target) = open_store();
        let ws = storage.create_workspace("ws", target.path()).unwrap();
        let log = storage.open_conversation(&ws.id, "c1").unwrap();
        log.append_event(&serde_json::json!({
            "type": "system",
            "subtype": "init",
            "session_id": "sk-claude-abc123",
        }))
        .unwrap();
        let entry = &storage.load_conversation_index(&ws.id).unwrap()[0];
        assert_eq!(entry.claude_session_id.as_deref(), Some("sk-claude-abc123"));
    }

    #[test]
    fn index_reconstructs_from_disk_when_missing() {
        let (storage, _root, target) = open_store();
        let ws = storage.create_workspace("ws", target.path()).unwrap();
        let log = storage.open_conversation(&ws.id, "c1").unwrap();
        log.append_event(&user_text_event("hi")).unwrap();
        // Delete the index file out-of-band.
        fs::remove_file(storage.index_path(&ws.id)).unwrap();
        // Load triggers reconstruction.
        let list = storage.load_conversation_index(&ws.id).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "c1");
        // Reconstruction uses a placeholder title — the original was lost
        // when the index disappeared (jsonl-replay-to-recover is Story 09).
        assert!(list[0].title.starts_with("Conversation "));
    }

    #[test]
    fn pin_conversation_title_rejects_empty() {
        let (storage, _root, target) = open_store();
        let ws = storage.create_workspace("ws", target.path()).unwrap();
        let log = storage.open_conversation(&ws.id, "c1").unwrap();
        log.append_event(&user_text_event("hi")).unwrap();
        let err = storage
            .pin_conversation_title(&ws.id, "c1", "  ")
            .unwrap_err()
            .to_string();
        assert!(err.contains("empty"), "got: {err}");
    }

    #[test]
    fn pin_conversation_title_unknown_id_errors() {
        let (storage, _root, target) = open_store();
        let ws = storage.create_workspace("ws", target.path()).unwrap();
        let err = storage
            .pin_conversation_title(&ws.id, "nope", "x")
            .unwrap_err()
            .to_string();
        assert!(err.contains("not found"), "got: {err}");
    }

    #[test]
    fn write_json_atomic_emits_pretty_parseable_json() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("nested").join("data.json");
        let value = serde_json::json!({"a": [1, 2, 3], "b": "hi"});
        write_json_atomic(&path, &value).unwrap();
        let read_back: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(read_back, value);
    }
}
