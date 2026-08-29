//! # ButtonGroup
//!
//! ShadCN **base** 按钮组：对齐
//! `Reference/ShadCN-UI/.../bases/base/ui/button-group.tsx`。
//!
//! ```text
//! ButtonGroup                    // role=group · flex · orientation
//! ├── Button / …                 // 相邻圆角合并 · 绘制期叠边（均匀边框走 SDF）
//! ├── ButtonGroupText            // 组内文案槽（flex 壳 + 文案子节点）
//! └── ButtonGroupSeparator       // 组内分割线（默认竖线）
//! ```
//!
//! ## 用法（双写法 · ADR-9）
//! ```ignore
//! // A · RSX / 标签
//! <ButtonGroup>
//!     <Button variant="outline">"Copy"</Button>
//!     <Button variant="outline">"Paste"</Button>
//! </ButtonGroup>
//! <ButtonGroup orientation="vertical">
//!     <Button variant="outline">"Top"</Button>
//!     <ButtonGroupSeparator orientation="horizontal" />
//!     <Button variant="outline">"Bottom"</Button>
//! </ButtonGroup>
//!
//! // B · Builder
//! Element::from(
//!     button_group()
//!         .child(button("").variant("outline").text("Copy"))
//!         .child(button("").variant("outline").text("Paste"))
//! )
//! button_group().orientation("vertical").separator().child(…)
//! ```
//!
//! ## 注意
//! - **展示型**：空 `id` 不写 `template_id`（WP-2.8）。
//! - `ui!` 多子节点会包 fragment；根会 [`take_packed`] 拆开再按方向排。
//! - 圆角 / 共享边：对齐 ShadCN 连体观感，但**不把 -1px 写进布局**（竖组 `-mt` 会吃掉
//!   容器高度，下一段标题叠上来；再叠 `z-index` 就会盖住下层）。相邻段只把**绘制盒
//!   向共享边加宽 1px**（远侧边仍在布局位置）。整框平移会把中间项的另一侧也带走，
//!   后面的 Separator / 第三项就会露出双线。`ButtonGroupSeparator` 仍两侧 `-1px` 嵌缝。
//!   Hover 写色后须经 [`apply_button_group_join_chrome`] 重套圆角。
//! - `<ButtonGroup>` / Text / Separator 走通用 PascalCase → `*Props`（与 Pagination 同，无需专用 scope）。

use crate::components::separator::separator;
use crate::core::tree::{CssDim, Element, UnitSlot};
use crate::primitives::r#box::{hstack, vstack};
use crate::style::with_class_list;
use crate::utils::text_node::{build_text_node, TextNodeConfig};
use rui_macros::component;

const ATTR_SLOT: &str = "data-slot";
const SLOT_ROOT: &str = "button-group";
const SLOT_TEXT: &str = "button-group-text";
const SLOT_SEPARATOR: &str = "button-group-separator";

/// 标记组内可折叠段（Button hover paint 会读）。
pub(crate) const ATTR_GROUP_EDGE: &str = "rui_button_group_edge";
pub(crate) const ATTR_GROUP_ORIENTATION: &str = "rui_button_group_orientation";

const EDGE_START: &str = "start";
const EDGE_MIDDLE: &str = "middle";
const EDGE_END: &str = "end";
const EDGE_ONLY: &str = "only";

/// 绘制期向共享边加宽（不进 Taffy）。`x` = 左侧 +1px，`y` = 上侧 +1px。
pub(crate) const ATTR_PAINT_NUDGE: &str = "rui_button_group_nudge";

/// ShadCN Button 默认圆角 `rounded-md` ≈ 0.375rem。
const JOIN_RADIUS_PX: f32 = 0.375 * 16.0;

/// 组方向。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ButtonGroupOrientation {
    #[default]
    Horizontal,
    Vertical,
}

impl ButtonGroupOrientation {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "vertical" | "col" | "column" => Self::Vertical,
            _ => Self::Horizontal,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Horizontal => "horizontal",
            Self::Vertical => "vertical",
        }
    }

    pub fn is_vertical(self) -> bool {
        matches!(self, Self::Vertical)
    }
}

impl From<&str> for ButtonGroupOrientation {
    fn from(s: &str) -> Self {
        Self::parse(s)
    }
}

impl From<String> for ButtonGroupOrientation {
    fn from(s: String) -> Self {
        Self::parse(&s)
    }
}

fn is_identity_node(el: &Element) -> bool {
    el.on_click.is_some()
        || el.focus_id.is_some()
        || el.button_hover_paint.is_some()
        || el.template_id.is_some()
        || el.attrs.contains_key(ATTR_SLOT)
}

/// `ui!` 多子节点会打成 VStack / fragment；递归拆开再按组方向重排。
fn take_packed(mut el: Element) -> Vec<Element> {
    if is_identity_node(&el) {
        return vec![el];
    }
    if el.attrs.get("rui_fragment").map(|s| s.as_str()) == Some("1")
        || (el.text.as_ref().is_none_or(|t| t.is_empty()) && el.children.len() > 1)
    {
        return std::mem::take(&mut el.children)
            .into_iter()
            .flat_map(take_packed)
            .collect();
    }
    vec![el]
}

fn slot_of(el: &Element) -> Option<&str> {
    el.attrs.get(ATTR_SLOT).map(String::as_str)
}

fn is_separator_slot(el: &Element) -> bool {
    slot_of(el) == Some(SLOT_SEPARATOR)
}

/// 参与圆角/边框折叠的段：Button、组内 Text，或其它带 `data-slot` 的非分割线子项。
fn is_join_segment(el: &Element) -> bool {
    if is_separator_slot(el) {
        return false;
    }
    if el.button_hover_paint.is_some() {
        return true;
    }
    match slot_of(el) {
        Some(SLOT_TEXT) => true,
        Some(_) => true,
        None => el.on_click.is_some() || el.focus_id.is_some(),
    }
}

/// 内部半径顺序 **[TR, BR, TL, BL]**（见 `devtools/style_edit`）。
fn join_radii(edge: &str, vertical: bool, r: f32) -> [f32; 4] {
    match edge {
        EDGE_ONLY => [r, r, r, r],
        EDGE_START if vertical => [r, 0.0, r, 0.0], // TL TR
        EDGE_END if vertical => [0.0, r, 0.0, r],   // BL BR
        EDGE_MIDDLE => [0.0, 0.0, 0.0, 0.0],
        EDGE_START => [0.0, 0.0, r, r], // TL BL
        EDGE_END => [r, r, 0.0, 0.0],   // TR BR
        _ => [r, r, r, r],
    }
}

/// 清掉旧版布局叠边残留（负 margin / z-index / translate），保持均匀边框。
///
/// 绘制叠边标记 [`ATTR_PAINT_NUDGE`] 由 [`mark_join_segment`] 写入；此处不改 attr，
/// 以便 hover 重套 chrome 时不会把「分隔符后的 End」又标成叠边。
fn overlap_shared_edge(el: &mut Element) {
    if el.style.translate != [0.0, 0.0] {
        el.style.translate = [0.0, 0.0];
    }
    el.style.set_css_dim(UnitSlot::MarginL, CssDim::Px(0.0));
    el.style.set_css_dim(UnitSlot::MarginT, CssDim::Px(0.0));
    el.style.z_index = 0;

    if el.style.border_width_sides.is_some() {
        let w = el
            .style
            .resolved_border_widths()
            .into_iter()
            .fold(0.0_f32, f32::max)
            .max(el.style.border_width);
        if w > 0.5 {
            el.style.set_uniform_border_width(w);
        } else {
            el.style.border_width_sides = None;
        }
    }
}

/// 连体段绘制：向共享边伸出 1px（原点左/上移，尺寸加宽），远侧边仍在布局位置。
///
/// 整框平移会把中间项的右/下边也带走，后面的 Separator 或第三项就会和这条边并成 2px。
pub(crate) fn button_group_paint_nudge(el: &Element) -> [f32; 2] {
    match el.attrs.get(ATTR_PAINT_NUDGE).map(String::as_str) {
        Some("x") => [-1.0, 0.0],
        Some("y") => [0.0, -1.0],
        _ => [0.0, 0.0],
    }
}

/// 按组标记重写圆角与共享边（装配 + Button hover 写色后共用）。
pub(crate) fn apply_button_group_join_chrome(el: &mut Element) {
    let Some(edge) = el.attrs.get(ATTR_GROUP_EDGE).cloned() else {
        return;
    };
    let vertical = el
        .attrs
        .get(ATTR_GROUP_ORIENTATION)
        .map(|s| ButtonGroupOrientation::parse(s).is_vertical())
        .unwrap_or(false);
    let r = {
        let m = el
            .style
            .radii
            .iter()
            .copied()
            .filter(|v| *v > 0.0)
            .fold(0.0_f32, f32::max);
        if m > 0.0 {
            m
        } else {
            JOIN_RADIUS_PX
        }
    };
    el.style.radii = join_radii(edge.as_str(), vertical, r);
    overlap_shared_edge(el);
}

fn mark_join_segment(
    el: &mut Element,
    edge: &str,
    orientation: ButtonGroupOrientation,
    overlap_prev: bool,
) {
    el.set_attr(ATTR_GROUP_EDGE, edge);
    el.set_attr(ATTR_GROUP_ORIENTATION, orientation.as_str());
    el.attrs.remove("rui_button_group_prev_join");
    if overlap_prev && matches!(edge, EDGE_MIDDLE | EDGE_END) {
        el.set_attr(
            ATTR_PAINT_NUDGE,
            if orientation.is_vertical() { "y" } else { "x" },
        );
    } else {
        el.attrs.remove(ATTR_PAINT_NUDGE);
    }
    apply_button_group_join_chrome(el);
}

/// Separator 嵌进两侧边框：1px 线 + 两侧 -1px margin（对齐 ShadCN）。
fn nest_group_separator(el: &mut Element, orientation: ButtonGroupOrientation) {
    let zero_min = taffy::style::Dimension::length(0.0);
    if orientation.is_vertical() {
        el.style.set_css_dim(UnitSlot::MarginT, CssDim::Px(-1.0));
        el.style.set_css_dim(UnitSlot::MarginB, CssDim::Px(-1.0));
        el.style.set_css_dim(UnitSlot::MarginL, CssDim::Px(0.0));
        el.style.set_css_dim(UnitSlot::MarginR, CssDim::Px(0.0));
    } else {
        el.style.set_css_dim(UnitSlot::MarginL, CssDim::Px(-1.0));
        el.style.set_css_dim(UnitSlot::MarginR, CssDim::Px(-1.0));
        el.style.set_css_dim(UnitSlot::MarginT, CssDim::Px(0.0));
        el.style.set_css_dim(UnitSlot::MarginB, CssDim::Px(0.0));
    }
    el.style.min_size.width = zero_min;
    el.style.min_size.height = zero_min;
    el.style.translate = [0.0, 0.0];
}

fn apply_group_join(kids: &mut [Element], orientation: ButtonGroupOrientation) {
    let segment_idx: Vec<usize> = kids
        .iter()
        .enumerate()
        .filter_map(|(i, el)| is_join_segment(el).then_some(i))
        .collect();
    let n = segment_idx.len();
    if n == 0 {
        return;
    }
    for (k, &i) in segment_idx.iter().enumerate() {
        let edge = if n == 1 {
            EDGE_ONLY
        } else if k == 0 {
            EDGE_START
        } else if k + 1 == n {
            EDGE_END
        } else {
            EDGE_MIDDLE
        };
        // 仅与前一连体段相邻时绘制叠边；夹 Separator 时由 sep 的 -1px 嵌缝负责。
        let overlap_prev = k > 0 && i == segment_idx[k - 1] + 1;
        mark_join_segment(&mut kids[i], edge, orientation, overlap_prev);
    }
    for kid in kids.iter_mut() {
        if is_separator_slot(kid) {
            nest_group_separator(kid, orientation);
        }
    }
}

fn build_button_group(
    id: &str,
    orientation: ButtonGroupOrientation,
    class: Option<String>,
    children: Vec<Element>,
) -> Element {
    let mut kids = children;
    apply_group_join(&mut kids, orientation);

    let mut row = if orientation.is_vertical() {
        vstack().w("auto").min_w(0.0).items("stretch").gap(0.0)
    } else {
        hstack().w("auto").min_w(0.0).items("stretch").gap(0.0)
    };
    for k in kids {
        row = row.child(k);
    }
    let mut el: Element = row.into();
    el.set_attr(ATTR_SLOT, SLOT_ROOT);
    el.set_attr("data-orientation", orientation.as_str());
    el = el.accesskit_role("group");
    if let Some(c) = class.filter(|s| !s.trim().is_empty()) {
        el = with_class_list(el, &c);
    }
    if !id.trim().is_empty() {
        el.template_id = Some(id.to_string());
    }
    el
}

// ---------------------------------------------------------------------------
// RSX parts
// ---------------------------------------------------------------------------

/// 根：`role=group` · 横向/纵向 flex · 子项圆角合并。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn ButtonGroup(
    id: String,
    orientation: Option<String>,
    class: Option<String>,
    children: Option<Element>,
) -> Element {
    let orient = orientation
        .as_deref()
        .map(ButtonGroupOrientation::parse)
        .unwrap_or_default();
    let kids = children.map(take_packed).unwrap_or_default();
    build_button_group(&id, orient, class, kids)
}

/// 组内文案槽：flex 壳 + 文案子节点（勿把 text 写在带 border 的根上，stretch 后会贴顶）。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn ButtonGroupText(class: Option<String>, children: Option<Element>) -> Element {
    let label = children
        .map(|c| {
            if let Some(t) = c.text.clone().filter(|s| !s.is_empty()) {
                t
            } else {
                extract_text(&c)
            }
        })
        .unwrap_or_default();
    // 对齐 new-york-v4：`rounded-md border bg-muted px-4 text-sm font-medium`
    let caption = build_text_node(
        label,
        TextNodeConfig {
            size_px: 14.0,
            weight: 500,
            color: crate::core::tree::parse_text_color("muted-foreground"),
            ..Default::default()
        },
    );
    let mut el: Element = hstack()
        .px("1rem")
        .items_center()
        .justify_center()
        .flex_shrink(0.0)
        .bg(crate::style::parse_color("muted"))
        .border(crate::style::parse_color("border"), 1.0)
        .rounded(JOIN_RADIUS_PX)
        .child(caption)
        .into();
    el.set_attr(ATTR_SLOT, SLOT_TEXT);
    if let Some(c) = class.filter(|s| !s.trim().is_empty()) {
        el = with_class_list(el, &c);
    }
    el
}

fn extract_text(el: &Element) -> String {
    if let Some(t) = &el.text {
        if !t.is_empty() {
            return t.clone();
        }
    }
    for c in &el.children {
        let t = extract_text(c);
        if !t.is_empty() {
            return t;
        }
    }
    String::new()
}

/// 组内分割线：默认 `orientation=vertical`（横组里的竖线）；可显式水平。
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn ButtonGroupSeparator(
    orientation: Option<String>,
    class: Option<String>,
    children: Option<Element>,
) -> Element {
    let _ = children;
    // ShadCN 默认 vertical separator（横组共享竖线）。
    let vertical = match orientation.as_deref().map(str::trim) {
        Some("horizontal") | Some("row") => false,
        _ => true,
    };
    let mut el: Element = separator().vertical(vertical).into();
    el.set_attr(ATTR_SLOT, SLOT_SEPARATOR);
    el.set_attr(
        "data-orientation",
        if vertical { "vertical" } else { "horizontal" },
    );
    // self-stretch：横组竖线撑满行高；竖组横线撑满列宽。
    // 勿用 `h/w: 100%`；交给 align-self: stretch。负 margin 由 `nest_group_separator` 写入。
    if vertical {
        el = el
            .h("auto")
            .w("1px")
            .min_w(0.0)
            .min_h(0.0)
            .align_self_kw("stretch")
            .flex_shrink(0.0);
    } else {
        el = el
            .w("auto")
            .h("1px")
            .min_w(0.0)
            .min_h(0.0)
            .align_self_kw("stretch")
            .flex_shrink(0.0);
    }
    if let Some(c) = class.filter(|s| !s.trim().is_empty()) {
        el = with_class_list(el, &c);
    }
    el
}

// ---------------------------------------------------------------------------
// Builder（ADR-9 面 B）
// ---------------------------------------------------------------------------

enum ButtonGroupPart {
    Child(Element),
    Separator { vertical: bool },
    Text(String),
}

/// Builder 状态。
pub struct ButtonGroupBuilder {
    id: String,
    class: String,
    orientation: ButtonGroupOrientation,
    parts: Vec<ButtonGroupPart>,
}

/// Builder 入口。
pub fn button_group() -> ButtonGroupBuilder {
    ButtonGroupBuilder {
        id: String::new(),
        class: String::new(),
        orientation: ButtonGroupOrientation::Horizontal,
        parts: Vec::new(),
    }
}

impl ButtonGroupBuilder {
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }

    pub fn class(mut self, class: impl Into<String>) -> Self {
        self.class = class.into();
        self
    }

    pub fn orientation(mut self, o: impl Into<ButtonGroupOrientation>) -> Self {
        self.orientation = o.into();
        self
    }

    pub fn child(mut self, el: impl Into<Element>) -> Self {
        self.parts.push(ButtonGroupPart::Child(el.into()));
        self
    }

    pub fn text(mut self, label: impl Into<String>) -> Self {
        self.parts.push(ButtonGroupPart::Text(label.into()));
        self
    }

    /// 插入分割线。未指定时：横组→竖线，竖组→横线。
    pub fn separator(mut self) -> Self {
        let vertical = !self.orientation.is_vertical();
        self.parts.push(ButtonGroupPart::Separator { vertical });
        self
    }

    pub fn separator_orientation(mut self, o: impl AsRef<str>) -> Self {
        let vertical = !matches!(
            o.as_ref().trim().to_ascii_lowercase().as_str(),
            "horizontal" | "row"
        );
        self.parts.push(ButtonGroupPart::Separator { vertical });
        self
    }

    /// 便捷：outline 按钮 + 可选点击。
    pub fn button(mut self, label: impl Into<String>, on_click: impl FnMut() + 'static) -> Self {
        use crate::components::button::button;
        let mut click = on_click;
        let el: Element = button("")
            .variant("outline")
            .text(label)
            .on_click(move || click())
            .into();
        self.parts.push(ButtonGroupPart::Child(el));
        self
    }
}

impl crate::core::tree::IntoElementList for ButtonGroupBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

impl From<ButtonGroupBuilder> for Element {
    fn from(builder: ButtonGroupBuilder) -> Self {
        let mut kids = Vec::new();
        for part in builder.parts {
            match part {
                ButtonGroupPart::Child(el) => kids.push(el),
                ButtonGroupPart::Text(label) => {
                    kids.push(ButtonGroupText(ButtonGroupTextProps {
                        class: None,
                        children: Some(Element::new().text(label)),
                        base: None,
                        ..Default::default()
                    }));
                }
                ButtonGroupPart::Separator { vertical } => {
                    kids.push(ButtonGroupSeparator(ButtonGroupSeparatorProps {
                        orientation: Some(if vertical { "vertical" } else { "horizontal" }.into()),
                        class: None,
                        children: None,
                        base: None,
                        ..Default::default()
                    }));
                }
            }
        }
        let class = if builder.class.trim().is_empty() {
            None
        } else {
            Some(builder.class)
        };
        build_button_group(&builder.id, builder.orientation, class, kids)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::button::button;

    #[test]
    fn root_slot_role_and_orientation() {
        let el = ButtonGroup(ButtonGroupProps {
            id: String::new(),
            orientation: Some("vertical".into()),
            class: None,
            children: Some(Element::new().text("x")),
            base: None,
            ..Default::default()
        });
        assert_eq!(el.attrs.get(ATTR_SLOT).map(String::as_str), Some(SLOT_ROOT));
        assert_eq!(
            el.attrs.get("data-orientation").map(String::as_str),
            Some("vertical")
        );
    }

    #[test]
    fn joins_outline_buttons_horizontal() {
        let a: Element = button("a").variant("outline").text("A").into();
        let b: Element = button("b").variant("outline").text("B").into();
        let el: Element = button_group().child(a).child(b).into();
        assert_eq!(el.children.len(), 2);
        let first = &el.children[0];
        let second = &el.children[1];
        assert_eq!(
            first.attrs.get(ATTR_GROUP_EDGE).map(String::as_str),
            Some(EDGE_START)
        );
        assert_eq!(
            second.attrs.get(ATTR_GROUP_EDGE).map(String::as_str),
            Some(EDGE_END)
        );
        // 内部 [TR, BR, TL, BL]：首段左圆、次段右圆；次段绘制期叠边，布局不写负 margin。
        assert_eq!(
            first.style.radii,
            [0.0, 0.0, JOIN_RADIUS_PX, JOIN_RADIUS_PX]
        );
        assert_eq!(
            second.style.radii,
            [JOIN_RADIUS_PX, JOIN_RADIUS_PX, 0.0, 0.0]
        );
        assert!(
            second.style.border_is_uniform(),
            "joined end must keep uniform border for SDF radii"
        );
        assert_eq!(
            second.attrs.get(ATTR_PAINT_NUDGE).map(String::as_str),
            Some("x"),
            "shared edge should overlap via paint nudge, not layout margin"
        );
        assert_eq!(
            second.style.margin.left,
            taffy::style::LengthPercentageAuto::length(0.0)
        );
        assert_eq!(second.style.z_index, 0);
        assert_eq!(second.style.translate, [0.0, 0.0]);
    }

    #[test]
    fn text_slot_is_flex_shell_with_caption_child() {
        let el = ButtonGroupText(ButtonGroupTextProps {
            class: None,
            children: Some(Element::new().text("Archive")),
            base: None,
            ..Default::default()
        });
        assert!(
            el.text.as_ref().is_none_or(|t| t.is_empty()),
            "caption must not live on the bordered root"
        );
        assert_eq!(el.children.len(), 1);
        assert_eq!(el.children[0].text.as_deref(), Some("Archive"));
        assert!(el.style.border_width > 0.5);
    }

    #[test]
    fn separator_between_buttons_still_joins_both_edges() {
        let el: Element = button_group()
            .button("Paste", || {})
            .separator()
            .button("Cut", || {})
            .into();
        assert_eq!(el.children.len(), 3);
        let a = &el.children[0];
        let sep = &el.children[1];
        let b = &el.children[2];
        assert_eq!(
            sep.attrs.get(ATTR_SLOT).map(String::as_str),
            Some(SLOT_SEPARATOR)
        );
        assert_eq!(
            a.attrs.get(ATTR_GROUP_EDGE).map(String::as_str),
            Some(EDGE_START),
            "button before separator is start, not end"
        );
        assert_eq!(
            b.attrs.get(ATTR_GROUP_EDGE).map(String::as_str),
            Some(EDGE_END),
            "button after separator must still join as end"
        );
        // Start: left round only; End: right round only (facing sides square → no pill gap).
        assert_eq!(a.style.radii, [0.0, 0.0, JOIN_RADIUS_PX, JOIN_RADIUS_PX]);
        assert_eq!(b.style.radii, [JOIN_RADIUS_PX, JOIN_RADIUS_PX, 0.0, 0.0]);
        // End 在 Separator 之后：布局叠边交给 sep，按钮不再绘制偏移（否则会盖住 1px 线）。
        assert!(
            b.attrs.get(ATTR_PAINT_NUDGE).is_none(),
            "segment after separator must not paint-nudge"
        );
        assert_eq!(
            b.style.margin.left,
            taffy::style::LengthPercentageAuto::length(0.0)
        );
        assert_eq!(
            sep.style.margin.left,
            taffy::style::LengthPercentageAuto::length(-1.0)
        );
        assert_eq!(
            sep.style.margin.right,
            taffy::style::LengthPercentageAuto::length(-1.0)
        );
        assert_eq!(b.style.translate, [0.0, 0.0]);
    }

    #[test]
    fn text_open_sep_delete_assigns_end_on_delete() {
        let kids = vec![
            ButtonGroupText(ButtonGroupTextProps {
                class: None,
                children: Some(Element::new().text("Archive")),
                base: None,
                ..Default::default()
            }),
            Element::from(button("open").variant("outline").text("Open")),
            ButtonGroupSeparator(ButtonGroupSeparatorProps {
                orientation: None,
                class: None,
                children: None,
                base: None,
                ..Default::default()
            }),
            Element::from(button("del").variant("outline").text("Delete")),
        ];
        let el = build_button_group("", ButtonGroupOrientation::Horizontal, None, kids);
        assert_eq!(el.children.len(), 4);
        assert_eq!(
            el.children[0]
                .attrs
                .get(ATTR_GROUP_EDGE)
                .map(String::as_str),
            Some(EDGE_START)
        );
        assert_eq!(
            el.children[1]
                .attrs
                .get(ATTR_GROUP_EDGE)
                .map(String::as_str),
            Some(EDGE_MIDDLE),
            "Open must be middle (square right), not end"
        );
        assert!(el.children[2].attrs.get(ATTR_GROUP_EDGE).is_none());
        assert_eq!(
            el.children[3]
                .attrs
                .get(ATTR_GROUP_EDGE)
                .map(String::as_str),
            Some(EDGE_END)
        );
        // Middle: no rounding; End: right only — facing sides stay square across the sep.
        assert_eq!(el.children[1].style.radii, [0.0; 4]);
        assert_eq!(
            el.children[3].style.radii,
            [JOIN_RADIUS_PX, JOIN_RADIUS_PX, 0.0, 0.0]
        );
        assert_eq!(
            el.children[1]
                .attrs
                .get(ATTR_PAINT_NUDGE)
                .map(String::as_str),
            Some("x"),
            "Open sits adjacent to Archive"
        );
        assert!(
            el.children[3].attrs.get(ATTR_PAINT_NUDGE).is_none(),
            "Delete follows separator — no paint nudge"
        );
    }

    #[test]
    fn separator_layout_width_is_one_px_not_min_rem() {
        use crate::core::reactivity::create_root;
        use crate::core::tree::LayoutTree;

        let (_handle, _) = create_root(|| {
            let mut root = Element::new().w(400.0).h(80.0).child(Element::from(
                button_group()
                    .button("Paste", || {})
                    .separator()
                    .button("Cut", || {}),
            ));
            let mut layout = LayoutTree::new();
            let _ = layout.build_and_layout(&mut root, 400.0, 80.0, [-1.0, -1.0]);
            let group = &root.children[0];
            assert_eq!(group.children.len(), 3);
            let sep = &group.children[1];
            let w = sep.layout_bounds[2];
            let h = sep.layout_bounds[3];
            assert!(
                w > 0.0 && w < 2.5,
                "vertical separator must stay ~1px wide (got {w}); min_w:1rem would open a white seam"
            );
            assert!(
                h > 20.0,
                "separator must stretch to button height (got {h}); zero-height sep paints a white seam"
            );
            let a = &group.children[0];
            let b = &group.children[2];
            let seam = b.layout_bounds[0] - (a.layout_bounds[0] + a.layout_bounds[2]);
            assert!(
                seam < 0.75,
                "Paste|Cut layout seam should nest via separator -1px margins, got {seam}"
            );
        });
    }

    #[test]
    fn three_outline_buttons_layout_seams_collapse() {
        use crate::core::reactivity::create_root;
        use crate::core::tree::LayoutTree;

        let (_handle, _) = create_root(|| {
            let mut root = Element::new().w(600.0).h(80.0).child(Element::from(
                button_group()
                    .child(button("a").variant("outline").text("Copy"))
                    .child(button("b").variant("outline").text("Paste"))
                    .child(button("c").variant("outline").text("Cut")),
            ));
            let mut layout = LayoutTree::new();
            let _ = layout.build_and_layout(&mut root, 600.0, 80.0, [-1.0, -1.0]);
            let group = &root.children[0];
            assert_eq!(group.children.len(), 3);
            let a = &group.children[0];
            let b = &group.children[1];
            let c = &group.children[2];
            let seam_ab = b.layout_bounds[0] - (a.layout_bounds[0] + a.layout_bounds[2]);
            let seam_bc = c.layout_bounds[0] - (b.layout_bounds[0] + b.layout_bounds[2]);
            assert!(
                seam_ab.abs() < 0.75,
                "Copy|Paste layout boxes should abut (paint-nudge overlap), got {seam_ab}"
            );
            assert!(
                seam_bc.abs() < 0.75,
                "Paste|Cut layout boxes should abut (paint-nudge overlap), got {seam_bc}"
            );
            assert_eq!(b.attrs.get(ATTR_PAINT_NUDGE).map(String::as_str), Some("x"));
            assert_eq!(
                c.attrs.get(ATTR_PAINT_NUDGE).map(String::as_str),
                Some("x"),
                "third segment also grows 1px toward previous; far edge stays at layout"
            );
            assert_eq!(button_group_paint_nudge(b), [-1.0, 0.0]);
            assert_eq!(button_group_paint_nudge(c), [-1.0, 0.0]);
            assert_eq!(
                b.style.margin.left,
                taffy::style::LengthPercentageAuto::length(0.0)
            );
            assert_eq!(
                c.style.margin.left,
                taffy::style::LengthPercentageAuto::length(0.0)
            );
        });
    }

    #[test]
    fn rsx_vstack_pack_three_buttons_still_overlaps() {
        use crate::core::reactivity::create_root;
        use crate::core::tree::{IntoElementList, LayoutTree};
        use crate::primitives::r#box::vstack;

        let (_handle, _) = create_root(|| {
            // 宏把多子节点打成 VStack；take_packed 必须拆开，否则横组会变成一列且无叠边。
            let packed = {
                let mut list = Vec::new();
                list.extend(
                    Element::from(button("a").variant("outline").text("Copy")).into_element_list(),
                );
                list.extend(
                    Element::from(button("b").variant("outline").text("Paste")).into_element_list(),
                );
                list.extend(
                    Element::from(button("c").variant("outline").text("Cut")).into_element_list(),
                );
                Element::from(vstack().children(list))
            };
            let mut root = Element::new()
                .w(600.0)
                .h(80.0)
                .child(ButtonGroup(ButtonGroupProps {
                    id: String::new(),
                    orientation: None,
                    class: None,
                    children: Some(packed),
                    base: None,
                    ..Default::default()
                }));
            let mut layout = LayoutTree::new();
            let _ = layout.build_and_layout(&mut root, 600.0, 80.0, [-1.0, -1.0]);
            let group = &root.children[0];
            assert_eq!(
                group.children.len(),
                3,
                "VStack pack must unpack to 3 buttons"
            );
            let a = &group.children[0];
            let b = &group.children[1];
            let c = &group.children[2];
            let seam_ab = b.layout_bounds[0] - (a.layout_bounds[0] + a.layout_bounds[2]);
            let seam_bc = c.layout_bounds[0] - (b.layout_bounds[0] + b.layout_bounds[2]);
            assert!(
                seam_ab.abs() < 0.75,
                "RSX Copy|Paste layout boxes should abut, got {seam_ab}"
            );
            assert!(
                seam_bc.abs() < 0.75,
                "RSX Paste|Cut layout boxes should abut, got {seam_bc}"
            );
            assert_eq!(b.attrs.get(ATTR_PAINT_NUDGE).map(String::as_str), Some("x"));
            assert_eq!(c.attrs.get(ATTR_PAINT_NUDGE).map(String::as_str), Some("x"));
        });
    }

    #[test]
    fn three_outline_buttons_vertical_seams_overlap() {
        use crate::core::reactivity::create_root;
        use crate::core::tree::LayoutTree;

        let (_handle, _) = create_root(|| {
            let mut root = Element::new().w(200.0).h(200.0).child(Element::from(
                button_group()
                    .orientation("vertical")
                    .child(button("t").variant("outline").text("Top"))
                    .child(button("m").variant("outline").text("Middle"))
                    .child(button("b").variant("outline").text("Bottom")),
            ));
            let mut layout = LayoutTree::new();
            let _ = layout.build_and_layout(&mut root, 200.0, 200.0, [-1.0, -1.0]);
            let group = &root.children[0];
            assert_eq!(group.children.len(), 3);
            let a = &group.children[0];
            let b = &group.children[1];
            let c = &group.children[2];
            let seam_ab = b.layout_bounds[1] - (a.layout_bounds[1] + a.layout_bounds[3]);
            let seam_bc = c.layout_bounds[1] - (b.layout_bounds[1] + b.layout_bounds[3]);
            assert!(
                seam_ab.abs() < 0.75,
                "Top|Middle layout boxes should abut (paint-nudge overlap), got {seam_ab}"
            );
            assert!(
                seam_bc.abs() < 0.75,
                "Middle|Bottom layout boxes should abut (paint-nudge overlap), got {seam_bc}"
            );
            assert_eq!(b.attrs.get(ATTR_PAINT_NUDGE).map(String::as_str), Some("y"));
            assert_eq!(
                c.attrs.get(ATTR_PAINT_NUDGE).map(String::as_str),
                Some("y"),
                "third vertical segment grows 1px toward previous; far edge stays at layout"
            );
            assert_eq!(button_group_paint_nudge(b), [0.0, -1.0]);
            assert_eq!(button_group_paint_nudge(c), [0.0, -1.0]);
            assert_eq!(c.style.z_index, 0);
            let group_bottom = group.layout_bounds[1] + group.layout_bounds[3];
            let last_bottom = c.layout_bounds[1] + c.layout_bounds[3];
            assert!(
                last_bottom <= group_bottom + 0.5,
                "vertical group must contain Bottom (got last={last_bottom} group={group_bottom}); negative margin used to collapse height and overlay the next Gallery label"
            );
        });
    }

    #[test]
    fn paint_nudge_parses_axis() {
        let mut el = Element::new();
        assert_eq!(button_group_paint_nudge(&el), [0.0, 0.0]);
        el.set_attr(ATTR_PAINT_NUDGE, "x");
        assert_eq!(button_group_paint_nudge(&el), [-1.0, 0.0]);
        el.set_attr(ATTR_PAINT_NUDGE, "y");
        assert_eq!(button_group_paint_nudge(&el), [0.0, -1.0]);
    }

    #[test]
    fn join_chrome_survives_uniform_border_reset() {
        let mut kid: Element = button("b").variant("outline").text("B").into();
        kid.set_attr(ATTR_GROUP_EDGE, EDGE_END);
        kid.set_attr(ATTR_GROUP_ORIENTATION, "horizontal");
        kid.set_attr(ATTR_PAINT_NUDGE, "x");
        apply_button_group_join_chrome(&mut kid);
        assert_eq!(
            kid.attrs.get(ATTR_PAINT_NUDGE).map(String::as_str),
            Some("x"),
            "hover chrome must preserve paint nudge"
        );
        kid.style.set_uniform_border_width(1.0);
        apply_button_group_join_chrome(&mut kid);
        assert_eq!(
            kid.attrs.get(ATTR_PAINT_NUDGE).map(String::as_str),
            Some("x"),
            "join chrome must keep paint nudge after set_uniform"
        );
        assert!(kid.style.border_is_uniform());
        assert_eq!(kid.style.radii, [JOIN_RADIUS_PX, JOIN_RADIUS_PX, 0.0, 0.0]);
    }

    #[test]
    fn dual_surface_compiles_shape() {
        let rsx = ButtonGroup(ButtonGroupProps {
            id: String::new(),
            orientation: None,
            class: None,
            children: Some(ButtonGroupText(ButtonGroupTextProps {
                class: None,
                children: Some(Element::new().text("Label")),
                base: None,
                ..Default::default()
            })),
            base: None,
            ..Default::default()
        });
        let built: Element = button_group().text("Label").into();
        assert_eq!(
            rsx.attrs.get(ATTR_SLOT).map(String::as_str),
            built.attrs.get(ATTR_SLOT).map(String::as_str)
        );
    }
}
