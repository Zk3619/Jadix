//! Portal 浮层 LIFO 栈：临时浮层登记、Esc 关顶层、按深度分配 z-index。
//!
//! 与 [`crate::app::navigator`]（页面栈）分离。常驻 UI 用 [`OverlayMode::Pinned`]，不进栈。
//! 组件在构建时调用 [`portal_sync`]；内容仍用树内 `rui_portal` 提升。
//!
//! **互斥**：新 Popover 关掉栈内 Menu / Popover / Tooltip；新 Menu 关掉其它 Menu / Tooltip
//!（**不**关 Popover，以便 DatePicker 内嵌 Select）。打开 Modal 会关掉这些临时表面。
//! Modal 之间仍可叠层。

use std::cell::RefCell;
use std::collections::HashMap;

use crate::core::reactivity::Signal;

use super::window_scope::{self, WindowScope};

const OVERLAY_Z_BASE: i32 = 1000;
const OVERLAY_Z_STEP: i32 = 10;
/// Pinned 浮层约定 z（不入栈；仍低于 Stacked 的 1000+）。
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
    /// 进栈；Esc / `try_close_top` 可关。
    #[default]
    Stacked,
    /// 仅 portal 提升用；不进栈、不吃 Esc。
    Pinned,
}

/// 一层 overlay 的 underlay / panel z-index。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OverlayZ {
    pub underlay: i32,
    pub panel: i32,
}

struct OverlayLayer {
    id: String,
    open: Signal<bool>,
    #[allow(dead_code)]
    kind: OverlayKind,
}

thread_local! {
    static OVERLAY_STACKS: RefCell<HashMap<WindowScope, Vec<OverlayLayer>>> =
        RefCell::new(HashMap::new());
}

fn with_stack_mut<R>(f: impl FnOnce(&mut Vec<OverlayLayer>) -> R) -> R {
    OVERLAY_STACKS.with(|cell| {
        let mut map = cell.borrow_mut();
        f(map.entry(window_scope::active()).or_default())
    })
}

fn z_for_index(i: usize) -> OverlayZ {
    let underlay = OVERLAY_Z_BASE + (i as i32) * OVERLAY_Z_STEP;
    OverlayZ {
        underlay,
        panel: underlay + 1,
    }
}

/// Drop the first already-closed layer and everything stacked above it (nested
/// Select inside a closed DatePicker / Dialog). Return still-open signals so the
/// caller can `try_set(false)` after releasing the stack borrow.
fn prune_closed(stack: &mut Vec<OverlayLayer>) -> Vec<Signal<bool>> {
    let mut to_close = Vec::new();
    if let Some(pos) = stack.iter().position(|l| !l.open.peek()) {
        for l in stack.drain(pos..) {
            if l.open.peek() {
                to_close.push(l.open);
            }
        }
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

/// 与组件 `open` 态对齐。
///
/// - [`OverlayMode::Stacked`]：开则 push，关则按 id 弹出；返回栈深度 z。
///   新打开的 Menu/Popover/Tooltip 会关掉栈内其它同类（以及被 Modal 顶替）；Modal 可叠层。
/// - [`OverlayMode::Pinned`]：不入栈；开时返回约定 z，关时 `None`。
pub fn portal_sync(
    id: impl Into<String>,
    open: Signal<bool>,
    kind: OverlayKind,
    mode: OverlayMode,
) -> Option<OverlayZ> {
    let id = id.into();
    match mode {
        OverlayMode::Pinned => {
            if open.peek() {
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
            let (z, to_close) = with_stack_mut(|stack| {
                let mut to_close = prune_closed(stack);
                if open.peek() {
                    if let Some(pos) = stack.iter().position(|l| l.id == id) {
                        stack[pos].open = open;
                        stack[pos].kind = kind;
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
                                to_close.push(l.open);
                                false
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
                        if !nested_sub && !had_peer_menu {
                            crate::utils::popup::disarm_underlay_close();
                        }
                        stack.push(OverlayLayer { id, open, kind });
                        (Some(z_for_index(stack.len() - 1)), to_close)
                    }
                } else {
                    // Closing: drop this id and anything stacked above it (nested Select
                    // inside DatePicker). Closing a Modal also clears leftover exclusives.
                    if let Some(pos) = stack.iter().position(|l| l.id == id) {
                        for l in stack.drain(pos..) {
                            if l.id != id {
                                to_close.push(l.open);
                            }
                        }
                    }
                    if matches!(kind, OverlayKind::Modal) {
                        stack.retain(|l| {
                            if is_exclusive_surface(l.kind) {
                                to_close.push(l.open);
                                false
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

/// Esc：先 prune 已关层，再关闭并弹出顶层 Stacked。返回是否关掉了一层。
///
/// `open.set` 必须在释放 overlay 栈借用之后调用，否则 reactive rebuild →
/// [`portal_sync`] 会 `RefCell already borrowed`。
pub fn try_close_top() -> bool {
    let (to_close_top, pruned) = with_stack_mut(|stack| {
        let pruned = prune_closed(stack);
        let top = stack.pop().map(|top| top.open);
        (top, pruned)
    });
    let mut closed = false;
    for other in pruned {
        if other.peek() {
            other.try_set(false);
            closed = true;
        }
    }
    let Some(open) = to_close_top else {
        return closed;
    };
    if open.peek() {
        // Prefer try_set: stack may briefly hold a disposed handle across teardown.
        open.try_set(false);
        true
    } else {
        closed
    }
}

/// Solid/Radix non-modal dismiss: close the top Stacked Popover/Menu/Tooltip when the
/// pointer press is outside hoisted portal content (and not on its trigger).
///
/// Modals keep a blocking underlay; this path must not close them.
///
/// When the top surface is a **Menu**, also pops any Menu stacked under it (context-menu
/// root + submenu close together on one outside click). Stops before Popover/Modal so a
/// Select Menu inside a DatePicker Popover only dismisses the Select.
///
/// Respects [`crate::utils::popup::underlay_close_is_armed`] so the opening gesture
/// cannot instantly dismiss. `open.set` runs **after** the stack borrow is dropped.
pub fn try_dismiss_outside_press() -> bool {
    if !crate::utils::popup::underlay_close_is_armed() {
        return false;
    }
    let (to_close, pruned) = with_stack_mut(|stack| {
        let pruned = prune_closed(stack);
        let Some(top) = stack.last() else {
            return (Vec::new(), pruned);
        };
        match top.kind {
            OverlayKind::Popover | OverlayKind::Menu | OverlayKind::Tooltip => {}
            OverlayKind::Modal => return (Vec::new(), pruned),
        }
        let mut closing = Vec::new();
        // Always pop the top exclusive surface.
        if let Some(l) = stack.pop() {
            let close_menus_below = matches!(l.kind, OverlayKind::Menu);
            closing.push(l.open);
            if close_menus_below {
                while stack
                    .last()
                    .is_some_and(|l| matches!(l.kind, OverlayKind::Menu))
                {
                    if let Some(l) = stack.pop() {
                        closing.push(l.open);
                    }
                }
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
        if open.peek() {
            open.try_set(false);
            any = true;
        }
    }
    any
}

/// True when the hit path intersects a hoisted portal surface (`rui_hoisted_portal`).
pub fn hit_path_on_hoisted_portal(path: &[super::pointer_events::HitPathEntry]) -> bool {
    path.iter().any(|e| e.hoisted_portal)
}

/// Id of the top Stacked Menu / Popover / Tooltip (exclusive dismissable surface), if any.
pub fn top_stacked_exclusive_id() -> Option<String> {
    with_stack_mut(|stack| {
        stack.iter().rev().find_map(|l| {
            if !l.open.peek() {
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
            l.open.peek()
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
                if !l.open.peek() || !matches!(l.kind, OverlayKind::Menu) {
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

pub(crate) fn drop_overlay_stack(scope: WindowScope) {
    OVERLAY_STACKS.with(|cell| {
        cell.borrow_mut().remove(&scope);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::reactivity::Signal;

    fn clear() {
        with_stack_mut(|s| s.clear());
        crate::utils::popup::arm_underlay_close();
    }

    fn depth() -> usize {
        with_stack_mut(|s| s.len())
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

    /// WP-1.2 / smoke #7：Escape → [`try_close_top`] 只关最顶层，不穿透下层。
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
        assert!(
            portal_sync("file__sub0", sub, OverlayKind::Menu, OverlayMode::Stacked).is_some()
        );
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
            portal_sync(sub_id.as_str(), sub, OverlayKind::Menu, OverlayMode::Stacked).is_some(),
            "namespaced sub must push onto the stack"
        );
        assert!(parent.peek(), "menubar parent Menu must stay open under namespaced __sub");
        assert!(sub.peek(), "sub open signal must remain true after portal_sync");
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
}
