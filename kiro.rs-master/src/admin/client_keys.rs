//! client API Key manage
//!
//! distributed externally by the relay station"client Key"layer. the client calls `/v1/messages` carry when `csk_*`
//! formatted Key, validated by this module and by Key Records call counts and accumulation by dimension. Token.
//!
//! with upstream Kiro credential (`KiroCredentials`,`ksk_*`) are independent of each other:
//! - Upstream credential pool: the service connects to Kiro of"exit"
//! - client Key: the relay station external"entry"
//!
//! persistas `client_api_keys.json`(with `credentials.json` samedirectory).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::Utc;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use subtle::ConstantTimeEq;

/// client Key prefix (to distinguish the upstream `ksk_`)
pub const CLIENT_KEY_PREFIX: &str = "csk_";

/// singleclient Key
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientKey {
    pub id: u64,
    /// plaintext Key(relay case, validation needs the original value, does not do hash)
    pub key: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub disabled: bool,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_used_at: Option<String>,
    #[serde(default)]
    pub total_calls: u64,
    #[serde(default)]
    pub total_input_tokens: u64,
    #[serde(default)]
    pub total_output_tokens: u64,
    #[serde(default)]
    pub total_cache_creation_tokens: u64,
    #[serde(default)]
    pub total_cache_read_tokens: u64,
    /// cumulative credit billingamount(meteringEvent.usage accumulate)
    #[serde(default)]
    pub total_credits: f64,
    /// The bound account group name (optional).
    ///
    /// after setting, use that Key The requests it initiates are only scheduled to groups Upstream accounts that contain this group name (strict isolation).
    /// None Indicates no group binding, can use all accounts (like master apiKey consistent behavior).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    /// system Key(by config.json apiKey bootstrap generated, cannot be deleted / cannot be rotated).
    /// Old data has no this field; defaults to false.
    #[serde(default, skip_serializing_if = "is_false")]
    pub is_system: bool,
}

/// client Key manager
///
/// internal dual index:
/// - `by_key: HashMap<String, u64>` —— used for `/v1` during auth O(1) queryhit
/// - `entries: HashMap<u64, ClientKey>` —— used to sort by id read write detail
///
/// the validation comparison still uses `subtle::ConstantTimeEq` prevent timing attacks.
pub struct ClientKeyManager {
    inner: RwLock<Inner>,
    path: Option<PathBuf>,
}

struct Inner {
    entries: HashMap<u64, ClientKey>,
    by_key: HashMap<String, u64>,
    next_id: u64,
}

impl ClientKeyManager {
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(Inner {
                entries: HashMap::new(),
                by_key: HashMap::new(),
                next_id: 1,
            }),
            path: None,
        }
    }

    /// Loads from the file (returns an empty manager when it does not exist).
    pub fn load<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let path = path.as_ref().to_path_buf();
        let entries: Vec<ClientKey> = if path.exists() {
            let content = std::fs::read_to_string(&path)?;
            if content.trim().is_empty() {
                Vec::new()
            } else {
                serde_json::from_str(&content)?
            }
        } else {
            Vec::new()
        };

        let mut by_key = HashMap::with_capacity(entries.len());
        let mut by_id = HashMap::with_capacity(entries.len());
        let mut max_id = 0u64;
        for ck in entries {
            max_id = max_id.max(ck.id);
            by_key.insert(ck.key.clone(), ck.id);
            by_id.insert(ck.id, ck);
        }

        Ok(Self {
            inner: RwLock::new(Inner {
                entries: by_id,
                by_key,
                next_id: max_id + 1,
            }),
            path: Some(path),
        })
    }

    fn save_locked(&self, inner: &Inner) {
        let path = match &self.path {
            Some(p) => p,
            None => return,
        };
        let mut list: Vec<&ClientKey> = inner.entries.values().collect();
        list.sort_by_key(|k| k.id);
        match serde_json::to_string_pretty(&list) {
            Ok(json) => {
                if let Err(e) = std::fs::write(path, json) {
                    tracing::warn!("writeclient Key filefailed: {}", e);
                }
            }
            Err(e) => tracing::warn!("serialize the client Key failed: {}", e),
        }
    }

    /// list (by id ascending)
    pub fn list(&self) -> Vec<ClientKey> {
        let inner = self.inner.read();
        let mut list: Vec<ClientKey> = inner.entries.values().cloned().collect();
        list.sort_by_key(|k| k.id);
        list
    }

    /// create new Key(generates a random plaintext string), returns the newly created entry.
    pub fn create(
        &self,
        name: String,
        description: Option<String>,
        group: Option<String>,
    ) -> ClientKey {
        self.create_with_key(name, description, group, generate_client_key())
    }

    /// create with the specified plaintext Key(only for first startup bootstrap use, take config.json apiKey imported directly as the first distribution key).
    /// If the plaintext already exists, skips and returns the existing entry.
    pub fn create_with_key(
        &self,
        name: String,
        description: Option<String>,
        group: Option<String>,
        plaintext: String,
    ) -> ClientKey {
        let mut inner = self.inner.write();
        // prevent bootstrap repeatedly import the same plaintext
        if let Some(&id) = inner.by_key.get(&plaintext) {
            return inner.entries.get(&id).cloned().expect("by_key and entries should be consistent");
        }
        let id = inner.next_id;
        inner.next_id += 1;
        let entry = ClientKey {
            id,
            key: plaintext.clone(),
            name,
            description,
            disabled: false,
            created_at: Utc::now().to_rfc3339(),
            last_used_at: None,
            total_calls: 0,
            total_input_tokens: 0,
            total_output_tokens: 0,
            total_cache_creation_tokens: 0,
            total_cache_read_tokens: 0,
            total_credits: 0.0,
            group: group.filter(|g| !g.trim().is_empty()),
            is_system: false,
        };
        inner.by_key.insert(plaintext, id);
        inner.entries.insert(id, entry.clone());
        self.save_locked(&inner);
        entry
    }

    /// ensure config.json apiKey correspondofsystem Key exists (idempotent, called on every startup).
    ///
    /// system Key **fixedoccupyuse id=0**: history master apiKey the usage data is all recorded in keyId=0 in bucket,
    /// fixed id=0 Lets the default key directly see all usage from before the upgrade, keeping data continuous.
    ///
    /// - if the plaintext is already in id=0: ensure is_system=true(no-op).
    /// - if the plaintext is in another id(old version bootstrap wrongly created): migrate to id=0.
    /// - the plaintext does not exist: in id=0 create new (id=0 fall back when occupied next_id, extremely rare).
    /// system Key Cannot be deleted; can be rotated (synced on rotation). config.apiKey).
    pub fn ensure_system_key(&self, name: String, description: Option<String>, plaintext: String) {
        let mut inner = self.inner.write();
        match inner.by_key.get(&plaintext).copied() {
            Some(0) => {
                // already in id=0: ensure is_system
                if let Some(e) = inner.entries.get_mut(&0) {
                    if !e.is_system {
                        e.is_system = true;
                        self.save_locked(&inner);
                    }
                }
            }
            Some(other) => {
                // plaintextinnon 0 id on: migrate as much as possible to id=0(foralign history keyId=0 usage)
                if !inner.entries.contains_key(&0) {
                    let mut entry = inner
                        .entries
                        .remove(&other)
                        .expect("by_key and entries should be consistent");
                    entry.id = 0;
                    entry.is_system = true;
                    inner.entries.insert(0, entry);
                    inner.by_key.insert(plaintext, 0);
                    self.save_locked(&inner);
                } else if let Some(e) = inner.entries.get_mut(&other) {
                    // id=0 by another Key occupied: only mark in place system
                    if !e.is_system {
                        e.is_system = true;
                        self.save_locked(&inner);
                    }
                }
            }
            None => {
                // the plaintext does not exist: in id=0 create new (if occupied then fall back next_id)
                let id = if !inner.entries.contains_key(&0) {
                    0
                } else {
                    let id = inner.next_id;
                    inner.next_id += 1;
                    id
                };
                let entry = ClientKey {
                    id,
                    key: plaintext.clone(),
                    name,
                    description,
                    disabled: false,
                    created_at: Utc::now().to_rfc3339(),
                    last_used_at: None,
                    total_calls: 0,
                    total_input_tokens: 0,
                    total_output_tokens: 0,
                    total_cache_creation_tokens: 0,
                    total_cache_read_tokens: 0,
                    total_credits: 0.0,
                    group: None,
                    is_system: true,
                };
                inner.by_key.insert(plaintext, id);
                inner.entries.insert(id, entry);
                self.save_locked(&inner);
            }
        }
    }

    pub fn delete(&self, id: u64) -> bool {
        let mut inner = self.inner.write();
        // system Key rejectdelete
        if inner.entries.get(&id).map(|e| e.is_system).unwrap_or(false) {
            return false;
        }
        let removed = match inner.entries.remove(&id) {
            Some(e) => {
                inner.by_key.remove(&e.key);
                true
            }
            None => false,
        };
        if removed {
            self.save_locked(&inner);
        }
        removed
    }

    pub fn set_disabled(&self, id: u64, disabled: bool) -> bool {
        let mut inner = self.inner.write();
        let updated = match inner.entries.get_mut(&id) {
            Some(e) => {
                e.disabled = disabled;
                true
            }
            None => false,
        };
        if updated {
            self.save_locked(&inner);
        }
        updated
    }

    pub fn update_meta(
        &self,
        id: u64,
        name: Option<String>,
        description: Option<Option<String>>,
        group: Option<Option<String>>,
    ) -> bool {
        let mut inner = self.inner.write();
        let updated = match inner.entries.get_mut(&id) {
            Some(e) => {
                if let Some(n) = name {
                    e.name = n;
                }
                if let Some(d) = description {
                    e.description = d;
                }
                if let Some(g) = group {
                    e.group = g.filter(|s| !s.trim().is_empty());
                }
                true
            }
            None => false,
        };
        if updated {
            self.save_locked(&inner);
        }
        updated
    }

    /// returnspecified Key the bound group name (None means not bound or Key does not exist)
    pub fn group_of(&self, id: u64) -> Option<String> {
        self.inner.read().entries.get(&id).and_then(|e| e.group.clone())
    }

    /// Lists all currently referenced group names (deduplicated only, no counts).
    pub fn used_group_names(&self) -> Vec<String> {
        let inner = self.inner.read();
        let mut set: std::collections::HashSet<String> = std::collections::HashSet::new();
        for e in inner.entries.values() {
            if let Some(g) = &e.group {
                set.insert(g.clone());
            }
        }
        let mut list: Vec<String> = set.into_iter().collect();
        list.sort();
        list
    }

    /// Counts how many keys the given group has Key bound (for the group management page / prompt before deletion).
    pub fn count_with_group(&self, group: &str) -> usize {
        self.inner
            .read()
            .entries
            .values()
            .filter(|e| e.group.as_deref() == Some(group))
            .count()
    }

    /// specified id of Key whether issystem Key(returns even if it does not exist false).
    pub fn is_system(&self, id: u64) -> bool {
        self.inner
            .read()
            .entries
            .get(&id)
            .map(|e| e.is_system)
            .unwrap_or(false)
    }

    /// takeallreference `old` of Key of group fieldchangeas `new`(for group rename cascade).
    /// return the affected Key count.
    pub fn rename_group(&self, old: &str, new: &str) -> usize {
        let mut inner = self.inner.write();
        let mut affected = 0usize;
        for entry in inner.entries.values_mut() {
            if entry.group.as_deref() == Some(old) {
                entry.group = Some(new.to_string());
                affected += 1;
            }
        }
        if affected > 0 {
            self.save_locked(&inner);
        }
        affected
    }

    /// takeallreference `name` of Key of group Clears the field (for force delete group cascade).
    /// return the affected Key count.
    pub fn clear_group(&self, name: &str) -> usize {
        let mut inner = self.inner.write();
        let mut affected = 0usize;
        for entry in inner.entries.values_mut() {
            if entry.group.as_deref() == Some(name) {
                entry.group = None;
                affected += 1;
            }
        }
        if affected > 0 {
            self.save_locked(&inner);
        }
        affected
    }

    /// rotate Key value: old Key Immediately invalidated, generates a new plaintext, keeps id/name/description/group/statistics/disabled/is_system.
    /// Used for the lost plaintext or suspected downstream leak cases, safer than delete and recreate (does not lose statistics or group bindings).
    /// On a hit and successful replacement, returns the new entry (including the new plaintext);id does not existreturn None.
    /// note:system Key After rotation the caller must write the new plaintext back in sync. config.json apiKey, avoiding duplicate import on the next startup.
    pub fn rotate(&self, id: u64) -> Option<ClientKey> {
        let new_key = generate_client_key();
        let mut inner = self.inner.write();
        // take out the old entry and from by_key remove from index
        let old_key = inner.entries.get(&id).map(|e| e.key.clone())?;
        inner.by_key.remove(&old_key);
        // writenewplaintext + index (is_system and other fields are kept unchanged)
        let entry = inner.entries.get_mut(&id)?;
        entry.key = new_key.clone();
        let snapshot = entry.clone();
        inner.by_key.insert(new_key, id);
        self.save_locked(&inner);
        Some(snapshot)
    }

    /// reset the count (keep Key andname)
    pub fn reset_stats(&self, id: u64) -> bool {
        let mut inner = self.inner.write();
        let updated = match inner.entries.get_mut(&id) {
            Some(e) => {
                e.total_calls = 0;
                e.total_input_tokens = 0;
                e.total_output_tokens = 0;
                e.total_cache_creation_tokens = 0;
                e.total_cache_read_tokens = 0;
                e.total_credits = 0.0;
                true
            }
            None => false,
        };
        if updated {
            self.save_locked(&inner);
        }
        updated
    }

    /// validate Key, on a hit and not disabled returns id;samewhenupdate `last_used_at`/`total_calls`
    ///
    /// use `ConstantTimeEq` for all active Key Does a constant time comparison to prevent timing attacks;
    /// previous HashMap directly lookup Serves only as a fast short circuit (after a hit it still does one constant time comparison).
    pub fn verify_and_touch(&self, presented: &str) -> Option<u64> {
        if !presented.starts_with(CLIENT_KEY_PREFIX) {
            return None;
        }
        let mut inner = self.inner.write();
        // first pass: scan all entry Does a constant time comparison to avoid HashMap short circuit leak
        let mut hit_id: Option<u64> = None;
        for (id, ck) in inner.entries.iter() {
            if ck.disabled {
                continue;
            }
            if ck.key.as_bytes().ct_eq(presented.as_bytes()).into() {
                hit_id = Some(*id);
                // not break, continues a full scan to keep constant time.
            }
        }
        let id = hit_id?;
        if let Some(entry) = inner.entries.get_mut(&id) {
            entry.total_calls += 1;
            entry.last_used_at = Some(Utc::now().to_rfc3339());
        }
        // Does not write to disk on every request (high frequency writes), instead record_usage / periodically flush persist
        Some(id)
    }

    /// accumulate at the end of the request Token usageand persist to disk
    pub fn record_usage(
        &self,
        id: u64,
        input_tokens: u64,
        output_tokens: u64,
        cache_creation_tokens: u64,
        cache_read_tokens: u64,
        credits: f64,
    ) {
        let mut inner = self.inner.write();
        if let Some(entry) = inner.entries.get_mut(&id) {
            entry.total_input_tokens += input_tokens;
            entry.total_output_tokens += output_tokens;
            entry.total_cache_creation_tokens += cache_creation_tokens;
            entry.total_cache_read_tokens += cache_read_tokens;
            if credits.is_finite() && credits > 0.0 {
                entry.total_credits += credits;
            }
            entry.last_used_at = Some(Utc::now().to_rfc3339());
        }
        self.save_locked(&inner);
    }

    /// get the post statistics active Key count (not disabled)
    pub fn active_count(&self) -> usize {
        self.inner.read().entries.values().filter(|e| !e.disabled).count()
    }
}

impl Default for ClientKeyManager {
    fn default() -> Self {
        Self::new()
    }
}

/// serde helper:bool as false skip serialization when
fn is_false(b: &bool) -> bool {
    !b
}

/// generate `csk_` prefix + 32 bit base62 randomstring
pub fn generate_client_key() -> String {
    const CHARSET: &[u8] =
        b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let body: String = (0..32)
        .map(|_| {
            let idx = fastrand::usize(..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect();
    format!("{}{}", CLIENT_KEY_PREFIX, body)
}

/// redacted display: keep the leading 8 digits (including prefix) and trailing 4 bit
pub fn mask_client_key(key: &str) -> String {
    if key.len() <= 12 {
        return key.to_string();
    }
    format!("{}...{}", &key[..8], &key[key.len() - 4..])
}

/// Default manager path (relative to the credential directory).
pub fn default_path_in(dir: &Path) -> PathBuf {
    dir.join("client_api_keys.json")
}

/// Arc wrap, to facilitate injection axum State
pub type SharedClientKeyManager = Arc<ClientKeyManager>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_and_verify() {
        let mgr = ClientKeyManager::new();
        let entry = mgr.create("test".to_string(), None, None);
        assert!(entry.key.starts_with(CLIENT_KEY_PREFIX));
        assert_eq!(mgr.verify_and_touch(&entry.key), Some(entry.id));
        // reject those without a prefix
        assert_eq!(mgr.verify_and_touch("nope"), None);
    }

    #[test]
    fn disabled_key_rejected() {
        let mgr = ClientKeyManager::new();
        let entry = mgr.create("test".to_string(), None, None);
        mgr.set_disabled(entry.id, true);
        assert_eq!(mgr.verify_and_touch(&entry.key), None);
        mgr.set_disabled(entry.id, false);
        assert_eq!(mgr.verify_and_touch(&entry.key), Some(entry.id));
    }

    #[test]
    fn record_usage_accumulates() {
        let mgr = ClientKeyManager::new();
        let entry = mgr.create("test".to_string(), None, None);
        mgr.record_usage(entry.id, 100, 50, 0, 0, 0.0);
        mgr.record_usage(entry.id, 200, 30, 5, 10, 1.5);
        let list = mgr.list();
        let e = list.iter().find(|x| x.id == entry.id).unwrap();
        assert_eq!(e.total_input_tokens, 300);
        assert_eq!(e.total_output_tokens, 80);
        assert_eq!(e.total_cache_creation_tokens, 5);
        assert_eq!(e.total_cache_read_tokens, 10);
    }

    #[test]
    fn mask_format() {
        assert_eq!(mask_client_key("csk_abcdefghijklmnop"), "csk_abcd...mnop");
        assert_eq!(mask_client_key("short"), "short");
    }

    #[test]
    fn rotate_replaces_key_but_keeps_metadata_and_stats() {
        let mgr = ClientKeyManager::new();
        let entry = mgr.create("kb".to_string(), Some("desc".into()), Some("groupA".into()));
        // accumulate some statistics
        mgr.record_usage(entry.id, 100, 50, 5, 10, 1.5);
        let old_key = entry.key.clone();
        let rotated = mgr.rotate(entry.id).expect("rotate should succeed");
        // new Key with old Key different, and still with prefix
        assert_ne!(rotated.key, old_key);
        assert!(rotated.key.starts_with(CLIENT_KEY_PREFIX));
        // metadataretain
        assert_eq!(rotated.id, entry.id);
        assert_eq!(rotated.name, "kb");
        assert_eq!(rotated.description.as_deref(), Some("desc"));
        assert_eq!(rotated.group.as_deref(), Some("groupA"));
        // statisticsretain
        assert_eq!(rotated.total_input_tokens, 100);
        assert_eq!(rotated.total_output_tokens, 50);
        // old Key immediatelyinvalid
        assert_eq!(mgr.verify_and_touch(&old_key), None);
        // new Key hit
        assert_eq!(mgr.verify_and_touch(&rotated.key), Some(entry.id));
    }

    #[test]
    fn rotate_unknown_id_returns_none() {
        let mgr = ClientKeyManager::new();
        assert!(mgr.rotate(999).is_none());
    }

    #[test]
    fn ensure_system_key_uses_id_zero() {
        let mgr = ClientKeyManager::new();
        mgr.ensure_system_key("defaultkey".into(), None, "sk-kiro-abc".into());
        // the system key is fixed at id=0, alignhistory keyId=0 usage bucket
        assert!(mgr.is_system(0));
        assert_eq!(mgr.list().first().map(|k| k.id), Some(0));
        // Idempotent: calling again does not create a duplicate.
        mgr.ensure_system_key("defaultkey".into(), None, "sk-kiro-abc".into());
        assert_eq!(mgr.list().iter().filter(|k| k.is_system).count(), 1);
    }

    #[test]
    fn ensure_system_key_migrates_misplaced_id_to_zero() {
        // simulateoldversion bootstrap take apiKey mistakenly created in id=1 upofscenario
        let mgr = ClientKeyManager::new();
        mgr.create_with_key("defaultkey".into(), None, None, "sk-kiro-abc".into());
        assert_eq!(mgr.list().first().map(|k| k.id), Some(1));
        // Startup after the fix: should migrate to id=0
        mgr.ensure_system_key("defaultkey".into(), None, "sk-kiro-abc".into());
        assert!(mgr.is_system(0));
        assert!(!mgr.list().iter().any(|k| k.id == 1 && k.key == "sk-kiro-abc"));
    }

    #[test]
    fn system_key_cannot_be_deleted() {
        let mgr = ClientKeyManager::new();
        mgr.ensure_system_key("defaultkey".into(), None, "sk-kiro-abc".into());
        assert!(!mgr.delete(0), "systemkey id=0 notdeletable");
        assert!(mgr.is_system(0));
    }
}
