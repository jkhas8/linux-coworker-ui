// On-disk schema and CRUD for Workspaces.
//
// Storage layout under `<root>`:
//   workspaces.json                                  — array of Workspace
//   workspaces/<workspace_id>/conversations/         — created lazily by later stories
//
// `workspaces.json` is rewritten atomically (temp file + rename) on every
// mutation so a crash leaves either the old file intact or the new file
// fully written — never a half-written one.
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
    /// is not orphaned.
    pub fn delete_workspace(&self, id: &str) -> Result<()> {
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
        Ok(())
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
        storage.delete_workspace(&ws.id).unwrap();
        assert!(storage.list_workspaces().unwrap().is_empty());
        assert!(!dir.exists());
    }

    #[test]
    fn delete_unknown_id_errors() {
        let (storage, _root, _target) = open_store();
        let err = storage.delete_workspace("nope").unwrap_err().to_string();
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
