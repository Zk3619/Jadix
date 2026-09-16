//! # ButtonGroup
//!
//! ShadCN **base** Button Group: Alignment
//! `Reference/ShadCN-UI/.../bases/base/ui/button-group.tsx`。
//!
//! ```text
//! ButtonGroup                    // role=group · flex · orientation
//! ├── Button / … // Merge adjacent fillets · Overlap edges during drawing (uniform borders go to SDF)
//! ├── ButtonGroupText // Copy slot in the group (flex shell + copy sub-node)
//! └── ButtonGroupSeparator //Separating line within the group (default vertical line)
//! ```
//!
//! ## Usage (dual style · ADR-9)
//! ```ignore
//! // A · RSX / tags
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
//! ## Notes
//! - **Presentational**: empty `id` omits `template_id` (WP-2.8).
//! - `ui!` 多子节点会包 fragment；根会 [`take_packed`] 拆开再按方向排。
//! - Rounded corners/shared edges: align ShadCN for a one-piece look, but **do not write -1px into the layout** (vertical group `-mt` will eat it
//!   The height of the container, the next section of title will be stacked; if you stack `z-index` again, it will cover the lower layer). Adjacent segments only draw boxes
//!   Widen the shared edge by 1px** (the far edge is still in layout position). Panning the entire frame will also take away the other side of the middle item.
//!   The subsequent Separator/third item will reveal double lines. `ButtonGroupSeparator` still has `-1px` caulking on both sides.
//!   Hover must go through [`apply_button_group_join_chrome`] to refill the corners after writing the color.
//! - `<ButtonGroup>` / Text / Separator takes the general PascalCase → `*Props` (same as Pagination, no special scope is required).

use crate::components::base::{packed_children, SlotChrome};
use crate::components::separator::separator;
use crate::core::tree::{CssDim, Element, UnitSlot};
use crate::impl_slot_chrome_methods;
use crate::primitives::r#box::{hstack, vstack};
use crate::utils::take_packed;
use crate::utils::text_node::{build_text_node, TextNodeConfig};
use rui_macros::component;

const ATTR_SLOT: &str = "data-slot";
const SLOT_ROOT: &str = "button-group";
const SLOT_TEXT: &str = "button-group-text";
const SLOT_SEPARATOR: &str = "button-group-separator";

/// Marks collapsible segments within a group (read by Button hover paint).
pub(crate) const ATTR_GROUP_EDGE: &str = "rui_button_group_edge";
pub(crate) const ATTR_GROUP_ORIENTATION: &str = "rui_button_group_orientation";

const EDGE_START: &str = "start";
const EDGE_MIDDLE: &str = "middle";
const EDGE_END: &str = "end";
const EDGE_ONLY: &str = "only";

/// Drawtime widens toward shared edges (without entering Taffy). `x` = left +1px, `y` = top +1px.
pub(crate) const ATTR_PAINT_NUDGE: &str = "rui_button_group_nudge";

/// ShadCN Button default rounded corners `rounded-md` ≈ 0.375rem.
const JOIN_RADIUS_PX: f32 = 0.375 * 16.0;

/// group direction.
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

/// `ui!` multi-child packs into `rui_fragment` / anonymous shell; unpack via `take_packed`, then rearrange by group direction.

fn slot_of(el: &Element) -> Option<&str> {
    el.attrs.get(ATTR_SLOT).map(String::as_str)
}

fn is_separator_slot(el: &Element) -> bool {
    slot_of(el) == Some(SLOT_SEPARATOR)
}

/// Segments participating in rounded corners/border folding: Button, Text within a group, or other non-splitting line items with `data-slot`.
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

/// CSS `border-radius` 顺序 **[TL, TR, BR, BL]**（与 [`crate::style::Style::radii`] 一致）。
fn join_radii(edge: &str, vertical: bool, r: f32) -> [f32; 4] {
    match edge {
        EDGE_ONLY => [r, r, r, r],
        EDGE_START if vertical => [r, r, 0.0, 0.0], // TL TR
        EDGE_END if vertical => [0.0, 0.0, r, r],   // BR BL
        EDGE_MIDDLE => [0.0, 0.0, 0.0, 0.0],
        EDGE_START => [r, 0.0, 0.0, r], // TL BL
        EDGE_END => [0.0, r, r, 0.0],   // TR BR
        _ => [r, r, r, r],
    }
}

/// Clear the overlapped edges (negative margin / z-index / translate) of the old version of the layout and maintain a uniform border.
///
/// Draw overlap mark [`ATTR_PAINT_NUDGE`] written by [`mark_join_segment`]; do not change attr here,
/// This will prevent the "End after the separator" from being marked as overlapped when hovering over chrome.
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

/// Drawing of conjoined segments: extends 1px toward the shared edge (origin moved left/up, size widened), with the far side still in the layout position.
///
/// Panning the entire frame will also take away the right/bottom edge of the middle item, and the subsequent Separator or third item will merge with this edge to form 2px.
pub(crate) fn button_group_paint_nudge(el: &Element) -> [f32; 2] {
    match el.attrs.get(ATTR_PAINT_NUDGE).map(String::as_str) {
        Some("x") => [-1.0, 0.0],
        Some("y") => [0.0, -1.0],
        _ => [0.0, 0.0],
    }
}

/// Rewrite fillets and shared edges by group tag (shared after assembly + Button hover color writing).
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

/// Separator embeds borders on both sides: 1px line + -1px margin on both sides (aligned to ShadCN).
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

/// Apply fused rounded corners / shared-edge paint nudge (ToggleGroup `spacing=0` reuses this).
pub(crate) fn apply_button_group_join(kids: &mut [Element], orientation: ButtonGroupOrientation) {
    apply_group_join(kids, orientation);
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
        // The overlapping edges are only drawn adjacent to the previous conjoined segment; the -1px caulk of the separator is responsible for this when clamping a Separator.
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
    if !id.trim().is_empty() {
        el.template_id = Some(id.to_string());
    }
    el
}

// ---------------------------------------------------------------------------
// RSX parts
// ---------------------------------------------------------------------------

/// Root: `role=group` · Horizontal/vertical flex · Children's rounded corners are merged.
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn ButtonGroup(id: String, orientation: Option<String>, children: Option<Element>) -> Element {
    let orient = orientation
        .as_deref()
        .map(ButtonGroupOrientation::parse)
        .unwrap_or_default();
    let kids = children.map(take_packed).unwrap_or_default();
    build_button_group(&id, orient, kids)
}

/// Copywriting slot in the group: flex shell + copywriting sub-node (do not write text on the root with border, it will stick to the top after stretching).
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn ButtonGroupText(children: Option<Element>) -> Element {
    let label = children
        .map(|c| {
            if let Some(t) = c.text.clone().filter(|s| !s.is_empty()) {
                t
            } else {
                extract_text(&c)
            }
        })
        .unwrap_or_default();
    // Alignment new-york-v4: `rounded-md border bg-muted px-4 text-sm font-medium`
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

/// Dividing line within the group: Default is `orientation=vertical` (vertical line in the horizontal group); can be explicitly horizontal.
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn ButtonGroupSeparator(orientation: Option<String>, children: Option<Element>) -> Element {
    let _ = children;
    // ShadCN defaults to vertical separator (horizontal groups share vertical lines).
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
    // self-stretch: The vertical lines in the horizontal group fill the row height; the horizontal lines in the vertical group fill the column width.
    // Don't use `h/w: 100%`; leave it to align-self: stretch. Negative margins are written by `nest_group_separator`.
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
    el
}

// ---------------------------------------------------------------------------
// Builder (ADR-9 surface B)
// ---------------------------------------------------------------------------

pub struct ButtonGroupTextBuilder {
    children: Vec<Element>,
    chrome: SlotChrome,
}

pub fn button_group_text() -> ButtonGroupTextBuilder {
    ButtonGroupTextBuilder {
        children: Vec::new(),
        chrome: SlotChrome::default(),
    }
}

impl ButtonGroupTextBuilder {
    impl_slot_chrome_methods!();

    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children.push(child.into());
        self
    }
}

impl crate::core::tree::IntoElementList for ButtonGroupTextBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

impl From<ButtonGroupTextBuilder> for Element {
    fn from(builder: ButtonGroupTextBuilder) -> Self {
        let el = ButtonGroupText(ButtonGroupTextProps {
            children: packed_children(builder.children),
            base: None,
            ..Default::default()
        });
        builder.chrome.apply(el)
    }
}

pub struct ButtonGroupSeparatorBuilder {
    chrome: SlotChrome,
    vertical: bool,
}

pub fn button_group_separator() -> ButtonGroupSeparatorBuilder {
    ButtonGroupSeparatorBuilder {
        chrome: SlotChrome::default(),
        vertical: true,
    }
}

impl ButtonGroupSeparatorBuilder {
    impl_slot_chrome_methods!();

    pub fn vertical(mut self, vertical: bool) -> Self {
        self.vertical = vertical;
        self
    }
}

impl crate::core::tree::IntoElementList for ButtonGroupSeparatorBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

impl From<ButtonGroupSeparatorBuilder> for Element {
    fn from(builder: ButtonGroupSeparatorBuilder) -> Self {
        let orient = if builder.vertical {
            "vertical"
        } else {
            "horizontal"
        };
        let el = ButtonGroupSeparator(ButtonGroupSeparatorProps {
            orientation: Some(orient.into()),
            children: None,
            base: None,
            ..Default::default()
        });
        builder.chrome.apply(el)
    }
}

/// Builder state.
pub struct ButtonGroupBuilder {
    id: String,
    chrome: SlotChrome,
    orientation: ButtonGroupOrientation,
    parts: Vec<Element>,
}

/// Builder entry.
pub fn button_group() -> ButtonGroupBuilder {
    ButtonGroupBuilder {
        id: String::new(),
        chrome: SlotChrome::default(),
        orientation: ButtonGroupOrientation::Horizontal,
        parts: Vec::new(),
    }
}

impl ButtonGroupBuilder {
    impl_slot_chrome_methods!();

    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }

    pub fn orientation(mut self, o: impl Into<ButtonGroupOrientation>) -> Self {
        self.orientation = o.into();
        self
    }

    pub fn child(mut self, el: impl Into<Element>) -> Self {
        self.parts.push(el.into());
        self
    }

    pub fn text(mut self, label: impl Into<String>) -> Self {
        self.parts.push(
            button_group_text()
                .child(Element::new().text(label.into()))
                .into(),
        );
        self
    }

    pub fn text_with(
        mut self,
        f: impl FnOnce(ButtonGroupTextBuilder) -> ButtonGroupTextBuilder,
    ) -> Self {
        self.parts.push(f(button_group_text()).into());
        self
    }

    /// Insert dividing line. When not specified: horizontal group → vertical line, vertical group → horizontal line.
    pub fn separator(mut self) -> Self {
        let vertical = !self.orientation.is_vertical();
        self.parts
            .push(button_group_separator().vertical(vertical).into());
        self
    }

    pub fn separator_with(
        mut self,
        f: impl FnOnce(ButtonGroupSeparatorBuilder) -> ButtonGroupSeparatorBuilder,
    ) -> Self {
        let vertical = !self.orientation.is_vertical();
        self.parts
            .push(f(button_group_separator().vertical(vertical)).into());
        self
    }

    pub fn separator_orientation(mut self, o: impl AsRef<str>) -> Self {
        let vertical = !matches!(
            o.as_ref().trim().to_ascii_lowercase().as_str(),
            "horizontal" | "row"
        );
        self.parts
            .push(button_group_separator().vertical(vertical).into());
        self
    }

    /// Convenience: outline button + optional click.
    pub fn button(mut self, label: impl Into<String>, on_click: impl FnMut() + 'static) -> Self {
        use crate::components::button::button;
        let mut click = on_click;
        let el: Element = button("")
            .variant("outline")
            .text(label)
            .on_click(move || click())
            .into();
        self.parts.push(el);
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
        let el = build_button_group(&builder.id, builder.orientation, builder.parts);
        builder.chrome.apply(el)
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
        // CSS [TL, TR, BR, BL]: start = left corners; end = right corners.
        assert_eq!(
            first.style.radii,
            [JOIN_RADIUS_PX, 0.0, 0.0, JOIN_RADIUS_PX]
        );
        assert_eq!(
            second.style.radii,
            [0.0, JOIN_RADIUS_PX, JOIN_RADIUS_PX, 0.0]
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
        assert_eq!(a.style.radii, [JOIN_RADIUS_PX, 0.0, 0.0, JOIN_RADIUS_PX]);
        assert_eq!(b.style.radii, [0.0, JOIN_RADIUS_PX, JOIN_RADIUS_PX, 0.0]);
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
                children: Some(Element::new().text("Archive")),
                base: None,
                ..Default::default()
            }),
            Element::from(button("open").variant("outline").text("Open")),
            ButtonGroupSeparator(ButtonGroupSeparatorProps {
                orientation: None,
                children: None,
                base: None,
                ..Default::default()
            }),
            Element::from(button("del").variant("outline").text("Delete")),
        ];
        let el = build_button_group("", ButtonGroupOrientation::Horizontal, kids);
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
            [0.0, JOIN_RADIUS_PX, JOIN_RADIUS_PX, 0.0]
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
            // The macro converts multiple child nodes into VStack; take_packed must be split, otherwise the horizontal group will become one column without overlapping edges.
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
        assert_eq!(kid.style.radii, [0.0, JOIN_RADIUS_PX, JOIN_RADIUS_PX, 0.0]);
    }

    #[test]
    fn dual_surface_compiles_shape() {
        let rsx = ButtonGroup(ButtonGroupProps {
            id: String::new(),
            orientation: None,
            children: Some(ButtonGroupText(ButtonGroupTextProps {
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

    #[test]
    fn text_with_style_text_size() {
        let el: Element = button_group()
            .text_with(|t| {
                t.style(|s| s.text_size(16.0))
                    .child(Element::new().text("Archive"))
            })
            .into();
        let text_slot = &el.children[0];
        assert_eq!(
            text_slot.attrs.get(ATTR_SLOT).map(String::as_str),
            Some(SLOT_TEXT)
        );
        assert_eq!(text_slot.text_style.size_resolved(), 16.0);
    }
}
