//! # Accordion
//!
//! 对照 ShadCN **base** [`accordion.tsx`](../../../../reference/ShadCN-UI/apps/v4/registry/bases/base/ui/accordion.tsx)：
//!
//! ```text
//! Accordion                      // Root：single | multiple；value 为已展开项 id 列表
//! └── AccordionItem              // value + border-b
//!     ├── AccordionTrigger       // Header + chevron（展开旋转）
//!     └── AccordionContent       // Panel：overflow-hidden + max-height CSS transition（ShadCN）
//! ```
//!
//! 进出场对位 ShadCN `accordion-down` / `accordion-up`（height 0 ↔ content-height，200ms ease-out）：
//! 实现为 `max-height` **CSS transition**（[`TransitionProperty::MaxHeight`]），不用 MotionStyle 手写插值。
//! Chevron 为 `transform`（rotate）CSS transition。
//!
//! 样式对位 `cn-accordion` / `cn-accordion-item` / `cn-accordion-trigger` /
//! `cn-accordion-content`：`flex w-full flex-col` · 项 `border-b` · 触发器 `py-4 text-sm font-medium`
//! · 内容 `pb-4 text-sm font-normal text-muted-foreground`。
//!
//! `ui!` 中 `<Accordion>` / `<AccordionContent>` 由宏展开（[`accordion_scope`] /
//! [`accordion_content`]）。Builder [`accordion`] 与 RSX 共用实现。
//!
//! ## 用法（双写）
//! ```ignore
//! // A · RSX（camelCase 宏别名：defaultValue / valueSignal）
//! ui! {
//!     <Accordion id="faq" defaultValue={vec!["item-1".into()]}>
//!         <AccordionItem value="item-1">
//!             <AccordionTrigger>"Is it accessible?"</AccordionTrigger>
//!             <AccordionContent>
//!                 <Label>"Yes. It adheres to the WAI-ARIA design pattern."</Label>
//!             </AccordionContent>
//!         </AccordionItem>
//!         <AccordionItem value="item-2">
//!             <AccordionTrigger>"Is it styled?"</AccordionTrigger>
//!             <AccordionContent>
//!                 <Label>"Yes. It comes with default styles."</Label>
//!             </AccordionContent>
//!         </AccordionItem>
//!     </Accordion>
//! }
//!
//! // B · Builder（只 snake_case）
//! accordion("faq")
//!     .default_value(vec!["item-1".into()])
//!     .item("item-1", "Is it accessible?", || {
//!         label("").text("Yes. It adheres to the WAI-ARIA design pattern.").into()
//!     })
//!     .item("item-2", "Is it styled?", || {
//!         label("").text("Yes. It comes with default styles.").into()
//!     })
//! ```
//!
//! **身份（WP-2.8）**：`id` 可选；空串 → [`resolve_instance_id`]。

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use rui_macros::component;
use taffy::style::{AlignItems, Dimension, Display, FlexDirection, JustifyContent, Overflow};

use crate::app::reactive::{reactive_anchor, reactive_placeholder_sized};
use crate::core::reactivity::{
    create_context, create_effect, provide_context, use_context, use_scoped_signal, use_signal,
    with_child_owner, Context, Signal,
};
use crate::core::tree::{parse_color, parse_text_color, Element, LayoutStatus};
use crate::layout::layout_tree::LayoutTree;
use crate::primitives::interactive::interactive;
use crate::primitives::r#box::{hstack, vstack};
use crate::style::{
    css_transition_needs_frame, tick_element_transitions, with_class_list, TimingFunction,
    TransitionEntry, TransitionProperty,
};
use crate::tokens;
use crate::utils::lucide::lucide_glyph;
use crate::utils::motion::{ensure_motion_transitions, snap_motion_baseline, MotionBootstrap};
use crate::utils::presence::{ExitGate, PresencePhase};
use crate::utils::resolve_instance_id;
use crate::utils::roving::{attach_focus_roving, FocusRovingAxis};
use crate::utils::take_fragment;
use crate::utils::text_node::{build_icon_text_node, build_text_node, TextNodeConfig};

const ATTR_SLOT: &str = "data-slot";
const ATTR_ITEM_VALUE: &str = "rui_accordion_item_value";
const SLOT_ROOT: &str = "accordion";
const SLOT_ITEM: &str = "accordion-item";
const SLOT_TRIGGER: &str = "accordion-trigger";
const SLOT_TRIGGER_ICON: &str = "accordion-trigger-icon";
const SLOT_CONTENT: &str = "accordion-content";
const SLOT_CONTENT_INNER: &str = "accordion-content-inner";

const ACCORDION_MS: f32 = 200.0;
const CHEVRON_PX: f32 = 16.0;
const CHEVRON_OPEN_DEG: f32 = 180.0;
const TRIGGER_WEIGHT: u16 = 500;
const CONTENT_WEIGHT: u16 = 400;

// ── Context ──────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct AccordionCtx {
    pub id: String,
    pub value: Signal<Vec<String>>,
    pub multiple: bool,
    pub disabled: bool,
}

#[derive(Clone)]
pub struct AccordionItemCtx {
    pub value: String,
    pub disabled: bool,
}

fn accordion_context() -> Context<AccordionCtx> {
    static CTX: OnceLock<Context<AccordionCtx>> = OnceLock::new();
    *CTX.get_or_init(create_context::<AccordionCtx>)
}

fn accordion_item_context() -> Context<AccordionItemCtx> {
    static CTX: OnceLock<Context<AccordionItemCtx>> = OnceLock::new();
    *CTX.get_or_init(create_context::<AccordionItemCtx>)
}

pub fn use_accordion() -> Option<AccordionCtx> {
    use_context(accordion_context())
}

pub fn use_accordion_item() -> Option<AccordionItemCtx> {
    use_context(accordion_item_context())
}

fn require_accordion(part: &str) -> AccordionCtx {
    use_accordion().unwrap_or_else(|| {
        panic!("{part} must be used within <Accordion> / accordion_scope");
    })
}

fn require_accordion_item(part: &str) -> AccordionItemCtx {
    use_accordion_item().unwrap_or_else(|| {
        panic!("{part} must be used within <AccordionItem>");
    })
}

fn resolve_open_items_signal(
    controlled: Option<Signal<Vec<String>>>,
    instance_id: &str,
    default: Vec<String>,
) -> Signal<Vec<String>> {
    match controlled {
        Some(sig) => sig,
        None => {
            let key = format!("{instance_id}__accordion");
            use_scoped_signal(&key, move || default.clone())
        }
    }
}

fn is_item_open(open: &[String], value: &str) -> bool {
    open.iter().any(|v| v == value)
}

fn toggle_open_item(open: &mut Vec<String>, value: &str, multiple: bool) {
    if multiple {
        if let Some(i) = open.iter().position(|v| v == value) {
            open.remove(i);
        } else {
            open.push(value.to_string());
        }
    } else if is_item_open(open, value) {
        open.clear();
    } else {
        open.clear();
        open.push(value.to_string());
    }
}

thread_local! {
    static PANEL_PRESENCE_REGISTRY: RefCell<Vec<Rc<RefCell<AccordionPanelPresenceCell>>>> =
        const { RefCell::new(Vec::new()) };
}

const PANEL_EXIT: Duration = Duration::from_millis(ACCORDION_MS as u64);

#[derive(Clone)]
struct AccordionPanelPresenceHandle {
    cell: Rc<RefCell<AccordionPanelPresenceCell>>,
}

struct AccordionPanelPresenceCell {
    mounted: Signal<bool>,
    gate: ExitGate,
    needs_enter: bool,
    enter_active: bool,
    exit_armed: bool,
    /// 是否已做过至少一次 presence 同步（区分冷启动 defaultOpen 与后续再展开）。
    initialized: bool,
    /// 上次完全展开时的面板高度（退场 height 动画用）。
    measured_height: f32,
    exit_from_height: Option<f32>,
    /// 退场 `max-height` 是否已 seed（勿每帧 SnapAfterEnsure 重置 tween）。
    exit_height_seeded: bool,
}

fn use_accordion_panel_presence(key: &str) -> AccordionPanelPresenceHandle {
    let cell_key = format!("__rui_accordion_panel:{key}");
    let mounted_key = format!("__rui_accordion_mounted:{key}");
    let cell = use_signal(cell_key.as_str(), || {
        let mounted = use_signal(mounted_key.as_str(), || false);
        let rc = Rc::new(RefCell::new(AccordionPanelPresenceCell {
            mounted,
            gate: ExitGate::new(false, PANEL_EXIT),
            needs_enter: false,
            enter_active: false,
            exit_armed: false,
            initialized: false,
            measured_height: 0.0,
            exit_from_height: None,
            exit_height_seeded: false,
        }));
        PANEL_PRESENCE_REGISTRY.with(|r| r.borrow_mut().push(Rc::clone(&rc)));
        rc
    });
    AccordionPanelPresenceHandle {
        cell: cell.peek(),
    }
}

fn accordion_panel_mounted_signal(handle: &AccordionPanelPresenceHandle) -> Signal<bool> {
    handle.cell.borrow().mounted
}

fn sync_accordion_panel_presence(
    handle: &AccordionPanelPresenceHandle,
    want: bool,
) -> PresencePhase {
    let mounted_sig = accordion_panel_mounted_signal(handle);
    let (phase, mounted, set_mounted) = {
        let mut c = handle.cell.borrow_mut();
        let was = c.gate.phase();
        let changed = c.gate.set_present(want, Instant::now());
        let phase = c.gate.phase();
        let mounted = c.gate.is_mounted();
        if want
            && matches!(phase, PresencePhase::Present)
            && (changed || was == PresencePhase::Absent)
        {
            if !c.initialized {
                c.needs_enter = false;
                c.enter_active = false;
            } else if changed && was != PresencePhase::Present {
                c.needs_enter = true;
                c.enter_active = false;
            }
            c.exit_armed = false;
            c.exit_from_height = None;
            c.exit_height_seeded = false;
        }
        c.initialized = true;
        if matches!(phase, PresencePhase::Exiting) && !c.exit_armed {
            c.exit_armed = true;
            c.needs_enter = false;
            c.enter_active = false;
            if c.exit_from_height.is_none() {
                c.exit_from_height = Some(c.measured_height.max(1.0));
            }
        }
        let set_mounted = c.mounted.peek() != mounted;
        (phase, mounted, set_mounted)
    };
    if set_mounted {
        mounted_sig.set(mounted);
    }
    phase
}

/// 事件循环推进退场计时；返回是否仍需 redraw。
pub(crate) fn tick_accordion_panel_presence() -> bool {
    let now = Instant::now();
    let mut needs_frame = false;
    let mut unmount: Vec<Signal<bool>> = Vec::new();
    PANEL_PRESENCE_REGISTRY.with(|reg| {
        for cell in reg.borrow().iter() {
            let maybe_unmount = {
                let mut c = cell.borrow_mut();
                if c.gate.phase() == PresencePhase::Exiting {
                    if c.gate.tick(now) {
                        c.needs_enter = false;
                        c.enter_active = false;
                        c.exit_armed = false;
                        c.exit_from_height = None;
                        c.exit_height_seeded = false;
                        Some(c.mounted)
                    } else {
                        needs_frame = true;
                        None
                    }
                } else if c.enter_active && c.gate.phase() == PresencePhase::Present {
                    needs_frame = true;
                    None
                } else {
                    None
                }
            };
            if let Some(sig) = maybe_unmount {
                unmount.push(sig);
            }
        }
    });
    for sig in unmount {
        sig.set(false);
    }
    needs_frame
}

fn panel_height_transition_entries(exiting: bool) -> [TransitionEntry; 1] {
    [TransitionEntry {
        property: TransitionProperty::MaxHeight,
        duration_ms: ACCORDION_MS,
        delay_ms: 0.0,
        timing: if exiting {
            TimingFunction::EaseIn
        } else {
            TimingFunction::EaseOut
        },
    }]
}

fn chevron_transition_entries() -> [TransitionEntry; 1] {
    [TransitionEntry {
        property: TransitionProperty::Transform,
        duration_ms: ACCORDION_MS,
        delay_ms: 0.0,
        timing: TimingFunction::EaseOut,
    }]
}

fn panel_height_animating(el: &Element) -> bool {
    css_transition_needs_frame(&el.style, &el.transition_snapshot)
        || el.transition_snapshot.has_active_tweens()
}

fn panel_content_height(presence: &AccordionPanelPresenceHandle, el: &Element) -> f32 {
    let c = presence.cell.borrow();
    c.exit_from_height
        .filter(|h| *h > 1.0)
        .or_else(|| (c.measured_height > 1.0).then_some(c.measured_height))
        .unwrap_or_else(|| el.layout_bounds[3].max(0.0))
}

fn seed_panel_height_transition(el: &mut Element) {
    if el.style.transition.is_empty() {
        return;
    }
    let _ = tick_element_transitions(&mut el.style, &mut el.transition_snapshot, 0.0);
}

fn set_panel_max_height(
    el: &mut Element,
    height_px: f32,
    exiting: bool,
    bootstrap: Option<MotionBootstrap>,
) -> bool {
    match bootstrap {
        Some(MotionBootstrap::SnapAfterEnsure) => {
            ensure_motion_transitions(&mut el.style, &panel_height_transition_entries(exiting));
            snap_motion_baseline(el);
        }
        Some(MotionBootstrap::EnsureAfterSnap) => {
            snap_motion_baseline(el);
            ensure_motion_transitions(&mut el.style, &panel_height_transition_entries(exiting));
        }
        Some(MotionBootstrap::EnsureOnly) | None => {
            ensure_motion_transitions(&mut el.style, &panel_height_transition_entries(exiting));
        }
    }
    let dim = Dimension::length(height_px.max(0.0));
    if el.style.max_size.height != dim {
        el.style.max_size.height = dim;
        true
    } else {
        false
    }
}

fn finish_accordion_panel_exit_if_height_idle(
    presence: &AccordionPanelPresenceHandle,
    el: &Element,
) {
    if accordion_panel_phase(presence) != PresencePhase::Exiting {
        return;
    }
    if panel_height_animating(el) {
        return;
    }
    let mounted_sig = accordion_panel_mounted_signal(presence);
    let set_mounted = {
        let mut c = presence.cell.borrow_mut();
        if c.gate.phase() != PresencePhase::Exiting {
            return;
        }
        c.gate.force_absent();
        c.exit_from_height = None;
        c.exit_height_seeded = false;
        c.mounted.peek() != false
    };
    if set_mounted {
        mounted_sig.set(false);
    }
}

fn accordion_panel_phase(presence: &AccordionPanelPresenceHandle) -> PresencePhase {
    presence.cell.borrow().gate.phase()
}

fn max_height_is_auto(el: &Element) -> bool {
    matches!(
        crate::layout::dim_kind(el.style.max_size.height),
        crate::layout::DimKind::Auto
    )
}

/// 内容区正文统一 normal(400)；覆盖 `<Label>` 默认 medium(500) 以免比 Trigger 更粗。
fn stamp_accordion_content_text_weight(el: &mut Element) {
    if el.text.is_some() {
        el.text_style.weight = CONTENT_WEIGHT;
        el.attrs.insert("font_weight".to_string(), "1".to_string());
    }
    for child in &mut el.children {
        stamp_accordion_content_text_weight(child);
    }
}

/// 单实例订阅 `open` 并 request_redraw；各面板在 `reactive_sync` 里 peek 同步 presence。
fn register_accordion_open_redraw_effect(instance_id: &str, open_sig: Signal<Vec<String>>) {
    let key = format!("{instance_id}__accordion_open_redraw");
    use_signal(key.as_str(), move || {
        create_effect(move || {
            let _ = open_sig.get();
            crate::app::runtime::request_redraw();
        });
        ()
    });
}

fn sync_accordion_panel_open(
    open_sig: Signal<Vec<String>>,
    value: &str,
    presence: &AccordionPanelPresenceHandle,
) {
    let want = is_item_open(&open_sig.peek(), value);
    sync_accordion_panel_presence(presence, want);
}

fn caption_text(el: &Element) -> Option<String> {
    if let Some(t) = el.text.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        return Some(t.to_string());
    }
    el.children.iter().find_map(caption_text)
}

fn chevron_icon() -> Element {
    let glyph = lucide_glyph("chevron-down").unwrap_or_else(|| "▾".into());
    build_icon_text_node(glyph)
        .text_size(CHEVRON_PX)
        .text_color(parse_text_color("muted-foreground"))
}

fn find_slot_child<'a>(el: &'a Element, slot: &str) -> Option<&'a Element> {
    if el.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(slot) {
        return Some(el);
    }
    el.children.iter().find_map(|c| find_slot_child(c, slot))
}

fn find_slot_child_mut<'a>(el: &'a mut Element, slot: &str) -> Option<&'a mut Element> {
    if el.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(slot) {
        return Some(el);
    }
    for child in &mut el.children {
        if let Some(hit) = find_slot_child_mut(child, slot) {
            return Some(hit);
        }
    }
    None
}

// ── Scope ─────────────────────────────────────────────────────────────────────

/// 提供 Accordion 上下文并构建根壳（`ui!` `<Accordion>` / Builder 共用）。
pub fn accordion_scope(
    id: impl Into<String>,
    value: Option<Signal<Vec<String>>>,
    default_value: Vec<String>,
    multiple: bool,
    disabled: bool,
    class: Option<String>,
    body: impl FnOnce() -> Element,
) -> Element {
    let id = resolve_instance_id(id, "accordion");
    let value = resolve_open_items_signal(value, &id, default_value);
    register_accordion_open_redraw_effect(&id, value);
    let mut root = with_child_owner(|| {
        provide_context(
            accordion_context(),
            AccordionCtx {
                id: id.clone(),
                value,
                multiple,
                disabled,
            },
        );
        let kids = take_fragment(body());
        let mut root: Element = vstack()
            .w("100%")
            .min_w(0.0)
            .gap(0.0)
            .children(kids)
            .into();
        if let Some(c) = class.filter(|s| !s.trim().is_empty()) {
            root = with_class_list(root, &c);
        }
        root
    });
    root.attrs.insert(ATTR_SLOT.to_string(), SLOT_ROOT.to_string());
    root.template_id = Some(id.clone());
    attach_focus_roving(&mut root, FocusRovingAxis::Vertical);
    root
}

/// 宏 `<AccordionContent>`：按 Item 上下文显隐 + 进出场过渡。
///
/// 显隐由 [`reactive_anchor`] + [`ExitGate`] 负责：收起时保持挂载至退场动画结束
/// （对位 Dialog/Sheet presence；勿在 placeholder 上再套 `Display::None` 的 reactive_sync）。
pub fn accordion_content(body: impl Fn() -> Element + 'static) -> Element {
    let acc = require_accordion("AccordionContent");
    let item = require_accordion_item("AccordionContent");
    let value = item.value.clone();
    let open_sig = acc.value;
    let menu_id = acc.id.clone();
    let anchor = format!("{menu_id}:content:{value}");
    let effect_anchor = anchor.clone();
    let value_anchor = value.clone();
    let presence = use_accordion_panel_presence(&format!("{menu_id}:presence:{value}"));
    let sync_presence = presence.clone();
    let mounted = accordion_panel_mounted_signal(&presence);
    let open_sync = open_sig;
    let value_for_presence = value.clone();
    sync_accordion_panel_open(open_sync, &value_for_presence, &presence);

    reactive_anchor(effect_anchor, move || {
        let want = is_item_open(&open_sync.get(), &value_for_presence);
        sync_accordion_panel_presence(&sync_presence, want);
        if !mounted.get() {
            let mut hidden = Element::new();
            hidden.style.display = Display::None;
            hidden
                .attrs
                .insert(ATTR_SLOT.to_string(), SLOT_CONTENT.to_string());
            hidden
                .attrs
                .insert(ATTR_ITEM_VALUE.to_string(), value_anchor.clone());
            return hidden;
        }
        let mut inner = body();
        if inner.attrs.get("rui_fragment").map(|s| s.as_str()) == Some("1") {
            let kids = std::mem::take(&mut inner.children);
            inner = vstack().w("100%").min_w(0.0).children(kids).into();
        }
        inner
            .attrs
            .insert(ATTR_SLOT.to_string(), SLOT_CONTENT_INNER.to_string());
        stamp_accordion_content_text_weight(&mut inner);
        let mut wrap: Element = vstack()
            .w("100%")
            .min_w(0.0)
            .px(tokens::SPACE_4)
            .pb(tokens::SPACE_4)
            .overflow_y("hidden")
            .child(inner)
            .into();
        wrap.attrs
            .insert(ATTR_SLOT.to_string(), SLOT_CONTENT.to_string());
        wrap.attrs
            .insert(ATTR_ITEM_VALUE.to_string(), value_anchor.clone());
        wrap = wrap
            .text_size(14.0)
            .text_color(parse_text_color("muted-foreground"));
        wrap = wrap.accesskit_role("region");
        wrap.template_id = Some(format!("{menu_id}:panel:{value_anchor}"));
        let sync_presence = sync_presence.clone();
        wrap = wrap.reactive_sync(move |el, layout_tree| {
            sync_accordion_panel_phase(el, layout_tree, &sync_presence)
        });
        wrap
    });

    let mut panel = reactive_placeholder_sized(anchor, 0.0, 0.0);
    panel
        .attrs
        .insert(ATTR_SLOT.to_string(), SLOT_CONTENT.to_string());
    panel.attrs.insert(ATTR_ITEM_VALUE.to_string(), value);
    panel
}

fn sync_accordion_panel_phase(
    el: &mut Element,
    layout_tree: &mut LayoutTree,
    presence: &AccordionPanelPresenceHandle,
) -> LayoutStatus {
    let phase = accordion_panel_phase(presence);
    if !phase.is_mounted() {
        let mut layout = false;
        if el.style.display != Display::None {
            el.style.display = Display::None;
            layout = true;
        }
        if layout && el.layout_node.is_some() {
            layout_tree.update_taffy_node_style(el);
            crate::app::damage::push_frame_paint_damage(el.layout_bounds);
            return LayoutStatus::LayoutChanged;
        }
        return LayoutStatus::Clean;
    }

    let visually_open = phase == PresencePhase::Present;
    let exiting = phase == PresencePhase::Exiting;
    let mut layout = false;
    let mut style_changed = false;

    if el.style.display != Display::Flex {
        el.style.display = Display::Flex;
        layout = true;
    }
    if el.style.overflow.y != Overflow::Hidden {
        el.style.overflow.y = Overflow::Hidden;
        layout = true;
    }
    if (el.style.opacity - 1.0).abs() > 0.001 {
        el.style.opacity = 1.0;
        style_changed = true;
    }
    if el.style.scale != [1.0, 1.0] {
        el.style.scale = [1.0, 1.0];
        style_changed = true;
    }

    if exiting {
        let mut c = presence.cell.borrow_mut();
        if c.exit_from_height.is_none() {
            c.exit_from_height = Some(c.measured_height.max(el.layout_bounds[3]).max(1.0));
        }
    }

    let exit_seed_height = if exiting {
        let mut c = presence.cell.borrow_mut();
        if !c.exit_height_seeded {
            c.exit_height_seeded = true;
            true
        } else {
            false
        }
    } else {
        false
    };

    let (needs_enter, enter_was_active) = {
        let mut c = presence.cell.borrow_mut();
        let needs_enter = c.needs_enter;
        if needs_enter && visually_open {
            c.needs_enter = false;
            c.enter_active = true;
        }
        (needs_enter, c.enter_active)
    };

    if visually_open && !exiting && !enter_was_active && max_height_is_auto(el) {
        let h = el.layout_bounds[3];
        if h > 1.0 {
            presence.cell.borrow_mut().measured_height = h;
        }
    }

    let w = crate::layout::dim_kind(el.style.size.width);
    if !matches!(w, crate::layout::DimKind::Percent(_)) {
        el.style.size.width = Dimension::percent(1.0);
        layout = true;
    }
    if !matches!(
        crate::layout::dim_kind(el.style.size.height),
        crate::layout::DimKind::Auto
    ) {
        el.style.size.height = Dimension::auto();
        layout = true;
    }

    if exiting {
        if exit_seed_height {
            let open_h = panel_content_height(presence, el);
            el.style.max_size.height = Dimension::length(open_h.max(0.0));
            style_changed |= set_panel_max_height(el, 0.0, true, Some(MotionBootstrap::SnapAfterEnsure));
            seed_panel_height_transition(el);
        }
        finish_accordion_panel_exit_if_height_idle(presence, el);
    } else if visually_open && (needs_enter || enter_was_active) {
        let measured = {
            let mut c = presence.cell.borrow_mut();
            if c.measured_height < 1.0 {
                if !max_height_is_auto(el) {
                    el.style.max_size.height = Dimension::auto();
                    layout = true;
                } else {
                    let h = el.layout_bounds[3];
                    if h > 1.0 {
                        c.measured_height = h;
                    }
                }
            }
            c.measured_height
        };
        if measured < 1.0 {
            if layout && el.layout_node.is_some() {
                layout_tree.update_taffy_node_style(el);
                crate::app::damage::push_frame_paint_damage(el.layout_bounds);
                return LayoutStatus::LayoutChanged;
            }
            return if layout {
                LayoutStatus::LayoutChanged
            } else {
                LayoutStatus::Clean
            };
        }
        if needs_enter {
            el.style.max_size.height = Dimension::length(0.0);
            snap_motion_baseline(el);
            ensure_motion_transitions(&mut el.style, &panel_height_transition_entries(false));
        }
        style_changed |= set_panel_max_height(el, measured, false, None);
        seed_panel_height_transition(el);
        if enter_was_active && !needs_enter && !panel_height_animating(el) {
            presence.cell.borrow_mut().enter_active = false;
            if el.style.max_size.height != Dimension::auto() {
                el.style.max_size.height = Dimension::auto();
                layout = true;
            }
        }
    } else if visually_open {
        if el.style.max_size.height != Dimension::auto() {
            el.style.max_size.height = Dimension::auto();
            layout = true;
        }
        el.style.transition.retain(|e| e.property != TransitionProperty::MaxHeight);
    }

    let animating = panel_height_animating(el);

    if style_changed || animating {
        crate::app::damage::push_frame_paint_damage(el.layout_bounds);
        if (style_changed || animating) && el.layout_node.is_some() {
            layout_tree.update_taffy_node_style(el);
            layout = true;
        }
    }

    if layout && el.layout_node.is_some() {
        crate::app::damage::push_frame_paint_damage(el.layout_bounds);
        return LayoutStatus::LayoutChanged;
    }

    if style_changed || animating {
        LayoutStatus::StyleChanged
    } else {
        LayoutStatus::Clean
    }
}

fn sync_accordion_chevron(el: &mut Element, open: bool) -> LayoutStatus {
    ensure_motion_transitions(&mut el.style, &chevron_transition_entries());
    let want_rotate = if open { CHEVRON_OPEN_DEG } else { 0.0 };
    let mut changed = false;
    if (el.style.rotate_deg - want_rotate).abs() > 0.01 {
        el.style.rotate_deg = want_rotate;
        changed = true;
    }
    let animating = css_transition_needs_frame(&el.style, &el.transition_snapshot)
        || el.transition_snapshot.has_active_tweens();
    if changed || animating {
        crate::app::damage::push_frame_paint_damage(el.layout_bounds);
        LayoutStatus::StyleChanged
    } else {
        LayoutStatus::Clean
    }
}

// ── RSX parts ─────────────────────────────────────────────────────────────────

/// 提供 Item 上下文并构建项壳（`ui!` `<AccordionItem>` 宏展开）。
pub fn accordion_item_scope(
    value: impl Into<String>,
    disabled: bool,
    class: Option<String>,
    body: impl FnOnce() -> Element,
) -> Element {
    let _acc = require_accordion("AccordionItem");
    let value = value.into();
    with_child_owner(|| {
        provide_context(
            accordion_item_context(),
            AccordionItemCtx {
                value: value.clone(),
                disabled,
            },
        );
        build_accordion_item_shell(value, disabled, class, take_fragment(body()))
    })
}

fn build_accordion_item_shell(
    value: String,
    _disabled: bool,
    class: Option<String>,
    kids: Vec<Element>,
) -> Element {
    let mut item: Element = vstack()
        .w("100%")
        .min_w(0.0)
        .border_bottom(parse_color("border"), 1.0)
        .children(kids)
        .into();
    item.attrs.insert(ATTR_SLOT.to_string(), SLOT_ITEM.to_string());
    item.attrs
        .insert(ATTR_ITEM_VALUE.to_string(), value.clone());
    item.template_id = Some(format!("accordion-item:{value}"));
    if let Some(c) = class.filter(|s| !s.trim().is_empty()) {
        item = with_class_list(item, &c);
    }
    item
}

/// ShadCN `AccordionItem`：`cn-accordion-item` + `border-b`。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn AccordionItem(
    value: String,
    disabled: bool,
    class: Option<String>,
    children: Option<Element>,
) -> Element {
    accordion_item_scope(value, disabled, class, move || children.unwrap_or_else(Element::new))
}

/// ShadCN `AccordionTrigger`：Header + 触发器 + chevron。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn AccordionTrigger(
    disabled: bool,
    class: Option<String>,
    children: Option<Element>,
) -> Element {
    let acc = require_accordion("AccordionTrigger");
    let item = require_accordion_item("AccordionTrigger");
    let item_disabled = item.disabled || disabled || acc.disabled;
    let value = item.value.clone();
    let open_sig = acc.value;
    let multiple = acc.multiple;
    let focus_id = format!("{}:{value}", acc.id);

    let label_text = children
        .as_ref()
        .and_then(caption_text)
        .unwrap_or_else(|| value.clone());

    let mut chevron = chevron_icon();
    chevron
        .attrs
        .insert(ATTR_SLOT.to_string(), SLOT_TRIGGER_ICON.to_string());
    chevron = chevron.flex_shrink(0.0);
    let sync_open = open_sig;
    let sync_value = value.clone();
    chevron = chevron.reactive_sync(move |el, _layout_tree| {
        let open = is_item_open(&sync_open.peek(), &sync_value);
        sync_accordion_chevron(el, open)
    });

    let mut caption = build_text_node(
        &label_text,
        TextNodeConfig {
            size_px: 14.0,
            weight: TRIGGER_WEIGHT,
            color: parse_text_color("foreground"),
            leading_scale: Some(1.25),
            ..Default::default()
        },
    );
    caption = caption.flex_grow(1.0).flex_shrink(1.0).min_w(0.0).text_left();

    let mut trigger_row: Element = hstack()
        .w("100%")
        .min_w(0.0)
        .items("start")
        .justify("between")
        .gap(tokens::SPACE_4)
        .px(tokens::SPACE_4)
        .py(tokens::SPACE_4)
        .font_weight(TRIGGER_WEIGHT as f32)
        .child(caption)
        .child(chevron)
        .into();
    trigger_row
        .attrs
        .insert(ATTR_SLOT.to_string(), SLOT_TRIGGER.to_string());
    trigger_row.style.flex_direction = FlexDirection::Row;
    trigger_row.style.align_items = Some(AlignItems::FLEX_START);
    trigger_row.style.justify_content = Some(JustifyContent::SPACE_BETWEEN);

    let val_click = value.clone();
    let open_click = open_sig;
    let fid = focus_id.clone();
    let mut trigger: Element = interactive(trigger_row)
        .focus_id(focus_id.clone())
        .focusable(!item_disabled)
        .focus_ring(!item_disabled)
        .on_click(move || {
            if item_disabled {
                return;
            }
            use_signal("__rui_button_focus_id", String::new).set(fid.clone());
            let mut next = open_click.get();
            toggle_open_item(&mut next, &val_click, multiple);
            open_click.set(next);
        })
        .into();
    trigger = trigger.accesskit_role("button");
    trigger.template_id = Some(focus_id);
    if item_disabled {
        trigger = trigger.opacity(0.5).cursor_not_allowed();
    } else {
        trigger = trigger.cursor_kind(crate::core::tree::CursorKind::Pointer);
    }
    if let Some(c) = class.filter(|s| !s.trim().is_empty()) {
        trigger = crate::style::with_class_list(trigger, &c);
    }

    hstack()
        .w("100%")
        .min_w(0.0)
        .child(trigger)
        .into()
}

/// 非宏路径：子节点已构建时用 reactive_sync 显隐（RSX 走 [`accordion_content`]）。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn AccordionContent(class: Option<String>, children: Option<Element>) -> Element {
    let acc = require_accordion("AccordionContent");
    let item = require_accordion_item("AccordionContent");
    let value = item.value.clone();
    let open_sig = acc.value;
    let presence = use_accordion_panel_presence(&format!("{}:presence:{value}", acc.id));
    let open_sync = open_sig;
    let value_for_presence = value.clone();
    sync_accordion_panel_open(open_sync, &value_for_presence, &presence);

    let mut inner = children.unwrap_or_else(Element::new);
    if inner.attrs.get("rui_fragment").map(|s| s.as_str()) == Some("1") {
        let kids = std::mem::take(&mut inner.children);
        inner = vstack().w("100%").min_w(0.0).children(kids).into();
    }
    inner
        .attrs
        .insert(ATTR_SLOT.to_string(), SLOT_CONTENT_INNER.to_string());
    stamp_accordion_content_text_weight(&mut inner);
    inner = inner
        .px(tokens::SPACE_4)
        .pb(tokens::SPACE_4)
        .text_size(14.0)
        .text_color(parse_text_color("muted-foreground"));
    if let Some(c) = class.filter(|s| !s.trim().is_empty()) {
        inner = crate::style::with_class_list(inner, &c);
    }

    let mut panel: Element = vstack().w("100%").min_w(0.0).child(inner).into();
    panel
        .attrs
        .insert(ATTR_SLOT.to_string(), SLOT_CONTENT.to_string());
    panel
        .attrs
        .insert(ATTR_ITEM_VALUE.to_string(), value.clone());
    panel = panel.overflow_y("hidden");
    panel.template_id = Some(format!("{}:panel:{value}", acc.id));

    let sync_presence = presence.clone();
    panel = panel.reactive_sync(move |el, layout_tree| {
        let want = is_item_open(&open_sync.peek(), &value_for_presence);
        sync_accordion_panel_presence(&sync_presence, want);
        sync_accordion_panel_phase(el, layout_tree, &sync_presence)
    });
    panel = panel.accesskit_role("region");
    panel
}

/// RSX 备用入口（无宏特殊展开时）；复合面请走宏 → [`accordion_scope`]。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn Accordion(
    id: String,
    value: Option<Signal<Vec<String>>>,
    default_value: Option<Vec<String>>,
    multiple: bool,
    disabled: bool,
    class: Option<String>,
    children: Option<Element>,
) -> Element {
    accordion_scope(
        id,
        value,
        default_value.unwrap_or_default(),
        multiple,
        disabled,
        class,
        move || children.unwrap_or_else(Element::new),
    )
}

// ── Builder ───────────────────────────────────────────────────────────────────

struct AccordionItemSpec {
    value: String,
    title: String,
    body: Box<dyn Fn() -> Element>,
    disabled: bool,
}

pub struct AccordionBuilder {
    id: String,
    value: Option<Signal<Vec<String>>>,
    default_value: Vec<String>,
    multiple: bool,
    disabled: bool,
    class: Option<String>,
    items: Vec<AccordionItemSpec>,
}

/// 复合 Accordion 入口（对位 `<Accordion>`）。
pub fn accordion(id: impl Into<String>) -> AccordionBuilder {
    AccordionBuilder {
        id: id.into(),
        value: None,
        default_value: Vec::new(),
        multiple: false,
        disabled: false,
        class: None,
        items: Vec::new(),
    }
}

impl AccordionBuilder {
    pub fn value_signal(mut self, s: Signal<Vec<String>>) -> Self {
        self.value = Some(s);
        self
    }

    pub fn default_value(mut self, v: Vec<String>) -> Self {
        self.default_value = v;
        self
    }

    pub fn multiple(mut self, v: bool) -> Self {
        self.multiple = v;
        self
    }

    pub fn disabled(mut self, v: bool) -> Self {
        self.disabled = v;
        self
    }

    pub fn class(mut self, c: impl Into<String>) -> Self {
        self.class = Some(c.into());
        self
    }

    pub fn item(
        mut self,
        value: impl Into<String>,
        title: impl Into<String>,
        body: impl Fn() -> Element + 'static,
    ) -> Self {
        self.items.push(AccordionItemSpec {
            value: value.into(),
            title: title.into(),
            body: Box::new(body),
            disabled: false,
        });
        self
    }

    pub fn item_disabled(
        mut self,
        value: impl Into<String>,
        title: impl Into<String>,
        body: impl Fn() -> Element + 'static,
    ) -> Self {
        self.items.push(AccordionItemSpec {
            value: value.into(),
            title: title.into(),
            body: Box::new(body),
            disabled: true,
        });
        self
    }
}

impl From<AccordionBuilder> for Element {
    fn from(builder: AccordionBuilder) -> Self {
        let AccordionBuilder {
            id,
            value,
            default_value,
            multiple,
            disabled,
            class,
            items,
        } = builder;
        accordion_scope(id, value, default_value, multiple, disabled, class, move || {
            let mut out = Vec::with_capacity(items.len());
            for spec in items {
                let value = spec.value.clone();
                let title = spec.title.clone();
                let body = spec.body;
                let item_disabled = spec.disabled;
                out.push(accordion_item_scope(
                    value,
                    item_disabled,
                    None,
                    move || {
                        let mut frag = Element::new();
                        frag.set_attr("rui_fragment", "1");
                        frag.children(vec![
                            AccordionTrigger(AccordionTriggerProps {
                                disabled: None,
                                class: None,
                                base: None,
                                children: Some(Element::new().text(title)),
                            }),
                            accordion_content(move || body()),
                        ])
                    },
                ));
            }
            let mut frag = Element::new();
            frag.set_attr("rui_fragment", "1");
            frag.children(out)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::reactivity::{create_root, use_signal};

    #[test]
    fn toggle_single_mode() {
        let mut open = vec!["a".into()];
        toggle_open_item(&mut open, "a", false);
        assert!(open.is_empty());
        toggle_open_item(&mut open, "b", false);
        assert_eq!(open, vec!["b"]);
        toggle_open_item(&mut open, "c", false);
        assert_eq!(open, vec!["c"]);
    }

    #[test]
    fn toggle_multiple_mode() {
        let mut open = Vec::<String>::new();
        toggle_open_item(&mut open, "a", true);
        toggle_open_item(&mut open, "b", true);
        assert_eq!(open, vec!["a", "b"]);
        toggle_open_item(&mut open, "a", true);
        assert_eq!(open, vec!["b"]);
    }

    #[test]
    fn scope_sets_template_and_slot() {
        let (_h, el) = create_root(|| {
            accordion_scope("acc-test", None, vec!["one".into()], false, false, None, || {
                Element::new()
            })
        });
        assert_eq!(el.template_id.as_deref(), Some("acc-test"));
        assert_eq!(
            el.attrs.get(ATTR_SLOT).map(|s| s.as_str()),
            Some(SLOT_ROOT)
        );
    }

    #[test]
    fn builder_renders_items() {
        let (_h, el): (_, Element) = create_root(|| {
            accordion("acc-b")
                .default_value(vec!["x".into()])
                .item("x", "Q", || Element::new().text("A"))
                .into()
        });
        assert_eq!(el.template_id.as_deref(), Some("acc-b"));
        assert_eq!(el.children.len(), 1);
        assert_eq!(
            el.children[0].attrs.get(ATTR_SLOT).map(|s| s.as_str()),
            Some(SLOT_ITEM)
        );
    }

    #[test]
    fn accordion_content_macro_shell_has_panel_slot() {
        let open = use_signal("__acc_content_open", || vec!["v".into()]);
        let (_h, el) = create_root(|| {
            accordion_scope(
                "acc-m",
                Some(open),
                Vec::new(),
                false,
                false,
                None,
                move || {
                    accordion_item_scope("v", false, None, move || {
                        accordion_content(|| Element::new().text("body"))
                    })
                },
            )
        });
        let panel = el
            .children
            .first()
            .and_then(|item| find_slot_child(item, SLOT_CONTENT));
        assert!(panel.is_some(), "expected accordion content panel");
    }
}
