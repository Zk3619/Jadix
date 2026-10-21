//! Portal 浮层 LIFO 栈：临时浮层登记、Esc 关顶层、按深度分配 z-index。
//!
//! Detached from [`crate::app::navigator`] (page stack). The resident UI uses [`OverlayMode::Pinned`] and does not push onto the stack.
//! 组件在构建时调用 [`portal_sync`]；内容仍用树内 `jadix_portal` 提升。
//!
//! **Mutually exclusive**: New Popover closes the Menu / Popover / Tooltip in the stack; new Menu closes other Menu / Tooltips
//!（**不**关 Popover，以便 DatePicker 内嵌 Select）。打开 Modal 会关掉这些临时表面。
//! Modals can still be stacked.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::fmt;

use crate::core::reactivity::Signal;

use super::window_scope::{self, WindowScope};

const OVERLAY_Z_BASE: i32 = 1000;
const OVERLAY_Z_STEP: i32 = 10;
/// Pinned floating layer convention z (not pushed into the stack; still lower than Stacked's 1000+).
const PINNED_Z_UNDERLAY: i32 = 900;

/// 浮层类别（调试 / 文档；z 一律按栈序）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverlayKind {
    Modal,
    Menu,
    Popover,
    Tooltip,
}

/// 是否进入 Esc LIFO 栈。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum OverlayMode {
    /// Push to the stack; Esc / `try_close_top` can be closed.
    #[default]
    Stacked,
    /// It is only used for portal promotion; it does not push into the stack and does not eat Esc.
    Pinned,
}

/// The underlay / panel z-index of an overlay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OverlayZ {
    pub underlay: i32,
    pub panel: i32,
}

/// Pinned portal z (DragOverlay host). Always-mounted; not on the Esc stack.
pub fn pinned_overlay_z() -> OverlayZ {
    OverlayZ {
        underlay: PINNED_Z_UNDERLAY,
        panel: PINNED_Z_UNDERLAY + 1,
    }
}

struct OverlayLayer {
    id: String,
    /// Logical open — Esc / outside-press set this `false` to start exit.
    open: Signal<bool>,
    /// Presence keep-alive (`mounted`); when set, layer stays while exit anim plays.
    keep_alive: Option<Signal<bool>>,
    #[allow(dead_code)]
    kind: OverlayKind,
    /// Wheel / pan / fling freeze while this layer is open. Default for Menu/Modal is
    /// [`ScrollFreeze::Page`] (page locked, overlay ScrollAreas still move).
    scroll_freeze: ScrollFreeze,
}

fn layer_live(s: Signal<bool>) -> bool {
    s.try_peek().unwrap_or(false)
}

fn layer_is_active(l: &OverlayLayer) -> bool {
    layer_live(l.open) || l.keep_alive.is_some_and(layer_live)
}

thread_local! {
    static OVERLAY_STACKS: RefCell<HashMap<WindowScope, Vec<OverlayLayer>>> =
        RefCell::new(HashMap::new());
    /// App-level scroll freeze ([`set_scroll_freeze`] / [`set_page_scroll_locked`]),
    /// independent of the overlay stack.
    static CUSTOM_SCROLL_FREEZE: Cell<ScrollFreeze> = const { Cell::new(ScrollFreeze::None) };
}

/// Where wheel / pan / fling is frozen while a surface is open.
///
/// Menu / Modal default to [`Self::Page`]: the page behind the surface does not
/// move, but ScrollAreas inside the hoisted overlay still receive wheel.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ScrollFreeze {
    /// Do not freeze.
    #[default]
    None,
    /// Only page (non-`jadix_hoisted_portal`) scroll.
    Page,
    /// Only scroll inside hoisted overlay / floating panels.
    Overlay,
    /// Page and overlays (whole program).
    All,
}

impl ScrollFreeze {
    /// `lock_page_scroll(true)` means page-only freeze; `false` means none.
    pub fn from_lock_page_scroll(yes: bool) -> Self {
        if yes {
            Self::Page
        } else {
            Self::None
        }
    }

    pub fn freezes_page(self) -> bool {
        matches!(self, Self::Page | Self::All)
    }

    pub fn freezes_overlay(self) -> bool {
        matches!(self, Self::Overlay | Self::All)
    }

    /// Union of two scopes (`Page` + `Overlay` → `All`).
    pub fn combine(self, other: Self) -> Self {
        use ScrollFreeze::*;
        match (self, other) {
            (None, x) | (x, None) => x,
            (All, _) | (_, All) => All,
            (Page, Overlay) | (Overlay, Page) => All,
            (Page, Page) => Page,
            (Overlay, Overlay) => Overlay,
        }
    }

    /// `is_overlay_rest` is true when the scroll viewport lives under a hoisted portal.
    pub fn applies_to_rest(self, is_overlay_rest: bool) -> bool {
        match self {
            Self::None => false,
            Self::Page => !is_overlay_rest,
            Self::Overlay => is_overlay_rest,
            Self::All => true,
        }
    }

    /// Whether a wheel / pan over `on_overlay` must be dropped.
    pub fn blocks_wheel(self, on_overlay: bool) -> bool {
        if on_overlay {
            self.freezes_overlay()
        } else {
            self.freezes_page()
        }
    }
}

impl From<bool> for ScrollFreeze {
    fn from(lock_page: bool) -> Self {
        Self::from_lock_page_scroll(lock_page)
    }
}

impl fmt::Display for ScrollFreeze {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::None => "none",
            Self::Page => "page",
            Self::Overlay => "overlay",
            Self::All => "all",
        })
    }
}

/// Parse RSX / string freeze scope (`page` / `overlay` / `all` / `none`).
pub fn parse_scroll_freeze(s: &str) -> ScrollFreeze {
    match s.trim().to_ascii_lowercase().as_str() {
        "page" | "page-only" | "page_only" | "true" | "1" | "yes" => ScrollFreeze::Page,
        "overlay" | "floating" | "float" => ScrollFreeze::Overlay,
        "all" | "app" | "program" | "full" | "both" => ScrollFreeze::All,
        "none" | "off" | "false" | "0" | "no" => ScrollFreeze::None,
        other if other.is_empty() => ScrollFreeze::None,
        _ => ScrollFreeze::Page,
    }
}

/// Menu / Modal lock the page by default; Popover / Tooltip do not.
pub fn default_lock_page_scroll(kind: OverlayKind) -> bool {
    matches!(kind, OverlayKind::Menu | OverlayKind::Modal)
}

/// Menu / Modal freeze **page** scroll by default; Popover / Tooltip do not freeze.
pub fn default_scroll_freeze(kind: OverlayKind) -> ScrollFreeze {
    if default_lock_page_scroll(kind) {
        ScrollFreeze::Page
    } else {
        ScrollFreeze::None
    }
}

/// Resolve an explicit overlay flag, falling back to [`default_lock_page_scroll`].
pub fn resolve_lock_page_scroll(kind: OverlayKind, explicit: Option<bool>) -> bool {
    explicit.unwrap_or_else(|| default_lock_page_scroll(kind))
}

/// `scroll_freeze` wins over `lock_page_scroll`; either falls back to [`default_scroll_freeze`].
pub fn resolve_scroll_freeze(
    kind: OverlayKind,
    freeze: Option<ScrollFreeze>,
    lock_page_scroll: Option<bool>,
) -> ScrollFreeze {
    freeze
        .or_else(|| lock_page_scroll.map(ScrollFreeze::from_lock_page_scroll))
        .unwrap_or_else(|| default_scroll_freeze(kind))
}

/// Lock or unlock page scroll from app code (for example while the pointer hovers a region).
///
/// Public entry: [`crate::set_page_scroll_locked`] (`use jadix::prelude::*;`).
/// Combines with overlay freeze: any lock wins. Default overlay scope is page-only, so
/// wheel over a hoisted overlay panel still reaches that overlay's own ScrollArea.
///
/// ```ignore
/// use jadix::prelude::*;
///
/// // 任意节点：悬停时冻住页面
/// Element::from(button("hint").text("?"))
///     .lock_page_scroll_on_hover();
///
/// // 任意自定义条件
/// set_page_scroll_locked(true);  // 冻页面
/// set_page_scroll_locked(false); // 解
/// set_scroll_freeze(ScrollFreeze::All); // 页面 + 浮层
/// ```
pub fn set_page_scroll_locked(locked: bool) {
    CUSTOM_SCROLL_FREEZE.set(ScrollFreeze::from_lock_page_scroll(locked));
}

/// Set the app-level wheel freeze scope (page / overlay / all / none).
pub fn set_scroll_freeze(scope: ScrollFreeze) {
    CUSTOM_SCROLL_FREEZE.set(scope);
}

/// Combined freeze from [`set_scroll_freeze`] and open overlay layers.
pub fn active_scroll_freeze() -> ScrollFreeze {
    CUSTOM_SCROLL_FREEZE.get().combine(stacked_scroll_freeze())
}

/// True when page scroll is locked (custom setter or an open overlay that opted in).
pub fn page_scroll_locked() -> bool {
    active_scroll_freeze().freezes_page()
}

fn stacked_scroll_freeze() -> ScrollFreeze {
    let (freeze, to_close) = with_stack_mut(|stack| {
        let to_close = prune_closed(stack);
        let freeze = stack
            .iter()
            .filter(|l| layer_live(l.open))
            .fold(ScrollFreeze::None, |acc, l| acc.combine(l.scroll_freeze));
        (freeze, to_close)
    });
    for other in to_close {
        other.try_set(false);
    }
    freeze
}

fn with_stack_mut<R>(f: impl FnOnce(&mut Vec<OverlayLayer>) -> R) -> R {
    with_stack_mut_in(window_scope::active(), f)
}

/// Overlay stack for an explicit window scope (commit-style; does not read TLS on Drop).
fn with_stack_mut_in<R>(scope: WindowScope, f: impl FnOnce(&mut Vec<OverlayLayer>) -> R) -> R {
    let mut f = Some(f);
    let mut result = None;
    let _ = OVERLAY_STACKS.try_with(|cell| {
        let mut map = cell.borrow_mut();
        result = Some(f.take().expect("overlay stack callback")(
            map.entry(scope).or_default(),
        ));
    });
    match result {
        Some(r) => r,
        None => f.take().expect("overlay stack callback")(&mut Vec::new()),
    }
}

fn z_for_index(i: usize) -> OverlayZ {
    let underlay = OVERLAY_Z_BASE + (i as i32) * OVERLAY_Z_STEP;
    OverlayZ {
        underlay,
        panel: underlay + 1,
    }
}

/// Nested menus / tooltips close with their parent. A Menu above another Menu
/// is a peer successor unless its id identifies it as a submenu.
fn exclusive_child_closes_with_parent(parent: &OverlayLayer, child: &OverlayLayer) -> bool {
    child.id.starts_with(&format!("{}__sub", parent.id))
        || child.kind == OverlayKind::Tooltip
        || (parent.kind != OverlayKind::Menu && child.kind == OverlayKind::Menu)
}

/// Drop `pos` and nested children stacked immediately above it. Peer surfaces
/// above stay so finishing an exclusive exit cannot close the live successor.
fn drain_layer_and_nested_children(
    stack: &mut Vec<OverlayLayer>,
    pos: usize,
    to_close: &mut Vec<Signal<bool>>,
) {
    let parent = stack.remove(pos);
    if layer_live(parent.open) {
        to_close.push(parent.open);
    }
    while pos < stack.len() && exclusive_child_closes_with_parent(&parent, &stack[pos]) {
        let child = stack.remove(pos);
        if layer_live(child.open) {
            to_close.push(child.open);
        }
    }
}

/// Drop inactive layers and nested children above them. Return still-open
/// signals so the caller can `try_set(false)` after releasing the stack borrow.
///
/// A layer with `open=false` but `keep_alive=true` (exit presence) stays active.
fn prune_closed(stack: &mut Vec<OverlayLayer>) -> Vec<Signal<bool>> {
    let mut to_close = Vec::new();
    let mut i = 0;
    while i < stack.len() {
        if layer_is_active(&stack[i]) {
            i += 1;
            continue;
        }
        drain_layer_and_nested_children(stack, i, &mut to_close);
    }
    to_close
}

/// Dropdown / popover style surfaces: only one should be open at a time.
///
/// Modals stack; opening a modal still dismisses these singletons underneath.
fn is_exclusive_surface(kind: OverlayKind) -> bool {
    matches!(
        kind,
        OverlayKind::Menu | OverlayKind::Popover | OverlayKind::Tooltip
    )
}

/// Whether `existing` should close when a new overlay of `incoming` is pushed.
///
/// Popover keeps nested Menu (Select / Combobox inside DatePicker). Sibling Menus stay
/// exclusive; a new Popover still dismisses an open Menu.
fn should_dismiss_for_incoming(existing: OverlayKind, incoming: OverlayKind) -> bool {
    match existing {
        OverlayKind::Modal => false,
        OverlayKind::Tooltip => {
            is_exclusive_surface(incoming) || matches!(incoming, OverlayKind::Modal)
        }
        OverlayKind::Menu => {
            matches!(
                incoming,
                OverlayKind::Menu | OverlayKind::Popover | OverlayKind::Modal
            )
        }
        OverlayKind::Popover => {
            matches!(incoming, OverlayKind::Popover | OverlayKind::Modal)
        }
    }
}

/// 与组件 `open` 态对齐（无退场 presence）。
///
/// - [`OverlayMode::Stacked`]: When it is turned on, it will push, and when it is turned off, it will pop up according to id; return to the stack depth z.
///   新打开的 Menu/Popover/Tooltip 会关掉栈内其它同类（以及被 Modal 顶替）；Modal 可叠层。
/// - [`OverlayMode::Pinned`]：不入栈；开时返回约定 z，关时 `None`。
///
/// Use [`portal_sync_presence`] when you need to enter and exit the scene: Esc to close the logic `open`, `mounted` to keep alive until the end of the exit.
pub fn portal_sync(
    id: impl Into<String>,
    open: Signal<bool>,
    kind: OverlayKind,
    mode: OverlayMode,
) -> Option<OverlayZ> {
    portal_sync_inner(id, open, None, kind, mode, default_scroll_freeze(kind))
}

/// [`portal_sync`] with presence keep-alive.
///
/// - Esc / outside-press turns off the logic [`open`] (starts to exit)
/// - [`mounted`] 为 true 时层仍占栈（播退场动画）；drain 后 `mounted=false` 再卸栈
pub fn portal_sync_presence(
    id: impl Into<String>,
    open: Signal<bool>,
    mounted: Signal<bool>,
    kind: OverlayKind,
    mode: OverlayMode,
) -> Option<OverlayZ> {
    portal_sync_inner(
        id,
        open,
        Some(mounted),
        kind,
        mode,
        default_scroll_freeze(kind),
    )
}

/// [`portal_sync_presence`] with an explicit freeze scope (floating overlay property).
pub(crate) fn portal_sync_presence_lock(
    id: impl Into<String>,
    open: Signal<bool>,
    mounted: Signal<bool>,
    kind: OverlayKind,
    mode: OverlayMode,
    freeze: impl Into<ScrollFreeze>,
) -> Option<OverlayZ> {
    portal_sync_inner(id, open, Some(mounted), kind, mode, freeze.into())
}

fn portal_sync_inner(
    id: impl Into<String>,
    open: Signal<bool>,
    keep_alive: Option<Signal<bool>>,
    kind: OverlayKind,
    mode: OverlayMode,
    scroll_freeze: ScrollFreeze,
) -> Option<OverlayZ> {
    let id = id.into();
    let active = layer_live(open) || keep_alive.is_some_and(layer_live);
    match mode {
        OverlayMode::Pinned => {
            if active {
                Some(OverlayZ {
                    underlay: PINNED_Z_UNDERLAY,
                    panel: PINNED_Z_UNDERLAY + 1,
                })
            } else {
                None
            }
        }
        OverlayMode::Stacked => {
            // Close conflicting layers *after* releasing the stack borrow — same rule as
            // [`try_close_top`]: `open.set` may rebuild and re-enter [`portal_sync`].
            // Capture scope once: unmount Drop must not re-read TLS after a window switch.
            let scope = window_scope::active();
            let (z, to_close) = with_stack_mut_in(scope, |stack| {
                let closing_modal = !active
                    && matches!(kind, OverlayKind::Modal)
                    && stack.iter().any(|layer| layer.id == id);
                let mut to_close = prune_closed(stack);
                if active {
                    if let Some(pos) = stack.iter().position(|l| l.id == id) {
                        stack[pos].open = open;
                        stack[pos].keep_alive = keep_alive;
                        stack[pos].kind = kind;
                        stack[pos].scroll_freeze = scroll_freeze;
                        (Some(z_for_index(pos)), to_close)
                    } else {
                        // Capture before retain: peer Menu already open (menubar switch).
                        let had_peer_menu = kind == OverlayKind::Menu
                            && stack.iter().any(|l| l.kind == OverlayKind::Menu);
                        stack.retain(|l| {
                            if l.id == id {
                                return true;
                            }
                            // Nested context-menu submenu (`{parent}__sub…`) must not dismiss parent.
                            if kind == OverlayKind::Menu
                                && l.kind == OverlayKind::Menu
                                && id.starts_with(&format!("{}__sub", l.id))
                            {
                                return true;
                            }
                            if should_dismiss_for_incoming(l.kind, kind) {
                                // Peer replace / exclusive: close logical open and keep
                                // the layer only while presence is still mounted.
                                to_close.push(l.open);
                                l.keep_alive.is_some_and(layer_live)
                            } else {
                                true
                            }
                        });
                        // New stack entry = overlay just opened; ignore underlay closes until
                        // primary-button release (see `disarm_underlay_close` / `arm_underlay_close`).
                        // Nested subs must NOT disarm — parent already armed.
                        // Peer Menu replace (menubar File→Edit) must NOT disarm either — otherwise
                        // outside-click needs an extra press after hover/arrow switch.
                        let nested_sub = kind == OverlayKind::Menu
                            && stack.iter().any(|l| {
                                l.kind == OverlayKind::Menu
                                    && id.starts_with(&format!("{}__sub", l.id))
                            });
                        let menu_over_modal = kind == OverlayKind::Menu
                            && stack.iter().any(|l| l.kind == OverlayKind::Modal);
                        if !nested_sub && !had_peer_menu && !menu_over_modal {
                            crate::utils::popup::disarm_underlay_close();
                        }
                        stack.push(OverlayLayer {
                            id,
                            open,
                            keep_alive,
                            kind,
                            scroll_freeze,
                        });
                        (Some(z_for_index(stack.len() - 1)), to_close)
                    }
                } else {
                    // Closing: drop this id and nested Menu/Tooltip above it (Select
                    // inside DatePicker). A peer Popover above is a live successor
                    // that opened while we were exiting — do not take it with us.
                    if let Some(pos) = stack.iter().position(|l| l.id == id) {
                        if layer_live(stack[pos].open) {
                            // Successor rebound this id with a live `open`.
                        } else {
                            drain_layer_and_nested_children(stack, pos, &mut to_close);
                        }
                    }
                    if closing_modal {
                        stack.retain(|l| {
                            if is_exclusive_surface(l.kind) {
                                to_close.push(l.open);
                                l.keep_alive.is_some_and(layer_live)
                            } else {
                                true
                            }
                        });
                    }
                    (None, to_close)
                }
            });
            for other in to_close {
                other.try_set(false);
            }
            z
        }
    }
}

/// Top Stacked overlay panel z-index (`OverlayZ::panel`), if any.
pub fn top_stacked_panel_z() -> Option<i32> {
    let (z, to_close) = with_stack_mut(|stack| {
        let to_close = prune_closed(stack);
        let z = if stack.is_empty() {
            None
        } else {
            Some(z_for_index(stack.len() - 1).panel)
        };
        (z, to_close)
    });
    for other in to_close {
        other.try_set(false);
    }
    z
}

/// Whether there is an open Modal (Dialog/Sheet/Drawer) in the current window Stacked stack.
pub fn stacked_modal_open() -> bool {
    let (open, to_close) = with_stack_mut(|stack| {
        let to_close = prune_closed(stack);
        let open = stack
            .iter()
            .any(|l| l.kind == OverlayKind::Modal && layer_live(l.open));
        (open, to_close)
    });
    for other in to_close {
        other.try_set(false);
    }
    open
}

/// Whether an open Menu (dropdown / context) or Modal (command / dialog) is on the stack.
/// Page-level wheel must not move content underneath these surfaces.
pub fn stacked_menu_or_modal_open() -> bool {
    let (open, to_close) = with_stack_mut(|stack| {
        let to_close = prune_closed(stack);
        let open = stack.iter().any(|l| {
            layer_live(l.open) && matches!(l.kind, OverlayKind::Modal | OverlayKind::Menu)
        });
        (open, to_close)
    });
    for other in to_close {
        other.try_set(false);
    }
    open
}

/// True when wheel over the page (not a hoisted overlay) must be ignored.
pub fn overlay_blocks_page_wheel(path: &[super::pointer_events::HitPathEntry]) -> bool {
    active_scroll_freeze().freezes_page() && !hit_path_on_hoisted_portal(path)
}

/// True when this wheel / pan must be dropped for the active [`ScrollFreeze`] scope.
pub fn overlay_blocks_wheel(path: &[super::pointer_events::HitPathEntry]) -> bool {
    active_scroll_freeze().blocks_wheel(hit_path_on_hoisted_portal(path))
}

/// Esc：先 prune 已关层，再关闭并弹出顶层 Stacked。返回是否关掉了一层。
///
/// `open.set` must be called after releasing the overlay stack borrowing, otherwise reactive rebuild →
/// [`portal_sync`] will `RefCell already borrowed`.
pub fn try_close_top() -> bool {
    let (to_close_top, pruned) = with_stack_mut(|stack| {
        let pruned = prune_closed(stack);
        let top = stack.pop().map(|top| top.open);
        (top, pruned)
    });
    let mut closed = false;
    for other in pruned {
        if layer_live(other) {
            other.try_set(false);
            closed = true;
        }
    }
    let Some(open) = to_close_top else {
        return closed;
    };
    if layer_live(open) {
        // Prefer try_set: stack may briefly hold a disposed handle across teardown.
        open.try_set(false);
        true
    } else {
        closed
    }
}

/// Close the top exclusive surface for an outside press.
///
/// Presence keep-alive stays on the stack so exit can play. Layers without
/// keep-alive still pop immediately (`portal_sync` without presence).
///
/// Returns whether this press was consumed — including when the top exclusive
/// is already exiting. Overlay underlay `on_click` must not treat that as
/// "nothing to dismiss" and close the modal (pointer-down already started exit).
fn dismiss_exclusive_from_top(
    stack: &mut Vec<OverlayLayer>,
    to_close: &mut Vec<Signal<bool>>,
) -> bool {
    let Some(top) = stack.last() else {
        return false;
    };
    match top.kind {
        OverlayKind::Popover | OverlayKind::Menu | OverlayKind::Tooltip => {}
        OverlayKind::Modal => return false,
    }
    if !layer_live(top.open) {
        return layer_is_active(top);
    }
    let close_menus_below = matches!(top.kind, OverlayKind::Menu);
    let mut i = stack.len() - 1;
    loop {
        let keep = stack[i].keep_alive.is_some_and(layer_live);
        if layer_live(stack[i].open) {
            to_close.push(stack[i].open);
        }
        if keep {
            if i == 0 || !close_menus_below {
                break;
            }
            i -= 1;
            if !matches!(stack[i].kind, OverlayKind::Menu) {
                break;
            }
            continue;
        }
        stack.remove(i);
        if !close_menus_below || i == 0 {
            break;
        }
        i -= 1;
        if !matches!(stack[i].kind, OverlayKind::Menu) {
            break;
        }
    }
    true
}

/// Solid/Radix non-modal dismiss: close the top Stacked Popover/Menu/Tooltip when the
/// pointer press is outside hoisted portal content (and not on its trigger).
///
/// Modals keep a blocking underlay; this path must not close them.
///
/// When the top surface is a **Menu**, also dismisses any Menu stacked under it
/// (context-menu root + submenu close together on one outside click). Stops before
/// Popover/Modal so a Select Menu inside a DatePicker Popover only dismisses the Select.
///
/// Presence keep-alive is left on the stack so the panel can play exit. A second
/// dismiss (overlay underlay `on_click` after pointer-down) consumes the press
/// without popping that exiting layer or closing the modal underneath.
///
/// Respects [`crate::utils::popup::underlay_close_is_armed`] so the opening gesture
/// cannot instantly dismiss. `open.set` runs **after** the stack borrow is dropped.
pub fn try_dismiss_outside_press() -> bool {
    if !crate::utils::popup::underlay_close_is_armed() {
        return false;
    }
    let (to_close, pruned, consumed) = with_stack_mut(|stack| {
        let pruned = prune_closed(stack);
        let mut closing = Vec::new();
        let consumed = dismiss_exclusive_from_top(stack, &mut closing);
        (closing, pruned, consumed)
    });
    for other in pruned {
        other.try_set(false);
    }
    let mut any = consumed;
    for open in to_close {
        if layer_live(open) {
            open.try_set(false);
            any = true;
        }
    }
    any
}

/// Close stacked Popover / Menu / Tooltip **before** viewport geometry moves
/// (page scrollbar press, window resize, minimize / maximize).
///
/// Outside-press waits until mouse-up on the scrollbar path, so the floating
/// layer would first `sync_floating` with the new layout and only then close.
/// Modal stays. Does not require underlay-close armed — these are not the
/// opening gesture.
pub fn dismiss_exclusive_on_viewport_change() -> bool {
    let (to_close, pruned) = with_stack_mut(|stack| {
        let pruned = prune_closed(stack);
        let mut closing = Vec::new();
        while stack.last().is_some_and(|l| {
            layer_is_active(l)
                && matches!(
                    l.kind,
                    OverlayKind::Popover | OverlayKind::Menu | OverlayKind::Tooltip
                )
        }) {
            if let Some(l) = stack.pop() {
                closing.push(l.open);
            }
        }
        (closing, pruned)
    });
    for other in pruned {
        other.try_set(false);
    }
    if to_close.is_empty() {
        return false;
    }
    let mut any = false;
    for open in to_close {
        if layer_live(open) {
            open.try_set(false);
            any = true;
        }
    }
    any
}

/// True when the hit path intersects a hoisted portal surface (`jadix_hoisted_portal`).
pub fn hit_path_on_hoisted_portal(path: &[super::pointer_events::HitPathEntry]) -> bool {
    path.iter().any(|e| e.hoisted_portal)
}

/// Id of the top Stacked Menu / Popover / Tooltip (exclusive dismissable surface), if any.
pub fn top_stacked_exclusive_id() -> Option<String> {
    with_stack_mut(|stack| {
        stack.iter().rev().find_map(|l| {
            if !layer_live(l.open) {
                return None;
            }
            match l.kind {
                OverlayKind::Menu | OverlayKind::Popover | OverlayKind::Tooltip => {
                    Some(l.id.clone())
                }
                OverlayKind::Modal => None,
            }
        })
    })
}

/// True when the press is on the **top** exclusive overlay panel (Select / Popover / …),
/// or on a **parent Menu** of a nested context-menu submenu (`{parent}__sub…`).
///
/// Unlike [`hit_path_on_hoisted_portal`], this ignores other hoisted surfaces (e.g. Dialog
/// stage) so clicking Dialog chrome outside the menu still dismisses the menu.
pub fn hit_path_on_top_exclusive_surface(path: &[super::pointer_events::HitPathEntry]) -> bool {
    fn path_hits_id(path: &[super::pointer_events::HitPathEntry], id: &str) -> bool {
        let panel_key = format!("panel:{id}");
        path.iter().any(|e| {
            e.floating_id.as_deref() == Some(id)
                || e.portal_key.as_deref() == Some(id)
                || e.portal_key.as_deref() == Some(panel_key.as_str())
        })
    }

    with_stack_mut(|stack| {
        let Some(top_pos) = stack.iter().rposition(|l| {
            layer_live(l.open)
                && matches!(
                    l.kind,
                    OverlayKind::Menu | OverlayKind::Popover | OverlayKind::Tooltip
                )
        }) else {
            return false;
        };
        let top_id = stack[top_pos].id.clone();
        let top_kind = stack[top_pos].kind;
        if path_hits_id(path, &top_id) {
            return true;
        }
        // Context-menu submenu open: clicks on the parent Menu still count as inside.
        if matches!(top_kind, OverlayKind::Menu) {
            for l in stack[..top_pos].iter().rev() {
                if !layer_live(l.open) || !matches!(l.kind, OverlayKind::Menu) {
                    continue;
                }
                if top_id.starts_with(&format!("{}__sub", l.id)) && path_hits_id(path, &l.id) {
                    return true;
                }
            }
        }
        false
    })
}

/// Drop portal stack entries for the **current** window scope (tests / that window).
pub fn clear_overlay_stack() {
    with_stack_mut(|stack| stack.clear());
    crate::utils::popup::arm_underlay_close();
}

/// Test/harness helper: current Stacked overlay depth for the active window scope.
#[cfg(test)]
pub fn overlay_stack_depth_for_test() -> usize {
    with_stack_mut(|s| s.len())
}

/// Modal `(index, len)` in stacked stack; only counts [`OverlayKind::Modal`].
pub(crate) fn modal_stack_metrics(id: &str) -> (usize, usize) {
    with_stack_mut(|stack| {
        let indices: Vec<usize> = stack
            .iter()
            .enumerate()
            .filter(|(_, l)| l.kind == OverlayKind::Modal)
            .map(|(i, _)| i)
            .collect();
        let len = indices.len();
        let pos = stack
            .iter()
            .position(|l| l.id == id && l.kind == OverlayKind::Modal);
        let idx = pos.and_then(|p| indices.iter().position(|&i| i == p));
        (idx.unwrap_or(0), len.max(1))
    })
}

pub(crate) fn drop_overlay_stack(scope: WindowScope) {
    let _ = OVERLAY_STACKS.try_with(|cell| {
        cell.borrow_mut().remove(&scope);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::reactivity::Signal;

    fn clear() {
        with_stack_mut(|s| s.clear());
        set_page_scroll_locked(false);
        crate::utils::popup::arm_underlay_close();
    }

    fn depth() -> usize {
        with_stack_mut(|s| s.len())
    }

    #[test]
    fn page_teardown_must_not_peek_disposed_overlay_open() {
        clear();
        let (owner, open) = crate::core::reactivity::create_root(|| {
            let open = Signal::new(true);
            assert!(portal_sync("sel", open, OverlayKind::Menu, OverlayMode::Stacked).is_some());
            open
        });
        drop(owner);
        assert!(
            !open.is_alive(),
            "page island dispose must drop the overlay open signal"
        );
        // Wheel / pointer / next portal_sync prune the stack. Peek panics were
        // the gallery crash after opening a Select then switching pages.
        let _ = stacked_menu_or_modal_open();
        let _ = try_close_top();
        assert_eq!(depth(), 0, "disposed layers must prune off the stack");
        clear();
    }

    #[test]
    fn inactive_modal_does_not_dismiss_live_exclusive_surfaces() {
        for kind in [
            OverlayKind::Menu,
            OverlayKind::Popover,
            OverlayKind::Tooltip,
        ] {
            clear();
            let open = Signal::new(true);
            let modal = Signal::new(false);
            assert!(portal_sync("live", open, kind, OverlayMode::Stacked).is_some());
            assert!(
                portal_sync("sidebar", modal, OverlayKind::Modal, OverlayMode::Stacked).is_none()
            );
            assert!(open.peek(), "inactive modal must not dismiss {kind:?}");
            assert_eq!(depth(), 1);
        }
        clear();
    }

    #[test]
    fn closing_modal_dismisses_nested_exclusive_surface() {
        clear();
        let modal = Signal::new(true);
        let menu = Signal::new(true);
        assert!(portal_sync("dlg", modal, OverlayKind::Modal, OverlayMode::Stacked).is_some());
        assert!(portal_sync("sel", menu, OverlayKind::Menu, OverlayMode::Stacked).is_some());
        assert_eq!(depth(), 2);

        // Dialog close: portal_sync with open=false must clear nested Select too.
        modal.set(false);
        assert!(portal_sync("dlg", modal, OverlayKind::Modal, OverlayMode::Stacked).is_none());
        assert!(
            !menu.peek(),
            "nested menu open signal must be cleared with modal"
        );
        assert_eq!(depth(), 0);
        clear();
    }

    #[test]
    fn top_stacked_panel_z_tracks_depth() {
        clear();
        assert_eq!(top_stacked_panel_z(), None);
        let a = Signal::new(true);
        assert!(portal_sync("a", a, OverlayKind::Modal, OverlayMode::Stacked).is_some());
        assert_eq!(top_stacked_panel_z(), Some(1001));
        let b = Signal::new(true);
        assert!(portal_sync("b", b, OverlayKind::Modal, OverlayMode::Stacked).is_some());
        assert_eq!(top_stacked_panel_z(), Some(1011));
        assert!(try_close_top());
        assert_eq!(top_stacked_panel_z(), Some(1001));
    }

    /// WP-1.2 / smoke #7: Escape → [`try_close_top`] only closes the top layer and does not penetrate the lower layer.
    #[test]
    fn push_two_esc_closes_top_only() {
        clear();
        let a = Signal::new(true);
        let b = Signal::new(true);
        let za = portal_sync("a", a, OverlayKind::Modal, OverlayMode::Stacked).unwrap();
        let zb = portal_sync("b", b, OverlayKind::Menu, OverlayMode::Stacked).unwrap();
        assert_eq!(za.underlay, 1000);
        assert_eq!(zb.underlay, 1010);
        assert_eq!(depth(), 2);

        assert!(try_close_top());
        assert!(!b.peek());
        assert!(a.peek());
        assert_eq!(depth(), 1);

        assert!(try_close_top());
        assert!(!a.peek());
        assert_eq!(depth(), 0);
        assert!(!try_close_top());
    }

    #[test]
    fn three_layers_esc_closes_top_only_each_time() {
        clear();
        let a = Signal::new(true);
        let b = Signal::new(true);
        let c = Signal::new(true);
        assert!(portal_sync("a", a, OverlayKind::Modal, OverlayMode::Stacked).is_some());
        assert!(portal_sync("b", b, OverlayKind::Modal, OverlayMode::Stacked).is_some());
        assert!(portal_sync("c", c, OverlayKind::Modal, OverlayMode::Stacked).is_some());
        assert_eq!(depth(), 3);

        assert!(try_close_top());
        assert!(!c.peek());
        assert!(b.peek());
        assert!(a.peek());

        assert!(try_close_top());
        assert!(!b.peek());
        assert!(a.peek());

        assert!(try_close_top());
        assert!(!a.peek());
        assert_eq!(depth(), 0);
    }

    #[test]
    fn pinned_does_not_enter_stack() {
        clear();
        let open = Signal::new(true);
        let z = portal_sync("pin", open, OverlayKind::Popover, OverlayMode::Pinned).unwrap();
        assert_eq!(z.underlay, 900);
        assert_eq!(depth(), 0);
        assert!(!try_close_top());
        assert!(open.peek());
    }

    #[test]
    fn sync_false_pops_by_id() {
        clear();
        let open = Signal::new(true);
        assert!(portal_sync("x", open, OverlayKind::Popover, OverlayMode::Stacked).is_some());
        open.set(false);
        assert!(portal_sync("x", open, OverlayKind::Popover, OverlayMode::Stacked).is_none());
        assert_eq!(depth(), 0);
    }

    #[test]
    fn same_id_does_not_duplicate() {
        clear();
        let open = Signal::new(true);
        let z1 = portal_sync("same", open, OverlayKind::Menu, OverlayMode::Stacked).unwrap();
        let z2 = portal_sync("same", open, OverlayKind::Menu, OverlayMode::Stacked).unwrap();
        assert_eq!(z1, z2);
        assert_eq!(depth(), 1);
    }

    #[test]
    fn menu_blocks_page_wheel_outside_hoisted_surface() {
        clear();
        let open = Signal::new(true);
        assert!(portal_sync("ctx", open, OverlayKind::Menu, OverlayMode::Stacked).is_some());
        let page = [dummy_hit(None, None, false)];
        assert!(
            overlay_blocks_page_wheel(&page),
            "context menu must lock page wheel"
        );
        let on_menu = [dummy_hit(Some("panel:ctx"), Some("ctx"), true)];
        assert!(
            !overlay_blocks_page_wheel(&on_menu),
            "wheel over the menu panel must still reach overlay scroll"
        );
        open.set(false);
        assert!(portal_sync("ctx", open, OverlayKind::Menu, OverlayMode::Stacked).is_none());
        assert!(
            !overlay_blocks_page_wheel(&page),
            "page wheel resumes after menu closes"
        );
    }

    #[test]
    fn modal_blocks_page_wheel_outside_hoisted_surface() {
        clear();
        let open = Signal::new(true);
        assert!(portal_sync("cmd", open, OverlayKind::Modal, OverlayMode::Stacked).is_some());
        let page = [dummy_hit(None, None, false)];
        assert!(
            overlay_blocks_page_wheel(&page),
            "command overlay must lock page wheel"
        );
        let on_underlay = [dummy_hit(Some("underlay:1000"), None, true)];
        assert!(
            !overlay_blocks_page_wheel(&on_underlay),
            "underlay is hoisted; wheel is consumed without page scroll"
        );
    }

    #[test]
    fn popover_does_not_block_page_wheel() {
        clear();
        let open = Signal::new(true);
        assert!(portal_sync("pop", open, OverlayKind::Popover, OverlayMode::Stacked).is_some());
        let page = [dummy_hit(None, None, false)];
        assert!(
            !overlay_blocks_page_wheel(&page),
            "popover/tooltip must not lock page scroll"
        );
    }

    #[test]
    fn custom_set_page_scroll_locked_blocks_page_wheel() {
        clear();
        let page = [dummy_hit(None, None, false)];
        assert!(!overlay_blocks_page_wheel(&page));
        set_page_scroll_locked(true);
        assert!(
            overlay_blocks_page_wheel(&page),
            "set_page_scroll_locked(true) must freeze page wheel"
        );
        let on_overlay = [dummy_hit(Some("panel:x"), Some("x"), true)];
        assert!(
            !overlay_blocks_page_wheel(&on_overlay),
            "hoisted panel still receives wheel while the page is locked"
        );
        set_page_scroll_locked(false);
        assert!(!overlay_blocks_page_wheel(&page));
    }

    #[test]
    fn lock_page_scroll_on_hover_toggles_global() {
        clear();
        let mut el = crate::core::tree::Element::new().lock_page_scroll_on_hover();
        let h = el.on_hover.take().expect("hover handler");
        (*h.borrow_mut())(true);
        assert!(page_scroll_locked());
        (*h.borrow_mut())(false);
        assert!(!page_scroll_locked());
    }

    #[test]
    fn popover_can_opt_in_to_lock_page_scroll() {
        clear();
        let open = Signal::new(true);
        let mounted = Signal::new(true);
        assert!(portal_sync_presence_lock(
            "pop-lock",
            open,
            mounted,
            OverlayKind::Popover,
            OverlayMode::Stacked,
            true,
        )
        .is_some());
        let page = [dummy_hit(None, None, false)];
        assert!(
            overlay_blocks_page_wheel(&page),
            "explicit lock_page_scroll on a popover must lock the page"
        );
    }

    #[test]
    fn menu_can_opt_out_of_lock_page_scroll() {
        clear();
        let open = Signal::new(true);
        let mounted = Signal::new(true);
        assert!(portal_sync_presence_lock(
            "menu-free",
            open,
            mounted,
            OverlayKind::Menu,
            OverlayMode::Stacked,
            false,
        )
        .is_some());
        let page = [dummy_hit(None, None, false)];
        assert!(
            !overlay_blocks_page_wheel(&page),
            "explicit lock_page_scroll=false must allow page scroll under a menu"
        );
    }

    #[test]
    fn menu_default_freeze_is_page_not_all() {
        clear();
        let open = Signal::new(true);
        assert!(portal_sync("ctx", open, OverlayKind::Menu, OverlayMode::Stacked).is_some());
        assert_eq!(active_scroll_freeze(), ScrollFreeze::Page);
        let on_menu = [dummy_hit(Some("panel:ctx"), Some("ctx"), true)];
        assert!(
            !overlay_blocks_wheel(&on_menu),
            "default page freeze must still allow overlay wheel"
        );
        let page = [dummy_hit(None, None, false)];
        assert!(overlay_blocks_wheel(&page));
        assert!(ScrollFreeze::Page.applies_to_rest(false));
        assert!(
            !ScrollFreeze::Page.applies_to_rest(true),
            "page freeze must not snap overlay scroll rests"
        );
    }

    #[test]
    fn scroll_freeze_overlay_blocks_only_overlay_wheel() {
        clear();
        let open = Signal::new(true);
        let mounted = Signal::new(true);
        assert!(portal_sync_presence_lock(
            "ov-only",
            open,
            mounted,
            OverlayKind::Popover,
            OverlayMode::Stacked,
            ScrollFreeze::Overlay,
        )
        .is_some());
        let page = [dummy_hit(None, None, false)];
        let on_overlay = [dummy_hit(Some("panel:ov-only"), Some("ov-only"), true)];
        assert!(!overlay_blocks_wheel(&page));
        assert!(overlay_blocks_wheel(&on_overlay));
        assert!(!page_scroll_locked());
    }

    #[test]
    fn scroll_freeze_all_blocks_page_and_overlay_wheel() {
        clear();
        let open = Signal::new(true);
        let mounted = Signal::new(true);
        assert!(portal_sync_presence_lock(
            "all",
            open,
            mounted,
            OverlayKind::Menu,
            OverlayMode::Stacked,
            ScrollFreeze::All,
        )
        .is_some());
        let page = [dummy_hit(None, None, false)];
        let on_overlay = [dummy_hit(Some("panel:all"), Some("all"), true)];
        assert!(overlay_blocks_wheel(&page));
        assert!(overlay_blocks_wheel(&on_overlay));
        assert!(ScrollFreeze::All.applies_to_rest(true));
        assert!(ScrollFreeze::All.applies_to_rest(false));
    }

    #[test]
    fn parse_scroll_freeze_aliases() {
        assert_eq!(parse_scroll_freeze("page"), ScrollFreeze::Page);
        assert_eq!(parse_scroll_freeze("overlay"), ScrollFreeze::Overlay);
        assert_eq!(parse_scroll_freeze("all"), ScrollFreeze::All);
        assert_eq!(parse_scroll_freeze("none"), ScrollFreeze::None);
        assert_eq!(parse_scroll_freeze("true"), ScrollFreeze::Page);
        assert_eq!(parse_scroll_freeze("false"), ScrollFreeze::None);
        assert_eq!(
            ScrollFreeze::Page.combine(ScrollFreeze::Overlay),
            ScrollFreeze::All
        );
    }

    #[test]
    fn set_scroll_freeze_all_blocks_overlay_wheel() {
        clear();
        let page = [dummy_hit(None, None, false)];
        let on_overlay = [dummy_hit(Some("panel:x"), Some("x"), true)];
        set_scroll_freeze(ScrollFreeze::All);
        assert!(overlay_blocks_wheel(&page));
        assert!(overlay_blocks_wheel(&on_overlay));
        set_scroll_freeze(ScrollFreeze::None);
        assert!(!overlay_blocks_wheel(&page));
        assert!(!overlay_blocks_wheel(&on_overlay));
    }

    #[test]
    fn dismiss_viewport_change_closes_exclusive_keeps_modal() {
        clear();
        let dialog = Signal::new(true);
        let pop = Signal::new(true);
        let menu = Signal::new(true);
        assert!(portal_sync("dlg", dialog, OverlayKind::Modal, OverlayMode::Stacked).is_some());
        assert!(portal_sync("pop", pop, OverlayKind::Popover, OverlayMode::Stacked).is_some());
        assert!(portal_sync("sel", menu, OverlayKind::Menu, OverlayMode::Stacked).is_some());
        // Resize / scrollbar must not wait for underlay arm (not an outside click).
        crate::utils::popup::disarm_underlay_close();
        assert!(dismiss_exclusive_on_viewport_change());
        assert!(dialog.peek(), "modal must survive viewport dismiss");
        assert!(!pop.peek(), "popover must close before layout follows");
        assert!(!menu.peek(), "menu must close before layout follows");
        assert_eq!(depth(), 1);
        clear();
    }

    #[test]
    fn dismiss_viewport_change_noop_when_empty() {
        clear();
        assert!(!dismiss_exclusive_on_viewport_change());
    }

    #[test]
    fn dismiss_outside_releases_borrow_before_set() {
        clear();
        let open = Signal::new(true);
        assert!(portal_sync("pop", open, OverlayKind::Popover, OverlayMode::Stacked).is_some());
        crate::utils::popup::arm_underlay_close();
        // Must not panic with RefCell already borrowed (set runs after stack unlock).
        assert!(try_dismiss_outside_press());
        assert!(!open.peek());
        assert_eq!(depth(), 0);
        assert!(portal_sync("pop", open, OverlayKind::Popover, OverlayMode::Stacked).is_none());
    }

    #[test]
    fn dismiss_outside_respects_disarm() {
        clear();
        let open = Signal::new(true);
        assert!(portal_sync("pop", open, OverlayKind::Popover, OverlayMode::Stacked).is_some());
        // portal_sync on new push disarms; dismiss must no-op until armed.
        assert!(!try_dismiss_outside_press());
        assert!(open.peek());
        crate::utils::popup::arm_underlay_close();
        assert!(try_dismiss_outside_press());
        assert!(!open.peek());
    }

    /// Presence keep-alive must stay stacked after outside-press so accordion-up
    /// can play. Popping it made the next underlay click close the parent overlay
    /// and the panel flashed then vanished.
    #[test]
    fn dismiss_outside_presence_keeps_layer_for_exit() {
        clear();
        let open = Signal::new(true);
        let mounted = Signal::new(true);
        assert!(portal_sync_presence(
            "sel",
            open,
            mounted,
            OverlayKind::Menu,
            OverlayMode::Stacked
        )
        .is_some());
        crate::utils::popup::arm_underlay_close();
        assert!(try_dismiss_outside_press());
        assert!(!open.peek());
        assert!(mounted.peek(), "keep-alive must still be mounted");
        assert_eq!(depth(), 1, "exiting menu must stay on the stack");
        assert!(
            portal_sync_presence(
                "sel",
                open,
                mounted,
                OverlayKind::Menu,
                OverlayMode::Stacked
            )
            .is_some(),
            "rebuild during exit must still receive a portal z"
        );
        assert!(
            try_dismiss_outside_press(),
            "second outside press must consume the exiting layer"
        );
        assert!(mounted.peek());
        assert_eq!(depth(), 1);
        clear();
    }

    #[test]
    fn dismiss_outside_presence_does_not_close_modal_underneath() {
        clear();
        let dialog = Signal::new(true);
        let dialog_keep = Signal::new(true);
        let menu = Signal::new(true);
        let menu_keep = Signal::new(true);
        assert!(portal_sync_presence(
            "dlg",
            dialog,
            dialog_keep,
            OverlayKind::Modal,
            OverlayMode::Stacked
        )
        .is_some());
        assert!(portal_sync_presence(
            "sel",
            menu,
            menu_keep,
            OverlayKind::Menu,
            OverlayMode::Stacked
        )
        .is_some());
        crate::utils::popup::arm_underlay_close();
        assert!(try_dismiss_outside_press());
        assert!(!menu.peek());
        assert!(dialog.peek());
        assert_eq!(depth(), 2);
        assert!(
            try_dismiss_outside_press(),
            "mouse-up underlay must still be consumed by the exiting menu"
        );
        assert!(dialog.peek(), "modal must stay open during nested exit");
        assert!(menu_keep.peek());
        assert_eq!(depth(), 2);
        clear();
    }

    #[test]
    fn peer_menu_replace_does_not_disarm() {
        clear();
        let a = Signal::new(true);
        let b = Signal::new(false);
        assert!(portal_sync("menu-a", a, OverlayKind::Menu, OverlayMode::Stacked).is_some());
        // Simulate post-click arm (opening gesture released).
        crate::utils::popup::arm_underlay_close();
        assert!(crate::utils::popup::underlay_close_is_armed());
        // Menubar hover/arrow switch: open peer while first is still open.
        b.set(true);
        assert!(portal_sync("menu-b", b, OverlayKind::Menu, OverlayMode::Stacked).is_some());
        assert!(
            crate::utils::popup::underlay_close_is_armed(),
            "replacing a peer Menu must keep underlay armed"
        );
        assert!(!a.peek(), "exclusive Menu should close the previous peer");
        assert!(try_dismiss_outside_press());
        assert!(!b.peek());
    }

    #[test]
    fn nested_menu_sub_does_not_disarm() {
        clear();
        let parent = Signal::new(true);
        let sub = Signal::new(false);
        assert!(portal_sync("file", parent, OverlayKind::Menu, OverlayMode::Stacked).is_some());
        crate::utils::popup::arm_underlay_close();
        sub.set(true);
        assert!(portal_sync("file__sub0", sub, OverlayKind::Menu, OverlayMode::Stacked).is_some());
        assert!(
            crate::utils::popup::underlay_close_is_armed(),
            "opening nested __sub must keep underlay armed"
        );
        assert!(parent.peek(), "parent menu must stay open under nested sub");
    }

    /// Menubar namespaces menus as `{bar}__{menu}`; sub ids are `{bar}__{menu}__subN`.
    /// Nested keep-parent must use the full parent overlay id, not a short suffix.
    #[test]
    fn nested_menubar_namespaced_sub_keeps_parent_open() {
        clear();
        let parent = Signal::new(true);
        let sub = Signal::new(false);
        let parent_id = "gal-menubar__file";
        let sub_id = format!("{parent_id}__sub0");
        assert!(portal_sync(parent_id, parent, OverlayKind::Menu, OverlayMode::Stacked).is_some());
        crate::utils::popup::arm_underlay_close();
        sub.set(true);
        assert!(
            portal_sync(
                sub_id.as_str(),
                sub,
                OverlayKind::Menu,
                OverlayMode::Stacked
            )
            .is_some(),
            "namespaced sub must push onto the stack"
        );
        assert!(
            parent.peek(),
            "menubar parent Menu must stay open under namespaced __sub"
        );
        assert!(
            sub.peek(),
            "sub open signal must remain true after portal_sync"
        );
        assert_eq!(depth(), 2, "parent + nested sub must both remain stacked");
    }

    /// Regression: `arm` must run *after* the open `on_click` (which disarms). Arming first
    /// then opening leaves the stack disarmed so the next outside press only blurs Combobox.
    #[test]
    fn arm_before_open_click_leaves_first_outside_dismiss_noop() {
        clear();
        let open = Signal::new(false);
        crate::utils::popup::arm_underlay_close();
        // Open click: disarm + push (portal_sync also disarms).
        crate::utils::popup::disarm_underlay_close();
        open.set(true);
        assert!(portal_sync("cb", open, OverlayKind::Menu, OverlayMode::Stacked).is_some());
        assert!(
            !try_dismiss_outside_press(),
            "still disarmed after open — first outside dismiss must no-op"
        );
        assert!(open.peek());
        // Next release arms; second outside press closes.
        crate::utils::popup::arm_underlay_close();
        assert!(try_dismiss_outside_press());
        assert!(!open.peek());
    }

    #[test]
    fn arm_after_open_click_allows_immediate_outside_dismiss() {
        clear();
        let open = Signal::new(false);
        crate::utils::popup::disarm_underlay_close();
        open.set(true);
        assert!(portal_sync("cb", open, OverlayKind::Menu, OverlayMode::Stacked).is_some());
        crate::utils::popup::arm_underlay_close();
        assert!(try_dismiss_outside_press());
        assert!(!open.peek());
    }

    #[test]
    fn hit_path_on_top_exclusive_ignores_other_hoisted_portals() {
        clear();
        let menu = Signal::new(true);
        assert!(portal_sync("sel", menu, OverlayKind::Menu, OverlayMode::Stacked).is_some());
        assert_eq!(top_stacked_exclusive_id().as_deref(), Some("sel"));

        let on_dialog = [crate::app::pointer_events::HitPathEntry {
            hoisted_portal: true,
            portal_key: Some("panel:dlg".into()),
            ..Default::default()
        }];
        assert!(
            !hit_path_on_top_exclusive_surface(&on_dialog),
            "Dialog portal must not count as Select menu surface"
        );

        let on_menu = [crate::app::pointer_events::HitPathEntry {
            hoisted_portal: true,
            portal_key: Some("panel:sel".into()),
            floating_id: Some("sel".into()),
            ..Default::default()
        }];
        assert!(hit_path_on_top_exclusive_surface(&on_menu));
    }

    #[test]
    fn opening_menu_closes_other_menu_and_popover() {
        clear();
        let menu_a = Signal::new(true);
        let pop = Signal::new(true);
        let menu_b = Signal::new(true);
        assert!(portal_sync("menu-a", menu_a, OverlayKind::Menu, OverlayMode::Stacked).is_some());
        assert!(portal_sync("pop", pop, OverlayKind::Popover, OverlayMode::Stacked).is_some());
        assert!(!menu_a.peek(), "popover must dismiss prior menu");
        assert_eq!(depth(), 1);

        assert!(portal_sync("menu-b", menu_b, OverlayKind::Menu, OverlayMode::Stacked).is_some());
        assert!(
            pop.peek(),
            "Select/Menu must stack inside Popover (DatePicker caption)"
        );
        assert!(menu_b.peek());
        assert_eq!(depth(), 2);
    }

    #[test]
    fn closing_popover_dismisses_nested_menu() {
        clear();
        let pop = Signal::new(true);
        let menu = Signal::new(true);
        assert!(portal_sync("pop", pop, OverlayKind::Popover, OverlayMode::Stacked).is_some());
        assert!(portal_sync("sel", menu, OverlayKind::Menu, OverlayMode::Stacked).is_some());
        assert_eq!(depth(), 2);
        pop.set(false);
        assert!(portal_sync("pop", pop, OverlayKind::Popover, OverlayMode::Stacked).is_none());
        assert!(!menu.peek(), "nested Select must close with DatePicker");
        assert_eq!(depth(), 0);
    }

    /// Exclusive keep-alive leaves the previous Popover on the stack so it can
    /// play exit. Finishing that exit must not drain a live successor Popover
    /// (gallery: first ColorPicker click while another overlay is still exiting).
    #[test]
    fn finishing_exclusive_exit_must_not_close_successor_popover() {
        clear();
        let leftover_open = Signal::new(false);
        let leftover_keep = Signal::new(true);
        assert!(portal_sync_presence(
            "old",
            leftover_open,
            leftover_keep,
            OverlayKind::Popover,
            OverlayMode::Stacked
        )
        .is_some());

        let fresh_open = Signal::new(true);
        let fresh_keep = Signal::new(true);
        assert!(portal_sync_presence(
            "new",
            fresh_open,
            fresh_keep,
            OverlayKind::Popover,
            OverlayMode::Stacked
        )
        .is_some());
        assert!(
            fresh_open.peek(),
            "opening a Popover must not clear its own open"
        );

        leftover_keep.set(false);
        assert!(portal_sync_presence(
            "old",
            leftover_open,
            leftover_keep,
            OverlayKind::Popover,
            OverlayMode::Stacked
        )
        .is_none());
        assert!(
            fresh_open.peek(),
            "finishing the previous Popover's exit must not close the live successor"
        );
        assert_eq!(depth(), 1);
        clear();
    }

    #[test]
    fn finishing_menu_exit_keeps_peer_and_its_submenu_open() {
        clear();
        let file = Signal::new(true);
        let file_keep = Signal::new(true);
        let file_sub = Signal::new(true);
        let file_sub_keep = Signal::new(true);
        let edit = Signal::new(true);
        let edit_sub = Signal::new(true);
        for (id, open, mounted) in [
            ("bar__file", file, file_keep),
            ("bar__file__sub0", file_sub, file_sub_keep),
        ] {
            assert!(portal_sync_presence(
                id,
                open,
                mounted,
                OverlayKind::Menu,
                OverlayMode::Stacked
            )
            .is_some());
        }
        assert!(portal_sync("bar__edit", edit, OverlayKind::Menu, OverlayMode::Stacked).is_some());
        assert!(!file.peek());
        assert!(!file_sub.peek());
        assert!(portal_sync(
            "bar__edit__sub0",
            edit_sub,
            OverlayKind::Menu,
            OverlayMode::Stacked
        )
        .is_some());

        file_keep.set(false);
        assert!(portal_sync_presence(
            "bar__file",
            file,
            file_keep,
            OverlayKind::Menu,
            OverlayMode::Stacked
        )
        .is_none());
        assert!(edit.peek(), "finishing File must retain the peer Edit menu");
        assert!(edit_sub.peek(), "finishing File must retain Edit's submenu");
        assert_eq!(depth(), 2);

        // File's later submenu exit must also leave the successor intact.
        file_sub_keep.set(false);
        assert!(portal_sync_presence(
            "bar__file__sub0",
            file_sub,
            file_sub_keep,
            OverlayKind::Menu,
            OverlayMode::Stacked
        )
        .is_none());
        assert!(edit.peek());
        assert!(edit_sub.peek());
        edit.set(false);
        assert!(portal_sync("bar__edit", edit, OverlayKind::Menu, OverlayMode::Stacked).is_none());
        assert!(
            !edit_sub.peek(),
            "a real submenu must still close with its parent"
        );
        assert_eq!(depth(), 0);
        clear();
    }

    #[test]
    fn finishing_exclusive_exit_still_closes_nested_menu() {
        clear();
        let pop_open = Signal::new(false);
        let pop_keep = Signal::new(true);
        let menu = Signal::new(true);
        assert!(portal_sync_presence(
            "dp",
            pop_open,
            pop_keep,
            OverlayKind::Popover,
            OverlayMode::Stacked
        )
        .is_some());
        assert!(portal_sync("sel", menu, OverlayKind::Menu, OverlayMode::Stacked).is_some());
        assert_eq!(depth(), 2);

        pop_keep.set(false);
        assert!(portal_sync_presence(
            "dp",
            pop_open,
            pop_keep,
            OverlayKind::Popover,
            OverlayMode::Stacked
        )
        .is_none());
        assert!(
            !menu.peek(),
            "nested Select must still close with DatePicker"
        );
        assert_eq!(depth(), 0);
        clear();
    }

    fn dummy_hit(
        portal_key: Option<&str>,
        floating_id: Option<&str>,
        hoisted: bool,
    ) -> crate::app::pointer_events::HitPathEntry {
        crate::app::pointer_events::HitPathEntry {
            hoisted_portal: hoisted,
            portal_key: portal_key.map(str::to_string),
            floating_id: floating_id.map(str::to_string),
            ..Default::default()
        }
    }

    #[test]
    fn nested_select_item_path_counts_as_inside_before_unmount() {
        clear();
        let pop = Signal::new(true);
        let menu = Signal::new(true);
        assert!(portal_sync("dp", pop, OverlayKind::Popover, OverlayMode::Stacked).is_some());
        assert!(portal_sync("sel", menu, OverlayKind::Menu, OverlayMode::Stacked).is_some());

        let press_on_item = [dummy_hit(Some("panel:sel"), Some("sel"), true)];
        assert!(
            hit_path_on_top_exclusive_surface(&press_on_item),
            "release dismiss must use this pre-click path, not a re-hit after Select closes"
        );

        menu.set(false);
        assert!(portal_sync("sel", menu, OverlayKind::Menu, OverlayMode::Stacked).is_none());
        assert!(
            pop.peek(),
            "parent popover stays open when only the nested menu closes"
        );

        crate::utils::popup::arm_underlay_close();
        let miss_after_unmount = [dummy_hit(None, None, false)];
        assert!(
            !hit_path_on_top_exclusive_surface(&miss_after_unmount),
            "post-unmount re-hit would look outside — that path must not drive dismiss"
        );
        assert!(try_dismiss_outside_press());
        assert!(!pop.peek());
        clear();
    }

    #[test]
    fn outside_path_is_not_on_exclusive_surface() {
        clear();
        let pop = Signal::new(true);
        assert!(portal_sync("dp", pop, OverlayKind::Popover, OverlayMode::Stacked).is_some());
        let miss = [dummy_hit(None, None, false)];
        assert!(!hit_path_on_top_exclusive_surface(&miss));
        clear();
    }

    #[test]
    fn sibling_menus_remain_exclusive() {
        clear();
        let a = Signal::new(true);
        let b = Signal::new(true);
        assert!(portal_sync("a", a, OverlayKind::Menu, OverlayMode::Stacked).is_some());
        assert!(portal_sync("b", b, OverlayKind::Menu, OverlayMode::Stacked).is_some());
        assert!(!a.peek());
        assert!(b.peek());
        assert_eq!(depth(), 1);
    }

    #[test]
    fn menu_stacks_above_modal_without_closing_it() {
        clear();
        let modal = Signal::new(true);
        let menu = Signal::new(true);
        assert!(portal_sync("modal", modal, OverlayKind::Modal, OverlayMode::Stacked).is_some());
        assert!(portal_sync("menu", menu, OverlayKind::Menu, OverlayMode::Stacked).is_some());
        assert!(modal.peek());
        assert!(menu.peek());
        assert_eq!(depth(), 2);
    }

    #[test]
    fn overlay_stacks_are_isolated_by_window_scope() {
        use crate::app::window_scope::{enter, WindowScope};
        clear();
        let primary = Signal::new(true);
        assert!(portal_sync("p", primary, OverlayKind::Menu, OverlayMode::Stacked).is_some());
        assert_eq!(depth(), 1);

        let _g = enter(WindowScope::Secondary(9));
        clear();
        let secondary = Signal::new(true);
        assert!(portal_sync("s", secondary, OverlayKind::Menu, OverlayMode::Stacked).is_some());
        assert_eq!(depth(), 1);
        drop(_g);

        assert_eq!(depth(), 1);
        assert!(primary.peek());
        assert!(try_close_top());
        assert!(!primary.peek());
        assert_eq!(depth(), 0);
        assert!(secondary.peek());
        clear();
    }

    #[test]
    fn dropping_secondary_overlay_stack_does_not_clear_primary() {
        use crate::app::window_scope::{enter, WindowScope};
        clear();
        let primary = Signal::new(true);
        assert!(portal_sync("p", primary, OverlayKind::Modal, OverlayMode::Stacked).is_some());
        let _g = enter(WindowScope::Secondary(3));
        let secondary = Signal::new(true);
        assert!(portal_sync("s", secondary, OverlayKind::Menu, OverlayMode::Stacked).is_some());
        drop(_g);
        super::drop_overlay_stack(WindowScope::Secondary(3));
        assert_eq!(depth(), 1);
        assert!(primary.peek());
        clear();
    }
}
