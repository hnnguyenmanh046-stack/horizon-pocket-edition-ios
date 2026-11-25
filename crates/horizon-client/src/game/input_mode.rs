//! What the player plays with (horizon_client::input_mode): the touch controls, the chat's bar and
//! the on-screen keyboard show while it is the touch screen. On Android the mouse's events come
//! here from the input hook (android.rs) and go on as the window events a computer's mouse
//! makes; a physical keyboard's keys and a controller (game/pad.rs) also count as the input used.
use super::Game;
#[cfg(target_os = "android")]
use super::{event_loop::EventLoopCtl, input_trace::InputOrigin};
use horizon_client::input_mode::InputMode;
#[cfg(target_os = "ios")]
use horizon_client::hud::{HudAtlas, HudBatch};
#[cfg(target_os = "android")]
use winit::{
    dpi::PhysicalPosition,
    event::{DeviceId, ElementState, MouseScrollDelta, TouchPhase, WindowEvent},
};

impl Game<'_> {
    /// The player used `input`. Off the touch screen the fingers let go of what they held and the
    /// on-screen keyboard goes (touch_frame); any input but a controller takes the menus back
    /// from the pad's pointer.
    pub(super) fn input_used(&mut self, input: InputMode) {
        let mode = self
            .settings
            .touch
            .input_selection
            .resolve(self.input_mode.after(input, horizon_client::PHONE));
        if mode == self.input_mode {
            return;
        }
        if self.input_mode == InputMode::Touch {
            self.touch_release_all();
        }
        if mode != InputMode::Pad {
            self.pad.in_use = false;
        }
        self.input_mode = mode;
    }

    /// iOS: a hardware keyboard's keys (the on-screen keyboard not asked for) and a mouse's or a
    /// trackpad's pointer are the keyboard and mouse, as Android's input hook tells them apart.
    #[cfg(target_os = "ios")]
    pub(super) fn ios_input(&mut self, event: &winit::event::WindowEvent) {
        use winit::event::WindowEvent;
        let keyboard_mouse = match event {
            WindowEvent::KeyboardInput { .. } => {
                !self.touch_keyboard_asked() && horizon_gamecontroller::keyboard_connected()
            }
            // Only a pointer that is there: nothing else makes these on a phone.
            WindowEvent::CursorMoved { .. }
            | WindowEvent::MouseInput { .. }
            | WindowEvent::MouseWheel { .. } => horizon_gamecontroller::mouse_connected(),
            _ => false,
        };
        if keyboard_mouse {
            self.input_used(InputMode::KeyboardMouse);
        }
    }

    /// The touch controls, the chat's bar and the on-screen keyboard show.
    pub(super) fn touch_shown(&self) -> bool {
        self.input_mode == InputMode::Touch
    }

    /// Once per loop iteration, before the touch frame: the pointer capture Android took back,
    /// a physical keyboard's keys, and the mouse's events since the last iteration, as window
    /// events (its captured motion turns the view as a computer's raw motion does).
    #[cfg(target_os = "android")]
    pub(super) fn android_input_frame(&mut self, event_loop: &dyn EventLoopCtl) {
        if crate::android::pointer_capture_lost() && self.captured {
            self.capture(false);
        }
        if crate::android::keyboard_used() {
            self.input_used(InputMode::KeyboardMouse);
        }
        let events = crate::android::take_mouse_events();
        if events.is_empty() {
            return;
        }
        self.input_used(InputMode::KeyboardMouse);
        let Some(id) = self.window.as_ref().map(|w| w.id()) else {
            return;
        };
        let device_id = DeviceId::dummy();
        for event in events {
            let event = match event {
                crate::android::MouseEvent::Moved(x, y) => WindowEvent::CursorMoved {
                    device_id,
                    position: PhysicalPosition::new(x, y),
                },
                crate::android::MouseEvent::Button(button, down) => WindowEvent::MouseInput {
                    device_id,
                    state: if down {
                        ElementState::Pressed
                    } else {
                        ElementState::Released
                    },
                    button,
                },
                crate::android::MouseEvent::Wheel(x, y) => WindowEvent::MouseWheel {
                    device_id,
                    delta: MouseScrollDelta::LineDelta(x, y),
                    phase: TouchPhase::Moved,
                },
                crate::android::MouseEvent::Motion(x, y) => {
                    self.grabbed_mouse_motion(x, y);
                    continue;
                }
            };
            self.handle_window_event(event_loop, id, event, InputOrigin::User);
        }
    }
}

#[cfg(target_os = "ios")]
impl Game<'_> {
    /// A hardware mouse on an iPhone: the system draws no pointer there, so a menu would have no
    /// visible spot to click. The game draws the arrow itself.
    pub(super) fn draw_hardware_cursor(&self, batch: &mut HudBatch) {
        use winit::platform::ios::WindowExtIOS;

        if self.captured
            || self.input_mode != InputMode::KeyboardMouse
            || !horizon_gamecontroller::mouse_connected()
        {
            return;
        }
        let Some(window) = &self.window else { return };
        if window.has_system_pointer() {
            return;
        }
        draw_mouse_arrow(batch, &self.gui.atlas, self.cursor);
    }
}

/// Hynis's cursor: a rounded triangle, black with a white edge, its top corner at `at`.
#[cfg(target_os = "ios")]
fn draw_mouse_arrow(batch: &mut HudBatch, atlas: &HudAtlas, at: (f32, f32)) {
    let tri = [[0.0f32, 0.0], [0.0, 15.5], [12.0, 9.0]];
    // Each corner cut in two points rounds the shape.
    let mut round: Vec<[f32; 2]> = Vec::new();
    for i in 0..3 {
        let (prev, p, next) = (tri[(i + 2) % 3], tri[i], tri[(i + 1) % 3]);
        for q in [prev, next] {
            let (dx, dy) = (q[0] - p[0], q[1] - p[1]);
            let length = (dx * dx + dy * dy).sqrt();
            let cut = 2.6 / length;
            round.push([p[0] + dx * cut, p[1] + dy * cut]);
        }
    }
    for (grow, color) in [(1.5, u32::MAX), (0.0, 0xFF000000)] {
        fill_poly(batch, atlas, at, &round, grow, color);
    }
}

/// One convex shape filled, its corners `grow` pixels further from their centre.
#[cfg(target_os = "ios")]
fn fill_poly(
    batch: &mut HudBatch,
    atlas: &HudAtlas,
    at: (f32, f32),
    shape: &[[f32; 2]],
    grow: f32,
    color: u32,
) {
    let n = shape.len() as f32;
    let c = shape
        .iter()
        .fold([0.0, 0.0], |c, p| [c[0] + p[0] / n, c[1] + p[1] / n]);
    let grown: Vec<[f32; 2]> = shape
        .iter()
        .map(|p| {
            let (dx, dy) = (p[0] - c[0], p[1] - c[1]);
            let length = (dx * dx + dy * dy).sqrt();
            let k = if length > 0.0 { 1.0 + grow / length } else { 1.0 };
            [at.0 + c[0] + dx * k, at.1 + c[1] + dy * k]
        })
        .collect();
    for i in 1..grown.len() - 1 {
        batch.fill_quad(atlas, [grown[0], grown[i], grown[i + 1], grown[i + 1]], color);
    }
}

/// Other platforms have a system pointer (or none to draw): nothing to do. The call in
/// `Game::hud` has no `cfg` of its own, which a native mod that hooks `hud` cannot resolve.
#[cfg(not(target_os = "ios"))]
impl Game<'_> {
    pub(super) fn draw_hardware_cursor(&self, _batch: &mut horizon_client::hud::HudBatch) {}
}
