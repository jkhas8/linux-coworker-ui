// On-disk schema and CRUD for Workspaces.
//
// Storage layout under `<root>`:
//   workspaces.json                                  — array of Workspace
//   active.json                                      — { active_id: Option<String> }
//   workspaces/<workspace_id>/conversations/         — created lazily by later stories
//
// Both top-level JSON files are rewritten atomically (temp file + rename)
// on every mutation so a crash leaves either the old file intact or the
// new file fully written — never a half-written one.
//
// All operations are synchronous. The Tauri commands wrap them in
// `tokio::task::spawn_blocking` when called from async contexts.

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
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
