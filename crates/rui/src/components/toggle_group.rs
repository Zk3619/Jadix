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
//!             ("left".into(), "Left".into()),
//!             ("center".into(), "Center".into()),
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

use crate::components::button_group::{
    apply_button_group_join, apply_button_group_join_chrome, ButtonGroupOrientation,
};
use crate::components::toggle::toggle_size_pad;
use crate::core::reactivity::{
    create_context, provide_context, use_context, use_scoped_signal, use_signal, with_child_owner,
    Context, Signal,
};
use crate::core::tree::{parse_color, parse_text_color, Element};
use crate::primitives::interactive::interactive;
use crate::primitives::r#box::{hstack, vstack};
use crate::style::{control_focus_ring_active, focus_ring_color, with_class_list, FOCUS_RING_BLUR};
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

/// Toggle chrome: ShadCN `default` (borderless) / `outline`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ToggleGroupVariant {
    #[default]
    Default,
    Outline,
}

impl ToggleGroupVariant {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Outline => "outline",
        }
    }

    pub fn is_outline(self) -> bool {
        matches!(self, Self::Outline)
    }
}

impl From<&str> for ToggleGroupVariant {
    fn from(s: &str) -> Self {
        s.parse().unwrap_or_default()
    }
}

impl From<String> for ToggleGroupVariant {
    fn from(s: String) -> Self {
        Self::from(s.as_str())
    }
}

impl std::str::FromStr for ToggleGroupVariant {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.trim().to_ascii_lowercase().as_str() {
            "outline" => Self::Outline,
            _ => Self::Default,
        })
    }
}

/// Toggle size ladder (same padding as [`super::toggle`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ToggleGroupSize {
    Sm,
    #[default]
    Default,
    Lg,
}

impl ToggleGroupSize {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sm => "sm",
            Self::Default => "default",
            Self::Lg => "lg",
        }
    }

    fn min_px(self) -> f32 {
        match self {
            Self::Sm => 32.0,
            Self::Default => 36.0,
            Self::Lg => 40.0,
        }
    }
}

impl From<&str> for ToggleGroupSize {
    fn from(s: &str) -> Self {
        s.parse().unwrap_or_default()
    }
}

impl From<String> for ToggleGroupSize {
    fn from(s: String) -> Self {
        Self::from(s.as_str())
    }
}

impl std::str::FromStr for ToggleGroupSize {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.trim().to_ascii_lowercase().as_str() {
            "sm" | "small" => Self::Sm,
            "lg" | "large" => Self::Lg,
            _ => Self::Default,
        })
    }
}

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
    let (px, py, text_px) = toggle_size_pad(ctx.size.as_str());
    let min_px = ctx.size.min_px();

    let nodes = nodes_from_label_or_children(label, children, fg, text_px);
    let mut chip = hstack()
        .px(px)
        .py(py)
        .min_w(min_px)
        .min_h(min_px)
        .h(min_px)
        .items_center()
        .justify_center()
        .gap("0.25rem")
        .flex_shrink(0.0)
        .bg(bg)
        .border(border, border_w);
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
    if disabled {
        out = out.cursor_not_allowed();
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
    class: Option<String>,
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
        if let Some(c) = class.filter(|s| !s.trim().is_empty()) {
            el = with_class_list(el, &c);
        }

        let sync_single = value_single;
        let sync_multi = value_multi;
        let sync_required = required;
        let default_sync = default_value.clone();
        el = el.reactive_sync(move |root, _layout_tree| {
            if sync_required {
                let values = collect_item_values(root);
                if let Some(fixed) =
                    coerce_required_single(&sync_single.peek(), &values, &default_sync)
                {
                    // Silent: paint sync may hold AppState; avoid effect flush / reconcile.
                    sync_single.set_silent(fixed);
                }
            }
            let outline = variant.is_outline();
            sync_toggle_group_pressed(
                root,
                outline,
                disabled,
                segmented,
                multiple,
                &sync_single.peek(),
                &sync_multi.peek(),
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
        if option.style.bg_color != want_bg {
            option.style.bg_color = want_bg;
            changed = true;
        }
        if option.style.border_color != want_border {
            option.style.border_color = want_border;
            changed = true;
        }
        if option.style.border_width != border_w {
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
            let (want_shadow, want_blur) = if show_ring {
                (focus_ring_color(), FOCUS_RING_BLUR)
            } else if pressed {
                ([0.0, 0.0, 0.0, 0.08], 2.0)
            } else {
                ([0.0, 0.0, 0.0, 0.0], 0.0)
            };
            if (option.style.shadow_blur - want_blur).abs() > 0.01
                || (option.style.shadow_color[0] - want_shadow[0]).abs() > 0.001
                || (option.style.shadow_color[1] - want_shadow[1]).abs() > 0.001
                || (option.style.shadow_color[2] - want_shadow[2]).abs() > 0.001
                || (option.style.shadow_color[3] - want_shadow[3]).abs() > 0.001
            {
                option.style.shadow_color = want_shadow;
                option.style.shadow_blur = want_blur;
                option.style.shadow_offset = [0.0, 0.0];
                option.style.shadow_spread = 0.0;
                changed = true;
            }
        } else if option.attrs.contains_key("rui_button_group_edge") {
            apply_button_group_join_chrome(option);
        }
        paint_descendant_fg(option, want_fg, &mut changed);
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
    class: Option<String>,
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
        if let Some(c) = class {
            b = b.class(c);
        }
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
        class,
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

pub struct ToggleGroupBuilder {
    id: String,
    multiple: bool,
    value_single: Option<Signal<String>>,
    value_multi: Option<Signal<Vec<String>>>,
    default_value: String,
    options: Vec<(String, String)>,
    disabled: bool,
    variant: ToggleGroupVariant,
    size: ToggleGroupSize,
    orientation: ToggleGroupOrientation,
    spacing: u8,
    joined: bool,
    /// TabsList muted pill (not the public ButtonGroup-join variant).
    segmented: bool,
    required: bool,
    class: String,
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
        class: String::new(),
    }
}

impl ToggleGroupBuilder {
    pub fn multiple(mut self, v: bool) -> Self {
        self.multiple = v;
        self
    }

    /// ShadCN / Radix `type="single" | "multiple"`.
    pub fn r#type(self, t: impl AsRef<str>) -> Self {
        self.multiple(t.as_ref().trim().eq_ignore_ascii_case("multiple"))
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

    pub fn options(mut self, opts: Vec<(String, String)>) -> Self {
        self.options = opts;
        self
    }

    pub fn item(mut self, value: impl Into<String>, label: impl Into<String>) -> Self {
        self.options.push((value.into(), label.into()));
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

    pub fn class(mut self, class: impl Into<String>) -> Self {
        self.class = class.into();
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
        let class = if builder.class.trim().is_empty() {
            None
        } else {
            Some(builder.class)
        };
        let options = builder.options;
        toggle_group_scope(
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
            class,
            move || {
                let ctx = require_toggle_group("toggle_group options");
                let mut list = Vec::with_capacity(options.len());
                for (value, label) in options {
                    list.push(build_toggle_group_item(
                        &ctx,
                        &value,
                        &label,
                        Vec::new(),
                        false,
                        ctx.segmented,
                    ));
                }
                let mut frag = Element::new();
                frag.set_attr("rui_fragment", "1");
                frag.children(list)
            },
        )
    }
}

impl crate::core::tree::IntoElementList for ToggleGroupBuilder {
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
            .options(vec![
                ("a".into(), "A".into()),
                ("b".into(), "B".into()),
                ("c".into(), "C".into()),
            ])
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
    }

    #[test]
    fn separated_has_no_group_edge() {
        let el: Element = toggle_group("tg-sep")
            .options(vec![("a".into(), "A".into()), ("b".into(), "B".into())])
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
            .options(vec![
                ("left".into(), "L".into()),
                ("right".into(), "R".into()),
            ])
            .into();
        assert_eq!(
            rsx.attrs.get(ATTR_SLOT).map(String::as_str),
            built.attrs.get(ATTR_SLOT).map(String::as_str)
        );
        assert_eq!(rsx.children.len(), built.children.len());
    }
}
