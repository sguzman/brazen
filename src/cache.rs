use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::config::CacheConfig;
use crate::platform_paths::RuntimePaths;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CaptureMode {
    MetadataOnly,
    Selective,
    Archive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StorageMode {
    Memory,
    Disk,
    Archive,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureDecision {
    pub mode: CaptureMode,
    pub capture_body: bool,
    pub truncated: bool,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetMetadata {
    pub asset_id: String,
    pub url: String,
    pub mime: String,
    pub size_bytes: u64,
    pub hash: Option<String>,
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
    pub profile_id: String,
    pub session_id: Option<String>,
    pub tab_id: Option<String>,
    pub request_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetQuery {
    pub url: Option<String>,
    pub mime: Option<String>,
    pub hash: Option<String>,
    pub session_id: Option<String>,
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
}

impl AssetStore {
    pub fn load(config: CacheConfig, paths: &RuntimePaths, profile_id: String) -> Self {
        let root = paths.cache_dir.join(&profile_id);
        let blobs_dir = root.join("blobs");
        let index_path = root.join("index.jsonl");
        let metadata_path = root.join("metadata.jsonl");
        let headers_path = root.join("headers.jsonl");
        let pinned_path = root.join("pinned.json");
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
        }
    }

    pub fn entries(&self) -> &[AssetMetadata] {
        &self.entries
    }

    pub fn evaluate_capture(
        &self,
        url: &str,
        mime: &str,
        size_bytes: u64,
        is_third_party: bool,
        authenticated: bool,
    ) -> CaptureDecision {
        if !self.host_allowed(url) {
            return CaptureDecision {
                mode: CaptureMode::MetadataOnly,
                capture_body: false,
                truncated: false,
                reason: "host-denied".to_string(),
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

        let allow_body = self.should_capture_body(mime);
        let mut capture_body = allow_body;
        let mut truncated = false;

        if size_bytes > self.config.max_entry_bytes {
            capture_body = false;
            truncated = true;
        }

        let mode = if self.config.archive_replay_mode {
            CaptureMode::Archive
        } else if self.config.selective_body_capture {
            CaptureMode::Selective
        } else {
            CaptureMode::MetadataOnly
        };

        CaptureDecision {
            mode,
            capture_body,
            truncated,
            reason: "policy".to_string(),
        }
    }

    pub fn record_asset(
        &mut self,
        url: &str,
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

    pub fn record_asset_with_timing(
        &mut self,
        url: &str,
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
        if let Some(bytes) = body {
            let digest = Sha256::digest(bytes);
            let hex_digest = hex::encode(digest);
            hash = Some(hex_digest.clone());
            if decision.capture_body && self.storage_mode() != StorageMode::Memory {
                let blob_path = self.blobs_dir.join(&hex_digest);
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
            asset_id: format!("asset-{}", self.entries.len() + 1),
            url: url.to_string(),
            mime: mime.to_string(),
            size_bytes,
            hash,
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
            })
            .cloned()
            .collect()
    }

    pub fn export_json(&self, path: &Path) -> std::io::Result<()> {
        let data = serde_json::to_vec_pretty(&self.entries)
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::Other, error))?;
        std::fs::write(path, data)
    }

    pub fn import_json(&mut self, path: &Path) -> std::io::Result<()> {
        let data = std::fs::read(path)?;
        let entries: Vec<AssetMetadata> = serde_json::from_slice(&data)
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::Other, error))?;
        for entry in entries {
            append_metadata(&self.index_path, &entry)?;
            self.entries.push(entry);
        }
        Ok(())
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
        let data = serde_json::to_vec_pretty(&manifest)
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::Other, error))?;
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
        let blob_path = self.blobs_dir.join(hash);
        let bytes = std::fs::read(blob_path)?;
        let digest = Sha256::digest(&bytes);
        Ok(hex::encode(digest) == hash)
    }

    fn gc_if_needed(&mut self) -> std::io::Result<()> {
        if self.config.gc_max_entries == 0 {
            return Ok(());
        }
        if self.entries.len() <= self.config.gc_max_entries as usize {
            return Ok(());
        }
        self.entries.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        while self.entries.len() > self.config.gc_max_entries as usize {
            if let Some(entry) = self.entries.first() {
                if entry
                    .hash
                    .as_ref()
                    .map(|hash| self.pinned.contains_key(hash))
                    == Some(true)
                {
                    self.entries.rotate_left(1);
                    continue;
                }
            }
            self.entries.remove(0);
        }
        overwrite_index(&self.index_path, &self.entries)
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
        if self.config.mime_allowlist.iter().any(|entry| entry == mime) {
            return true;
        }
        if self.config.capture_html_json_css_js {
            if matches!(
                mime,
                "text/html"
                    | "application/json"
                    | "text/css"
                    | "application/javascript"
                    | "text/javascript"
            ) {
                return true;
            }
        }
        if self.config.capture_media {
            if mime.starts_with("image/")
                || mime.starts_with("font/")
                || mime.starts_with("audio/")
                || mime.starts_with("video/")
            {
                return true;
            }
        }
        false
    }
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
    let mut line = serde_json::to_string(metadata)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::Other, error))?;
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
        "headers": metadata.response_headers,
    });
    let mut line = serde_json::to_string(&entry)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::Other, error))?;
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
        .map(|entry| serde_json::to_string(entry))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::Other, error))?
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
        let entry: AssetMetadata = serde_json::from_str(line)
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::Other, error))?;
        entries.push(entry);
    }
    Ok(entries)
}

fn read_pins(path: &Path) -> std::io::Result<BTreeMap<String, bool>> {
    if !path.exists() {
        return Ok(BTreeMap::new());
    }
    let data = std::fs::read_to_string(path)?;
    let pins: BTreeMap<String, bool> = serde_json::from_str(&data)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::Other, error))?;
    Ok(pins)
}

fn write_pins(path: &Path, pins: &BTreeMap<String, bool>) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let data = serde_json::to_vec_pretty(pins)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::Other, error))?;
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
        };
        let mut store = AssetStore::load(config, &paths, "default".to_string());
        let headers = BTreeMap::new();
        let body = b"hello";

        let first = store
            .record_asset(
                "https://example.com",
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
}
