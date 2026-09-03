//! # Combobox (ShadCN React Aria can filter the drop-down)
//!
//! Product surface & styles align with shadcn/ui **React Aria** base
//! ([`combobox.tsx`](https://github.com/shadcn-ui/ui/blob/main/apps/v4/registry/bases/aria/ui/combobox.tsx)
//! + `cn-combobox-*` / `cn-combobox-*-aria`)，浮层 chrome / 选项行与 [`super::select`] 一致。
//!
//! ## Usage (composite/class front end)
//!
//! ```ignore
//! ui! {
//!   <Combobox id="fruit" value={fruit} placeholder="Search fruit…" clearable={true} class="w-full">
//!     <ComboboxInput />
//!     <ComboboxContent max_h="16rem">
//!       <ComboboxList>
//!         <ComboboxGroup>
//!           <ComboboxLabel>"Citrus"</ComboboxLabel>
//!           <ComboboxItem value="orange">"Orange"</ComboboxItem>
//!           <ComboboxItem value="lemon">"Lemon"</ComboboxItem>
//!         </ComboboxGroup>
//!         <ComboboxSeparator />
//!         <ComboboxItem value="apple">"Apple"</ComboboxItem>
//!       </ComboboxList>
//!       <ComboboxEmpty>"No results."</ComboboxEmpty>
//!     </ComboboxContent>
//!   </Combobox>
//! }
//! ```
//!
//! ## Usage (Builder convenient entrance)
//!
//! ```ignore
//! Element::from(
//!     combobox("city")
//!         .value_signal(city)
//! .placeholder("Search city...")
//!         .clearable(true)
//!         .group(
//!             ComboboxGroupData::new("east")
//!                 .label("East")
//!                 .item(ComboboxItemData::new("sh", "Shanghai")),
//!         )
//!         .separator()
//!         .item(ComboboxItemData::new("bj", "Beijing")),
//! )
//! ```
//!
//! ## Behavioral points
//! - **选中值**与**输入/过滤文案**分离：选中后框内显示完整 label，不是 value id。
//! - 打开且输入文案仍等于当前 label 时列出全部项，便于切换；开始编辑后按 label 过滤。
//! - 关闭时若输入为空则清空选中；否则把输入恢复为当前选中项的 label。
//! - 打开时 ArrowUp/Down 走 [`crate::utils::roving`]。

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::app::portal_overlay::{OverlayKind, OverlayMode};
use crate::app::reactive::{reactive_anchor, reactive_placeholder_sized};
use crate::components::button::button;
use crate::components::input::{
    input, input_adornment_button, input_value_signal_key, lucide_named,
};
use crate::components::internal::floating_motion::{
    apply_floating_panel_motion, floating_portal_phase, use_floating_presence, FloatingMotionKind,
};
use crate::components::label::label;
use crate::components::scroll_area::scroll_area;
use crate::components::separator::separator;
use crate::core::reactivity::{
    create_context, provide_context, untrack, use_context, use_signal, with_child_owner, Context,
    Signal,
};
use crate::core::tree::Element;
use crate::primitives::r#box::{hstack, vstack};
use crate::primitives::text::text;
use crate::style::{parse_color, parse_text_color, with_class_list};
use crate::tokens;
use crate::utils::lucide_glyph;
use crate::utils::popup::{
    apply_stored_floating_place, disarm_underlay_close, mark_floating, mount_anchored_overlay,
    PopupPlacement, ATTR_FLOATING_H, ATTR_FLOATING_MATCH_TRIGGER, ATTR_FLOATING_MAX_H,
    ATTR_FLOATING_MIN_H, ATTR_FLOATING_MIN_W, ATTR_FLOATING_W,
};
use crate::utils::take_packed;
use crate::utils::text_field::{commit_value, commit_value_sig};
use crate::utils::text_node::build_icon_text_node;
use rui_macros::component;
use winit::keyboard::{Key, NamedKey};

// ---------------------------------------------------------------------------
// Context
// ---------------------------------------------------------------------------

/// 键盘 / 勾选用的可见项快照（Content 每次打开重建时写入）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComboboxVisibleItem {
    pub value: String,
    pub label: String,
    pub disabled: bool,
}

/// Combobox root context (shared by composite components).
#[derive(Clone)]
pub struct ComboboxCtx {
    pub id: String,
    pub open: Signal<bool>,
    pub value: Signal<String>,
    pub active: Signal<usize>,
    pub placeholder: String,
    pub clearable: bool,
    pub labels: Signal<HashMap<String, String>>,
    /// Content 构建时填充；Input 键盘用 `peek`。
    pub visible: Signal<Vec<ComboboxVisibleItem>>,
    pub match_trigger: bool,
    pub menu_width: Option<String>,
    pub menu_height: Option<String>,
    pub min_height: Option<String>,
    pub max_height: Option<String>,
    pub placement: PopupPlacement,
    pub trigger_class: String,
    pub content_class: String,
}

fn combobox_context() -> Context<ComboboxCtx> {
    static CTX: OnceLock<Context<ComboboxCtx>> = OnceLock::new();
    *CTX.get_or_init(create_context::<ComboboxCtx>)
}

/// Read the most recent [`Combobox`] context.
pub fn use_combobox() -> Option<ComboboxCtx> {
    use_context(combobox_context())
}

fn require_combobox(part: &str) -> ComboboxCtx {
    use_combobox().unwrap_or_else(|| {
        panic!("{part} must be used within <Combobox> / combobox_scope");
    })
}

fn register_label(labels: Signal<HashMap<String, String>>, value: &str, label: &str) {
    let value = value.to_string();
    let label = label.to_string();
    if labels.peek().get(&value).is_some_and(|l| l == &label) {
        return;
    }
    labels.update(|m| {
        m.insert(value, label);
    });
}

fn label_for_map(map: &HashMap<String, String>, value: &str) -> String {
    if value.is_empty() {
        return String::new();
    }
    map.get(value).cloned().unwrap_or_else(|| value.to_string())
}

fn filter_active(open: bool, query: &str, selected: &str, selected_label: &str) -> bool {
    open && !query.is_empty() && (selected.is_empty() || query != selected_label)
}

/// 打开时把 `active` 指到当前 value（TimePicker `"09"` → index 9），避免高亮停在 `00`。
fn active_index_for_value(visible: &[ComboboxVisibleItem], value: &str) -> usize {
    if let Some(i) = visible.iter().position(|it| it.value == value) {
        return i;
    }
    match value.parse::<usize>() {
        Ok(i) if visible.is_empty() || i < visible.len() => i,
        _ => 0,
    }
}

fn label_matches_query(label: &str, query: &str) -> bool {
    let q = query.to_ascii_lowercase();
    q.is_empty() || label.to_ascii_lowercase().contains(&q)
}

// ---------------------------------------------------------------------------
// Root scope（宏 `<Combobox>` / 复合子树）
// ---------------------------------------------------------------------------

/// 一个 Combobox **实例**的根：自带 owner，再 `provide_context`。
///
/// Trigger + Content（下拉 portal / Item）都在这棵子树上构建；下拉 effect 的 parent owner
/// 就是本实例，而不是外层 TimePicker / DatePicker。勿把三个 HMS Combobox 建在同一父 owner
/// 上提供 context（后写覆盖前写）。
pub fn combobox_scope(
    id: impl Into<String>,
    open: Option<Signal<bool>>,
    value: Option<Signal<String>>,
    placeholder: impl Into<String>,
    clearable: bool,
    trigger_class: impl Into<String>,
    content_class: impl Into<String>,
    body: impl FnOnce() -> Element,
) -> Element {
    let id = crate::utils::resolve_instance_id(id, "combo");
    let placeholder = placeholder.into();
    let trigger_class = trigger_class.into();
    let content_class = content_class.into();
    let open = open.unwrap_or_else(|| use_signal(format!("{id}__combo_open").as_str(), || false));
    let value =
        value.unwrap_or_else(|| use_signal(format!("{id}__combo_value").as_str(), String::new));
    let active = use_signal(format!("{id}__combo_active").as_str(), || 0usize);
    let labels = use_signal(format!("{id}__combo_labels").as_str(), HashMap::new);
    let visible = use_signal(
        format!("{id}__combo_visible").as_str(),
        Vec::<ComboboxVisibleItem>::new,
    );
    // 输入 store：与 value 分离；若已有选中则灌入 label。
    let want = label_for_map(&labels.peek(), &value.peek());
    let input_store = use_signal(input_value_signal_key(&id).as_str(), || want.clone());
    if input_store.peek() != want && !want.is_empty() {
        commit_value_sig(&id, input_store, want);
    }

    // 宽度由 class 控制：默认 16rem（避免撑满卡片）；`w-full` 时吃满父级。
    let full_width = class_is_full_width(&trigger_class);
    let has_width = class_has_width_token(&trigger_class);
    // 实例 owner：context + Input + Content 同属这一 Combobox，下拉不是父级的。
    with_child_owner(move || {
        provide_context(
            combobox_context(),
            ComboboxCtx {
                id: id.clone(),
                open,
                value,
                active,
                placeholder,
                clearable,
                labels,
                visible,
                match_trigger: true,
                menu_width: None,
                menu_height: None,
                min_height: None,
                max_height: None,
                placement: PopupPlacement::BottomStart,
                trigger_class: trigger_class.clone(),
                content_class,
            },
        );
        let mut root = body();
        let kids = take_packed(root);
        let mut shell = vstack()
            .gap("0")
            .relative(true)
            .min_w(0.0)
            .max_w("100%")
            .children(kids);
        shell = if full_width {
            shell.w("100%")
        } else if has_width {
            shell.w("auto")
        } else {
            // shadcn 示例常见 ~200–256px；过宽会像「输入框/菜单拉满整列」。
            shell.w("16rem")
        };
        root = Element::from(shell);
        if !trigger_class.is_empty() && (full_width || has_width) {
            // Drop `w-*` onto the shell, Input then `w-100%` to fill the shell.
            root = with_class_list(root, &trigger_class);
        }
        root.template_id = Some(format!("{id}__combobox"));
        root
    })
}

fn class_is_full_width(class: &str) -> bool {
    class.split_whitespace().any(|t| {
        let t = t.trim_end_matches('!');
        t == "w-full" || t == "w-100" || t == "w-[100%]" || t == "w-100%"
    })
}

fn class_has_width_token(class: &str) -> bool {
    class.split_whitespace().any(|t| {
        let t = t.trim_end_matches('!');
        t.starts_with("w-") || t.starts_with("min-w-") || t.starts_with("max-w-")
    })
}
fn extract_text_leaf(el: &Element) -> String {
    if let Some(t) = &el.text {
        return t.clone();
    }
    for c in &el.children {
        let t = extract_text_leaf(c);
        if !t.is_empty() {
            return t;
        }
    }
    String::new()
}

// ---------------------------------------------------------------------------
// ComboboxInput
// ---------------------------------------------------------------------------

/// ShadCN Aria `ComboboxInput`：可过滤 Input + chevron（可选 clear）。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn ComboboxInput(
    class: Option<String>,
    placeholder: Option<String>,
    clearable: Option<bool>,
    children: Option<Element>,
) -> Element {
    let _ = children;
    let ctx = require_combobox("ComboboxInput");
    let id = ctx.id.clone();
    let open = ctx.open;
    let value = ctx.value;
    let active = ctx.active;
    let labels = ctx.labels;
    let visible = ctx.visible;
    let ph = placeholder
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| ctx.placeholder.clone());
    let clearable = clearable.unwrap_or(ctx.clearable);
    let class = class.unwrap_or_else(|| ctx.trigger_class.clone());
    let view_id = format!("{id}__combo_input");
    let input_store = use_signal(input_value_signal_key(&id).as_str(), String::new);

    // 禁止 reactive_view 默认 100%×100% 占位（会把触发器/锚点 bound 撑满父级，菜单跟着越界）。
    reactive_anchor(view_id.clone(), move || {
        let selected = value.get();
        let is_open = open.get();
        let query = input_store.get();
        let map = labels.get();
        let selected_label = label_for_map(&map, &selected);

        if !is_open {
            if selected.is_empty() {
                if !query.is_empty() {
                    commit_value_sig(&id, input_store, String::new());
                }
            } else if query != selected_label {
                commit_value_sig(&id, input_store, selected_label.clone());
            }
        }

        let o_focus = open;
        let a_sig = active;
        let val_sig = value;
        let input_id = id.clone();
        let vis = visible;

        // Combobox trigger is a real Input: Hidden overflow (crop + caret pan), not ellipsis.
        let mut trigger: Element = input(&id).placeholder(ph.as_str()).w("100%").into();
        trigger = trigger
            .accesskit_role("combobox")
            .accesskit_name(ph.as_str());

        if clearable {
            let clear_val = value;
            let clear_open = open;
            let clear_id = id.clone();
            let show_clear = !query.is_empty() || !selected.is_empty();
            let mut clear_btn = input_adornment_button(
                format!("{id}__combo_clear"),
                lucide_named("x"),
                false,
                move || {
                    clear_val.set(String::new());
                    commit_value(&clear_id, String::new());
                    clear_open.set(false);
                    use_signal("__rui_button_focus_id", String::new)
                        .set(crate::utils::text_field::input_focus_id(&clear_id));
                    use_signal("__rui_focus_visible", || false).set(true);
                },
            );
            if !show_clear {
                clear_btn = clear_btn.opacity(0.0).pointer_events_none();
            }
            trigger = trigger.gap("0.25rem").child(clear_btn);
        }

        let o_chev = open;
        let a_chev = active;
        let v_chev = value;
        let vis_chev = visible;
        let chev_id = id.clone();
        let chevron = input_adornment_button(
            format!("{id}__combo_chevron"),
            lucide_named("chevron-down"),
            false,
            move || {
                let next = !o_chev.peek();
                if next {
                    a_chev.set(active_index_for_value(&vis_chev.peek(), &v_chev.peek()));
                    disarm_underlay_close();
                }
                o_chev.set(next);
                use_signal("__rui_button_focus_id", String::new)
                    .set(crate::utils::text_field::input_focus_id(&chev_id));
                use_signal("__rui_focus_visible", || false).set(true);
            },
        );
        trigger = trigger.gap("0.25rem").child(chevron);

        trigger = trigger
            .on_click({
                let a_sig = a_sig;
                let val_sig = val_sig;
                let vis = vis;
                move || {
                    a_sig.set(active_index_for_value(&vis.peek(), &val_sig.peek()));
                    disarm_underlay_close();
                    o_focus.set(true);
                }
            })
            .on_key_down(move |ev| {
                let list = vis.peek();
                let n = list.len();
                match &ev.logical_key {
                    Key::Named(NamedKey::ArrowDown)
                    | Key::Named(NamedKey::ArrowUp)
                    | Key::Named(NamedKey::Home)
                    | Key::Named(NamedKey::End)
                        if o_focus.peek() =>
                    {
                        if let Some(next) =
                            crate::utils::list_nav::list_nav_move(&ev.logical_key, n, a_sig.peek())
                        {
                            a_sig.set(next);
                        }
                        return true;
                    }
                    Key::Named(NamedKey::ArrowDown) => {
                        if !o_focus.peek() {
                            a_sig.set(active_index_for_value(&list, &val_sig.peek()));
                            disarm_underlay_close();
                            o_focus.set(true);
                        }
                        return true;
                    }
                    Key::Named(NamedKey::Enter) => {
                        if !o_focus.peek() {
                            a_sig.set(0);
                            disarm_underlay_close();
                            o_focus.set(true);
                            return true;
                        }
                        if let Some(opt) = list.get(a_sig.peek()) {
                            if !opt.disabled {
                                val_sig.set(opt.value.clone());
                                commit_value(&input_id, opt.label.clone());
                                o_focus.set(false);
                            }
                            return true;
                        }
                    }
                    Key::Named(NamedKey::Escape) => {
                        o_focus.set(false);
                        return true;
                    }
                    _ => {}
                }
                false
            });

        // 宽度已由壳承担；这里只应用非宽度类（避免重复 w-*）。
        let non_width: String = class
            .split_whitespace()
            .filter(|t| {
                let t = t.trim_end_matches('!');
                !(t.starts_with("w-") || t.starts_with("min-w-") || t.starts_with("max-w-"))
            })
            .collect::<Vec<_>>()
            .join(" ");
        if !non_width.is_empty() {
            trigger = with_class_list(trigger, &non_width);
        }
        trigger
    });
    reactive_placeholder_sized(view_id, "100%", "auto")
}

// ---------------------------------------------------------------------------
// ComboboxContent
// ---------------------------------------------------------------------------

/// `<ComboboxContent>` / [`combobox_content`] 选项。
#[derive(Clone, Debug, Default)]
pub struct ComboboxContentOpts {
    pub class: Option<String>,
    pub match_trigger: Option<bool>,
    pub menu_width: Option<String>,
    pub menu_height: Option<String>,
    pub min_height: Option<String>,
    pub max_height: Option<String>,
    pub placement: Option<PopupPlacement>,
}

/// ShadCN `ComboboxContent`：Portal 浮层 + 可滚动视口；仅在 open 时构建。
pub fn combobox_content(
    opts: ComboboxContentOpts,
    body: impl Fn() -> Element + 'static,
) -> Element {
    let ctx = require_combobox("ComboboxContent");
    let open = ctx.open;
    let active = ctx.active;
    let value = ctx.value;
    let labels = ctx.labels;
    let visible = ctx.visible;
    let id = ctx.id.clone();
    let view_id = format!("{id}__combo_content");
    let trigger_id = format!("input:{id}");
    let match_trigger = opts.match_trigger.unwrap_or(ctx.match_trigger);
    let menu_width = opts.menu_width.or(ctx.menu_width.clone());
    let menu_height = opts.menu_height.or(ctx.menu_height.clone());
    let min_height = opts.min_height.or(ctx.min_height.clone());
    let max_height = opts
        .max_height
        .or(ctx.max_height.clone())
        .or_else(|| Some("24rem".into()));
    let placement = opts.placement.unwrap_or(ctx.placement);
    let content_class = opts
        .class
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| ctx.content_class.clone());

    // 预扫注册 labels，避免首屏输入框闪 value id。
    if labels.peek().is_empty() {
        visible.set(Vec::new());
        let _ = body();
    }

    let id_for_effect = id.clone();
    let input_store = use_signal(input_value_signal_key(&id).as_str(), String::new);
    let presence = use_floating_presence(&id, FloatingMotionKind::Expand);
    let max_h_cap = max_height.as_deref().and_then(parse_css_length_px);
    let presence_for_cleanup = presence.clone();
    reactive_anchor(view_id.clone(), move || {
        let _ = active.get();
        let _ = input_store.get();
        let _ = value.get();
        let Some((phase, oz)) = floating_portal_phase(
            &id_for_effect,
            open,
            &presence,
            OverlayKind::Menu,
            OverlayMode::Stacked,
        ) else {
            crate::app::scroll_support::cancel_scroll_item_into_view(&format!(
                "{id_for_effect}:menu_scroll"
            ));
            return Element::new().w(0.0).h(0.0);
        };
        if !open.peek() {
            crate::app::scroll_support::cancel_scroll_item_into_view(&format!(
                "{id_for_effect}:menu_scroll"
            ));
        }

        let cur = value.peek();
        let ix0 = active_index_for_value(&visible.peek(), &cur);
        if active.peek() != ix0 {
            active.set_silent(ix0);
        }
        visible.set(Vec::new());
        let mut rows = untrack(|| body());
        let ix = active_index_for_value(&visible.peek(), &cur);
        if active.peek() != ix {
            active.set_silent(ix);
            visible.set(Vec::new());
            rows = untrack(|| body());
        }
        let scroll_id = format!("{id_for_effect}:menu_scroll");
        let n = visible.peek().len().max(1);
        if n > 1 {
            crate::app::scroll_support::request_scroll_item_into_view(
                scroll_id.as_str(),
                ix,
                n,
                crate::app::scroll_support::ScrollItemAlign::Center,
            );
        }
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

        let mut menu: Element = vstack()
            .w("auto")
            .h("auto")
            .min_h(0.0)
            .max_w("100%")
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
            .shadow([0.0, 0.0, 0.0, 0.14], 12.0)
            .absolute();

        mark_floating(&mut menu, &id_for_effect, &trigger_id, placement);
        apply_stored_floating_place(&mut menu, &id_for_effect);
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
        if !content_class.is_empty() {
            menu = with_class_list(menu, &content_class);
        }
        if apply_floating_panel_motion(
            &id_for_effect,
            &mut menu,
            FloatingMotionKind::Expand,
            phase,
            &presence,
            open,
            OverlayKind::Menu,
            OverlayMode::Stacked,
            max_h_cap,
        ) {
            crate::app::runtime::request_redraw();
        }

        mount_anchored_overlay(menu, oz.panel)
    });
    if crate::core::reactivity::is_tracking() {
        let open_reset = open;
        let id_reset = id.clone();
        let presence_reset = presence_for_cleanup.clone();
        crate::core::reactivity::on_cleanup(move || {
            if open_reset.peek() {
                open_reset.set(false);
            }
            let _ = floating_portal_phase(
                &id_reset,
                open_reset,
                &presence_reset,
                OverlayKind::Menu,
                OverlayMode::Stacked,
            );
        });
    }
    reactive_placeholder_sized(view_id, 0.0, 0.0).pointer_events_box_none()
}

fn parse_css_length_px(s: &str) -> Option<f32> {
    let s = s.trim();
    if let Some(n) = s.strip_suffix("rem") {
        n.trim().parse::<f32>().ok().map(|v| v * 16.0)
    } else if let Some(n) = s.strip_suffix("px") {
        n.trim().parse().ok()
    } else {
        s.parse().ok()
    }
}

// ---------------------------------------------------------------------------
// ComboboxList / Group / Label / Item / Separator / Empty
// ---------------------------------------------------------------------------

/// ShadCN Aria `ComboboxList`（可滚动列表容器；过滤由 Item 自行隐藏）。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn ComboboxList(class: Option<String>, children: Option<Element>) -> Element {
    let _ = require_combobox("ComboboxList");
    let kids = children.map(take_packed).unwrap_or_default();
    let mut el: Element = vstack().w("100%").gap("0").children(kids).into();
    el = el.accesskit_role("listbox");
    if let Some(c) = class.filter(|s| !s.trim().is_empty()) {
        el = with_class_list(el, &c);
    }
    el
}

/// ShadCN Aria `ComboboxGroup`。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn ComboboxGroup(children: Option<Element>) -> Element {
    let _ = require_combobox("ComboboxGroup");
    let kids = children.map(take_packed).unwrap_or_default();
    Element::from(vstack().w("100%").gap("0").children(kids))
}

/// ShadCN Aria `ComboboxLabel`。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn ComboboxLabel(children: Option<Element>) -> Element {
    let ctx = require_combobox("ComboboxLabel");
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
    head = head
        .w("100%")
        .px("0.375rem")
        .mt("0.375rem")
        .pt("0.125rem")
        .pb("0.125rem");
    head
}

/// ShadCN Aria `ComboboxItem`。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn ComboboxItem(
    value: String,
    disabled: bool,
    class: Option<String>,
    children: Option<Element>,
) -> Element {
    let ctx = require_combobox("ComboboxItem");
    let label_text = match &children {
        Some(c) => c
            .text
            .clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| extract_text_leaf(c)),
        None => value.clone(),
    };
    register_label(ctx.labels, &value, &label_text);

    let query = use_signal(input_value_signal_key(&ctx.id).as_str(), String::new).peek();
    let selected = ctx.value.peek();
    let selected_label = label_for_map(&ctx.labels.peek(), &selected);
    let filtering = filter_active(ctx.open.peek(), &query, &selected, &selected_label);
    if filtering && !label_matches_query(&label_text, &query) {
        return Element::new().w(0.0).h(0.0);
    }

    let ix = {
        let mut list = ctx.visible.peek();
        let ix = list.len();
        list.push(ComboboxVisibleItem {
            value: value.clone(),
            label: label_text.clone(),
            disabled,
        });
        ctx.visible.set(list);
        ix
    };

    let open = ctx.open;
    let val_sig = ctx.value;
    let active = ctx.active;
    let highlighted = active.peek() == ix;
    let is_selected = selected == value;
    let bid = format!("{}:opt:{}", ctx.id, value);
    let ov = value.clone();
    let ol = label_text.clone();
    let input_id = ctx.id.clone();

    let mut row = combobox_item_row(
        &bid,
        &label_text,
        is_selected,
        highlighted,
        disabled,
        move || {
            if disabled {
                return;
            }
            active.set(ix);
            val_sig.set(ov.clone());
            commit_value(&input_id, ol.clone());
            open.set(false);
        },
    );
    if let Some(c) = class.filter(|s| !s.trim().is_empty()) {
        row = with_class_list(row, &c);
    }
    row.set_attr("rui_combobox_item", value.as_str());
    row.accesskit_role("option")
}

/// ShadCN / solid-ui menu dividing line: `-mx-1 my-1 h-px bg-border` (same as [`crate::components::select::SelectSeparator`]).
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn ComboboxSeparator(children: Option<Element>) -> Element {
    let _ = children;
    let _ = require_combobox("ComboboxSeparator");
    separator().menu_bleed().pointer_events_none()
}

/// ShadCN Aria `ComboboxEmpty`：请放在 List **之后**，以便可见项已登记。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn ComboboxEmpty(children: Option<Element>) -> Element {
    let ctx = require_combobox("ComboboxEmpty");
    let msg = match &children {
        Some(c) => c
            .text
            .clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| extract_text_leaf(c)),
        None => "No results.".into(),
    };
    if !ctx.open.peek() || !ctx.visible.peek().is_empty() {
        return Element::new().w(0.0).h(0.0);
    }
    let empty: Element = text(msg.as_str())
        .size(14.0)
        .color(parse_text_color("muted-foreground"))
        .into();
    Element::from(vstack().w("100%").px("0.375rem").py("0.5rem").child(empty))
}

/// Aria `cn-combobox-item` line.
fn combobox_item_row(
    bid: &str,
    label_text: &str,
    selected: bool,
    highlighted: bool,
    disabled: bool,
    on_click: impl FnMut() + 'static,
) -> Element {
    let active = selected || highlighted;
    let txt_id = format!("{bid}:txt");
    let mut text_el: Element = label(txt_id.as_str())
        .text(label_text.to_string())
        .size(14.0)
        .color(parse_text_color(if active {
            "accent-foreground"
        } else {
            "foreground"
        }))
        .into();
    // 在已匹配触发器宽度的菜单内允许收缩，避免 label 把面板撑破。
    text_el = text_el.flex_grow(1.0).min_w(0.0).flex_shrink(1.0);

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

    row = row
        .w("100%")
        .min_w(0.0)
        .max_w("100%")
        .h("auto")
        .min_h("auto")
        .rounded("0.375rem")
        .gap("0.375rem")
        .py("0.25rem")
        .px("0.375rem")
        .justify_start();
    if active {
        row = row.bg(parse_color("accent"));
    }
    if disabled {
        row = row.opacity(0.5);
    }
    row
}

// ---------------------------------------------------------------------------
//Data Model + Builder
// ---------------------------------------------------------------------------

/// Builder 选项项。
#[derive(Clone, Debug)]
pub struct ComboboxItemData {
    pub value: String,
    pub label: String,
    pub disabled: bool,
}

impl ComboboxItemData {
    pub fn new(value: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
            disabled: false,
        }
    }

    pub fn disabled(mut self, yes: bool) -> Self {
        self.disabled = yes;
        self
    }
}

/// Builder grouping.
#[derive(Clone, Debug, Default)]
pub struct ComboboxGroupData {
    pub id: String,
    pub label: Option<String>,
    pub items: Vec<ComboboxItemData>,
}

impl ComboboxGroupData {
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

    pub fn item(mut self, item: ComboboxItemData) -> Self {
        self.items.push(item);
        self
    }
}

#[derive(Clone, Debug)]
enum ComboboxEntry {
    Item(ComboboxItemData),
    Group(ComboboxGroupData),
    Separator,
}

pub struct ComboboxBuilder {
    id: String,
    value: Option<Signal<String>>,
    default_value: String,
    entries: Vec<ComboboxEntry>,
    placeholder: String,
    placement: PopupPlacement,
    clearable: bool,
    class: String,
}

pub fn combobox(id: impl Into<String>) -> ComboboxBuilder {
    ComboboxBuilder {
        id: id.into(),
        value: None,
        default_value: String::new(),
        entries: Vec::new(),
        placeholder: "Search…".into(),
        placement: PopupPlacement::BottomStart,
        clearable: false,
        class: String::new(),
    }
}

impl ComboboxBuilder {
    pub fn value_signal(mut self, s: Signal<String>) -> Self {
        self.value = Some(s);
        self
    }

    pub fn default_value(mut self, v: impl Into<String>) -> Self {
        self.default_value = v.into();
        self
    }

    pub fn options(
        mut self,
        opts: impl IntoIterator<Item = (impl Into<String>, impl Into<String>)>,
    ) -> Self {
        self.entries.extend(
            opts.into_iter()
                .map(|(v, l)| ComboboxEntry::Item(ComboboxItemData::new(v, l))),
        );
        self
    }

    pub fn item(mut self, item: ComboboxItemData) -> Self {
        self.entries.push(ComboboxEntry::Item(item));
        self
    }

    pub fn group(mut self, group: ComboboxGroupData) -> Self {
        self.entries.push(ComboboxEntry::Group(group));
        self
    }

    pub fn separator(mut self) -> Self {
        self.entries.push(ComboboxEntry::Separator);
        self
    }

    pub fn placeholder(mut self, s: impl Into<String>) -> Self {
        self.placeholder = s.into();
        self
    }

    pub fn placement(mut self, p: PopupPlacement) -> Self {
        self.placement = p;
        self
    }

    pub fn clearable(mut self, v: bool) -> Self {
        self.clearable = v;
        self
    }

    /// Trigger/shell utility class（`w-*` / `min-w-*` 落到壳上，避免默认 `16rem`）。
    pub fn class(mut self, c: impl Into<String>) -> Self {
        self.class = c.into();
        self
    }
}

fn collect_labels(entries: &[ComboboxEntry]) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for e in entries {
        match e {
            ComboboxEntry::Item(i) => {
                map.insert(i.value.clone(), i.label.clone());
            }
            ComboboxEntry::Group(g) => {
                for i in &g.items {
                    map.insert(i.value.clone(), i.label.clone());
                }
            }
            ComboboxEntry::Separator => {}
        }
    }
    map
}

impl From<ComboboxBuilder> for Element {
    fn from(builder: ComboboxBuilder) -> Self {
        let ComboboxBuilder {
            id,
            value,
            default_value,
            entries,
            placeholder,
            placement,
            clearable,
            class,
        } = builder;
        let labels_map = collect_labels(&entries);
        let default_initial = default_value;
        let value = value.unwrap_or_else(|| {
            use_signal(format!("{id}__combo_value").as_str(), move || {
                default_initial.clone()
            })
        });
        // Pre-seed labels signal via scope path.
        let id_owned = id.clone();
        let ph = placeholder.clone();
        combobox_scope(
            id,
            None,
            Some(value),
            ph,
            clearable,
            class,
            String::new(),
            move || {
                // Seed labels before Input sync.
                let ctx = require_combobox("ComboboxBuilder");
                for (v, l) in &labels_map {
                    register_label(ctx.labels, v, l);
                }
                let want = label_for_map(&labels_map, &ctx.value.peek());
                let store = use_signal(input_value_signal_key(&id_owned).as_str(), String::new);
                if store.peek() != want {
                    commit_value_sig(&id_owned, store, want);
                }

                let input_el = ComboboxInput(ComboboxInputProps {
                    class: None,
                    placeholder: Some(placeholder.clone()),
                    clearable: Some(clearable),
                    children: None,
                    base: None,
                });
                let entries_body = entries.clone();
                let placement = placement;
                let content = combobox_content(
                    ComboboxContentOpts {
                        placement: Some(placement),
                        max_height: Some("24rem".into()),
                        ..Default::default()
                    },
                    move || build_builder_entries(&entries_body),
                );
                Element::from(vstack().gap("0").w("100%").child(input_el).child(content))
            },
        )
    }
}

fn build_builder_entries(entries: &[ComboboxEntry]) -> Element {
    let mut col: Element = vstack().w("100%").gap("0").into();
    col = col.accesskit_role("listbox");
    for e in entries {
        match e {
            ComboboxEntry::Item(item) => {
                col = col.child(ComboboxItem(ComboboxItemProps {
                    value: Some(item.value.clone()),
                    disabled: Some(item.disabled),
                    class: None,
                    children: Some(Element::new().text(item.label.clone())),
                    base: None,
                }));
            }
            ComboboxEntry::Group(g) => {
                let mut kids = Vec::new();
                if let Some(title) = &g.label {
                    kids.push(ComboboxLabel(ComboboxLabelProps {
                        children: Some(Element::new().text(title.clone())),
                        base: None,
                    }));
                }
                for item in &g.items {
                    kids.push(ComboboxItem(ComboboxItemProps {
                        value: Some(item.value.clone()),
                        disabled: Some(item.disabled),
                        class: None,
                        children: Some(Element::new().text(item.label.clone())),
                        base: None,
                    }));
                }
                col = col.child(ComboboxGroup(ComboboxGroupProps {
                    children: Some({
                        let mut frag = Element::new();
                        frag.set_attr("rui_fragment", "1");
                        frag.children(kids)
                    }),
                    base: None,
                }));
            }
            ComboboxEntry::Separator => {
                col = col.child(ComboboxSeparator(ComboboxSeparatorProps {
                    children: None,
                    base: None,
                }));
            }
        }
    }
    // Empty state for builder path when filter yields nothing.
    col = col.child(ComboboxEmpty(ComboboxEmptyProps {
        children: Some(Element::new().text("No results.")),
        base: None,
    }));
    col
}

// ---------------------------------------------------------------------------
// RSX `<Combobox>` root
// ---------------------------------------------------------------------------

/// RSX `<Combobox>`：有复合子树时走 [`combobox_scope`]；否则走 Builder。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn Combobox(
    id: String,
    value: Option<Signal<String>>,
    default_value: String,
    options: Vec<(String, String)>,
    placeholder: String,
    clearable: bool,
    class: Option<String>,
    children: Option<Element>,
) -> Element {
    if let Some(c) = children {
        let tc = class.clone().unwrap_or_default();
        return combobox_scope(
            id,
            None,
            value,
            if placeholder.is_empty() {
                "Search…".into()
            } else {
                placeholder
            },
            clearable,
            tc,
            String::new(),
            || c,
        );
    }
    let mut b = combobox(&id)
        .default_value(default_value)
        .options(options)
        .placeholder(if placeholder.is_empty() {
            "Search…".into()
        } else {
            placeholder
        })
        .clearable(clearable);
    if let Some(sig) = value {
        b = b.value_signal(sig);
    }
    let _ = class;
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
    fn combobox_class_overrides_default_16rem_shell() {
        let plain: Element = combobox("cb-plain").options([("a", "A")]).into();
        let sized: Element = combobox("cb-w")
            .options([("a", "A")])
            .class("w-[4.75rem] min-w-[4.75rem]")
            .into();
        match crate::layout::dim_kind(plain.style.size.width) {
            crate::layout::DimKind::Length(px) => {
                assert!(
                    (px - 16.0 * 16.0).abs() < 0.5,
                    "default shell 16rem, got {px}"
                )
            }
            other => panic!("default combobox shell should be 16rem, got {other:?}"),
        }
        match crate::layout::dim_kind(sized.style.size.width) {
            crate::layout::DimKind::Length(px) => assert!(
                (px - 4.75 * 16.0).abs() < 0.5,
                "class should set 4.75rem, got {px}"
            ),
            other => panic!("expected 4.75rem width, got {other:?}"),
        }
    }

    #[test]
    fn sibling_combobox_scopes_keep_distinct_context() {
        use crate::core::reactivity::create_effect;
        use std::cell::RefCell;
        use std::rc::Rc;
        let (_owner, (id_h, id_s)) = create_root(|| {
            let id_h = Rc::new(RefCell::new(String::new()));
            let id_s = Rc::new(RefCell::new(String::new()));
            let _h = combobox_scope("cb-hms-h", None, None, "HH", false, "", "", {
                let id_h = id_h.clone();
                move || {
                    create_effect(move || {
                        *id_h.borrow_mut() = require_combobox("sibling-h").id.clone();
                    });
                    Element::new()
                }
            });
            let _s = combobox_scope("cb-hms-s", None, None, "SS", false, "", "", {
                let id_s = id_s.clone();
                move || {
                    create_effect(move || {
                        *id_s.borrow_mut() = require_combobox("sibling-s").id.clone();
                    });
                    Element::new()
                }
            });
            (id_h, id_s)
        });
        assert_eq!(id_h.borrow().as_str(), "cb-hms-h");
        assert_eq!(id_s.borrow().as_str(), "cb-hms-s");
    }

    #[test]
    fn active_index_follows_zero_padded_value() {
        let items: Vec<ComboboxVisibleItem> = (0..=23)
            .map(|i| ComboboxVisibleItem {
                value: format!("{i:02}"),
                label: format!("{i:02}"),
                disabled: false,
            })
            .collect();
        assert_eq!(active_index_for_value(&items, "09"), 9);
        assert_eq!(active_index_for_value(&items, "00"), 0);
        assert_eq!(active_index_for_value(&[], "09"), 9);
        assert_eq!(active_index_for_value(&items, "xx"), 0);
    }

    #[test]
    fn combobox_builds() {
        let _el: Element = combobox("cb1")
            .options([("a", "Alpha"), ("b", "Beta")])
            .into();
    }

    #[test]
    fn combobox_select_writes_label_to_input_and_value_id() {
        let (_owner, _) = create_root(|| {
            let sig = use_signal("cb_sel_val", || "a".to_string());
            let _el: Element = combobox("cb_sel")
                .value_signal(sig)
                .item(ComboboxItemData::new("a", "Alpha"))
                .item(ComboboxItemData::new("b", "Beta Long Name"))
                .into();
            let store = use_signal(input_value_signal_key("cb_sel").as_str(), String::new);
            assert_eq!(sig.peek(), "a");
            assert_eq!(
                store.peek(),
                "Alpha",
                "input must show full label, not value id"
            );
        });
    }

    #[test]
    fn combobox_group_entries_collect_labels() {
        let entries = vec![
            ComboboxEntry::Group(
                ComboboxGroupData::new("g1")
                    .label("G")
                    .item(ComboboxItemData::new("x", "Xray")),
            ),
            ComboboxEntry::Separator,
            ComboboxEntry::Item(ComboboxItemData::new("y", "Yankee")),
        ];
        let map = collect_labels(&entries);
        assert_eq!(map.get("x").map(String::as_str), Some("Xray"));
        assert_eq!(map.get("y").map(String::as_str), Some("Yankee"));
    }

    #[test]
    fn combobox_menu_chrome_marks_floating() {
        let mut menu = Element::new().absolute();
        mark_floating(&mut menu, "cb_c", "input:cb_c", PopupPlacement::BottomStart);
        menu.set_attr(ATTR_FLOATING_MATCH_TRIGGER, "1");
        assert_eq!(
            menu.attrs.get(ATTR_FLOATING).map(|s| s.as_str()),
            Some("cb_c")
        );
        assert_eq!(
            menu.attrs.get(ATTR_FLOATING_TRIGGER).map(|s| s.as_str()),
            Some("input:cb_c")
        );
        assert_eq!(
            menu.attrs
                .get(ATTR_FLOATING_MATCH_TRIGGER)
                .map(|s| s.as_str()),
            Some("1")
        );
    }
}
