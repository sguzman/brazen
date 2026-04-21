use std::path::PathBuf;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use serde::{Serialize, Deserialize};
use url::Url;
use serde_json::json;

/// The type of resource being mounted.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum MountType {
    /// A local filesystem directory.
    FileSystem(PathBuf),
    /// A dynamic terminal session.
    Terminal,
    /// MCP tool definitions.
    Mcp,
    /// Cross-tab visibility and management.
    Tabs,
}

/// A virtual resource mount.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mount {
    /// The unique name of the mount, used in the URI (e.g., brazen://fs/<name>/...).
    pub name: String,
    /// The type and configuration of the mount.
    pub mount_type: MountType,
    /// Whether the resource is read-only.
    pub read_only: bool,
    /// Optional domain allowlist for visibility (e.g. ["chatgpt.com"]).
    /// If set and non-empty, only matching origins may access this mount.
    #[serde(default)]
    pub allowed_domains: Vec<String>,
}

#[derive(Debug, Default)]
struct MountManagerInner {
    mounts: HashMap<String, Mount>,
}

/// Thread-safe manager for virtual resource mounts.
#[derive(Debug, Clone, Default)]
pub struct MountManager {
    inner: Arc<RwLock<MountManagerInner>>,
}

impl MountManager {
    /// Create a new empty MountManager.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add or update a mount.
    pub fn add_mount(&self, mount: Mount) {
        let mut inner = self.inner.write().unwrap();
        inner.mounts.insert(mount.name.clone(), mount);
    }

    /// Remove a mount by name.
    pub fn remove_mount(&self, name: &str) {
        let mut inner = self.inner.write().unwrap();
        inner.mounts.remove(name);
    }

    /// Resolve a brazen:// URI to a local filesystem path if applicable.
    /// Returns (PathBuf, read_only) if successful.
    pub fn resolve_fs_request(&self, url: &Url) -> Option<(PathBuf, bool)> {
        self.resolve_fs_request_with_origin(url, None)
    }

    pub fn resolve_fs_request_with_origin(&self, url: &Url, origin: Option<&str>) -> Option<(PathBuf, bool)> {
        if url.scheme() != "brazen" {
            return None;
        }

        // URI format: brazen://fs/<mount_name>/<path...>
        let host = url.host_str()?;
        if host != "fs" {
            return None;
        }

        let mut path_segments = url.path_segments()?;
        let mount_name = path_segments.next()?;

        let inner = self.inner.read().unwrap();
        let mount = inner.mounts.get(mount_name)?;
        if !mount.allowed_domains.is_empty() {
            let origin = origin?;
            let origin_url = Url::parse(origin).ok();
            let host = origin_url.as_ref().and_then(|u| u.host_str()).unwrap_or(origin);
            if !mount.allowed_domains.iter().any(|d| d == host) {
                return None;
            }
        }

        match &mount.mount_type {
            MountType::FileSystem(base_path) => {
                let mut full_path = base_path.clone();
                for segment in path_segments {
                    // Avoid directory traversal attacks
                    if segment == ".." || segment.contains('/') || segment.contains('\\') {
                        return None;
                    }
                    full_path.push(segment);
                }

                // Canonicalize paths to ensure safety if they exist
                // For now, just check if it starts with the base path
                if full_path.starts_with(base_path) {
                    Some((full_path, mount.read_only))
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Resolve a brazen://fs request to a mount plus a local path (without performing IO).
    /// Returns (mount, full_path, remaining_segments).
    pub fn resolve_fs_target(&self, url: &Url) -> Option<(Mount, PathBuf)> {
        self.resolve_fs_target_with_origin(url, None)
    }

    pub fn resolve_fs_target_with_origin(&self, url: &Url, origin: Option<&str>) -> Option<(Mount, PathBuf)> {
        if url.scheme() != "brazen" {
            return None;
        }
        if url.host_str()? != "fs" {
            return None;
        }
        let mut path_segments = url.path_segments()?;
        let mount_name = path_segments.next()?;
        let inner = self.inner.read().unwrap();
        let mount = inner.mounts.get(mount_name)?.clone();
        if !mount.allowed_domains.is_empty() {
            let origin = origin?;
            let origin_url = Url::parse(origin).ok();
            let host = origin_url.as_ref().and_then(|u| u.host_str()).unwrap_or(origin);
            if !mount.allowed_domains.iter().any(|d| d == host) {
                return None;
            }
        }

        let MountType::FileSystem(base_path) = &mount.mount_type else {
            return None;
        };
        let mut full_path = base_path.clone();
        for segment in path_segments {
            if segment == ".." || segment.contains('/') || segment.contains('\\') {
                return None;
            }
            full_path.push(segment);
        }
        if full_path.starts_with(base_path) {
            Some((mount, full_path))
        } else {
            None
        }
    }

    pub fn list_directory_json(&self, url: &Url) -> Option<(Vec<u8>, &'static str)> {
        self.list_directory_json_with_origin(url, None)
    }

    pub fn list_directory_json_with_origin(
        &self,
        url: &Url,
        origin: Option<&str>,
    ) -> Option<(Vec<u8>, &'static str)> {
        let (mount, path) = self.resolve_fs_target_with_origin(url, origin)?;
        let MountType::FileSystem(base_path) = &mount.mount_type else {
            return None;
        };
        if !path.starts_with(base_path) {
            return None;
        }
        let entries = std::fs::read_dir(&path).ok()?;
        let mut out = Vec::new();
        for entry in entries.flatten() {
            let file_type = entry.file_type().ok();
            let name = entry.file_name().to_string_lossy().to_string();
            let is_dir = file_type.as_ref().map(|t| t.is_dir()).unwrap_or(false);
            let is_file = file_type.as_ref().map(|t| t.is_file()).unwrap_or(false);
            let size = entry.metadata().ok().map(|m| m.len());
            out.push(json!({
                "name": name,
                "is_dir": is_dir,
                "is_file": is_file,
                "size_bytes": size,
            }));
        }
        let payload = serde_json::to_vec(&json!({
            "mount": mount.name,
            "path": url.path(),
            "read_only": mount.read_only,
            "entries": out
        }))
        .ok()?;
        Some((payload, "application/json"))
    }

    /// Resolve a brazen://terminal request.
    pub fn resolve_terminal_request(&self, url: &Url) -> bool {
        url.scheme() == "brazen" && url.host_str() == Some("terminal")
    }

    /// Resolve a brazen://tabs request.
    pub fn resolve_tabs_request(&self, url: &Url) -> bool {
        url.scheme() == "brazen" && url.host_str() == Some("tabs")
    }

    /// Resolve a brazen://mcp request.
    pub fn resolve_mcp_request(&self, url: &Url) -> bool {
        url.scheme() == "brazen" && url.host_str() == Some("mcp")
    }

    /// List all currently active mounts.
    pub fn list_mounts(&self) -> Vec<Mount> {
        let inner = self.inner.read().unwrap();
        inner.mounts.values().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn resolve_fs_target_denies_traversal() {
        let dir = tempdir().unwrap();
        let manager = MountManager::new();
        manager.add_mount(Mount {
            name: "root".to_string(),
            mount_type: MountType::FileSystem(dir.path().to_path_buf()),
            read_only: true,
            allowed_domains: Vec::new(),
        });

        let url = Url::parse("brazen://fs/root/../secret.txt").unwrap();
        assert!(manager.resolve_fs_target(&url).is_none());
    }

    #[test]
    fn resolve_fs_target_enforces_allowed_domains() {
        let dir = tempdir().unwrap();
        let manager = MountManager::new();
        manager.add_mount(Mount {
            name: "root".to_string(),
            mount_type: MountType::FileSystem(dir.path().to_path_buf()),
            read_only: true,
            allowed_domains: vec!["example.com".to_string()],
        });

        let url = Url::parse("brazen://fs/root/file.txt").unwrap();
        assert!(manager
            .resolve_fs_target_with_origin(&url, Some("https://example.com"))
            .is_some());
        assert!(manager
            .resolve_fs_target_with_origin(&url, Some("https://evil.com"))
            .is_none());
        assert!(manager.resolve_fs_target_with_origin(&url, None).is_none());
    }
}
