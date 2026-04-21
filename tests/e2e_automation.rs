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
        .stdout(Stdio::null())
        .stderr(Stdio::null())
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
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn brazen")
}

fn start_xvfb_or_skip() -> Option<(Child, String)> {
    if std::env::var("DISPLAY").ok().is_some() || std::env::var("WAYLAND_DISPLAY").ok().is_some()
    {
        return None;
    }
    let Some(xvfb_path) = which::which("Xvfb").ok() else {
        return None;
    };
    let display = ":99".to_string();
    let child = Command::new(xvfb_path)
        .arg(&display)
        .arg("-screen")
        .arg("0")
        .arg("1280x720x24")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
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
            if current.starts_with(expected_prefix) {
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
