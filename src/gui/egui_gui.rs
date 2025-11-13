use crate::control::*;
use crate::core::*;
use egui_glow::Painter;
use std::cell::RefCell;

#[cfg(not(target_arch = "wasm32"))]
use arboard::Clipboard as SystemClipboard;
#[cfg(all(target_arch = "wasm32", feature = "window"))]
use std::rc::Rc;
#[cfg(all(target_arch = "wasm32", feature = "window"))]
use wasm_bindgen_futures::{spawn_local, JsFuture};
#[cfg(all(target_arch = "wasm32", feature = "window"))]
use web_sys::Clipboard as WebClipboard;

#[doc(hidden)]
pub use egui;

///
/// Integration of [egui](https://crates.io/crates/egui), an immediate mode GUI.
///
pub struct GUI {
    painter: RefCell<Painter>,
    egui_context: egui::Context,
    output: RefCell<Option<egui::FullOutput>>,
    viewport: Viewport,
    modifiers: Modifiers,
    clipboard: ClipboardManager,
}

impl GUI {
    ///
    /// Creates a new GUI from a mid-level [Context].
    ///
    pub fn new(context: &Context) -> Self {
        use std::ops::Deref;
        Self::from_gl_context(context.deref().clone())
    }

    ///
    /// Creates a new GUI from a low-level graphics [Context](crate::context::Context).
    ///
    pub fn from_gl_context(context: std::sync::Arc<crate::context::Context>) -> Self {
        GUI {
            egui_context: egui::Context::default(),
            painter: RefCell::new(Painter::new(context, "", None, true).unwrap()),
            output: RefCell::new(None),
            viewport: Viewport::new_at_origo(1, 1),
            modifiers: Modifiers::default(),
            clipboard: ClipboardManager::new(),
        }
    }

    ///
    /// Get the egui context.
    ///
    pub fn context(&self) -> &egui::Context {
        &self.egui_context
    }

    ///
    /// Initialises a new frame of the GUI and handles events.
    /// Construct the GUI (Add panels, widgets etc.) using the [egui::Context] in the callback function.
    /// This function returns whether or not the GUI has changed, ie. if it consumes any events, and therefore needs to be rendered again.
    ///
    pub fn update(
        &mut self,
        events: &mut [Event],
        accumulated_time_in_ms: f64,
        viewport: Viewport,
        device_pixel_ratio: f32,
        callback: impl FnOnce(&egui::Context),
    ) -> bool {
        self.egui_context.set_pixels_per_point(device_pixel_ratio);
        self.viewport = viewport;
        let mut egui_events = Vec::new();
        if let Some(paste) = self.clipboard.take_pending_paste() {
            egui_events.push(egui::Event::Paste(paste));
        }
        for event in events.iter() {
            match event {
                Event::KeyPress {
                    kind,
                    modifiers,
                    handled,
                } => {
                    if !handled {
                        if is_paste_command(modifiers, kind) {
                            if let Some(text) = self.clipboard.request_paste() {
                                egui_events.push(egui::Event::Paste(text));
                            }
                        }
                        egui_events.push(egui::Event::Key {
                            key: kind.into(),
                            pressed: true,
                            modifiers: modifiers.into(),
                            repeat: false,
                            physical_key: None,
                        });
                    }
                }
                Event::KeyRelease {
                    kind,
                    modifiers,
                    handled,
                } => {
                    if !handled {
                        egui_events.push(egui::Event::Key {
                            key: kind.into(),
                            pressed: false,
                            modifiers: modifiers.into(),
                            repeat: false,
                            physical_key: None,
                        });
                    }
                }
                Event::MousePress {
                    button,
                    position,
                    modifiers,
                    handled,
                } => {
                    if !handled {
                        egui_events.push(egui::Event::PointerButton {
                            pos: egui::Pos2 {
                                x: position.x / device_pixel_ratio,
                                y: (viewport.height as f32 - position.y) / device_pixel_ratio,
                            },
                            button: button.into(),
                            pressed: true,
                            modifiers: modifiers.into(),
                        });
                    }
                }
                Event::MouseRelease {
                    button,
                    position,
                    modifiers,
                    handled,
                } => {
                    if !handled {
                        egui_events.push(egui::Event::PointerButton {
                            pos: egui::Pos2 {
                                x: position.x / device_pixel_ratio,
                                y: (viewport.height as f32 - position.y) / device_pixel_ratio,
                            },
                            button: button.into(),
                            pressed: false,
                            modifiers: modifiers.into(),
                        });
                    }
                }
                Event::MouseMotion {
                    position, handled, ..
                } => {
                    if !handled {
                        egui_events.push(egui::Event::PointerMoved(egui::Pos2 {
                            x: position.x / device_pixel_ratio,
                            y: (viewport.height as f32 - position.y) / device_pixel_ratio,
                        }));
                    }
                }
                Event::Text(text) => {
                    egui_events.push(egui::Event::Text(text.clone()));
                }
                Event::MouseLeave => {
                    egui_events.push(egui::Event::PointerGone);
                }
                Event::MouseWheel {
                    delta,
                    handled,
                    modifiers,
                    ..
                } => {
                    if !handled {
                        egui_events.push(egui::Event::MouseWheel {
                            delta: egui::Vec2::new(delta.0, delta.1),
                            unit: egui::MouseWheelUnit::Point,
                            modifiers: modifiers.into(),
                        });
                    }
                }
                Event::PinchGesture { delta, handled, .. } => {
                    if !handled {
                        egui_events.push(egui::Event::Zoom(delta.exp()));
                    }
                }
                _ => {}
            }
        }

        let egui_input = egui::RawInput {
            screen_rect: Some(egui::Rect {
                min: egui::Pos2 {
                    x: viewport.x as f32 / device_pixel_ratio,
                    y: viewport.y as f32 / device_pixel_ratio,
                },
                max: egui::Pos2 {
                    x: viewport.x as f32 / device_pixel_ratio
                        + viewport.width as f32 / device_pixel_ratio,
                    y: viewport.y as f32 / device_pixel_ratio
                        + viewport.height as f32 / device_pixel_ratio,
                },
            }),
            time: Some(accumulated_time_in_ms * 0.001),
            modifiers: (&self.modifiers).into(),
            events: egui_events,
            ..Default::default()
        };

        self.egui_context.begin_pass(egui_input);
        callback(&self.egui_context);
        let output = self.egui_context.end_pass();
        self.handle_platform_output(&output.platform_output);
        *self.output.borrow_mut() = Some(output);

        for event in events.iter_mut() {
            if let Event::ModifiersChange { modifiers } = event {
                self.modifiers = *modifiers;
            }
            if self.egui_context.wants_pointer_input() {
                match event {
                    Event::MousePress {
                        ref mut handled, ..
                    } => {
                        *handled = true;
                    }
                    Event::MouseRelease {
                        ref mut handled, ..
                    } => {
                        *handled = true;
                    }
                    Event::MouseWheel {
                        ref mut handled, ..
                    } => {
                        *handled = true;
                    }
                    Event::MouseMotion {
                        ref mut handled, ..
                    } => {
                        *handled = true;
                    }
                    Event::PinchGesture {
                        ref mut handled, ..
                    } => {
                        *handled = true;
                    }
                    Event::RotationGesture {
                        ref mut handled, ..
                    } => {
                        *handled = true;
                    }
                    _ => {}
                }
            }

            if self.egui_context.wants_keyboard_input() {
                match event {
                    Event::KeyRelease {
                        ref mut handled, ..
                    } => {
                        *handled = true;
                    }
                    Event::KeyPress {
                        ref mut handled, ..
                    } => {
                        *handled = true;
                    }
                    _ => {}
                }
            }
        }
        self.egui_context.wants_pointer_input() || self.egui_context.wants_keyboard_input()
    }

    ///
    /// Render the GUI defined in the [update](Self::update) function.
    /// Must be called in the callback given as input to a [RenderTarget], [ColorTarget] or [DepthTarget] write method.
    ///
    pub fn render(&self) -> Result<(), crate::CoreError> {
        let output = self
            .output
            .borrow_mut()
            .take()
            .expect("need to call GUI::update before GUI::render");
        let scale = self.egui_context.pixels_per_point();
        let clipped_meshes = self.egui_context.tessellate(output.shapes, scale);
        self.painter.borrow_mut().paint_and_update_textures(
            [self.viewport.width, self.viewport.height],
            scale,
            &clipped_meshes,
            &output.textures_delta,
        );
        #[cfg(not(target_arch = "wasm32"))]
        #[allow(unsafe_code)]
        unsafe {
            use glow::HasContext as _;
            self.painter.borrow().gl().disable(glow::FRAMEBUFFER_SRGB);
        }
        Ok(())
    }

    fn handle_platform_output(&mut self, platform_output: &egui::PlatformOutput) {
        if !platform_output.copied_text.is_empty() {
            self.clipboard.set_text(&platform_output.copied_text);
        }
    }
}

impl Drop for GUI {
    fn drop(&mut self) {
        self.painter.borrow_mut().destroy();
    }
}

impl From<&Key> for egui::Key {
    fn from(key: &Key) -> Self {
        use crate::control::Key::*;
        use egui::Key;
        match key {
            ArrowDown => Key::ArrowDown,
            ArrowLeft => Key::ArrowLeft,
            ArrowRight => Key::ArrowRight,
            ArrowUp => Key::ArrowUp,
            Escape => Key::Escape,
            Tab => Key::Tab,
            Backspace => Key::Backspace,
            Enter => Key::Enter,
            Space => Key::Space,
            Insert => Key::Insert,
            Delete => Key::Delete,
            Home => Key::Home,
            End => Key::End,
            PageUp => Key::PageUp,
            PageDown => Key::PageDown,
            Num0 => Key::Num0,
            Num1 => Key::Num1,
            Num2 => Key::Num2,
            Num3 => Key::Num3,
            Num4 => Key::Num4,
            Num5 => Key::Num5,
            Num6 => Key::Num6,
            Num7 => Key::Num7,
            Num8 => Key::Num8,
            Num9 => Key::Num9,
            A => Key::A,
            B => Key::B,
            C => Key::C,
            D => Key::D,
            E => Key::E,
            F => Key::F,
            G => Key::G,
            H => Key::H,
            I => Key::I,
            J => Key::J,
            K => Key::K,
            L => Key::L,
            M => Key::M,
            N => Key::N,
            O => Key::O,
            P => Key::P,
            Q => Key::Q,
            R => Key::R,
            S => Key::S,
            T => Key::T,
            U => Key::U,
            V => Key::V,
            W => Key::W,
            X => Key::X,
            Y => Key::Y,
            Z => Key::Z,
        }
    }
}

impl From<&Modifiers> for egui::Modifiers {
    fn from(modifiers: &Modifiers) -> Self {
        Self {
            alt: modifiers.alt,
            ctrl: modifiers.ctrl,
            shift: modifiers.shift,
            command: modifiers.command,
            mac_cmd: cfg!(target_os = "macos") && modifiers.command,
        }
    }
}

impl From<&MouseButton> for egui::PointerButton {
    fn from(button: &MouseButton) -> Self {
        match button {
            MouseButton::Left => egui::PointerButton::Primary,
            MouseButton::Right => egui::PointerButton::Secondary,
            MouseButton::Middle => egui::PointerButton::Middle,
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
struct ClipboardManager {
    clipboard: Option<SystemClipboard>,
}

#[cfg(not(target_arch = "wasm32"))]
impl ClipboardManager {
    fn new() -> Self {
        Self {
            clipboard: SystemClipboard::new().ok(),
        }
    }

    fn set_text(&mut self, text: &str) {
        if let Some(clipboard) = self.clipboard.as_mut() {
            if clipboard.set_text(text.to_owned()).is_ok() {
                return;
            }
        }
        self.clipboard = SystemClipboard::new().ok();
        if let Some(clipboard) = self.clipboard.as_mut() {
            let _ = clipboard.set_text(text.to_owned());
        }
    }

    fn request_paste(&mut self) -> Option<String> {
        self.get_text()
    }

    fn take_pending_paste(&mut self) -> Option<String> {
        None
    }

    fn get_text(&mut self) -> Option<String> {
        if let Some(clipboard) = self.clipboard.as_mut() {
            match clipboard.get_text() {
                Ok(text) => return Some(text),
                Err(_) => {
                    self.clipboard = SystemClipboard::new().ok();
                }
            }
        }
        None
    }
}

#[cfg(all(target_arch = "wasm32", feature = "window"))]
struct ClipboardManager {
    pending_paste: Rc<RefCell<Option<String>>>,
}

#[cfg(all(target_arch = "wasm32", feature = "window"))]
impl ClipboardManager {
    fn new() -> Self {
        Self {
            pending_paste: Rc::new(RefCell::new(None)),
        }
    }

    fn set_text(&mut self, text: &str) {
        if let Some(clipboard) = Self::clipboard() {
            if let Ok(promise) = clipboard.write_text(text) {
                spawn_local(async move {
                    let _ = JsFuture::from(promise).await;
                });
            }
        }
    }

    fn request_paste(&mut self) -> Option<String> {
        if let Some(clipboard) = Self::clipboard() {
            if let Ok(promise) = clipboard.read_text() {
                let pending = self.pending_paste.clone();
                spawn_local(async move {
                    if let Ok(value) = JsFuture::from(promise).await {
                        if let Some(text) = value.as_string() {
                            *pending.borrow_mut() = Some(text);
                        }
                    }
                });
            }
        }
        None
    }

    fn take_pending_paste(&mut self) -> Option<String> {
        self.pending_paste.borrow_mut().take()
    }

    fn clipboard() -> Option<WebClipboard> {
        web_sys::window()?.navigator().clipboard().ok()
    }
}

#[cfg(all(target_arch = "wasm32", not(feature = "window")))]
struct ClipboardManager;

#[cfg(all(target_arch = "wasm32", not(feature = "window")))]
impl ClipboardManager {
    fn new() -> Self {
        Self
    }

    fn set_text(&mut self, _text: &str) {}

    fn request_paste(&mut self) -> Option<String> {
        None
    }

    fn take_pending_paste(&mut self) -> Option<String> {
        None
    }
}

fn is_paste_command(modifiers: &Modifiers, key: &Key) -> bool {
    modifiers.command && matches!(key, Key::V)
}
