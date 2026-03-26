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
    Location, Modifiers, MouseButton, MouseButtonAction, MouseButtonEvent, MouseMoveEvent,
    NamedKey, Opts, RenderingContext, Servo, ServoBuilder, ServoDelegate, SoftwareRenderingContext,
    WebView, WebViewBuilder, WebViewDelegate, WebViewPoint, WheelDelta, WheelEvent, WheelMode,
};
use tracing_log::LogTracer;
use url::Url;

#[derive(Debug, Clone, PartialEq)]
pub struct UpstreamSnapshot {
    pub url: String,
    pub title: Option<String>,
    pub favicon_url: Option<String>,
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
}

impl BrazenWebViewDelegate {
    pub fn new(snapshot: Rc<RefCell<UpstreamSnapshot>>, frame_ready: Arc<AtomicBool>) -> Self {
        Self {
            snapshot,
            frame_ready,
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
    pub fn new(width: u32, height: u32, config: ServoUpstreamConfig) -> Result<Self, String> {
        let _ = LogTracer::init();
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
        tracing::info!(
            target: "brazen::servo::resources",
            path = %path.display(),
            source = ?source,
            "servo resources resolved"
        );
        tracing::info!(
            target: "brazen::servo::network",
            ignore_certificate_errors = config.ignore_certificate_errors,
            certificate_path = ?config.certificate_path,
            "servo network configuration"
        );
        libservo::resources::set(Box::new(ServoResourceReader::new(path.clone())));
        let mut opts = Opts::default();
        opts.ignore_certificate_errors = config.ignore_certificate_errors;
        if let Some(cert_path) = &config.certificate_path {
            opts.certificate_path = Some(cert_path.display().to_string());
        }
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
        ));
        let servo_delegate = Rc::new(BrazenServoDelegate::new(
            devtools_endpoint.clone(),
            last_error.clone(),
        ));
        servo.set_delegate(servo_delegate);
        let webview = WebViewBuilder::new(&servo, rendering_context.clone())
            .delegate(delegate)
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
        if let Some(probe) = &mut self.pixel_probe {
            if probe.pending {
                Self::apply_pixel_probe(&self.pixel_format, probe, &image);
            }
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

    pub fn handle_keyboard(&self, key: &str, pressed: bool, modifiers: KeyModifiers) {
        let state = if pressed {
            KeyState::Down
        } else {
            KeyState::Up
        };
        let key_value = if key.len() == 1 {
            Key::Character(key.to_string())
        } else {
            Key::Named(NamedKey::Unidentified)
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
            false,
            false,
        );
        self.handle_input(InputEvent::Keyboard(event));
    }

    pub fn handle_ime(&self, text: String) {
        self.handle_input(InputEvent::Ime(ImeEvent::Composition(CompositionEvent {
            state: CompositionState::Update,
            data: text,
        })));
    }
}
