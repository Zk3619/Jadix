//! # Pagination
//!
//! 对齐 ShadCN `pagination.tsx`：分页导航复合零件（非 Table 内建分页状态）。
//!
//! ```text
//! Pagination                 // nav · w-full · justify-center
//! └── PaginationContent      // 横排 items-center
//!     └── PaginationItem     // 单项壳
//!         ├── PaginationPrevious / PaginationNext
//!         ├── PaginationLink     // Button default|ghost · size icon|default
//!         └── PaginationEllipsis
//! ```
//!
//! **Identity (WP-2.8)**: presentational shell; user `id` **not required**; empty/omitted → no `template_id` (I-1).
//! 受控页码用 [`Signal`]；Builder 未传则按实例键建内部信号。
//!
//! `ui!` 中 `<Pagination>` / `<PaginationLink>` 等由通用 PascalCase → `*Props` 展开
//!（与 [`Navbar`](crate::components::Navbar) 相同，无需专用 scope）。
//!
//! ## Usage (dual style)
//! ```ignore
//! // A. Composite RSX (recommended)
//! ui! {
//!     <Pagination>
//!         <PaginationContent>
//!             <PaginationItem>
//!                 <PaginationPrevious on_click={move || { /* page - 1 */ }} />
//!             </PaginationItem>
//!             <PaginationItem>
//!                 <PaginationLink is_active={true} on_click={|| {}}>"1"</PaginationLink>
//!             </PaginationItem>
//!             <PaginationItem><PaginationEllipsis /></PaginationItem>
//!             <PaginationItem>
//!                 <PaginationNext on_click={move || { /* page + 1 */ }} />
//!             </PaginationItem>
//!         </PaginationContent>
//!     </Pagination>
//! }
//!
//! // B. Builder（页码窗 + 省略号；page 为 1-based；左/中/右为上限）
//! pagination("docs")
//!     .page_count(20)
//!     .page_signal(page)
//!     .left_count(3)
//!     .sibling_count(2)
//!     .right_count(4)
//!     .jumper(true) // 可选：整数页码输入 + 跳转按钮
//!     .max_items(9) // 可选硬上限；默认还会按容器宽度自动压缩
//! ```
//!
//! ## 样式（ShadCN）
//! - 根：`mx-auto flex w-full justify-center` · `role=navigation`
//! - Content：`flex items-center gap-1`
//! - Link：`isActive` → `default`（primary），否则 `ghost`；默认 `size=icon`
//! - Previous / Next：`size=default` + chevron + 文案（文案为空时退化为 `icon`）
//! - Jumper（可选）：`type=number` 居中输入 + 跳转按钮（输入框 Enter 亦可跳转）

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};

use winit::event::ElementState;
use winit::keyboard::{Key, NamedKey};

use rui_macros::component;

use crate::app::reactive::{reactive_anchor, reactive_placeholder_sized};
use crate::components::base::SlotChrome;
use crate::components::button::button;
use crate::components::input::input;
use crate::core::reactivity::{use_signal, Signal};
use crate::core::tree::{parse_text_color, Element, Handler, LayoutStatus, LayoutTree};
use crate::impl_slot_chrome_methods;
use crate::primitives::r#box::hstack;
use crate::utils::lucide::lucide_glyph;
use crate::utils::take_packed;
use crate::utils::text_node::build_icon_text_node;

/// 页码格（icon 按钮）约 `2.25rem`。
const PAGE_SLOT_PX: f32 = 36.0;
/// Content `gap-1` ≈ `0.25rem`。
const SLOT_GAP_PX: f32 = 4.0;
/// 仅图标的 Prev/Next（`size=icon`）。
const ICON_NAV_PX: f32 = 36.0;
/// Jumper 与页码行之间的额外间距。
const JUMPER_LEAD_GAP_PX: f32 = 8.0;

const ATTR_SLOT: &str = "data-slot";
const SLOT_ROOT: &str = "pagination";
const SLOT_CONTENT: &str = "pagination-content";
const SLOT_ITEM: &str = "pagination-item";
const SLOT_LINK: &str = "pagination-link";
const SLOT_PREV: &str = "pagination-previous";
const SLOT_NEXT: &str = "pagination-next";
const SLOT_ELLIPSIS: &str = "pagination-ellipsis";
const SLOT_JUMPER: &str = "pagination-jumper";
fn lucide_icon(name: &str, size_px: f32) -> Element {
    let glyph = lucide_glyph(name).unwrap_or_else(|| "…".into());
    // 图标勿抢点击：命中应落到外层 Button / interactive。
    build_icon_text_node(glyph)
        .text_size(size_px)
        .text_color(parse_text_color("foreground"))
        .pointer_events("none")
}

fn extract_text_leaf(el: &Element) -> String {
    crate::core::tree::find_in_tree(el, |n| n.text.as_deref().is_some_and(|t| !t.is_empty()))
        .and_then(|n| n.text.clone())
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Page window (Builder)
// ---------------------------------------------------------------------------

/// 页码窗条目：数字或省略号。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaginationPage {
    Number(usize),
    Ellipsis,
}

/// 页码窗分段配置（左 / 中 / 右均为**上限**），中间用 `…` 连接不相邻段。
///
/// 换页时尽量保持「数字 + 省略号」格子总数稳定（靠前/靠后把省下的省略号位补给扩展段），
/// 避免工具条左右跳动。窄屏可用 [`Self::max_items`] 或 Builder 默认的宽度自动压缩再压低有效上限。
///
/// - [`Self::left_count`]：开头最多显示的页数
/// - [`Self::sibling_count`]：当前页两侧各最多保留的页数
/// - [`Self::right_count`]：末尾最多显示的页数
/// - [`Self::max_items`]：格子总数上限（含省略号）；超出时按 sibling → 较大边界 的顺序压缩
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PaginationRangeOpts {
    pub left_count: usize,
    pub sibling_count: usize,
    pub right_count: usize,
    pub max_items: Option<usize>,
}

impl Default for PaginationRangeOpts {
    fn default() -> Self {
        Self {
            left_count: 1,
            sibling_count: 1,
            right_count: 1,
            max_items: None,
        }
    }
}

impl PaginationRangeOpts {
    pub fn new(left_count: usize, sibling_count: usize, right_count: usize) -> Self {
        Self {
            left_count,
            sibling_count,
            right_count,
            max_items: None,
        }
    }

    pub fn max_items(mut self, n: usize) -> Self {
        self.max_items = Some(n.max(1));
        self
    }
}

/// 双省略号形态下的目标格子数：`left + mid + right + 2`。
#[inline]
fn range_item_budget(left: usize, sibling: usize, right: usize) -> usize {
    left.saturating_add(right)
        .saturating_add(sibling.saturating_mul(2).saturating_add(1))
        .saturating_add(2)
}

/// 将左/中/右上限压进 `max_items`（若有），保证预算 ≤ 上限。
///
/// `max_items ≥ 5` 时左右边界至少保留 1，避免出现孤立的 `… 8 …`。
fn fit_range_caps(
    mut left: usize,
    mut sibling: usize,
    mut right: usize,
    max_items: Option<usize>,
) -> (usize, usize, usize) {
    let Some(max_items) = max_items else {
        return (left, sibling, right);
    };
    let max_items = max_items.max(1);
    let min_boundary = if max_items >= 5 { 1 } else { 0 };
    while range_item_budget(left, sibling, right) > max_items {
        if sibling > 0 {
            sibling -= 1;
        } else if left > right && left > min_boundary {
            left -= 1;
        } else if right > min_boundary {
            right -= 1;
        } else if left > min_boundary {
            left -= 1;
        } else {
            break;
        }
    }
    (left, sibling, right)
}

/// 估算 Prev/Next 按钮宽度（含 chevron + 内边距 + 文案）。
fn estimate_nav_btn_px(label: &str) -> f32 {
    let label = label.trim();
    if label.is_empty() {
        return ICON_NAV_PX;
    }
    let text_w: f32 = label
        .chars()
        .map(|c| if c.is_ascii() { 8.0 } else { 14.0 })
        .sum();
    // icon 14 + gap ~8 + px≈32 + text
    (14.0 + 8.0 + 32.0 + text_w).max(ICON_NAV_PX)
}

fn estimate_text_px(label: &str) -> f32 {
    let label = label.trim();
    if label.is_empty() {
        return 0.0;
    }
    label
        .chars()
        .map(|c| if c.is_ascii() { 8.0 } else { 14.0 })
        .sum()
}

/// 无图标的 outline 按钮（跳转）宽度估算。
fn estimate_text_btn_px(label: &str) -> f32 {
    let tw = estimate_text_px(label);
    if tw <= 0.0 {
        return 0.0;
    }
    (tw + 32.0).max(ICON_NAV_PX)
}

fn jumper_input_width_px(page_count: usize) -> f32 {
    let digits = page_count.max(1).to_string().len().max(2) as f32;
    (24.0 + digits * 10.0).max(48.0)
}

/// 跳转区（输入 + 按钮）占用宽度，含与页码行的间距。
pub fn estimate_jumper_chrome_px(button_text: &str, page_count: usize) -> f32 {
    let input = jumper_input_width_px(page_count);
    let btn = estimate_text_btn_px(button_text);
    JUMPER_LEAD_GAP_PX + input + SLOT_GAP_PX + 2.0 + btn
}

/// 给定可用宽度，估算页码格上限，以及是否应收起 Prev/Next 文案。
///
/// `extra_chrome_px`：跳转区等额外占用（见 [`estimate_jumper_chrome_px`]）。
/// 返回 `(max_page_slots, compact_nav)`；`width_px ≤ 1` 时返回 `(0, false)` 表示尚未测量。
pub fn fit_pagination_for_width(
    width_px: f32,
    previous_text: &str,
    next_text: &str,
) -> (usize, bool) {
    fit_pagination_for_width_ex(width_px, previous_text, next_text, 0.0)
}

/// 同 [`fit_pagination_for_width`]，可扣除跳转区等额外 chrome。
pub fn fit_pagination_for_width_ex(
    width_px: f32,
    previous_text: &str,
    next_text: &str,
    extra_chrome_px: f32,
) -> (usize, bool) {
    if width_px <= 1.0 {
        return (0, false);
    }
    let avail = (width_px - extra_chrome_px.max(0.0)).max(0.0);
    let bar_w = |slots: usize, compact: bool| -> f32 {
        let (nav_prev, nav_next) = if compact {
            (ICON_NAV_PX, ICON_NAV_PX)
        } else {
            (
                estimate_nav_btn_px(previous_text),
                estimate_nav_btn_px(next_text),
            )
        };
        let pages = if slots == 0 {
            0.0
        } else {
            slots as f32 * PAGE_SLOT_PX + (slots.saturating_sub(1) as f32) * SLOT_GAP_PX
        };
        nav_prev + nav_next + SLOT_GAP_PX * 2.0 + pages
    };

    let slots_for = |compact: bool| -> usize {
        let nav_prev = if compact {
            ICON_NAV_PX
        } else {
            estimate_nav_btn_px(previous_text)
        };
        let nav_next = if compact {
            ICON_NAV_PX
        } else {
            estimate_nav_btn_px(next_text)
        };
        let chrome = nav_prev + nav_next + SLOT_GAP_PX * 2.0;
        let rest = (avail - chrome).max(0.0);
        let n = ((rest + SLOT_GAP_PX) / (PAGE_SLOT_PX + SLOT_GAP_PX)).floor() as i32;
        n.max(1) as usize
    };

    let mut compact = false;
    let mut slots = slots_for(false);
    // 至少能放下「1 … cur … n」五格时保留文案；否则改 icon-only 腾出宽度。
    if slots < 5 {
        compact = true;
        slots = slots_for(true);
    }
    // 估算误差（字宽/内边距）时再收一格，直到估算宽度落入容器。
    while slots > 1 && bar_w(slots, compact) > avail + 0.5 {
        slots -= 1;
        if !compact && bar_w(slots, true) <= avail + 0.5 {
            compact = true;
            slots = slots_for(true).min(slots);
        }
    }
    (slots.max(1), compact)
}

/// 解析跳转输入：仅接受正整数，并钳到 `[1, page_count]`。
pub fn parse_jumper_page(raw: &str, page_count: usize) -> Option<usize> {
    let s = raw.trim();
    if s.is_empty() {
        return None;
    }
    // 拒绝小数 / 符号等：整段必须是十进制数字。
    if !s.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let n: usize = s.parse().ok()?;
    if n == 0 {
        return None;
    }
    Some(n.clamp(1, page_count.max(1)))
}

/// `max_items ≤ 4` 时的极窄窗（避免 `fit_range_caps` 压到 left=right=0）。
fn compact_pagination_range(
    page: usize,
    page_count: usize,
    max_items: usize,
) -> Vec<PaginationPage> {
    let page = page.clamp(1, page_count);
    match max_items.max(1) {
        1 => vec![PaginationPage::Number(page)],
        2 => {
            if page_count == 1 {
                vec![PaginationPage::Number(1)]
            } else if page == 1 {
                vec![
                    PaginationPage::Number(1),
                    PaginationPage::Number(page_count),
                ]
            } else if page == page_count {
                vec![
                    PaginationPage::Number(1),
                    PaginationPage::Number(page_count),
                ]
            } else {
                vec![
                    PaginationPage::Number(page),
                    PaginationPage::Number(page_count),
                ]
            }
        }
        3 => {
            if page_count <= 3 {
                return (1..=page_count).map(PaginationPage::Number).collect();
            }
            densify_page_window(vec![
                PaginationPage::Number(1),
                PaginationPage::Ellipsis,
                PaginationPage::Number(page_count),
            ])
        }
        _ => {
            // 4：尽量露出当前页
            if page_count <= 4 {
                return (1..=page_count).map(PaginationPage::Number).collect();
            }
            if page <= 2 {
                densify_page_window(vec![
                    PaginationPage::Number(1),
                    PaginationPage::Number(2),
                    PaginationPage::Ellipsis,
                    PaginationPage::Number(page_count),
                ])
            } else if page >= page_count.saturating_sub(1) {
                densify_page_window(vec![
                    PaginationPage::Number(1),
                    PaginationPage::Ellipsis,
                    PaginationPage::Number(page_count.saturating_sub(1).max(2)),
                    PaginationPage::Number(page_count),
                ])
            } else {
                densify_page_window(vec![
                    PaginationPage::Number(1),
                    PaginationPage::Ellipsis,
                    PaginationPage::Number(page),
                    PaginationPage::Number(page_count),
                ])
            }
        }
    }
}

fn push_numbers(out: &mut Vec<PaginationPage>, from: usize, to: usize) {
    if from > to {
        return;
    }
    for n in from..=to {
        out.push(PaginationPage::Number(n));
    }
}

/// 生成带省略号的页码窗（`page` / 返回值均为 **1-based**）。
///
/// 左/中/右为上限；在页数足够时，靠前 / 居中 / 靠后三种形态的格子数尽量一致：
/// - 靠前：`1 … k … n` 扩展为 `1..expanded … right`
/// - 居中：`left … mid … right`
/// - 靠后：对称扩展右段
///
/// 示例（`left≤3, sibling≤2, right≤4`，无 `max_items`）：
/// - 靠前：`1 2 3 4 5 6 7 8 9 … n-3 n-2 n-1 n`（扩展填满预算）
/// - 居中：`1 2 3 … 6 7 8 9 10 … n-3 n-2 n-1 n`
pub fn pagination_range(
    page: usize,
    page_count: usize,
    opts: PaginationRangeOpts,
) -> Vec<PaginationPage> {
    let page_count = page_count.max(1);
    let page = page.clamp(1, page_count);

    if let Some(max_items) = opts.max_items {
        if max_items <= 4 && page_count > max_items {
            return compact_pagination_range(page, page_count, max_items);
        }
    }

    let (left_n, sibling, right_n) = fit_range_caps(
        opts.left_count,
        opts.sibling_count,
        opts.right_count,
        opts.max_items,
    );

    let mid_span = sibling.saturating_mul(2).saturating_add(1);
    let number_budget = left_n.saturating_add(mid_span).saturating_add(right_n);

    if page_count <= number_budget.max(1) {
        return (1..=page_count).map(PaginationPage::Number).collect();
    }

    // 靠前/靠后把「少掉的那个省略号」补给扩展数字段，使总格子数 ≈ number_budget + 2。
    let left_expanded = left_n.saturating_add(mid_span).saturating_add(1);
    let right_expanded = right_n.saturating_add(mid_span).saturating_add(1);

    let ideal_sib_start = page.saturating_sub(sibling).max(1);
    let ideal_sib_end = (page + sibling).min(page_count);
    let end_start = page_count.saturating_sub(right_n.saturating_sub(1));

    let show_left_ellipsis = ideal_sib_start > left_n.saturating_add(1);
    let show_right_ellipsis = ideal_sib_end + 1 < end_start;

    let mut out = Vec::with_capacity(number_budget.saturating_add(2));

    match (show_left_ellipsis, show_right_ellipsis) {
        (false, true) => {
            // 靠前：扩展左段，保留右上限与一个省略号。
            let left_to = left_expanded.min(end_start.saturating_sub(2)).max(1);
            push_numbers(&mut out, 1, left_to);
            if left_to + 1 < end_start {
                out.push(PaginationPage::Ellipsis);
            } else if left_to + 1 == end_start.saturating_sub(1) {
                push_numbers(&mut out, left_to + 1, left_to + 1);
            }
            push_numbers(&mut out, end_start, page_count);
        }
        (true, false) => {
            // 靠后：扩展右段。
            let right_from = page_count
                .saturating_sub(right_expanded.saturating_sub(1))
                .max(left_n.saturating_add(2));
            push_numbers(&mut out, 1, left_n);
            if left_n + 1 < right_from {
                out.push(PaginationPage::Ellipsis);
            } else if left_n + 1 == right_from.saturating_sub(1) {
                push_numbers(&mut out, left_n + 1, left_n + 1);
            }
            push_numbers(&mut out, right_from, page_count);
        }
        (true, true) => {
            push_numbers(&mut out, 1, left_n);
            out.push(PaginationPage::Ellipsis);
            push_numbers(&mut out, ideal_sib_start, ideal_sib_end);
            out.push(PaginationPage::Ellipsis);
            push_numbers(&mut out, end_start, page_count);
        }
        (false, false) => {
            return (1..=page_count).map(PaginationPage::Number).collect();
        }
    }

    densify_page_window(out)
}

/// 单页间隙补数字；去掉重复页码。
fn densify_page_window(items: Vec<PaginationPage>) -> Vec<PaginationPage> {
    let mut densified = Vec::with_capacity(items.len().saturating_add(2));
    for item in items {
        match (densified.last().copied(), item) {
            (Some(PaginationPage::Number(a)), PaginationPage::Number(b)) if b > a + 1 => {
                if b == a + 2 {
                    densified.push(PaginationPage::Number(a + 1));
                } else {
                    densified.push(PaginationPage::Ellipsis);
                }
                densified.push(item);
            }
            (Some(PaginationPage::Number(a)), PaginationPage::Number(b)) if b == a => {}
            (Some(PaginationPage::Ellipsis), PaginationPage::Ellipsis) => {}
            _ => densified.push(item),
        }
    }
    densified
}

/// 兼容旧调用：仅配置两侧 sibling，边界各最多 1 页。
pub fn pagination_range_simple(
    page: usize,
    page_count: usize,
    sibling_count: usize,
) -> Vec<PaginationPage> {
    pagination_range(
        page,
        page_count,
        PaginationRangeOpts {
            left_count: 1,
            sibling_count,
            right_count: 1,
            max_items: None,
        },
    )
}

// ---------------------------------------------------------------------------
// RSX parts
// ---------------------------------------------------------------------------

/// 根：`nav` · 全宽居中。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn Pagination(id: String, children: Option<Element>) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    let mut row = hstack()
        .w("100%")
        .min_w(0.0)
        .overflow_x("hidden")
        .overflow_y("visible")
        .justify("center")
        .items_center()
        .gap("0");
    for k in kids {
        row = row.child(k);
    }
    let mut el: Element = row.into();
    el.set_attr(ATTR_SLOT, SLOT_ROOT);
    el = el.accesskit_role("navigation");
    if !id.trim().is_empty() {
        el.template_id = Some(id);
    }
    el
}

/// 内容行：`flex items-center gap-1`。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn PaginationContent(children: Option<Element>) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    let mut row = hstack().gap("0.25rem").items_center().w("auto").min_w(0.0);
    for k in kids {
        row = row.child(k);
    }
    let mut el: Element = row.into();
    el.set_attr(ATTR_SLOT, SLOT_CONTENT);
    el
}

/// 单项壳（对齐 `<li>`）。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn PaginationItem(children: Option<Element>) -> Element {
    let kids = children.map(take_packed).unwrap_or_default();
    let mut row = hstack().gap("0").items_center().w("auto").flex_shrink(0.0);
    for k in kids {
        row = row.child(k);
    }
    let mut el: Element = row.into();
    el.set_attr(ATTR_SLOT, SLOT_ITEM);
    el
}

/// 页码 / 导航链接：`is_active` → default（primary），否则 ghost；默认 `size=icon`。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn PaginationLink(
    id: String,
    is_active: bool,
    size: String,
    disabled: bool,
    #[prop(default = "Handler::default()")] mut on_click: Handler<()>,
    children: Option<Element>,
) -> Element {
    let size = if size.trim().is_empty() {
        "icon"
    } else {
        size.trim()
    };
    let variant = if is_active { "default" } else { "ghost" };
    let label = children
        .as_ref()
        .map(extract_text_leaf)
        .filter(|s| !s.is_empty())
        .unwrap_or_default();

    let mut btn = button(if id.trim().is_empty() { "" } else { &id })
        .variant(variant)
        .size(size)
        .disabled(disabled);
    if let Some(c) = children {
        let kids = take_packed(c);
        if kids.len() == 1 && kids[0].text.as_ref().is_some_and(|t| !t.is_empty()) {
            btn = btn.text(label);
        } else {
            for k in kids {
                btn = btn.child(k);
            }
        }
    } else if !label.is_empty() {
        btn = btn.text(label);
    }
    btn = btn.on_click(move || on_click.call(()));
    let mut el: Element = btn.into();
    el.set_attr(ATTR_SLOT, SLOT_LINK);
    if is_active {
        el.set_attr("data-active", "true");
        el.set_attr("aria-current", "page");
    }
    el
}

/// 上一页：chevron-left + 文案（默认 `"Previous"`）。
///
/// `hide_text`: icon only (Builder enables on narrow/compressed layouts).
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn PaginationPrevious(
    id: String,
    text: String,
    hide_text: bool,
    disabled: bool,
    #[prop(default = "Handler::default()")] mut on_click: Handler<()>,
    children: Option<Element>,
) -> Element {
    let label = if text.trim().is_empty() {
        "Previous".to_string()
    } else {
        text
    };
    let show_text = !hide_text;
    let size = if show_text { "default" } else { "icon" };
    let btn_id = if id.trim().is_empty() {
        ""
    } else {
        id.as_str()
    };
    let mut btn = button(btn_id)
        .variant("ghost")
        .size(size)
        .disabled(disabled)
        .child(lucide_icon("chevron-left", 14.0));
    if let Some(c) = children {
        for k in take_packed(c) {
            btn = btn.child(k);
        }
    } else if show_text {
        // Prefer `.text` so Button applies variant chrome; keep icon as non-hitting child.
        btn = btn.text(label);
    }
    let mut el: Element = btn.into();
    // Attach after `Into` so the interactive root always owns the handler (even if
    // Builder `on_click` was cleared by a future From change).
    if !disabled {
        let mut old = el.on_click.clone();
        el = el.on_click(move || {
            if let Some(ref mut rc) = old {
                rc.borrow_mut()();
            }
            on_click.call(());
        });
    }
    el.set_attr(ATTR_SLOT, SLOT_PREV);
    el.set_attr("aria-label", "Go to previous page");
    el
}

/// 下一页：文案（默认 `"Next"`）+ chevron-right。
///
/// `hide_text`: icon only (Builder enables on narrow/compressed layouts).
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn PaginationNext(
    id: String,
    text: String,
    hide_text: bool,
    disabled: bool,
    #[prop(default = "Handler::default()")] mut on_click: Handler<()>,
    children: Option<Element>,
) -> Element {
    let label = if text.trim().is_empty() {
        "Next".to_string()
    } else {
        text
    };
    let show_text = !hide_text;
    let size = if show_text { "default" } else { "icon" };
    let btn_id = if id.trim().is_empty() {
        ""
    } else {
        id.as_str()
    };
    let mut btn = button(btn_id)
        .variant("ghost")
        .size(size)
        .disabled(disabled);
    if let Some(c) = children {
        for k in take_packed(c) {
            btn = btn.child(k);
        }
    } else if show_text {
        btn = btn.text(label);
    }
    btn = btn.child(lucide_icon("chevron-right", 14.0));
    let mut el: Element = btn.into();
    if !disabled {
        let mut old = el.on_click.clone();
        el = el.on_click(move || {
            if let Some(ref mut rc) = old {
                rc.borrow_mut()();
            }
            on_click.call(());
        });
    }
    el.set_attr(ATTR_SLOT, SLOT_NEXT);
    el.set_attr("aria-label", "Go to next page");
    el
}

/// 省略号：`more-horizontal`。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn PaginationEllipsis() -> Element {
    let mut el: Element = hstack()
        .h("2.25rem")
        .w("2.25rem")
        .items_center()
        .justify("center")
        .flex_shrink(0.0)
        .child(lucide_icon("more-horizontal", 14.0))
        .into();
    el.set_attr(ATTR_SLOT, SLOT_ELLIPSIS);
    el.set_attr("aria-hidden", "true");
    el
}

static NEXT_PAGINATION_ID: AtomicUsize = AtomicUsize::new(1);

/// 跳转输入：整数页码框 + 跳转按钮。
///
/// 用于复合 RSX；Builder 用 [`.jumper(true)`](PaginationBuilder::jumper)。
/// 成功跳转后更新 `page` 信号，并触发 `on_go`（可用 `page.peek()` 读新页）。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn PaginationJumper(
    id: String,
    /// 总页数（1-based 最大页）。
    page_count: Option<i32>,
    /// 当前页信号；省略则按 `id` 建内部信号。
    page: Option<Signal<usize>>,
    button_text: String,
    #[prop(default = "Handler::default()")] mut on_go: Handler<()>,
) -> Element {
    let page_count = page_count.unwrap_or(1).max(1) as usize;
    let instance = if id.trim().is_empty() {
        format!(
            "pagination-jumper-{}",
            NEXT_PAGINATION_ID.fetch_add(1, Ordering::Relaxed)
        )
    } else {
        id.clone()
    };
    let page_sig =
        page.unwrap_or_else(|| use_signal(format!("{instance}__page").as_str(), || 1usize));
    let button_text = if button_text.trim().is_empty() {
        "Go".to_string()
    } else {
        button_text
    };
    let on_go = Rc::new(RefCell::new(on_go));
    paint_jumper(
        &instance,
        page_sig.peek().clamp(1, page_count),
        page_count,
        &button_text,
        page_sig,
        Some(Rc::new(move |_n| {
            on_go.borrow_mut().call(());
        })),
    )
}

/// 构建跳转控件（number 输入 + 按钮），包在 [`PaginationItem`] 内。
fn paint_jumper(
    id_prefix: &str,
    page: usize,
    page_count: usize,
    button_text: &str,
    page_sig: Signal<usize>,
    on_page_change: Option<Rc<dyn Fn(usize)>>,
) -> Element {
    let page = page.clamp(1, page_count.max(1));
    let draft = use_signal(format!("{id_prefix}__jump_draft").as_str(), || {
        page.to_string()
    });
    let synced = use_signal(format!("{id_prefix}__jump_synced").as_str(), || 0usize);
    if synced.peek() != page {
        synced.set_silent(page);
        draft.set_silent(page.to_string());
    }

    let digits = page_count.max(1).to_string().len().max(2);
    // 约 2～4 位数字宽度
    let input_w = format!("{}rem", (1.75 + digits as f32 * 0.55).clamp(2.5, 4.5));

    let go_draft = draft;
    let go_page = page_sig;
    let go_count = page_count;
    let go_notify = on_page_change;
    let do_jump = Rc::new(RefCell::new(move || {
        let raw = go_draft.peek();
        let Some(n) = parse_jumper_page(&raw, go_count) else {
            // 非法输入：回填当前页
            go_draft.set(go_page.peek().clamp(1, go_count.max(1)).to_string());
            return;
        };
        go_draft.set(n.to_string());
        go_page.set(n);
        if let Some(ref cb) = go_notify {
            cb(n);
        }
    }));

    let mut row = hstack()
        .gap("0.375rem")
        .items_center()
        .flex_shrink(0.0)
        .overflow("visible")
        .ml(format!("{JUMPER_LEAD_GAP_PX}px"));

    let jump_on_enter = Rc::clone(&do_jump);
    let mut input_el: Element = input(&format!("{id_prefix}__jump_input"))
        .input_type("number")
        .number_steppers(false)
        .text_align("center")
        .px("0.25rem")
        .w(input_w.as_str())
        .h("2.25rem")
        .value_signal(draft)
        .into();
    input_el = input_el.on_key_down(move |ev| {
        if ev.state != ElementState::Pressed || ev.repeat {
            return false;
        }
        if matches!(ev.logical_key, Key::Named(NamedKey::Enter)) {
            jump_on_enter.borrow_mut()();
            return true;
        }
        false
    });
    row = row.child(input_el);

    let jump_on_click = Rc::clone(&do_jump);
    let jump_btn: Element = button(&format!("{id_prefix}__jump_btn"))
        .variant("outline")
        .size("default")
        .text(button_text.to_string())
        .on_click(move || jump_on_click.borrow_mut()())
        .into();
    row = row.child(jump_btn);

    let mut shell: Element = row.into();
    shell.set_attr(ATTR_SLOT, SLOT_JUMPER);
    shell.set_attr("aria-label", "Pagination jumper");

    // Item 勿裁切输入焦点环；高度随内容 + 焦点阴影。
    let mut item = PaginationItem(PaginationItemProps {
        children: Some(shell),
        base: None,
    });
    item = item.overflow("visible").py("0.125rem");
    item
}

// ---------------------------------------------------------------------------
// Builder
// ---------------------------------------------------------------------------

pub struct PaginationBuilder {
    id: String,
    class: Option<String>,
    root_style: Option<crate::components::base::SlotStyleFn>,
    page_count: usize,
    page: Option<Signal<usize>>,
    default_page: usize,
    left_count: usize,
    sibling_count: usize,
    right_count: usize,
    max_items: Option<usize>,
    /// 按容器宽度自动压缩页码格（默认开启）。
    auto_fit: bool,
    /// 显示跳转输入 + 按钮（默认关闭）。
    jumper: bool,
    jumper_button_text: String,
    previous_text: String,
    next_text: String,
    on_page_change: Option<Rc<dyn Fn(usize)>>,
    parts: PaginationPartChrome,
    custom_body: Option<Box<dyn FnOnce() -> Element>>,
}

#[derive(Clone, Default)]
struct PaginationPartChrome {
    content: SlotChrome,
    item: SlotChrome,
    previous: SlotChrome,
    next: SlotChrome,
    ellipsis: SlotChrome,
    link: SlotChrome,
}

pub struct PaginationSlotBuilder {
    chrome: SlotChrome,
}

fn pagination_slot() -> PaginationSlotBuilder {
    PaginationSlotBuilder {
        chrome: SlotChrome::default(),
    }
}

impl PaginationSlotBuilder {
    impl_slot_chrome_methods!();
}

/// 便捷入口：按 `page_count` 生成 Prev / 页码窗 / Next（复合零件组装）。
///
/// 展示型默认无用户 id；需要稳定受控槽或热重载时再 `.id(...)`。
/// 默认按容器宽度自动压缩页码格；可用 `.auto_fit(false)` 关闭。
/// 可选 `.jumper(true)` 启用整数页码输入与跳转按钮。
pub fn pagination(id: impl Into<String>) -> PaginationBuilder {
    PaginationBuilder {
        id: id.into(),
        class: None,
        root_style: None,
        page_count: 1,
        page: None,
        default_page: 1,
        left_count: 1,
        sibling_count: 1,
        right_count: 1,
        max_items: None,
        auto_fit: true,
        jumper: false,
        jumper_button_text: "Go".into(),
        previous_text: "Previous".into(),
        next_text: "Next".into(),
        on_page_change: None,
        parts: PaginationPartChrome::default(),
        custom_body: None,
    }
}

impl PaginationBuilder {
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }
    pub fn class(mut self, class: impl Into<String>) -> Self {
        self.class = Some(class.into());
        self
    }
    pub fn style(
        mut self,
        f: impl Fn(
                crate::components::base::BaseComponentBuilder,
            ) -> crate::components::base::BaseComponentBuilder
            + 'static,
    ) -> Self {
        self.root_style = Some(std::rc::Rc::new(f));
        self
    }
    pub fn child(mut self, body: impl FnOnce() -> Element + 'static) -> Self {
        self.custom_body = Some(Box::new(body));
        self
    }
    pub fn previous_with(
        mut self,
        f: impl FnOnce(PaginationSlotBuilder) -> PaginationSlotBuilder,
    ) -> Self {
        self.parts.previous = f(pagination_slot()).chrome;
        self
    }
    pub fn next_with(
        mut self,
        f: impl FnOnce(PaginationSlotBuilder) -> PaginationSlotBuilder,
    ) -> Self {
        self.parts.next = f(pagination_slot()).chrome;
        self
    }
    pub fn ellipsis_with(
        mut self,
        f: impl FnOnce(PaginationSlotBuilder) -> PaginationSlotBuilder,
    ) -> Self {
        self.parts.ellipsis = f(pagination_slot()).chrome;
        self
    }
    pub fn link_with(
        mut self,
        f: impl FnOnce(PaginationSlotBuilder) -> PaginationSlotBuilder,
    ) -> Self {
        self.parts.link = f(pagination_slot()).chrome;
        self
    }
    pub fn content_with(
        mut self,
        f: impl FnOnce(PaginationSlotBuilder) -> PaginationSlotBuilder,
    ) -> Self {
        self.parts.content = f(pagination_slot()).chrome;
        self
    }
    pub fn item_with(
        mut self,
        f: impl FnOnce(PaginationSlotBuilder) -> PaginationSlotBuilder,
    ) -> Self {
        self.parts.item = f(pagination_slot()).chrome;
        self
    }
    pub fn page_count(mut self, n: usize) -> Self {
        self.page_count = n.max(1);
        self
    }
    /// 当前页（**1-based**）信号。
    pub fn page_signal(mut self, s: Signal<usize>) -> Self {
        self.page = Some(s);
        self
    }
    pub fn default_page(mut self, page: usize) -> Self {
        self.default_page = page.max(1);
        self
    }
    /// 开头最多显示的页数（上限，默认 1）。
    pub fn left_count(mut self, n: usize) -> Self {
        self.left_count = n;
        self
    }
    /// 当前页两侧各最多保留的页数（上限，默认 1）。
    pub fn sibling_count(mut self, n: usize) -> Self {
        self.sibling_count = n;
        self
    }
    /// 末尾最多显示的页数（上限，默认 1）。
    pub fn right_count(mut self, n: usize) -> Self {
        self.right_count = n;
        self
    }
    /// 一次设置左 / 中(sibling) / 右段**上限**页数。
    pub fn range_counts(mut self, left: usize, sibling: usize, right: usize) -> Self {
        self.left_count = left;
        self.sibling_count = sibling;
        self.right_count = right;
        self
    }
    /// 页码格（数字+省略号）总数硬上限；与自动宽度压缩取更严者。
    pub fn max_items(mut self, n: usize) -> Self {
        self.max_items = Some(n.max(1));
        self
    }
    /// 是否按容器宽度自动压缩（默认 `true`）。
    pub fn auto_fit(mut self, yes: bool) -> Self {
        self.auto_fit = yes;
        self
    }
    /// 是否显示整数页码输入 + 跳转按钮（默认 `false`）。
    pub fn jumper(mut self, yes: bool) -> Self {
        self.jumper = yes;
        self
    }
    /// 跳转按钮文案（默认 `"Go"`）。
    pub fn jumper_button_text(mut self, text: impl Into<String>) -> Self {
        self.jumper_button_text = text.into();
        self
    }
    pub fn previous_text(mut self, text: impl Into<String>) -> Self {
        self.previous_text = text.into();
        self
    }
    pub fn next_text(mut self, text: impl Into<String>) -> Self {
        self.next_text = text.into();
        self
    }
    pub fn on_page_change(mut self, f: impl Fn(usize) + 'static) -> Self {
        self.on_page_change = Some(Rc::new(f));
        self
    }
}

#[derive(Clone)]
struct JumperPaint {
    button_text: String,
}

fn paint_builder_content(
    id_prefix: &str,
    page: usize,
    page_count: usize,
    range: PaginationRangeOpts,
    previous_text: &str,
    next_text: &str,
    hide_nav_text: bool,
    jumper: Option<&JumperPaint>,
    page_sig: Signal<usize>,
    on_page_change: Option<Rc<dyn Fn(usize)>>,
    parts: &PaginationPartChrome,
) -> Element {
    let page = page.clamp(1, page_count.max(1));
    let notify: Rc<dyn Fn(usize)> = Rc::new(move |n: usize| {
        page_sig.set(n);
        if let Some(ref cb) = on_page_change {
            cb(n);
        }
    });

    let mut items = Vec::new();

    let prev_disabled = page <= 1;
    let prev_notify = Rc::clone(&notify);
    let prev_sig = page_sig;
    let prev_el = parts
        .previous
        .apply(PaginationPrevious(PaginationPreviousProps {
            id: format!("{id_prefix}__prev"),
            text: Some(previous_text.to_string()),
            hide_text: Some(hide_nav_text),
            disabled: Some(prev_disabled),
            on_click: Some(Handler::new(move |_| {
                let cur = prev_sig.peek();
                if cur > 1 {
                    prev_notify(cur - 1);
                }
            })),
            children: None,
            base: None,
        }));
    items.push(parts.item.apply(PaginationItem(PaginationItemProps {
        children: Some(prev_el),
        base: None,
    })));

    for entry in pagination_range(page, page_count, range) {
        match entry {
            PaginationPage::Ellipsis => {
                items.push(parts.item.apply(PaginationItem(PaginationItemProps {
                    children: Some(parts.ellipsis.apply(PaginationEllipsis(
                        PaginationEllipsisProps {
                            children: None,
                            base: None,
                        },
                    ))),
                    base: None,
                })));
            }
            PaginationPage::Number(n) => {
                let go = Rc::clone(&notify);
                items.push(parts.item.apply(PaginationItem(PaginationItemProps {
                    children: Some(parts.link.apply(PaginationLink(PaginationLinkProps {
                        id: format!("{id_prefix}__p{n}"),
                        is_active: Some(n == page),
                        size: Some("icon".into()),
                        disabled: Some(false),
                        on_click: Some(Handler::new(move |_| go(n))),
                        children: Some(Element::new().text(n.to_string())),
                        base: None,
                    }))),
                    base: None,
                })));
            }
        }
    }

    let next_disabled = page >= page_count;
    let next_notify = Rc::clone(&notify);
    let next_sig = page_sig;
    let next_count = page_count;
    let next_el = parts.next.apply(PaginationNext(PaginationNextProps {
        id: format!("{id_prefix}__next"),
        text: Some(next_text.to_string()),
        hide_text: Some(hide_nav_text),
        disabled: Some(next_disabled),
        on_click: Some(Handler::new(move |_| {
            let cur = next_sig.peek();
            if cur < next_count {
                next_notify(cur + 1);
            }
        })),
        children: None,
        base: None,
    }));
    items.push(parts.item.apply(PaginationItem(PaginationItemProps {
        children: Some(next_el),
        base: None,
    })));

    if let Some(j) = jumper {
        items.push(paint_jumper(
            id_prefix,
            page,
            page_count,
            &j.button_text,
            page_sig,
            Some(Rc::clone(&notify)),
        ));
    }

    let mut frag = Element::new();
    frag.set_attr("rui_fragment", "1");
    parts
        .content
        .apply(PaginationContent(PaginationContentProps {
            children: Some(frag.children(items)),
            base: None,
        }))
}

impl From<PaginationBuilder> for Element {
    fn from(builder: PaginationBuilder) -> Self {
        let PaginationBuilder {
            id,
            class,
            root_style,
            page_count,
            page,
            default_page,
            left_count,
            sibling_count,
            right_count,
            max_items,
            auto_fit,
            jumper,
            jumper_button_text,
            previous_text,
            next_text,
            on_page_change,
            parts,
            custom_body,
        } = builder;
        if let Some(body) = custom_body {
            return SlotChrome::from_opts(class, root_style).apply(Pagination(PaginationProps {
                id,
                children: Some(body()),
                base: None,
            }));
        }
        let instance = if id.trim().is_empty() {
            format!(
                "pagination-{}",
                NEXT_PAGINATION_ID.fetch_add(1, Ordering::Relaxed)
            )
        } else {
            id.clone()
        };
        let page_sig = page.unwrap_or_else(|| {
            use_signal(format!("{instance}__page").as_str(), move || {
                default_page.clamp(1, page_count.max(1))
            })
        });
        // 0 = 尚未测到宽度；>0 = 可容纳的页码格上限。
        // 宽度压缩在 reactive_sync 里原地改子树（set_silent），避免在 AppState
        // 借用期间 set() → flush effect → reconcile BORROW_FAIL 导致永远不压缩。
        let fit_max = use_signal(format!("{instance}__fit_max").as_str(), || 0usize);
        let fit_compact = use_signal(format!("{instance}__fit_compact").as_str(), || false);
        let view_id = format!("{instance}__pagination_view");
        let prev_text = previous_text.clone();
        let next_text = next_text.clone();
        let prev_fit = previous_text.clone();
        let next_fit = next_text.clone();
        let class_k = class.clone();
        let id_k = id.clone();
        let instance_k = instance.clone();
        let instance_sync = instance.clone();
        let explicit_max = max_items;
        let on_change_sync = on_page_change.clone();
        let jumper_paint = jumper.then(|| JumperPaint {
            button_text: jumper_button_text.clone(),
        });
        let jumper_chrome = if jumper {
            estimate_jumper_chrome_px(&jumper_button_text, page_count)
        } else {
            0.0
        };
        let parts_k = parts.clone();
        let parts_sync = parts.clone();
        let root_style_k = root_style.clone();

        reactive_anchor(view_id.clone(), move || {
            let page = page_sig.get();
            // peek：宽度压缩由 reactive_sync 负责，勿在此订阅以免与 sync 期 set 打架。
            let fitted = if auto_fit { fit_max.peek() } else { 0 };
            let hide_nav = auto_fit && fit_compact.peek();
            let effective_max = match (explicit_max, if fitted > 0 { Some(fitted) } else { None }) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (Some(a), None) => Some(a),
                (None, Some(b)) => Some(b),
                (None, None) => None,
            };
            let range = PaginationRangeOpts {
                left_count,
                sibling_count,
                right_count,
                max_items: effective_max,
            };
            let content = paint_builder_content(
                &instance_k,
                page,
                page_count,
                range,
                &prev_text,
                &next_text,
                hide_nav,
                jumper_paint.as_ref(),
                page_sig,
                on_page_change.clone(),
                &parts_k,
            );
            let mut root = SlotChrome::from_opts(class_k.clone(), root_style_k.clone()).apply(
                Pagination(PaginationProps {
                    id: id_k.clone(),
                    children: Some(content),
                    base: None,
                }),
            );
            if auto_fit {
                let fit_max_s = fit_max;
                let fit_compact_s = fit_compact;
                let prev_m = prev_fit.clone();
                let next_m = next_fit.clone();
                let page_s = page_sig;
                let on_chg = on_change_sync.clone();
                let id_prefix = instance_sync.clone();
                let jumper_p = jumper_paint.clone();
                let jumper_c = jumper_chrome;
                let parts_fit = parts_sync.clone();
                root = root.reactive_sync(move |el, _lt: &mut LayoutTree| {
                    let width = el.layout_bounds[2];
                    if width <= 1.0 {
                        return LayoutStatus::Clean;
                    }
                    let (slots, compact) =
                        fit_pagination_for_width_ex(width, &prev_m, &next_m, jumper_c);
                    if slots == 0 {
                        return LayoutStatus::Clean;
                    }
                    let prev_slots = fit_max_s.peek();
                    let prev_compact = fit_compact_s.peek();
                    if slots == prev_slots && compact == prev_compact {
                        return LayoutStatus::Clean;
                    }
                    fit_max_s.set_silent(slots);
                    fit_compact_s.set_silent(compact);

                    let effective_max = match (explicit_max, Some(slots)) {
                        (Some(a), Some(b)) => Some(a.min(b)),
                        (Some(a), None) => Some(a),
                        (None, Some(b)) => Some(b),
                        (None, None) => None,
                    };
                    let range = PaginationRangeOpts {
                        left_count,
                        sibling_count,
                        right_count,
                        max_items: effective_max,
                    };
                    let page = page_s.peek().clamp(1, page_count.max(1));
                    let content = paint_builder_content(
                        &id_prefix,
                        page,
                        page_count,
                        range,
                        &prev_m,
                        &next_m,
                        compact,
                        jumper_p.as_ref(),
                        page_s,
                        on_chg.clone(),
                        &parts_fit,
                    );
                    el.children = vec![content];
                    LayoutStatus::StructureChanged
                });
            }
            root
        });
        // hug 内容高度；勿用 reactive_view 默认 100%×100%（会撑满父级 / 盖住邻居）。
        // min_w(0) 允许在窄 flex 容器内收缩，避免撑破父级。
        reactive_placeholder_sized(view_id, "100%", "auto")
            .min_w(0.0)
            .overflow_x("hidden")
            .overflow_y("visible")
    }
}

impl crate::core::tree::IntoElementList for PaginationBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_small_lists_all_pages() {
        assert_eq!(
            pagination_range(1, 5, PaginationRangeOpts::new(1, 1, 1)),
            (1..=5).map(PaginationPage::Number).collect::<Vec<_>>()
        );
    }

    #[test]
    fn range_near_start_expands_to_keep_stable_width() {
        // L=3,S=1,R=1 → 目标格子数 3+1+2+1+2=9；靠前扩展左段。
        // left_expanded = 3+3+1 = 7 → 1..7 … 20
        let r = pagination_range(1, 20, PaginationRangeOpts::new(3, 1, 1));
        assert_eq!(
            r,
            vec![
                PaginationPage::Number(1),
                PaginationPage::Number(2),
                PaginationPage::Number(3),
                PaginationPage::Number(4),
                PaginationPage::Number(5),
                PaginationPage::Number(6),
                PaginationPage::Number(7),
                PaginationPage::Ellipsis,
                PaginationPage::Number(20),
            ]
        );
    }

    #[test]
    fn range_middle_with_single_boundaries() {
        // 1 … 5 6 7 … n
        let r = pagination_range(6, 20, PaginationRangeOpts::new(1, 1, 1));
        assert_eq!(
            r,
            vec![
                PaginationPage::Number(1),
                PaginationPage::Ellipsis,
                PaginationPage::Number(5),
                PaginationPage::Number(6),
                PaginationPage::Number(7),
                PaginationPage::Ellipsis,
                PaginationPage::Number(20),
            ]
        );
    }

    #[test]
    fn range_three_segments_configurable() {
        // 1 2 3 … 6 7 8 9 10 … n-3 n-2 n-1 n
        let r = pagination_range(8, 30, PaginationRangeOpts::new(3, 2, 4));
        assert_eq!(
            r,
            vec![
                PaginationPage::Number(1),
                PaginationPage::Number(2),
                PaginationPage::Number(3),
                PaginationPage::Ellipsis,
                PaginationPage::Number(6),
                PaginationPage::Number(7),
                PaginationPage::Number(8),
                PaginationPage::Number(9),
                PaginationPage::Number(10),
                PaginationPage::Ellipsis,
                PaginationPage::Number(27),
                PaginationPage::Number(28),
                PaginationPage::Number(29),
                PaginationPage::Number(30),
            ]
        );
    }

    #[test]
    fn range_item_count_stable_across_pages() {
        let opts = PaginationRangeOpts::new(3, 2, 4);
        let expected = range_item_budget(3, 2, 4); // 3+4+5+2 = 14
        for page in 1..=30 {
            let r = pagination_range(page, 30, opts);
            assert_eq!(
                r.len(),
                expected,
                "page {page}: expected stable len {expected}, got {:?} (len={})",
                r,
                r.len()
            );
        }
    }

    #[test]
    fn range_max_items_compresses_caps() {
        let opts = PaginationRangeOpts::new(3, 2, 4).max_items(7);
        // 压到预算 ≤ 7：最终约 left=1,sibling=0,right=1 → 1+1+1+2=5，或类似。
        let mid = pagination_range(15, 30, opts);
        assert!(
            mid.len() <= 7,
            "max_items must cap grid count, got {:?}",
            mid
        );
        let start = pagination_range(1, 30, opts);
        let end = pagination_range(30, 30, opts);
        assert_eq!(start.len(), mid.len());
        assert_eq!(end.len(), mid.len());
    }

    #[test]
    fn range_very_small_max_items_stays_compact() {
        let r = pagination_range(8, 30, PaginationRangeOpts::new(3, 2, 4).max_items(3));
        assert!(r.len() <= 3, "got {:?}", r);
        assert!(r.contains(&PaginationPage::Number(1)));
        assert!(r.contains(&PaginationPage::Number(30)));
    }

    #[test]
    fn fit_width_compresses_and_may_hide_nav_text() {
        let (wide_slots, wide_compact) = fit_pagination_for_width(800.0, "上一页", "下一页");
        assert!(
            wide_slots >= 10,
            "wide should fit many slots, got {wide_slots}"
        );
        assert!(!wide_compact);

        let (narrow_slots, narrow_compact) = fit_pagination_for_width(280.0, "上一页", "下一页");
        assert!(narrow_slots < wide_slots);
        assert!(narrow_slots >= 1);
        // 中文 Prev/Next 较宽，窄屏应收起文案。
        assert!(narrow_compact);

        assert_eq!(
            fit_pagination_for_width(0.0, "Previous", "Next"),
            (0, false)
        );
    }

    #[test]
    fn parse_jumper_page_accepts_integers_only() {
        assert_eq!(parse_jumper_page("12", 30), Some(12));
        assert_eq!(parse_jumper_page(" 30 ", 30), Some(30));
        assert_eq!(parse_jumper_page("99", 30), Some(30));
        assert_eq!(parse_jumper_page("0", 30), None);
        assert_eq!(parse_jumper_page("-1", 30), None);
        assert_eq!(parse_jumper_page("1.5", 30), None);
        assert_eq!(parse_jumper_page("abc", 30), None);
        assert_eq!(parse_jumper_page("", 30), None);
    }

    #[test]
    fn jumper_input_wires_enter_to_jump() {
        let page = use_signal("pagination_jumper_enter_page", || 3usize);
        let el = paint_jumper("pg-enter", 3, 20, "Go", page, None);
        fn find_template<'a>(el: &'a Element, tid: &str) -> Option<&'a Element> {
            if el.template_id.as_deref() == Some(tid) {
                return Some(el);
            }
            el.children.iter().find_map(|c| find_template(c, tid))
        }
        let input = find_template(&el, "pg-enter__jump_input").expect("jump input");
        assert!(
            input.on_key_down.is_some(),
            "jump input must wire on_key_down for Enter"
        );
    }

    #[test]
    fn builder_content_with_jumper_exposes_slot() {
        let page = use_signal("pagination_builder_jumper_page", || 3usize);
        let jumper = JumperPaint {
            button_text: "确定".into(),
        };
        let el = paint_builder_content(
            "pg-jump",
            3,
            20,
            PaginationRangeOpts::new(1, 1, 1),
            "上一页",
            "下一页",
            false,
            Some(&jumper),
            page,
            None,
            &PaginationPartChrome::default(),
        );
        fn find_slot<'a>(el: &'a Element, slot: &str) -> Option<&'a Element> {
            if el.attrs.get(ATTR_SLOT).map(String::as_str) == Some(slot) {
                return Some(el);
            }
            el.children.iter().find_map(|c| find_slot(c, slot))
        }
        assert!(
            find_slot(&el, SLOT_JUMPER).is_some(),
            "jumper slot must be present"
        );
    }

    #[test]
    fn jumper_chrome_reserves_width_for_fit() {
        let bare = fit_pagination_for_width(400.0, "上一页", "下一页").0;
        let chrome = estimate_jumper_chrome_px("确定", 30);
        assert!(chrome > 40.0);
        let with = fit_pagination_for_width_ex(400.0, "上一页", "下一页", chrome).0;
        assert!(with <= bare);
    }

    #[test]
    fn rsx_root_exposes_slot() {
        let el = Pagination(PaginationProps {
            id: String::new(),
            children: Some(PaginationContent(PaginationContentProps {
                children: Some(PaginationItem(PaginationItemProps {
                    children: Some(PaginationEllipsis(PaginationEllipsisProps {
                        children: None,
                        base: None,
                    })),
                    base: None,
                })),
                base: None,
            })),
            base: None,
        });
        assert_eq!(el.attrs.get(ATTR_SLOT).map(String::as_str), Some(SLOT_ROOT));
        assert!(el.template_id.is_none());
    }

    #[test]
    fn link_active_sets_data_active() {
        let el = PaginationLink(PaginationLinkProps {
            id: String::new(),
            is_active: Some(true),
            size: Some("icon".into()),
            disabled: Some(false),
            on_click: Some(Handler::default()),
            children: Some(Element::new().text("3")),
            base: None,
        });
        assert_eq!(
            el.attrs.get("data-active").map(String::as_str),
            Some("true")
        );
        assert_eq!(el.attrs.get(ATTR_SLOT).map(String::as_str), Some(SLOT_LINK));
    }

    #[test]
    fn pagination_link_wires_on_click() {
        use std::cell::Cell;
        let hit = Rc::new(Cell::new(0usize));
        let hit2 = Rc::clone(&hit);
        let el = PaginationLink(PaginationLinkProps {
            id: "pg-link-click".into(),
            is_active: Some(false),
            size: Some("icon".into()),
            disabled: Some(false),
            on_click: Some(Handler::new(move |_| hit2.set(hit2.get() + 1))),
            children: Some(Element::new().text("2")),
            base: None,
        });
        assert!(el.on_click.is_some(), "link must expose on_click");
        el.on_click.as_ref().unwrap().borrow_mut()();
        assert_eq!(hit.get(), 1);
    }

    #[test]
    fn previous_next_wire_on_click_when_enabled() {
        use std::cell::Cell;
        let hit = Rc::new(Cell::new(0usize));
        let hit_p = Rc::clone(&hit);
        let prev = PaginationPrevious(PaginationPreviousProps {
            id: "pg-prev".into(),
            text: Some("上一页".into()),
            hide_text: Some(false),
            disabled: Some(false),
            on_click: Some(Handler::new(move |_| hit_p.set(hit_p.get() + 1))),
            children: None,
            base: None,
        });
        assert_eq!(
            prev.attrs.get(ATTR_SLOT).map(String::as_str),
            Some(SLOT_PREV)
        );
        assert!(prev.on_click.is_some(), "previous must expose on_click");
        prev.on_click.as_ref().unwrap().borrow_mut()();
        assert_eq!(hit.get(), 1);

        let hit_n = Rc::clone(&hit);
        let next = PaginationNext(PaginationNextProps {
            id: "pg-next".into(),
            text: Some("下一页".into()),
            hide_text: Some(false),
            disabled: Some(false),
            on_click: Some(Handler::new(move |_| hit_n.set(hit_n.get() + 10))),
            children: None,
            base: None,
        });
        assert!(next.on_click.is_some(), "next must expose on_click");
        next.on_click.as_ref().unwrap().borrow_mut()();
        assert_eq!(hit.get(), 11);
    }

    #[test]
    fn previous_disabled_suppresses_on_click() {
        let prev = PaginationPrevious(PaginationPreviousProps {
            id: "pg-prev-dis".into(),
            text: Some("Prev".into()),
            hide_text: Some(false),
            disabled: Some(true),
            on_click: Some(Handler::new(move |_| panic!("disabled previous fired"))),
            children: None,
            base: None,
        });
        // Disabled buttons still expose a no-op interactive click that ignores the prop.
        if let Some(cb) = &prev.on_click {
            cb.borrow_mut()();
        }
    }

    #[test]
    fn item_keeps_previous_button_shell_with_icon_and_caption() {
        let prev = PaginationPrevious(PaginationPreviousProps {
            id: "pg-prev-keep".into(),
            text: Some("上一页".into()),
            hide_text: Some(false),
            disabled: Some(false),
            on_click: Some(Handler::new(|_| {})),
            children: None,
            base: None,
        });
        assert!(prev.children.len() >= 2, "prev has icon + caption");
        let item = PaginationItem(PaginationItemProps {
            children: Some(prev),
            base: None,
        });
        fn find_slot<'a>(el: &'a Element, slot: &str) -> Option<&'a Element> {
            if el.attrs.get(ATTR_SLOT).map(String::as_str) == Some(slot) {
                return Some(el);
            }
            el.children.iter().find_map(|c| find_slot(c, slot))
        }
        let kept = find_slot(&item, SLOT_PREV).expect("item must keep previous button shell");
        assert!(
            kept.on_click.is_some(),
            "unpacking icon+caption must not drop on_click"
        );
    }

    #[test]
    fn builder_content_wires_prev_next_and_updates_page() {
        let page = use_signal("pagination_builder_nav_page", || 5usize);
        let el = paint_builder_content(
            "pg-nav",
            5,
            12,
            PaginationRangeOpts::new(1, 1, 1),
            "上一页",
            "下一页",
            false,
            None,
            page,
            None,
            &PaginationPartChrome::default(),
        );
        assert_eq!(
            el.attrs.get(ATTR_SLOT).map(String::as_str),
            Some(SLOT_CONTENT)
        );

        fn find_slot<'a>(el: &'a Element, slot: &str) -> Option<&'a Element> {
            if el.attrs.get(ATTR_SLOT).map(String::as_str) == Some(slot) {
                return Some(el);
            }
            el.children.iter().find_map(|c| find_slot(c, slot))
        }
        let prev = find_slot(&el, SLOT_PREV).expect("prev");
        let next = find_slot(&el, SLOT_NEXT).expect("next");
        assert!(prev.on_click.is_some());
        assert!(next.on_click.is_some());
        prev.on_click.as_ref().unwrap().borrow_mut()();
        assert_eq!(page.peek(), 4);
        next.on_click.as_ref().unwrap().borrow_mut()();
        assert_eq!(page.peek(), 5);
    }

    #[test]
    fn builder_content_wires_page_link_clicks() {
        let page = use_signal("pagination_builder_click_page", || 2usize);
        let el = paint_builder_content(
            "pg-links",
            2,
            5,
            PaginationRangeOpts::new(1, 1, 1),
            "Prev",
            "Next",
            false,
            None,
            page,
            None,
            &PaginationPartChrome::default(),
        );
        assert_eq!(
            el.attrs.get(ATTR_SLOT).map(String::as_str),
            Some(SLOT_CONTENT)
        );
        let mut found_click = false;
        fn walk(el: &Element, found: &mut bool) {
            if el.attrs.get(ATTR_SLOT).map(String::as_str) == Some(SLOT_LINK)
                && el.on_click.is_some()
            {
                *found = true;
            }
            for c in &el.children {
                walk(c, found);
            }
        }
        walk(&el, &mut found_click);
        assert!(found_click, "builder page links must wire on_click");
    }
}
