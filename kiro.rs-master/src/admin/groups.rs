//! Account group management (independent entity version).
//!
//! thismoduletake"group"from being attached to the credential / client Key the string tag, promoted to a first class entity:
//! - group in `groups.json` persisted independently in (with `credentials.json` samedirectory)
//! - credential / client Key of `groups`/`group` the field references the group**name**(keep schema compatible)
//! - add update deletecredential / Key validates that each referenced group name is already registered (to prevent typo drift)
//! - Rename goes through cascade: automatically syncs all referencing credentials and Key
//!
//! designreference `client_keys.rs` of RwLock + JSON persistence mode.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::Utc;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

/// A single group (persisted entity).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    /// Group name (primary key, case sensitive, no duplicates, no leading or trailing whitespace).
    pub name: String,
    /// note (optional)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// createtime(ISO8601)
    pub created_at: String,
}

/// Group manager (thread safe + auto persist)
pub struct GroupManager {
    inner: RwLock<Inner>,
    path: Option<PathBuf>,
}

struct Inner {
    /// by name index;HashMap guarantee O(1) existspropertyquery
    entries: std::collections::HashMap<String, Group>,
}

impl GroupManager {
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(Inner {
                entries: std::collections::HashMap::new(),
            }),
            path: None,
        }
    }

    /// from `groups.json` Loads (returns an empty manager when it does not exist).
    pub fn load<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let path = path.as_ref().to_path_buf();
        let list: Vec<Group> = if path.exists() {
            let content = std::fs::read_to_string(&path)?;
            if content.trim().is_empty() {
                Vec::new()
            } else {
                serde_json::from_str(&content)?
            }
        } else {
            Vec::new()
        };

        let mut entries = std::collections::HashMap::with_capacity(list.len());
        for g in list {
            entries.insert(g.name.clone(), g);
        }

        Ok(Self {
            inner: RwLock::new(Inner { entries }),
            path: Some(path),
        })
    }

    fn save_locked(&self, inner: &Inner) {
        let path = match &self.path {
            Some(p) => p,
            None => return,
        };
        let mut list: Vec<&Group> = inner.entries.values().collect();
        list.sort_by(|a, b| a.name.cmp(&b.name));
        match serde_json::to_string_pretty(&list) {
            Ok(json) => {
                if let Err(e) = std::fs::write(path, json) {
                    tracing::warn!("failed to write the group file: {}", e);
                }
            }
            Err(e) => tracing::warn!("failed to serialize the group: {}", e),
        }
    }

    /// list all groups (by name dictionary order)
    pub fn list(&self) -> Vec<Group> {
        let inner = self.inner.read();
        let mut list: Vec<Group> = inner.entries.values().cloned().collect();
        list.sort_by(|a, b| a.name.cmp(&b.name));
        list
    }

    /// singlequery
    pub fn get(&self, name: &str) -> Option<Group> {
        self.inner.read().entries.get(name).cloned()
    }

    /// Whether the given group exists (used by credential / Key validate before writing)
    pub fn exists(&self, name: &str) -> bool {
        self.inner.read().entries.contains_key(name)
    }

    /// Validates whether a set of names are all registered; returns the list of unregistered names (the caller decides whether to reject the write accordingly).
    #[allow(dead_code)]
    pub fn missing<'a>(&self, names: impl IntoIterator<Item = &'a str>) -> Vec<String> {
        let inner = self.inner.read();
        names
            .into_iter()
            .filter(|n| !inner.entries.contains_key(*n))
            .map(|s| s.to_string())
            .collect()
    }

    /// Creates a group. A duplicate name errors directly and does not silently overwrite (avoids accidental creation losing the note).
    pub fn create(&self, name: String, description: Option<String>) -> anyhow::Result<Group> {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            anyhow::bail!("the group name cannot be empty");
        }
        if trimmed.chars().count() > 64 {
            anyhow::bail!("group name too long (at most 64 characters)");
        }
        let mut inner = self.inner.write();
        if inner.entries.contains_key(trimmed) {
            anyhow::bail!("groupalready exists: {}", trimmed);
        }
        let group = Group {
            name: trimmed.to_string(),
            description: description.map(|d| d.trim().to_string()).filter(|d| !d.is_empty()),
            created_at: Utc::now().to_rfc3339(),
        };
        inner.entries.insert(group.name.clone(), group.clone());
        self.save_locked(&inner);
        Ok(group)
    }

    /// Updates the note (does not change the name).
    pub fn update_description(
        &self,
        name: &str,
        description: Option<String>,
    ) -> anyhow::Result<Group> {
        let mut inner = self.inner.write();
        let entry = inner
            .entries
            .get_mut(name)
            .ok_or_else(|| anyhow::anyhow!("groupdoes not exist: {}", name))?;
        entry.description = description.map(|d| d.trim().to_string()).filter(|d| !d.is_empty());
        let cloned = entry.clone();
        self.save_locked(&inner);
        Ok(cloned)
    }

    /// rename. returns `Ok(new_name)`; the caller is responsible for cascade updating credentials. / Key inreference.
    /// `new_name` must not be occupied; if with `old_name` if fully identical treat as no-op return success directly.
    pub fn rename(&self, old_name: &str, new_name: &str) -> anyhow::Result<Group> {
        let trimmed = new_name.trim();
        if trimmed.is_empty() {
            anyhow::bail!("the new group name cannot be empty");
        }
        if trimmed.chars().count() > 64 {
            anyhow::bail!("group name too long (at most 64 characters)");
        }

        let mut inner = self.inner.write();
        if !inner.entries.contains_key(old_name) {
            anyhow::bail!("groupdoes not exist: {}", old_name);
        }
        if trimmed == old_name {
            return Ok(inner.entries.get(old_name).cloned().unwrap());
        }
        if inner.entries.contains_key(trimmed) {
            anyhow::bail!("the target group name already exists: {}", trimmed);
        }
        let mut group = inner.entries.remove(old_name).unwrap();
        group.name = trimmed.to_string();
        inner.entries.insert(group.name.clone(), group.clone());
        self.save_locked(&inner);
        Ok(group)
    }

    /// Deletes a group. The caller should first confirm there are no references (or explicitly accept cascade cleanup).
    /// return `true` means it was actually deleted; return `false` means it did not exist in the first place.
    pub fn delete(&self, name: &str) -> bool {
        let mut inner = self.inner.write();
        let removed = inner.entries.remove(name).is_some();
        if removed {
            self.save_locked(&inner);
        }
        removed
    }

    /// Startup migration: from the existing name set (credential groups + Key.group aggregation) writes back into the registry.
    /// An already existing name keeps its original note. / The creation time stays unchanged; only fills gaps. Returns the number added.
    pub fn bootstrap_from_existing<I: IntoIterator<Item = String>>(&self, names: I) -> usize {
        let mut inner = self.inner.write();
        let now = Utc::now().to_rfc3339();
        let mut added = 0usize;
        for raw in names {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                continue;
            }
            if !inner.entries.contains_key(trimmed) {
                inner.entries.insert(
                    trimmed.to_string(),
                    Group {
                        name: trimmed.to_string(),
                        description: None,
                        created_at: now.clone(),
                    },
                );
                added += 1;
            }
        }
        if added > 0 {
            self.save_locked(&inner);
        }
        added
    }
}

impl Default for GroupManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Default manager path (relative to the credential directory).
pub fn default_path_in(dir: &Path) -> PathBuf {
    dir.join("groups.json")
}

/// Arc wrap, to facilitate injection axum State
pub type SharedGroupManager = Arc<GroupManager>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_then_list_sorted() {
        let mgr = GroupManager::new();
        mgr.create("zz".into(), None).unwrap();
        mgr.create("aa".into(), Some("first".into())).unwrap();
        let list = mgr.list();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].name, "aa");
        assert_eq!(list[0].description.as_deref(), Some("first"));
        assert_eq!(list[1].name, "zz");
    }

    #[test]
    fn create_rejects_duplicate() {
        let mgr = GroupManager::new();
        mgr.create("dup".into(), None).unwrap();
        assert!(mgr.create("dup".into(), None).is_err());
        assert!(mgr.create("  dup  ".into(), None).is_err()); // trim equivalent after
    }

    #[test]
    fn create_rejects_empty_or_too_long() {
        let mgr = GroupManager::new();
        assert!(mgr.create("".into(), None).is_err());
        assert!(mgr.create("   ".into(), None).is_err());
        assert!(mgr.create("a".repeat(65), None).is_err());
    }

    #[test]
    fn missing_reports_unregistered() {
        let mgr = GroupManager::new();
        mgr.create("known".into(), None).unwrap();
        let missing = mgr.missing(["known", "ghost", "another-ghost"].iter().copied());
        assert_eq!(missing.len(), 2);
        assert!(missing.contains(&"ghost".to_string()));
        assert!(missing.contains(&"another-ghost".to_string()));
    }

    #[test]
    fn rename_swaps_key() {
        let mgr = GroupManager::new();
        mgr.create("old".into(), Some("note".into())).unwrap();
        let renamed = mgr.rename("old", "new").unwrap();
        assert_eq!(renamed.name, "new");
        assert_eq!(renamed.description.as_deref(), Some("note"));
        assert!(!mgr.exists("old"));
        assert!(mgr.exists("new"));
    }

    #[test]
    fn rename_to_existing_fails() {
        let mgr = GroupManager::new();
        mgr.create("a".into(), None).unwrap();
        mgr.create("b".into(), None).unwrap();
        assert!(mgr.rename("a", "b").is_err());
        // originaldatanotchange
        assert!(mgr.exists("a"));
        assert!(mgr.exists("b"));
    }

    #[test]
    fn rename_same_name_is_noop() {
        let mgr = GroupManager::new();
        mgr.create("x".into(), None).unwrap();
        assert!(mgr.rename("x", "x").is_ok());
        assert!(mgr.rename("x", "  x  ").is_ok());
    }

    #[test]
    fn delete_returns_correct_flag() {
        let mgr = GroupManager::new();
        mgr.create("g".into(), None).unwrap();
        assert!(mgr.delete("g"));
        assert!(!mgr.delete("g"));
        assert!(!mgr.exists("g"));
    }

    #[test]
    fn bootstrap_dedups_and_skips_existing() {
        let mgr = GroupManager::new();
        mgr.create("existing".into(), Some("kept".into())).unwrap();
        let added = mgr.bootstrap_from_existing(vec![
            "existing".into(), // already exists → skip, keep the note
            "new1".into(),
            "new1".into(), // duplicate → numbertwotimesskip
            "  new2  ".into(),
            "".into(), // empty → skip
        ]);
        assert_eq!(added, 2); // new1 + new2
        let list = mgr.list();
        assert_eq!(list.len(), 3);
        // existing the note was not overwritten
        let existing = mgr.get("existing").unwrap();
        assert_eq!(existing.description.as_deref(), Some("kept"));
    }

    #[test]
    fn load_empty_file_yields_empty_manager() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("kiro_test_groups_empty_{}.json", std::process::id()));
        std::fs::write(&path, "").unwrap();
        let mgr = GroupManager::load(&path).unwrap();
        assert!(mgr.list().is_empty());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn save_roundtrip_preserves_data() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("kiro_test_groups_{}.json", std::process::id()));
        let _ = std::fs::remove_file(&path);

        let mgr = GroupManager::load(&path).unwrap();
        mgr.create("alpha".into(), Some("a-desc".into())).unwrap();
        mgr.create("beta".into(), None).unwrap();

        // heavynewload
        let mgr2 = GroupManager::load(&path).unwrap();
        let list = mgr2.list();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].name, "alpha");
        assert_eq!(list[0].description.as_deref(), Some("a-desc"));
        assert_eq!(list[1].name, "beta");

        let _ = std::fs::remove_file(&path);
    }
}
