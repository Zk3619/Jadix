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
pub(crate) use crate::core::geometry::ATTR_PAINT_NUDGE;
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
    if el
        .authoring_state
        .get::<crate::components::button::ButtonHoverPaint>()
        .is_some()
    {
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
#[cfg(all(test, feature = "product-tests"))]
use crate::core::geometry::{
    paint_bounds as group_paint_bounds, paint_nudge as button_group_paint_nudge,
};

fn positive_radius(el: &Element) -> f32 {
    let m = el
        .style
        .radii
        .iter()
        .copied()
        .filter(|v| *v > 0.0)
        .fold(0.0_f32, f32::max);
    if m > 0.0 {
        [el.style.size.width, el.style.size.height]
            .into_iter()
            .filter_map(crate::layout::dim_length_px)
            .filter(|size| *size > 0.0)
            .fold(m, |radius, size| radius.min(size * 0.5))
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
/// Painted after the items (`z` + paint-only) to preserve both connecting borders
/// when either neighboring segment changes its fill.
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
    el.style.border_color = color;
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

pub(crate) fn attach_join_seams(element: &mut Element) {
    crate::authoring::presentation::attach_after_style(element, sync_join_seams);
}

fn sync_join_seams(group: &mut Element, is_hovered: &dyn Fn(&Element) -> bool) -> bool {
    if !matches!(slot_of(group), Some(SLOT_ROOT | "toggle-group"))
        || !group.children.iter().any(is_separator_slot)
    {
        return false;
    }
    let hovered = group.children.iter().position(is_hovered);
    let previous_hovered = group
        .attrs
        .get("jadix_button_group_hover_index")
        .and_then(|value| value.parse::<usize>().ok());
    let hover_changed = hovered != previous_hovered;
    if hover_changed {
        if let Some(index) = hovered {
            group.set_attr("jadix_button_group_hover_index", index.to_string());
        } else {
            group.attrs.remove("jadix_button_group_hover_index");
        }
    }
    let transitioning = group.children.iter().any(|item| {
        item.transition_snapshot.has_active_tweens() || item.transition_snapshot.settled_this_tick()
    });
    if transitioning || hover_changed {
        crate::app::damage::push_frame_paint_element(group, group.paint_scroll_offset());
    }
    transitioning || hover_changed
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
    attach_join_seams(&mut el);
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

crate::__component_contract! {
ButtonGroupProps [];
/// Root: `role=group` · Horizontal/vertical flex · Children's rounded corners are merged.
#[allow(non_snake_case)]
pub fn ButtonGroup(
identity [] id: String,
plain [] orientation: Option<String>,
plain [] children: Option<Element>,
) -> Element {
    let orient = orientation
        .as_deref()
        .map(ButtonGroupOrientation::parse)
        .unwrap_or_default();
    let kids = children.map(take_packed).unwrap_or_default();
    build_button_group(&id, orient, kids)
}
}

crate::__component_contract! {
ButtonGroupTextProps [];
/// Copywriting slot in the group: flex shell + copywriting sub-node (do not write text on the root with border, it will stick to the top after stretching).
#[allow(non_snake_case)]
pub fn ButtonGroupText(
plain [] children: Option<Element>,
) -> Element {
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

crate::__component_contract! {
ButtonGroupSeparatorProps [];
/// Dividing line within the group: Default is `orientation=vertical` (vertical line in the horizontal group); can be explicitly horizontal.
#[allow(non_snake_case)]
pub fn ButtonGroupSeparator(
plain [] orientation: Option<String>,
plain [] children: Option<Element>,
) -> Element {
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

#[cfg(all(test, feature = "product-tests"))]
#[path = "tests/button_group.rs"]
mod tests;
