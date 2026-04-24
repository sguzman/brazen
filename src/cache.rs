use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::Utc;
use globset::{Glob, GlobSet, GlobSetBuilder};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use url::Url;

use crate::config::{CacheConfig, HostCapturePolicy};
use crate::platform_paths::RuntimePaths;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, JsonSchema)]
pub enum CaptureMode {
    #[default]
    MetadataOnly,
    Selective,
    Archive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, JsonSchema)]
pub enum StorageMode {
    Memory,
    #[default]
    Disk,
    Archive,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CaptureDecision {
    pub mode: CaptureMode,
    pub capture_body: bool,
    pub truncated: bool,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, JsonSchema)]
#[serde(default)]
pub struct AssetMetadata {
    pub asset_id: String,
    pub url: String,
    pub final_url: Option<String>,
    pub method: Option<String>,
    pub status_code: Option<u16>,
    pub mime: String,
    pub size_bytes: u64,
    pub hash: Option<String>,
    pub body_key: Option<String>,
    pub created_at: String,
    pub response_headers: BTreeMap<String, String>,
    pub request_started_at: Option<String>,
    pub response_finished_at: Option<String>,
    pub duration_ms: Option<u64>,
    pub capture_mode: CaptureMode,
    pub truncated: bool,
    pub is_third_party: bool,
    pub authenticated: bool,
    pub pinned: bool,
    pub storage_mode: StorageMode,
    pub profile_id: String,
    pub session_id: Option<String>,
    pub tab_id: Option<String>,
    pub request_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
pub struct AssetQuery {
    pub url: Option<String>,
    pub mime: Option<String>,
    pub hash: Option<String>,
    pub session_id: Option<String>,
    pub tab_id: Option<String>,
    pub status_code: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CacheStats {
    pub entries: usize,
    pub total_bytes: u64,
    pub unique_blobs: usize,
    pub captured_with_body: usize,
    pub capture_ratio: f32,
}

#[derive(Debug)]
pub struct AssetStore {
    root: PathBuf,
    blobs_dir: PathBuf,
    index_path: PathBuf,
    metadata_path: PathBuf,
    headers_path: PathBuf,
    pinned_path: PathBuf,
    config: CacheConfig,
    profile_id: String,
    entries: Vec<AssetMetadata>,
    pinned: BTreeMap<String, bool>,
    mime_allowlist: Option<GlobSet>,
    mime_denylist: Option<GlobSet>,
}

impl AssetStore {
    pub fn load(config: CacheConfig, paths: &RuntimePaths, profile_id: String) -> Self {
        let root = paths.cache_dir.join(&profile_id);
        let blobs_dir = root.join("blobs");
        let index_path = root.join("index.jsonl");
        let metadata_path = root.join("metadata.jsonl");
        let headers_path = root.join("headers.jsonl");
        let pinned_path = root.join("pinned.json");
        let mime_allowlist = build_globset(&config.mime_allowlist);
        let mime_denylist = build_globset(&config.mime_denylist);
        let mut entries = read_index(&index_path).unwrap_or_default();
        let pinned = read_pins(&pinned_path).unwrap_or_default();
        for entry in &mut entries {
            if let Some(hash) = &entry.hash {
                entry.pinned = pinned.get(hash).copied().unwrap_or(false);
            }
        }

        Self {
            root,
            blobs_dir,
            index_path,
            metadata_path,
            headers_path,
            pinned_path,
            config,
            profile_id,
            entries,
            pinned,
            mime_allowlist,
            mime_denylist,
        }
    }

    pub fn entries(&self) -> &[AssetMetadata] {
        &self.entries
    }

    pub fn stats(&self) -> CacheStats {
        let entries = self.entries.len();
        let captured_with_body = self
            .entries
            .iter()
            .filter(|entry| entry.body_key.is_some())
            .count();
        let unique_blobs = self
            .entries
            .iter()
            .filter_map(|entry| entry.body_key.as_ref())
            .collect::<HashSet<_>>()
            .len();
        let total_bytes = if self.storage_mode() == StorageMode::Memory {
            0
        } else {
            self.total_blob_bytes()
        };
        let capture_ratio = if entries == 0 {
            0.0
        } else {
            captured_with_body as f32 / entries as f32
        };

        CacheStats {
            entries,
            total_bytes,
            unique_blobs,
            captured_with_body,
            capture_ratio,
        }
    }

    pub fn latest_entry(&self) -> Option<&AssetMetadata> {
        self.entries
            .iter()
            .max_by(|a, b| a.created_at.cmp(&b.created_at))
    }

    pub fn find_by_id_or_hash(&self, key: &str) -> Option<&AssetMetadata> {
        self.entries
            .iter()
            .find(|entry| entry.asset_id == key || entry.hash.as_deref() == Some(key))
    }

    pub fn blob_path(&self, body_key: &str) -> PathBuf {
        self.blobs_dir.join(body_key)
    }

    pub fn evaluate_capture(
        &self,
        url: &str,
        mime: &str,
        size_bytes: u64,
        is_third_party: bool,
        authenticated: bool,
    ) -> CaptureDecision {
        let host_policy = self.host_policy(url);
        let mut capture_mode = capture_mode_from_str(
            host_policy
                .and_then(|policy| policy.capture_mode.as_deref())
                .unwrap_or(&self.config.capture_mode),
        );
        if self.config.archive_replay_mode {
            capture_mode = CaptureModeSetting::Archive;
        }
        if !self.config.selective_body_capture && capture_mode == CaptureModeSetting::Selective {
            capture_mode = CaptureModeSetting::MetadataOnly;
        }
        let mut max_entry_bytes = host_policy
            .and_then(|policy| policy.max_entry_bytes)
            .unwrap_or(self.config.max_entry_bytes);

        if !self.host_allowed(url) {
            return CaptureDecision {
                mode: CaptureMode::MetadataOnly,
                capture_body: false,
                truncated: false,
                reason: "host-denied".to_string(),
            };
        }

        if !(host_policy
            .and_then(|policy| policy.capture_body)
            .unwrap_or(true))
        {
            return CaptureDecision {
                mode: CaptureMode::MetadataOnly,
                capture_body: false,
                truncated: false,
                reason: "host-policy".to_string(),
            };
        }

        if self.config.authenticated_only && !authenticated {
            return CaptureDecision {
                mode: CaptureMode::MetadataOnly,
                capture_body: false,
                truncated: false,
                reason: "auth-required".to_string(),
            };
        }

        if is_third_party && self.config.third_party_mode == "deny" {
            return CaptureDecision {
                mode: CaptureMode::MetadataOnly,
                capture_body: false,
                truncated: false,
                reason: "third-party-denied".to_string(),
            };
        }

        let allow_body = match capture_mode {
            CaptureModeSetting::MetadataOnly => false,
            CaptureModeSetting::All | CaptureModeSetting::Archive => {
                self.is_mime_allowed(mime, host_policy)
            }
            CaptureModeSetting::Selective => {
                self.is_mime_allowed(mime, host_policy) && self.should_capture_body(mime)
            }
        };
        let mut capture_body = allow_body;
        let mut truncated = false;

        if max_entry_bytes == 0 {
            max_entry_bytes = self.config.max_entry_bytes;
        }
        if size_bytes > max_entry_bytes {
            capture_body = false;
            truncated = true;
        }

        let mode = match capture_mode {
            CaptureModeSetting::Archive => CaptureMode::Archive,
            CaptureModeSetting::All | CaptureModeSetting::Selective => CaptureMode::Selective,
            CaptureModeSetting::MetadataOnly => CaptureMode::MetadataOnly,
        };

        CaptureDecision {
            mode,
            capture_body,
            truncated,
            reason: "policy".to_string(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn record_asset(
        &mut self,
        url: &str,
        final_url: Option<String>,
        method: Option<String>,
        status_code: Option<u16>,
        mime: &str,
        body: Option<&[u8]>,
        headers: BTreeMap<String, String>,
        is_third_party: bool,
        authenticated: bool,
        session_id: Option<String>,
        tab_id: Option<String>,
        request_id: Option<String>,
    ) -> std::io::Result<AssetMetadata> {
        self.record_asset_with_timing(
            url,
            final_url,
            method,
            status_code,
            mime,
            body,
            headers,
            is_third_party,
            authenticated,
            session_id,
            tab_id,
            request_id,
            None,
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn record_asset_with_timing(
        &mut self,
        url: &str,
        final_url: Option<String>,
        method: Option<String>,
        status_code: Option<u16>,
        mime: &str,
        body: Option<&[u8]>,
        headers: BTreeMap<String, String>,
        is_third_party: bool,
        authenticated: bool,
        session_id: Option<String>,
        tab_id: Option<String>,
        request_id: Option<String>,
        request_started_at: Option<String>,
        response_finished_at: Option<String>,
    ) -> std::io::Result<AssetMetadata> {
        std::fs::create_dir_all(&self.blobs_dir)?;
        std::fs::create_dir_all(&self.root)?;

        let asset_id = format!("asset-{}", self.entries.len() + 1);
        let size_bytes = body.map(|bytes| bytes.len() as u64).unwrap_or(0);
        let decision = self.evaluate_capture(url, mime, size_bytes, is_third_party, authenticated);
        tracing::info!(
            target: "brazen::cache",
            url,
            mime,
            size_bytes,
            mode = ?decision.mode,
            capture_body = decision.capture_body,
            truncated = decision.truncated,
            reason = %decision.reason,
            "capture decision"
        );

        let mut hash = None;
        let mut body_key = None;
        if let Some(bytes) = body {
            let digest = Sha256::digest(bytes);
            let hex_digest = hex::encode(digest);
            hash = Some(hex_digest.clone());
            if decision.capture_body && self.storage_mode() != StorageMode::Memory {
                let key = if self.config.dedupe_bodies {
                    hex_digest.clone()
                } else {
                    asset_id.clone()
                };
                body_key = Some(key.clone());
                let blob_path = self.blobs_dir.join(&key);
                if !blob_path.exists() {
                    std::fs::write(&blob_path, bytes)?;
                }
            }
        }

        let now = Utc::now().to_rfc3339();
        let duration_ms = match (&request_started_at, &response_finished_at) {
            (Some(start), Some(end)) => {
                let start_dt = chrono::DateTime::parse_from_rfc3339(start).ok();
                let end_dt = chrono::DateTime::parse_from_rfc3339(end).ok();
                start_dt
                    .zip(end_dt)
                    .and_then(|(s, e)| e.signed_duration_since(s).to_std().ok())
                    .map(|duration| duration.as_millis() as u64)
            }
            _ => None,
        };
        let pinned = hash
            .as_ref()
            .and_then(|value| self.pinned.get(value).copied())
            .unwrap_or(false);
        let metadata = AssetMetadata {
            asset_id: asset_id.clone(),
            url: url.to_string(),
            final_url,
            method,
            status_code,
            mime: mime.to_string(),
            size_bytes,
            hash,
            body_key,
            created_at: now,
            response_headers: normalize_headers(headers),
            request_started_at,
            response_finished_at,
            duration_ms,
            capture_mode: decision.mode,
            truncated: decision.truncated,
            is_third_party,
            authenticated,
            pinned,
            storage_mode: self.storage_mode(),
            profile_id: self.profile_id.clone(),
            session_id,
            tab_id,
            request_id,
        };

        append_metadata(&self.index_path, &metadata)?;
        append_metadata(&self.metadata_path, &metadata)?;
        append_headers(&self.headers_path, &metadata)?;
        self.entries.push(metadata.clone());
        self.gc_if_needed()?;
        Ok(metadata)
    }

    pub fn query(&self, query: AssetQuery) -> Vec<AssetMetadata> {
        self.entries
            .iter()
            .filter(|entry| {
                query
                    .url
                    .as_ref()
                    .map(|value| entry.url.contains(value))
                    .unwrap_or(true)
                    && query
                        .mime
                        .as_ref()
                        .map(|value| entry.mime.contains(value))
                        .unwrap_or(true)
                    && query
                        .hash
                        .as_ref()
                        .map(|value| entry.hash.as_ref() == Some(value))
                        .unwrap_or(true)
                    && query
                        .session_id
                        .as_ref()
                        .map(|value| entry.session_id.as_ref() == Some(value))
                        .unwrap_or(true)
                    && query
                        .tab_id
                        .as_ref()
                        .map(|value| entry.tab_id.as_ref() == Some(value))
                        .unwrap_or(true)
                    && query
                        .status_code
                        .map(|value| entry.status_code == Some(value))
                        .unwrap_or(true)
            })
            .cloned()
            .collect()
    }

    pub fn export_json(&self, path: &Path) -> std::io::Result<()> {
        let data = serde_json::to_vec_pretty(&self.entries).map_err(std::io::Error::other)?;
        std::fs::write(path, data)
    }

    pub fn export_jsonl(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = std::fs::File::create(path)?;
        for entry in &self.entries {
            let mut line = serde_json::to_string(entry).map_err(std::io::Error::other)?;
            line.push('\n');
            file.write_all(line.as_bytes())?;
        }
        Ok(())
    }

    pub fn export_summary(&self, path: &Path) -> std::io::Result<()> {
        let stats = self.stats();
        let summary = format!(
            "entries={}\nunique_blobs={}\nbytes={}\ncaptured_with_body={}\ncapture_ratio={:.3}\n",
            stats.entries,
            stats.unique_blobs,
            stats.total_bytes,
            stats.captured_with_body,
            stats.capture_ratio
        );
        std::fs::write(path, summary)
    }

    pub fn import_json(&mut self, path: &Path) -> std::io::Result<()> {
        let data = std::fs::read(path)?;
        let entries: Vec<AssetMetadata> =
            serde_json::from_slice(&data).map_err(std::io::Error::other)?;
        for entry in entries {
            append_metadata(&self.index_path, &entry)?;
            self.entries.push(entry);
        }
        Ok(())
    }

    pub fn import_json_merge(&mut self, path: &Path) -> std::io::Result<usize> {
        let data = std::fs::read_to_string(path)?;
        let entries: Vec<AssetMetadata> = if data.trim_start().starts_with('[') {
            serde_json::from_str(&data).map_err(std::io::Error::other)?
        } else {
            data.lines()
                .filter(|line| !line.trim().is_empty())
                .map(|line| {
                    serde_json::from_str::<AssetMetadata>(line).map_err(std::io::Error::other)
                })
                .collect::<Result<Vec<_>, _>>()?
        };
        let mut imported = 0;
        for mut entry in entries {
            if self
                .entries
                .iter()
                .any(|existing| existing.asset_id == entry.asset_id)
            {
                entry.asset_id = format!("asset-{}", self.entries.len() + 1);
            }
            append_metadata(&self.index_path, &entry)?;
            self.entries.push(entry);
            imported += 1;
        }
        Ok(imported)
    }

    pub fn build_replay_manifest(&self, path: &Path) -> std::io::Result<()> {
        let manifest = self
            .entries
            .iter()
            .map(|entry| {
                serde_json::json!({
                    "url": entry.url,
                    "hash": entry.hash,
                    "mime": entry.mime,
                    "created_at": entry.created_at,
                })
            })
            .collect::<Vec<_>>();
        let data = serde_json::to_vec_pretty(&manifest).map_err(std::io::Error::other)?;
        std::fs::write(path, data)
    }

    pub fn replay_session(&self, session_id: &str) -> Vec<AssetMetadata> {
        self.entries
            .iter()
            .filter(|entry| entry.session_id.as_deref() == Some(session_id))
            .cloned()
            .collect()
    }

    pub fn pin_asset(&mut self, hash: &str) -> std::io::Result<()> {
        self.pinned.insert(hash.to_string(), true);
        for entry in &mut self.entries {
            if entry.hash.as_deref() == Some(hash) {
                entry.pinned = true;
            }
        }
        write_pins(&self.pinned_path, &self.pinned)
    }

    pub fn unpin_asset(&mut self, hash: &str) -> std::io::Result<()> {
        self.pinned.remove(hash);
        for entry in &mut self.entries {
            if entry.hash.as_deref() == Some(hash) {
                entry.pinned = false;
            }
        }
        write_pins(&self.pinned_path, &self.pinned)
    }

    pub fn storage_mode(&self) -> StorageMode {
        match self.config.storage_mode.as_str() {
            "memory" => StorageMode::Memory,
            "archive" => StorageMode::Archive,
            _ => StorageMode::Disk,
        }
    }

    pub fn verify_asset(&self, hash: &str) -> std::io::Result<bool> {
        let blob_path = if self.blobs_dir.join(hash).exists() {
            self.blobs_dir.join(hash)
        } else if let Some(entry) = self
            .entries
            .iter()
            .find(|entry| entry.hash.as_deref() == Some(hash))
        {
            entry
                .body_key
                .as_ref()
                .map(|key| self.blobs_dir.join(key))
                .unwrap_or_else(|| self.blobs_dir.join(hash))
        } else {
            self.blobs_dir.join(hash)
        };
        let bytes = std::fs::read(blob_path)?;
        let digest = Sha256::digest(&bytes);
        Ok(hex::encode(digest) == hash)
    }

    fn gc_if_needed(&mut self) -> std::io::Result<()> {
        if self.config.gc_max_entries == 0 && self.config.max_total_bytes == 0 {
            return Ok(());
        }
        if self.config.gc_max_entries > 0
            && self.entries.len() <= self.config.gc_max_entries as usize
            && self.config.max_total_bytes == 0
        {
            return Ok(());
        }
        let mut removed = false;
        if self.config.gc_max_entries > 0 {
            removed |= self.gc_by_entry_limit()?;
        }
        if self.config.max_total_bytes > 0 && self.config.gc_strategy == "oldest" {
            removed |= self.gc_by_size_limit(self.config.max_total_bytes)?;
        }
        if removed {
            overwrite_index(&self.index_path, &self.entries)?;
        }
        Ok(())
    }

    fn gc_by_entry_limit(&mut self) -> std::io::Result<bool> {
        if self.entries.len() <= self.config.gc_max_entries as usize {
            return Ok(false);
        }
        let mut removed = false;
        let mut indices: Vec<usize> = (0..self.entries.len()).collect();
        indices.sort_by_key(|idx| self.entries[*idx].created_at.clone());
        let mut ref_counts = self.body_key_ref_counts();
        for idx in indices {
            if self.entries.len() <= self.config.gc_max_entries as usize {
                break;
            }
            let asset_id = self.entries[idx].asset_id.clone();
            if self.entries[idx]
                .hash
                .as_ref()
                .map(|hash| self.pinned.contains_key(hash))
                == Some(true)
            {
                continue;
            }
            self.remove_entry_by_id(&asset_id, &mut ref_counts)?;
            removed = true;
        }
        Ok(removed)
    }

    fn gc_by_size_limit(&mut self, max_bytes: u64) -> std::io::Result<bool> {
        let mut total_bytes = self.total_blob_bytes();
        if total_bytes <= max_bytes {
            return Ok(false);
        }
        let mut removed = false;
        let mut indices: Vec<usize> = (0..self.entries.len()).collect();
        indices.sort_by_key(|idx| self.entries[*idx].created_at.clone());
        let mut ref_counts = self.body_key_ref_counts();
        for idx in indices {
            if total_bytes <= max_bytes {
                break;
            }
            let asset_id = self.entries[idx].asset_id.clone();
            if self.entries[idx]
                .hash
                .as_ref()
                .map(|hash| self.pinned.contains_key(hash))
                == Some(true)
            {
                continue;
            }
            let removed_bytes = self.entry_blob_size(&self.entries[idx]);
            self.remove_entry_by_id(&asset_id, &mut ref_counts)?;
            total_bytes = total_bytes.saturating_sub(removed_bytes);
            removed = true;
        }
        Ok(removed)
    }

    fn remove_entry_by_id(
        &mut self,
        asset_id: &str,
        ref_counts: &mut HashMap<String, usize>,
    ) -> std::io::Result<()> {
        if let Some(index) = self
            .entries
            .iter()
            .position(|entry| entry.asset_id == asset_id)
        {
            if let Some(body_key) = self.entries[index].body_key.clone()
                && let Some(count) = ref_counts.get_mut(&body_key)
            {
                *count = count.saturating_sub(1);
                if *count == 0 {
                    let _ = std::fs::remove_file(self.blobs_dir.join(&body_key));
                }
            }
            self.entries.remove(index);
        }
        Ok(())
    }

    fn body_key_ref_counts(&self) -> HashMap<String, usize> {
        let mut counts = HashMap::new();
        for entry in &self.entries {
            if let Some(body_key) = &entry.body_key {
                *counts.entry(body_key.clone()).or_insert(0) += 1;
            }
        }
        counts
    }

    fn entry_blob_size(&self, entry: &AssetMetadata) -> u64 {
        entry
            .body_key
            .as_ref()
            .and_then(|key| std::fs::metadata(self.blobs_dir.join(key)).ok())
            .map(|meta| meta.len())
            .unwrap_or(0)
    }

    fn total_blob_bytes(&self) -> u64 {
        let mut seen = HashSet::new();
        let mut total: u64 = 0;
        for entry in &self.entries {
            if let Some(body_key) = &entry.body_key
                && seen.insert(body_key.clone())
                && let Ok(meta) = std::fs::metadata(self.blobs_dir.join(body_key))
            {
                total = total.saturating_add(meta.len());
            }
        }
        total
    }

    fn host_allowed(&self, url: &str) -> bool {
        let host = url.split('/').nth(2).unwrap_or(url);
        if self
            .config
            .host_denylist
            .iter()
            .any(|value| host.contains(value))
        {
            return false;
        }
        if self.config.host_allowlist.is_empty() {
            return true;
        }
        self.config
            .host_allowlist
            .iter()
            .any(|value| host.contains(value))
    }

    fn should_capture_body(&self, mime: &str) -> bool {
        if self.config.capture_html_json_css_js
            && matches!(
                mime,
                "text/html"
                    | "application/json"
                    | "text/css"
                    | "application/javascript"
                    | "text/javascript"
            )
        {
            return true;
        }
        if self.config.capture_media
            && (mime.starts_with("image/")
                || mime.starts_with("font/")
                || mime.starts_with("audio/")
                || mime.starts_with("video/"))
        {
            return true;
        }
        false
    }

    fn host_policy(&self, url: &str) -> Option<&HostCapturePolicy> {
        let host = Url::parse(url)
            .ok()
            .and_then(|parsed| parsed.host_str().map(|value| value.to_string()))
            .or_else(|| url.split('/').nth(2).map(|value| value.to_string()))?;
        self.config.host_overrides.iter().find_map(|(key, policy)| {
            if host == *key || host.ends_with(&format!(".{key}")) {
                Some(policy)
            } else {
                None
            }
        })
    }

    fn is_mime_allowed(&self, mime: &str, host_policy: Option<&HostCapturePolicy>) -> bool {
        if let Some(policy) = host_policy {
            if !policy.mime_denylist.is_empty()
                && matches_globset(build_globset(&policy.mime_denylist).as_ref(), mime)
            {
                return false;
            }
            if !policy.mime_allowlist.is_empty() {
                return matches_globset(build_globset(&policy.mime_allowlist).as_ref(), mime);
            }
        }

        if matches_globset(self.mime_denylist.as_ref(), mime) {
            return false;
        }
        if let Some(allowlist) = &self.mime_allowlist {
            return allowlist.is_match(mime);
        }
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CaptureModeSetting {
    MetadataOnly,
    Selective,
    Archive,
    All,
}

fn capture_mode_from_str(value: &str) -> CaptureModeSetting {
    match value {
        "metadata-only" => CaptureModeSetting::MetadataOnly,
        "archive" => CaptureModeSetting::Archive,
        "all" => CaptureModeSetting::All,
        _ => CaptureModeSetting::Selective,
    }
}

fn build_globset(patterns: &[String]) -> Option<GlobSet> {
    if patterns.is_empty() {
        return None;
    }
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        match Glob::new(pattern) {
            Ok(glob) => {
                builder.add(glob);
            }
            Err(error) => {
                tracing::warn!(
                    target: "brazen::cache",
                    pattern = %pattern,
                    %error,
                    "invalid glob pattern ignored"
                );
            }
        }
    }
    builder.build().ok()
}

fn matches_globset(globset: Option<&GlobSet>, value: &str) -> bool {
    globset.map(|set| set.is_match(value)).unwrap_or(false)
}

fn normalize_headers(headers: BTreeMap<String, String>) -> BTreeMap<String, String> {
    headers
        .into_iter()
        .map(|(key, value)| (key.to_lowercase(), value))
        .collect()
}

fn append_metadata(path: &Path, metadata: &AssetMetadata) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut line = serde_json::to_string(metadata).map_err(std::io::Error::other)?;
    line.push('\n');
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?
        .write_all(line.as_bytes())
}

fn append_headers(path: &Path, metadata: &AssetMetadata) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let entry = serde_json::json!({
        "asset_id": metadata.asset_id,
        "url": metadata.url,
        "final_url": metadata.final_url,
        "method": metadata.method,
        "status_code": metadata.status_code,
        "body_key": metadata.body_key,
        "headers": metadata.response_headers,
    });
    let mut line = serde_json::to_string(&entry).map_err(std::io::Error::other)?;
    line.push('\n');
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?
        .write_all(line.as_bytes())
}

fn overwrite_index(path: &Path, entries: &[AssetMetadata]) -> std::io::Result<()> {
    let data = entries
        .iter()
        .map(serde_json::to_string)
        .collect::<Result<Vec<_>, _>>()
        .map_err(std::io::Error::other)?
        .join("\n");
    std::fs::write(path, data)
}

fn read_index(path: &Path) -> std::io::Result<Vec<AssetMetadata>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let data = std::fs::read_to_string(path)?;
    let mut entries = Vec::new();
    for line in data.lines() {
        let entry: AssetMetadata = serde_json::from_str(line).map_err(std::io::Error::other)?;
        entries.push(entry);
    }
    Ok(entries)
}

fn read_pins(path: &Path) -> std::io::Result<BTreeMap<String, bool>> {
    if !path.exists() {
        return Ok(BTreeMap::new());
    }
    let data = std::fs::read_to_string(path)?;
    let pins: BTreeMap<String, bool> =
        serde_json::from_str(&data).map_err(std::io::Error::other)?;
    Ok(pins)
}

fn write_pins(path: &Path, pins: &BTreeMap<String, bool>) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let data = serde_json::to_vec_pretty(pins).map_err(std::io::Error::other)?;
    std::fs::write(path, data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn asset_store_deduplicates_body_hashes() {
        let dir = tempdir().unwrap();
        let config = CacheConfig::default();
        let paths = RuntimePaths {
            config_path: dir.path().join("brazen.toml"),
            data_dir: dir.path().join("data"),
            logs_dir: dir.path().join("logs"),
            profiles_dir: dir.path().join("profiles"),
            cache_dir: dir.path().join("cache"),
            downloads_dir: dir.path().join("downloads"),
            crash_dumps_dir: dir.path().join("crash"),
            active_profile_dir: dir.path().join("profiles/default"),
            session_path: dir.path().join("profiles/default/session.json"),
            audit_log_path: dir.path().join("logs/audit.jsonl"),
        };
        let mut store = AssetStore::load(config, &paths, "default".to_string());
        let headers = BTreeMap::new();
        let body = b"hello";

        let first = store
            .record_asset(
                "https://example.com",
                None,
                Some("GET".to_string()),
                Some(200),
                "text/html",
                Some(body),
                headers.clone(),
                false,
                false,
                None,
                None,
                None,
            )
            .unwrap();
        let second = store
            .record_asset(
                "https://example.com",
                None,
                Some("GET".to_string()),
                Some(200),
                "text/html",
                Some(body),
                headers,
                false,
                false,
                None,
                None,
                None,
            )
            .unwrap();

        assert_eq!(first.hash, second.hash);
        let hash = first.hash.unwrap();
        assert!(store.verify_asset(&hash).unwrap());
    }

    #[test]
    fn mime_policy_honors_glob_allowlist_and_denylist() {
        let dir = tempdir().unwrap();
        let config = CacheConfig {
            mime_allowlist: vec!["text/*".to_string(), "image/*".to_string()],
            mime_denylist: vec!["image/svg+xml".to_string()],
            ..CacheConfig::default()
        };
        let paths = RuntimePaths {
            config_path: dir.path().join("brazen.toml"),
            data_dir: dir.path().join("data"),
            logs_dir: dir.path().join("logs"),
            profiles_dir: dir.path().join("profiles"),
            cache_dir: dir.path().join("cache"),
            downloads_dir: dir.path().join("downloads"),
            crash_dumps_dir: dir.path().join("crash"),
            active_profile_dir: dir.path().join("profiles/default"),
            session_path: dir.path().join("profiles/default/session.json"),
            audit_log_path: dir.path().join("logs/audit.jsonl"),
        };
        let store = AssetStore::load(config, &paths, "default".to_string());

        assert!(store.is_mime_allowed("text/html", None));
        assert!(store.is_mime_allowed("image/png", None));
        assert!(!store.is_mime_allowed("image/svg+xml", None));
        assert!(!store.is_mime_allowed("application/octet-stream", None));
    }

    #[test]
    fn cache_storage_uses_profile_subdir() {
        let dir = tempdir().unwrap();
        let config = CacheConfig::default();
        let paths = RuntimePaths {
            config_path: dir.path().join("brazen.toml"),
            data_dir: dir.path().join("data"),
            logs_dir: dir.path().join("logs"),
            profiles_dir: dir.path().join("profiles"),
            cache_dir: dir.path().join("cache"),
            downloads_dir: dir.path().join("downloads"),
            crash_dumps_dir: dir.path().join("crash"),
            active_profile_dir: dir.path().join("profiles/default"),
            session_path: dir.path().join("profiles/default/session.json"),
            audit_log_path: dir.path().join("logs/audit.jsonl"),
        };
        let store = AssetStore::load(config, &paths, "p1".to_string());
        assert!(store.root.ends_with("cache/p1") || store.root.ends_with("cache\\p1"));
    }

    #[test]
    fn no_dedupe_mode_stores_distinct_bodies() {
        let dir = tempdir().unwrap();
        let config = CacheConfig {
            dedupe_bodies: false,
            ..CacheConfig::default()
        };
        let paths = RuntimePaths {
            config_path: dir.path().join("brazen.toml"),
            data_dir: dir.path().join("data"),
            logs_dir: dir.path().join("logs"),
            profiles_dir: dir.path().join("profiles"),
            cache_dir: dir.path().join("cache"),
            downloads_dir: dir.path().join("downloads"),
            crash_dumps_dir: dir.path().join("crash"),
            active_profile_dir: dir.path().join("profiles/default"),
            session_path: dir.path().join("profiles/default/session.json"),
            audit_log_path: dir.path().join("logs/audit.jsonl"),
        };
        let mut store = AssetStore::load(config, &paths, "default".to_string());
        let headers = BTreeMap::new();
        let body = b"hello";

        let first = store
            .record_asset(
                "https://example.com",
                None,
                Some("GET".to_string()),
                Some(200),
                "text/html",
                Some(body),
                headers.clone(),
                false,
                false,
                None,
                None,
                None,
            )
            .unwrap();
        let second = store
            .record_asset(
                "https://example.com/2",
                None,
                Some("GET".to_string()),
                Some(200),
                "text/html",
                Some(body),
                headers,
                false,
                false,
                None,
                None,
                None,
            )
            .unwrap();

        assert_eq!(first.hash, second.hash);
        assert_ne!(first.body_key, second.body_key);
        let first_key = first.body_key.unwrap();
        let second_key = second.body_key.unwrap();
        assert!(store.blob_path(&first_key).exists());
        assert!(store.blob_path(&second_key).exists());
    }
}
