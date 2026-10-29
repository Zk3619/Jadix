//! Page/view navigation stack (separate from [`crate::app::portal_overlay`] overlay stack).
//!
//! - 每窗口 / 每 [`navigation_host`] 一个 [`Navigator`]
//! - typed `Route: Clone + Eq + Hash`
//! - 默认 keep-alive；root 路由 `bring_to_front` / 重复 `push` 会**截断栈**
//! - Esc: portal_overlay first, then [`try_pop_active`]
//! - Host uninstall: [`Element::on_unmount`](crate::core::tree::Element::on_unmount) Clear active registration (do not delete named signals to avoid accidental damage during the first frame rebuild)
//! - 显式释放：[`Navigator::dispose`]
//! - 转场：[`Transition`] + [`FrameState`]；由 [`tick_active`] 推进
//! - Stack → Element: [`navigation_host`]([`host`])
//!
//! z（Host 渲染）：`Host = 100 + i*10`，`Sheet = 500 + i*50`。

mod host;

pub use host::{navigation_host, NavigationHostBuilder, NavigationHostFrame};

use std::cell::RefCell;
use std::collections::HashSet;
use std::hash::Hash;
use std::rc::Rc;

use crate::core::reactivity::{remove_named_signal, use_signal, Signal};
use crate::core::tree::UnmountHook;

/// Host 内容区基线 z。
pub const VIEW_HOST_Z_BASE: i32 = 100;
pub const VIEW_HOST_Z_STEP: i32 = 10;
/// Sheet baseline z (always above Host, below portal_overlay's 1000).
pub const VIEW_SHEET_Z_BASE: i32 = 500;
pub const VIEW_SHEET_Z_STEP: i32 = 50;

pub fn host_z(index: usize) -> i32 {
    VIEW_HOST_Z_BASE + (index as i32) * VIEW_HOST_Z_STEP
}

pub fn sheet_z(index: usize) -> i32 {
    VIEW_SHEET_Z_BASE + (index as i32) * VIEW_SHEET_Z_STEP
}

pub fn navigator_signal_key(host_id: &str) -> String {
    format!("__jadix_navigator:{host_id}")
}

/// Page rendering method.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Presentation {
    #[default]
    Host,
    Sheet,
}

/// 转场状态。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum FrameState {
    Entering,
    #[default]
    Active,
    Exiting,
}

/// Transition type (v1: none / slide in and slide out).
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum Transition {
    #[default]
    None,
    /// 水平滑入/滑出；`duration_secs` 默认约 0.2s。
    Slide { duration_secs: f32 },
}

impl Transition {
    pub fn slide_default() -> Self {
        Self::Slide { duration_secs: 0.2 }
    }

    pub fn duration_secs(self) -> f32 {
        match self {
            Self::None => 0.0,
            Self::Slide { duration_secs } => duration_secs.max(0.01),
        }
    }

    pub fn is_animated(self) -> bool {
        !matches!(self, Self::None)
    }
}

#[derive(Clone, Debug)]
pub struct PushOptions {
    pub presentation: Presentation,
    pub keep_alive: bool,
    pub replace: bool,
    pub transition: Transition,
}

impl Default for PushOptions {
    fn default() -> Self {
        Self {
            presentation: Presentation::Host,
            keep_alive: true,
            replace: false,
            transition: Transition::None,
        }
    }
}

impl PushOptions {
    pub fn host() -> Self {
        Self::default()
    }
    pub fn sheet() -> Self {
        Self {
            presentation: Presentation::Sheet,
            ..Self::default()
        }
    }
    pub fn with_slide(mut self) -> Self {
        self.transition = Transition::slide_default();
        self
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Frame<R> {
    pub route: R,
    pub presentation: Presentation,
    pub keep_alive: bool,
    pub key: u64,
    pub state: FrameState,
    pub transition: Transition,
    /// Entering：0→1；Active：1；Exiting：1→0。
    pub progress: f32,
}

#[derive(Clone, Debug, PartialEq)]
struct NavInner<R: Clone + Eq + Hash + 'static> {
    frames: Vec<Frame<R>>,
    roots: HashSet<R>,
    next_key: u64,
}

/// Page stack controller (cloneable handle).
#[derive(Clone)]
pub struct Navigator<R: Clone + Eq + Hash + 'static> {
    host_id: String,
    state: Signal<NavInner<R>>,
    unmount: Rc<UnmountHook>,
}

impl<R: Clone + Eq + Hash + 'static> Navigator<R> {
    pub fn new(host_id: impl Into<String>, initial: R, roots: impl IntoIterator<Item = R>) -> Self {
        let host_id = host_id.into();
        let key = navigator_signal_key(&host_id);
        let roots: HashSet<R> = roots.into_iter().collect();
        let initial_clone = initial.clone();
        let roots_init = roots.clone();
        let state = use_signal(key.as_str(), move || NavInner {
            frames: vec![Frame {
                route: initial_clone,
                presentation: Presentation::Host,
                keep_alive: true,
                key: 1,
                state: FrameState::Active,
                transition: Transition::None,
                progress: 1.0,
            }],
            roots: roots_init,
            next_key: 2,
        });
        let mut inner = state.peek();
        for r in roots {
            inner.roots.insert(r);
        }
        state.set_silent(inner);

        // Host 离开树时只清 active 注册。不要 `remove_named_signal`：
        // When starting, `app_builder` will first build a tree without tracking, and then rebuild it in the root effect;
        // If the signal is deleted here, the deferred's reactive host reading stack will unwrap to None.
        let hid = host_id.clone();
        let unmount = UnmountHook::new(move || {
            clear_active_navigator(&hid);
            clear_active_tick(&hid);
        });

        let nav = Self {
            host_id,
            state,
            unmount,
        };
        nav.register_active();
        nav
    }

    pub fn host_id(&self) -> &str {
        &self.host_id
    }

    pub fn unmount_hook(&self) -> Rc<UnmountHook> {
        Rc::clone(&self.unmount)
    }

    pub fn stack(&self) -> Vec<Frame<R>> {
        self.state.get().frames
    }

    pub fn peek_stack(&self) -> Vec<Frame<R>> {
        self.state.peek().frames
    }

    pub fn can_pop(&self) -> bool {
        let inner = self.state.peek();
        let live = inner
            .frames
            .iter()
            .filter(|f| f.state != FrameState::Exiting)
            .count();
        live > 1
    }

    pub fn push(&self, route: R, opts: PushOptions) {
        if opts.replace {
            self.replace(route, opts);
            return;
        }
        let mut inner = self.state.peek();
        if let Some(pos) = inner.frames.iter().position(|f| f.route == route) {
            self.bring_to_front_at(&mut inner, pos, Some(&opts));
            self.state.set(inner);
            self.arm_tick_if_needed();
            return;
        }
        let key = inner.next_key;
        inner.next_key = inner.next_key.saturating_add(1);
        let animated = opts.transition.is_animated();
        inner.frames.push(Frame {
            route,
            presentation: opts.presentation,
            keep_alive: opts.keep_alive,
            key,
            state: if animated {
                FrameState::Entering
            } else {
                FrameState::Active
            },
            transition: opts.transition,
            progress: if animated { 0.0 } else { 1.0 },
        });
        self.state.set(inner);
        self.arm_tick_if_needed();
    }

    pub fn replace(&self, route: R, opts: PushOptions) {
        let mut inner = self.state.peek();
        let animated = opts.transition.is_animated();
        if let Some(top) = inner.frames.last_mut() {
            top.route = route;
            top.presentation = opts.presentation;
            top.keep_alive = opts.keep_alive;
            top.transition = opts.transition;
            top.state = if animated {
                FrameState::Entering
            } else {
                FrameState::Active
            };
            top.progress = if animated { 0.0 } else { 1.0 };
        } else {
            let key = inner.next_key;
            inner.next_key = inner.next_key.saturating_add(1);
            inner.frames.push(Frame {
                route,
                presentation: opts.presentation,
                keep_alive: opts.keep_alive,
                key,
                state: if animated {
                    FrameState::Entering
                } else {
                    FrameState::Active
                },
                transition: opts.transition,
                progress: if animated { 0.0 } else { 1.0 },
            });
        }
        self.state.set(inner);
        self.arm_tick_if_needed();
    }

    pub fn pop(&self) -> bool {
        let mut inner = self.state.peek();
        // Find the last non-Exiting frame
        let Some(idx) = inner
            .frames
            .iter()
            .rposition(|f| f.state != FrameState::Exiting)
        else {
            return false;
        };
        let live = inner
            .frames
            .iter()
            .filter(|f| f.state != FrameState::Exiting)
            .count();
        if live <= 1 {
            return false;
        }
        let animated = inner.frames[idx].transition.is_animated();
        if animated {
            inner.frames[idx].state = FrameState::Exiting;
            // progress 从当前值（通常 1）降到 0
            self.state.set(inner);
            self.arm_tick_if_needed();
            true
        } else {
            inner.frames.remove(idx);
            self.state.set(inner);
            true
        }
    }

    pub fn bring_to_front(&self, route: &R) -> bool {
        let mut inner = self.state.peek();
        let Some(pos) = inner.frames.iter().position(|f| &f.route == route) else {
            return false;
        };
        self.bring_to_front_at(&mut inner, pos, None);
        self.state.set(inner);
        true
    }

    fn bring_to_front_at(&self, inner: &mut NavInner<R>, pos: usize, opts: Option<&PushOptions>) {
        let mut frame = inner.frames.remove(pos);
        if let Some(opts) = opts {
            frame.presentation = opts.presentation;
            frame.keep_alive = opts.keep_alive;
            frame.transition = opts.transition;
            if opts.transition.is_animated() {
                frame.state = FrameState::Entering;
                frame.progress = 0.0;
            } else {
                frame.state = FrameState::Active;
                frame.progress = 1.0;
            }
        } else {
            frame.state = FrameState::Active;
            frame.progress = 1.0;
        }
        let is_root = inner.roots.contains(&frame.route);
        if is_root {
            inner.frames.clear();
            inner.frames.push(frame);
        } else {
            inner.frames.push(frame);
        }
    }

    /// 推进 Entering / Exiting；返回是否仍有动画。
    pub fn tick(&self, dt: f32) -> bool {
        let mut inner = self.state.peek();
        let mut dirty = false;
        let mut still = false;
        let mut remove: Vec<usize> = Vec::new();

        for (i, frame) in inner.frames.iter_mut().enumerate() {
            let dur = frame.transition.duration_secs();
            match frame.state {
                FrameState::Entering => {
                    dirty = true;
                    if !frame.transition.is_animated() {
                        frame.state = FrameState::Active;
                        frame.progress = 1.0;
                    } else {
                        frame.progress = (frame.progress + dt / dur).min(1.0);
                        if frame.progress >= 1.0 {
                            frame.state = FrameState::Active;
                            frame.progress = 1.0;
                        } else {
                            still = true;
                        }
                    }
                }
                FrameState::Exiting => {
                    dirty = true;
                    if !frame.transition.is_animated() {
                        remove.push(i);
                    } else {
                        frame.progress = (frame.progress - dt / dur).max(0.0);
                        if frame.progress <= 0.0 {
                            remove.push(i);
                        } else {
                            still = true;
                        }
                    }
                }
                FrameState::Active => {}
            }
        }

        for i in remove.into_iter().rev() {
            inner.frames.remove(i);
            dirty = true;
        }

        if dirty {
            self.state.set(inner);
        }
        still
    }

    fn arm_tick_if_needed(&self) {
        let needs = self
            .state
            .peek()
            .frames
            .iter()
            .any(|f| matches!(f.state, FrameState::Entering | FrameState::Exiting));
        if needs {
            let nav = self.clone();
            let hid = self.host_id.clone();
            set_active_tick(
                hid,
                Rc::new(move |dt| nav.tick(dt)) as Rc<dyn Fn(f32) -> bool>,
            );
            crate::app::runtime::request_redraw();
        }
    }

    pub fn register_active(&self) {
        let nav = self.clone();
        set_active_navigator(
            self.host_id.clone(),
            Rc::new(move || nav.pop()) as Rc<dyn Fn() -> bool>,
        );
    }

    /// 显式释放：取消 active 注册并删除命名信号。Host 自动 unmount 不会走这条路径。
    pub fn dispose(&self) {
        self.unmount.disarm();
        clear_active_navigator(&self.host_id);
        clear_active_tick(&self.host_id);
        remove_named_signal(&navigator_signal_key(&self.host_id));
    }
}

thread_local! {
    static ACTIVE_NAV: RefCell<Option<(String, Rc<dyn Fn() -> bool>)>> = RefCell::new(None);
    static ACTIVE_TICK: RefCell<Option<(String, Rc<dyn Fn(f32) -> bool>)>> = RefCell::new(None);
}

fn set_active_navigator(host_id: String, pop: Rc<dyn Fn() -> bool>) {
    let prev = ACTIVE_NAV.with(|c| {
        let mut b = c.borrow_mut();
        std::mem::replace(&mut *b, Some((host_id, pop)))
    });
    // Drop old callback outside `RefCell` borrow scope: its captures may run
    // cleanup hooks that re-enter `clear_active_navigator`.
    drop(prev);
}

fn clear_active_navigator(host_id: &str) {
    let _ = ACTIVE_NAV.try_with(|c| {
        let mut b = c.borrow_mut();
        if b.as_ref().map(|(id, _)| id.as_str()) == Some(host_id) {
            *b = None;
        }
    });
}

fn set_active_tick(host_id: String, tick: Rc<dyn Fn(f32) -> bool>) {
    let prev = ACTIVE_TICK.with(|c| {
        let mut b = c.borrow_mut();
        std::mem::replace(&mut *b, Some((host_id, tick)))
    });
    // Same re-entrancy rule as navigator callback replacement.
    drop(prev);
}

fn clear_active_tick(host_id: &str) {
    let _ = ACTIVE_TICK.try_with(|c| {
        let mut b = c.borrow_mut();
        if b.as_ref().map(|(id, _)| id.as_str()) == Some(host_id) {
            *b = None;
        }
    });
}

pub fn try_pop_active() -> bool {
    // Clone the callback out before invoking: `pop` may `state.set` → rebuild →
    // host unmount → `clear_active_navigator` while this cell is still borrowed.
    let Some(pop) = ACTIVE_NAV.with(|c| c.borrow().as_ref().map(|(_, pop)| Rc::clone(pop))) else {
        return false;
    };
    pop()
}

/// Advance the active Navigator transition each frame; returns true if still animating (redraw should continue).
pub fn tick_active(dt: f32) -> bool {
    // Same re-entrancy rule as `try_pop_active`: `tick` may rebuild or re-arm.
    let Some((id, tick)) =
        ACTIVE_TICK.with(|c| c.borrow().as_ref().map(|(i, t)| (i.clone(), Rc::clone(t))))
    else {
        return false;
    };
    let still = tick(dt);
    if !still {
        clear_active_tick(&id);
    }
    still
}

#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
