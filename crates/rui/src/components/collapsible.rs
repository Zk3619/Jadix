//! # Collapsible
//!
//! Compare ShadCN **base** [`collapsible.tsx`](../../../../reference/ShadCN-UI/apps/v4/registry/bases/base/ui/collapsible.tsx):
//!
//! ```text
//! Collapsible                      // Root：open / defaultOpen；可选 exclusive + group 互斥
//! ├── CollapsibleTrigger           // 点击切换；纯文本带 chevron，或包裹自定义子节点
//! └── CollapsibleContent           // Panel：overflow-hidden + `@keyframes collapsible-down/up`
//! ```
//!
//! Entry and exit matchup ShadCN community practice `collapsible-down` / `collapsible-up` (height 0 ↔ content-height, 200ms),
//! 与 Accordion 的 `accordion-down/up` **同形不同名**（官方 base 默认无动画；有动画时用独立 keyframes）。
//!
//! 间距对齐 ShadCN Root（普通块级容器，不是带 `gap` 的 flex）：Root 默认 `gap: 0`，
//! Trigger↔Content 间距走 Content `pb-4`。收起后 `display:none`，打开后仍在文档流里长高度，
//! 不把占位改成 `absolute`。
//!
//! `<Collapsible>` in `ui!` / `<CollapsibleContent>` is expanded by macro ([`collapsible_scope`] /
//![`collapsible_content`]). Builder [`collapsible`] Shared implementation with RSX.
//!
//! ## Usage (dual style)
//! ```ignore
//! // A · RSX (camelCase macro alias: defaultOpen/openSignal)
//! ui! {
//!     <Collapsible id="faq" defaultOpen={true}>
//!         <CollapsibleTrigger>"Can I use this in my project?"</CollapsibleTrigger>
//!         <CollapsibleContent>
//!             <Label>"Yes. Free for personal and commercial use."</Label>
//!         </CollapsibleContent>
//!     </Collapsible>
//! }
//!
//! // B · Builder (snake_case only)
//! collapsible("faq")
//!     .default_open(true)
//!     .trigger(|| Element::new().text("Can I use this in my project?"))
//!     .content(|| {
//!         label("").text("Yes. Free for personal and commercial use.").into()
//!     })
//!
//! // 同组互斥：`exclusive` + 共享 `group`（未设 group 时 exclusive 无效）
//! collapsible("a").exclusive(true).group("faq").default_open(true)…
//! collapsible("b").exclusive(true).group("faq")…
//! ```
//!
//! **Identity (WP-2.8)**: `id` optional; empty string → [`resolve_instance_id`].

use std::sync::OnceLock;

use rui_macros::component;
use taffy::style::{AlignItems, Display, FlexDirection, JustifyContent};

use crate::app::reactive::{reactive_anchor, reactive_placeholder_in_flow};
use crate::components::base::{ensure_data_slot, overlay_root_author_text, SlotChrome};
use crate::core::reactivity::{
    create_context, create_effect, on_cleanup, provide_context, use_context, use_signal,
    with_child_owner, Context, Signal,
};
use crate::core::tree::{parse_text_color, Element, LayoutStatus};
use crate::impl_simple_slot_builder;
use crate::impl_slot_chrome_methods;
use crate::layout::layout_tree::LayoutTree;
use crate::primitives::interactive::interactive;
use crate::primitives::r#box::{hstack, vstack};
use crate::style::{
    css_transition_needs_frame, TimingFunction, TransitionEntry, TransitionProperty,
};
use crate::tokens;
use crate::utils::collapse_panel::{
    apply_collapse_motion, sync_collapse_panel_presence, use_collapse_panel_presence,
    CollapseKeyframes, CollapsePanelPresence,
};
use crate::utils::motion::ensure_motion_transitions;
use crate::utils::resolve_instance_id;
use crate::utils::take_fragment;
use crate::utils::text_node::{build_lucide_icon_text_node, build_text_node, TextNodeConfig};
use crate::utils::toggle_state::{resolve_bool_signal, write_bool_toggle};

const ATTR_SLOT: &str = "data-slot";
const SLOT_ROOT: &str = "collapsible";
const SLOT_TRIGGER: &str = "collapsible-trigger";
const SLOT_TRIGGER_ICON: &str = "collapsible-trigger-icon";
const SLOT_CONTENT: &str = "collapsible-content";
const SLOT_CONTENT_INNER: &str = "collapsible-content-inner";

const COLLAPSE_MS: f32 = crate::utils::collapse_panel::CollapseKeyframes::COLLAPSIBLE.duration_ms;
const CHEVRON_PX: f32 = 16.0;
const CHEVRON_OPEN_DEG: f32 = 180.0;
const TRIGGER_WEIGHT: u16 = 500;
const CONTENT_WEIGHT: u16 = 400;

// ── Context ──────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct CollapsibleCtx {
    pub id: String,
    pub open: Signal<bool>,
    pub disabled: bool,
}

fn collapsible_context() -> Context<CollapsibleCtx> {
    static CTX: OnceLock<Context<CollapsibleCtx>> = OnceLock::new();
    *CTX.get_or_init(create_context::<CollapsibleCtx>)
}

pub fn use_collapsible() -> Option<CollapsibleCtx> {
    use_context(collapsible_context())
}

fn require_collapsible(part: &str) -> CollapsibleCtx {
    use_collapsible().unwrap_or_else(|| {
        panic!("{part} must be used within <Collapsible> / collapsible_scope");
    })
}

fn apply_collapse_panel_motion(
    el: &mut Element,
    layout_tree: &mut LayoutTree,
    presence: &CollapsePanelPresence,
) -> LayoutStatus {
    apply_collapse_motion(el, layout_tree, presence, CollapseKeyframes::COLLAPSIBLE)
}

fn chevron_transition_entries() -> [TransitionEntry; 1] {
    [TransitionEntry {
        property: TransitionProperty::Transform,
        duration_ms: COLLAPSE_MS,
        delay_ms: 0.0,
        timing: TimingFunction::EaseOut,
    }]
}

fn stamp_content_text_weight(el: &mut Element) {
    crate::core::tree::walk_elements_mut(el, |node| {
        if node.text.is_some() {
            node.text_style.weight = Some(CONTENT_WEIGHT);
            node.attrs
                .insert("font_weight".to_string(), "1".to_string());
        }
    });
}

fn register_open_redraw_effect(open_sig: Signal<bool>) {
    // Each mount owns its observer; a named registration flag survives disposal.
    create_effect(move || {
        if open_sig.try_get().is_some() {
            crate::app::runtime::request_redraw();
        }
    });
}

fn exclusive_winner_signal(group: &str) -> Signal<Option<String>> {
    use_signal(
        &format!("__rui_collapsible_excl:{group}"),
        || None::<String>,
    )
}

/// 同 `group` 内最多一项 `open`。先声明且默认打开的占坑，其余关掉。
fn bind_collapsible_exclusive(id: String, group: String, open: Signal<bool>) {
    let winner = exclusive_winner_signal(&group);
    if open.try_peek() == Some(true) {
        match winner.try_peek() {
            Some(None) | None => {
                let _ = winner.try_set(Some(id.clone()));
            }
            Some(Some(other)) if other != id => {
                let _ = open.try_set(false);
            }
            _ => {}
        }
    }

    let id_from_open = id.clone();
    let winner_from_open = winner;
    create_effect(move || {
        let Some(is_open) = open.try_get() else {
            return;
        };
        let Some(cur) = winner_from_open.try_peek() else {
            return;
        };
        if is_open {
            if cur.as_deref() != Some(id_from_open.as_str()) {
                let _ = winner_from_open.try_set(Some(id_from_open.clone()));
            }
        } else if cur.as_deref() == Some(id_from_open.as_str()) {
            let _ = winner_from_open.try_set(None);
        }
    });

    let id_follow = id.clone();
    create_effect(move || {
        let Some(cur) = winner.try_get() else {
            return;
        };
        if let Some(w) = cur {
            if w != id_follow && open.try_peek() == Some(true) {
                let _ = open.try_set(false);
            }
        }
    });

    on_cleanup(move || {
        if winner.try_peek().as_ref().and_then(|c| c.as_deref()) == Some(id.as_str()) {
            let _ = winner.try_set(None);
        }
    });
}

fn sync_collapse_panel_open(open_sig: Signal<bool>, presence: &CollapsePanelPresence) {
    let want = open_sig.try_peek().unwrap_or(false);
    sync_collapse_panel_presence(presence, want);
}

fn chevron_icon() -> Element {
    build_lucide_icon_text_node("chevron-down")
        .text_size(CHEVRON_PX)
        .w(CHEVRON_PX)
        .h(CHEVRON_PX)
        .text_color(parse_text_color("muted-foreground"))
}

fn sync_collapse_chevron(el: &mut Element, open: bool) -> LayoutStatus {
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

fn is_text_leaf(el: &Element) -> bool {
    el.children.is_empty() && el.text.as_ref().is_some_and(|t| !t.is_empty())
}

fn caption_text(el: &Element) -> Option<String> {
    crate::core::tree::first_trimmed_text(el)
}

fn chain_click(mut el: Element, handler: impl FnMut() + 'static) -> Element {
    let prev = el.on_click.clone();
    let mut handler = handler;
    el = el.on_click(move || {
        if let Some(ref rc) = prev {
            rc.borrow_mut()();
        }
        handler();
    });
    el
}

// ── Scope ─────────────────────────────────────────────────────────────────────

/// 提供 Collapsible 上下文并构建根壳（`ui!` `<Collapsible>` / Builder 共用）。
///
/// `exclusive` 且 `group` 非空时，同组只允许一项打开。
pub fn collapsible_scope(
    id: impl Into<String>,
    open: Option<Signal<bool>>,
    default_open: bool,
    disabled: bool,
    exclusive: bool,
    group: Option<String>,
    body: impl FnOnce() -> Element,
) -> Element {
    let id = resolve_instance_id(id, "collapsible");
    let open = resolve_bool_signal(open, &id, "open", default_open);
    register_open_redraw_effect(open);
    if exclusive {
        if let Some(g) = group.filter(|s| !s.trim().is_empty()) {
            bind_collapsible_exclusive(id.clone(), g, open);
        }
    }
    let mut root: Element = with_child_owner(|| {
        provide_context(
            collapsible_context(),
            CollapsibleCtx {
                id: id.clone(),
                open,
                disabled,
            },
        );
        let kids = take_fragment(body());
        vstack().w("100%").min_w(0.0).gap(0.0).children(kids).into()
    });
    root.attrs
        .insert(ATTR_SLOT.to_string(), SLOT_ROOT.to_string());
    root.template_id = Some(id.clone());
    root
}

/// 宏 `<CollapsibleContent>`：按 Root `open` 显隐 + 进出场过渡。
pub fn collapsible_content(body: impl Fn() -> Element + 'static) -> Element {
    let ctx = require_collapsible("CollapsibleContent");
    let open_sig = ctx.open;
    let menu_id = ctx.id.clone();
    let anchor = format!("{menu_id}:content");
    let effect_anchor = anchor.clone();
    let presence = use_collapse_panel_presence(&format!("{menu_id}:presence"));
    let sync_presence = presence.clone();
    let mounted = presence.mounted();
    sync_collapse_panel_open(open_sig, &presence);
    let open_presence = presence.clone();
    create_effect(move || {
        let want = open_sig.try_get().unwrap_or(false);
        sync_collapse_panel_presence(&open_presence, want);
    });

    reactive_anchor(effect_anchor, move || {
        // Open/close drives presence without rebuilding still-mounted content.
        // Preserve child observers and the current animation clip on reversal.
        let want = open_sig.try_peek().unwrap_or(false);
        sync_collapse_panel_presence(&sync_presence, want);
        if mounted.try_get().is_none_or(|m| !m) {
            let mut hidden = Element::new();
            hidden.style.display = Display::None;
            hidden
                .attrs
                .insert(ATTR_SLOT.to_string(), SLOT_CONTENT.to_string());
            return hidden;
        }
        let mut body = body();
        if body.attrs.get("rui_fragment").map(|s| s.as_str()) == Some("1") {
            let kids = std::mem::take(&mut body.children);
            body = vstack().w("100%").min_w(0.0).children(kids).into();
        }
        stamp_content_text_weight(&mut body);
        // 与 Trigger 同左缘：ShadCN Content 仅 `pb-4 pt-0`（无水平 padding）；
        // padding 打在布局壳上，勿打在文本叶节点。
        let mut inner: Element = vstack()
            .w("100%")
            .min_w(0.0)
            .flex_shrink(0.0)
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
        wrap = wrap
            .text_size(14.0)
            .text_color(parse_text_color("muted-foreground"));
        wrap = wrap.accesskit_role("region");
        wrap.template_id = Some(format!("{menu_id}:panel"));
        let sync_presence = sync_presence.clone();
        wrap = wrap.reactive_sync(move |el, layout_tree| {
            overlay_root_author_text(el);
            apply_collapse_panel_motion(el, layout_tree, &sync_presence)
        });
        wrap
    });

    let mut panel = reactive_placeholder_in_flow(anchor, open_sig.try_peek().unwrap_or(false));
    panel
        .attrs
        .insert(ATTR_SLOT.to_string(), SLOT_CONTENT.to_string());
    panel
}

// ── RSX parts ─────────────────────────────────────────────────────────────────

/// ShadCN `CollapsibleTrigger`：点击切换；纯文本行带 chevron，自定义子节点则 chain click。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn CollapsibleTrigger(disabled: bool, children: Option<Element>) -> Element {
    let ctx = require_collapsible("CollapsibleTrigger");
    let item_disabled = disabled || ctx.disabled;
    let open_sig = ctx.open;
    let focus_id = format!("{}:trigger", ctx.id);
    let focus_for_toggle = focus_id.clone();

    let toggle = move || {
        if item_disabled {
            return;
        }
        use_signal("__rui_button_focus_id", String::new).set(focus_for_toggle.clone());
        write_bool_toggle(open_sig, false);
    };

    // 自定义子节点（如 Button）：只挂点击，不改外观。
    // Slot Builder `packed_children` 把单个文案叶包进 `rui_fragment`，须拆开才走 chevron 路径。
    let children = children.map(|mut c| {
        if c.attrs.get("rui_fragment").map(|s| s.as_str()) == Some("1") && c.children.len() == 1 {
            c.children.pop().unwrap()
        } else {
            c
        }
    });
    let label_text = match children {
        Some(c) if !is_text_leaf(&c) => {
            let mut trigger = c;
            if trigger.focus_id.is_none() {
                trigger = trigger.focus_id(focus_id.as_str());
            }
            let mut trigger = chain_click(trigger, toggle);
            if item_disabled {
                trigger = trigger.opacity(0.5).cursor_not_allowed();
            }
            trigger
                .attrs
                .insert(ATTR_SLOT.to_string(), SLOT_TRIGGER.to_string());
            return trigger;
        }
        Some(ref c) => caption_text(c).unwrap_or_else(|| {
            crate::tr_default_zh("rui.collapsible.toggle", "Toggle", "展开/折叠")
        }),
        None => crate::tr_default_zh("rui.collapsible.toggle", "Toggle", "展开/折叠"),
    };

    let mut chevron = chevron_icon();
    chevron
        .attrs
        .insert(ATTR_SLOT.to_string(), SLOT_TRIGGER_ICON.to_string());
    chevron = chevron.flex_shrink(0.0);
    let sync_open = open_sig;
    chevron = chevron.reactive_sync(move |el, _layout_tree| {
        let open = sync_open.try_peek().unwrap_or(false);
        sync_collapse_chevron(el, open)
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
        .items("center")
        .justify("between")
        .gap(tokens::SPACE_4)
        .py(tokens::SPACE_3)
        .font_weight(TRIGGER_WEIGHT as f32)
        .child(caption)
        .child(chevron)
        .into();
    trigger_row
        .attrs
        .insert(ATTR_SLOT.to_string(), SLOT_TRIGGER.to_string());
    trigger_row.style.flex_direction = FlexDirection::Row;
    trigger_row.style.align_items = Some(AlignItems::CENTER);
    trigger_row.style.justify_content = Some(JustifyContent::SPACE_BETWEEN);

    let mut trigger: Element = interactive(trigger_row)
        .focus_id(focus_id.clone())
        .focusable(!item_disabled)
        .focus_ring(!item_disabled)
        .on_click(toggle)
        .into();
    trigger = trigger.accesskit_role("button");
    trigger.template_id = Some(focus_id);
    if item_disabled {
        trigger = trigger.opacity(0.5).cursor_not_allowed();
    } else {
        trigger = trigger.cursor_kind(crate::core::tree::CursorKind::Pointer);
    }

    let mut host: Element = hstack().w("100%").min_w(0.0).child(trigger).into();
    host.attrs
        .insert(ATTR_SLOT.to_string(), SLOT_TRIGGER.to_string());
    host
}

/// 非宏路径：子节点已构建时用 reactive_sync 显隐（RSX 走 [`collapsible_content`]）。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn CollapsibleContent(children: Option<Element>) -> Element {
    let ctx = require_collapsible("CollapsibleContent");
    let open_sig = ctx.open;
    let presence = use_collapse_panel_presence(&format!("{}:presence", ctx.id));
    sync_collapse_panel_open(open_sig, &presence);

    let mut body = children.unwrap_or_else(Element::new);
    if body.attrs.get("rui_fragment").map(|s| s.as_str()) == Some("1") {
        let kids = std::mem::take(&mut body.children);
        body = vstack().w("100%").min_w(0.0).children(kids).into();
    }
    stamp_content_text_weight(&mut body);
    let mut inner: Element = vstack()
        .w("100%")
        .min_w(0.0)
        .flex_shrink(0.0)
        .pb(tokens::SPACE_4)
        .child(body)
        .into();
    inner
        .attrs
        .insert(ATTR_SLOT.to_string(), SLOT_CONTENT_INNER.to_string());
    inner = inner
        .text_size(14.0)
        .text_color(parse_text_color("muted-foreground"));

    let mut panel: Element = vstack().w("100%").min_w(0.0).min_h(0.0).child(inner).into();
    panel
        .attrs
        .insert(ATTR_SLOT.to_string(), SLOT_CONTENT.to_string());
    panel = panel.overflow_y("hidden");
    if !open_sig.try_peek().unwrap_or(false) {
        panel.style.display = Display::None;
    }
    panel.template_id = Some(format!("{}:panel", ctx.id));

    let sync_presence = presence.clone();
    panel = panel.reactive_sync(move |el, layout_tree| {
        overlay_root_author_text(el);
        let want = open_sig.try_peek().unwrap_or(false);
        sync_collapse_panel_presence(&sync_presence, want);
        apply_collapse_panel_motion(el, layout_tree, &sync_presence)
    });
    panel = panel.accesskit_role("region");
    panel
}

/// RSX 备用入口（无宏特殊展开时）；复合面请走宏 → [`collapsible_scope`]。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn Collapsible(
    id: String,
    open: Option<Signal<bool>>,
    default_open: bool,
    disabled: bool,
    exclusive: bool,
    group: Option<String>,
    children: Option<Element>,
) -> Element {
    collapsible_scope(
        id,
        open,
        default_open,
        disabled,
        exclusive,
        group,
        move || children.unwrap_or_else(Element::new),
    )
}

// ── Builder ───────────────────────────────────────────────────────────────────

pub struct CollapsibleBuilder {
    id: String,
    open: Option<Signal<bool>>,
    default_open: bool,
    disabled: bool,
    chrome: SlotChrome,
    exclusive: bool,
    group: Option<String>,
    trigger: Option<Box<dyn FnOnce() -> Element>>,
    content: Option<Box<dyn Fn() -> Element>>,
    content_slot: Option<Box<dyn FnOnce() -> Element>>,
}

/// Composite Collapsible entry (opposite `<Collapsible>`).
pub fn collapsible(id: impl Into<String>) -> CollapsibleBuilder {
    CollapsibleBuilder {
        id: id.into(),
        open: None,
        default_open: false,
        disabled: false,
        chrome: SlotChrome::default(),
        exclusive: false,
        group: None,
        trigger: None,
        content: None,
        content_slot: None,
    }
}

impl CollapsibleBuilder {
    impl_slot_chrome_methods!();

    pub fn open_signal(mut self, s: Signal<bool>) -> Self {
        self.open = Some(s);
        self
    }

    /// Same as [`Self::open_signal`].
    pub fn open(self, s: Signal<bool>) -> Self {
        self.open_signal(s)
    }

    pub fn default_open(mut self, v: bool) -> Self {
        self.default_open = v;
        self
    }

    pub fn disabled(mut self, v: bool) -> Self {
        self.disabled = v;
        self
    }

    /// 与同 [`Self::group`] 的其它 Collapsible 互斥（组键为空时无效）。
    pub fn exclusive(mut self, v: bool) -> Self {
        self.exclusive = v;
        self
    }

    pub fn group(mut self, g: impl Into<String>) -> Self {
        let g = g.into();
        self.group = if g.trim().is_empty() { None } else { Some(g) };
        self
    }

    pub fn trigger(mut self, build: impl FnOnce() -> Element + 'static) -> Self {
        self.trigger = Some(Box::new(build));
        self
    }

    pub fn trigger_with(
        self,
        f: impl FnOnce(CollapsibleTriggerBuilder) -> CollapsibleTriggerBuilder + 'static,
    ) -> Self {
        self.trigger(move || f(collapsible_trigger()).into())
    }

    pub fn content(mut self, build: impl Fn() -> Element + 'static) -> Self {
        self.content = Some(Box::new(build));
        self
    }

    pub fn content_with(
        mut self,
        f: impl FnOnce(CollapsibleContentSlotBuilder) -> CollapsibleContentSlotBuilder + 'static,
    ) -> Self {
        self.content_slot = Some(Box::new(move || f(collapsible_content_slot()).into()));
        self
    }

    /// 在上下文内构建任意子树（Trigger / Content 等）。
    pub fn child(self, body: impl FnOnce() -> Element) -> Element {
        self.chrome.apply(collapsible_scope(
            self.id,
            self.open,
            self.default_open,
            self.disabled,
            self.exclusive,
            self.group,
            body,
        ))
    }
}

impl_simple_slot_builder!(
    CollapsibleTriggerBuilder,
    collapsible_trigger,
    CollapsibleTrigger,
    CollapsibleTriggerProps
);
impl_simple_slot_builder!(
    CollapsibleContentSlotBuilder,
    collapsible_content_slot,
    CollapsibleContent,
    CollapsibleContentProps
);

impl From<CollapsibleBuilder> for Element {
    fn from(builder: CollapsibleBuilder) -> Self {
        let CollapsibleBuilder {
            id,
            open,
            default_open,
            disabled,
            chrome,
            exclusive,
            group,
            trigger,
            content,
            content_slot,
        } = builder;
        chrome.apply(collapsible_scope(
            id,
            open,
            default_open,
            disabled,
            exclusive,
            group,
            move || {
                let mut kids = Vec::new();
                if let Some(t) = trigger {
                    kids.push(ensure_data_slot(t(), SLOT_TRIGGER, |el| {
                        collapsible_trigger().child(el).into()
                    }));
                }
                if let Some(c) = content_slot {
                    kids.push(c());
                } else if let Some(c) = content {
                    kids.push(collapsible_content(c));
                }
                let mut frag = Element::new();
                frag.set_attr("rui_fragment", "1");
                frag.children(kids)
            },
        ))
    }
}

impl crate::core::tree::IntoElementList for CollapsibleBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::reactivity::create_root;

    #[test]
    fn scope_sets_slot_and_template() {
        let (_o, el): (_, Element) = create_root(|| {
            collapsible("col-test")
                .default_open(true)
                .trigger(|| Element::new().text("Toggle"))
                .content(|| Element::new().text("Body"))
                .into()
        });
        assert_eq!(el.attrs.get(ATTR_SLOT).map(|s| s.as_str()), Some(SLOT_ROOT));
        assert_eq!(el.template_id.as_deref(), Some("col-test"));
    }

    #[test]
    fn builder_trigger_class_and_style() {
        let (_o, el): (_, Element) = create_root(|| {
            collapsible("col-chrome")
                .trigger_with(|t| t.class("px-2").style(|b| b.text_size(20.0)).text("Toggle"))
                .content(|| Element::new().text("Body"))
                .into()
        });
        let trigger = find_slot(&el, SLOT_TRIGGER).expect("trigger");
        fn find_size(el: &Element, want: f32) -> bool {
            el.text_style.size.is_some_and(|s| (s - want).abs() < 0.01)
                || (el.text_style.size_resolved() - want).abs() < 0.01
                || el.children.iter().any(|c| find_size(c, want))
        }
        assert!(
            find_size(trigger, 20.0),
            "trigger .style(|b| b.text_size(20)) must land on the trigger tree"
        );
    }

    fn find_slot<'a>(el: &'a Element, slot: &str) -> Option<&'a Element> {
        if el.attrs.get(ATTR_SLOT).map(|s| s.as_str()) == Some(slot) {
            return Some(el);
        }
        el.children.iter().find_map(|c| find_slot(c, slot))
    }

    fn find_icon_name<'a>(el: &'a Element, name: &str) -> Option<&'a Element> {
        if el
            .get_attr(crate::utils::ATTR_ICON_NAME)
            .map(|s| s.as_str())
            == Some(name)
        {
            return Some(el);
        }
        el.children.iter().find_map(|c| find_icon_name(c, name))
    }

    #[test]
    fn trigger_chevron_is_named_lucide_for_square_svg_rotate() {
        let (_o, el): (_, Element) = create_root(|| {
            collapsible("col-chev")
                .trigger(|| Element::new().text("Toggle"))
                .content(|| Element::new().text("Body"))
                .into()
        });
        let icon = find_icon_name(&el, "chevron-down").expect("collapsible chevron");
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
    fn dual_surface_compiles_shape() {
        let _b: Element = collapsible("x").default_open(false).into();
        let el = Collapsible(CollapsibleProps {
            id: "y".into(),
            open: None,
            default_open: Some(true),
            disabled: Some(false),
            exclusive: Some(false),
            group: None,
            children: None,
            base: None,
            ..Default::default()
        });
        let _: Element = el;
    }

    #[test]
    fn closed_content_is_display_none() {
        let (_o, el): (_, Element) = create_root(|| {
            collapsible("col-hidden")
                .default_open(false)
                .trigger(|| Element::new().text("Toggle"))
                .content(|| Element::new().text("Body"))
                .into()
        });
        let panel = find_slot(&el, SLOT_CONTENT).expect("content");
        assert_eq!(
            panel.style.display,
            Display::None,
            "closed panel matches CSS hidden — not a 0×0 flex sibling"
        );
    }

    #[test]
    fn open_content_placeholder_stays_in_flow() {
        let (_o, el): (_, Element) = create_root(|| {
            collapsible("col-open")
                .default_open(true)
                .trigger(|| Element::new().text("Toggle"))
                .content(|| Element::new().text("Body"))
                .into()
        });
        let panel = find_slot(&el, SLOT_CONTENT).expect("content");
        assert_ne!(
            panel.style.display,
            Display::None,
            "open panel must stay in-flow so height animation can run"
        );
    }

    #[test]
    fn exclusive_group_keeps_one_open() {
        let (_o, ()) = create_root(|| {
            let a = use_signal("col_excl_a", || true);
            let b = use_signal("col_excl_b", || true);
            let _: Element = collapsible("col-excl-a")
                .open(a)
                .exclusive(true)
                .group("col-excl-g")
                .default_open(true)
                .trigger(|| Element::new().text("A"))
                .content(|| Element::new().text("a"))
                .into();
            let _: Element = collapsible("col-excl-b")
                .open(b)
                .exclusive(true)
                .group("col-excl-g")
                .default_open(true)
                .trigger(|| Element::new().text("B"))
                .content(|| Element::new().text("b"))
                .into();
            assert!(
                a.peek() ^ b.peek(),
                "first default-open panel should occupy the exclusive group"
            );
            a.set(true);
            assert!(a.peek(), "forcing A open");
            assert!(!b.peek(), "B must close when A claims the group");
            b.set(true);
            assert!(b.peek());
            assert!(!a.peek());
        });
    }

    #[test]
    fn enter_anim_seeds_after_measure_not_on_empty_bounds() {
        use crate::layout::layout_tree::LayoutTree;
        let (_o, ()) = create_root(|| {
            let p = use_collapse_panel_presence("col_enter_measure");
            sync_collapse_panel_presence(&p, false);
            sync_collapse_panel_presence(&p, true);
            assert!(p.needs_enter());
            let mut el = Element::new();
            el.layout_bounds = [0.0, 0.0, 100.0, 0.0];
            let mut tree = LayoutTree::new();
            let _ = apply_collapse_panel_motion(&mut el, &mut tree, &p);
            assert!(
                p.needs_enter(),
                "must keep needs_enter until height is known"
            );
            assert!(
                !el.style
                    .animations
                    .iter()
                    .any(|a| a.name == CollapseKeyframes::COLLAPSIBLE.down),
                "must not seed down with 0 height"
            );
            el.layout_bounds = [0.0, 0.0, 100.0, 64.0];
            let _ = apply_collapse_panel_motion(&mut el, &mut tree, &p);
            assert!(!p.needs_enter());
            assert!(
                el.style
                    .animations
                    .iter()
                    .any(|a| a.name == CollapseKeyframes::COLLAPSIBLE.down),
                "first real expand must run collapsible-down"
            );
        });
    }
}
