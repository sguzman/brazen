use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};

use base64::Engine;
use http::HeaderMap;
use url::Url;

use crate::config::TerminalConfig;
use crate::mounts::MountManager;
use crate::permissions::{Capability, PermissionDecision, PermissionPolicy};
use crate::session::SessionSnapshot;

#[derive(Debug, Clone)]
pub struct VirtualResponse {
    pub headers: BTreeMap<String, String>,
    pub body: Vec<u8>,
}

fn origin_host(origin: &str) -> Option<String> {
    let origin_url = Url::parse(origin).ok();
    Some(
        origin_url
            .as_ref()
            .and_then(|u| u.host_str())
            .unwrap_or(origin)
            .to_string(),
    )
}

fn allow_origin(headers: &mut BTreeMap<String, String>, origin: &str) {
    headers.insert(
        http::header::ACCESS_CONTROL_ALLOW_ORIGIN.to_string(),
        origin.to_string(),
    );
}

fn decision_for_origin(
    permissions: &PermissionPolicy,
    origin: Option<&str>,
    capability: Capability,
) -> PermissionDecision {
    let Some(origin) = origin else {
        return PermissionDecision::Allow;
    };
    if origin == "null" {
        return PermissionDecision::Allow;
    }
    let host = origin_host(origin).unwrap_or_else(|| origin.to_string());
    permissions.decision_for_domain(&host, &capability)
}

pub fn handle_sync(
    url: &Url,
    request_headers: &HeaderMap,
    mount_manager: &MountManager,
    permissions: &PermissionPolicy,
    session: &Arc<RwLock<SessionSnapshot>>,
    terminal_config: &TerminalConfig,
) -> Option<VirtualResponse> {
    if url.scheme() != "brazen" {
        return None;
    }

    let origin = request_headers
        .get("Origin")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("null");
    let origin_opt = Some(origin);

    match url.host_str()? {
        "fs" => handle_fs(url, origin_opt, mount_manager, permissions),
        "terminal" => handle_terminal(url, origin_opt, permissions, terminal_config),
        "tabs" => handle_tabs(url, origin_opt, permissions, session),
        "mcp" => handle_mcp(url, origin_opt, permissions),
        _ => None,
    }
}

fn handle_fs(
    url: &Url,
    origin: Option<&str>,
    mount_manager: &MountManager,
    permissions: &PermissionPolicy,
) -> Option<VirtualResponse> {
    if decision_for_origin(permissions, origin, Capability::FsRead) != PermissionDecision::Allow {
        return None;
    }

    let (mount, path) = mount_manager.resolve_fs_target_with_origin(url, origin)?;

    // directory listing
    if path.is_dir() {
        let (body, mime) = mount_manager.list_directory_json_with_origin(url, origin)?;
        let mut headers = BTreeMap::new();
        headers.insert(http::header::CONTENT_TYPE.to_string(), mime.to_string());
        if let Some(origin) = origin {
            allow_origin(&mut headers, origin);
        }
        return Some(VirtualResponse { headers, body });
    }

    // write support via query parameter
    let mut write_b64: Option<String> = None;
    for (k, v) in url.query_pairs() {
        if k == "write_base64" {
            write_b64 = Some(v.into_owned());
        }
    }
    if let Some(b64) = write_b64 {
        if mount.read_only {
            return None;
        }
        if decision_for_origin(permissions, origin, Capability::FsWrite)
            != PermissionDecision::Allow
        {
            return None;
        }
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64.as_bytes())
            .ok()?;
        std::fs::write(&path, bytes).ok()?;
        let mut headers = BTreeMap::new();
        headers.insert(
            http::header::CONTENT_TYPE.to_string(),
            "application/json".to_string(),
        );
        if let Some(origin) = origin {
            allow_origin(&mut headers, origin);
        }
        return Some(VirtualResponse {
            headers,
            body: serde_json::to_vec(&serde_json::json!({"ok": true})).ok()?,
        });
    }

    let mut data = std::fs::read(&path).ok()?;
    // Streaming support via byte slicing: ?offset=<u64>&limit=<u64>
    let mut offset: Option<u64> = None;
    let mut limit: Option<u64> = None;
    for (k, v) in url.query_pairs() {
        if k == "offset" {
            offset = v.parse::<u64>().ok();
        } else if k == "limit" {
            limit = v.parse::<u64>().ok();
        }
    }
    if offset.is_some() || limit.is_some() {
        let start = offset.unwrap_or(0) as usize;
        let end = match limit {
            Some(lim) => start.saturating_add(lim as usize).min(data.len()),
            None => data.len(),
        };
        if start >= data.len() {
            data.clear();
        } else {
            data = data[start..end].to_vec();
        }
    }
    let mut headers = BTreeMap::new();
    let mime = match path.extension().and_then(|e| e.to_str()) {
        Some("html") => "text/html",
        Some("js") => "application/javascript",
        Some("css") => "text/css",
        Some("json") => "application/json",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("svg") => "image/svg+xml",
        _ => "application/octet-stream",
    };
    headers.insert(http::header::CONTENT_TYPE.to_string(), mime.to_string());
    if let Some(origin) = origin {
        allow_origin(&mut headers, origin);
    }
    Some(VirtualResponse { headers, body: data })
}

fn handle_terminal(
    url: &Url,
    origin: Option<&str>,
    permissions: &PermissionPolicy,
    terminal_config: &TerminalConfig,
) -> Option<VirtualResponse> {
    if url.path() != "/run" {
        return None;
    }
    if decision_for_origin(permissions, origin, Capability::TerminalExec) != PermissionDecision::Allow
    {
        return None;
    }

    let mut cmd = String::new();
    let mut args = Vec::new();
    for (k, v) in url.query_pairs() {
        if k == "cmd" {
            cmd = v.into_owned();
        } else if k == "arg" {
            args.push(v.into_owned());
        }
    }
    if cmd.is_empty() {
        return None;
    }
    let request = crate::terminal::TerminalRequest {
        cmd,
        args,
        cwd: None,
    };
    let response = tokio::runtime::Handle::current()
        .block_on(crate::terminal::TerminalBroker::execute(terminal_config, request));
    let body = serde_json::to_vec(&response).ok()?;
    let mut headers = BTreeMap::new();
    headers.insert(
        http::header::CONTENT_TYPE.to_string(),
        "application/json".to_string(),
    );
    if let Some(origin) = origin {
        allow_origin(&mut headers, origin);
    }
    Some(VirtualResponse { headers, body })
}

fn handle_tabs(
    url: &Url,
    origin: Option<&str>,
    permissions: &PermissionPolicy,
    session: &Arc<RwLock<SessionSnapshot>>,
) -> Option<VirtualResponse> {
    if decision_for_origin(permissions, origin, Capability::TabInspect) != PermissionDecision::Allow {
        return None;
    }
    if url.path() != "/list" {
        return None;
    }
    let session = session.read().ok()?;
    let active_window_idx = session.active_window;
    let tabs = session
        .windows
        .get(active_window_idx)
        .map(|w| &w.tabs)
        .cloned()
        .unwrap_or_default();
    let body = serde_json::to_vec(&tabs).ok()?;
    let mut headers = BTreeMap::new();
    headers.insert(
        http::header::CONTENT_TYPE.to_string(),
        "application/json".to_string(),
    );
    if let Some(origin) = origin {
        allow_origin(&mut headers, origin);
    }
    Some(VirtualResponse { headers, body })
}

fn handle_mcp(
    url: &Url,
    origin: Option<&str>,
    permissions: &PermissionPolicy,
) -> Option<VirtualResponse> {
    // Placeholder: expose tool schemas + calls later (Phase 3).
    if decision_for_origin(permissions, origin, Capability::AiToolUse) != PermissionDecision::Allow {
        return None;
    }
    let mut headers = BTreeMap::new();
    headers.insert(
        http::header::CONTENT_TYPE.to_string(),
        "application/json".to_string(),
    );
    if let Some(origin) = origin {
        allow_origin(&mut headers, origin);
    }
    let body = serde_json::to_vec(&serde_json::json!({
        "ok": false,
        "error": "mcp not implemented",
        "path": url.path()
    }))
    .ok()?;
    Some(VirtualResponse { headers, body })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn headers_with_origin(origin: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert("Origin", origin.parse().unwrap());
        headers
    }

    #[tokio::test]
    async fn fs_list_returns_json() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), b"hello").unwrap();
        let mount_manager = MountManager::new();
        mount_manager.add_mount(crate::mounts::Mount {
            name: "m".to_string(),
            mount_type: crate::mounts::MountType::FileSystem(dir.path().to_path_buf()),
            read_only: true,
            allowed_domains: vec!["example.com".to_string()],
        });
        let permissions = PermissionPolicy {
            capabilities: {
                let mut map = PermissionPolicy::default().capabilities;
                map.insert(Capability::FsRead, PermissionDecision::Allow);
                map
            },
            ..PermissionPolicy::default()
        };
        let session = Arc::new(RwLock::new(SessionSnapshot::new(
            "default".to_string(),
            "now".to_string(),
        )));
        let url = Url::parse("brazen://fs/m/").unwrap();
        let res = handle_sync(
            &url,
            &headers_with_origin("https://example.com"),
            &mount_manager,
            &permissions,
            &session,
            &TerminalConfig::default(),
        )
        .expect("response");
        assert!(String::from_utf8_lossy(&res.body).contains("a.txt"));
    }

    #[test]
    fn fs_read_supports_offset_limit() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("a.bin"), b"abcdef").unwrap();
        let mount_manager = MountManager::new();
        mount_manager.add_mount(crate::mounts::Mount {
            name: "m".to_string(),
            mount_type: crate::mounts::MountType::FileSystem(dir.path().to_path_buf()),
            read_only: true,
            allowed_domains: vec!["example.com".to_string()],
        });
        let permissions = PermissionPolicy {
            capabilities: {
                let mut map = PermissionPolicy::default().capabilities;
                map.insert(Capability::FsRead, PermissionDecision::Allow);
                map
            },
            ..PermissionPolicy::default()
        };
        let session = Arc::new(RwLock::new(SessionSnapshot::new(
            "default".to_string(),
            "now".to_string(),
        )));
        let url = Url::parse("brazen://fs/m/a.bin?offset=2&limit=3").unwrap();
        let res = handle_sync(
            &url,
            &headers_with_origin("https://example.com"),
            &mount_manager,
            &permissions,
            &session,
            &TerminalConfig::default(),
        )
        .expect("response");
        assert_eq!(res.body, b"cde");
    }

    #[test]
    fn fs_sets_cors_header() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), b"hello").unwrap();
        let mount_manager = MountManager::new();
        mount_manager.add_mount(crate::mounts::Mount {
            name: "m".to_string(),
            mount_type: crate::mounts::MountType::FileSystem(dir.path().to_path_buf()),
            read_only: true,
            allowed_domains: vec!["example.com".to_string()],
        });
        let permissions = PermissionPolicy {
            capabilities: {
                let mut map = PermissionPolicy::default().capabilities;
                map.insert(Capability::FsRead, PermissionDecision::Allow);
                map
            },
            ..PermissionPolicy::default()
        };
        let session = Arc::new(RwLock::new(SessionSnapshot::new(
            "default".to_string(),
            "now".to_string(),
        )));
        let url = Url::parse("brazen://fs/m/a.txt").unwrap();
        let res = handle_sync(
            &url,
            &headers_with_origin("https://example.com"),
            &mount_manager,
            &permissions,
            &session,
            &TerminalConfig::default(),
        )
        .expect("response");
        assert_eq!(
            res.headers
                .get(http::header::ACCESS_CONTROL_ALLOW_ORIGIN.as_str())
                .map(|s| s.as_str()),
            Some("https://example.com")
        );
    }

    #[test]
    fn fs_sniffs_content_type() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("a.html"), b"<html></html>").unwrap();
        let mount_manager = MountManager::new();
        mount_manager.add_mount(crate::mounts::Mount {
            name: "m".to_string(),
            mount_type: crate::mounts::MountType::FileSystem(dir.path().to_path_buf()),
            read_only: true,
            allowed_domains: vec!["example.com".to_string()],
        });
        let permissions = PermissionPolicy {
            capabilities: {
                let mut map = PermissionPolicy::default().capabilities;
                map.insert(Capability::FsRead, PermissionDecision::Allow);
                map
            },
            ..PermissionPolicy::default()
        };
        let session = Arc::new(RwLock::new(SessionSnapshot::new(
            "default".to_string(),
            "now".to_string(),
        )));
        let url = Url::parse("brazen://fs/m/a.html").unwrap();
        let res = handle_sync(
            &url,
            &headers_with_origin("https://example.com"),
            &mount_manager,
            &permissions,
            &session,
            &TerminalConfig::default(),
        )
        .expect("response");
        assert_eq!(
            res.headers
                .get(http::header::CONTENT_TYPE.as_str())
                .map(|s| s.as_str()),
            Some("text/html")
        );
    }

    #[test]
    fn fs_write_denied_on_read_only_mount() {
        let dir = tempdir().unwrap();
        let mount_manager = MountManager::new();
        mount_manager.add_mount(crate::mounts::Mount {
            name: "m".to_string(),
            mount_type: crate::mounts::MountType::FileSystem(dir.path().to_path_buf()),
            read_only: true,
            allowed_domains: vec!["example.com".to_string()],
        });
        let permissions = PermissionPolicy {
            capabilities: {
                let mut map = PermissionPolicy::default().capabilities;
                map.insert(Capability::FsRead, PermissionDecision::Allow);
                map.insert(Capability::FsWrite, PermissionDecision::Allow);
                map
            },
            ..PermissionPolicy::default()
        };
        let session = Arc::new(RwLock::new(SessionSnapshot::new(
            "default".to_string(),
            "now".to_string(),
        )));
        let payload = base64::engine::general_purpose::STANDARD.encode(b"hello");
        let url = Url::parse(&format!("brazen://fs/m/a.txt?write_base64={payload}")).unwrap();
        assert!(
            handle_sync(
                &url,
                &headers_with_origin("https://example.com"),
                &mount_manager,
                &permissions,
                &session,
                &TerminalConfig::default(),
            )
            .is_none()
        );
    }
}
