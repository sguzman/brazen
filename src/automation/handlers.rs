use std::sync::atomic::Ordering;
use std::sync::RwLock;
use chrono::{Local, Utc};
use base64::Engine;
use image::ImageFormat;
use std::io::Cursor;
use crate::audit_log::AuditEntry;
use crate::cache::{AssetQuery, AssetStore};
use crate::permissions::Capability;
use crate::engine::PixelFormat;
use super::types::*;
use super::server::{AutomationServerState, PendingApproval};

pub async fn handle_request(
    state: &AutomationServerState,
    raw: &str,
    subscribed_topics: &mut Vec<String>,
    user_agent: Option<String>,
    client_ip: Option<String>,
) -> Option<String> {
    let parsed: Result<AutomationEnvelope<AutomationRequest>, _> = serde_json::from_str(raw);
    let Ok(envelope) = parsed else {
        return Some(
            serde_json::to_string(&AutomationResponse::<serde_json::Value> {
                id: None,
                ok: false,
                result: None,
                error: Some("invalid request".to_string()),
            })
            .unwrap(),
        );
    };
    let id = envelope.id.clone();
    let command_name = format!("{:?}", envelope.payload);
    let activity_id = id.clone().unwrap_or_else(|| {
        let count = state.handle.activity_counter.fetch_add(1, Ordering::SeqCst);
        format!("auto-{}", count)
    });

    tracing::info!(target: "brazen::automation", id, "handling automation request: {}", command_name);

    let audit_entry = AuditEntry {
        timestamp: Utc::now(),
        command: command_name.clone(),
        user_agent: user_agent.clone(),
        client_ip: client_ip.clone(),
        outcome: "pending".to_string(),
    };

    state.handle.record_activity(AutomationActivity {
        id: activity_id.clone(),
        command: command_name.clone(),
        status: AutomationActivityStatus::Running,
        timestamp: Local::now().format("%H:%M:%S").to_string(),
        output: None,
    });

    state.handle.request_repaint();

    let response = match envelope.payload {
        AutomationRequest::WindowList => {
            let response = AutomationResponse {
                id: id.clone(),
                ok: true,
                result: Some(serde_json::json!({
                    "active_window": 0,
                    "window_count": 1
                })),
                error: None,
            };
            Some(serde_json::to_string(&response).unwrap())
        }
        AutomationRequest::LogSubscribe => {
            if !subscribed_topics.iter().any(|t| t == "logs") {
                subscribed_topics.push("logs".to_string());
            }
            Some(serde_json::to_string(&AutomationResponse::<serde_json::Value> {
                id: id.clone(),
                ok: true,
                result: Some(serde_json::json!({"status": "subscribed"})),
                error: None,
            }).unwrap())
        }
        AutomationRequest::TabList => {
            if let Err(error) = ensure_tab_api(state) {
                return Some(error_response(id, &error));
            }
            let snapshot = state.handle.snapshot.read().expect("snapshot");
            let response = AutomationResponse {
                id: id.clone(),
                ok: true,
                result: Some(&snapshot.tabs),
                error: None,
            };
            Some(serde_json::to_string(&response).unwrap())
        }
        AutomationRequest::Snapshot => {
            let snapshot = state.handle.snapshot();
            let response = AutomationResponse {
                id: id.clone(),
                ok: true,
                result: Some(snapshot),
                error: None,
            };
            Some(serde_json::to_string(&response).unwrap())
        }
        AutomationRequest::TabActivate { index, tab_id } => {
            if let Err(error) = ensure_tab_api(state) {
                return Some(error_response(id, &error));
            }
            match resolve_tab_index(&state.handle.snapshot, index, tab_id.as_deref()) {
                Ok(index) => {
                    let _ = state.handle.command_tx.send(AutomationCommand::ActivateTab { index });
                    Some(ok_response(id))
                }
                Err(error) => Some(error_response(id, &error)),
            }
        }
        AutomationRequest::TabNew { url } => {
            if let Err(error) = ensure_tab_api(state) {
                return Some(error_response(id, &error));
            }
            let _ = state.handle.command_tx.send(AutomationCommand::NewTab { url });
            Some(ok_response(id))
        }
        AutomationRequest::TabClose { index, tab_id } => {
            if let Err(error) = ensure_tab_api(state) {
                return Some(error_response(id, &error));
            }
            match resolve_tab_index(&state.handle.snapshot, index, tab_id.as_deref()) {
                Ok(index) => {
                    let _ = state.handle.command_tx.send(AutomationCommand::CloseTab { index });
                    Some(ok_response(id))
                }
                Err(error) => Some(error_response(id, &error)),
            }
        }
        AutomationRequest::TabNavigate { url } => {
            if let Err(error) = ensure_tab_api(state) {
                return Some(error_response(id, &error));
            }
            let _ = state.handle.command_tx.send(AutomationCommand::Navigate { url });
            Some(ok_response(id))
        }
        AutomationRequest::TabReload => {
            if let Err(error) = ensure_tab_api(state) {
                return Some(error_response(id, &error));
            }
            let _ = state.handle.command_tx.send(AutomationCommand::Reload);
            Some(ok_response(id))
        }
        AutomationRequest::TabStop => {
            if let Err(error) = ensure_tab_api(state) {
                return Some(error_response(id, &error));
            }
            let _ = state.handle.command_tx.send(AutomationCommand::Stop);
            Some(ok_response(id))
        }
        AutomationRequest::TabBack => {
            if let Err(error) = ensure_tab_api(state) {
                return Some(error_response(id, &error));
            }
            let _ = state.handle.command_tx.send(AutomationCommand::GoBack);
            Some(ok_response(id))
        }
        AutomationRequest::TabForward => {
            if let Err(error) = ensure_tab_api(state) {
                return Some(error_response(id, &error));
            }
            let _ = state.handle.command_tx.send(AutomationCommand::GoForward);
            Some(ok_response(id))
        }
        AutomationRequest::DomQuery { selector } => {
            let (tx, rx) = tokio::sync::oneshot::channel();
            let _ = state.handle.command_tx.send(AutomationCommand::DomQuery { selector, response_tx: tx });
            match rx.await {
                Ok(Ok(result)) => {
                    let stable = match result {
                        serde_json::Value::Null => serde_json::Value::String(String::new()),
                        serde_json::Value::String(s) => serde_json::Value::String(s),
                        other => serde_json::Value::String(other.to_string()),
                    };
                    let response = AutomationResponse {
                        id: id.clone(),
                        ok: true,
                        result: Some(stable),
                        error: None,
                    };
                    Some(serde_json::to_string(&response).unwrap())
                }
                Ok(Err(error)) => Some(error_response(id, &error)),
                Err(_) => Some(error_response(id, "internal error")),
            }
        }
        AutomationRequest::Screenshot => {
            let (tx, rx) = tokio::sync::oneshot::channel();
            let _ = state.handle.command_tx.send(AutomationCommand::Screenshot { response_tx: tx });
            match rx.await {
                Ok(Ok(frame)) => {
                    let mut png_data = Vec::new();
                    let mut cursor = Cursor::new(&mut png_data);
                    let result: Result<(), String> = match frame.pixel_format {
                        PixelFormat::Rgba8 => {
                            let img_opt = image::RgbaImage::from_raw(frame.width, frame.height, frame.pixels);
                            if let Some(img) = img_opt {
                                img.write_to(&mut cursor, ImageFormat::Png).map_err(|e| e.to_string())
                            } else {
                                Err("Failed to create image from raw pixels".to_string())
                            }
                        }
                        PixelFormat::Bgra8 => {
                            let mut rgba_pixels = Vec::with_capacity(frame.pixels.len());
                            for chunk in frame.pixels.chunks_exact(4) {
                                rgba_pixels.push(chunk[2]);
                                rgba_pixels.push(chunk[1]);
                                rgba_pixels.push(chunk[0]);
                                rgba_pixels.push(chunk[3]);
                            }
                            let img_opt = image::RgbaImage::from_raw(frame.width, frame.height, rgba_pixels);
                            if let Some(img) = img_opt {
                                img.write_to(&mut cursor, ImageFormat::Png).map_err(|e| e.to_string())
                            } else {
                                Err("Failed to create image from raw pixels".to_string())
                            }
                        }
                    };
                    match result {
                        Ok(_) => {
                            let encoded = base64::engine::general_purpose::STANDARD.encode(&png_data);
                            let response = AutomationResponse {
                                id: id.clone(),
                                ok: true,
                                result: Some(encoded),
                                error: None,
                            };
                            Some(serde_json::to_string(&response).unwrap())
                        }
                        Err(error) => Some(error_response(id, &error)),
                    }
                }
                Ok(Err(error)) => Some(error_response(id, &error)),
                Err(_) => Some(error_response(id, "internal error")),
            }
        }
        AutomationRequest::ScreenshotMeta => {
            let (tx, rx) = tokio::sync::oneshot::channel();
            let _ = state.handle.command_tx.send(AutomationCommand::Screenshot { response_tx: tx });
            match rx.await {
                Ok(Ok(frame)) => {
                    let mut png_data = Vec::new();
                    let mut cursor = Cursor::new(&mut png_data);
                    let result: Result<(), String> = match frame.pixel_format {
                        PixelFormat::Rgba8 => {
                            let img_opt = image::RgbaImage::from_raw(frame.width, frame.height, frame.pixels);
                            if let Some(img) = img_opt {
                                img.write_to(&mut cursor, ImageFormat::Png).map_err(|e| e.to_string())
                            } else {
                                Err("Failed to create image from raw pixels".to_string())
                            }
                        }
                        PixelFormat::Bgra8 => {
                            let mut rgba_pixels = Vec::with_capacity(frame.pixels.len());
                            for chunk in frame.pixels.chunks_exact(4) {
                                rgba_pixels.push(chunk[2]);
                                rgba_pixels.push(chunk[1]);
                                rgba_pixels.push(chunk[0]);
                                rgba_pixels.push(chunk[3]);
                            }
                            let img_opt = image::RgbaImage::from_raw(frame.width, frame.height, rgba_pixels);
                            if let Some(img) = img_opt {
                                img.write_to(&mut cursor, ImageFormat::Png).map_err(|e| e.to_string())
                            } else {
                                Err("Failed to create image from raw pixels".to_string())
                            }
                        }
                    };
                    match result {
                        Ok(_) => {
                            let encoded = base64::engine::general_purpose::STANDARD.encode(&png_data);
                            let response = AutomationResponse {
                                id: id.clone(),
                                ok: true,
                                result: Some(serde_json::json!({
                                    "png_base64": encoded,
                                    "width": frame.width,
                                    "height": frame.height,
                                    "pixel_format": format!("{:?}", frame.pixel_format),
                                })),
                                error: None,
                            };
                            Some(serde_json::to_string(&response).unwrap())
                        }
                        Err(error) => Some(error_response(id, &error)),
                    }
                }
                Ok(Err(error)) => Some(error_response(id, &error)),
                Err(_) => Some(error_response(id, "internal error")),
            }
        }
        AutomationRequest::RenderedText => {
            let (tx, rx) = tokio::sync::oneshot::channel();
            let _ = state.handle.command_tx.send(AutomationCommand::RenderedText { response_tx: tx });
            match rx.await {
                Ok(Ok(text)) => Some(serde_json::to_string(&AutomationResponse { id: id.clone(), ok: true, result: Some(text), error: None }).unwrap()),
                Ok(Err(error)) => Some(error_response(id, &error)),
                Err(_) => Some(error_response(id, "internal error")),
            }
        }
        AutomationRequest::ArticleText => {
            let (tx, rx) = tokio::sync::oneshot::channel();
            let _ = state.handle.command_tx.send(AutomationCommand::ArticleText { response_tx: tx });
            match rx.await {
                Ok(Ok(text)) => Some(serde_json::to_string(&AutomationResponse { id: id.clone(), ok: true, result: Some(text), error: None }).unwrap()),
                Ok(Err(error)) => Some(error_response(id, &error)),
                Err(_) => Some(error_response(id, "internal error")),
            }
        }
        AutomationRequest::CacheStats => {
            if let Err(error) = ensure_cache_api(state) {
                return Some(error_response(id, &error));
            }
            let store = AssetStore::load(state.handle.cache_config.clone(), &state.handle.runtime_paths, state.handle.profile_id.clone());
            let stats = store.stats();
            Some(serde_json::to_string(&AutomationResponse { id: id.clone(), ok: true, result: Some(stats), error: None }).unwrap())
        }
        AutomationRequest::CacheQuery { query, limit } => {
            if let Err(error) = ensure_cache_api(state) {
                return Some(error_response(id, &error));
            }
            let store = AssetStore::load(state.handle.cache_config.clone(), &state.handle.runtime_paths, state.handle.profile_id.clone());
            let query = query.unwrap_or_default();
            let mut assets = store.query(query);
            if let Some(limit) = limit {
                assets.truncate(limit);
            }
            Some(serde_json::to_string(&AutomationResponse { id: id.clone(), ok: true, result: Some(assets), error: None }).unwrap())
        }
        AutomationRequest::CacheBody { asset_id } => {
            if let Err(error) = ensure_cache_api(state) {
                return Some(error_response(id, &error));
            }
            let (tx, rx) = tokio::sync::oneshot::channel();
            let _ = state.handle.command_tx.send(AutomationCommand::CacheBody { asset_id, response_tx: tx });
            match rx.await {
                Ok(Ok(body)) => Some(serde_json::to_string(&AutomationResponse { id: id.clone(), ok: true, result: Some(body), error: None }).unwrap()),
                Ok(Err(error)) => Some(error_response(id, &error)),
                Err(_) => Some(error_response(id, "internal error")),
            }
        }
        _ => Some(error_response(id, "not implemented in modularized version yet")),
    };

    if let Some(res_str) = &response {
        let mut entry = audit_entry;
        entry.outcome = if res_str.contains("\"ok\":true") { "success".to_string() } else { "failed".to_string() };
        state.audit_logger.log(entry);

        let mut snapshot = state.handle.snapshot.write().expect("snapshot");
        if let Some(act) = snapshot.activities.iter_mut().find(|a| a.id == activity_id) {
            act.status = if res_str.contains("\"ok\":true") { AutomationActivityStatus::Success } else { AutomationActivityStatus::Failed };
        }
        state.handle.request_repaint();
    }

    response
}

fn ok_response(id: Option<String>) -> String {
    serde_json::to_string(&AutomationResponse::<serde_json::Value> {
        id,
        ok: true,
        result: None,
        error: None,
    })
    .unwrap()
}

fn error_response(id: Option<String>, message: &str) -> String {
    serde_json::to_string(&AutomationResponse::<serde_json::Value> {
        id,
        ok: false,
        result: None,
        error: Some(message.to_string()),
    })
    .unwrap()
}

fn resolve_tab_index(
    snapshot: &RwLock<AutomationSnapshot>,
    index: Option<usize>,
    tab_id: Option<&str>,
) -> Result<usize, String> {
    let snapshot = snapshot.read().map_err(|_| "snapshot lock".to_string())?;
    if let Some(index) = index {
        if index < snapshot.tabs.len() {
            return Ok(index);
        } else {
            return Err("tab index out of range".to_string());
        }
    }
    if let Some(tab_id) = tab_id {
        if let Some(tab) = snapshot.tabs.iter().find(|tab| tab.tab_id == tab_id) {
            return Ok(tab.index);
        }
    }
    Err("tab not found".to_string())
}

fn ensure_tab_api(state: &AutomationServerState) -> Result<(), String> {
    if !state.handle.expose_tab_api {
        return Err("tab api is disabled in configuration".to_string());
    }
    Ok(())
}

fn ensure_cache_api(state: &AutomationServerState) -> Result<(), String> {
    if !state.handle.expose_cache_api {
        return Err("cache api is disabled in configuration".to_string());
    }
    Ok(())
}
