//! Relative mouse input from Apple's public GameController API (iPadOS 14+).
//! Handlers run on GCDevice's default main queue, the same thread as UIKit/winit.
use super::{
    app_state::{self, EventWrapper},
    DEVICE_ID,
};
use crate::{
    dpi::PhysicalPosition,
    event::{
        DeviceEvent, ElementState, Event, MouseButton, MouseScrollDelta, TouchPhase, WindowEvent,
    },
    window::WindowId,
};
use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{class, msg_send, sel};
use objc2_foundation::{MainThreadMarker, NSArray};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::atomic::{AtomicBool, Ordering},
};

/// Horizon PE: a mouse is attached to a device that shows no pointer of its own (an iPhone), and
/// is not captured: the app draws the pointer where `CursorMoved` says.
static DRAWN_POINTER: AtomicBool = AtomicBool::new(false);

pub(crate) fn drawn_pointer() -> bool {
    DRAWN_POINTER.load(Ordering::Relaxed)
}

/// Horizon PE: whether a GameController mouse is attached.
pub(super) fn attached() -> bool {
    // SAFETY: a read-only GCMouse class property, on the main thread.
    let mice: Retained<NSArray<AnyObject>> = unsafe { objc2::msg_send_id![class!(GCMouse), mice] };
    mice.iter().next().is_some()
}

#[link(name = "GameController", kind = "framework")]
extern "C" {}

#[derive(Default)]
pub(super) struct Mouse {
    captured: Rc<Cell<bool>>,
    mice: RefCell<Vec<Retained<AnyObject>>>,
    /// Horizon PE: the device has no pointer of its own and no pointer lock (an iPhone). The
    /// mouse's motion then moves a pointer kept here, `at` in the view's pixels, and its buttons
    /// are window events whether captured or not.
    own_pointer: Rc<Cell<bool>>,
    at: Rc<Cell<(f64, f64)>>,
    /// The view's size in pixels, and pixels per point.
    bounds: Rc<Cell<(f64, f64)>>,
    scale: Rc<Cell<f64>>,
    placed: Cell<bool>,
}

fn device(event: DeviceEvent) {
    app_state::handle_nonuser_event(MainThreadMarker::new().unwrap(),
        EventWrapper::StaticEvent(Event::DeviceEvent { device_id: DEVICE_ID, event }));
}

fn send(window_id: WindowId, event: WindowEvent) {
    app_state::handle_nonuser_event(
        MainThreadMarker::new().unwrap(),
        EventWrapper::StaticEvent(Event::WindowEvent { window_id, event }),
    );
}

impl Mouse {
    pub(super) fn captured(&self) -> bool { self.captured.get() }
    pub(super) fn capture(&self, captured: bool) {
        // Horizon PE: the app takes its pointer to be in the middle whenever the grab changes.
        if self.captured.replace(captured) != captured {
            let (w, h) = self.bounds.get();
            self.at.set((w / 2.0, h / 2.0));
        }
    }

    /// `own_pointer`: the device shows no pointer (a phone). `bounds`: the view's size in pixels.
    pub(super) fn update(
        &self,
        window: WindowId,
        own_pointer: bool,
        bounds: (f64, f64),
        scale: f64,
    ) {
        self.own_pointer.set(own_pointer);
        self.bounds.set(bounds);
        self.scale.set(scale);
        if !self.placed.get() && bounds.0 > 0.0 {
            self.placed.set(true);
            self.at.set((bounds.0 / 2.0, bounds.1 / 2.0));
        }
        DRAWN_POINTER.store(
            own_pointer && !self.captured.get() && attached(),
            Ordering::Relaxed,
        );
        // SAFETY: documented GCMouse/GCDevice/GCController element properties. The framework copies blocks.
        // This function and the default GCDevice handler queue are main-thread only; captured is Rc<Cell>.
        unsafe {
            let mice: Retained<NSArray<AnyObject>> = objc2::msg_send_id![class!(GCMouse), mice];
            let mut known = self.mice.borrow_mut();
            if known
                .iter()
                .any(|old| !mice.iter().any(|m| std::ptr::eq(&*m, &**old)))
            {
                // A disconnected device may never deliver button-up.
                for button in [MouseButton::Left, MouseButton::Right, MouseButton::Middle] {
                    send(
                        window,
                        WindowEvent::MouseInput {
                            device_id: DEVICE_ID,
                            state: ElementState::Released,
                            button,
                        },
                    );
                }
            }
            for mouse in mice.iter() {
                if known.iter().any(|old| std::ptr::eq(&**old, &*mouse)) {
                    continue;
                }
                let input: Retained<AnyObject> = objc2::msg_send_id![&*mouse, mouseInput];
                let (own, captured, at, bounds, scale) = (
                    self.own_pointer.clone(),
                    self.captured.clone(),
                    self.at.clone(),
                    self.bounds.clone(),
                    self.scale.clone(),
                );
                let moved = RcBlock::new(move |_: &AnyObject, dx: f32, dy: f32| {
                    device(DeviceEvent::MouseMotion { delta: (dx as f64, -dy as f64) });
                    // Horizon PE: no pointer of the system to hover with, so this is the pointer.
                    if own.get() && !captured.get() {
                        let (w, h) = bounds.get();
                        let s = scale.get().max(1.0);
                        let (x, y) = at.get();
                        let p = (
                            (x + dx as f64 * s).clamp(0.0, (w - 1.0).max(0.0)),
                            (y - dy as f64 * s).clamp(0.0, (h - 1.0).max(0.0)),
                        );
                        at.set(p);
                        send(
                            window,
                            WindowEvent::CursorMoved {
                                device_id: DEVICE_ID,
                                position: PhysicalPosition::new(p.0, p.1),
                            },
                        );
                    }
                });
                let _: () = msg_send![&*input, setMouseMovedHandler: &*moved];
                for (selector, button) in [
                    (sel!(leftButton), MouseButton::Left),
                    (sel!(rightButton), MouseButton::Right),
                    (sel!(middleButton), MouseButton::Middle),
                ] {
                    let element: Option<Retained<AnyObject>> =
                        objc2::msg_send_id![&*input, performSelector: selector];
                    if let Some(element) = element {
                        let captured = self.captured.clone();
                        let own = self.own_pointer.clone();
                        let changed = RcBlock::new(
                            move |_: &AnyObject, _: f32, down: objc2::runtime::Bool| {
                                device(DeviceEvent::Button { button: match button { MouseButton::Left => 0, MouseButton::Right => 1, _ => 2 },
                                    state: if down.as_bool() { ElementState::Pressed } else { ElementState::Released } });
                                if captured.get() || own.get() {
                                    send(
                                        window,
                                        WindowEvent::MouseInput {
                                            device_id: DEVICE_ID,
                                            state: if down.as_bool() {
                                                ElementState::Pressed
                                            } else {
                                                ElementState::Released
                                            },
                                            button,
                                        },
                                    );
                                }
                            },
                        );
                        let _: () = msg_send![&*element, setPressedChangedHandler: &*changed];
                    }
                }
                let scroll: Retained<AnyObject> = objc2::msg_send_id![&*input, scroll];
                let scrolled = RcBlock::new(move |_: &AnyObject, x: f32, y: f32| {
                    device(DeviceEvent::MouseWheel { delta: MouseScrollDelta::LineDelta(x, y) });
                    send(window, WindowEvent::MouseWheel { device_id: DEVICE_ID,
                        delta: MouseScrollDelta::LineDelta(x, y), phase: TouchPhase::Moved });
                });
                let _: () = msg_send![&*scroll, setValueChangedHandler: &*scrolled];
            }
            *known = mice
                .iter()
                .map(|m| Retained::retain(m as *const AnyObject as *mut AnyObject).unwrap())
                .collect();
        }
    }
}
