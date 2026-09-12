//! # Accordion
//!
//! Compare ShadCN **base** [`accordion.tsx`](../../../../reference/ShadCN-UI/apps/v4/registry/bases/base/ui/accordion.tsx):
//!
//! ```text
//! Accordion                      // Root：single | multiple；value 为已展开项 id 列表
//! └── AccordionItem              // value + border-b
//! ├── AccordionTrigger // Header + chevron (expand rotation)
//!     └── AccordionContent       // Panel：overflow-hidden + `@keyframes accordion-down/up`
//!                                //     或 `RUI_ACCORDION_GRID_FR=1` → `grid-template-rows: 0fr→1fr`
//! ```
//!
//! Entry and exit alignment ShadCN `accordion-down` / `accordion-up` (height 0 ↔ content-height, 200ms ease-out):
//! Same as [`dialog`](super::dialog), using **CSS `@keyframes`** + `AnimationSnapshot` to advance, without MotionStyle handwritten interpolation.
//! Chevron is the `transform` (rotate) CSS transition.
//!
//! 样式对位 `cn-accordion` / `cn-accordion-item` / `cn-accordion-trigger` /
//! `cn-accordion-content`：`flex w-full flex-col` · 项 `border-b` · 触发器 `py-4 text-sm font-medium`
//! · Content `pb-4 text-sm font-normal text-muted-foreground`.
//!
//! `<Accordion>` in `ui!` / `<AccordionContent>` is expanded by macro ([`accordion_scope`] /
//![`accordion_content`]). Builder [`accordion`] Shared implementation with RSX.
//!
//! ## Usage (dual style)
//! ```ignore
//! // A · RSX (camelCase macro alias: defaultValue / valueSignal)
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
//! // B · Builder (snake_case only)
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
//! **Identity (WP-2.8)**: `id` optional; empty string → [`resolve_instance_id`].

use std::cell::Cell;
use std::sync::OnceLock;

use rui_macros::component;
use taffy::style::{AlignItems, Dimension, Display, FlexDirection, JustifyContent, Overflow};

use crate::app::reactive::{reactive_anchor, reactive_placeholder_sized};
use crate::components::base::{
    overlay_root_author_text, packed_children, BaseComponentBuilder, SlotChrome,
};
use crate::core::reactivity::{
    create_context, create_effect, provide_context, use_context, use_scoped_signal, use_signal,
    with_child_owner, Context, Signal,
};
use crate::core::tree::{parse_color, parse_text_color, Element, LayoutStatus};
use crate::impl_slot_chrome_methods;
use crate::layout::layout_tree::LayoutTree;
use crate::layout::{first_track_fr, set_first_track_fr};
use crate::primitives::interactive::interactive;
use crate::primitives::r#box::{hstack, vstack};
use crate::style::{
    css_transition_needs_frame, with_class_list, TimingFunction, TransitionEntry,
    TransitionProperty,
};
use crate::tokens;
use crate::utils::collapse_panel::{
    apply_collapse_motion, sync_collapse_panel_presence, use_collapse_panel_presence,
    CollapseKeyframes, CollapsePanelPresence,
};
use crate::utils::motion::{ensure_motion_transitions, motion_is_animating};
use crate::utils::presence::PresencePhase;
use crate::utils::resolve_instance_id;
use crate::utils::roving::{attach_focus_roving, FocusRovingAxis};
use crate::utils::take_fragment;
use crate::utils::text_node::{build_lucide_icon_text_node, build_text_node, TextNodeConfig};

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
    static ACCORDION_USE_GRID_FR: Cell<Option<bool>> = const { Cell::new(None) };
}

/// Override Accordion panel motion to CSS `grid-template-rows: 0fr→1fr` (ANIM A3.6).
/// `None` restores auto-detect via `RUI_ACCORDION_GRID_FR`.
pub fn set_accordion_use_grid_fr(on: Option<bool>) {
    ACCORDION_USE_GRID_FR.set(on);
}

/// Whether Accordion content uses the `grid-template-rows` transition path.
pub fn accordion_use_grid_fr() -> bool {
    if let Some(v) = ACCORDION_USE_GRID_FR.get() {
        return v;
    }
    match std::env::var("RUI_ACCORDION_GRID_FR") {
        Ok(s) if s == "1" || s.eq_ignore_ascii_case("true") => true,
        _ => false,
    }
}

fn use_accordion_panel_presence(key: &str) -> CollapsePanelPresence {
    use_collapse_panel_presence(key)
}

fn sync_accordion_panel_presence(handle: &CollapsePanelPresence, want: bool) -> PresencePhase {
    sync_collapse_panel_presence(handle, want)
}

fn chevron_transition_entries() -> [TransitionEntry; 1] {
    [TransitionEntry {
        property: TransitionProperty::Transform,
        duration_ms: ACCORDION_MS,
        delay_ms: 0.0,
        timing: TimingFunction::EaseOut,
    }]
}

fn reset_accordion_anim_snapshot(el: &mut Element) {
    el.animation_snapshot = Default::default();
}

/// 内容区正文统一 normal(400)；覆盖 `<Label>` 默认 medium(500) 以免比 Trigger 更粗。
fn stamp_accordion_content_text_weight(el: &mut Element) {
    if el.text.is_some() {
        el.text_style.weight = Some(CONTENT_WEIGHT);
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
            if open_sig.is_alive() {
                let _ = open_sig.get();
            }
            crate::app::runtime::request_redraw();
        });
        ()
    });
}

fn sync_accordion_panel_open(
    open_sig: Signal<Vec<String>>,
    value: &str,
    presence: &CollapsePanelPresence,
) {
    let want = open_sig
        .try_peek()
        .map(|open| is_item_open(&open, value))
        .unwrap_or(false);
    sync_accordion_panel_presence(presence, want);
}

fn caption_text(el: &Element) -> Option<String> {
    crate::core::tree::first_trimmed_text(el)
}

fn chevron_icon() -> Element {
    build_lucide_icon_text_node("chevron-down")
        .text_size(CHEVRON_PX)
        .w(CHEVRON_PX)
        .h(CHEVRON_PX)
        .text_color(parse_text_color("muted-foreground"))
}

#[cfg(test)]
fn find_slot_child<'a>(el: &'a Element, slot: &str) -> Option<&'a Element> {
    crate::core::tree::find_by_attr(el, ATTR_SLOT, slot)
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
        let mut root: Element = vstack().w("100%").min_w(0.0).gap(0.0).children(kids).into();
        if let Some(c) = class.filter(|s| !s.trim().is_empty()) {
            root = with_class_list(root, &c);
        }
        root
    });
    root.attrs
        .insert(ATTR_SLOT.to_string(), SLOT_ROOT.to_string());
    root.template_id = Some(id.clone());
    attach_focus_roving(&mut root, FocusRovingAxis::Vertical);
    root
}

/// 宏 `<AccordionContent>`：按 Item 上下文显隐 + 进出场过渡。
///
/// 显隐由 [`reactive_anchor`] + [`ExitGate`] 负责：收起时保持挂载至退场动画结束
/// (Compare Dialog/Sheet presence; do not apply reactive_sync of `Display::None` to placeholder).
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
    let mounted = presence.mounted();
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
        let mut body = body();
        if body.attrs.get("rui_fragment").map(|s| s.as_str()) == Some("1") {
            let kids = std::mem::take(&mut body.children);
            body = vstack().w("100%").min_w(0.0).children(kids).into();
        }
        stamp_accordion_content_text_weight(&mut body);
        // padding 必须在布局壳上（勿打在文本叶）：ShadCN `pb-4 pt-0`，无水平 padding，
        // 与 Trigger 的 `py-4`（无 px）左缘对齐；且须在高度动画外壳之内。
        let mut inner: Element = vstack()
            .w("100%")
            .min_w(0.0)
            .min_h(0.0)
            .overflow_y("hidden")
            .pb(tokens::SPACE_4)
            .child(body)
            .into();
        inner
            .attrs
            .insert(ATTR_SLOT.to_string(), SLOT_CONTENT_INNER.to_string());
        let mut wrap: Element = vstack()
            .w("100%")
            .min_w(0.0)
            .min_h(0.0)
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
            overlay_root_author_text(el);
            apply_accordion_panel_motion(el, layout_tree, &sync_presence)
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

fn apply_accordion_panel_motion(
    el: &mut Element,
    layout_tree: &mut LayoutTree,
    presence: &CollapsePanelPresence,
) -> LayoutStatus {
    if accordion_use_grid_fr() {
        return apply_accordion_panel_motion_grid(el, layout_tree, presence);
    }
    apply_collapse_motion(el, layout_tree, presence, CollapseKeyframes::ACCORDION)
}

fn grid_rows_transition_entry(timing: TimingFunction) -> TransitionEntry {
    TransitionEntry {
        property: TransitionProperty::GridTemplateRows,
        duration_ms: ACCORDION_MS,
        delay_ms: 0.0,
        timing,
    }
}

fn apply_grid_panel_shell(el: &mut Element) -> bool {
    let mut layout = false;
    if el.style.display != Display::Grid {
        el.style.display = Display::Grid;
        layout = true;
    }
    if el.style.overflow.y != Overflow::Hidden {
        el.style.overflow.y = Overflow::Hidden;
        layout = true;
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
    if el.style.max_size.height != Dimension::auto() {
        el.style.max_size.height = Dimension::auto();
        layout = true;
    }
    if (el.style.opacity - 1.0).abs() > 0.001 {
        el.style.opacity = 1.0;
    }
    if el.style.scale != [1.0, 1.0] {
        el.style.scale = [1.0, 1.0];
    }
    for child in &mut el.children {
        if child.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(SLOT_CONTENT_INNER) {
            if child.style.min_size.height != Dimension::length(0.0) {
                child.style.min_size.height = Dimension::length(0.0);
                layout = true;
            }
            if child.style.overflow.y != Overflow::Hidden {
                child.style.overflow.y = Overflow::Hidden;
                layout = true;
            }
        }
    }
    layout
}

fn accordion_has_grid_exit(el: &Element) -> bool {
    el.style
        .transition
        .iter()
        .any(|e| e.property == TransitionProperty::GridTemplateRows)
        && first_track_fr(&el.style.grid_template_rows).is_some_and(|fr| fr < 0.999)
}

fn seed_accordion_grid_fr(el: &mut Element, from_fr: f32, to_fr: f32, timing: TimingFunction) {
    el.style.animations.clear();
    reset_accordion_anim_snapshot(el);
    let _ = apply_grid_panel_shell(el);
    el.style.transition = vec![grid_rows_transition_entry(timing)];
    set_first_track_fr(&mut el.style.grid_template_rows, from_fr);
    el.snap_css_transition_baseline();
    set_first_track_fr(&mut el.style.grid_template_rows, to_fr);
    let _ = el.tick_css_transitions(0.0);
}

fn apply_accordion_panel_motion_grid(
    el: &mut Element,
    layout_tree: &mut LayoutTree,
    presence: &CollapsePanelPresence,
) -> LayoutStatus {
    let phase = presence.phase();
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

    let exiting = phase == PresencePhase::Exiting;
    let open_visual = phase == PresencePhase::Present && !exiting;
    let mut layout = apply_grid_panel_shell(el);

    let (needs_enter, needs_exit_anim) = presence.begin_frame_flags(open_visual, exiting);
    let exit_armed = presence.exit_armed();
    if exiting && (needs_exit_anim || (!accordion_has_grid_exit(el) && exit_armed)) {
        let from = first_track_fr(&el.style.grid_template_rows)
            .unwrap_or(1.0)
            .max(0.0);
        seed_accordion_grid_fr(el, from, 0.0, TimingFunction::EaseIn);
        if el.layout_node.is_some() {
            layout_tree.update_taffy_node_style(el);
        }
        crate::app::damage::push_frame_paint_damage(el.layout_bounds);
        return LayoutStatus::LayoutChanged;
    }

    if exiting {
        if motion_is_animating(&el.style, &el.transition_snapshot) {
            crate::app::damage::push_frame_paint_damage(el.layout_bounds);
            return LayoutStatus::LayoutChanged;
        }
        el.style.animations.clear();
        set_first_track_fr(&mut el.style.grid_template_rows, 0.0);
        if el.layout_node.is_some() {
            layout_tree.update_taffy_node_style(el);
        }
        presence.finish_exit();
        crate::app::damage::push_frame_paint_damage(el.layout_bounds);
        return LayoutStatus::LayoutChanged;
    }

    if open_visual && needs_enter {
        presence.clear_needs_enter();
        seed_accordion_grid_fr(el, 0.0, 1.0, TimingFunction::EaseOut);
        if el.layout_node.is_some() {
            layout_tree.update_taffy_node_style(el);
        }
        crate::app::damage::push_frame_paint_damage(el.layout_bounds);
        return LayoutStatus::LayoutChanged;
    }

    if open_visual {
        if motion_is_animating(&el.style, &el.transition_snapshot) {
            if el.layout_node.is_some() {
                layout_tree.update_taffy_node_style(el);
            }
            crate::app::damage::push_frame_paint_damage(el.layout_bounds);
            return LayoutStatus::LayoutChanged;
        }
        el.style.animations.clear();
        let fr = first_track_fr(&el.style.grid_template_rows).unwrap_or(0.0);
        if (fr - 1.0).abs() > 1e-3 {
            set_first_track_fr(&mut el.style.grid_template_rows, 1.0);
            layout = true;
        }
        presence.clear_enter_active();
    }

    if layout && el.layout_node.is_some() {
        layout_tree.update_taffy_node_style(el);
        crate::app::damage::push_frame_paint_damage(el.layout_bounds);
        return LayoutStatus::LayoutChanged;
    }

    LayoutStatus::Clean
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
    item.attrs
        .insert(ATTR_SLOT.to_string(), SLOT_ITEM.to_string());
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
    accordion_item_scope(value, disabled, class, move || {
        children.unwrap_or_else(Element::new)
    })
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
        let open = sync_open
            .try_peek()
            .map(|v| is_item_open(&v, &sync_value))
            .unwrap_or(false);
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
    caption = caption
        .flex_grow(1.0)
        .flex_shrink(1.0)
        .min_w(0.0)
        .text_left();

    let mut trigger_row: Element = hstack()
        .w("100%")
        .min_w(0.0)
        .items("start")
        .justify("between")
        .gap(tokens::SPACE_4)
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

    let wrap: Element = hstack().w("100%").min_w(0.0).child(trigger).into();
    wrap.reactive_sync(move |el, _| {
        overlay_root_author_text(el);
        LayoutStatus::Clean
    })
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

    let mut body = children.unwrap_or_else(Element::new);
    if body.attrs.get("rui_fragment").map(|s| s.as_str()) == Some("1") {
        let kids = std::mem::take(&mut body.children);
        body = vstack().w("100%").min_w(0.0).children(kids).into();
    }
    stamp_accordion_content_text_weight(&mut body);
    let mut inner: Element = vstack()
        .w("100%")
        .min_w(0.0)
        .pb(tokens::SPACE_4)
        .child(body)
        .into();
    inner
        .attrs
        .insert(ATTR_SLOT.to_string(), SLOT_CONTENT_INNER.to_string());
    inner = inner
        .text_size(14.0)
        .text_color(parse_text_color("muted-foreground"));
    if let Some(c) = class.filter(|s| !s.trim().is_empty()) {
        inner = crate::style::with_class_list(inner, &c);
    }

    let mut panel: Element = vstack().w("100%").min_w(0.0).min_h(0.0).child(inner).into();
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
        overlay_root_author_text(el);
        let want = open_sync
            .try_peek()
            .map(|v| is_item_open(&v, &value_for_presence))
            .unwrap_or(false);
        sync_accordion_panel_presence(&sync_presence, want);
        apply_accordion_panel_motion(el, layout_tree, &sync_presence)
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

pub struct AccordionTriggerBuilder {
    disabled: bool,
    children: Vec<Element>,
    chrome: SlotChrome,
}

pub fn accordion_trigger() -> AccordionTriggerBuilder {
    AccordionTriggerBuilder {
        disabled: false,
        children: Vec::new(),
        chrome: SlotChrome::default(),
    }
}

impl AccordionTriggerBuilder {
    impl_slot_chrome_methods!();

    pub fn disabled(mut self, v: bool) -> Self {
        self.disabled = v;
        self
    }

    pub fn text(mut self, content: impl Into<String>) -> Self {
        self.children.push(Element::new().text(content.into()));
        self
    }

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children
            .extend(crate::utils::take_packed(child.into()));
        self
    }
}

impl From<AccordionTriggerBuilder> for Element {
    fn from(builder: AccordionTriggerBuilder) -> Self {
        let el = AccordionTrigger(AccordionTriggerProps {
            disabled: Some(builder.disabled),
            class: None,
            children: packed_children(builder.children),
            base: None,
            ..Default::default()
        });
        builder.chrome.apply(el)
    }
}

pub struct AccordionContentBuilder {
    children: Vec<Element>,
    chrome: SlotChrome,
}

impl AccordionContentBuilder {
    impl_slot_chrome_methods!();

    pub fn text(mut self, content: impl Into<String>) -> Self {
        self.children.push(Element::new().text(content.into()));
        self
    }

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children
            .extend(crate::utils::take_packed(child.into()));
        self
    }
}

impl From<AccordionContentBuilder> for Element {
    fn from(builder: AccordionContentBuilder) -> Self {
        let class = {
            let c = builder.chrome.class.trim();
            if c.is_empty() {
                None
            } else {
                Some(c.to_string())
            }
        };
        let el = AccordionContent(AccordionContentProps {
            class,
            children: packed_children(builder.children),
            base: None,
            ..Default::default()
        });
        if builder.chrome.style.is_some() {
            builder.chrome.apply(el)
        } else {
            el
        }
    }
}

pub struct AccordionItemBuilder {
    value: String,
    disabled: bool,
    chrome: SlotChrome,
    trigger: Option<AccordionTriggerBuilder>,
    content: Option<AccordionContentBuilder>,
}

pub fn accordion_item(value: impl Into<String>) -> AccordionItemBuilder {
    AccordionItemBuilder {
        value: value.into(),
        disabled: false,
        chrome: SlotChrome::default(),
        trigger: None,
        content: None,
    }
}

impl AccordionItemBuilder {
    impl_slot_chrome_methods!();

    pub fn disabled(mut self, v: bool) -> Self {
        self.disabled = v;
        self
    }

    pub fn trigger(mut self, text: impl Into<String>) -> Self {
        self.trigger = Some(accordion_trigger().text(text));
        self
    }

    pub fn trigger_with(
        mut self,
        f: impl FnOnce(AccordionTriggerBuilder) -> AccordionTriggerBuilder,
    ) -> Self {
        self.trigger = Some(f(accordion_trigger()));
        self
    }

    pub fn content(mut self, child: impl Into<Element>) -> Self {
        self.content = Some(AccordionContentBuilder {
            children: crate::utils::take_packed(child.into()),
            chrome: SlotChrome::default(),
        });
        self
    }

    pub fn content_with(
        mut self,
        f: impl FnOnce(AccordionContentBuilder) -> AccordionContentBuilder,
    ) -> Self {
        self.content = Some(f(AccordionContentBuilder {
            children: Vec::new(),
            chrome: SlotChrome::default(),
        }));
        self
    }
}

impl From<AccordionItemBuilder> for Element {
    fn from(builder: AccordionItemBuilder) -> Self {
        let class = if builder.chrome.class.trim().is_empty() {
            None
        } else {
            Some(builder.chrome.class.clone())
        };
        let style = builder.chrome.style.clone();
        let trigger = builder.trigger;
        let content = builder.content;
        let mut el = accordion_item_scope(builder.value, builder.disabled, class, move || {
            let mut kids = Vec::new();
            if let Some(t) = trigger {
                kids.push(t.into());
            }
            if let Some(c) = content {
                kids.push(c.into());
            }
            packed_children(kids).unwrap_or_else(Element::new)
        });
        if let Some(style) = style {
            el = style(BaseComponentBuilder::new(el)).el;
        }
        el
    }
}

pub struct AccordionBuilder {
    id: String,
    value: Option<Signal<Vec<String>>>,
    default_value: Vec<String>,
    multiple: bool,
    disabled: bool,
    class: Option<String>,
    style: Option<crate::components::base::SlotStyleFn>,
    items: Vec<AccordionItemSpec>,
    custom_items: Vec<Box<dyn FnOnce() -> Element>>,
    body: Option<Box<dyn FnOnce() -> Element>>,
}

/// Composite Accordion entry (opposite `<Accordion>`).
pub fn accordion(id: impl Into<String>) -> AccordionBuilder {
    AccordionBuilder {
        id: id.into(),
        value: None,
        default_value: Vec::new(),
        multiple: false,
        disabled: false,
        class: None,
        style: None,
        items: Vec::new(),
        custom_items: Vec::new(),
        body: None,
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

    pub fn style(
        mut self,
        f: impl Fn(BaseComponentBuilder) -> BaseComponentBuilder + 'static,
    ) -> Self {
        self.style = Some(std::rc::Rc::new(f));
        self
    }

    /// Compound surface: assemble [`AccordionItem`] / Trigger / Content inside scope.
    pub fn child(mut self, body: impl FnOnce() -> Element + 'static) -> Self {
        self.body = Some(Box::new(body));
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

    pub fn item_with(
        mut self,
        value: impl Into<String>,
        f: impl FnOnce(AccordionItemBuilder) -> AccordionItemBuilder + 'static,
    ) -> Self {
        let value = value.into();
        self.custom_items
            .push(Box::new(move || f(accordion_item(value)).into()));
        self
    }
}

impl crate::core::tree::IntoElementList for AccordionBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
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
            style,
            items,
            custom_items,
            body,
        } = builder;
        let mut el = accordion_scope(
            id,
            value,
            default_value,
            multiple,
            disabled,
            class,
            move || {
                if let Some(body) = body {
                    return body();
                }
                let mut out = Vec::with_capacity(items.len() + custom_items.len());
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
                for custom in custom_items {
                    out.push(custom());
                }
                packed_children(out).unwrap_or_else(Element::new)
            },
        );
        if let Some(style) = style {
            el = style(BaseComponentBuilder::new(el)).el;
        }
        el
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::reactivity::{create_effect, create_root, use_signal};

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
    fn accordion_exit_run_inactive_after_keyframes_finish() {
        let mut el = Element::new();
        el.layout_bounds = [0.0, 0.0, 100.0, 48.0];
        let bounds = el.layout_bounds;
        el.used_layout.stamp_border_box(bounds, false);
        crate::utils::collapse_panel::seed_collapse_exit_anim(
            &mut el,
            crate::utils::CollapseKeyframes::ACCORDION,
        );
        assert!(
            crate::style::animation_run_active(&el.style, &el.animation_snapshot),
            "fresh accordion-up must count as animating"
        );
        for _ in 0..20 {
            let _ = el.tick_css_animations(0.016);
        }
        assert!(
            !crate::style::animation_run_active(&el.style, &el.animation_snapshot),
            "finished accordion-up must not keep LayoutChanged pumping"
        );
    }

    #[test]
    fn collapsed_panel_height_is_zero_when_padding_is_on_inner() {
        use crate::core::tree::LayoutTree;
        let (_h, _) = create_root(|| {
            create_effect(|| {
                let inner: Element = vstack()
                    .w("100%")
                    .pb(tokens::SPACE_4)
                    .child(
                        Element::new()
                            .text("Yes. Builder 与 RSX 共用 accordion_scope / accordion_content。"),
                    )
                    .into();
                let wrap: Element = vstack()
                    .w(320.0)
                    .min_w(0.0)
                    .min_h(0.0)
                    .overflow_y("hidden")
                    .max_h(0.0)
                    .child(inner)
                    .into();
                let mut root = Element::new().w(400.0).h(400.0).child(wrap);
                let mut layout = LayoutTree::new();
                let _ = layout.build_and_layout(&mut root, 400.0, 400.0, [-1.0, -1.0]);
                let h = root.children[0].layout_bounds[3];
                assert!(
                    h <= 1.0,
                    "clip shell with min-h:0 and padding on inner must collapse to ~0, got {h}"
                );
            });
        });
    }

    #[test]
    fn scope_sets_template_and_slot() {
        let (_h, el) = create_root(|| {
            accordion_scope(
                "acc-test",
                None,
                vec!["one".into()],
                false,
                false,
                None,
                || Element::new(),
            )
        });
        assert_eq!(el.template_id.as_deref(), Some("acc-test"));
        assert_eq!(el.attrs.get(ATTR_SLOT).map(|s| s.as_str()), Some(SLOT_ROOT));
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
    fn trigger_chevron_is_named_lucide_for_square_svg_rotate() {
        let (_h, el): (_, Element) = create_root(|| {
            accordion("acc-chev")
                .item("x", "Q", || Element::new().text("A"))
                .into()
        });
        let icon = find_slot_child(&el, SLOT_TRIGGER_ICON).expect("accordion chevron");
        assert_eq!(
            icon.get_attr(crate::utils::ATTR_ICON_NAME)
                .map(String::as_str),
            Some("chevron-down"),
            "named SVG texture keeps ink centered under 180° rotate"
        );
        assert_eq!(icon.text_style.font_family.as_deref(), Some("lucide"));
        assert!((icon.text_style.size_resolved() - CHEVRON_PX).abs() < 0.01);
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

    #[test]
    fn enter_anim_seeds_after_measure_not_on_empty_bounds() {
        use crate::core::tree::LayoutTree;
        let (_h, ()) = create_root(|| {
            let p = use_accordion_panel_presence("acc_enter_measure");
            sync_accordion_panel_presence(&p, false);
            sync_accordion_panel_presence(&p, true);
            assert!(p.needs_enter());
            let mut el = Element::new();
            el.layout_bounds = [0.0, 0.0, 100.0, 0.0];
            let mut tree = LayoutTree::new();
            let _ = apply_accordion_panel_motion(&mut el, &mut tree, &p);
            assert!(
                p.needs_enter(),
                "must keep needs_enter until height is known"
            );
            assert!(
                !el.style
                    .animations
                    .iter()
                    .any(|a| a.name == CollapseKeyframes::ACCORDION.down),
                "must not seed down with 0 height"
            );
            el.layout_bounds = [0.0, 0.0, 100.0, 64.0];
            let _ = apply_accordion_panel_motion(&mut el, &mut tree, &p);
            assert!(!p.needs_enter());
            assert!(
                el.style
                    .animations
                    .iter()
                    .any(|a| a.name == CollapseKeyframes::ACCORDION.down),
                "first real expand must run accordion-down"
            );
        });
    }

    #[test]
    fn grid_fr_enter_does_not_wait_for_used_height() {
        use crate::core::tree::LayoutTree;
        let (_h, ()) = create_root(|| {
            set_accordion_use_grid_fr(Some(true));
            struct Reset;
            impl Drop for Reset {
                fn drop(&mut self) {
                    set_accordion_use_grid_fr(None);
                }
            }
            let _reset = Reset;
            let p = use_accordion_panel_presence("acc_enter_grid_fr");
            sync_accordion_panel_presence(&p, false);
            sync_accordion_panel_presence(&p, true);
            assert!(p.needs_enter());
            let mut el = Element::new();
            el.layout_bounds = [0.0, 0.0, 100.0, 0.0];
            let mut tree = LayoutTree::new();
            let _ = apply_accordion_panel_motion(&mut el, &mut tree, &p);
            assert!(!p.needs_enter(), "grid-fr must not wait for used height");
            assert!(
                !el.style
                    .animations
                    .iter()
                    .any(|a| a.name == CollapseKeyframes::ACCORDION.down),
                "grid-fr path must not seed accordion-down"
            );
            assert_eq!(el.style.display, Display::Grid);
            assert!(
                el.style
                    .transition
                    .iter()
                    .any(|e| e.property == TransitionProperty::GridTemplateRows),
                "must attach grid-template-rows transition"
            );
            assert!(
                el.transition_snapshot.has_active_tweens(),
                "0fr→1fr tween must start on seed"
            );
            let used = first_track_fr(&el.style.grid_template_rows).unwrap_or(-1.0);
            assert!(used < 0.05, "tick(0) must leave used at 0fr, got {used}");
        });
    }
}
