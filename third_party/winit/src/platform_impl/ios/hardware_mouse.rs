//! Relative mouse input from Apple's public GameController API (iPadOS 14+, iOS 14+).
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
    sync::{
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
        OnceLock,
    },
    time::Instant,
};

#[link(name = "GameController", kind = "framework")]
extern "C" {}

static START: OnceLock<Instant> = OnceLock::new();
/// When (in ms since start, plus one: zero is never) the mouse last did anything, a scroll of
/// its wheel came by GameController, and one came by UIKit's scroll gesture.
static LAST_ACTIVE: AtomicU64 = AtomicU64::new(0);
static LAST_WHEEL: AtomicU64 = AtomicU64::new(0);
static LAST_SCROLL_GESTURE: AtomicU64 = AtomicU64::new(0);
static LAST_HOVER: AtomicU64 = AtomicU64::new(0);

/// UIKit's hover gesture placed the cursor (where iOS's own pointer is): the mouse's relative
/// motion is then left out, so the two do not fight over where the cursor is.
pub(super) fn note_hover() {
    LAST_HOVER.store(now_ms(), Ordering::Relaxed);
}

static TRACED: AtomicU32 = AtomicU32::new(0);

/// A line in the game's stderr log (Horizon PE/logs), the first hundred or so only: what the mouse
/// and iOS did, for finding why a click comes twice or a wheel does nothing.
pub(super) fn trace(args: std::fmt::Arguments<'_>) {
    if TRACED.fetch_add(1, Ordering::Relaxed) < 150 {
        eprintln!("[winit-mouse] {args}");
    }
}

/// A GCMouse is connected now (kept by `Mouse::update`).
static MICE_PRESENT: AtomicBool = AtomicBool::new(false);

pub(super) fn gc_mouse_present() -> bool {
    MICE_PRESENT.load(Ordering::Relaxed)
}

fn now_ms() -> u64 {
    START.get_or_init(Instant::now).elapsed().as_millis() as u64 + 1
}

fn within(stamp: &AtomicU64, ms: u64) -> bool {
    let at = stamp.load(Ordering::Relaxed);
    at != 0 && now_ms().saturating_sub(at) < ms
}

/// A mouse moved, clicked or scrolled lately. With AssistiveTouch on, iOS turns its clicks into
/// touches too, which would click a second time: the view leaves those out while this is true.
pub(super) fn mouse_recently_used() -> bool {
    // As long as a mouse is connected, not only for a while after it moved: a click that came
    // after a pause (the first touch of it ahead of the mouse's own report) was let through, and
    // counted twice.
    gc_mouse_present() || within(&LAST_ACTIVE, 15_000)
}

/// A scroll from UIKit's gesture counts unless the wheel already came by GameController, so that
/// whichever of the two the system delivers is heard once.
pub(super) fn scroll_gesture_allowed() -> bool {
    if within(&LAST_WHEEL, 150) {
        return false;
    }
    let now = now_ms();
    LAST_SCROLL_GESTURE.store(now, Ordering::Relaxed);
    LAST_ACTIVE.store(now, Ordering::Relaxed);
    true
}

#[derive(Default)]
pub(super) struct Mouse {
    captured: Rc<Cell<bool>>,
    mice: RefCell<Vec<Retained<AnyObject>>>,
    /// Where the pointer is, in physical pixels, when the phone has no system cursor to say it.
    position: Rc<Cell<(f64, f64)>>,
    size: Rc<Cell<(f64, f64)>>,
    window: Cell<Option<WindowId>>,
    relative: Cell<bool>,
    /// Which of the left, right and middle buttons the game was last told are down.
    down: Rc<Cell<[bool; 3]>>,
}

fn device(event: DeviceEvent) {
    app_state::handle_nonuser_event(MainThreadMarker::new().unwrap(),
        EventWrapper::StaticEvent(Event::DeviceEvent { device_id: DEVICE_ID, event }));
}

fn send(window: WindowId, event: WindowEvent) {
    app_state::handle_nonuser_event(
        MainThreadMarker::new().unwrap(),
        EventWrapper::StaticEvent(Event::WindowEvent { window_id: window, event }),
    );
}

impl Mouse {
    pub(super) fn captured(&self) -> bool { self.captured.get() }

    /// Once a frame: each button as the device says it is now, against what the game was told. A
    /// hold whose change events were missed (or a release that never was) is put right here: held
    /// down for a while, a button used to stop counting.
    pub(super) fn poll(&self) {
        let Some(window) = self.window.get() else { return };
        if !(self.captured.get() || self.relative.get()) {
            return;
        }
        let mut held = [false; 3];
        {
            // Not held while the events go out: the game may ask for `update` meanwhile.
            let mice = self.mice.borrow();
            if mice.is_empty() {
                return;
            }
            // SAFETY: documented GCMouse/GCMouseInput/GCControllerButtonInput properties, on the
            // main thread.
            unsafe {
                for mouse in mice.iter() {
                let Some(input): Option<Retained<AnyObject>> =
                    objc2::msg_send_id![&**mouse, mouseInput]
                else {
                    continue;
                };
                for (i, selector) in
                    [sel!(leftButton), sel!(rightButton), sel!(middleButton)].into_iter().enumerate()
                {
                    let element: Option<Retained<AnyObject>> =
                        objc2::msg_send_id![&*input, performSelector: selector];
                    if let Some(element) = element {
                        let pressed: bool = msg_send![&*element, isPressed];
                        held[i] |= pressed;
                    }
                }
                }
            }
        }
        let told = self.down.get();
        for (i, button) in [MouseButton::Left, MouseButton::Right, MouseButton::Middle]
            .into_iter()
            .enumerate()
        {
            if held[i] != told[i] {
                trace(format_args!("{button:?} was {} by the game, the mouse says {}", told[i], held[i]));
                let state = if held[i] { ElementState::Pressed } else { ElementState::Released };
                device(DeviceEvent::Button { button: i as u32, state });
                send(window, WindowEvent::MouseInput { device_id: DEVICE_ID, state, button });
            }
        }
        self.down.set(held);
    }
    pub(super) fn capture(&self, captured: bool) {
        self.captured.set(captured);
        if !captured {
            self.centre();
            self.say();
        }
    }

    /// A grab warped the pointer to the window's centre; releasing it starts there, as the
    /// game's own cursor does (MouseHandler.releaseMouse). A turn does too.
    fn centre(&self) {
        let (width, height) = self.size.get();
        self.position.set((width / 2.0, height / 2.0));
    }

    /// Tells the game where the pointer a phone has no system cursor for now is.
    fn say(&self) {
        if self.captured.get() || !self.relative.get() {
            return;
        }
        let Some(window) = self.window.get() else { return };
        let (x, y) = self.position.get();
        send(window, WindowEvent::CursorMoved { device_id: DEVICE_ID, position: PhysicalPosition::new(x, y) });
    }

    /// `size` is the window's physical size and `scale` the screen's: a phone has no system
    /// cursor, so relative motion is turned into the cursor position the game's screens read.
    pub(super) fn update(&self, window: WindowId, size: (f64, f64), scale: f64, relative: bool) {
        self.window.set(Some(window));
        self.relative.set(relative);
        if self.size.replace(size) != size {
            self.centre();
            self.say();
        }
        // SAFETY: documented GCMouse/GCDevice/GCController element properties. The framework copies blocks.
        // This function and the default GCDevice handler queue are main-thread only; captured, size
        // and position are Rc<Cell>.
        unsafe {
            let mice: Retained<NSArray<AnyObject>> = objc2::msg_send_id![class!(GCMouse), mice];
            let mut known = self.mice.borrow_mut();
            let went_away = known
                .iter()
                .any(|old| !mice.iter().any(|m| std::ptr::eq(&*m, &**old)));
            if went_away {
                trace(format_args!("a mouse went away ({} left)", mice.len()));
            }
            // Only when the last one is gone: iOS may list a mouse again as a new device while a
            // button is held, which is not the button coming up (the new device's own state is
            // read below).
            if went_away && mice.is_empty() {
                self.down.set([false; 3]);
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
                self.say();
                let input: Retained<AnyObject> = objc2::msg_send_id![&*mouse, mouseInput];
                let captured = self.captured.clone();
                let position = self.position.clone();
                let size = self.size.clone();
                let moved = RcBlock::new(move |_: &AnyObject, dx: f32, dy: f32| {
                    LAST_ACTIVE.store(now_ms(), Ordering::Relaxed);
                    device(DeviceEvent::MouseMotion { delta: (dx as f64, -dy as f64) });
                    if captured.get() || !relative || within(&LAST_HOVER, 1500) {
                        return;
                    }
                    let (width, height) = size.get();
                    let (x, y) = position.get();
                    let (x, y) = (
                        (x + f64::from(dx) * scale).clamp(0.0, width.max(0.0)),
                        (y - f64::from(dy) * scale).clamp(0.0, height.max(0.0)),
                    );
                    position.set((x, y));
                    send(
                        window,
                        WindowEvent::CursorMoved {
                            device_id: DEVICE_ID,
                            position: PhysicalPosition::new(x, y),
                        },
                    );
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
                        let told = self.down.clone();
                        let index = match button {
                            MouseButton::Left => 0,
                            MouseButton::Right => 1,
                            _ => 2,
                        };
                        let changed = RcBlock::new(
                            move |_: &AnyObject, _: f32, down: objc2::runtime::Bool| {
                                LAST_ACTIVE.store(now_ms(), Ordering::Relaxed);
                                let mut now = told.get();
                                now[index] = down.as_bool();
                                told.set(now);
                                trace(format_args!(
                                    "GameController {button:?} down={} captured={} relative={relative}",
                                    down.as_bool(),
                                    captured.get()
                                ));
                                device(DeviceEvent::Button { button: match button { MouseButton::Left => 0, MouseButton::Right => 1, _ => 2 },
                                    state: if down.as_bool() { ElementState::Pressed } else { ElementState::Released } });
                                // A UIKit indirect pointer (an iPad's trackpad) delivers the
                                // click itself unless the pointer is locked; a phone has no such
                                // pointer, so its clicks come from here either way.
                                if captured.get() || relative {
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
                        // The device's state now, so a button held across its being listed again
                        // keeps being held (and one let go meanwhile is let go).
                        let pressed: bool = msg_send![&*element, isPressed];
                        trace(format_args!("mouse {button:?} listed, pressed={pressed}"));
                        let mut now = self.down.get();
                        now[index] = pressed;
                        self.down.set(now);
                        let state = if pressed { ElementState::Pressed } else { ElementState::Released };
                        device(DeviceEvent::Button {
                            button: match button { MouseButton::Left => 0, MouseButton::Right => 1, _ => 2 },
                            state,
                        });
                        if self.captured.get() || relative {
                            send(
                                window,
                                WindowEvent::MouseInput { device_id: DEVICE_ID, state, button },
                            );
                        }
                    }
                }
                let scroll: Retained<AnyObject> = objc2::msg_send_id![&*input, scroll];
                let scrolled = RcBlock::new(move |_: &AnyObject, x: f32, y: f32| {
                    if within(&LAST_SCROLL_GESTURE, 150) {
                        return;
                    }
                    let now = now_ms();
                    LAST_WHEEL.store(now, Ordering::Relaxed);
                    LAST_ACTIVE.store(now, Ordering::Relaxed);
                    trace(format_args!("GameController wheel {x} {y}"));
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
            MICE_PRESENT.store(!known.is_empty(), Ordering::Relaxed);
        }
    }
}
