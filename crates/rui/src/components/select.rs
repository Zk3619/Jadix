//! # Select（ShadCN React Aria 复合组件）
//!
//! Product surface & styles align with shadcn/ui **React Aria** base
//! (`registry/bases/aria/ui/select.tsx` + `cn-select-*` / `cn-select-*-aria`)：
//!
//! ```ignore
//! ui! {
//!   <Select id="fruit" value={sig} placeholder="Select a fruit">
//!     <SelectTrigger class="w-full">
//!       <SelectValue />
//!     </SelectTrigger>
//!     <SelectContent max_h="12rem">
//!       <SelectGroup>
//!         <SelectLabel>"Fruits"</SelectLabel>
//!         <SelectItem value="apple">"Apple"</SelectItem>
//!         <SelectItem value="banana">"Banana"</SelectItem>
//!       </SelectGroup>
//!     </SelectContent>
//!   </Select>
//! }
//! ```
//!
//! 便捷 Builder（单入口、内建 Trigger+Content）仍可用：
//! `select("id").item(…).trigger_width("100%").scroll_selected("center")`。
//!
//! **打开时滚动**：[`SelectBuilder::scroll_selected`] / `<Select scrollSelected>` /
//! `<SelectContent scrollSelected>`，取值 `none`（默认）| `start` | `center`。
//!
//! ## 浮层契约
//! 菜单走 [`crate::utils::anchored_overlay::anchored_overlay_content`]（Trigger 定位 + Expand）；
//! 几何由壳写入。**禁止**自管 left/top 信号，也禁止用岛刷 Trigger 文案（[`SelectValue`] 走 `bind_text`）。

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::app::portal_overlay::{OverlayKind, OverlayMode};
use crate::app::reactive::{bind_style, bind_text, StylePatch};
use crate::components::base::{author_class, paint_slot, SlotMap, SlotStyleFn};
use crate::components::button::button;
use crate::components::label::label;
use crate::components::scroll_area::scroll_area;
use crate::components::separator::separator;
use crate::core::reactivity::{
    create_context, provide_context, untrack, use_context, use_signal, with_child_owner, Context,
    Signal,
};
use crate::core::tree::Element;
use crate::impl_simple_slot_builder;
use crate::primitives::r#box::{hstack, vstack};
use crate::style::{parse_color, parse_text_color, with_class_list};
use crate::tokens;
use crate::utils::anchored_overlay::{
    anchored_overlay_content, provide_anchored_overlay_context, AnchoredOverlayContentOpts,
    FloatingMotionKind, FloatingStrategy, Placement,
};
use crate::utils::lucide_glyph;
use crate::utils::popup::{
    disarm_underlay_close, PopupPlacement, ATTR_FLOATING_H, ATTR_FLOATING_MATCH_TRIGGER,
    ATTR_FLOATING_MAX_H, ATTR_FLOATING_MIN_H, ATTR_FLOATING_MIN_W, ATTR_FLOATING_W,
    FLOATING_TRIGGER_GAP,
};
use crate::utils::take_packed;
use crate::utils::text_node::build_icon_text_node;

const SLOT_CONTENT: &str = "select-content";
use rui_macros::component;

// ---------------------------------------------------------------------------
// Context
// ---------------------------------------------------------------------------

/// Select 根上下文（复合子件共享）。
#[derive(Clone)]
pub struct SelectCtx {
    pub id: String,
    pub open: Signal<bool>,
    /// Trigger focus_id (Content anchor); written when Trigger mounts.
    pub trigger_fid: Signal<String>,
    pub value: Signal<String>,
    pub placeholder: String,
    /// value → 展示文案（由 [`SelectItem`] 注册，供 [`SelectValue`] 读取）。
    pub labels: Signal<HashMap<String, String>>,
    pub match_trigger: bool,
    pub menu_width: Option<String>,
    pub menu_height: Option<String>,
    pub min_height: Option<String>,
    pub max_height: Option<String>,
    pub placement: PopupPlacement,
    pub trigger_class: String,
    pub content_class: String,
    /// 打开菜单时是否把当前选中项滚进视口（见 [`SelectScrollSelected`]）。
    pub scroll_selected: SelectScrollSelected,
}

/// 打开 Select 菜单时如何滚动到当前选中项。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SelectScrollSelected {
    /// 不自动滚动（默认）。
    #[default]
    None,
    /// 选中项对齐视口顶部。
    Start,
    /// 选中项对齐视口中线。
    Center,
}

impl SelectScrollSelected {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "center" | "middle" => Self::Center,
            "start" | "top" => Self::Start,
            "true" | "1" => Self::Center,
            _ => Self::None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Start => "start",
            Self::Center => "center",
        }
    }
}

fn select_context() -> Context<SelectCtx> {
    static CTX: OnceLock<Context<SelectCtx>> = OnceLock::new();
    *CTX.get_or_init(create_context::<SelectCtx>)
}

/// 读取最近的 [`Select`] 上下文。
pub fn use_select() -> Option<SelectCtx> {
    use_context(select_context())
}

fn require_select(part: &str) -> SelectCtx {
    use_select().unwrap_or_else(|| {
        panic!("{part} must be used within <Select> / select_scope");
    })
}

fn register_label(labels: Signal<HashMap<String, String>>, value: &str, label: &str) {
    let value = value.to_string();
    let label = label.to_string();
    // 已有相同映射则跳过，避免 SelectContent 预扫 / 打开时重复 notify。
    if labels.peek().get(&value).is_some_and(|l| l == &label) {
        return;
    }
    labels.update(|m| {
        m.insert(value, label);
    });
}

// ---------------------------------------------------------------------------
// Root scope（宏 `<Select>` / Builder 共用）
// ---------------------------------------------------------------------------

/// 一个 Select **实例**的根：自带 owner，再 `provide_context`（Trigger + Content 同属该实例）。
pub fn select_scope(
    id: impl Into<String>,
    open: Option<Signal<bool>>,
    value: Option<Signal<String>>,
    placeholder: impl Into<String>,
    trigger_class: impl Into<String>,
    content_class: impl Into<String>,
    scroll_selected: impl Into<String>,
    body: impl FnOnce() -> Element,
) -> Element {
    let id = crate::utils::resolve_instance_id(id, "select");
    let placeholder = placeholder.into();
    let trigger_class = trigger_class.into();
    let content_class = content_class.into();
    let scroll_selected = SelectScrollSelected::parse(&scroll_selected.into());
    let open = open.unwrap_or_else(|| use_signal(format!("{id}__select_open").as_str(), || false));
    let value =
        value.unwrap_or_else(|| use_signal(format!("{id}__select_value").as_str(), String::new));
    let labels = use_signal(format!("{id}__select_labels").as_str(), HashMap::new);
    let trigger_fid = use_signal(format!("{id}__select_trigger_fid").as_str(), || {
        format!("{id}:trigger")
    });
    with_child_owner(move || {
        provide_anchored_overlay_context(open, id.clone(), trigger_fid);
        provide_context(
            select_context(),
            SelectCtx {
                id: id.clone(),
                open,
                trigger_fid,
                value,
                placeholder,
                labels,
                match_trigger: true,
                menu_width: None,
                menu_height: None,
                min_height: None,
                max_height: None,
                placement: PopupPlacement::BottomStart,
                trigger_class: trigger_class.clone(),
                content_class,
                scroll_selected,
            },
        );
        let mut root = body();
        // 复合 Trigger + Content：相对定位容器；宽度吃满父级且允许收缩（避免撑破 Dialog）。
        let mut kids = take_packed(root);
        if !kids.is_empty() {
            let merged = format!("{} {}", trigger_class, author_class(&kids[0]));
            if class_is_full_width(&merged) {
                let trigger = kids.remove(0);
                kids.insert(0, trigger.min_w(0.0).overflow("hidden").flex_shrink(1.0));
            }
        }
        root = Element::from(
            vstack()
                .gap("0")
                .relative(true)
                .w("100%")
                .min_w(0.0)
                .max_w("100%")
                .children(kids),
        );
        root.template_id = Some(format!("{id}__select"));
        root
    })
}

fn class_is_full_width(class: &str) -> bool {
    class.split_whitespace().any(|t| {
        let t = t.trim_end_matches('!');
        t == "w-full" || t == "w-100" || t == "w-[100%]" || t == "w-100%"
    })
}

// ---------------------------------------------------------------------------
// SelectTrigger / SelectValue
// ---------------------------------------------------------------------------

/// ShadCN Aria `SelectTrigger`：`cn-select-trigger` + chevron；点击切换开合。
///
/// 视觉对齐 nova：`h-8` / `rounded-lg` / `gap-1.5` / `pl-2.5 pr-2` / `border-input`。
/// 复用 Button outline 的 hover / focus-ring；`w-full` 时配合 `min-w-0`，避免撑破 Dialog。
/// `size`：`default`（h-8）| `sm`（h-7）。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn SelectTrigger(size: Option<String>, children: Option<Element>) -> Element {
    let ctx = require_select("SelectTrigger");
    let open = ctx.open;
    let trigger_id = format!("{}:trigger", ctx.id);
    ctx.trigger_fid.set(trigger_id.clone());
    let class = ctx.trigger_class.clone();
    let full_width = class.split_whitespace().any(|t| {
        let t = t.trim_end_matches('!');
        t == "w-full" || t == "w-100" || t == "w-[100%]" || t == "w-100%"
    });
    let sm = size
        .as_deref()
        .map(|s| s.trim().eq_ignore_ascii_case("sm"))
        .unwrap_or(false);
    // data-[size=default]:h-8 / data-[size=sm]:h-7
    let height = if sm { "1.75rem" } else { "2rem" };
    let radius = if sm { "0.375rem" } else { "0.5rem" };

    let inner = match children {
        Some(c) => c,
        None => SelectValue(SelectValueProps {
            placeholder: None,
            children: None,
            base: None,
        }),
    };

    // cn-select-trigger-icon: size-4 text-muted-foreground
    let chevron = lucide_glyph("chevron-down").unwrap_or_else(|| "▾".into());
    let icon = build_icon_text_node(chevron)
        .text_size(16.0)
        .text_color(parse_text_color("muted-foreground"))
        .flex_shrink(0.0);

    // Value + chevron must be **direct** Button children so `Button::into` rebuilds
    // bind_text via `build_text_node` (nested shells paint as empty).
    let mut btn = button(&trigger_id).variant("outline");
    if full_width {
        // `truncate` 让 Button 重建文案叶时带上 ellipsis（SelectValue 的 overflow
        // 也会被 From 保留）；同时覆盖默认 `shrink-0`，槽位变窄时才能省略。
        btn = btn.w_full().truncate(true);
    }
    let mut trigger: Element = btn
        .on_click(move || {
            let next = !open.peek();
            if next && !crate::app::portal_overlay::stacked_modal_open() {
                disarm_underlay_close();
            }
            open.set(next);
            if !next {
                crate::utils::roving::focus_id_only(trigger_id.clone());
            }
        })
        .child(inner)
        .child(icon)
        .into();

    // Override Button default h-9 / px-4 / rounded-md → Aria trigger metrics.
    trigger = trigger
        .h(height)
        .min_h(height)
        .rounded(radius)
        .pl("0.625rem")
        .pr("0.5rem")
        .py("0.5rem")
        .gap("0.375rem")
        .justify("between")
        .items_center()
        .border_color(tokens::input());
    if !class.is_empty() {
        trigger = with_class_list(trigger, &class);
    }
    if full_width {
        // Button 默认 `shrink-0`；吃满父级时必须能在 flex 槽里收缩，否则文案把槽撑破、
        // 命中盒大于可视盒，悬停在相邻 Select 间来回切。
        trigger = trigger.min_w(0.0).overflow("hidden").flex_shrink(1.0);
    }
    trigger.accesskit_role("combobox")
}

/// ShadCN Aria `SelectValue`：当前选中文案或 placeholder（`cn-select-value-aria`）。
///
/// 使用 [`bind_text`] + [`bind_style`]（原地补丁），**禁止** `reactive_view`：在 Dialog
/// 内建树时若因 labels 更新触发子树 reconcile，会弄丢 `SelectContent` 的菜单 effect。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn SelectValue(placeholder: Option<String>, children: Option<Element>) -> Element {
    let _ = children;
    let ctx = require_select("SelectValue");
    let value = ctx.value;
    let labels = ctx.labels;
    let ph = placeholder.unwrap_or_else(|| ctx.placeholder.clone());
    let bind_id = format!("{}:value", ctx.id);
    let style_id = format!("{}:value_style", ctx.id);

    let mut text_el = bind_text(bind_id, move || {
        let val = value.get();
        let map = labels.get();
        if val.is_empty() {
            ph.clone()
        } else if let Some(l) = map.get(&val) {
            l.clone()
        } else {
            val
        }
    });
    text_el = text_el
        .text_size(14.0)
        .min_w(0.0)
        .flex_grow(1.0)
        .flex_shrink(1.0)
        .overflow("hidden")
        .whitespace_nowrap()
        .text_overflow_ellipsis();
    // data-placeholder:text-muted-foreground
    text_el = bind_style(text_el, style_id, move || {
        let empty = value.get().is_empty();
        StylePatch {
            text_color: Some(if empty {
                parse_text_color("muted-foreground")
            } else {
                parse_text_color("foreground")
            }),
            ..Default::default()
        }
    });
    text_el
}

// ---------------------------------------------------------------------------
// SelectContent
// ---------------------------------------------------------------------------

/// `<SelectContent>` / [`select_content`] 选项。
#[derive(Clone)]
pub struct SelectContentOpts {
    pub panel: crate::components::base::SlotChrome,
    pub match_trigger: Option<bool>,
    pub menu_width: Option<String>,
    pub menu_height: Option<String>,
    pub min_height: Option<String>,
    pub max_height: Option<String>,
    pub placement: Option<PopupPlacement>,
    /// `none` | `start` | `center`；空则用根 [`SelectCtx::scroll_selected`]。
    pub scroll_selected: Option<String>,
}

impl Default for SelectContentOpts {
    fn default() -> Self {
        Self {
            panel: Default::default(),
            match_trigger: None,
            menu_width: None,
            menu_height: None,
            min_height: None,
            max_height: None,
            placement: None,
            scroll_selected: None,
        }
    }
}

impl SelectContentOpts {
    crate::impl_content_opts_chrome!(panel);
}

/// ShadCN `SelectContent`：Portal 浮层 + 可滚动视口；仅在 open 时构建。
pub fn select_content(opts: SelectContentOpts, body: impl Fn() -> Element + 'static) -> Element {
    let ctx = require_select("SelectContent");
    let value = ctx.value;
    let id = ctx.id.clone();
    let match_trigger = opts.match_trigger.unwrap_or(ctx.match_trigger);
    let menu_width = opts.menu_width.or(ctx.menu_width.clone());
    let menu_height = opts.menu_height.or(ctx.menu_height.clone());
    let min_height = opts.min_height.or(ctx.min_height.clone());
    let max_height = opts.max_height.or(ctx.max_height.clone());
    let placement = opts.placement.unwrap_or(ctx.placement);
    let panel = opts.panel.fallback_class(&ctx.content_class);
    let scroll_mode = opts
        .scroll_selected
        .as_deref()
        .map(SelectScrollSelected::parse)
        .unwrap_or(ctx.scroll_selected);

    // 复合 API：Items 只在 Content 内；先静默构建一次以注册 value→label，避免首屏闪 raw id。
    if ctx.labels.peek().is_empty() {
        let _ = body();
    }

    let id_for_effect = id.clone();
    let ao_opts = AnchoredOverlayContentOpts {
        placement: Some(Placement::trigger(
            String::new(),
            placement,
            FLOATING_TRIGGER_GAP,
            FloatingStrategy::Auto,
        )),
        kind: OverlayKind::Menu,
        lock_page_scroll: Some(true),
        // Select menus may be nested inside a retained popover (for example
        // the year selector inside DatePicker). Fade keeps the parent panel's
        // geometry stable while the child portal enters/exits, avoiding a
        // retained shadow fringe and duplicate hover paint at the anchor.
        motion: FloatingMotionKind::Fade,
        mode: OverlayMode::Stacked,
        match_trigger_width: match_trigger,
        max_h: max_height.clone(),
        width: menu_width.clone(),
        ..Default::default()
    };
    anchored_overlay_content(ao_opts, move || {
        // untrack：Item/ScrollArea 构建期不可订到 hover/scroll 信号，否则菜单一开就整树重建。
        let rows = untrack(|| body());
        let scroll_id = format!("{id_for_effect}:menu_scroll");
        let open_now = ctx.open.try_peek().unwrap_or(false);
        if !open_now {
            crate::app::scroll_support::reset_scroll_item_into_view(&scroll_id);
        } else if scroll_mode != SelectScrollSelected::None {
            if let Some(index) =
                find_select_item_index(&rows, &value.try_peek().unwrap_or_default())
            {
                let n = count_select_items(&rows).max(1);
                let align = match scroll_mode {
                    SelectScrollSelected::Center => {
                        crate::app::scroll_support::ScrollItemAlign::Center
                    }
                    SelectScrollSelected::Start => {
                        crate::app::scroll_support::ScrollItemAlign::Start
                    }
                    SelectScrollSelected::None => unreachable!(),
                };
                crate::app::scroll_support::request_scroll_item_into_view(
                    scroll_id.as_str(),
                    index,
                    n,
                    align,
                );
            }
        }
        // Bounded flex child: when the floating chrome is capped by max_h / viewport,
        // this ScrollArea must shrink (`min_h: 0`) so overflow can scroll — `h: auto`
        // alone sizes to content and never creates a viewport.
        let viewport: Element = untrack(|| {
            scroll_area(&scroll_id)
                .overflow_x("hidden")
                .overflow_y("auto")
                .scrollbar_visibility("auto")
                .w("100%")
                .h("100%")
                .min_h(0.0)
                .flex_grow(1.0)
                .flex_shrink(1.0)
                .child(rows)
                .into()
        });

        // 外层 chrome 承载 floating 几何（勿把 scroll_area 当作 floating 根）。
        // cn-select-content-aria: bg-popover ring-1 ring-foreground/10 rounded-lg shadow-md
        // min-w-36 only when not matching the trigger. overflow hidden：裁剪由 chrome 负责。
        let mut menu: Element = vstack()
            .w("auto")
            .h("auto")
            .min_h(0.0)
            .overflow_x("hidden")
            .overflow_y("hidden")
            .p("0.25rem")
            .child(viewport)
            .into();
        let mut ring = parse_color("foreground");
        ring[3] *= 0.1;
        menu = menu
            .bg(tokens::background())
            .rounded("0.5rem")
            .outline(ring, 1.0)
            .popup_shadow();

        menu.set_attr(
            ATTR_FLOATING_MATCH_TRIGGER,
            if match_trigger { "1" } else { "0" },
        );
        if let Some(mw) = &menu_width {
            menu.set_attr(ATTR_FLOATING_W, mw.as_str());
        }
        if match_trigger {
            // 宽度严格等于触发器（shadcn `w-(--trigger-width)`）；勿再叠 style min_w 干扰。
            menu.set_attr(ATTR_FLOATING_MIN_W, "trigger");
        } else {
            menu.set_attr(ATTR_FLOATING_MIN_W, "8rem");
            menu = menu.min_w("9rem");
        }
        if let Some(h) = &menu_height {
            menu.set_attr(ATTR_FLOATING_H, h.as_str());
        }
        if let Some(h) = &min_height {
            menu.set_attr(ATTR_FLOATING_MIN_H, h.as_str());
        }
        if let Some(mh) = &max_height {
            menu.set_attr(ATTR_FLOATING_MAX_H, mh.as_str());
            menu = menu.max_h(mh.as_str());
        }
        if !panel.is_empty() {
            menu = paint_slot(menu, SLOT_CONTENT, &panel);
        }
        menu
    })
}

// ---------------------------------------------------------------------------
// SelectGroup / SelectLabel / SelectItem / SelectSeparator
// ---------------------------------------------------------------------------

/// ShadCN Aria `SelectGroup`（`cn-select-group`：`p-1` 由 Content 承担，此处只做分组容器）。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn SelectGroup(children: Option<Element>) -> Element {
    let _ = require_select("SelectGroup");
    let kids = children.map(take_packed).unwrap_or_default();
    Element::from(vstack().w("100%").gap("0").children(kids))
}

/// ShadCN Aria `SelectLabel`（`cn-select-label`：`px-1.5` + 与上下分隔的均衡间距）。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn SelectLabel(children: Option<Element>) -> Element {
    let ctx = require_select("SelectLabel");
    let muted = parse_text_color("muted-foreground");
    let text = match &children {
        Some(c) => c
            .text
            .clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| extract_text_leaf(c)),
        None => String::new(),
    };
    let mut head: Element = label(format!("{}:label:{}", ctx.id, text).as_str())
        .text(text)
        .size(12.0)
        .line_height(12.0)
        .color(muted)
        .into();
    // 视觉上标签易贴着上方 Separator、离下方 Item 过远：加大 mt、收紧行高与 pb。
    head = head
        .w("100%")
        .px("0.375rem")
        .mt("0.375rem")
        .pt("0.125rem")
        .pb("0.125rem");
    head
}

/// ShadCN Aria `SelectItem`：`cn-select-item` + `cn-select-item-aria`；勾选在右侧绝对定位。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn SelectItem(
    value: String,
    disabled: bool,
    align: Option<String>,
    children: Option<Element>,
) -> Element {
    let ctx = require_select("SelectItem");
    let label_text = match &children {
        Some(c) => c
            .text
            .clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| extract_text_leaf(c)),
        None => value.clone(),
    };
    register_label(ctx.labels, &value, &label_text);

    let open = ctx.open;
    let trigger_fid = ctx.trigger_fid;
    let val_sig = ctx.value;
    let selected = val_sig.peek() == value;
    let align = SelectItemAlign::parse(align.as_deref().unwrap_or("start"));
    let bid = format!("{}:opt:{}", ctx.id, value);
    let ov = value.clone();

    let mut row = select_item_row(&bid, &label_text, selected, disabled, align, move || {
        if disabled {
            return;
        }
        open.set(false);
        val_sig.set(ov.clone());
        // The menu is a hoisted portal. Restore the trigger as the active
        // focus target before its exit frame is reconciled so a hovered
        // trigger is painted exactly once after the menu disappears.
        crate::utils::roving::focus_id_only(trigger_fid.peek());
    });
    row.set_attr("rui_select_item", value.as_str());
    row.accesskit_role("option")
}

/// ShadCN / solid-ui 菜单分割线：`-mx-1 my-1 h-px bg-border`。
///
/// Cancel Content `p-1` via [`Element::menu_bleed`]; do not hand-write `w-full` + negative `mx`.
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn SelectSeparator(children: Option<Element>) -> Element {
    let _ = children;
    let _ = require_select("SelectSeparator");
    separator().menu_bleed().pointer_events_none()
}

/// 项内容对齐。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SelectItemAlign {
    #[default]
    Start,
    Center,
    End,
}

impl SelectItemAlign {
    fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "center" => Self::Center,
            "end" | "right" => Self::End,
            _ => Self::Start,
        }
    }
}

fn find_select_item_index(el: &Element, value: &str) -> Option<usize> {
    let mut idx = 0;
    let mut found = None;
    crate::core::tree::walk_elements_pruned(el, |n| {
        if let Some(v) = n.attrs.get("rui_select_item") {
            if found.is_none() && v.as_str() == value {
                found = Some(idx);
            }
            idx += 1;
            return false;
        }
        true
    });
    found
}

fn count_select_items(el: &Element) -> usize {
    let mut n = 0;
    crate::core::tree::walk_elements_pruned(el, |node| {
        if node.attrs.contains_key("rui_select_item") {
            n += 1;
            return false;
        }
        true
    });
    n
}
fn extract_text_leaf(el: &Element) -> String {
    crate::core::tree::first_text_leaf(el)
}

// ---------------------------------------------------------------------------
// 数据型 SelectItem（Builder 便捷 API）
// ---------------------------------------------------------------------------

/// Builder / `options` 用的数据项（RSX 标签请用组件 [`SelectItem`]）。
#[derive(Clone, Debug)]
pub struct SelectItemData {
    pub value: String,
    pub label: String,
    pub disabled: bool,
    pub align: SelectItemAlign,
}

impl SelectItemData {
    pub fn new(value: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
            disabled: false,
            align: SelectItemAlign::Start,
        }
    }

    pub fn disabled(mut self, yes: bool) -> Self {
        self.disabled = yes;
        self
    }

    pub fn align(mut self, align: impl AsRef<str>) -> Self {
        self.align = SelectItemAlign::parse(align.as_ref());
        self
    }
}

/// Builder 分组（RSX 请用组件 [`SelectGroup`]）。
#[derive(Clone, Debug, Default)]
pub struct SelectGroupData {
    pub id: String,
    pub label: Option<String>,
    pub items: Vec<SelectItemData>,
}

impl SelectGroupData {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: None,
            items: Vec::new(),
        }
    }

    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn item(mut self, item: SelectItemData) -> Self {
        self.items.push(item);
        self
    }
}

#[derive(Clone, Debug)]
enum SelectEntry {
    Item(SelectItemData),
    Group(SelectGroupData),
    Separator,
}

impl_simple_slot_builder!(
    SelectTriggerBuilder,
    select_trigger,
    SelectTrigger,
    SelectTriggerProps
);
impl_simple_slot_builder!(
    SelectValueBuilder,
    select_value,
    SelectValue,
    SelectValueProps
);
impl_simple_slot_builder!(
    SelectGroupBuilder,
    select_group,
    SelectGroup,
    SelectGroupProps
);
impl_simple_slot_builder!(
    SelectSeparatorBuilder,
    select_separator,
    SelectSeparator,
    SelectSeparatorProps
);

pub struct SelectItemBuilder {
    value: String,
    disabled: bool,
    children: Vec<Element>,
    chrome: crate::components::base::SlotChrome,
}

pub fn select_item(value: impl Into<String>) -> SelectItemBuilder {
    SelectItemBuilder {
        value: value.into(),
        disabled: false,
        children: Vec::new(),
        chrome: crate::components::base::SlotChrome::default(),
    }
}

impl SelectItemBuilder {
    crate::impl_slot_chrome_methods!();

    pub fn disabled(mut self, v: bool) -> Self {
        self.disabled = v;
        self
    }

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children
            .extend(crate::utils::take_packed(child.into()));
        self
    }

    pub fn text(mut self, content: impl Into<String>) -> Self {
        self.children.push(Element::new().text(content.into()));
        self
    }
}

impl From<SelectItemBuilder> for Element {
    fn from(builder: SelectItemBuilder) -> Self {
        let el = SelectItem(SelectItemProps {
            value: Some(builder.value),
            disabled: Some(builder.disabled),
            align: None,
            children: crate::components::base::packed_children(builder.children),
            base: None,
            ..Default::default()
        });
        builder.chrome.apply(el)
    }
}

impl crate::core::tree::IntoElementList for SelectItemBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

pub struct SelectContentSlotBuilder {
    opts: SelectContentOpts,
}

pub fn select_content_slot() -> SelectContentSlotBuilder {
    SelectContentSlotBuilder {
        opts: SelectContentOpts::default(),
    }
}

impl SelectContentSlotBuilder {
    pub fn class(mut self, classes: impl Into<String>) -> Self {
        self.opts = self.opts.class(classes);
        self
    }
    pub fn style(
        mut self,
        f: impl Fn(
                crate::components::base::BaseComponentBuilder,
            ) -> crate::components::base::BaseComponentBuilder
            + 'static,
    ) -> Self {
        self.opts = self.opts.style(f);
        self
    }
    pub fn child(self, body: impl Fn() -> Element + 'static) -> Element {
        select_content(self.opts, body)
    }
}

// ---------------------------------------------------------------------------
// Convenience Builder（单组件入口，内建 Trigger+Content）
// ---------------------------------------------------------------------------

pub struct SelectBuilder {
    id: String,
    value: Option<Signal<String>>,
    default_value: String,
    entries: Vec<SelectEntry>,
    placeholder: String,
    trigger_width: String,
    menu_width: Option<String>,
    menu_height: Option<String>,
    min_height: Option<String>,
    max_height: Option<String>,
    match_trigger: bool,
    placement: PopupPlacement,
    trigger_class: String,
    trigger_style: Option<SlotStyleFn>,
    slots: SlotMap,
    scroll_selected: SelectScrollSelected,
}

/// 便捷入口：一个 Builder 即完整 Select（内含 Trigger / Content / Items）。
pub fn select(id: impl Into<String>) -> SelectBuilder {
    SelectBuilder {
        id: id.into(),
        value: None,
        default_value: String::new(),
        entries: Vec::new(),
        placeholder: crate::tr_default_zh("rui.select.placeholder", "Select…", "请选择…"),
        trigger_width: "auto".into(),
        menu_width: None,
        menu_height: None,
        min_height: None,
        max_height: None,
        match_trigger: true,
        placement: PopupPlacement::BottomStart,
        trigger_class: String::new(),
        trigger_style: None,
        slots: SlotMap::default(),
        scroll_selected: SelectScrollSelected::None,
    }
}

impl SelectBuilder {
    crate::impl_slot_map_methods!();
    crate::impl_overlay_content_slot!(SLOT_CONTENT);

    pub fn value_signal(mut self, s: Signal<String>) -> Self {
        self.value = Some(s);
        self
    }
    pub fn default_value(mut self, v: impl Into<String>) -> Self {
        self.default_value = v.into();
        self
    }
    pub fn options(mut self, pairs: Vec<(String, String)>) -> Self {
        self.entries.extend(
            pairs
                .into_iter()
                .map(|(v, l)| SelectEntry::Item(SelectItemData::new(v, l))),
        );
        self
    }
    pub fn item(mut self, item: SelectItemData) -> Self {
        self.entries.push(SelectEntry::Item(item));
        self
    }
    pub fn group(mut self, group: SelectGroupData) -> Self {
        self.entries.push(SelectEntry::Group(group));
        self
    }
    pub fn separator(mut self) -> Self {
        self.entries.push(SelectEntry::Separator);
        self
    }
    pub fn placeholder(mut self, p: impl Into<String>) -> Self {
        self.placeholder = p.into();
        self
    }
    pub fn trigger_width(mut self, w: impl Into<String>) -> Self {
        self.trigger_width = w.into();
        self
    }
    pub fn menu_width(mut self, w: impl Into<String>) -> Self {
        self.menu_width = Some(w.into());
        self.match_trigger = false;
        self
    }
    pub fn menu_height(mut self, h: impl Into<String>) -> Self {
        self.menu_height = Some(h.into());
        self
    }
    pub fn min_height(mut self, h: impl Into<String>) -> Self {
        self.min_height = Some(h.into());
        self
    }
    pub fn max_height(mut self, h: impl Into<String>) -> Self {
        self.max_height = Some(h.into());
        self
    }
    pub fn match_trigger(mut self, yes: bool) -> Self {
        self.match_trigger = yes;
        self
    }
    pub fn placement(mut self, p: PopupPlacement) -> Self {
        self.placement = p;
        self
    }
    /// Trigger 宽度别名。RSX 请写在 `<SelectTrigger class="…">`。
    pub fn class(mut self, c: impl Into<String>) -> Self {
        self.trigger_class = c.into();
        self
    }
    /// Same [`BaseComponentBuilder`] surface as RSX props on the trigger.
    pub fn style(
        mut self,
        f: impl Fn(
                crate::components::base::BaseComponentBuilder,
            ) -> crate::components::base::BaseComponentBuilder
            + 'static,
    ) -> Self {
        self.trigger_style = Some(std::rc::Rc::new(f));
        self
    }
    /// 打开时滚动到当前选中项：`none` | `start` | `center`。
    pub fn scroll_selected(mut self, v: impl AsRef<str>) -> Self {
        self.scroll_selected = SelectScrollSelected::parse(v.as_ref());
        self
    }
}

impl From<SelectBuilder> for Element {
    fn from(builder: SelectBuilder) -> Self {
        let SelectBuilder {
            id,
            value,
            default_value,
            entries,
            placeholder,
            trigger_width,
            menu_width,
            menu_height,
            min_height,
            max_height,
            match_trigger,
            placement,
            trigger_class,
            trigger_style,
            slots,
            scroll_selected,
        } = builder;
        let id = crate::utils::resolve_instance_id(id, "select");

        let open = use_signal(format!("{id}__select_open").as_str(), || false);
        // Empty default → placeholder; only honor explicit `default_value`.
        let default_initial = default_value;
        let val = value.unwrap_or_else(|| {
            use_signal(format!("{id}__select_value").as_str(), move || {
                default_initial.clone()
            })
        });
        let labels = use_signal(format!("{id}__select_labels").as_str(), HashMap::new);
        let trigger_fid = use_signal(format!("{id}__select_trigger_fid").as_str(), || {
            format!("{id}:trigger")
        });

        // Pre-register labels for SelectValue.
        for e in &entries {
            match e {
                SelectEntry::Item(i) => register_label(labels, &i.value, &i.label),
                SelectEntry::Group(g) => {
                    for i in &g.items {
                        register_label(labels, &i.value, &i.label);
                    }
                }
                SelectEntry::Separator => {}
            }
        }

        let mut trigger_class = trigger_class;
        if trigger_width == "100%" && !trigger_class.contains("w-full") {
            trigger_class = format!("w-full {trigger_class}");
        }

        with_child_owner(move || {
            provide_anchored_overlay_context(open, id.clone(), trigger_fid);
            provide_context(
                select_context(),
                SelectCtx {
                    id: id.clone(),
                    open,
                    trigger_fid,
                    value: val,
                    placeholder: placeholder.clone(),
                    labels,
                    match_trigger,
                    menu_width: menu_width.clone(),
                    menu_height: menu_height.clone(),
                    min_height: min_height.clone(),
                    max_height: max_height.clone(),
                    placement,
                    trigger_class: trigger_class.clone(),
                    content_class: slots.get(SLOT_CONTENT).class.clone(),
                    scroll_selected,
                },
            );

            let mut trigger = SelectTrigger(SelectTriggerProps {
                size: None,
                children: Some(SelectValue(SelectValueProps {
                    placeholder: None,
                    children: None,
                    base: None,
                })),
                base: None,
            });
            if let Some(style) = trigger_style {
                trigger = style(crate::components::base::BaseComponentBuilder::new(trigger)).el;
            }
            let entries_body = entries;
            let id_for_menu = id.clone();
            let content_chrome = slots.get(SLOT_CONTENT);
            let content = select_content(
                SelectContentOpts {
                    panel: content_chrome,
                    match_trigger: Some(match_trigger),
                    menu_width,
                    menu_height,
                    min_height,
                    max_height,
                    placement: Some(placement),
                    scroll_selected: None,
                },
                move || build_entries_element(&id_for_menu, &entries_body, val, open, labels),
            );

            Element::from(
                vstack()
                    .gap("0")
                    .relative(true)
                    .w("100%")
                    .min_w(0.0)
                    .max_w("100%")
                    .child(trigger)
                    .child(content),
            )
            .template_id(format!("{id}__select"))
        })
    }
}

fn build_entries_element(
    id: &str,
    entries: &[SelectEntry],
    val: Signal<String>,
    open: Signal<bool>,
    labels: Signal<HashMap<String, String>>,
) -> Element {
    let mut col: Element = vstack().w("100%").gap("0").into();
    col = col.accesskit_role("listbox");
    for e in entries {
        match e {
            SelectEntry::Item(item) => {
                col = col.child(build_data_item_row(id, item, val, open, labels));
            }
            SelectEntry::Group(g) => {
                if let Some(title) = &g.label {
                    let muted = parse_text_color("muted-foreground");
                    let hid = format!("{id}:grp:{}", g.id);
                    let mut head: Element = label(&hid)
                        .text(title.clone())
                        .size(12.0)
                        .line_height(12.0)
                        .color(muted)
                        .into();
                    head = head
                        .w("100%")
                        .px("0.375rem")
                        .mt("0.375rem")
                        .pt("0.125rem")
                        .pb("0.125rem");
                    col = col.child(head);
                }
                for item in &g.items {
                    col = col.child(build_data_item_row(id, item, val, open, labels));
                }
            }
            SelectEntry::Separator => {
                col = col.child(SelectSeparator(SelectSeparatorProps {
                    children: None,
                    base: None,
                }));
            }
        }
    }
    col
}

fn build_data_item_row(
    select_id: &str,
    item: &SelectItemData,
    val: Signal<String>,
    open: Signal<bool>,
    labels: Signal<HashMap<String, String>>,
) -> Element {
    register_label(labels, &item.value, &item.label);
    let selected = val.peek() == item.value;
    let ov = item.value.clone();
    let bid = format!("{select_id}:opt:{}", item.value);
    let disabled = item.disabled;
    let trigger_fid = use_select().map(|ctx| ctx.trigger_fid);

    let mut row = select_item_row(
        &bid,
        &item.label,
        selected,
        disabled,
        item.align,
        move || {
            if disabled {
                return;
            }
            open.set(false);
            val.set(ov.clone());
            if let Some(trigger_fid) = trigger_fid {
                crate::utils::roving::focus_id_only(trigger_fid.peek());
            }
        },
    );
    row.set_attr("rui_select_item", item.value.as_str());
    row.accesskit_role("option")
}

/// Aria `cn-select-item` row: ghost hover; label start / check end (分居两侧).
fn select_item_row(
    bid: &str,
    label_text: &str,
    selected: bool,
    disabled: bool,
    align: SelectItemAlign,
    on_click: impl FnMut() + 'static,
) -> Element {
    let txt_id = format!("{bid}:txt");
    let mut text_el: Element = label(txt_id.as_str())
        .text(label_text.to_string())
        .size(14.0)
        .color(parse_text_color(if selected {
            "accent-foreground"
        } else {
            "foreground"
        }))
        .into();
    text_el = text_el
        .flex_grow(1.0)
        .min_w(0.0)
        .flex_shrink(1.0)
        .overflow("hidden")
        .whitespace_nowrap()
        .text_overflow_ellipsis();

    // Fixed-size end slot so selected/unselected rows share the same text width.
    let indicator = if selected {
        let g = lucide_glyph("check").unwrap_or_else(|| "✓".into());
        build_icon_text_node(g)
            .text_size(16.0)
            .text_color(parse_text_color("accent-foreground"))
            .w(16.0)
            .h(16.0)
            .flex_shrink(0.0)
            .pointer_events_none()
    } else {
        Element::new()
            .w(16.0)
            .h(16.0)
            .flex_shrink(0.0)
            .pointer_events_none()
    };

    let inner = hstack()
        .w("100%")
        .min_w(0.0)
        .items_center()
        .justify("between")
        .gap("0.375rem")
        .child(text_el)
        .child(indicator);

    let mut row: Element = button(bid)
        .variant("ghost")
        .size("sm")
        .w("100%")
        .disabled(disabled)
        .on_click(on_click)
        .child(inner)
        .into();

    row = match align {
        SelectItemAlign::Start => row.justify_start(),
        SelectItemAlign::Center => row.justify_center(),
        SelectItemAlign::End => row.justify_end(),
    };
    // Override Button sm metrics → Aria item: gap-1.5 rounded-md py-1 px-1.5
    row = row
        .w("100%")
        .min_w(0.0)
        .max_w("100%")
        .h("auto")
        .min_h("auto")
        .rounded("0.375rem")
        .gap("0.375rem")
        .py("0.25rem")
        .px("0.375rem");
    if selected {
        row = row.bg(parse_color("accent"));
    }
    if disabled {
        row = row.opacity(0.5);
    }
    row
}

/// 旧 RSX `<Select options=…>` 兼容。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn Select(
    id: String,
    value: Option<Signal<String>>,
    default_value: String,
    options: Vec<(String, String)>,
    placeholder: String,
    w: Option<String>,
    scroll_selected: Option<String>,
    children: Option<Element>,
) -> Element {
    // 若已有复合子树（Trigger/Content），走 scope。
    if let Some(c) = children {
        let ss = scroll_selected.clone().unwrap_or_default();
        return select_scope(
            id,
            None,
            value,
            placeholder,
            String::new(),
            String::new(),
            ss,
            || c,
        );
    }
    let mut b = select(&id)
        .default_value(default_value)
        .options(options)
        .placeholder(placeholder);
    if let Some(width) = w {
        b = b.trigger_width(width);
    }
    if let Some(s) = scroll_selected {
        b = b.scroll_selected(s);
    }
    if let Some(sig) = value {
        b = b.value_signal(sig);
    }
    b.into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::reactivity::create_root;
    use crate::utils::popup::{
        mark_floating, ATTR_FLOATING, ATTR_FLOATING_MATCH_TRIGGER, ATTR_FLOATING_TRIGGER,
    };

    #[test]
    fn select_builder_keeps_template_id() {
        let (_owner, el) = create_root(|| {
            Element::from(
                select("calc:mode")
                    .default_value("a")
                    .options(vec![("a".into(), "A".into())])
                    .trigger_width("7rem"),
            )
        });
        assert_eq!(el.template_id.as_deref(), Some("calc:mode__select"));
    }

    #[test]
    fn select_trigger_keeps_value_ellipsis_after_button_rebuild() {
        let (_owner, el) = create_root(|| {
            Element::from(
                select("sel_ell")
                    .default_value("september")
                    .options(vec![("september".into(), "September".into())])
                    .trigger_width("100%"),
            )
        });
        fn find_tid<'a>(el: &'a Element, want: &str) -> Option<&'a Element> {
            if el.template_id.as_deref() == Some(want) {
                return Some(el);
            }
            el.children.iter().find_map(|c| find_tid(c, want))
        }
        let value = find_tid(&el, "sel_ell:value").expect("SelectValue leaf");
        assert!(
            matches!(
                value.text_style.overflow,
                crate::style::TextOverflow::Ellipsis
            ),
            "trigger caption must ellipsis, got {:?}",
            value.text_style.overflow
        );
        assert!(
            value.style.flex_shrink > 0.5,
            "ellipsis caption must shrink inside the trigger"
        );
    }

    #[test]
    fn select_builder_wires_trigger_and_content_anchor() {
        let (_owner, el) = create_root(|| {
            Element::from(
                select("sel_c")
                    .default_value("a")
                    .options(vec![("a".into(), "A".into())])
                    .trigger_width("100%")
                    .max_height("12rem"),
            )
        });

        fn has_focus(el: &Element, id: &str) -> bool {
            if el.focus_id.as_deref() == Some(id) {
                return true;
            }
            el.children.iter().any(|c| has_focus(c, id))
        }
        fn has_template(el: &Element, id: &str) -> bool {
            if el.template_id.as_deref() == Some(id) {
                return true;
            }
            el.children.iter().any(|c| has_template(c, id))
        }

        assert!(
            has_focus(&el, "sel_c:trigger"),
            "SelectTrigger must use {{id}}:trigger focus id"
        );
        assert!(
            has_template(&el, "sel_c__view"),
            "SelectContent anchored overlay anchor must be present"
        );
        assert_eq!(el.template_id.as_deref(), Some("sel_c__select"));
    }

    #[test]
    fn select_value_uses_bind_text_not_reactive_view() {
        let (_owner, el) = create_root(|| {
            let open = use_signal("sv_bt__select_open", || false);
            let value = use_signal("sv_bt__select_value", || "a".to_string());
            let labels = use_signal("sv_bt__select_labels", || {
                let mut m = HashMap::new();
                m.insert("a".into(), "Alpha".into());
                m
            });
            provide_context(
                select_context(),
                SelectCtx {
                    id: "sv_bt".into(),
                    open,
                    trigger_fid: use_signal("sv_bt__select_trigger_fid", || "sv_bt:trigger".into()),
                    value,
                    placeholder: "Pick".into(),
                    labels,
                    match_trigger: true,
                    menu_width: None,
                    menu_height: None,
                    min_height: None,
                    max_height: None,
                    placement: PopupPlacement::BottomStart,
                    trigger_class: String::new(),
                    content_class: String::new(),
                    scroll_selected: SelectScrollSelected::None,
                },
            );
            SelectValue(SelectValueProps {
                placeholder: None,
                children: None,
                base: None,
            })
        });
        assert_eq!(el.template_id.as_deref(), Some("sv_bt:value"));
        assert_eq!(el.text.as_deref(), Some("Alpha"));
    }

    #[test]
    fn select_builder_without_default_keeps_empty_value() {
        let (_owner, _) = create_root(|| {
            let sig = use_signal("sel_empty_val", String::new);
            let _el: Element = select("sel_empty")
                .value_signal(sig)
                .item(SelectItemData::new("a", "Alpha"))
                .item(SelectItemData::new("b", "Beta"))
                .into();
            assert!(
                sig.peek().is_empty(),
                "no default_value must leave selection empty for placeholder"
            );
        });
    }

    #[test]
    fn select_menu_chrome_marks_floating() {
        let mut menu = Element::new().absolute();
        mark_floating(
            &mut menu,
            "sel_c",
            "sel_c:trigger",
            PopupPlacement::BottomStart,
        );
        menu.set_attr(ATTR_FLOATING_MATCH_TRIGGER, "1");
        assert_eq!(
            menu.attrs.get(ATTR_FLOATING).map(|s| s.as_str()),
            Some("sel_c")
        );
        assert_eq!(
            menu.attrs.get(ATTR_FLOATING_TRIGGER).map(|s| s.as_str()),
            Some("sel_c:trigger")
        );
        assert_eq!(
            menu.attrs
                .get(ATTR_FLOATING_MATCH_TRIGGER)
                .map(|s| s.as_str()),
            Some("1")
        );
    }

    #[test]
    fn scroll_offset_centers_selected_item() {
        use crate::app::scroll_support::scroll_offset_to_center_item;
        // 12 months, August (index 7), 16rem menu − pad ≈ 248px viewport.
        let sy = scroll_offset_to_center_item(7, 32.0, 248.0, 12);
        assert!(
            (sy - 116.0).abs() < 0.5,
            "August should sit near mid, got {sy}"
        );
        assert_eq!(scroll_offset_to_center_item(0, 32.0, 248.0, 12), 0.0);
        let last = scroll_offset_to_center_item(11, 32.0, 248.0, 12);
        assert!(
            (last - 136.0).abs() < 0.5,
            "last month max scroll, got {last}"
        );
    }

    #[test]
    fn find_select_item_index_reads_attr() {
        let mut a = Element::new();
        a.set_attr("rui_select_item", "jan");
        let mut b = Element::new();
        b.set_attr("rui_select_item", "aug");
        let col = Element::new().child(a).child(b);
        assert_eq!(find_select_item_index(&col, "aug"), Some(1));
        assert_eq!(count_select_items(&col), 2);
    }

    #[test]
    fn scroll_selected_parse() {
        assert_eq!(
            SelectScrollSelected::parse("center"),
            SelectScrollSelected::Center
        );
        assert_eq!(
            SelectScrollSelected::parse("start"),
            SelectScrollSelected::Start
        );
        assert_eq!(
            SelectScrollSelected::parse("none"),
            SelectScrollSelected::None
        );
        assert_eq!(SelectScrollSelected::parse(""), SelectScrollSelected::None);
    }

    #[test]
    fn content_keeps_panel_tid_while_exiting() {
        use crate::app::portal_overlay::{clear_overlay_stack, OverlayKind, OverlayMode};
        use crate::core::reactivity::Signal;
        use crate::utils::anchored_overlay::{
            anchored_overlay_is_mounted, anchored_overlay_phase,
            apply_anchored_overlay_panel_motion, AnchoredOverlayContentOpts,
        };
        use crate::utils::floating_motion::FloatingMotionKind;
        use crate::utils::presence::PresencePhase;

        clear_overlay_stack();
        create_root(|| {
            let open = Signal::new(true);
            let id = "sel-exit-smoke";
            let opts = AnchoredOverlayContentOpts {
                kind: OverlayKind::Menu,
                lock_page_scroll: Some(true),
                motion: FloatingMotionKind::Expand,
                mode: OverlayMode::Stacked,
                ..Default::default()
            };
            assert_eq!(
                anchored_overlay_phase(id, open, &opts),
                Some(PresencePhase::Present)
            );

            open.set(false);
            assert_eq!(
                anchored_overlay_phase(id, open, &opts),
                Some(PresencePhase::Exiting),
                "must stay mounted while exiting"
            );
            assert!(anchored_overlay_is_mounted(id, &opts));

            let mut panel = Element::new();
            apply_anchored_overlay_panel_motion(id, &mut panel, &opts, open);
            assert_eq!(
                panel.template_id.as_deref(),
                Some("sel-exit-smoke__floating_panel")
            );
        });
        clear_overlay_stack();
    }
}
