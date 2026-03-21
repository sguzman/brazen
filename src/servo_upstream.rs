#![cfg(feature = "servo-upstream")]

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::engine::KeyModifiers;
use dpi::PhysicalSize;
use libservo::{
    Code, CompositionEvent, DeviceIntPoint, DeviceIntRect, DeviceIntSize, EventLoopWaker, ImeEvent,
    InputEvent, Key, KeyState, KeyboardEvent, LoadStatus, Location, Modifiers, MouseButton,
    MouseButtonAction, MouseButtonEvent, MouseMoveEvent, RenderingContext, Servo, ServoBuilder,
    ServoDelegate, SoftwareRenderingContext, WebView, WebViewBuilder, WebViewDelegate,
    WebViewPoint, WheelDelta, WheelEvent, WheelMode,
};
use tracing_log::LogTracer;
use url::Url;

#[derive(Debug, Clone)]
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
    frame_ready: Rc<Cell<bool>>,
}

impl BrazenWebViewDelegate {
    pub fn new(snapshot: Rc<RefCell<UpstreamSnapshot>>, frame_ready: Rc<Cell<bool>>) -> Self {
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
        let favicon = webview.favicon();
        let favicon_url = favicon
            .and_then(|image| image.url.map(|url| url.to_string()))
            .or_else(|| {
                if self.snapshot.borrow().url.starts_with("http") {
                    Some(format!("{}/favicon.ico", self.snapshot.borrow().url))
                } else {
                    None
                }
            });
        self.snapshot.borrow_mut().favicon_url = favicon_url;
    }

    fn notify_load_status_changed(&self, _webview: WebView, status: LoadStatus) {
        self.snapshot.borrow_mut().load_status = status;
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
        self.frame_ready.set(true);
    }

    fn notify_crashed(&self, _webview: WebView, reason: String, _backtrace: Option<String>) {
        self.snapshot.borrow_mut().last_error = Some(reason);
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
    frame_ready: Rc<Cell<bool>>,
}

impl EventLoopWaker for BrazenEventLoopWaker {
    fn wake(&self) {
        self.frame_ready.set(true);
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
    frame_ready: Rc<Cell<bool>>,
    devtools_endpoint: Rc<RefCell<Option<String>>>,
    last_error: Rc<RefCell<Option<String>>>,
}

impl ServoUpstreamRuntime {
    pub fn new(width: u32, height: u32) -> Result<Self, String> {
        let _ = LogTracer::init();
        let rendering_context = Rc::new(
            SoftwareRenderingContext::new(PhysicalSize::new(width, height))
                .map_err(|error| format!("rendering context error: {error:?}"))?,
        );
        let servo = ServoBuilder::default()
            .event_loop_waker(Box::new(BrazenEventLoopWaker {
                frame_ready: frame_ready.clone(),
            }))
            .build();
        let snapshot = Rc::new(RefCell::new(UpstreamSnapshot::default()));
        let frame_ready = Rc::new(Cell::new(false));
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
        Ok(Self {
            servo,
            webview,
            rendering_context,
            snapshot,
            frame_ready,
            devtools_endpoint,
            last_error,
        })
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
        self.rendering_context
            .resize(PhysicalSize::new(width, height));
        self.frame_ready.set(true);
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

    pub fn render_frame(&self) -> Option<Vec<u8>> {
        if !self.frame_ready.replace(false) {
            return None;
        }
        self.webview.paint();
        self.rendering_context.present();
        let size = self.rendering_context.size();
        let rect = DeviceIntRect::new(
            DeviceIntPoint::new(0, 0),
            DeviceIntSize::new(size.width as i32, size.height as i32),
        );
        let image = self.rendering_context.read_to_image(rect)?;
        Some(image.into_raw())
    }

    pub fn handle_input(&self, event: InputEvent) {
        self.webview.notify_input_event(event);
    }

    pub fn handle_mouse_move(&self, x: f32, y: f32) {
        self.handle_input(InputEvent::MouseMove(MouseMoveEvent::new(
            WebViewPoint::new(x as f64, y as f64),
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
            WebViewPoint::new(x as f64, y as f64),
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
            WebViewPoint::new(x as f64, y as f64),
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
            Key::Unidentified
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
            data: Some(text),
            ..Default::default()
        })));
    }
}
