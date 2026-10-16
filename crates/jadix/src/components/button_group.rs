//! # ButtonGroup
//!
//! ShadCN **base** Button Group: Alignment
//! `Reference/ShadCN-UI/.../bases/base/ui/button-group.tsx`。
//!
//! ```text
//! ButtonGroup                    // role=group · flex · orientation
//! ├── Button / … // Fill only · outer corners follow the group frame
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
//! - One control, one stroke. The group draws the perimeter. Items paint fill only
//!   (an inset SDF stroke is mixed with that item's own fill, so a pressed chip's
//!   seam looks heavier than its neighbor's). Internal seams are the same 1px
//!   strip, whether or not the author placed a separator. Hover must go through
//!   [`apply_button_group_join_chrome`] so a color write cannot put the per-item
//!   stroke back.
//! - `<ButtonGroup>` / Text / Separator takes the general PascalCase → `*Props` (same as Pagination, no special scope is required).

use crate::components::base::{packed_children, SlotChrome};
use crate::components::separator::separator;
use crate::core::tree::{CssDim, Element, UnitSlot};
use crate::impl_slot_chrome_methods;
use crate::primitives::r#box::{hstack, vstack};
use crate::utils::take_packed;
use crate::utils::text_node::{build_text_node, TextNodeConfig};
use jadix_macros::component;

const ATTR_SLOT: &str = "data-slot";
const SLOT_ROOT: &str = "button-group";
const SLOT_TEXT: &str = "button-group-text";
const SLOT_SEPARATOR: &str = "button-group-separator";

/// Marks collapsible segments within a group (read by Button hover paint).
pub(crate) const ATTR_GROUP_EDGE: &str = "jadix_button_group_edge";
pub(crate) const ATTR_GROUP_ORIENTATION: &str = "jadix_button_group_orientation";

const EDGE_START: &str = "start";
const EDGE_MIDDLE: &str = "middle";
const EDGE_END: &str = "end";
const EDGE_ONLY: &str = "only";

/// Drawtime widens toward shared edges (without entering Taffy). `x` = left +1px, `y` = top +1px.
///
/// OTP slots still use this. Joined groups do not: an inset stroke plus a 1px
/// shift draws a second line beside the neighbor.
pub(crate) const ATTR_PAINT_NUDGE: &str = "jadix_button_group_nudge";
/// Fill-corner radius (px) after the group takes the perimeter stroke.
const ATTR_GROUP_RADIUS: &str = "jadix_button_group_radius";
/// Paint the seam after the fills without opening a stacking band.
const ATTR_Z_PAINT_ONLY: &str = "jadix_z_paint_only";

/// Perimeter stroke width. The item fill radius is the outer radius minus this,
/// so the fill follows the inner curve of the group border.
const JOIN_BORDER_PX: f32 = 1.0;

/// Color and outer radius of the single frame around a joined group.
pub(crate) struct JoinFrame {
    pub color: [f32; 4],
    pub radius: f32,
}

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

/// `ui!` multi-child packs into `jadix_fragment` / anonymous shell; unpack via `take_packed`, then rearrange by group direction.

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

/// Clear leftover negative margin / z-index / translate and the per-item stroke.
///
/// The group owns the perimeter. Hover writes a 1px border back onto the button;
/// leaving it there mixes that stroke with this item's fill.
fn overlap_shared_edge(el: &mut Element) {
    if el.style.translate != [0.0, 0.0] {
        el.style.translate = [0.0, 0.0];
    }
    el.style.set_css_dim(UnitSlot::MarginL, CssDim::Px(0.0));
    el.style.set_css_dim(UnitSlot::MarginT, CssDim::Px(0.0));
    el.style.z_index = 0;
    el.style.set_uniform_border_width(0.0);
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

/// Expand a visual paint box to include the shared edge that the group chrome
/// draws outside the Taffy layout box. The same box must be used for damage and
/// retained-node matching; otherwise the dest reset erases the seam while the
/// node scissor clips the replacement paint back to the unnudged box.
pub(crate) fn expand_paint_nudge(el: &Element, bounds: [f32; 4]) -> [f32; 4] {
    let [dx, dy] = button_group_paint_nudge(el);
    if dx == 0.0 && dy == 0.0 {
        return bounds;
    }
    [
        bounds[0] + dx,
        bounds[1] + dy,
        (bounds[2] - dx).max(0.0),
        (bounds[3] - dy).max(0.0),
    ]
}

/// Visual box for chrome that opts into [`ATTR_PAINT_NUDGE`] (OTP slots).
///
/// Joined groups leave this box alone. Their perimeter is the group's border,
/// not a second stroke grown out of the item.
pub(crate) fn group_paint_bounds(el: &Element, bounds: [f32; 4]) -> [f32; 4] {
    expand_paint_nudge(el, bounds)
}

/// Use the same ownership box for hover damage as for retained node paint.
pub(crate) fn group_damage_bounds(el: &Element, bounds: [f32; 4]) -> [f32; 4] {
    group_paint_bounds(el, bounds)
}

fn positive_radius(el: &Element) -> f32 {
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
}

/// Rewrite fillets and drop the per-item stroke (after assembly and after Button hover).
pub(crate) fn apply_button_group_join_chrome(el: &mut Element) {
    let Some(edge) = el.attrs.get(ATTR_GROUP_EDGE).cloned() else {
        return;
    };
    let vertical = el
        .attrs
        .get(ATTR_GROUP_ORIENTATION)
        .map(|s| ButtonGroupOrientation::parse(s).is_vertical())
        .unwrap_or(false);
    let stored = el
        .attrs
        .get(ATTR_GROUP_RADIUS)
        .and_then(|s| s.parse::<f32>().ok());
    let r = if let Some(v) = stored {
        v.max(0.0)
    } else {
        // Standalone chrome (hover before the attr exists): inset once and remember it.
        // Reading the live radii again would shrink a pixel on every call.
        let fill = (positive_radius(el) - JOIN_BORDER_PX).max(0.0);
        el.set_attr(ATTR_GROUP_RADIUS, format!("{fill}"));
        fill
    };
    el.style.radii = join_radii(edge.as_str(), vertical, r);
    overlap_shared_edge(el);
}

fn mark_join_segment(
    el: &mut Element,
    edge: &str,
    orientation: ButtonGroupOrientation,
    fill_r: f32,
) {
    el.set_attr(ATTR_GROUP_EDGE, edge);
    el.set_attr(ATTR_GROUP_ORIENTATION, orientation.as_str());
    el.set_attr(ATTR_GROUP_RADIUS, format!("{fill_r}"));
    el.attrs.remove("jadix_button_group_prev_join");
    el.attrs.remove(ATTR_PAINT_NUDGE);
    apply_button_group_join_chrome(el);
}

/// 1px seam. Solid fill, no layout border — a 1px border-box plus a 1px stroke
/// collapses the content box and Taffy can grow the line to 2px.
/// Painted after the items (`z` + paint-only) so both sides of a pressed chip
/// cover the same strip; tree order would let the chip tint only the leading seam.
fn paint_seam(el: &mut Element, color: [f32; 4]) {
    let zero_min = taffy::style::Dimension::length(0.0);
    el.style.set_css_dim(UnitSlot::MarginT, CssDim::Px(0.0));
    el.style.set_css_dim(UnitSlot::MarginB, CssDim::Px(0.0));
    el.style.set_css_dim(UnitSlot::MarginL, CssDim::Px(0.0));
    el.style.set_css_dim(UnitSlot::MarginR, CssDim::Px(0.0));
    el.style.min_size.width = zero_min;
    el.style.min_size.height = zero_min;
    el.style.translate = [0.0, 0.0];
    el.style.bg_color = color;
    el.style.set_uniform_border_width(0.0);
    el.style.radii = [0.0; 4];
    el.style.z_index = 1;
    el.set_attr(ATTR_Z_PAINT_ONLY, "1");
}

fn join_divider(vertical_group: bool, color: [f32; 4]) -> Element {
    let mut el = ButtonGroupSeparator(ButtonGroupSeparatorProps {
        orientation: Some(if vertical_group {
            "horizontal".into()
        } else {
            "vertical".into()
        }),
        children: None,
        base: None,
        ..Default::default()
    });
    paint_seam(&mut el, color);
    el
}

/// Put the perimeter on `el`. Children stay in the content box, so they do not
/// cover the stroke the way a later sibling covers a shared edge.
pub(crate) fn paint_join_frame(el: &mut Element, frame: &JoinFrame) {
    // Default `border_style` is `none`, and `resolved_border_widths` then
    // reports 0: layout would not inset and the stroke would not be drawn.
    el.style.border_style = crate::style::LineStyle::Solid;
    el.style.border_color = frame.color;
    el.style.border_color_sides = None;
    el.style.set_uniform_border_width(JOIN_BORDER_PX);
    el.style.radii = [frame.radius; 4];
}

/// Fuse corners and, when any segment had a stroke, return the frame to paint
/// on the group. ToggleGroup `spacing=0` reuses this.
pub(crate) fn apply_button_group_join(
    kids: &mut Vec<Element>,
    orientation: ButtonGroupOrientation,
) -> Option<JoinFrame> {
    apply_group_join(kids, orientation)
}

fn apply_group_join(
    kids: &mut Vec<Element>,
    orientation: ButtonGroupOrientation,
) -> Option<JoinFrame> {
    for kid in kids.iter_mut() {
        kid.attrs.remove(ATTR_PAINT_NUDGE);
    }
    let segment_idx: Vec<usize> = kids
        .iter()
        .enumerate()
        .filter_map(|(i, el)| is_join_segment(el).then_some(i))
        .collect();
    let n = segment_idx.len();
    if n == 0 {
        return None;
    }
    let frame = segment_idx.iter().find_map(|&i| {
        let el = &kids[i];
        if el.style.border_width > 0.5 && el.style.border_color[3] > 0.01 {
            let radius = segment_idx
                .iter()
                .map(|&j| positive_radius(&kids[j]))
                .fold(0.0_f32, f32::max);
            Some(JoinFrame {
                color: el.style.border_color,
                radius,
            })
        } else {
            None
        }
    });
    let vertical = orientation.is_vertical();
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
        let fill_r = if let Some(frame) = &frame {
            (frame.radius - JOIN_BORDER_PX).max(0.0)
        } else {
            positive_radius(&kids[i])
        };
        mark_join_segment(&mut kids[i], edge, orientation, fill_r);
    }
    let Some(frame) = frame else {
        return None;
    };
    // Rebuild so adjacent segments gain one strip. An author separator already
    // is that strip — don't stack a second line beside it.
    let mut rebuilt = Vec::with_capacity(kids.len() + n);
    let mut prev_segment = false;
    for (i, mut kid) in kids.drain(..).enumerate() {
        let is_seg = segment_idx.binary_search(&i).is_ok();
        if is_seg && prev_segment {
            rebuilt.push(join_divider(vertical, frame.color));
        }
        if is_separator_slot(&kid) {
            paint_seam(&mut kid, frame.color);
            prev_segment = false;
        } else {
            prev_segment = is_seg;
        }
        rebuilt.push(kid);
    }
    *kids = rebuilt;
    Some(frame)
}

fn build_button_group(
    id: &str,
    orientation: ButtonGroupOrientation,
    children: Vec<Element>,
) -> Element {
    let mut kids = children;
    let frame = apply_group_join(&mut kids, orientation);

    let mut row = if orientation.is_vertical() {
        vstack().w("auto").min_w(0.0).items("stretch").gap(0.0)
    } else {
        hstack().w("auto").min_w(0.0).items("stretch").gap(0.0)
    };
    // Hug the buttons. A stretched auto box was invisible before; a perimeter
    // stroke would otherwise frame the whole parent track.
    if frame.is_some() {
        row = row.align_self_kw("start");
    }
    for k in kids {
        row = row.child(k);
    }
    let mut el: Element = row.into();
    if let Some(frame) = frame {
        paint_join_frame(&mut el, &frame);
    }
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
    // Don't use `h/w: 100%`; leave it to align-self: stretch. Margins stay 0 (`nest_group_separator`).
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

    fn segments(el: &Element) -> Vec<&Element> {
        el.children
            .iter()
            .filter(|c| c.attrs.contains_key(ATTR_GROUP_EDGE))
            .collect()
    }

    fn fill_radius() -> f32 {
        JOIN_RADIUS_PX - 1.0
    }

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
        let segs = segments(&el);
        assert_eq!(segs.len(), 2);
        assert_eq!(el.children.len(), 3, "one seam strip between the two fills");
        let first = segs[0];
        let second = segs[1];
        assert_eq!(
            first.attrs.get(ATTR_GROUP_EDGE).map(String::as_str),
            Some(EDGE_START)
        );
        assert_eq!(
            second.attrs.get(ATTR_GROUP_EDGE).map(String::as_str),
            Some(EDGE_END)
        );
        let r = fill_radius();
        // CSS [TL, TR, BR, BL]: start = left corners; end = right corners.
        // Fill radius is the frame radius minus the 1px perimeter.
        assert_eq!(first.style.radii, [r, 0.0, 0.0, r]);
        assert_eq!(second.style.radii, [0.0, r, r, 0.0]);
        assert!(
            first.style.border_width < 0.01 && second.style.border_width < 0.01,
            "joined items paint fill only; the group owns the stroke"
        );
        assert!(
            (el.style.border_width - 1.0).abs() < 0.01,
            "group perimeter is the single 1px stroke"
        );
        assert_eq!(el.style.radii, [JOIN_RADIUS_PX; 4]);
        assert!(second.attrs.get(ATTR_PAINT_NUDGE).is_none());
        assert_eq!(
            second.style.margin.left,
            taffy::style::LengthPercentageAuto::length(0.0)
        );
        assert_eq!(second.style.z_index, 0);
        assert_eq!(second.style.translate, [0.0, 0.0]);
        assert_eq!(
            el.children[1].style.z_index, 1,
            "seam paints after both fills"
        );
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
        assert_eq!(
            el.children.len(),
            3,
            "author separator is the seam; no extra strip"
        );
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
        let r = fill_radius();
        // Start: left round only; End: right round only (facing sides square → no pill gap).
        assert_eq!(a.style.radii, [r, 0.0, 0.0, r]);
        assert_eq!(b.style.radii, [0.0, r, r, 0.0]);
        assert!(a.style.border_width < 0.01 && b.style.border_width < 0.01);
        assert!((el.style.border_width - 1.0).abs() < 0.01);
        assert!(a.attrs.get(ATTR_PAINT_NUDGE).is_none());
        assert!(b.attrs.get(ATTR_PAINT_NUDGE).is_none());
        assert_eq!(
            b.style.margin.left,
            taffy::style::LengthPercentageAuto::length(0.0)
        );
        assert_eq!(
            sep.style.margin.left,
            taffy::style::LengthPercentageAuto::length(0.0)
        );
        assert_eq!(
            sep.style.margin.right,
            taffy::style::LengthPercentageAuto::length(0.0)
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
        let segs = segments(&el);
        assert_eq!(segs.len(), 3);
        assert_eq!(
            el.children.len(),
            5,
            "Archive|Open get one strip; the author separator is the Open|Delete seam"
        );
        assert_eq!(
            segs[0].attrs.get(ATTR_GROUP_EDGE).map(String::as_str),
            Some(EDGE_START)
        );
        assert_eq!(
            segs[1].attrs.get(ATTR_GROUP_EDGE).map(String::as_str),
            Some(EDGE_MIDDLE),
            "Open must be middle (square right), not end"
        );
        assert_eq!(
            segs[2].attrs.get(ATTR_GROUP_EDGE).map(String::as_str),
            Some(EDGE_END)
        );
        let r = fill_radius();
        assert_eq!(segs[1].style.radii, [0.0; 4]);
        assert_eq!(segs[2].style.radii, [0.0, r, r, 0.0]);
        assert!(segs.iter().all(|s| s.style.border_width < 0.01));
        assert!((el.style.border_width - 1.0).abs() < 0.01);
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
                seam > 0.5 && seam < 2.0,
                "Paste|Cut seam should be the 1px separator, not a collapsed overlap or a multi-pixel gap, got {seam}"
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
            let segs = segments(group);
            assert_eq!(segs.len(), 3);
            assert_eq!(group.children.len(), 5);
            let a = segs[0];
            let b = segs[1];
            let c = segs[2];
            let seam_ab = b.layout_bounds[0] - (a.layout_bounds[0] + a.layout_bounds[2]);
            let seam_bc = c.layout_bounds[0] - (b.layout_bounds[0] + b.layout_bounds[2]);
            assert!(
                seam_ab > 0.5 && seam_ab < 2.0,
                "Copy|Paste seam should be one 1px strip, got {seam_ab}"
            );
            assert!(
                (seam_bc - seam_ab).abs() < 0.25,
                "Paste|Cut seam must match Copy|Paste, got ab={seam_ab} bc={seam_bc}"
            );
            assert!(segs.iter().all(|s| s.style.border_width < 0.01));
            assert!((group.style.border_width - 1.0).abs() < 0.01);
            assert!(a.attrs.get(ATTR_PAINT_NUDGE).is_none());
            assert!(b.attrs.get(ATTR_PAINT_NUDGE).is_none());
            assert!(c.attrs.get(ATTR_PAINT_NUDGE).is_none());
            assert_eq!(group_paint_bounds(a, a.layout_bounds), a.layout_bounds);
            assert_eq!(group_paint_bounds(b, b.layout_bounds), b.layout_bounds);
            assert_eq!(group_paint_bounds(c, c.layout_bounds), c.layout_bounds);
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
            let segs = segments(group);
            assert_eq!(segs.len(), 3, "VStack pack must unpack to 3 buttons");
            let a = segs[0];
            let b = segs[1];
            let c = segs[2];
            let seam_ab = b.layout_bounds[0] - (a.layout_bounds[0] + a.layout_bounds[2]);
            let seam_bc = c.layout_bounds[0] - (b.layout_bounds[0] + b.layout_bounds[2]);
            assert!(
                seam_ab > 0.5 && seam_ab < 2.0,
                "RSX Copy|Paste seam should be one 1px strip, got {seam_ab}"
            );
            assert!(
                (seam_bc - seam_ab).abs() < 0.25,
                "RSX Paste|Cut seam must match Copy|Paste, got ab={seam_ab} bc={seam_bc}"
            );
            assert!(segs.iter().all(|s| s.style.border_width < 0.01));
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
            let result = layout.build_and_layout(&mut root, 200.0, 200.0, [-1.0, -1.0]);
            let group = &root.children[0];
            let segs = segments(group);
            assert_eq!(segs.len(), 3);
            let a = segs[0];
            let b = segs[1];
            let c = segs[2];
            let seam_ab = b.layout_bounds[1] - (a.layout_bounds[1] + a.layout_bounds[3]);
            let seam_bc = c.layout_bounds[1] - (b.layout_bounds[1] + b.layout_bounds[3]);
            assert!(
                seam_ab > 0.5 && seam_ab < 2.0,
                "Top|Middle seam should be one 1px strip, got {seam_ab}"
            );
            assert!(
                (seam_bc - seam_ab).abs() < 0.25,
                "Middle|Bottom seam must match Top|Middle, got ab={seam_ab} bc={seam_bc}"
            );
            assert!(segs.iter().all(|s| s.style.border_width < 0.01));
            assert_eq!(button_group_paint_nudge(b), [0.0, 0.0]);
            assert_eq!(button_group_paint_nudge(c), [0.0, 0.0]);
            let b_bounds = b.layout_bounds;
            assert!(
                result.node_paints.iter().any(|paint| {
                    (paint.bounds[0] - b_bounds[0]).abs() < 0.01
                        && (paint.bounds[1] - b_bounds[1]).abs() < 0.01
                        && (paint.bounds[2] - b_bounds[2]).abs() < 0.01
                        && (paint.bounds[3] - b_bounds[3]).abs() < 0.01
                }),
                "retained paint bounds stay on the layout box; paints={:?}, layout={b_bounds:?}",
                result
                    .node_paints
                    .iter()
                    .map(|paint| paint.bounds)
                    .collect::<Vec<_>>()
            );
            let group_bounds = group.layout_bounds;
            assert!(
                result.instances.iter().any(|instance| {
                    (instance.rect_pos[0] - group_bounds[0]).abs() < 0.01
                        && (instance.rect_pos[1] - group_bounds[1]).abs() < 0.01
                        && (instance.rect_size[0] - group_bounds[2]).abs() < 0.01
                        && (instance.rect_size[1] - group_bounds[3]).abs() < 0.01
                        && instance.border_width > 0.5
                }),
                "the group draws the only perimeter stroke; bounds={:?} instances={:?}",
                group.layout_bounds,
                result
                    .instances
                    .iter()
                    .map(|instance| {
                        (instance.rect_pos, instance.rect_size, instance.border_width)
                    })
                    .collect::<Vec<_>>()
            );
            let middle_bounds = b.layout_bounds;
            assert!(
                result.instances.iter().any(|instance| {
                    (instance.rect_pos[0] - middle_bounds[0]).abs() < 0.01
                        && (instance.rect_pos[1] - middle_bounds[1]).abs() < 0.01
                        && (instance.rect_size[0] - middle_bounds[2]).abs() < 0.01
                        && (instance.rect_size[1] - middle_bounds[3]).abs() < 0.01
                        && instance.border_width < 0.01
                }),
                "the middle segment is fill only, so a pressed bg cannot recolor one seam; instances={:?}",
                result
                    .instances
                    .iter()
                    .map(|instance| (instance.rect_pos, instance.rect_size, instance.border_width))
                    .collect::<Vec<_>>()
            );
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
    fn joined_hover_at_125_dpi_replays_the_seam_the_scissor_eats() {
        use crate::app::damage::damage_to_scissor_physical;
        use crate::app::draw_cull::{
            last_present_visual_slots, node_present_replays_shared_seam, node_present_reset_rect,
        };
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
            let result = layout.build_and_layout(&mut root, 200.0, 200.0, [-1.0, -1.0]);
            let group = &root.children[0];
            let segs = segments(group);
            let top = segs[0];
            let middle = segs[1];
            let seam = &group.children[1];
            let key = u64::from(top.layout_node.expect("top layout node"));
            let bounds = top.layout_bounds;
            let seam_box = seam.layout_bounds;
            let slots = last_present_visual_slots(
                &result.node_paints,
                &result.instances,
                &[],
                &result.text_nodes,
                &[],
            );
            let prev = slots.iter().find(|(k, _)| *k == key).map(|(_, b)| *b);
            let scale = 1.25_f32;
            let wipe = node_present_reset_rect(
                key,
                bounds,
                &result.node_paints,
                &result.instances,
                &[],
                &[],
                0.0,
                scale,
                prev,
            )
            .expect("joined hover has a dest-reset");
            let logical = [
                wipe[0] / scale,
                wipe[1] / scale,
                wipe[2] / scale,
                wipe[3] / scale,
            ];
            let (x, y, w, h) =
                damage_to_scissor_physical(logical, 0.0, scale, 800, 600).expect("scissor");
            let hole = [
                x as f32 / scale,
                y as f32 / scale,
                w as f32 / scale,
                h as f32 / scale,
            ];
            let overlap_y =
                (hole[1] + hole[3]).min(seam_box[1] + seam_box[3]) - hole[1].max(seam_box[1]);
            assert!(
                overlap_y > 0.25,
                "125% DPI ceil must eat the Top|Middle seam (that is the bug)\n hole={hole:?} seam={seam_box:?} logical={logical:?}"
            );
            let seam_key = u64::from(seam.layout_node.expect("seam layout node"));
            let seam_paint = result
                .node_paints
                .iter()
                .find(|p| p.key == seam_key)
                .expect("seam slot");
            assert!(
                node_present_replays_shared_seam(
                    key,
                    bounds,
                    seam_paint,
                    hole,
                    &result.node_paints,
                ),
                "the eaten seam must be drawn again after the hover fill"
            );
            let middle_key = u64::from(middle.layout_node.expect("middle layout node"));
            let middle_paint = result
                .node_paints
                .iter()
                .find(|p| p.key == middle_key)
                .expect("middle slot");
            assert!(
                !node_present_replays_shared_seam(
                    key,
                    bounds,
                    middle_paint,
                    hole,
                    &result.node_paints,
                ),
                "the next item is not a hairline seam"
            );
            let lower = &group.children[3];
            let lower_key = u64::from(lower.layout_node.expect("lower seam"));
            let lower_paint = result
                .node_paints
                .iter()
                .find(|p| p.key == lower_key)
                .expect("lower seam slot");
            assert!(
                !node_present_replays_shared_seam(
                    key,
                    bounds,
                    lower_paint,
                    hole,
                    &result.node_paints,
                ),
                "Middle|Bottom must stay out of Top's hover hole"
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
        assert!(
            kid.style.border_width < 0.01,
            "hover must not put the per-item stroke back"
        );
        let r = fill_radius();
        assert_eq!(kid.style.radii, [0.0, r, r, 0.0]);
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
