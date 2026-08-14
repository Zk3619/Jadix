//! Portal 浮层 LIFO 栈：临时浮层登记、Esc 关顶层、按深度分配 z-index。
//!
//! 与 [`crate::app::navigator`]（页面栈）分离。常驻 UI 用 [`OverlayMode::Pinned`]，不进栈。
//! 组件在构建时调用 [`portal_sync`]；内容仍用树内 `rui_portal` 提升。
//!
//! **互斥**：新 Popover 关掉栈内 Menu / Popover / Tooltip；新 Menu 关掉其它 Menu / Tooltip
//!（**不**关 Popover，以便 DatePicker 内嵌 Select）。打开 Modal 会关掉这些临时表面。
//! Modal 之间仍可叠层。

use std::cell::RefCell;

use crate::core::reactivity::Signal;

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
    static OVERLAY_STACK: RefCell<Vec<OverlayLayer>> = const { RefCell::new(Vec::new()) };
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
            let (z, to_close) = OVERLAY_STACK.with(|cell| {
                let mut stack = cell.borrow_mut();
                let mut to_close = prune_closed(&mut stack);
                if open.peek() {
                    if let Some(pos) = stack.iter().position(|l| l.id == id) {
                        stack[pos].open = open;
                        stack[pos].kind = kind;
                        (Some(z_for_index(pos)), to_close)
                    } else {
                        stack.retain(|l| {
                            if l.id == id {
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
                        crate::utils::popup::disarm_underlay_close();
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
    let (z, to_close) = OVERLAY_STACK.with(|cell| {
        let mut stack = cell.borrow_mut();
        let to_close = prune_closed(&mut stack);
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
/// `open.set` 必须在释放 `OVERLAY_STACK` 借用之后调用，否则 reactive rebuild →
/// [`portal_sync`] 会 `RefCell already borrowed`。
pub fn try_close_top() -> bool {
    let (to_close_top, pruned) = OVERLAY_STACK.with(|cell| {
        let mut stack = cell.borrow_mut();
        let pruned = prune_closed(&mut stack);
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
/// Respects [`crate::utils::popup::underlay_close_is_armed`] so the opening gesture
/// cannot instantly dismiss. `open.set` runs **after** the stack borrow is dropped.
pub fn try_dismiss_outside_press() -> bool {
    if !crate::utils::popup::underlay_close_is_armed() {
        return false;
    }
    let (to_close, pruned) = OVERLAY_STACK.with(|cell| {
        let mut stack = cell.borrow_mut();
        let pruned = prune_closed(&mut stack);
        let Some(top) = stack.last() else {
            return (None, pruned);
        };
        match top.kind {
            OverlayKind::Popover | OverlayKind::Menu | OverlayKind::Tooltip => {}
            OverlayKind::Modal => return (None, pruned),
        }
        let open = top.open;
        stack.pop();
        (Some(open), pruned)
    });
    for other in pruned {
        other.try_set(false);
    }
    let Some(open) = to_close else {
        return false;
    };
    if open.peek() {
        // Prefer try_set: stack may briefly hold a disposed handle across teardown.
        open.try_set(false);
        true
    } else {
        false
    }
}

/// True when the hit path intersects a hoisted portal surface (`rui_hoisted_portal`).
pub fn hit_path_on_hoisted_portal(path: &[super::pointer_events::HitPathEntry]) -> bool {
    path.iter().any(|e| e.hoisted_portal)
}

/// Id of the top Stacked Menu / Popover / Tooltip (exclusive dismissable surface), if any.
pub fn top_stacked_exclusive_id() -> Option<String> {
    OVERLAY_STACK.with(|cell| {
        let stack = cell.borrow();
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

/// True when the press is on the **top** exclusive overlay panel (Select / Popover / …).
///
/// Unlike [`hit_path_on_hoisted_portal`], this ignores other hoisted surfaces (e.g. Dialog
/// stage) so clicking Dialog chrome outside the menu still dismisses the menu.
pub fn hit_path_on_top_exclusive_surface(path: &[super::pointer_events::HitPathEntry]) -> bool {
    let Some(id) = top_stacked_exclusive_id() else {
        return false;
    };
    let panel_key = format!("panel:{id}");
    path.iter().any(|e| {
        e.floating_id.as_deref() == Some(id.as_str())
            || e.portal_key.as_deref() == Some(id.as_str())
            || e.portal_key.as_deref() == Some(panel_key.as_str())
    })
}

/// Drop portal stack entries (call on app shutdown so Signal handles do not linger).
pub fn clear_overlay_stack() {
    OVERLAY_STACK.with(|cell| cell.borrow_mut().clear());
    crate::utils::popup::arm_underlay_close();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::reactivity::Signal;

    fn clear() {
        OVERLAY_STACK.with(|c| c.borrow_mut().clear());
        crate::utils::popup::arm_underlay_close();
    }

    fn depth() -> usize {
        OVERLAY_STACK.with(|c| c.borrow().len())
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
            focus_id: None,
            label_for: None,
            on_pointer_down: None,
            on_pointer_up: None,
            on_click: None,
            on_context_menu: None,
            on_double_click: None,
            hoisted_portal: true,
            portal_key: Some("panel:dlg".into()),
            floating_id: None,
            bounds: [0.0; 4],
        }];
        assert!(
            !hit_path_on_top_exclusive_surface(&on_dialog),
            "Dialog portal must not count as Select menu surface"
        );

        let on_menu = [crate::app::pointer_events::HitPathEntry {
            focus_id: None,
            label_for: None,
            on_pointer_down: None,
            on_pointer_up: None,
            on_click: None,
            on_context_menu: None,
            on_double_click: None,
            hoisted_portal: true,
            portal_key: Some("panel:sel".into()),
            floating_id: Some("sel".into()),
            bounds: [0.0; 4],
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
            focus_id: None,
            label_for: None,
            on_pointer_down: None,
            on_pointer_up: None,
            on_click: None,
            on_context_menu: None,
            on_double_click: None,
            hoisted_portal: hoisted,
            portal_key: portal_key.map(str::to_string),
            floating_id: floating_id.map(str::to_string),
            bounds: [0.0; 4],
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
    fn opening_modal_dismisses_exclusive_surfaces() {
        clear();
        let menu = Signal::new(true);
        let modal = Signal::new(true);
        assert!(portal_sync("menu", menu, OverlayKind::Menu, OverlayMode::Stacked).is_some());
        assert!(portal_sync("modal", modal, OverlayKind::Modal, OverlayMode::Stacked).is_some());
        assert!(!menu.peek());
        assert!(modal.peek());
        assert_eq!(depth(), 1);
    }
}
