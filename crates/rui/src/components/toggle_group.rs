//! # ToggleGroup
//!
//! ShadCN **base** Toggle Group: a set of two-state buttons that share selection.
//! Align `Reference/ShadCN-UI/.../bases/base/ui/toggle-group.tsx`.
//!
//! In ShadCN the chips stay **visually independent** (default `spacing=2`). This crate adds a
//! **joined** layout (`spacing=0`) that reuses [`button_group`](super::button_group) fused
//! corners / shared-edge paint so the group can look like a ButtonGroup.
//!
//! ```text
//! ToggleGroup                       // role=group · flex · type / orientation / spacing
//! └── ToggleGroupItem               // Toggle chrome · value · inherits variant / size
//! ```
//!
//! ## Usage (dual style · ADR-9)
//! ```ignore
//! // A · RSX / tags (macro → toggle_group_scope before Item)
//! <ToggleGroup value={align} variant="outline">
//!     <ToggleGroupItem value="left">"Left"</ToggleGroupItem>
//!     <ToggleGroupItem value="center">"Center"</ToggleGroupItem>
//!     <ToggleGroupItem value="right">"Right"</ToggleGroupItem>
//! </ToggleGroup>
//! <ToggleGroup type="multiple" values={formats} spacing={0} variant="outline">
//!     <ToggleGroupItem value="bold">"B"</ToggleGroupItem>
//!     <ToggleGroupItem value="italic">"I"</ToggleGroupItem>
//! </ToggleGroup>
//!
//! // B · Builder
//! Element::from(
//!     toggle_group("align")
//!         .value_signal(align)
//!         .variant("outline")
//!         .options(vec![
//!             ("left", "Left"),
//!             ("center", "Center"),
//!         ]),
//! )
//! toggle_group("fmt").multiple(true).spacing(0).item("bold", "B").item("italic", "I")
//! ```
//!
//! ## Notes
//! - **Identity (WP-2.8)**: `id` optional; empty → [`resolve_instance_id`].
//! - `type="single"` (default) is **exclusive**; click again clears unless [`required`](ToggleGroupBuilder::required).
//!   `type="multiple"` / `multiple={true}` lets each item toggle independently.
//! - `variant` / `size` follow standalone [`Toggle`](super::toggle) (`default`/`outline`, `sm`/`default`/`lg`).
//! - `spacing=0` / `joined={true}` / `.layout("joined")` → ButtonGroup fused look.
//!   Default spacing `2` keeps a gap (ShadCN independent chips).
//! - `<ToggleGroup>` without `options` is expanded by the macro ([`toggle_group_scope`]).
//!   `options=…` stays on the generic PascalCase `ToggleGroupProps` path.
//! - Pressed chrome syncs via root [`crate::core::tree::ReactiveSync`] (no root UI effect).

use std::sync::OnceLock;

use crate::components::base::SlotChrome;
use crate::components::button_group::{
    apply_button_group_join, apply_button_group_join_chrome, ButtonGroupOrientation,
};
use crate::components::toggle::{toggle_hover_classes, toggle_size_pad};
use crate::core::reactivity::{
    create_context, provide_context, use_context, use_scoped_signal, use_signal, with_child_owner,
    Context, Signal,
};
use crate::core::tree::{parse_color, parse_text_color, Element};
use crate::primitives::interactive::interactive;
use crate::primitives::r#box::{hstack, vstack};
use crate::style::{
    control_focus_ring_active, ensure_class_list, focus_ring_color, focus_ring_params_for,
    is_variant_style_overridden, patch_class_set_base_paint, patch_class_set_base_radii,
    patch_class_set_base_shadow, ClassSet, VariantStyleField, ATTR_SYNC_LABEL_FG, TRANSPARENT,
};
use crate::tokens;
use crate::utils::controlled::resolve_string_signal;
use crate::utils::resolve_instance_id;
use crate::utils::roving::{attach_focus_roving, FocusRovingAxis};
use crate::utils::take_fragment;
use crate::utils::take_packed;
use crate::utils::text_node::{build_text_node, TextNodeConfig};
use crate::utils::toggle_state::bool_slot_key;
use rui_macros::component;

const ATTR_SLOT: &str = "data-slot";
const ATTR_VALUE: &str = "data-value";
const ATTR_SEGMENTED: &str = "rui_toggle_segmented";
const ATTR_TOGGLE_REQUIRED: &str = "rui_toggle_required";
const SLOT_ROOT: &str = "toggle-group";
const SLOT_ITEM: &str = "toggle-group-item";

/// ShadCN default `--gap: 2` → `0.5rem`.
const DEFAULT_SPACING: u8 = 2;

// ---------------------------------------------------------------------------
// Variant / size / orientation
// ---------------------------------------------------------------------------

/// Toggle Group chrome shares [`ToggleVariant`] / [`ToggleSize`].
pub use super::toggle::{ToggleSize as ToggleGroupSize, ToggleVariant as ToggleGroupVariant};

/// Group flex direction (ShadCN `orientation`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ToggleGroupOrientation {
    #[default]
    Horizontal,
    Vertical,
}

impl ToggleGroupOrientation {
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

    fn to_button_group(self) -> ButtonGroupOrientation {
        if self.is_vertical() {
            ButtonGroupOrientation::Vertical
        } else {
            ButtonGroupOrientation::Horizontal
        }
    }

    fn roving_axis(self) -> FocusRovingAxis {
        if self.is_vertical() {
            FocusRovingAxis::Vertical
        } else {
            FocusRovingAxis::Horizontal
        }
    }
}

impl From<&str> for ToggleGroupOrientation {
    fn from(s: &str) -> Self {
        Self::parse(s)
    }
}

impl From<String> for ToggleGroupOrientation {
    fn from(s: String) -> Self {
        Self::parse(&s)
    }
}

/// Visual layout: independent chips (ShadCN) vs ButtonGroup fused corners.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ToggleGroupLayout {
    #[default]
    Default,
    Joined,
}

impl ToggleGroupLayout {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Joined => "joined",
        }
    }

    pub fn is_joined(self) -> bool {
        matches!(self, Self::Joined)
    }
}

impl From<&str> for ToggleGroupLayout {
    fn from(s: &str) -> Self {
        s.parse().unwrap_or_default()
    }
}

impl From<String> for ToggleGroupLayout {
    fn from(s: String) -> Self {
        Self::from(s.as_str())
    }
}

impl std::str::FromStr for ToggleGroupLayout {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.trim().to_ascii_lowercase().as_str() {
            "joined" | "group" | "fused" => Self::Joined,
            _ => Self::Default,
        })
    }
}

/// Parse ShadCN `spacing` (`0` = fused, default `2` = independent gap).
pub fn parse_toggle_group_spacing(v: impl ToString) -> u8 {
    let s = v.to_string();
    let t = s.trim().to_ascii_lowercase();
    match t.as_str() {
        "" | "default" | "separated" => DEFAULT_SPACING,
        "joined" | "group" | "fused" => 0,
        other => other
            .parse::<f32>()
            .ok()
            .map(|n| n.round().clamp(0.0, 32.0) as u8)
            .unwrap_or(DEFAULT_SPACING),
    }
}

fn spacing_gap_rem(spacing: u8) -> String {
    if spacing == 0 {
        "0".into()
    } else {
        format!("{}rem", spacing as f32 * 0.25)
    }
}

fn parse_multiple_type(type_attr: Option<&str>, multiple_flag: bool) -> bool {
    if let Some(t) = type_attr {
        return t.trim().eq_ignore_ascii_case("multiple");
    }
    multiple_flag
}

// ---------------------------------------------------------------------------
// Context
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct ToggleGroupCtx {
    pub id: String,
    pub multiple: bool,
    pub required: bool,
    pub disabled: bool,
    pub variant: ToggleGroupVariant,
    pub size: ToggleGroupSize,
    pub segmented: bool,
    pub value_single: Signal<String>,
    pub value_multi: Signal<Vec<String>>,
}

fn toggle_group_context() -> Context<ToggleGroupCtx> {
    static CTX: OnceLock<Context<ToggleGroupCtx>> = OnceLock::new();
    *CTX.get_or_init(create_context::<ToggleGroupCtx>)
}

pub fn use_toggle_group() -> Option<ToggleGroupCtx> {
    use_context(toggle_group_context())
}

fn require_toggle_group(part: &str) -> ToggleGroupCtx {
    use_toggle_group().unwrap_or_else(|| {
        panic!("{part} must be used within <ToggleGroup> / toggle_group_scope");
    })
}

fn resolve_multi_signal(
    controlled: Option<Signal<Vec<String>>>,
    instance_id: &str,
    default: Vec<String>,
) -> Signal<Vec<String>> {
    match controlled {
        Some(sig) => sig,
        None => {
            let key = bool_slot_key(instance_id, "tg_multi");
            use_scoped_signal(&key, move || default)
        }
    }
}

// ---------------------------------------------------------------------------
// Selection
// ---------------------------------------------------------------------------

/// Resulting value after clicking a single-select chip.
pub(crate) fn single_select_after_click(current: &str, clicked: &str, required: bool) -> String {
    if current == clicked {
        if required {
            current.to_string()
        } else {
            String::new()
        }
    } else {
        clicked.to_string()
    }
}

/// Resolve a non-empty selection when `required` (invalid / empty → default → first).
pub(crate) fn coerce_required_single(
    current: &str,
    values: &[String],
    default_value: &str,
) -> Option<String> {
    if values.is_empty() {
        return None;
    }
    if values.iter().any(|v| v == current) {
        return None;
    }
    if !default_value.is_empty() && values.iter().any(|v| v == default_value) {
        return Some(default_value.to_string());
    }
    Some(values[0].clone())
}

/// Independent (non-exclusive) multi-select: add or remove `clicked`.
pub(crate) fn multi_select_after_click(current: &[String], clicked: &str) -> Vec<String> {
    let mut next = current.to_vec();
    if let Some(i) = next.iter().position(|x| x == clicked) {
        next.remove(i);
    } else {
        next.push(clicked.to_string());
    }
    next
}

fn item_is_pressed(ctx: &ToggleGroupCtx, value: &str) -> bool {
    if ctx.multiple {
        ctx.value_multi.peek().iter().any(|v| v == value)
    } else {
        ctx.value_single.peek() == value
    }
}

fn apply_item_click(ctx: &ToggleGroupCtx, value: &str) {
    if ctx.disabled {
        return;
    }
    if ctx.multiple {
        let next = multi_select_after_click(&ctx.value_multi.get(), value);
        ctx.value_multi.set(next);
    } else {
        let next = single_select_after_click(&ctx.value_single.get(), value, ctx.required);
        if ctx.value_single.peek() != next {
            ctx.value_single.set(next);
        }
    }
}

// ---------------------------------------------------------------------------
// Chrome
// ---------------------------------------------------------------------------

fn toggle_chip_colors(
    pressed: bool,
    disabled: bool,
    outline: bool,
    segmented: bool,
) -> ([f32; 4], [f32; 4], [u8; 4]) {
    let mut bg = if pressed {
        if segmented {
            parse_color("background")
        } else {
            parse_color("accent")
        }
    } else {
        parse_color("transparent")
    };
    let mut border = if outline && !segmented {
        parse_color("input")
    } else {
        parse_color("transparent")
    };
    let mut fg = if pressed {
        if segmented {
            parse_text_color("foreground")
        } else {
            parse_text_color("accent-foreground")
        }
    } else {
        parse_text_color("foreground")
    };
    if disabled {
        bg[3] *= 0.5;
        border[3] *= 0.5;
        fg[3] = (fg[3] as f32 * 0.5) as u8;
    }
    (bg, border, fg)
}

fn segmented_radii(index: usize, count: usize, r: f32) -> [f32; 4] {
    if count <= 1 {
        return [r, r, r, r];
    }
    if index == 0 {
        [r, 0.0, 0.0, r]
    } else if index + 1 == count {
        [0.0, r, r, 0.0]
    } else {
        [0.0, 0.0, 0.0, 0.0]
    }
}

fn apply_segmented_radii(kids: &mut [Element]) {
    let n = kids.len();
    let r = 0.25 * 16.0;
    for (i, kid) in kids.iter_mut().enumerate() {
        if kid.attrs.get(ATTR_SLOT).map(String::as_str) != Some(SLOT_ITEM) {
            continue;
        }
        kid.style.radii = segmented_radii(i, n, r);
    }
}

fn paint_descendant_fg(el: &mut Element, fg: [u8; 4], changed: &mut bool) {
    if el.text_style.color_resolved() != fg {
        el.text_style.color = Some(fg);
        *changed = true;
    }
    for child in &mut el.children {
        paint_descendant_fg(child, fg, changed);
    }
}

fn collect_item_values(root: &Element) -> Vec<String> {
    root.children
        .iter()
        .filter_map(|c| {
            if c.attrs.get(ATTR_SLOT).map(String::as_str) == Some(SLOT_ITEM) {
                c.attrs.get(ATTR_VALUE).cloned()
            } else {
                None
            }
        })
        .collect()
}

fn unpack_item_children(children: Option<Element>) -> Vec<Element> {
    children.map(take_packed).unwrap_or_default()
}

fn nodes_from_label_or_children(
    label: &str,
    children: Vec<Element>,
    fg: [u8; 4],
    text_px: f32,
) -> Vec<Element> {
    let cfg = TextNodeConfig {
        size_px: text_px,
        weight: 500,
        color: fg,
        ..Default::default()
    };
    let mut out = Vec::new();
    if !label.is_empty() {
        out.push(build_text_node(label, cfg.clone()));
    }
    for child in children {
        if keep_toggle_item_child(&child) {
            let mut child = child;
            if child.text_style.color != Some(fg) {
                child.text_style.color = Some(fg);
            }
            out.push(child);
            continue;
        }
        if child.children.is_empty() {
            if let Some(t) = child.text.clone().filter(|s| !s.is_empty()) {
                out.push(build_text_node(t, cfg.clone()));
                continue;
            }
        }
        out.push(child);
    }
    out
}

/// Lucide `<Icon>` / MorphIcon / image icons: do not re-wrap `.text` in a
/// default UI font — PUA glyphs would render as tofu / Latin lookalikes.
fn keep_toggle_item_child(child: &Element) -> bool {
    !child.children.is_empty()
        || child.text_style.font_family.is_some()
        || child.image_texture.is_some()
        || child.attrs.contains_key(crate::utils::ATTR_ICON_NAME)
        || child.attrs.contains_key("rui_morph_icon")
}

fn is_toggle_icon_node(el: &Element) -> bool {
    if el.text_style.font_family.is_some()
        || el.image_texture.is_some()
        || el.attrs.contains_key(crate::utils::ATTR_ICON_NAME)
        || el.attrs.contains_key("rui_morph_icon")
    {
        return true;
    }
    if el.text.as_ref().is_some_and(|t| !t.is_empty()) {
        return false;
    }
    !el.children.is_empty() && el.children.iter().all(is_toggle_icon_node)
}

/// Icon-only chips match Button `size="icon*"`: equal frame, no extra pad.
fn is_icon_only_item(label: &str, children: &[Element]) -> bool {
    label.is_empty() && !children.is_empty() && children.iter().all(is_toggle_icon_node)
}

fn build_toggle_group_item(
    ctx: &ToggleGroupCtx,
    value: &str,
    label: &str,
    children: Vec<Element>,
    item_disabled: bool,
    segmented: bool,
) -> Element {
    let disabled = item_disabled || ctx.disabled;
    let pressed = item_is_pressed(ctx, value);
    let outline = ctx.variant.is_outline();
    let (bg, border, fg) = toggle_chip_colors(pressed, disabled, outline, segmented);
    let border_w = if outline && !segmented { 1.0 } else { 0.0 };
    let (px, py, text_px) = toggle_size_pad(ctx.size);
    let min_px = ctx.size.min_px();
    let icon_only = is_icon_only_item(label, &children);

    let nodes = nodes_from_label_or_children(label, children, fg, text_px);
    let mut chip = hstack()
        .items_center()
        .justify_center()
        .gap("0.25rem")
        .flex_shrink(0.0)
        .bg(bg)
        .border(border, border_w);
    if icon_only {
        chip = chip.w(min_px).h(min_px).min_w(min_px).min_h(min_px);
    } else {
        chip = chip.px(px).py(py).min_w(min_px).min_h(min_px).h(min_px);
    }
    if !segmented {
        chip = chip.rounded("0.375rem");
    }
    for node in nodes {
        chip = chip.child(node);
    }
    let chip: Element = chip.into();

    let focus_id = format!("{}:{value}", ctx.id);
    let fid_click = focus_id.clone();
    let ctx_click = ctx.clone();
    let value_click = value.to_string();
    let mut out: Element = interactive(chip)
        .focus_id(focus_id)
        .focusable(!disabled)
        .focus_ring(!disabled)
        .on_click(move || {
            if !disabled {
                use_signal("__rui_button_focus_id", String::new).set(fid_click.clone());
            }
            apply_item_click(&ctx_click, &value_click);
        })
        .into();
    out.set_attr(ATTR_SLOT, SLOT_ITEM);
    out.set_attr(ATTR_VALUE, value);
    out.set_attr("data-state", if pressed { "on" } else { "off" });
    out.set_attr("data-variant", ctx.variant.as_str());
    out.set_attr("data-size", ctx.size.as_str());
    out.set_attr(ATTR_SYNC_LABEL_FG, "1");
    let _ = ensure_class_list(&mut out, toggle_hover_classes(outline, segmented));
    if disabled {
        out = out.cursor_not_allowed().pointer_events_none();
        out.set_attr("disabled", "true");
    } else {
        out = out.cursor_kind(crate::core::tree::CursorKind::Pointer);
    }
    out
}

fn is_toggle_item(el: &Element) -> bool {
    el.attrs.get(ATTR_SLOT).map(String::as_str) == Some(SLOT_ITEM)
}

// ---------------------------------------------------------------------------
// Scope (macro `<ToggleGroup>` without options)
// ---------------------------------------------------------------------------

/// Provide ToggleGroup context, then build the flex shell (RSX compound / Builder shared).
pub fn toggle_group_scope(
    id: impl Into<String>,
    value: Option<Signal<String>>,
    values: Option<Signal<Vec<String>>>,
    default_value: impl Into<String>,
    multiple: bool,
    disabled: bool,
    variant: impl Into<ToggleGroupVariant>,
    size: impl Into<ToggleGroupSize>,
    orientation: impl Into<ToggleGroupOrientation>,
    spacing: u8,
    joined: bool,
    segmented: bool,
    required: bool,
    body: impl FnOnce() -> Element,
) -> Element {
    let id = resolve_instance_id(id, "toggle-group");
    let default_value = default_value.into();
    let variant = variant.into();
    let size = size.into();
    let orientation = orientation.into();
    let spacing = if joined || segmented { 0 } else { spacing };
    let required = required && !multiple;
    let fused = spacing == 0;
    let use_button_group_join = fused && !segmented;

    let value_single = resolve_string_signal(value, &id, "tg", default_value.clone());
    let default_multi = if default_value.is_empty() {
        Vec::new()
    } else {
        vec![default_value.clone()]
    };
    let value_multi = resolve_multi_signal(values, &id, default_multi);

    let root = with_child_owner(move || {
        provide_context(
            toggle_group_context(),
            ToggleGroupCtx {
                id: id.clone(),
                multiple,
                required,
                disabled,
                variant,
                size,
                segmented,
                value_single,
                value_multi,
            },
        );
        let mut kids = take_fragment(body());
        if required {
            let values: Vec<String> = kids
                .iter()
                .filter_map(|c| {
                    if is_toggle_item(c) {
                        c.attrs.get(ATTR_VALUE).cloned()
                    } else {
                        None
                    }
                })
                .collect();
            if let Some(fixed) =
                coerce_required_single(&value_single.peek(), &values, &default_value)
            {
                value_single.set(fixed);
            }
        }
        if use_button_group_join {
            apply_button_group_join(&mut kids, orientation.to_button_group());
            for kid in kids.iter_mut() {
                patch_class_set_base_radii(kid);
            }
        } else if segmented {
            apply_segmented_radii(&mut kids);
        }

        let gap = spacing_gap_rem(spacing);
        let mut row = if orientation.is_vertical() {
            vstack()
                .w("auto")
                .min_w(0.0)
                .items("stretch")
                .gap(gap.as_str())
        } else {
            hstack()
                .w("auto")
                .min_w(0.0)
                .items("center")
                .gap(gap.as_str())
        };
        if segmented {
            row = row
                .p("0.125rem")
                .rounded("0.375rem")
                .bg(parse_color("muted"))
                .border(parse_color("input"), 1.0);
        }
        for k in kids {
            row = row.child(k);
        }
        let mut el: Element = row.into();
        el.set_attr(ATTR_SLOT, SLOT_ROOT);
        el.set_attr("data-variant", variant.as_str());
        el.set_attr("data-size", size.as_str());
        el.set_attr("data-spacing", spacing.to_string());
        el.set_attr("data-orientation", orientation.as_str());
        el.set_attr("data-multiple", if multiple { "true" } else { "false" });
        if segmented {
            el.set_attr(ATTR_SEGMENTED, "1");
        }
        if required {
            el.set_attr(ATTR_TOGGLE_REQUIRED, "1");
        }
        el = el.accesskit_role("group");

        let sync_single = value_single;
        let sync_multi = value_multi;
        let sync_required = required;
        let default_sync = default_value.clone();
        el = el.reactive_sync(move |root, _layout_tree| {
            let Some(mut single) = sync_single.try_peek() else {
                return crate::core::tree::LayoutStatus::Clean;
            };
            let Some(multi) = sync_multi.try_peek() else {
                return crate::core::tree::LayoutStatus::Clean;
            };
            if sync_required {
                let values = collect_item_values(root);
                if let Some(fixed) = coerce_required_single(&single, &values, &default_sync) {
                    // Silent: paint sync may hold AppState; avoid effect flush / reconcile.
                    if sync_single.try_set_silent(fixed.clone()) {
                        single = fixed;
                    }
                }
            }
            let outline = variant.is_outline();
            sync_toggle_group_pressed(
                root, outline, disabled, segmented, multiple, &single, &multi,
            )
        });
        attach_focus_roving(&mut el, orientation.roving_axis());
        el.template_id = Some(id.clone());
        el
    });
    root
}

fn sync_toggle_group_pressed(
    root: &mut Element,
    outline: bool,
    disabled: bool,
    segmented: bool,
    multiple: bool,
    current_single: &str,
    current_multi: &[String],
) -> crate::core::tree::LayoutStatus {
    let mut changed = false;
    let count = root.children.iter().filter(|c| is_toggle_item(c)).count();
    let border_w = if outline && !segmented { 1.0 } else { 0.0 };
    let mut item_index = 0usize;
    for option in root.children.iter_mut() {
        if !is_toggle_item(option) {
            continue;
        }
        let Some(value) = option.attrs.get(ATTR_VALUE).cloned() else {
            item_index += 1;
            continue;
        };
        let pressed = if multiple {
            current_multi.iter().any(|v| v == &value)
        } else {
            current_single == value
        };
        let (want_bg, want_border, want_fg) =
            toggle_chip_colors(pressed, disabled, outline, segmented);
        let _ = ensure_class_list(option, toggle_hover_classes(outline, segmented));
        let class_hover = option
            .class_set
            .as_ref()
            .is_some_and(ClassSet::has_hover_color);
        if !is_variant_style_overridden(&option.attrs, VariantStyleField::Bg)
            && option.style.bg_color != want_bg
        {
            option.style.bg_color = want_bg;
            changed = true;
        }
        if !is_variant_style_overridden(&option.attrs, VariantStyleField::Border)
            && option.style.border_color != want_border
        {
            option.style.border_color = want_border;
            changed = true;
        }
        if !is_variant_style_overridden(&option.attrs, VariantStyleField::Border)
            && option.style.border_width != border_w
        {
            option.style.border_width = border_w;
            changed = true;
        }
        let want_state = if pressed { "on" } else { "off" };
        if option.attrs.get("data-state").map(String::as_str) != Some(want_state) {
            option.set_attr("data-state", want_state);
            changed = true;
        }
        if segmented {
            let want_radii = segmented_radii(item_index, count, 0.25 * 16.0);
            if option.style.radii != want_radii {
                option.style.radii = want_radii;
                changed = true;
            }
            // Prefer keyboard focus ring over pressed chip shadow (same token/blur as Input).
            let show_ring = option
                .focus_id
                .as_deref()
                .is_some_and(|fid| control_focus_ring_active(fid, option.focusable, disabled));
            let (want_shadow, want_blur, want_spread) = if show_ring {
                let (blur, spread) = focus_ring_params_for(option);
                let color = option
                    .style
                    .focus_ring_color
                    .unwrap_or_else(focus_ring_color);
                (color, blur, spread)
            } else if pressed {
                (tokens::shadow_ink(0.08), 2.0, 0.0)
            } else {
                (TRANSPARENT, 0.0, 0.0)
            };
            if !is_variant_style_overridden(&option.attrs, VariantStyleField::Shadow)
                && ((option.style.shadow_blur - want_blur).abs() > 0.01
                    || (option.style.shadow_spread - want_spread).abs() > 0.01
                    || (option.style.shadow_color[0] - want_shadow[0]).abs() > 0.001
                    || (option.style.shadow_color[1] - want_shadow[1]).abs() > 0.001
                    || (option.style.shadow_color[2] - want_shadow[2]).abs() > 0.001
                    || (option.style.shadow_color[3] - want_shadow[3]).abs() > 0.001)
            {
                option.style.shadow_color = want_shadow;
                option.style.shadow_blur = want_blur;
                option.style.shadow_offset = [0.0, 0.0];
                option.style.shadow_spread = want_spread;
                changed = true;
            }
            if class_hover && !is_variant_style_overridden(&option.attrs, VariantStyleField::Shadow)
            {
                patch_class_set_base_shadow(option, want_shadow, want_blur);
            }
        } else if option.attrs.contains_key("rui_button_group_edge") {
            apply_button_group_join_chrome(option);
        }
        if !is_variant_style_overridden(&option.attrs, VariantStyleField::Fg) {
            paint_descendant_fg(option, want_fg, &mut changed);
        }
        if class_hover {
            patch_class_set_base_paint(
                option,
                option.style.bg_color,
                want_fg,
                (option.style.shadow_color, option.style.shadow_blur),
            );
        }
        item_index += 1;
    }
    if changed {
        crate::core::tree::LayoutStatus::StyleChanged
    } else {
        crate::core::tree::LayoutStatus::Clean
    }
}

// ---------------------------------------------------------------------------
// RSX parts
// ---------------------------------------------------------------------------

/// Root: `role=group` · exclusive or independent selection · optional ButtonGroup join.
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn ToggleGroup(
    id: String,
    multiple: bool,
    /// ShadCN `type="single" | "multiple"` (alias of `multiple`; `type` wins when set).
    selection: Option<String>,
    value: Option<Signal<String>>,
    values: Option<Signal<Vec<String>>>,
    default_value: String,
    options: Option<Vec<(String, String)>>,
    disabled: bool,
    variant: Option<String>,
    size: Option<String>,
    orientation: Option<String>,
    spacing: Option<String>,
    joined: bool,
    required: bool,
    children: Option<Element>,
) -> Element {
    let multiple = parse_multiple_type(selection.as_deref(), multiple);
    let variant = variant.unwrap_or_else(|| "default".into());
    let size = size.unwrap_or_else(|| "default".into());
    let orientation = orientation.unwrap_or_else(|| "horizontal".into());
    let spacing = spacing
        .map(parse_toggle_group_spacing)
        .unwrap_or(DEFAULT_SPACING);
    if let Some(opts) = options {
        let mut b = toggle_group(&id)
            .multiple(multiple)
            .default_value(default_value)
            .disabled(disabled)
            .variant(variant)
            .size(size)
            .orientation(orientation)
            .spacing(spacing)
            .joined(joined)
            .required(required)
            .options(opts);
        if let Some(sig) = value {
            b = b.value_signal(sig);
        }
        if let Some(sig) = values {
            b = b.value_multi_signal(sig);
        }
        return b.into();
    }
    toggle_group_scope(
        id,
        value,
        values,
        default_value,
        multiple,
        disabled,
        variant,
        size,
        orientation,
        spacing,
        joined,
        false,
        required,
        || children.unwrap_or_else(Element::new),
    )
}

/// ShadCN `ToggleGroupItem`: two-state chip; exclusivity comes from the parent group.
#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn ToggleGroupItem(value: String, disabled: bool, children: Option<Element>) -> Element {
    let ctx = require_toggle_group("ToggleGroupItem");
    let kids = unpack_item_children(children);
    build_toggle_group_item(&ctx, &value, "", kids, disabled, ctx.segmented)
}

// ---------------------------------------------------------------------------
// Builder (ADR-9 surface B)
// ---------------------------------------------------------------------------

/// One chip for Builder `.options()` sugar. Default chrome; per-item class/style → [`toggle_option`].
#[derive(Clone, Debug, Default)]
pub struct ToggleOption {
    pub value: String,
    pub label: String,
    pub chrome: SlotChrome,
}

/// Sugar entry: `toggle_group("align").options([toggle_option("left", "Left").class("ring-2")])`.
pub fn toggle_option(value: impl Into<String>, label: impl Into<String>) -> ToggleOption {
    ToggleOption {
        value: value.into(),
        label: label.into(),
        chrome: SlotChrome::default(),
    }
}

impl ToggleOption {
    crate::impl_slot_chrome_methods!();
}

impl From<(String, String)> for ToggleOption {
    fn from((value, label): (String, String)) -> Self {
        Self {
            value,
            label,
            chrome: SlotChrome::default(),
        }
    }
}

impl From<(&str, &str)> for ToggleOption {
    fn from((value, label): (&str, &str)) -> Self {
        Self {
            value: value.to_string(),
            label: label.to_string(),
            chrome: SlotChrome::default(),
        }
    }
}

pub struct ToggleGroupBuilder {
    id: String,
    multiple: bool,
    value_single: Option<Signal<String>>,
    value_multi: Option<Signal<Vec<String>>>,
    default_value: String,
    options: Vec<ToggleOption>,
    disabled: bool,
    variant: ToggleGroupVariant,
    size: ToggleGroupSize,
    orientation: ToggleGroupOrientation,
    spacing: u8,
    joined: bool,
    /// TabsList muted pill (not the public ButtonGroup-join variant).
    segmented: bool,
    required: bool,
    chrome: SlotChrome,
    child: Option<Box<dyn FnOnce() -> Element>>,
}

pub fn toggle_group(id: impl Into<String>) -> ToggleGroupBuilder {
    ToggleGroupBuilder {
        id: id.into(),
        multiple: false,
        value_single: None,
        value_multi: None,
        default_value: String::new(),
        options: Vec::new(),
        disabled: false,
        variant: ToggleGroupVariant::Default,
        size: ToggleGroupSize::Default,
        orientation: ToggleGroupOrientation::Horizontal,
        spacing: DEFAULT_SPACING,
        joined: false,
        segmented: false,
        required: false,
        chrome: SlotChrome::default(),
        child: None,
    }
}

impl ToggleGroupBuilder {
    crate::impl_slot_chrome_methods!();

    pub fn multiple(mut self, v: bool) -> Self {
        self.multiple = v;
        self
    }

    pub fn value_signal(mut self, s: Signal<String>) -> Self {
        self.value_single = Some(s);
        self
    }

    pub fn value_multi_signal(mut self, s: Signal<Vec<String>>) -> Self {
        self.value_multi = Some(s);
        self
    }

    pub fn default_value(mut self, v: impl Into<String>) -> Self {
        self.default_value = v.into();
        self
    }

    /// Data-driven sugar: [`toggle_option`] or `(value, label)` tuples.
    pub fn options(mut self, opts: impl IntoIterator<Item = impl Into<ToggleOption>>) -> Self {
        self.options = opts.into_iter().map(Into::into).collect();
        self
    }

    pub fn item(mut self, value: impl Into<String>, label: impl Into<String>) -> Self {
        self.options
            .push(ToggleOption::from((value.into(), label.into())));
        self
    }

    pub fn disabled(mut self, v: bool) -> Self {
        self.disabled = v;
        self
    }

    pub fn variant(mut self, v: impl Into<ToggleGroupVariant>) -> Self {
        self.variant = v.into();
        self
    }

    pub fn size(mut self, s: impl Into<ToggleGroupSize>) -> Self {
        self.size = s.into();
        self
    }

    pub fn orientation(mut self, o: impl Into<ToggleGroupOrientation>) -> Self {
        self.orientation = o.into();
        self
    }

    /// `0` = ButtonGroup fused look; default `2` = independent gap (ShadCN).
    pub fn spacing(mut self, v: impl ToString) -> Self {
        self.spacing = parse_toggle_group_spacing(v);
        self.joined = self.spacing == 0;
        self
    }

    /// ButtonGroup-style fused corners / shared edges (`spacing=0`).
    pub fn joined(mut self, v: bool) -> Self {
        self.joined = v;
        if v {
            self.spacing = 0;
        } else if self.spacing == 0 {
            self.spacing = DEFAULT_SPACING;
        }
        self
    }

    /// Layout axis: `default` (independent) / `joined` (ButtonGroup fusion).
    pub fn layout(self, v: impl Into<ToggleGroupLayout>) -> Self {
        self.joined(v.into().is_joined())
    }

    /// Single-select must keep one value (clicking the active chip does not clear).
    pub fn required(mut self, v: bool) -> Self {
        self.required = v;
        self
    }

    pub fn child(mut self, body: impl FnOnce() -> Element + 'static) -> Self {
        self.child = Some(Box::new(body));
        self
    }

    /// TabsList muted track + seamless chips (not the public joined variant).
    pub(crate) fn segmented(mut self, v: bool) -> Self {
        self.segmented = v;
        if v {
            self.spacing = 0;
        }
        self
    }
}

impl From<ToggleGroupBuilder> for Element {
    fn from(builder: ToggleGroupBuilder) -> Self {
        let chrome = builder.chrome.clone();
        let options = builder.options;
        let child = builder.child;
        chrome.apply(toggle_group_scope(
            builder.id,
            builder.value_single,
            builder.value_multi,
            builder.default_value,
            builder.multiple,
            builder.disabled,
            builder.variant,
            builder.size,
            builder.orientation,
            builder.spacing,
            builder.joined,
            builder.segmented,
            builder.required,
            move || {
                let ctx = require_toggle_group("toggle_group options");
                let mut list = Vec::with_capacity(options.len());
                for opt in options {
                    let el = build_toggle_group_item(
                        &ctx,
                        &opt.value,
                        &opt.label,
                        Vec::new(),
                        false,
                        ctx.segmented,
                    );
                    list.push(opt.chrome.apply(el));
                }
                if let Some(body) = child {
                    list.extend(take_packed(body()));
                }
                let mut frag = Element::new();
                frag.set_attr("rui_fragment", "1");
                frag.children(list)
            },
        ))
    }
}

impl crate::core::tree::IntoElementList for ToggleGroupBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

pub struct ToggleGroupItemBuilder {
    value: String,
    disabled: bool,
    children: Vec<Element>,
    chrome: SlotChrome,
}

pub fn toggle_group_item(value: impl Into<String>) -> ToggleGroupItemBuilder {
    ToggleGroupItemBuilder {
        value: value.into(),
        disabled: false,
        children: Vec::new(),
        chrome: SlotChrome::default(),
    }
}

impl ToggleGroupItemBuilder {
    crate::impl_slot_chrome_methods!();

    pub fn disabled(mut self, v: bool) -> Self {
        self.disabled = v;
        self
    }
    pub fn child(mut self, child: impl Into<Element>) -> Self {
        self.children.extend(take_packed(child.into()));
        self
    }
    pub fn text(mut self, content: impl Into<String>) -> Self {
        self.children.push(Element::new().text(content.into()));
        self
    }
}

impl From<ToggleGroupItemBuilder> for Element {
    fn from(builder: ToggleGroupItemBuilder) -> Self {
        let el = ToggleGroupItem(ToggleGroupItemProps {
            value: Some(builder.value),
            disabled: Some(builder.disabled),
            children: crate::components::base::packed_children(builder.children),
            base: None,
            ..Default::default()
        });
        builder.chrome.apply(el)
    }
}

impl crate::core::tree::IntoElementList for ToggleGroupItemBuilder {
    fn into_element_list(self) -> Vec<Element> {
        vec![self.into()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn required_click_keeps_selection() {
        assert_eq!(single_select_after_click("list", "list", true), "list");
        assert_eq!(single_select_after_click("list", "grid", true), "grid");
        assert_eq!(single_select_after_click("list", "list", false), "");
    }

    #[test]
    fn coerce_required_falls_back() {
        let vals = vec!["list".into(), "grid".into()];
        assert_eq!(coerce_required_single("list", &vals, "grid"), None);
        assert_eq!(
            coerce_required_single("", &vals, "grid"),
            Some("grid".into())
        );
        assert_eq!(
            coerce_required_single("nope", &vals, ""),
            Some("list".into())
        );
    }

    #[test]
    fn multi_select_toggles_independently() {
        let cur = vec!["bold".to_string()];
        assert_eq!(
            multi_select_after_click(&cur, "italic"),
            vec!["bold".to_string(), "italic".to_string()]
        );
        assert_eq!(multi_select_after_click(&cur, "bold"), Vec::<String>::new());
    }

    #[test]
    fn spacing_zero_is_joined_layout() {
        assert_eq!(parse_toggle_group_spacing(0), 0);
        assert_eq!(parse_toggle_group_spacing("joined"), 0);
        assert_eq!(parse_toggle_group_spacing("default"), DEFAULT_SPACING);
        assert!(ToggleGroupLayout::from("joined").is_joined());
    }

    #[test]
    fn type_multiple_parses() {
        assert!(parse_multiple_type(Some("multiple"), false));
        assert!(!parse_multiple_type(Some("single"), true));
        assert!(parse_multiple_type(None, true));
    }

    #[test]
    fn joined_marks_button_group_edges() {
        let el: Element = toggle_group("tg-join")
            .spacing(0)
            .variant("outline")
            .options(vec![("a", "A"), ("b", "B"), ("c", "C")])
            .into();
        assert_eq!(el.attrs.get(ATTR_SLOT).map(String::as_str), Some(SLOT_ROOT));
        assert_eq!(el.attrs.get("data-spacing").map(String::as_str), Some("0"));
        assert_eq!(
            el.children[0]
                .attrs
                .get("rui_button_group_edge")
                .map(String::as_str),
            Some("start")
        );
        assert_eq!(
            el.children[1]
                .attrs
                .get("rui_button_group_edge")
                .map(String::as_str),
            Some("middle")
        );
        assert_eq!(
            el.children[2]
                .attrs
                .get("rui_button_group_edge")
                .map(String::as_str),
            Some("end")
        );
        assert_eq!(
            el.children[0]
                .attrs
                .get("rui_button_group_nudge_next")
                .map(String::as_str),
            Some("x")
        );
        assert_eq!(
            el.children[1]
                .attrs
                .get("rui_button_group_nudge_next")
                .map(String::as_str),
            Some("x")
        );
        assert_eq!(
            el.children[2]
                .attrs
                .get("rui_button_group_nudge_next")
                .map(String::as_str),
            None
        );
    }

    #[test]
    fn joined_vertical_inner_corners_stay_square_on_hover() {
        use crate::components::button_group::ATTR_PAINT_NUDGE;
        use crate::core::reactivity::create_root;
        use crate::core::tree::LayoutTree;

        let (_handle, mut el): (_, Element) = create_root(|| {
            toggle_group("tg-vert-join")
                .orientation("vertical")
                .variant("outline")
                .spacing(0)
                .default_value("top")
                .options(vec![
                    ("top", "Top"),
                    ("middle", "Middle"),
                    ("bottom", "Bottom"),
                ])
                .into()
        });
        assert_eq!(el.children.len(), 3);
        let r = el.children[0].style.radii[0];
        assert!(r > 0.0, "start chip keeps outer rounds");
        assert_eq!(el.children[0].style.radii, [r, r, 0.0, 0.0]);
        assert_eq!(el.children[1].style.radii, [0.0, 0.0, 0.0, 0.0]);
        assert_eq!(el.children[2].style.radii, [0.0, 0.0, r, r]);
        assert_eq!(
            el.children[1]
                .class_set
                .as_ref()
                .and_then(|cs| cs.base_style.as_ref())
                .map(|s| s.radii),
            Some([0.0, 0.0, 0.0, 0.0]),
            "join radii must be in the class restore snapshot"
        );
        assert_eq!(
            el.children[1]
                .attrs
                .get(ATTR_PAINT_NUDGE)
                .map(String::as_str),
            Some("y")
        );
        assert_eq!(
            el.children[2]
                .attrs
                .get(ATTR_PAINT_NUDGE)
                .map(String::as_str),
            Some("y")
        );

        for i in 0..3 {
            let join = el.children[i].style.radii;
            let attrs = el.children[i].attrs.clone();
            let _ = crate::style::resolve_class_variants(
                &mut el.children[i],
                &crate::style::VariantContext {
                    hovered: true,
                    attrs: Some(&attrs),
                    ..Default::default()
                },
            );
            assert_eq!(
                el.children[i].style.radii, join,
                "item {i} inner corners must not flash rounded-md on hover restore"
            );
        }

        let mut layout = LayoutTree::new();
        let mut root = Element::new().w(200.0).h(200.0).child(el);
        let _ = layout.build_and_layout(&mut root, 200.0, 200.0, [-1.0, -1.0]);
        let group = &root.children[0];
        let a = &group.children[0];
        let b = &group.children[1];
        let c = &group.children[2];
        let seam_ab = b.layout_bounds[1] - (a.layout_bounds[1] + a.layout_bounds[3]);
        let seam_bc = c.layout_bounds[1] - (b.layout_bounds[1] + b.layout_bounds[3]);
        assert!(
            seam_ab.abs() < 0.75,
            "Top|Middle layout boxes should abut, got {seam_ab}"
        );
        assert!(
            (seam_bc - seam_ab).abs() < 0.75,
            "Middle|Bottom seam must match Top|Middle (got ab={seam_ab} bc={seam_bc})"
        );
    }

    #[test]
    fn separated_has_no_group_edge() {
        let el: Element = toggle_group("tg-sep")
            .options(vec![("a", "A"), ("b", "B")])
            .into();
        assert!(el.children[0].attrs.get("rui_button_group_edge").is_none());
        assert_eq!(el.attrs.get("data-spacing").map(String::as_str), Some("2"));
    }

    #[test]
    fn dual_surface_root_slot_matches() {
        let rsx = ToggleGroup(ToggleGroupProps {
            id: "tg-dual".into(),
            default_value: Some("left".into()),
            options: Some(vec![
                ("left".into(), "L".into()),
                ("right".into(), "R".into()),
            ]),
            variant: Some("outline".into()),
            base: None,
            ..Default::default()
        });
        let built: Element = toggle_group("tg-dual")
            .default_value("left")
            .variant("outline")
            .options(vec![("left", "L"), ("right", "R")])
            .into();
        assert_eq!(
            rsx.attrs.get(ATTR_SLOT).map(String::as_str),
            built.attrs.get(ATTR_SLOT).map(String::as_str)
        );
        assert_eq!(rsx.children.len(), built.children.len());
    }

    #[test]
    fn builder_item_style() {
        use crate::core::reactivity::create_root;
        let (_owner, el): (_, Element) = create_root(|| {
            toggle_group("tg-chrome")
                .child(|| {
                    toggle_group_item("left")
                        .style(|b| b.text_size(20.0))
                        .text("Left")
                        .into()
                })
                .into()
        });
        fn find_size(el: &Element, want: f32) -> bool {
            el.text_style.size.is_some_and(|s| (s - want).abs() < 0.01)
                || (el.text_style.size_resolved() - want).abs() < 0.01
                || el.children.iter().any(|c| find_size(c, want))
        }
        assert!(
            find_size(&el, 20.0),
            "toggle_group_item .style(|b| b.text_size(20)) must land on the chip"
        );
    }

    #[test]
    fn builder_option_style_reaches_chip() {
        use crate::core::reactivity::create_root;
        let (_owner, el): (_, Element) = create_root(|| {
            toggle_group("tg-opt-chrome")
                .options([toggle_option("left", "Left").style(|b| b.text_size(20.0))])
                .into()
        });
        fn find_size(el: &Element, want: f32) -> bool {
            el.text_style.size.is_some_and(|s| (s - want).abs() < 0.01)
                || (el.text_style.size_resolved() - want).abs() < 0.01
                || el.children.iter().any(|c| find_size(c, want))
        }
        assert!(
            find_size(&el, 20.0),
            "toggle_option .style must land on the assembled chip"
        );
    }

    #[test]
    fn toggle_group_item_hover_uses_shadcn_muted() {
        let mut el: Element = toggle_group("tg-hover")
            .default_value("a")
            .options(vec![("a", "A"), ("b", "B")])
            .into();
        let off_i = el
            .children
            .iter()
            .position(|c| c.attrs.get(ATTR_VALUE).map(String::as_str) == Some("b"))
            .expect("off chip");
        let on_i = el
            .children
            .iter()
            .position(|c| c.attrs.get(ATTR_VALUE).map(String::as_str) == Some("a"))
            .expect("on chip");
        assert!(el.children[off_i]
            .class_set
            .as_ref()
            .is_some_and(ClassSet::has_hover_color));
        let attrs = el.children[off_i].attrs.clone();
        let _ = crate::style::resolve_class_variants(
            &mut el.children[off_i],
            &crate::style::VariantContext {
                hovered: true,
                attrs: Some(&attrs),
                ..Default::default()
            },
        );
        assert_eq!(el.children[off_i].style.bg_color, parse_color("muted"));
        let mut off_fg = None;
        fn child_fg(el: &Element, acc: &mut Option<[u8; 4]>) {
            if el.text.is_some() {
                *acc = el.text_style.color;
            }
            for c in &el.children {
                child_fg(c, acc);
            }
        }
        child_fg(&el.children[off_i], &mut off_fg);
        assert_eq!(
            off_fg,
            Some(parse_text_color("foreground")),
            "off-chip hover keeps foreground, matching observed ShadCN ghost Button"
        );

        let attrs = el.children[on_i].attrs.clone();
        let _ = crate::style::resolve_class_variants(
            &mut el.children[on_i],
            &crate::style::VariantContext {
                hovered: true,
                attrs: Some(&attrs),
                ..Default::default()
            },
        );
        assert_eq!(el.children[on_i].style.bg_color, parse_color("accent"));
    }

    #[test]
    fn lucide_icon_child_keeps_font_family() {
        let mut icon = Element::new().text("\u{ea0e}").font_family("lucide");
        icon.set_attr(crate::utils::ATTR_ICON_NAME, "heart");
        let nodes = nodes_from_label_or_children("", vec![icon], [1, 2, 3, 255], 14.0);
        assert_eq!(nodes.len(), 1);
        assert_eq!(
            nodes[0].text_style.font_family.as_deref(),
            Some("lucide"),
            "PUA glyph must keep the lucide face"
        );
        assert_eq!(
            nodes[0]
                .attrs
                .get(crate::utils::ATTR_ICON_NAME)
                .map(String::as_str),
            Some("heart")
        );
        assert_eq!(nodes[0].text_style.color, Some([1, 2, 3, 255]));

        let plain = Element::new().text("Hello");
        let nodes = nodes_from_label_or_children("", vec![plain], [0, 0, 0, 255], 14.0);
        assert_eq!(nodes[0].text.as_deref(), Some("Hello"));
        assert!(
            nodes[0].text_style.font_family.is_none(),
            "plain caption still uses the chip typeface"
        );
    }

    #[test]
    fn icon_only_item_is_square_like_icon_button() {
        use taffy::style::Dimension;

        let el: Element = toggle_group("tg-icon-sq")
            .size("sm")
            .variant("outline")
            .child(|| {
                let mut icon = Element::new().text("\u{ea0e}").font_family("lucide");
                icon.set_attr(crate::utils::ATTR_ICON_NAME, "heart");
                toggle_group_item("heart").child(icon).into()
            })
            .into();
        let chip = &el.children[0];
        let side = ToggleGroupSize::Sm.min_px();
        assert_eq!(chip.style.size.width, Dimension::length(side));
        assert_eq!(chip.style.size.height, Dimension::length(side));
        assert_eq!(chip.style.padding.left, chip.style.padding.right);
        assert_eq!(chip.style.padding.top, chip.style.padding.bottom);
        assert_eq!(chip.style.padding.left, chip.style.padding.top);

        let text: Element = toggle_group("tg-text-rect")
            .size("sm")
            .item("a", "snappy")
            .into();
        assert!(
            text.children[0].style.size.width == Dimension::auto(),
            "caption chips stay content-width, not a forced square"
        );
        assert_eq!(text.children[0].style.size.height, Dimension::length(side));
    }
}
