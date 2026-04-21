use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::json;
use futures_util::{SinkExt, StreamExt};
use base64::Engine;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::protocol::Message;
use url::Url;

fn write_test_config(path: &std::path::Path) {
    let toml = format!(
        r#"
[features]
automation_server = true

[automation]
enabled = true
bind = "ws://127.0.0.1:0/ws"
require_auth = false

[permissions]
default = "allow"
[permissions.capabilities]
tab-inspect = "allow"
dom-read = "allow"
cache-read = "allow"
terminal-exec = "allow"
terminal-output-read = "allow"
ai-tool-use = "allow"
virtual-resource-mount = "allow"
fs-read = "allow"
fs-write = "allow"

[logging]
console_filter = "info"
file_filter = "off"
"#
    );
    std::fs::write(path, toml).expect("write config");
}

fn spawn_brazen(config_path: &std::path::Path, endpoint_file: &std::path::Path) -> Child {
    let exe = env!("CARGO_BIN_EXE_brazen");
    Command::new(exe)
        .arg("--config")
        .arg(config_path)
        .env("BRAZEN_AUTOMATION_ENDPOINT_FILE", endpoint_file)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("spawn brazen")
}

fn spawn_brazen_with_display(
    config_path: &std::path::Path,
    endpoint_file: &std::path::Path,
    display: &str,
) -> Child {
    let exe = env!("CARGO_BIN_EXE_brazen");
    Command::new(exe)
        .arg("--config")
        .arg(config_path)
        .env("BRAZEN_AUTOMATION_ENDPOINT_FILE", endpoint_file)
        .env("DISPLAY", display)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("spawn brazen")
}

fn check_display_health() -> bool {
    let Ok(display) = std::env::var("DISPLAY") else {
        return false;
    };
    if display.is_empty() {
        return false;
    }
    // Try xdpyinfo to see if the display is actually reachable
    Command::new("xdpyinfo")
        .arg("-display")
        .arg(&display)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn start_xvfb_or_skip() -> Option<(Child, String)> {
    if check_display_health()
        || std::env::var("WAYLAND_DISPLAY")
            .map(|v| !v.is_empty())
            .unwrap_or(false)
    {
        return None;
    }
    let Some(xvfb_path) = which::which("Xvfb").ok() else {
        return None;
    };
    let display = format!(":{}", 100 + (std::process::id() % 1000));
    let child = Command::new(xvfb_path)
        .arg(&display)
        .arg("-screen")
        .arg("0")
        .arg("1280x720x24")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    
    // Give Xvfb a moment to start
    std::thread::sleep(Duration::from_millis(800));
    
    Some((child, display))
}

async fn wait_for_endpoint_file(path: &std::path::Path) -> Url {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if Instant::now() > deadline {
            panic!("timed out waiting for automation endpoint file: {}", path.display());
        }
        if let Ok(text) = std::fs::read_to_string(path) {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                return Url::parse(trimmed).expect("endpoint file contains url");
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn ws_roundtrip(url: &Url, payload: serde_json::Value) -> serde_json::Value {
    let (ws, _) = connect_async(url.as_str()).await.expect("connect ws");
    let (mut write, mut read) = ws.split();
    write
        .send(Message::Text(payload.to_string().into()))
        .await
        .expect("send");
    let msg = tokio::time::timeout(Duration::from_secs(5), read.next())
        .await
        .expect("recv timeout")
        .expect("recv")
        .expect("recv ok");
    let Message::Text(text) = msg else {
        panic!("expected text response");
    };
    serde_json::from_str(&text).expect("json response")
}

async fn snapshot(url: &Url) -> serde_json::Value {
    ws_roundtrip(url, json!({"id":"snap","type":"snapshot"})).await
}

async fn wait_for_url(url: &Url, expected_prefix: &str) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if Instant::now() > deadline {
            panic!("timed out waiting for url prefix: {expected_prefix}");
        }
        let response = snapshot(url).await;
        if response["ok"].as_bool().unwrap_or(false) {
            let current = response["result"]["tabs"]
                .as_array()
                .and_then(|tabs| tabs.first())
                .and_then(|tab| tab["url"].as_str())
                .unwrap_or("");
            let ready = response["result"]["document_ready"].as_bool().unwrap_or(false);
            if current.starts_with(expected_prefix) && ready {
                return;
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_boot_connect_logs_and_shutdown() {
    if std::env::var("BRAZEN_E2E").ok().as_deref() != Some("1") {
        return;
    }
    let tmp = tempfile::tempdir().expect("tempdir");
    let config_path = tmp.path().join("brazen.toml");
    let endpoint_file = tmp.path().join("endpoint.txt");
    write_test_config(&config_path);

    let mut xvfb = start_xvfb_or_skip();
    let mut child = if let Some((_, ref display)) = xvfb {
        spawn_brazen_with_display(&config_path, &endpoint_file, display)
    } else {
        spawn_brazen(&config_path, &endpoint_file)
    };
    let url = wait_for_endpoint_file(&endpoint_file).await;

    // Subscribe to logs (smoke)
    let response = ws_roundtrip(
        &url,
        json!({"id":"t1","type":"log-subscribe"}),
    )
    .await;
    assert!(response["ok"].as_bool().unwrap_or(false), "log subscribe failed: {response}");

    // Snapshot + tab list should succeed.
    let response = snapshot(&url).await;
    assert!(response["ok"].as_bool().unwrap_or(false), "snapshot failed: {response}");
    assert!(response["result"]["engine_status"].as_str().unwrap_or("") != "", "missing engine_status");
    assert!(response["result"]["page_title"].is_string(), "expected page_title string");
    assert!(response["result"]["can_go_back"].is_boolean(), "expected can_go_back bool");
    assert!(response["result"]["log_panel_open"].is_boolean(), "expected log_panel_open bool");
    let response = ws_roundtrip(&url, json!({"id":"t2","type":"tab-list"})).await;
    assert!(response["ok"].as_bool().unwrap_or(false), "tab list failed: {response}");

    // Navigate to a stable data: page and wait for the url to reflect it.
    let nav_url = "data:text/html;charset=utf-8,<html><body><h1>E2E</h1></body></html>";
    let response = ws_roundtrip(&url, json!({"id":"t3","type":"tab-navigate","url":nav_url})).await;
    assert!(response["ok"].as_bool().unwrap_or(false), "tab navigate failed: {response}");
    wait_for_url(&url, "data:text/html").await;

    // DOM query returns non-empty outerHTML.
    let response = ws_roundtrip(&url, json!({"id":"t4","type":"dom-query","selector":"body"})).await;
    assert!(response["ok"].as_bool().unwrap_or(false), "dom query failed: {response}");
    let dom = response["result"].as_str().unwrap_or("");
    assert!(!dom.trim().is_empty(), "expected non-empty dom");

    // Rendered/article text surfaces.
    let response = ws_roundtrip(&url, json!({"id":"t4b","type":"rendered-text"})).await;
    assert!(response["ok"].as_bool().unwrap_or(false), "rendered-text failed: {response}");
    assert!(!response["result"].as_str().unwrap_or("").trim().is_empty(), "expected rendered text");
    let response = ws_roundtrip(&url, json!({"id":"t4c","type":"article-text"})).await;
    assert!(response["ok"].as_bool().unwrap_or(false), "article-text failed: {response}");
    assert!(!response["result"].as_str().unwrap_or("").trim().is_empty(), "expected article text");

    // TTS enqueue + play/pause surfaces in snapshot.
    let response = ws_roundtrip(&url, json!({"id":"tts1","type":"tts-enqueue","text":"Hello"})).await;
    assert!(response["ok"].as_bool().unwrap_or(false), "tts enqueue failed: {response}");
    let response = snapshot(&url).await;
    assert_eq!(response["result"]["tts_queue_len"].as_u64().unwrap_or(0), 1);
    let response = ws_roundtrip(&url, json!({"id":"tts2","type":"tts-control","action":"play"})).await;
    assert!(response["ok"].as_bool().unwrap_or(false), "tts play failed: {response}");
    let response = snapshot(&url).await;
    assert!(response["result"]["tts_playing"].as_bool().unwrap_or(false));
    let response = ws_roundtrip(&url, json!({"id":"tts3","type":"tts-control","action":"pause"})).await;
    assert!(response["ok"].as_bool().unwrap_or(false), "tts pause failed: {response}");
    let response = snapshot(&url).await;
    assert!(!response["result"]["tts_playing"].as_bool().unwrap_or(true));

    // Screenshot-meta returns base64 PNG + dimensions.
    let response = ws_roundtrip(&url, json!({"id":"t5","type":"screenshot-meta"})).await;
    assert!(response["ok"].as_bool().unwrap_or(false), "screenshot failed: {response}");
    let b64 = response["result"]["png_base64"].as_str().unwrap_or("");
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .expect("base64 decode");
    assert!(bytes.len() > 32, "expected png bytes");
    assert!(response["result"]["width"].as_u64().unwrap_or(0) > 0, "expected width");
    assert!(response["result"]["height"].as_u64().unwrap_or(0) > 0, "expected height");

    // Window screenshot returns base64 PNG + dimensions (shell UI screenshot, not engine frame).
    let response = ws_roundtrip(&url, json!({"id":"t5b","type":"screenshot-window"})).await;
    assert!(response["ok"].as_bool().unwrap_or(false), "screenshot-window failed: {response}");
    let b64 = response["result"]["png_base64"].as_str().unwrap_or("");
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .expect("base64 decode");
    assert!(bytes.len() > 32, "expected png bytes");
    assert!(response["result"]["width"].as_u64().unwrap_or(0) > 0, "expected width");
    assert!(response["result"]["height"].as_u64().unwrap_or(0) > 0, "expected height");

    // Request shutdown and ensure process exits.
    let response = ws_roundtrip(&url, json!({"id":"t6","type":"shutdown"})).await;
    assert!(response["ok"].as_bool().unwrap_or(false), "shutdown failed: {response}");

    let status = tokio::task::spawn_blocking(move || {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Ok(Some(status)) = child.try_wait() {
                return status;
            }
            if Instant::now() > deadline {
                let _ = child.kill();
                return child.wait().expect("wait after kill");
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    })
    .await
    .expect("join");
    assert!(status.success(), "brazen did not exit cleanly: {status}");

    if let Some((mut xvfb_child, _)) = xvfb.take() {
        let _ = xvfb_child.kill();
        let _ = xvfb_child.wait();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2e_automation_roadmap_features() {
    if std::env::var("BRAZEN_E2E").ok().as_deref() != Some("1") {
        return;
    }
    let tmp = tempfile::tempdir().expect("tempdir");
    let config_path = tmp.path().join("brazen.toml");
    let endpoint_file = tmp.path().join("endpoint.txt");
    write_test_config(&config_path);

    let mut xvfb = start_xvfb_or_skip();
    let mut child = if let Some((_, ref display)) = xvfb {
        spawn_brazen_with_display(&config_path, &endpoint_file, display)
    } else {
        spawn_brazen(&config_path, &endpoint_file)
    };
    let url = wait_for_endpoint_file(&endpoint_file).await;

    // Navigate to a test page
    let nav_url = "data:text/html;charset=utf-8,<html><body><article><h1>Article</h1><p>Content</p></article></body></html>";
    let _ = ws_roundtrip(&url, json!({"id":"nav","type":"tab-navigate","url":nav_url})).await;
    wait_for_url(&url, "data:text/html").await;

    // Test rendered-text
    let response = ws_roundtrip(&url, json!({"id":"rt","type":"rendered-text"})).await;
    println!("Rendered text: {:?}", response["result"]);
    assert!(response["ok"].as_bool().unwrap_or(false), "rendered-text failed: {response}");
    assert!(response["result"].as_str().unwrap_or("").contains("Article"), "rendered-text missing content");

    // Test article-text
    let response = ws_roundtrip(&url, json!({"id":"at","type":"article-text"})).await;
    assert!(response["ok"].as_bool().unwrap_or(false), "article-text failed: {response}");
    assert!(response["result"].as_str().unwrap_or("").contains("Content"), "article-text missing content");

    // Test cache-stats
    let response = ws_roundtrip(&url, json!({"id":"cs","type":"cache-stats"})).await;
    assert!(response["ok"].as_bool().unwrap_or(false), "cache-stats failed: {response}");
    assert!(response["result"]["total_entries"].is_number(), "cache-stats missing total_entries");

    // Test interact-dom
    let response = ws_roundtrip(&url, json!({"id":"id1","type":"interact-dom","selector":"h1","event":"click"})).await;
    assert!(response["ok"].as_bool().unwrap_or(false), "interact-dom failed: {response}");

    // Test screenshot-window
    let response = ws_roundtrip(&url, json!({"id":"sw1","type":"screenshot-window"})).await;
    assert!(response["ok"].as_bool().unwrap_or(false), "screenshot-window failed: {response}");
    assert!(response["result"]["png_base64"].is_string(), "expected base64 PNG");

    // Test cache-body (negative test for now as NullEngine/data URLs might not populate cache)
    let response = ws_roundtrip(&url, json!({"id":"cb1","type":"cache-body","asset_id":"nonexistent"})).await;
    assert!(!response["ok"].as_bool().unwrap_or(true), "expected failure for nonexistent asset");
    assert!(response["error"].as_str().unwrap_or("").contains("asset not found"), "expected asset not found error");

    // Test cache-query
    let response = ws_roundtrip(&url, json!({"id":"cq","type":"cache-query","limit":10})).await;
    assert!(response["ok"].as_bool().unwrap_or(false), "cache-query failed: {response}");
    assert!(response["result"].is_array(), "cache-query expected array");

    // Test tts-control
    let response = ws_roundtrip(&url, json!({"id":"tc","type":"tts-control","action":"play"})).await;
    assert!(response["ok"].as_bool().unwrap_or(false), "tts-control failed: {response}");

    // Test tts-enqueue
    let response = ws_roundtrip(&url, json!({"id":"te","type":"tts-enqueue","text":"Hello world"})).await;
    assert!(response["ok"].as_bool().unwrap_or(false), "tts-enqueue failed: {response}");

    // Shutdown
    let _ = ws_roundtrip(&url, json!({"id":"sd","type":"shutdown"})).await;
    let _ = child.wait();

    if let Some((mut xvfb_child, _)) = xvfb.take() {
        let _ = xvfb_child.kill();
        let _ = xvfb_child.wait();
    }
}
