#![cfg(feature = "servo-upstream")]

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::engine::{AlphaMode, ColorSpace, KeyModifiers, PixelFormat};
use crate::servo_resources::{
    ResourceDirResolution, ResourceDirSource, SERVO_SOURCE_ENV, ServoResourceReader,
    resolve_resource_dir,
};
use dpi::PhysicalSize;
use libservo::{
    Code, CompositionEvent, CompositionState, DeviceIntPoint, DeviceIntRect, DeviceIntSize,
    DevicePoint, EventLoopWaker, ImeEvent, InputEvent, Key, KeyState, KeyboardEvent, LoadStatus,
    Location, Modifiers, MouseButton, MouseButtonAction, MouseButtonEvent, MouseLeftViewportEvent,
    MouseMoveEvent, NamedKey, Opts, RenderingContext, Servo, ServoBuilder, ServoDelegate,
    SoftwareRenderingContext, WebView, WebViewBuilder, WebViewDelegate, WebViewPoint, WheelDelta,
    WheelEvent, WheelMode, WebResourceLoad, WebResourceResponse,
};
use libservo::clipboard_delegate::{ClipboardDelegate, StringRequest};
use tracing_log::LogTracer;
use http::HeaderMap;
use crate::engine::EngineEvent;
use crate::mounts::MountManager;
use crate::session::SessionSnapshot;
use std::sync::RwLock;
use url::Url;

#[derive(Debug, Clone, PartialEq)]
pub struct UpstreamSnapshot {
    pub url: String,
    pub title: Option<String>,
    pub favicon_url: Option<String>,
    pub cursor: Option<libservo::Cursor>,
    pub load_status: LoadStatus,
    pub history: Vec<String>,
    pub history_index: usize,
    pub animating: bool,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct UpstreamFrame {
    pub width: u32,
    pub height: u32,
    pub stride_bytes: usize,
    pub pixel_format: PixelFormat,
    pub alpha_mode: AlphaMode,
    pub color_space: ColorSpace,
    pub pixels: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct ServoUpstreamConfig {
    pub pixel_format: PixelFormat,
    pub alpha_mode: AlphaMode,
    pub color_space: ColorSpace,
    pub enable_pixel_probe: bool,
    pub resources_dir: Option<PathBuf>,
    pub certificate_path: Option<PathBuf>,
    pub ignore_certificate_errors: bool,
}

#[derive(Debug, Clone)]
struct PixelProbeState {
    pending: bool,
    test_url: Url,
}

impl Default for UpstreamSnapshot {
    fn default() -> Self {
        Self {
            url: "about:blank".to_string(),
            title: None,
            favicon_url: None,
            cursor: None,
            load_status: LoadStatus::Started,
            history: vec!["about:blank".to_string()],
            history_index: 0,
            animating: false,
            last_error: None,
        }
    }
}

#[derive(Clone)]
pub struct BrazenWebViewDelegate {
    snapshot: Rc<RefCell<UpstreamSnapshot>>,
    frame_ready: Arc<AtomicBool>,
    mount_manager: MountManager,
    permissions: crate::permissions::PermissionPolicy,
    session: Arc<RwLock<SessionSnapshot>>,
}

impl BrazenWebViewDelegate {
    pub fn new(
        snapshot: Rc<RefCell<UpstreamSnapshot>>,
        frame_ready: Arc<AtomicBool>,
        mount_manager: MountManager,
        permissions: crate::permissions::PermissionPolicy,
        session: Arc<RwLock<SessionSnapshot>>,
    ) -> Self {
        Self {
            snapshot,
            frame_ready,
            mount_manager,
            permissions,
            session,
        }
    }
}

impl WebViewDelegate for BrazenWebViewDelegate {
    fn notify_url_changed(&self, _webview: WebView, url: Url) {
        self.snapshot.borrow_mut().url = url.to_string();
    }

    fn notify_page_title_changed(&self, _webview: WebView, title: Option<String>) {
        self.snapshot.borrow_mut().title = title;
    }

    fn notify_favicon_changed(&self, webview: WebView) {
        let favicon_url = if webview.favicon().is_some() {
            if self.snapshot.borrow().url.starts_with("http") {
                Some(format!("{}/favicon.ico", self.snapshot.borrow().url))
            } else {
                None
            }
        } else {
            None
        };
        self.snapshot.borrow_mut().favicon_url = favicon_url;
    }

    fn notify_load_status_changed(&self, _webview: WebView, status: LoadStatus) {
        self.snapshot.borrow_mut().load_status = status;
        tracing::trace!(
            target: "brazen::servo::lifecycle",
            status = ?status,
            "load status updated"
        );
    }

    fn notify_cursor_changed(&self, _webview: WebView, cursor: libservo::Cursor) {
        self.snapshot.borrow_mut().cursor = Some(cursor);
    }

    fn notify_history_changed(&self, _webview: WebView, entries: Vec<Url>, current: usize) {
        let mut snapshot = self.snapshot.borrow_mut();
        snapshot.history = entries.into_iter().map(|url| url.to_string()).collect();
        snapshot.history_index = current;
    }

    fn notify_animating_changed(&self, _webview: WebView, animating: bool) {
        self.snapshot.borrow_mut().animating = animating;
    }

    fn notify_new_frame_ready(&self, _webview: WebView) {
        self.frame_ready.store(true, Ordering::Release);
        tracing::trace!(
            target: "brazen::servo::lifecycle",
            "new frame ready"
        );
    }

    fn notify_crashed(&self, _webview: WebView, reason: String, _backtrace: Option<String>) {
        self.snapshot.borrow_mut().last_error = Some(reason);
    }

    fn request_navigation(&self, _webview: WebView, request: libservo::NavigationRequest) {
        tracing::info!(
            target: "brazen::servo::lifecycle",
            url = %request.url,
            "navigation request observed"
        );
        request.allow();
    }

    fn load_web_resource(&self, _webview: WebView, load: WebResourceLoad) {
        let url = load.request.url.clone();
        
        if let Some((path, _read_only)) = self.mount_manager.resolve_fs_request(&url) {
            tracing::info!(target: "brazen::mounts", url = %url, path = ?path, "intercepting virtual resource");
            
            // Try to read the file
            match std::fs::read(&path) {
                Ok(data) => {
                    let mut headers = HeaderMap::new();
                    // Guess mime type
                    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                        let mime = match ext {
                            "html" => "text/html",
                            "js" => "application/javascript",
                            "css" => "text/css",
                            "json" => "application/json",
                            "png" => "image/png",
                            "jpg" | "jpeg" => "image/jpeg",
                            "svg" => "image/svg+xml",
                            _ => "application/octet-stream",
                        };
                        headers.insert(http::header::CONTENT_TYPE, http::HeaderValue::from_static(mime));
                    }
                    
                    self.send_intercepted_response(load, headers, data);
                }
                Err(e) => {
                    tracing::error!(target: "brazen::mounts", url = %url, error = ?e, "failed to read virtual resource");
                }
            }
        } else if self.mount_manager.resolve_terminal_request(&url) {
            let host = url.host_str().unwrap_or("");
            let path = url.path();
            
            if path == "/run" {
                tracing::info!(target: "brazen::terminal", url = %url, "intercepting terminal run request");
                
                // Check permissions
                let origin = load.request.headers.get("Origin")
                    .and_then(|h| h.to_str().ok())
                    .unwrap_or("null");
                
                let decision = if origin == "null" {
                    crate::permissions::PermissionDecision::Allow
                } else {
                    let origin_url = Url::parse(origin).ok();
                    let host = origin_url.as_ref().and_then(|u| u.host_str()).unwrap_or(origin);
                    self.permissions.decision_for_domain(host, &crate::permissions::Capability::TerminalExec)
                };

                if decision != crate::permissions::PermissionDecision::Allow {
                    tracing::warn!(target: "brazen::terminal", origin = %origin, "denying terminal access due to permissions");
                    return;
                }

                // Extract command and args from query
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
                    tracing::error!(target: "brazen::terminal", "missing 'cmd' parameter in terminal/run");
                    return;
                }

                let intercepted = load.intercept(WebResourceResponse::new(url));
                
                // Run asynchronously
                tokio::spawn(async move {
                    let request = crate::terminal::TerminalRequest {
                        cmd,
                        args,
                        cwd: None,
                    };
                    let response = crate::terminal::TerminalBroker::execute(request).await;
                    if let Ok(data) = serde_json::to_vec(&response) {
                        intercepted.send_body_data(data);
                    }
                    intercepted.finish();
                });
            }
        } else if self.mount_manager.resolve_tabs_request(&url) {
            tracing::info!(target: "brazen::tabs", url = %url, "intercepting tabs request");
            
            // Check permissions
            let origin = load.request.headers.get("Origin")
                .and_then(|h| h.to_str().ok())
                .unwrap_or("null");
            
            let decision = if origin == "null" {
                crate::permissions::PermissionDecision::Allow
            } else {
                let origin_url = Url::parse(origin).ok();
                let host = origin_url.as_ref().and_then(|u| u.host_str()).unwrap_or(origin);
                self.permissions.decision_for_domain(host, &crate::permissions::Capability::TabInspect)
            };

            if decision != crate::permissions::PermissionDecision::Allow {
                tracing::warn!(target: "brazen::tabs", origin = %origin, "denying tab access due to permissions");
                return;
            }

            if url.path() == "/list" {
                let session = self.session.read().unwrap();
                let active_window_idx = session.active_window;
                let tabs = session.windows.get(active_window_idx)
                    .map(|w| &w.tabs)
                    .cloned()
                    .unwrap_or_default();
                
                if let Ok(data) = serde_json::to_vec(&tabs) {
                    let mut headers = HeaderMap::new();
                    headers.insert(http::header::CONTENT_TYPE, http::HeaderValue::from_static("application/json"));
                    self.send_intercepted_response(load, headers, data);
                }
            }
        }
    }

    fn send_intercepted_response(&self, load: WebResourceLoad, mut headers: HeaderMap, data: Vec<u8>) {
        // Check permissions for CORS
        let origin = load.request.headers.get("Origin")
            .and_then(|h| h.to_str().ok())
            .unwrap_or("null");
        
        let decision = if origin == "null" {
            crate::permissions::PermissionDecision::Allow
        } else {
            let origin_url = Url::parse(origin).ok();
            let host = origin_url.as_ref().and_then(|u| u.host_str()).unwrap_or(origin);
            self.permissions.decision_for_domain(host, &crate::permissions::Capability::VirtualResourceMount)
        };

        if decision == crate::permissions::PermissionDecision::Allow {
            headers.insert(http::header::ACCESS_CONTROL_ALLOW_ORIGIN, http::HeaderValue::from_str(origin).unwrap_or(http::HeaderValue::from_static("*")));
        } else {
            tracing::warn!(target: "brazen::mounts", origin = %origin, "denying virtual resource access due to permissions");
            return;
        }

        let mut response = WebResourceResponse::new(load.request.url.clone());
        response.headers = headers;
        let intercepted = load.intercept(response);
        intercepted.send_body_data(data);
        intercepted.finish();
    }
}

#[derive(Clone)]
pub struct BrazenServoDelegate {
    devtools_endpoint: Rc<RefCell<Option<String>>>,
    last_error: Rc<RefCell<Option<String>>>,
}

impl BrazenServoDelegate {
    pub fn new(
        devtools_endpoint: Rc<RefCell<Option<String>>>,
        last_error: Rc<RefCell<Option<String>>>,
    ) -> Self {
        Self {
            devtools_endpoint,
            last_error,
        }
    }
}

impl ServoDelegate for BrazenServoDelegate {
    fn notify_devtools_server_started(&self, port: u16, token: String) {
        let endpoint = format!("tcp://127.0.0.1:{port}?token={token}");
        *self.devtools_endpoint.borrow_mut() = Some(endpoint);
    }

    fn notify_error(&self, error: libservo::ServoError) {
        *self.last_error.borrow_mut() = Some(format!("{error:?}"));
    }
}

pub struct BrazenClipboardDelegate {
    event_sender: std::sync::mpsc::Sender<EngineEvent>,
    pending_request: Arc<std::sync::Mutex<Option<StringRequest>>>,
}

impl BrazenClipboardDelegate {
    pub fn new(
        event_sender: std::sync::mpsc::Sender<EngineEvent>,
        pending_request: Arc<std::sync::Mutex<Option<StringRequest>>>,
    ) -> Self {
        Self {
            event_sender,
            pending_request,
        }
    }
}

impl ClipboardDelegate for BrazenClipboardDelegate {
    fn get_text(&self, _webview: WebView, request: StringRequest) {
        let mut pending = self.pending_request.lock().unwrap();
        *pending = Some(request);
        let _ = self.event_sender.send(EngineEvent::ClipboardRequested(
            crate::engine::ClipboardRequest::Read,
        ));
    }

    fn set_text(&self, _webview: WebView, new_contents: String) {
        let _ = self.event_sender.send(EngineEvent::ClipboardRequested(
            crate::engine::ClipboardRequest::Write(new_contents),
        ));
    }
}

struct BrazenEventLoopWaker {
    frame_ready: Arc<AtomicBool>,
}

impl EventLoopWaker for BrazenEventLoopWaker {
    fn wake(&self) {
        self.frame_ready.store(true, Ordering::Release);
    }

    fn clone_box(&self) -> Box<dyn EventLoopWaker> {
        Box::new(BrazenEventLoopWaker {
            frame_ready: self.frame_ready.clone(),
        })
    }
}

pub struct ServoUpstreamRuntime {
    servo: Servo,
    webview: WebView,
    rendering_context: Rc<dyn RenderingContext>,
    snapshot: Rc<RefCell<UpstreamSnapshot>>,
    frame_ready: Arc<AtomicBool>,
    devtools_endpoint: Rc<RefCell<Option<String>>>,
    last_error: Rc<RefCell<Option<String>>>,
    pixel_format: std::cell::Cell<PixelFormat>,
    alpha_mode: AlphaMode,
    color_space: ColorSpace,
    pixel_probe: Option<PixelProbeState>,
    resources_dir: PathBuf,
    resource_source: ResourceDirSource,
    pending_clipboard_request: Arc<std::sync::Mutex<Option<StringRequest>>>,
}

impl std::fmt::Debug for ServoUpstreamRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServoUpstreamRuntime")
            .field("snapshot", &self.snapshot.borrow())
            .field("devtools_endpoint", &self.devtools_endpoint.borrow())
            .field("last_error", &self.last_error.borrow())
            .field("pixel_format", &self.pixel_format.get())
            .field("alpha_mode", &self.alpha_mode)
            .field("color_space", &self.color_space)
            .field("pixel_probe", &self.pixel_probe)
            .field("resources_dir", &self.resources_dir)
            .field("resource_source", &self.resource_source)
            .finish()
    }
}

impl ServoUpstreamRuntime {
    pub fn new(
        width: u32,
        height: u32,
        config: ServoUpstreamConfig,
        event_sender: std::sync::mpsc::Sender<EngineEvent>,
        mount_manager: MountManager,
        permissions: crate::permissions::PermissionPolicy,
        session: Arc<RwLock<SessionSnapshot>>,
    ) -> Result<Self, String> {
        let _ = LogTracer::init();
        let _ = rustls::crypto::ring::default_provider().install_default();
        let resolved_certificate_path =
            resolve_system_certificate_path(config.certificate_path.as_deref());
        let env_source = std::env::var(SERVO_SOURCE_ENV).ok();
        let ResourceDirResolution { path, source } = resolve_resource_dir(
            config.resources_dir.as_ref().and_then(|dir| dir.to_str()),
            env_source.as_deref(),
        )
        .map_err(|error| {
            tracing::error!(
                target: "brazen::servo::resources",
                %error,
                "failed to resolve servo resources directory"
            );
            format!("servo resources error: {error}")
        })?;
        println!("Servo resources resolved from: {} (source: {:?})", path.display(), source);
        tracing::info!(
            target: "brazen::servo::network",
            ignore_certificate_errors = config.ignore_certificate_errors,
            certificate_path = ?resolved_certificate_path,
            "servo network configuration"
        );
        libservo::resources::set(Box::new(ServoResourceReader::new(path.clone())));
        let opts = Opts {
            ignore_certificate_errors: config.ignore_certificate_errors,
            certificate_path: resolved_certificate_path
                .as_ref()
                .map(|path| path.display().to_string()),
            ..Opts::default()
        };
        let frame_ready = Arc::new(AtomicBool::new(true));
        let rendering_context = Rc::new(
            SoftwareRenderingContext::new(PhysicalSize::new(width, height))
                .map_err(|error| format!("rendering context error: {error:?}"))?,
        );
        let servo = ServoBuilder::default()
            .event_loop_waker(Box::new(BrazenEventLoopWaker {
                frame_ready: frame_ready.clone(),
            }))
            .opts(opts)
            .build();
        let snapshot = Rc::new(RefCell::new(UpstreamSnapshot::default()));
        let devtools_endpoint = Rc::new(RefCell::new(None));
        let last_error = Rc::new(RefCell::new(None));
        let delegate = Rc::new(BrazenWebViewDelegate::new(
            snapshot.clone(),
            frame_ready.clone(),
            mount_manager.clone(),
            permissions,
            session,
        ));
        let servo_delegate = Rc::new(BrazenServoDelegate::new(
            devtools_endpoint.clone(),
            last_error.clone(),
        ));
        servo.set_delegate(servo_delegate);

        let pending_clipboard_request = Arc::new(std::sync::Mutex::new(None));
        let clipboard_delegate = Rc::new(BrazenClipboardDelegate::new(
            event_sender.clone(),
            pending_clipboard_request.clone(),
        ));
        let webview = WebViewBuilder::new(&servo, rendering_context.clone())
            .delegate(delegate)
            .clipboard_delegate(clipboard_delegate)
            .build();
        webview.show();

        let pixel_probe = if config.enable_pixel_probe {
            match Url::parse("data:text/html,<style>body{margin:0;background:#ff0000}</style>") {
                Ok(url) => {
                    webview.load(url.clone());
                    Some(PixelProbeState {
                        pending: true,
                        test_url: url,
                    })
                }
                Err(error) => {
                    tracing::warn!(
                        target: "brazen::servo::probe",
                        %error,
                        "failed to build pixel probe url"
                    );
                    None
                }
            }
        } else {
            None
        };

        Ok(Self {
            servo,
            webview,
            rendering_context,
            snapshot,
            frame_ready,
            devtools_endpoint,
            last_error,
            pixel_format: std::cell::Cell::new(config.pixel_format),
            alpha_mode: config.alpha_mode,
            color_space: config.color_space,
            pixel_probe,
            resources_dir: path,
            resource_source: source,
            pending_clipboard_request,
        })
    }

    pub fn resources_dir(&self) -> &Path {
        &self.resources_dir
    }

    pub fn resource_source(&self) -> ResourceDirSource {
        self.resource_source
    }

    pub fn snapshot(&self) -> UpstreamSnapshot {
        self.snapshot.borrow().clone()
    }

    pub fn take_devtools_endpoint(&self) -> Option<String> {
        self.devtools_endpoint.borrow_mut().take()
    }

    pub fn last_error(&self) -> Option<String> {
        self.last_error.borrow().clone()
    }

    pub fn resize(&self, width: u32, height: u32) {
        self.webview.resize(PhysicalSize::new(width, height));
        self.rendering_context
            .resize(PhysicalSize::new(width, height));
        self.frame_ready.store(true, Ordering::Release);
    }

    pub fn navigate(&self, url: &str) -> Result<(), String> {
        let url = Url::parse(url).map_err(|error| format!("invalid url: {error}"))?;
        self.webview.load(url);
        Ok(())
    }

    pub fn reload(&self) {
        self.webview.reload();
    }

    pub fn stop(&self) {}

    pub fn spin(&self) {
        self.servo.spin_event_loop();
    }

    pub fn render_frame(&mut self) -> Option<UpstreamFrame> {
        if !self.frame_ready.swap(false, Ordering::AcqRel) {
            return None;
        }
        let size = self.rendering_context.size();
        if size.width == 0 || size.height == 0 {
            return None;
        }
        self.rendering_context.prepare_for_rendering();
        self.webview.paint();
        let rect = DeviceIntRect::from_origin_and_size(
            DeviceIntPoint::new(0, 0),
            DeviceIntSize::new(size.width as i32, size.height as i32),
        );
        tracing::trace!(
            target: "brazen::servo::render",
            width = size.width,
            height = size.height,
            "readback render surface"
        );
        let image = self.rendering_context.read_to_image(rect)?;
        self.rendering_context.present();
        if let Some(probe) = &mut self.pixel_probe
            && probe.pending
        {
            Self::apply_pixel_probe(&self.pixel_format, probe, &image);
        }
        let pixels = image.into_raw();
        let stride_bytes = size.width as usize * 4;
        let pixel_format = self.pixel_format.get();
        let alpha_mode = self.alpha_mode;
        let color_space = self.color_space;
        Some(UpstreamFrame {
            width: size.width,
            height: size.height,
            stride_bytes,
            pixel_format,
            alpha_mode,
            color_space,
            pixels,
        })
    }

    pub fn handle_input(&self, event: InputEvent) {
        self.webview.notify_input_event(event);
    }

    fn apply_pixel_probe(
        pixel_format: &std::cell::Cell<PixelFormat>,
        probe: &mut PixelProbeState,
        image: &libservo::RgbaImage,
    ) {
        let width = image.width();
        let height = image.height();
        if width == 0 || height == 0 {
            return;
        }
        let raw = image.as_raw();
        let idx = ((height / 2) * width + (width / 2)) as usize * 4;
        if idx + 3 >= raw.len() {
            return;
        }
        let r = raw[idx];
        let g = raw[idx + 1];
        let b = raw[idx + 2];
        let detected = if r > 200 && g < 50 && b < 50 {
            PixelFormat::Rgba8
        } else if b > 200 && r < 50 && g < 50 {
            PixelFormat::Bgra8
        } else {
            pixel_format.get()
        };
        pixel_format.set(detected);
        probe.pending = false;
        tracing::info!(
            target: "brazen::servo::probe",
            expected_url = %probe.test_url,
            sample = format!("{r},{g},{b}"),
            format = detected.as_str(),
            "pixel probe complete"
        );
    }

    pub fn handle_mouse_move(&self, x: f32, y: f32) {
        self.handle_input(InputEvent::MouseMove(MouseMoveEvent::new(
            WebViewPoint::Device(DevicePoint::new(x, y)),
        )));
    }

    pub fn handle_mouse_button(&self, button: u8, pressed: bool, x: f32, y: f32) {
        let action = if pressed {
            MouseButtonAction::Down
        } else {
            MouseButtonAction::Up
        };
        self.handle_input(InputEvent::MouseButton(MouseButtonEvent::new(
            action,
            MouseButton::from(button),
            WebViewPoint::Device(DevicePoint::new(x, y)),
        )));
    }

    pub fn handle_mouse_leave(&self) {
        self.handle_input(InputEvent::MouseLeftViewport(MouseLeftViewportEvent {
            focus_moving_to_another_iframe: false,
        }));
    }

    pub fn handle_wheel(&self, delta_x: f32, delta_y: f32, x: f32, y: f32) {
        let delta = WheelDelta {
            x: delta_x as f64,
            y: delta_y as f64,
            z: 0.0,
            mode: WheelMode::DeltaPixel,
        };
        self.handle_input(InputEvent::Wheel(WheelEvent::new(
            delta,
            WebViewPoint::Device(DevicePoint::new(x, y)),
        )));
    }

    pub fn handle_pinch_zoom(&self, magnification: f32, x: f32, y: f32) {
        self.webview
            .pinch_zoom(magnification, DevicePoint::new(x, y));
    }

    pub fn set_page_zoom(&self, zoom: f32) {
        self.webview.set_page_zoom(zoom);
    }

    pub fn page_zoom(&self) -> f32 {
        self.webview.page_zoom()
    }

    pub fn handle_keyboard(&self, key: &str, pressed: bool, modifiers: KeyModifiers, repeat: bool) {
        let state = if pressed {
            KeyState::Down
        } else {
            KeyState::Up
        };
        let key_value = if key.len() == 1 {
            Key::Character(key.to_string())
        } else {
            let named = match key {
                "Enter" => NamedKey::Enter,
                "Tab" => NamedKey::Tab,
                "Escape" => NamedKey::Escape,
                "Backspace" => NamedKey::Backspace,
                "Delete" => NamedKey::Delete,
                "ArrowLeft" => NamedKey::ArrowLeft,
                "ArrowRight" => NamedKey::ArrowRight,
                "ArrowUp" => NamedKey::ArrowUp,
                "ArrowDown" => NamedKey::ArrowDown,
                "Home" => NamedKey::Home,
                "End" => NamedKey::End,
                "PageUp" => NamedKey::PageUp,
                "PageDown" => NamedKey::PageDown,
                "Insert" => NamedKey::Insert,
                "CapsLock" => NamedKey::CapsLock,
                "Shift" => NamedKey::Shift,
                "Control" => NamedKey::Control,
                "Alt" => NamedKey::Alt,
                "Meta" => NamedKey::Meta,
                _ => NamedKey::Unidentified,
            };
            if key == "Space" {
                Key::Character(" ".to_string())
            } else {
                Key::Named(named)
            }
        };
        let mut servo_modifiers = Modifiers::empty();
        if modifiers.alt {
            servo_modifiers.insert(Modifiers::ALT);
        }
        if modifiers.ctrl {
            servo_modifiers.insert(Modifiers::CONTROL);
        }
        if modifiers.shift {
            servo_modifiers.insert(Modifiers::SHIFT);
        }
        if modifiers.command {
            servo_modifiers.insert(Modifiers::META);
        }
        let event = KeyboardEvent::new_without_event(
            state,
            key_value,
            Code::Unidentified,
            Location::Standard,
            servo_modifiers,
            repeat,
            false,
        );
        self.handle_input(InputEvent::Keyboard(event));
    }

    pub fn handle_ime_composition(&self, state: CompositionState, text: String) {
        self.handle_input(InputEvent::Ime(ImeEvent::Composition(CompositionEvent {
            state,
            data: text,
        })));
    }

    pub fn handle_ime_dismissed(&self) {
        self.handle_input(InputEvent::Ime(ImeEvent::Dismissed));
    }

    pub fn handle_clipboard(&self, request: &crate::engine::ClipboardRequest) {
        if let crate::engine::ClipboardRequest::Write(text) = request {
            let mut pending = self.pending_clipboard_request.lock().unwrap();
            if let Some(string_request) = pending.take() {
                string_request.success(text.clone());
            }
        }
    }

    pub fn evaluate_javascript(
        &mut self,
        script: String,
        callback: Box<dyn FnOnce(Result<serde_json::Value, String>) + Send + 'static>,
    ) {
        let webview_id = self.webview.id();
        self.servo.javascript_evaluator_mut().evaluate(
            webview_id,
            script,
            Box::new(move |result| match result {
                Ok(val) => callback(Ok(js_value_to_json(val))),
                Err(err) => callback(Err(format!("{:?}", err))),
            }),
        );
    }
}

fn resolve_system_certificate_path(configured: Option<&Path>) -> Option<PathBuf> {
    if let Some(path) = configured {
        return Some(path.to_path_buf());
    }
    let candidates = [
        "/etc/ssl/certs/ca-certificates.crt",
        "/etc/ssl/cert.pem",
        "/etc/pki/tls/certs/ca-bundle.crt",
        "/etc/ssl/ca-bundle.pem",
        "/etc/pki/ca-trust/extracted/pem/tls-ca-bundle.pem",
    ];
    candidates
        .iter()
        .map(Path::new)
        .find(|path| path.exists())
        .map(Path::to_path_buf)
}

fn js_value_to_json(val: libservo::JSValue) -> serde_json::Value {
    match val {
        libservo::JSValue::Undefined => serde_json::Value::Null,
        libservo::JSValue::Null => serde_json::Value::Null,
        libservo::JSValue::Boolean(b) => serde_json::Value::Bool(b),
        libservo::JSValue::Number(n) => serde_json::Number::from_f64(n)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        libservo::JSValue::String(s) => serde_json::Value::String(s),
        libservo::JSValue::Element(s) => serde_json::Value::String(s),
        libservo::JSValue::ShadowRoot(s) => serde_json::Value::String(s),
        libservo::JSValue::Frame(s) => serde_json::Value::String(s),
        libservo::JSValue::Window(s) => serde_json::Value::String(s),
        libservo::JSValue::Array(a) => {
            serde_json::Value::Array(a.into_iter().map(js_value_to_json).collect())
        }
        libservo::JSValue::Object(o) => {
            let mut map = serde_json::Map::new();
            for (k, v) in o {
                map.insert(k, js_value_to_json(v));
            }
            serde_json::Value::Object(map)
        }
    }
}
