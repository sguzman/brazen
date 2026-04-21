use std::path::PathBuf;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use serde::{Serialize, Deserialize};
use url::Url;

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
