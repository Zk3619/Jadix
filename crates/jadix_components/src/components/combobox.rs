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
//!   <Combobox id="fruit" value={fruit} placeholder="Search fruit…">
//!     <ComboboxInput class="w-full" />
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
//!         .placeholder("Search city...")
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
//! - 关闭时若输入为空则清空选中；否则把输入恢复为当前选中项的 label（effect 写 store，**不**重建 Input）。
//! - 打开时 ArrowUp/Down、Home/End 跳过 disabled 项；过滤后按值保留活动项。
//! - 打开时滚动到选中项：[`ComboboxBuilder::scroll_selected`] / `<Combobox scrollSelected>` /
//!   `<ComboboxContent scrollSelected>`，取值 `none`（默认）| `start` | `center`。
//! - 菜单走 [`crate::utils::anchored_overlay::anchored_overlay_content`]；触发器一次挂载（N.4：禁止用岛刷 Input 文案）。
//! - Builder 的 `items_signal` / `loading_signal` 支持外部结果；`filterable(false)` 保留服务端排序。
//!   用 `combobox_query(id)` 读写原 Input store，或 `query_signal` 绑定应用的查询状态。
//! - `selection_signal` 启用多选，切换后保留菜单和查询；`on_confirm` 让 Enter 确认全部选择
//!   （包括空选择），否则 Enter 切换活动项。`on_change` 在状态更新后、关闭前调用。
//! - `on_create` 接收无匹配查询的去空格名称；加载中、空名称或已有名称不会触发。
//!   插入选项和赋值由调用者负责。Builder 的 `child` 入口也可组合这些行为。

use crate::text_filter::TextFilter;
use std::collections::HashMap;
use std::sync::OnceLock;
use std::{cell::RefCell, rc::Rc};

use crate::app::portal_overlay::{OverlayKind, OverlayMode};
use crate::components::base::{SlotMap, SlotStyleFn, author_class, paint_slot};
use crate::components::button::button;
use crate::components::input::{input, input_adornment_button, input_value_signal_key};
use crate::components::label::label;
use crate::components::scroll_area::scroll_area;
use crate::components::select::SelectScrollSelected;
use crate::components::separator::separator;
use crate::components::virtual_list::virtual_list;
use crate::core::reactivity::{
    Context, Signal, batch, create_context, create_effect, provide_context, untrack, use_context,
    use_signal, with_child_owner,
};
use crate::core::tree::{ATTR_POINTER_EVENTS, Element, LayoutStatus, PointerEvents};
use crate::primitives::r#box::{hstack, vstack};
use crate::primitives::text::text;
use crate::style::{parse_color, parse_text_color, with_class_list};
use crate::tokens;
use crate::utils::anchored_overlay::{
    AnchoredOverlayContentOpts, FloatingMotionKind, FloatingStrategy, Placement,
    anchored_overlay_content, provide_anchored_overlay_context,
};
use crate::utils::lucide_glyph;
use crate::utils::popup::{
    ATTR_FLOATING_H, ATTR_FLOATING_MATCH_TRIGGER, ATTR_FLOATING_MAX_H, ATTR_FLOATING_MIN_H,
    ATTR_FLOATING_MIN_W, ATTR_FLOATING_W, FLOATING_TRIGGER_GAP, PopupPlacement,
    disarm_underlay_close,
};
use crate::utils::take_packed;
use crate::utils::text_field::{commit_value, commit_value_sig};
use crate::utils::text_node::build_icon_text_node;
use winit::keyboard::{Key, NamedKey};

const SLOT_CONTENT: &str = "combobox-content";

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
    /// Trigger focus_id (Content anchor); written when Input mounts.
    pub trigger_fid: Signal<String>,
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
    /// 打开菜单时是否把当前选中项滚进视口（`none` 默认 / `start` / `center`）。
    pub scroll_selected: SelectScrollSelected,
}

/// Origin of an explicit user selection; external signal writes do not emit events.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComboboxChangeReason {
    Keyboard,
    Pointer,
    Clear,
}

/// Selection delta delivered after the controlled state changes, before closing.
/// `Clear` clears the entire selection and carries an empty value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComboboxChange {
    pub value: String,
    pub selected: bool,
    pub reason: ComboboxChangeReason,
}

/// Access the existing input store for an explicit stable Combobox id.
/// While open this is the search query; a closed single select displays its label.
#[derive(Clone, Debug)]
pub struct ComboboxQuery {
    id: String,
}

pub fn combobox_query(id: impl AsRef<str>) -> ComboboxQuery {
    assert!(
        !id.as_ref().is_empty(),
        "combobox_query requires an explicit id"
    );
    ComboboxQuery {
        id: id.as_ref().into(),
    }
}

impl ComboboxQuery {
    pub fn get(&self) -> String {
        use_signal(&input_value_signal_key(&self.id), String::new).get()
    }

    pub fn set(&self, query: impl Into<String>) {
        commit_value(&self.id, query.into());
    }
}

#[derive(Clone)]
struct ComboboxBehavior {
    filterable: bool,
    query: Option<Signal<String>>,
    items: Option<Signal<Vec<ComboboxItemData>>>,
    loading: Option<Signal<bool>>,
    selection: Option<Signal<Vec<String>>>,
    confirm: Option<Rc<dyn Fn(Vec<String>)>>,
    change: Option<Rc<dyn Fn(ComboboxChange)>>,
    create: Option<Rc<dyn Fn(String)>>,
}

impl Default for ComboboxBehavior {
    fn default() -> Self {
        Self {
            filterable: true,
            query: None,
            items: None,
            loading: None,
            selection: None,
            confirm: None,
            change: None,
            create: None,
        }
    }
}

fn combobox_behavior_context() -> Context<ComboboxBehavior> {
    thread_local! {
        static CTX: Context<ComboboxBehavior> = create_context();
    }
    CTX.with(|ctx| *ctx)
}

fn combobox_behavior() -> ComboboxBehavior {
    use_context(combobox_behavior_context()).unwrap_or_default()
}

fn commit_option(
    ctx: &ComboboxCtx,
    behavior: &ComboboxBehavior,
    value: &str,
    label: &str,
    reason: ComboboxChangeReason,
) {
    batch(|| {
        let selected = if let Some(selection) = behavior.selection {
            let next =
                crate::components::toggle_group::multi_select_after_click(&selection.peek(), value);
            let selected = next.iter().any(|item| item == value);
            selection.set(next);
            selected
        } else {
            ctx.value.set(value.into());
            commit_value(&ctx.id, label.into());
            true
        };
        if let Some(change) = &behavior.change {
            change(ComboboxChange {
                value: value.into(),
                selected,
                reason,
            });
        }
        if behavior.selection.is_none() {
            ctx.open.set(false);
        }
    });
}

fn creatable_query(ctx: &ComboboxCtx, behavior: &ComboboxBehavior) -> Option<String> {
    if behavior.create.is_none()
        || !ctx.open.peek()
        || behavior.loading.is_some_and(|loading| loading.peek())
        || !ctx.visible.with_untracked(Vec::is_empty)
    {
        return None;
    }
    let query = use_signal(&input_value_signal_key(&ctx.id), String::new).peek();
    let query = query.trim();
    if query.is_empty() {
        return None;
    }
    let normalized = query.to_lowercase();
    let duplicate = ctx.labels.with_untracked(|labels| {
        labels.iter().any(|(value, label)| {
            value.to_lowercase() == normalized || label.to_lowercase() == normalized
        })
    });
    (!duplicate).then(|| query.to_owned())
}

fn request_creation(ctx: &ComboboxCtx, behavior: &ComboboxBehavior) {
    if let Some(query) = creatable_query(ctx, behavior) {
        batch(|| {
            behavior.create.as_ref().unwrap()(query);
            if behavior.selection.is_none() {
                ctx.open.set(false);
            }
        });
    }
}

fn combobox_context() -> Context<ComboboxCtx> {
    static CTX: OnceLock<Context<ComboboxCtx>> = OnceLock::new();
    *CTX.get_or_init(create_context::<ComboboxCtx>)
}

struct ComboboxFilterPass {
    id: String,
    query: String,
    index: Rc<RefCell<TextFilter>>,
}

fn combobox_filter_context() -> Context<Rc<ComboboxFilterPass>> {
    thread_local! {
        static CTX: Context<Rc<ComboboxFilterPass>> = create_context();
    }
    CTX.with(|ctx| *ctx)
}

fn filtered_rows(
    id: &str,
    query: String,
    index: &Rc<RefCell<TextFilter>>,
    body: &dyn Fn() -> Element,
) -> Element {
    index.borrow_mut().begin(&query);
    with_child_owner(|| {
        provide_context(
            combobox_filter_context(),
            Rc::new(ComboboxFilterPass {
                id: id.to_owned(),
                query,
                index: index.clone(),
            }),
        );
        let rows = untrack(body);
        index.borrow_mut().finish();
        rows
    })
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
    if labels.with_untracked(|m| m.get(value).is_some_and(|l| l == label)) {
        return;
    }
    labels.update_in_place(|m| {
        m.insert(value.to_string(), label.to_string());
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
    if let Some(i) = visible
        .iter()
        .position(|it| !it.disabled && it.value == value)
    {
        return i;
    }
    match value.parse::<usize>() {
        Ok(i) if visible.is_empty() => i,
        Ok(i) if visible.get(i).is_some_and(|item| !item.disabled) => i,
        _ => visible
            .iter()
            .position(|item| !item.disabled)
            .unwrap_or(usize::MAX),
    }
}

fn label_matches_query(label: &str, query: &str) -> bool {
    let mut filter = TextFilter::default();
    filter.begin(query);
    filter.matches([label].into_iter())
}

// ---------------------------------------------------------------------------
// Root scope（宏 `<Combobox>` / 复合子树）
// ---------------------------------------------------------------------------

/// 一个 Combobox **实例**的根：自带 owner，再 `provide_context`。
///
/// Trigger + Content（下拉 portal / Item）都在这棵子树上构建；下拉 effect 的 parent owner
/// 就是本实例，而不是外层 TimePicker / DatePicker。勿把三个 HMS Combobox 建在同一父 owner
/// 上提供 context（后写覆盖前写）。
#[expect(
    clippy::too_many_arguments,
    reason = "The public scope adapter retains its existing signal and event parameters"
)]
pub fn combobox_scope(
    id: impl Into<String>,
    open: Option<Signal<bool>>,
    value: Option<Signal<String>>,
    placeholder: impl Into<String>,
    clearable: bool,
    trigger_class: impl Into<String>,
    content_class: impl Into<String>,
    scroll_selected: impl Into<String>,
    body: impl FnOnce() -> Element,
) -> Element {
    combobox_scope_with_behavior(
        id,
        open,
        value,
        placeholder,
        clearable,
        trigger_class,
        content_class,
        scroll_selected,
        ComboboxBehavior::default(),
        body,
    )
}

#[expect(
    clippy::too_many_arguments,
    reason = "Internal adapter preserves the public scope API"
)]
fn combobox_scope_with_behavior(
    id: impl Into<String>,
    open: Option<Signal<bool>>,
    value: Option<Signal<String>>,
    placeholder: impl Into<String>,
    clearable: bool,
    trigger_class: impl Into<String>,
    content_class: impl Into<String>,
    scroll_selected: impl Into<String>,
    behavior: ComboboxBehavior,
    body: impl FnOnce() -> Element,
) -> Element {
    let id = crate::utils::resolve_instance_id(id, "combo");
    let placeholder = placeholder.into();
    let trigger_class = trigger_class.into();
    let content_class = content_class.into();
    let scroll_selected = SelectScrollSelected::parse(&scroll_selected.into());
    let open = open.unwrap_or_else(|| use_signal(format!("{id}__combo_open").as_str(), || false));
    let value =
        value.unwrap_or_else(|| use_signal(format!("{id}__combo_value").as_str(), String::new));
    let active = use_signal(format!("{id}__combo_active").as_str(), || 0usize);
    let labels = use_signal(format!("{id}__combo_labels").as_str(), HashMap::new);
    let visible = use_signal(
        format!("{id}__combo_visible").as_str(),
        Vec::<ComboboxVisibleItem>::new,
    );
    let trigger_fid = use_signal(format!("{id}__combo_trigger_fid").as_str(), || {
        format!("input:{id}")
    });
    // 输入 store：与 value 分离；若已有选中则灌入 label。
    let want = if behavior.selection.is_some() {
        String::new()
    } else {
        labels.with_untracked(|m| label_for_map(m, &value.peek()))
    };
    let input_store = use_signal(input_value_signal_key(&id).as_str(), || want.clone());
    if !open.peek() && input_store.peek() != want && !want.is_empty() {
        commit_value_sig(&id, input_store, want);
    }

    // 实例 owner：context + Input + Content 同属这一 Combobox，下拉不是父级的。
    // 壳宽：Builder `.class()` 的 trigger_class，或 `<ComboboxInput class="w-*">` 的 chrome。
    // 都没有宽度 token 时默认 16rem（避免撑满卡片）。
    with_child_owner(move || {
        if let Some(query) = behavior.query {
            crate::utils::text_field::bind_text_query_signal(&id, query);
        }
        provide_context(combobox_behavior_context(), behavior);
        provide_anchored_overlay_context(open, id.clone(), trigger_fid);
        provide_context(
            combobox_context(),
            ComboboxCtx {
                id: id.clone(),
                open,
                trigger_fid,
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
                scroll_selected,
            },
        );
        let mut root = body();
        let kids = take_packed(root);
        let child_class = kids.first().map(author_class).unwrap_or_default();
        let width_src = format!("{trigger_class} {child_class}");
        let full_width = class_is_full_width(&width_src);
        let has_width = class_has_width_token(&width_src);
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
        let width_tokens = width_class_tokens(&width_src);
        if !width_tokens.is_empty() {
            // `w-*` 落到壳上，Input 保持 `w-100%` 填满。
            root = with_class_list(root, &width_tokens);
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

fn width_class_tokens(class: &str) -> String {
    class
        .split_whitespace()
        .filter(|t| {
            let t = t.trim_end_matches('!');
            t.starts_with("w-") || t.starts_with("min-w-") || t.starts_with("max-w-")
        })
        .collect::<Vec<_>>()
        .join(" ")
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

crate::__component_contract! {
ComboboxInputProps [];
/// ShadCN Aria `ComboboxInput`：可过滤 Input + chevron（可选 clear）。
#[allow(non_snake_case)]
pub fn ComboboxInput(
plain [] placeholder: Option<String>,
plain [] clearable: Option<bool>,
plain [] children: Option<Element>,
) -> Element {
    let _ = children;
    let ctx = require_combobox("ComboboxInput");
    let behavior = combobox_behavior();
    let id = ctx.id.clone();
    let trigger_id = format!("input:{id}");
    ctx.trigger_fid.set(trigger_id);
    let open = ctx.open;
    let value = ctx.value;
    let active = ctx.active;
    let labels = ctx.labels;
    let visible = ctx.visible;
    let open_from_end =
        use_signal(format!("{}__combo_open_from_end", id).as_str(), || false);
    let ph = placeholder
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| ctx.placeholder.clone());
    let clearable = clearable.unwrap_or(ctx.clearable);
    let class = ctx.trigger_class.clone();
    let input_store = use_signal(input_value_signal_key(&id).as_str(), String::new);

    {
        let id = id.clone();
        let multi = behavior.selection.is_some();
        create_effect(move || {
            let Some(is_open) = open.try_get() else {
                return;
            };
            if is_open {
                return;
            }
            let Some(selected) = value.try_get() else {
                return;
            };
            // Peek: this effect writes `input_store`. `get()` would subscribe and
            // re-enter the same FnMut on commit (Gallery forms Combobox).
            let Some(query) = input_store.try_peek() else {
                return;
            };
            let Some(map) = labels.try_get() else {
                return;
            };
            let selected_label = label_for_map(&map, &selected);
            if multi || selected.is_empty() {
                if !query.is_empty() {
                    commit_value_sig(&id, input_store, String::new());
                }
            } else if query != selected_label {
                commit_value_sig(&id, input_store, selected_label);
            }
        });
    }

    let o_focus = open;
    let a_sig = active;
    let val_sig = value;
    let vis = visible;

    // Combobox trigger is a real Input: Hidden overflow (crop + caret pan), not ellipsis.
    // Mount once — do not wrap in reactive_anchor (that rebuilt the field on every keystroke).
    let mut trigger: Element = input(&id).placeholder(ph.as_str()).w("100%").into();
    trigger = trigger
        .accesskit_role("combobox")
        .accesskit_name(ph.as_str());
    trigger.set_attr("aria-haspopup", "listbox");
    trigger.set_attr("aria-autocomplete", "list");
    let relation_id = id.clone();
    let listbox_id = format!("{id}:listbox");
    trigger = crate::app::reactive::bind_attr(
        trigger,
        format!("{id}__combo_semantics"),
        move || {
            let expanded = open.get();
            let index = active.get();
            let option = visible.with(|items| {
                items.get(index).filter(|item| !item.disabled).map(|item| {
                    format!("{relation_id}:opt:{}", item.value)
                })
            });
            (expanded, option)
        },
        move |element, (expanded, option)| {
            element.set_attr("aria-expanded", if expanded { "true" } else { "false" });
            if expanded {
                element.set_attr("aria-controls", listbox_id.clone());
            } else {
                element.attrs.remove("aria-controls");
            }
            if let Some(option) = option.filter(|_| expanded) {
                element.set_attr("aria-activedescendant", option);
            } else {
                element.attrs.remove("aria-activedescendant");
            }
            LayoutStatus::StyleChanged
        },
    );

    if clearable {
        let clear_val = value;
        let clear_open = open;
        let clear_id = id.clone();
        let clear_store = input_store;
        let clear_behavior = behavior.clone();
        let mut clear_btn =
            input_adornment_button(format!("{id}__combo_clear"), "x", false, move || {
                batch(|| {
                if let Some(selection) = clear_behavior.selection {
                    selection.set(Vec::new());
                }
                clear_val.set(String::new());
                commit_value(&clear_id, String::new());
                if let Some(change) = &clear_behavior.change {
                    change(ComboboxChange { value: String::new(), selected: false, reason: ComboboxChangeReason::Clear });
                }
                clear_open.set(false);
                use_signal("__jadix_button_focus_id", String::new)
                    .set(crate::utils::text_field::input_focus_id(&clear_id));
                use_signal("__jadix_focus_visible", || false).set(true);
                });
            });
        let clear_selection = behavior.selection;
        clear_btn = clear_btn.reactive_sync(move |el, _| {
            let show = !clear_store.try_peek().unwrap_or_default().is_empty()
                || !clear_val.try_peek().unwrap_or_default().is_empty()
                || clear_selection.is_some_and(|selection| selection.with_untracked(|items| !items.is_empty()));
            let want_opacity = if show { 1.0 } else { 0.0 };
            let want_pe = if show {
                PointerEvents::Auto
            } else {
                PointerEvents::None
            };
            let opacity_same = (el.style.opacity - want_opacity).abs() < 0.001;
            let pe_same = el.pointer_events_mode() == want_pe;
            if opacity_same && pe_same {
                return LayoutStatus::Clean;
            }
            el.style.opacity = want_opacity;
            el.set_attr(ATTR_POINTER_EVENTS, want_pe.as_str());
            LayoutStatus::StyleChanged
        });
        trigger = trigger.gap("0.25rem").child(clear_btn);
    }

    let o_chev = open;
    let a_chev = active;
    let v_chev = value;
    let vis_chev = visible;
    let chev_id = id.clone();
    let chevron = input_adornment_button(
        format!("{id}__combo_chevron"),
        "chevron-down",
        false,
        move || {
            let next = !o_chev.peek();
            if next {
                a_chev.set(active_index_for_value(&vis_chev.peek(), &v_chev.peek()));
                disarm_underlay_close();
            }
            o_chev.set(next);
            use_signal("__jadix_button_focus_id", String::new)
                .set(crate::utils::text_field::input_focus_id(&chev_id));
            use_signal("__jadix_focus_visible", || false).set(true);
        },
    );
    trigger = trigger.gap("0.25rem").child(chevron);

    let reveal = use_signal(&format!("{}__combo_virtual_focus", ctx.id), || usize::MAX);
    trigger = trigger
        .on_click({



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
                    if let Some(next) = crate::utils::list_nav::list_nav_move_skipping(
                        &ev.logical_key,
                        n,
                        a_sig.peek(),
                        |i| list[i].disabled,
                    ) {
                        a_sig.set(next);
                        reveal.set_silent(next);
                    }
                    return true;
                }
                Key::Named(NamedKey::ArrowDown) | Key::Named(NamedKey::ArrowUp) => {
                    if !o_focus.peek() {
                        let selected = val_sig.peek();
                        let index = if selected.is_empty()
                            && ev.logical_key == Key::Named(NamedKey::ArrowUp)
                        {
                            open_from_end.set(true);
                            list.iter().rposition(|item| !item.disabled).unwrap_or(usize::MAX)
                        } else {
                            active_index_for_value(&list, &selected)
                        };
                        a_sig.set(index);
                        disarm_underlay_close();
                        o_focus.set(true);
                    }
                    return true;
                }
                Key::Named(NamedKey::Enter) => {
                    if !o_focus.peek() {
                        a_sig.set(active_index_for_value(&list, &val_sig.peek()));
                        disarm_underlay_close();
                        o_focus.set(true);
                        return true;
                    }
                    if let (Some(selection), Some(confirm)) = (behavior.selection, &behavior.confirm) {
                        batch(|| {
                            confirm(selection.peek());
                            o_focus.set(false);
                        });
                        return true;
                    }
                    if let Some(opt) = list.get(a_sig.peek()) {
                        if !opt.disabled {
                            commit_option(&ctx, &behavior, &opt.value, &opt.label, ComboboxChangeReason::Keyboard);
                        }
                        return true;
                    }
                    // Creation is explicit, only for an unmatched nonempty query,
                    // and never while an asynchronous result is still pending.
                    request_creation(&ctx, &behavior);
                    return true;
                }
                Key::Named(NamedKey::Escape) if o_focus.peek() => {
                    o_focus.set(false);
                    return true;
                }
                Key::Named(NamedKey::Tab) if o_focus.peek() => {
                    o_focus.set(false);
                    // Continue normal forward/backward focus traversal without committing.
                    return false;
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
}
}

// ---------------------------------------------------------------------------
// ComboboxContent
// ---------------------------------------------------------------------------

/// `<ComboboxContent>` / [`combobox_content`] 选项。
#[derive(Clone, Default)]
pub struct ComboboxContentOpts {
    pub panel: crate::components::base::SlotChrome,
    pub match_trigger: Option<bool>,
    pub menu_width: Option<String>,
    pub menu_height: Option<String>,
    pub min_height: Option<String>,
    pub max_height: Option<String>,
    pub placement: Option<PopupPlacement>,
    /// `none` | `start` | `center`；空则用根 [`ComboboxCtx::scroll_selected`]。
    pub scroll_selected: Option<String>,
}

impl ComboboxContentOpts {
    crate::impl_content_opts_chrome!(panel);
}

/// ShadCN `ComboboxContent`：Portal 浮层 + 可滚动视口；仅在 open 时构建。
pub fn combobox_content(
    opts: ComboboxContentOpts,
    body: impl Fn() -> Element + 'static,
) -> Element {
    let ctx = require_combobox("ComboboxContent");
    let behavior = combobox_behavior();
    let open = ctx.open;
    let active = ctx.active;
    let value = ctx.value;
    let labels = ctx.labels;
    let visible = ctx.visible;
    let id = ctx.id.clone();
    let match_trigger = opts.match_trigger.unwrap_or(ctx.match_trigger);
    let menu_width = opts.menu_width.or(ctx.menu_width.clone());
    let menu_height = opts.menu_height.or(ctx.menu_height.clone());
    let min_height = opts.min_height.or(ctx.min_height.clone());
    let max_height = opts
        .max_height
        .or(ctx.max_height.clone())
        .or_else(|| Some("24rem".into()));
    let placement = opts.placement.unwrap_or(ctx.placement);
    let panel = opts.panel.fallback_class(&ctx.content_class);
    let scroll_mode = opts
        .scroll_selected
        .as_deref()
        .map(SelectScrollSelected::parse)
        .unwrap_or(ctx.scroll_selected);

    // 预扫注册 labels，避免首屏输入框闪 value id。
    if labels.with_untracked(HashMap::is_empty) {
        visible.set(Vec::new());
        let _ = body();
    }

    let id_for_effect = id.clone();
    let input_store = use_signal(input_value_signal_key(&id).as_str(), String::new);
    let text_filter = use_signal(format!("{id}__combo_text_filter").as_str(), || {
        Rc::new(RefCell::new(TextFilter::default()))
    })
    .peek();
    let ao_opts = AnchoredOverlayContentOpts {
        placement: Some(Placement::trigger(
            String::new(),
            placement,
            FLOATING_TRIGGER_GAP,
            FloatingStrategy::Auto,
        )),
        kind: OverlayKind::Menu,
        lock_page_scroll: Some(true),
        motion: FloatingMotionKind::Expand,
        mode: OverlayMode::Stacked,
        match_trigger_width: match_trigger,
        max_h: max_height.clone(),
        width: menu_width.clone(),
        ..Default::default()
    };
    let body = std::rc::Rc::new(body);
    let content = anchored_overlay_content(ao_opts, move || {
        // Overlay chrome isolates reads. Give the changing rows their own owner
        // so typing and navigation update the list without remounting Input.
        let rows_id = format!("{id_for_effect}:rows__view");
        let scroll_id = format!("{id_for_effect}:menu_scroll");
        let rows_body = body.clone();
        let rows_scroll_id = scroll_id.clone();
        let text_filter = text_filter.clone();
        let rows_instance_id = id_for_effect.clone();
        let behavior = behavior.clone();
        let open_from_end = use_signal(
            format!("{}__combo_open_from_end", id_for_effect).as_str(),
            || false,
        );
        crate::app::reactive::reactive_anchor(rows_id.clone(), move || {
            let _ = active.get();
            let _ = input_store.get();
            let _ = value.get();
            if let Some(items) = behavior.items {
                items.with(|_| ());
            }
            if let Some(selection) = behavior.selection {
                selection.with(|_| ());
            }
            let busy = behavior.loading.is_some_and(|loading| loading.get());
            let opening_from_end = open_from_end.get();

            let cur = value.peek();
            // Preserve the active value across rebuilds/filtering. A keyboard move
            // must not snap back to the selected value when the rows repaint.
            let active_value = visible
                .peek()
                .get(active.peek())
                .map(|item| item.value.clone());
            visible.set(Vec::new());
            let mut rows = filtered_rows(
                &rows_instance_id,
                input_store.peek(),
                &text_filter,
                &*rows_body,
            );
            let list = visible.peek();
            let had_active = active_value.is_some();
            let ix = active_value
                .and_then(|value| {
                    list.iter()
                        .position(|item| !item.disabled && item.value == value)
                })
                .or_else(|| {
                    opening_from_end
                        .then(|| list.iter().rposition(|item| !item.disabled))
                        .flatten()
                })
                .unwrap_or_else(|| active_index_for_value(&list, &cur));
            if opening_from_end {
                open_from_end.set_silent(false);
            }
            if active.peek() != ix {
                active.set_silent(ix);
                visible.set(Vec::new());
                rows = filtered_rows(
                    &rows_instance_id,
                    input_store.peek(),
                    &text_filter,
                    &*rows_body,
                );
            }
            let scroll_id = &rows_scroll_id;
            rows.set_attr("id", format!("{rows_instance_id}:listbox"));
            rows.set_attr("aria-busy", if busy { "true" } else { "false" });
            rows.set_attr(
                "accesskit_size_of_set",
                visible.with_untracked(Vec::len).to_string(),
            );
            if behavior.selection.is_some() {
                rows.set_attr("aria-multiselectable", "true");
            }
            rows = rows.accesskit_role("listbox");
            let n = visible.peek().len().max(1);
            if !open.peek() {
                crate::authoring::widgets::scroll::reset_scroll_item_into_view(&scroll_id);
            } else if rows.attrs.contains_key("jadix_combobox_virtual") {
                if scroll_mode != SelectScrollSelected::None || had_active {
                    use_signal(
                        &format!("{}__combo_virtual_focus", rows_instance_id),
                        || usize::MAX,
                    )
                    .set_silent(ix);
                }
            } else if scroll_mode != SelectScrollSelected::None && n > 1 && ix < n {
                let align = match scroll_mode {
                    SelectScrollSelected::Center => {
                        crate::authoring::widgets::scroll::ScrollItemAlign::Center
                    }
                    SelectScrollSelected::Start => {
                        crate::authoring::widgets::scroll::ScrollItemAlign::Start
                    }
                    SelectScrollSelected::None => unreachable!(),
                };
                crate::authoring::widgets::scroll::request_scroll_item_into_view(
                    scroll_id.as_str(),
                    ix,
                    n,
                    align,
                );
            }
            if rows.attrs.contains_key("jadix_combobox_virtual") {
                rows
            } else {
                scroll_area(&rows_scroll_id)
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
            }
        });
        let viewport = crate::app::reactive::reactive_placeholder_sized(rows_id, "100%", "auto")
            .min_h(0.0)
            .flex_grow(1.0)
            .flex_shrink(1.0);

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
    });
    if crate::core::reactivity::is_tracking() {
        let open_reset = open;
        crate::core::reactivity::on_cleanup(move || {
            if open_reset.try_peek() == Some(true) {
                open_reset.try_set(false);
            }
        });
    }
    content
}

// ---------------------------------------------------------------------------
// ComboboxList / Group / Label / Item / Separator / Empty
// ---------------------------------------------------------------------------

crate::__component_contract! {
ComboboxListProps [];
/// ShadCN Aria `ComboboxList`（可滚动列表容器；过滤由 Item 自行隐藏）。
#[allow(non_snake_case)]
pub fn ComboboxList(
plain [] children: Option<Element>,
) -> Element {
    let _ = require_combobox("ComboboxList");
    let kids = children.map(take_packed).unwrap_or_default();
    let mut el: Element = vstack().w("100%").gap("0").children(kids).into();
    el = el.accesskit_role("listbox");
    el
}
}

crate::__component_contract! {
ComboboxGroupProps [];
/// ShadCN Aria `ComboboxGroup`。
#[allow(non_snake_case)]
pub fn ComboboxGroup(
plain [] children: Option<Element>,
) -> Element {
    let _ = require_combobox("ComboboxGroup");
    let kids = children.map(take_packed).unwrap_or_default();
    Element::from(vstack().w("100%").gap("0").children(kids))
}
}

crate::__component_contract! {
ComboboxLabelProps [];
/// ShadCN Aria `ComboboxLabel`。
#[allow(non_snake_case)]
pub fn ComboboxLabel(
plain [] children: Option<Element>,
) -> Element {
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
}

crate::__component_contract! {
ComboboxItemProps [];
/// ShadCN Aria `ComboboxItem`。
#[allow(non_snake_case)]
pub fn ComboboxItem(
default [] value: String,
default [] disabled: bool,
plain [] children: Option<Element>,
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

    let selected = ctx.value.peek();
    let selected_label = ctx.labels.with_untracked(|m| label_for_map(m, &selected));
    let pass = use_context(combobox_filter_context()).filter(|pass| pass.id == ctx.id && pass.index.borrow().is_active());
    let query = pass.as_ref().map_or_else(
        || std::borrow::Cow::Owned(use_signal(input_value_signal_key(&ctx.id).as_str(), String::new).peek()),
        |pass| std::borrow::Cow::Borrowed(pass.query.as_str()),
    );
    let filtering = combobox_behavior().filterable && filter_active(ctx.open.peek(), &query, &selected, &selected_label);
    let matches = if let Some(pass) = &pass {
        pass.index.borrow_mut().matches([label_text.as_str()].into_iter())
    } else {
        label_matches_query(&label_text, &query)
    };
    if filtering && !matches {
        return Element::new().w(0.0).h(0.0);
    }

    combobox_registered_item(&ctx, value, label_text, disabled, if filtering { &query } else { "" })
}
}

fn combobox_registered_item(
    ctx: &ComboboxCtx,
    value: String,
    label_text: String,
    disabled: bool,
    query: &str,
) -> Element {
    let ix = ctx.visible.update_in_place(|list| {
        let ix = list.len();
        list.push(ComboboxVisibleItem {
            value: value.clone(),
            label: label_text.clone(),
            disabled,
        });
        ix
    });

    combobox_indexed_item(ctx, value, label_text, disabled, query, ix)
}

fn combobox_indexed_item(
    ctx: &ComboboxCtx,
    value: String,
    label_text: String,
    disabled: bool,
    query: &str,
    ix: usize,
) -> Element {
    let behavior = combobox_behavior();
    let active = ctx.active;
    let highlighted = !disabled && active.peek() == ix;
    let is_selected = if let Some(selection) = behavior.selection {
        selection.with_untracked(|selected| selected.contains(&value))
    } else {
        ctx.value.with_untracked(|selected| selected == &value)
    };
    let bid = format!("{}:opt:{}", ctx.id, value);
    let ov = value.clone();
    let ol = label_text.clone();
    let item_ctx = ctx.clone();

    let mut row = combobox_item_row(
        &bid,
        &label_text,
        query,
        is_selected,
        highlighted,
        disabled,
        move || {
            if disabled {
                return;
            }
            active.set(ix);
            commit_option(
                &item_ctx,
                &behavior,
                &ov,
                &ol,
                ComboboxChangeReason::Pointer,
            );
        },
    );
    row.set_attr("jadix_combobox_item", value.as_str());
    row.set_attr("id", bid);
    row.set_attr("aria-selected", if is_selected { "true" } else { "false" });
    row.set_attr("aria-disabled", disabled.to_string());
    row.set_attr("aria-posinset", (ix + 1).to_string());
    row.set_attr(
        "aria-setsize",
        ctx.visible.with_untracked(Vec::len).to_string(),
    );
    let visible = ctx.visible;
    crate::authoring::presentation::attach(
        &mut row,
        crate::authoring::presentation::Phase::State,
        move |element, _, _| {
            let size = visible.with_untracked(Vec::len).to_string();
            if element.attrs.get("aria-setsize") == Some(&size) {
                LayoutStatus::Clean
            } else {
                element.set_attr("aria-setsize", size);
                LayoutStatus::StyleChanged
            }
        },
    );
    row.accesskit_role("option").focusable(false)
}

crate::__component_contract! {
ComboboxSeparatorProps [];
/// ShadCN / solid-ui menu dividing line: `-mx-1 my-1 h-px bg-border` (same as [`crate::components::select::SelectSeparator`]).
#[allow(non_snake_case)]
pub fn ComboboxSeparator(
plain [] children: Option<Element>,
) -> Element {
    let _ = children;
    let _ = require_combobox("ComboboxSeparator");
    separator().menu_bleed().pointer_events_none()
}
}

crate::__component_contract! {
ComboboxEmptyProps [];
/// ShadCN Aria `ComboboxEmpty`：请放在 List **之后**，以便可见项已登记。
#[allow(non_snake_case)]
pub fn ComboboxEmpty(
plain [] children: Option<Element>,
) -> Element {
    let ctx = require_combobox("ComboboxEmpty");
    let behavior = combobox_behavior();
    if ctx.open.peek() && behavior.loading.is_some_and(|loading| loading.peek()) {
        return Element::from(text(crate::tr_default_zh("jadix.combobox.loading", "Loading…", "加载中…")))
            .accesskit_role("status");
    }
    if let Some(query) = creatable_query(&ctx, &behavior) {
        let caption = format!("{} “{query}”", crate::tr_default_zh("jadix.combobox.create", "Create", "创建"));
        return Element::from(button(format!("{}:create", ctx.id))
            .variant("ghost").w("100%")
            .text(caption)
            .on_click(move || request_creation(&ctx, &behavior)))
            .focusable(false);
    }
    let msg = match &children {
        Some(c) => c
            .text
            .clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| extract_text_leaf(c)),
        None => crate::tr_default_zh("jadix.combobox.empty", "No results.", "无匹配结果。"),
    };
    if !ctx.open.peek() || !ctx.visible.with_untracked(Vec::is_empty)
        || combobox_behavior().loading.is_some_and(|loading| loading.peek()) {
        return Element::new().w(0.0).h(0.0);
    }
    let empty: Element = text(msg.as_str())
        .size(14.0)
        .color(parse_text_color("muted-foreground"))
        .into();
    Element::from(vstack().w("100%").px("0.375rem").py("0.5rem").child(empty))
}
}

/// Aria `cn-combobox-item` line.
fn combobox_item_row(
    bid: &str,
    label_text: &str,
    query: &str,
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
    if !query.is_empty() {
        text_el = text_el.text_spans(crate::style::TextSpan::highlight_matches(
            label_text,
            query,
            parse_text_color("match-foreground"),
        ));
    }
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
#[derive(Clone, Debug, PartialEq, Eq)]
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
    open: Option<Signal<bool>>,
    value: Option<Signal<String>>,
    behavior: ComboboxBehavior,
    child: Option<Rc<dyn Fn() -> Element>>,
    default_value: String,
    entries: Vec<ComboboxEntry>,
    placeholder: String,
    placement: PopupPlacement,
    clearable: bool,
    class: String,
    style: Option<SlotStyleFn>,
    slots: SlotMap,
    scroll_selected: SelectScrollSelected,
}

pub fn combobox(id: impl Into<String>) -> ComboboxBuilder {
    ComboboxBuilder {
        id: id.into(),
        open: None,
        value: None,
        behavior: ComboboxBehavior::default(),
        child: None,
        default_value: String::new(),
        entries: Vec::new(),
        placeholder: crate::tr_default_zh("jadix.combobox.search", "Search…", "搜索…"),
        placement: PopupPlacement::BottomStart,
        clearable: false,
        class: String::new(),
        style: None,
        slots: SlotMap::default(),
        scroll_selected: SelectScrollSelected::None,
    }
}

impl ComboboxBuilder {
    /// Bind the committed input text, independently of selected value IDs. Closed
    /// single-select inputs publish their selected label; multi-select publishes
    /// the cleared query. External writes wait during focus/IME; local edits win.
    /// Applies to Builder results and `child` composition.
    pub fn query_signal(mut self, query: Signal<String>) -> Self {
        self.behavior.query = Some(query);
        self
    }

    crate::impl_slot_map_methods!();
    crate::impl_overlay_content_slot!(SLOT_CONTENT);

    pub fn open_signal(mut self, open: Signal<bool>) -> Self {
        self.open = Some(open);
        self
    }

    /// Keep externally ranked/filtered results in their supplied order.
    pub fn filterable(mut self, filterable: bool) -> Self {
        self.behavior.filterable = filterable;
        self
    }

    /// Replace asynchronous result metadata without remounting the input.
    /// Combine with filterable(false) for results ranked by a service.
    pub fn items_signal(mut self, items: Signal<Vec<ComboboxItemData>>) -> Self {
        self.behavior.items = Some(items);
        self
    }

    /// Pending results expose aria-busy and suppress the empty message/creation.
    pub fn loading_signal(mut self, loading: Signal<bool>) -> Self {
        self.behavior.loading = Some(loading);
        self
    }

    /// Stable unique option values; toggling retains both query and open menu.
    /// Selected values can remain outside the current result set.
    pub fn selection_signal(mut self, selection: Signal<Vec<String>>) -> Self {
        self.behavior.selection = Some(selection);
        self
    }

    /// In multi select, Enter confirms the entire selection, including empty.
    /// Without this callback, Enter toggles the highlighted option.
    pub fn on_confirm(mut self, confirm: impl Fn(Vec<String>) + 'static) -> Self {
        self.behavior.confirm = Some(Rc::new(confirm));
        self
    }

    pub fn on_change(mut self, change: impl Fn(ComboboxChange) + 'static) -> Self {
        self.behavior.change = Some(Rc::new(change));
        self
    }

    /// Enter with no matching option requests creation of a trimmed query.
    /// The caller owns insertion and value assignment; no synthetic option is stored.
    pub fn on_create(mut self, create: impl Fn(String) + 'static) -> Self {
        self.behavior.create = Some(Rc::new(create));
        self
    }

    /// Compose Input/Content/Item under this Builder's instance and behavior.
    /// Custom children retain their own rendering strategy.
    pub fn child(mut self, child: impl Fn() -> Element + 'static) -> Self {
        self.child = Some(Rc::new(child));
        self
    }

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

    /// Trigger 宽度别名（`w-*` / `min-w-*` 落到壳上，避免默认 `16rem`）。
    /// RSX 请写在 `<ComboboxInput class="…">`。
    pub fn class(mut self, c: impl Into<String>) -> Self {
        self.class = c.into();
        self
    }
    pub fn style(
        mut self,
        f: impl Fn(
            crate::components::base::BaseComponentBuilder,
        ) -> crate::components::base::BaseComponentBuilder
        + 'static,
    ) -> Self {
        self.style = Some(std::rc::Rc::new(f));
        self
    }
    /// 打开时滚动到当前选中项：`none`（默认）| `start` | `center`。
    pub fn scroll_selected(mut self, v: impl AsRef<str>) -> Self {
        self.scroll_selected = SelectScrollSelected::parse(v.as_ref());
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
            open,
            value,
            behavior,
            child,
            default_value,
            entries,
            placeholder,
            placement,
            clearable,
            class,
            style,
            slots,
            scroll_selected,
        } = builder;
        assert!(
            behavior.confirm.is_none() || behavior.selection.is_some(),
            "Combobox on_confirm requires selection_signal"
        );
        let id = crate::utils::resolve_instance_id(id, "combo");
        let default_initial = default_value;
        let value = value.unwrap_or_else(|| {
            use_signal(format!("{id}__combo_value").as_str(), move || {
                default_initial.clone()
            })
        });
        let labels = use_signal(
            format!("{id}__combo_labels").as_str(),
            HashMap::<String, String>::new,
        );
        if child.is_none() {
            if let Some(items) = behavior.items {
                let selection = behavior.selection;
                // Metadata follows the result set even while closed. Keep the
                // selected labels when remote results omit them; prune all other
                // stale metadata. Navigation does not rebuild this map.
                create_effect(move || {
                    let mut map: HashMap<String, String> = items.with(|items| {
                        items
                            .iter()
                            .map(|item| (item.value.clone(), item.label.clone()))
                            .collect()
                    });
                    let selected = selection.map_or_else(|| vec![value.get()], |s| s.get());
                    labels.with_untracked(|current| {
                        for selected in selected {
                            if !map.contains_key(&selected) {
                                if let Some(label) = current.get(&selected) {
                                    map.insert(selected, label.clone());
                                }
                            }
                        }
                    });
                    if labels.with_untracked(|current| current != &map) {
                        labels.set(map);
                    }
                });
            } else {
                let map = collect_labels(&entries);
                if labels.with_untracked(|current| current != &map) {
                    labels.set(map);
                }
            }
        }
        // Pre-seed labels signal via scope path.
        let ph = placeholder.clone();
        let content_chrome = slots.get(SLOT_CONTENT);
        let content_class = content_chrome.class.clone();
        let mut root = combobox_scope_with_behavior(
            id,
            open,
            Some(value),
            ph,
            clearable,
            class,
            content_class,
            scroll_selected.as_str(),
            behavior,
            move || {
                if let Some(child) = &child {
                    return child();
                }

                let input_el = ComboboxInput(ComboboxInputProps {
                    placeholder: Some(placeholder.clone()),
                    clearable: Some(clearable),
                    children: None,
                    base: None,
                });
                let entries_body = Rc::new(entries);
                let placement = placement;
                let content = combobox_content(
                    ComboboxContentOpts {
                        panel: content_chrome,
                        placement: Some(placement),
                        max_height: Some("24rem".into()),
                        scroll_selected: Some(scroll_selected.as_str().into()),
                        ..Default::default()
                    },
                    move || build_builder_entries(entries_body.clone()),
                );
                Element::from(vstack().gap("0").w("100%").child(input_el).child(content))
            },
        );
        if let Some(style) = style {
            root = style(crate::components::base::BaseComponentBuilder::new(root)).el;
        }
        root
    }
}

#[derive(Clone, Copy)]
enum ComboboxBuilderRow {
    Item {
        entry: usize,
        child: Option<usize>,
        index: usize,
    },
    Heading(usize),
    Separator,
}

fn build_builder_entries(entries: Rc<Vec<ComboboxEntry>>) -> Element {
    let ctx = require_combobox("ComboboxBuilder");
    let behavior = combobox_behavior();
    let entries = behavior.items.map_or(entries, |items| {
        Rc::new(items.peek().into_iter().map(ComboboxEntry::Item).collect())
    });
    let pass = use_context(combobox_filter_context())
        .filter(|pass| pass.id == ctx.id && pass.index.borrow().is_active());
    let query = pass.as_ref().map_or_else(
        || use_signal(input_value_signal_key(&ctx.id).as_str(), String::new).peek(),
        |pass| pass.query.clone(),
    );
    let selected = ctx.value.peek();
    let selected_label = ctx
        .labels
        .with_untracked(|labels| label_for_map(labels, &selected));
    let filtering =
        behavior.filterable && filter_active(ctx.open.peek(), &query, &selected, &selected_label);
    let index = pass.as_ref().map_or_else(
        || {
            let mut index = TextFilter::default();
            index.begin(&query);
            Rc::new(RefCell::new(index))
        },
        |pass| pass.index.clone(),
    );
    // Filter and register the full logical list before mounting any viewport rows.
    // ponytail: filtering still scans O(N) metadata; Element construction is bounded
    // by the viewport for large lists. Compound custom children keep their own path.
    let mut rows = Vec::new();
    let mut item_rows = Vec::new();
    let mut visible = Vec::new();
    let mut separator_pending = false;
    for (entry, value) in entries.iter().enumerate() {
        let items = match value {
            ComboboxEntry::Item(item) => std::slice::from_ref(item),
            ComboboxEntry::Group(group) => group.items.as_slice(),
            ComboboxEntry::Separator => {
                separator_pending = !rows.is_empty();
                continue;
            }
        };
        let mut first = true;
        for (child, item) in items.iter().enumerate() {
            register_label(ctx.labels, &item.value, &item.label);
            let matched = index
                .borrow_mut()
                .matches([item.label.as_str()].into_iter());
            if filtering && !matched {
                continue;
            }
            if first {
                if separator_pending {
                    rows.push(ComboboxBuilderRow::Separator);
                }
                separator_pending = false;
                if let ComboboxEntry::Group(group) = value {
                    if group.label.is_some() {
                        rows.push(ComboboxBuilderRow::Heading(entry));
                    }
                }
                first = false;
            }
            let index = visible.len();
            item_rows.push(rows.len());
            rows.push(ComboboxBuilderRow::Item {
                entry,
                child: matches!(value, ComboboxEntry::Group(_)).then_some(child),
                index,
            });
            visible.push(ComboboxVisibleItem {
                value: item.value.clone(),
                label: item.label.clone(),
                disabled: item.disabled,
            });
        }
    }
    ctx.visible.set_silent(visible);
    let rows = Rc::new(rows);
    let count = rows.len();
    let item_rows = Rc::new(item_rows);
    let render = {
        let rows = rows.clone();
        let ctx = ctx.clone();
        let query = if filtering { query } else { String::new() };
        move |row: usize| {
            with_child_owner(|| {
                provide_context(combobox_context(), ctx.clone());
                provide_context(combobox_behavior_context(), behavior.clone());
                match rows[row] {
                    ComboboxBuilderRow::Item {
                        entry,
                        child,
                        index,
                    } => {
                        let item = match &entries[entry] {
                            ComboboxEntry::Item(item) => item,
                            ComboboxEntry::Group(group) => &group.items[child.unwrap()],
                            ComboboxEntry::Separator => unreachable!(),
                        };
                        combobox_indexed_item(
                            &ctx,
                            item.value.clone(),
                            item.label.clone(),
                            item.disabled,
                            &query,
                            index,
                        )
                    }
                    ComboboxBuilderRow::Heading(entry) => {
                        let ComboboxEntry::Group(group) = &entries[entry] else {
                            unreachable!()
                        };
                        ComboboxLabel(ComboboxLabelProps {
                            children: Some(Element::new().text(group.label.clone().unwrap())),
                            base: None,
                        })
                    }
                    ComboboxBuilderRow::Separator => {
                        ComboboxSeparator(ComboboxSeparatorProps::default())
                    }
                }
            })
        }
    };
    if pass.is_none() {
        index.borrow_mut().finish();
    }
    let reveal = use_signal(&format!("{}__combo_virtual_focus", ctx.id), || usize::MAX);
    if count >= 64 {
        let mut list: Element = virtual_list(format!("{}:menu_scroll", ctx.id))
            .item_count(count)
            .item_height(32.0)
            .measure_rows()
            .height(300.0)
            .focused_index_align(ctx.scroll_selected.as_str())
            .focused_index_fn(move || {
                let index = reveal.peek();
                reveal.set_silent(usize::MAX);
                item_rows.get(index).copied()
            })
            .render_item(render)
            .into();
        list.set_attr("jadix_combobox_virtual", "1");
        return list
            .max_h("100%")
            .min_h(0.0)
            .flex_shrink(1.0)
            .accesskit_role("listbox");
    }
    reveal.set_silent(usize::MAX);
    let mut children = (0..count).map(render).collect::<Vec<_>>();
    children.push(ComboboxEmpty(ComboboxEmptyProps {
        children: Some(Element::new().text(crate::tr_default_zh(
            "jadix.combobox.empty",
            "No results.",
            "无匹配结果。",
        ))),
        base: None,
    }));
    Element::from(vstack().w("100%").gap("0").children(children)).accesskit_role("listbox")
}

// ---------------------------------------------------------------------------
// RSX `<Combobox>` root
// ---------------------------------------------------------------------------

crate::__component_contract! {
ComboboxProps [];
/// RSX `<Combobox>`：有复合子树时走 [`combobox_scope`]；否则走 Builder。
#[allow(non_snake_case)]
pub fn Combobox(
identity [] id: String,
plain [] value: Option<Signal<String>>,
default [] default_value: String,
default [] options: Vec<(String, String)>,
default [] placeholder: String,
default [] clearable: bool,
plain [] scroll_selected: Option<String>,
plain [] children: Option<Element>,
) -> Element {
    if let Some(c) = children {
        let ss = scroll_selected.clone().unwrap_or_default();
        return combobox_scope(
            id,
            None,
            value,
            if placeholder.is_empty() {
                crate::tr_default_zh("jadix.combobox.search", "Search…", "搜索…")
            } else {
                placeholder
            },
            clearable,
            String::new(),
            String::new(),
            ss,
            || c,
        );
    }
    let mut b = combobox(&id)
        .default_value(default_value)
        .options(options)
        .placeholder(if placeholder.is_empty() {
            crate::tr_default_zh("jadix.combobox.search", "Search…", "搜索…")
        } else {
            placeholder
        })
        .clearable(clearable);
    if let Some(sig) = value {
        b = b.value_signal(sig);
    }
    if let Some(s) = scroll_selected {
        b = b.scroll_selected(s);
    }
    b.into()
}
}

#[cfg(all(test, feature = "product-tests"))]
#[path = "tests/combobox.rs"]
mod tests;
