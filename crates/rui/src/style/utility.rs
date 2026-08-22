//! Runtime utility-class application — single apply path for static `ui!` classes and
//! dynamic `bind_class` / `bind_style` updates.
//!
//! Token recognition lives in [`rui_style_tokens`] so the macro can warn on unknown
//! classes against the same catalog.

use rui_style_tokens::{
    classify_utility, parse_class_list, KnownUtility, ParsedClassList, VariantSelector,
};

use std::cell::Cell;

use crate::core::tree::{Element, LayoutStatus};
use crate::layout::cols_equal_fr;
use crate::primitives::interactive::mark_hover_paint_dirty;
use crate::style::{parse_color, parse_text_color, Style, TextStyle};

thread_local! {
    static FRAME_DT: Cell<f32> = const { Cell::new(1.0 / 60.0) };
}

/// Set the frame delta-time (seconds) for callers that still read [`frame_dt`].
///
/// CSS `transition-*` progress uses a **monotonic** timeline (`Instant`), not this `dt`.
/// Do not floor near-zero values to invent time for other chrome either.
pub fn set_frame_dt(dt: f32) {
    FRAME_DT.with(|c| c.set(dt.max(0.0)));
}

fn frame_dt() -> f32 {
    FRAME_DT.with(|c| c.get())
}

/// Whether `after` differs from `before` in fields that feed Taffy / flex geometry.
///
/// Paint-only transform/opacity/color changes must not call `update_taffy_node_style`
/// (taffy `set_style` marks dirty even when layout props are identical).
fn style_needs_taffy_sync(before: &Style, after: &Style) -> bool {
    let mut a = before.clone();
    let mut b = after.clone();
    // Normalize paint-only fields, then compare the rest.
    for s in [&mut a, &mut b] {
        s.bg_color = [0.0; 4];
        s.border_color = [0.0; 4];
        s.outline_color = [0.0; 4];
        s.outline_width = 0.0;
        s.outline_offset = 0.0;
        s.shadow_color = [0.0; 4];
        s.shadow_offset = [0.0; 2];
        s.shadow_blur = 0.0;
        s.shadow_spread = 0.0;
        s.shadow_inset = false;
        s.extra_shadows.clear();
        s.bg_gradient = None;
        s.opacity = 1.0;
        s.translate = [0.0; 2];
        s.translate_pct = [0.0; 2];
        s.rotate_deg = 0.0;
        s.scale = [1.0; 2];
        s.scale_z = 1.0;
        s.transform = None;
        s.transform_origin = Default::default();
        s.filter = None;
        s.transition.clear();
    }
    a != b
}

/// Snapshot + variant rules attached to an [`Element`] after [`apply_class_list`].
#[derive(Clone, Debug, Default)]
pub struct ClassSet {
    pub base: Vec<String>,
    /// Each entry: all selectors must match (AND), then apply the utility.
    pub variants: Vec<(Vec<VariantSelector>, String)>,
    pub base_style: Option<Style>,
    pub base_text_style: Option<TextStyle>,
    /// Last resolved interaction fingerprint; skip `Style` clone when unchanged.
    last_ctx_fp: Option<u64>,
    /// Last `:hover` geometry sample from [`sync_class_variants_subtree`].
    /// CursorMoved uses this via [`class_variant_hover_needs_frame`].
    last_pointer_hover: Option<bool>,
}

/// Interaction / theme context used to resolve variant utilities each frame.
#[derive(Clone, Debug)]
pub struct VariantContext<'a> {
    pub hovered: bool,
    pub focused: bool,
    pub disabled: bool,
    pub active: bool,
    pub dark: bool,
    /// Logical viewport width (for `sm:` / `md:` / … breakpoints).
    pub viewport_w: f32,
    /// Ancestor `group` / `group/name` markers that are currently hovered.
    /// `None` name entry means the anonymous `group` class.
    pub group_hovered: &'a [Option<&'a str>],
    /// Previous sibling `peer` / `peer/name` markers that are currently hovered.
    pub peer_hovered: &'a [Option<&'a str>],
    /// Raw element attrs (for `data-[state=…]`).
    pub attrs: Option<&'a std::collections::HashMap<String, String>>,
}

impl Default for VariantContext<'static> {
    fn default() -> Self {
        Self {
            hovered: false,
            focused: false,
            disabled: false,
            active: false,
            dark: false,
            viewport_w: 0.0,
            group_hovered: &[],
            peer_hovered: &[],
            attrs: None,
        }
    }
}

impl VariantContext<'_> {
    fn group_matches(&self, name: &Option<String>) -> bool {
        match name {
            None => self.group_hovered.iter().any(|n| n.is_none()),
            Some(want) => self
                .group_hovered
                .iter()
                .any(|n| n.is_some_and(|have| have == want.as_str())),
        }
    }

    fn peer_matches(&self, name: &Option<String>) -> bool {
        match name {
            None => self.peer_hovered.iter().any(|n| n.is_none()),
            Some(want) => self
                .peer_hovered
                .iter()
                .any(|n| n.is_some_and(|have| have == want.as_str())),
        }
    }

    fn matches_selector(&self, sel: &VariantSelector) -> bool {
        match sel {
            VariantSelector::Hover => self.hovered,
            VariantSelector::Focus | VariantSelector::FocusVisible => self.focused,
            VariantSelector::Active => self.active,
            VariantSelector::Disabled => self.disabled,
            VariantSelector::Dark => self.dark,
            VariantSelector::Breakpoint { min_width } => self.viewport_w >= *min_width as f32,
            VariantSelector::GroupHover { name } => self.group_matches(name),
            VariantSelector::PeerHover { name } => self.peer_matches(name),
            VariantSelector::Data { key, value } => {
                let Some(attrs) = self.attrs else {
                    return false;
                };
                let data_key = format!("data-{key}");
                attrs
                    .get(&data_key)
                    .or_else(|| attrs.get(key))
                    .is_some_and(|v| v == value)
            }
        }
    }
}

/// Apply a whitespace-separated utility / variant class list onto `el`.
///
/// Base tokens are applied immediately and snapshotted. Variant tokens are stored on
/// [`Element::class_set`] and re-applied by [`resolve_class_variants`].
pub fn apply_class_list(el: &mut Element, classes: &str) -> LayoutStatus {
    let parsed = parse_class_list(classes);
    apply_parsed_class_list(el, parsed, classes)
}

/// Backward-compatible alias used by [`crate::app::reactive::bind_class`].
#[inline]
pub fn apply_utility_classes(el: &mut Element, classes: &str) -> LayoutStatus {
    apply_class_list(el, classes)
}

/// Builder-friendly: apply classes and return the element.
pub fn with_class_list(mut el: Element, classes: &str) -> Element {
    let _ = apply_class_list(&mut el, classes);
    el
}

fn apply_parsed_class_list(
    el: &mut Element,
    parsed: ParsedClassList,
    raw_classes: &str,
) -> LayoutStatus {
    let mut impact = LayoutStatus::Clean;
    for token in &parsed.base {
        impact = impact.merge(apply_known_utility(el, token));
    }

    let class_set = ClassSet {
        base: parsed.base,
        variants: parsed.variants,
        base_style: Some(el.style.clone()),
        base_text_style: Some(el.text_style.clone()),
        last_ctx_fp: None,
        last_pointer_hover: None,
    };
    el.class_set = Some(class_set);
    el.attrs
        .insert("rui_utility_class".to_string(), raw_classes.to_string());
    impact
}

/// Restore base snapshot and apply currently-matching variant utilities.
pub fn resolve_class_variants(el: &mut Element, ctx: &VariantContext<'_>) -> LayoutStatus {
    let Some(cs) = el.class_set.as_ref() else {
        return LayoutStatus::Clean;
    };
    if cs.variants.is_empty() {
        return LayoutStatus::Clean;
    }

    if let Some(s) = cs.base_style.as_ref() {
        el.style = s.clone();
    }
    if let Some(t) = cs.base_text_style.as_ref() {
        el.text_style = t.clone();
    }

    let mut impact = LayoutStatus::Clean;
    // Collect matching utilities first to avoid borrow issues.
    let mut apply: Vec<String> = Vec::new();
    if let Some(cs) = el.class_set.as_ref() {
        for (sels, util) in &cs.variants {
            if sels.iter().all(|s| ctx.matches_selector(s)) {
                apply.push(util.clone());
            }
        }
    }
    for util in apply {
        impact = impact.merge(apply_known_utility(el, &util));
    }
    impact
}

fn mix_fp(h: &mut u64, x: u64) {
    *h ^= x;
    *h = h.wrapping_mul(0x0000_0100_0000_01b3);
}

fn str_fp(s: &str) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325_u64;
    for b in s.as_bytes() {
        mix_fp(&mut h, *b as u64);
    }
    h
}

fn variant_ctx_fingerprint(
    hovered: bool,
    focused: bool,
    disabled: bool,
    dark: bool,
    viewport_w: f32,
    group_hovered: &[Option<&str>],
    peer_hovered: &[Option<&str>],
    attrs: &std::collections::HashMap<String, String>,
) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325_u64;
    mix_fp(&mut h, hovered as u64);
    mix_fp(&mut h, focused as u64 + 2);
    mix_fp(&mut h, disabled as u64 + 4);
    mix_fp(&mut h, dark as u64 + 8);
    mix_fp(&mut h, viewport_w.to_bits() as u64);
    for g in group_hovered {
        mix_fp(&mut h, 0x11);
        if let Some(s) = g {
            mix_fp(&mut h, str_fp(s));
        }
    }
    for p in peer_hovered {
        mix_fp(&mut h, 0x22);
        if let Some(s) = p {
            mix_fp(&mut h, str_fp(s));
        }
    }
    let mut attrs_h = 0u64;
    for (k, v) in attrs {
        if k.starts_with("data-") || k == "disabled" || k == "aria-disabled" {
            attrs_h ^= str_fp(k).wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ str_fp(v);
        }
    }
    mix_fp(&mut h, attrs_h);
    if h == 0 {
        1
    } else {
        h
    }
}

/// Walk the tree and resolve class variants (chrome sync).
pub fn sync_class_variants_subtree(
    el: &mut Element,
    mouse_pos: [f32; 2],
    focused_id: &str,
    dark: bool,
    viewport_w: f32,
    viewport_h: f32,
    layout_tree: &mut crate::layout::layout_tree::LayoutTree,
    impact: &mut LayoutStatus,
) {
    sync_class_variants_subtree_inner(
        el,
        mouse_pos,
        focused_id,
        dark,
        viewport_w,
        viewport_h,
        &[],
        &[],
        [0.0_f32, 0.0],
        None,
        layout_tree,
        impact,
    );
}

/// During smooth-scroll / scrollbar-fade frames: advance already-running CSS tweens only.
///
/// Do **not** re-sample pointer `:hover`. Content sliding under a still cursor would otherwise
/// enter/leave every `transition-transform` tile on the gallery layout page, spawning a tween
/// storm (`mark_hover_paint_dirty` + full redraw pump) that janks hard and flickers sidebar hover.
pub fn tick_active_css_transitions_subtree(
    el: &mut Element,
    viewport_w: f32,
    viewport_h: f32,
    scroll_acc: [f32; 2],
    layout_tree: &mut crate::layout::layout_tree::LayoutTree,
    impact: &mut LayoutStatus,
) {
    let is_portal =
        el.template_id.as_deref() == Some(crate::layout::layout_tree::PORTAL_LAYER_ID);
    let scroll_acc = if el.style.position_fixed || is_portal {
        [0.0_f32, 0.0]
    } else {
        scroll_acc
    };

    if crate::core::tree::element_is_offscreen(el, scroll_acc, viewport_w, viewport_h) {
        return;
    }

    let has_spin = el.attrs.get("rui_animate").map(|v| v.as_str()) == Some("spin");
    let has_active_css = el.transition_snapshot.has_active_tweens();
    let css_needs = super::animation::css_transition_needs_frame(&el.style, &el.transition_snapshot);

    if has_spin || has_active_css || css_needs {
        let before = el.style.clone();
        let spun = tick_animate_spin(el);
        let css_trans = if has_active_css || css_needs || !el.style.transition.is_empty() {
            super::animation::tick_element_transitions(
                &mut el.style,
                &mut el.transition_snapshot,
                frame_dt(),
            )
        } else {
            false
        };
        if spun || css_trans || el.style != before {
            *impact = impact.merge(LayoutStatus::StyleChanged);
            mark_hover_paint_dirty();
            let need_taffy = style_needs_taffy_sync(&before, &el.style);
            if need_taffy && el.layout_node.is_some() {
                layout_tree.update_taffy_node_style(el);
            }
        }
    }

    let child_acc = if is_portal {
        scroll_acc
    } else {
        crate::core::tree::child_scroll_accumulation(el, scroll_acc)
    };
    for child in &mut el.children {
        tick_active_css_transitions_subtree(
            child,
            viewport_w,
            viewport_h,
            child_acc,
            layout_tree,
            impact,
        );
    }
}

/// True when pointer geometry would change any utility-class `:hover` sample.
///
/// Gallery `hover:scale-*` Rects have no `on_hover`, so CursorMoved alone does not
/// `request_redraw`. Without this wake, enter waits for an unrelated frame (scrollbar
/// auto-hide) and leave sticks until another chrome hover forces a redraw.
pub fn class_variant_hover_needs_frame(el: &Element, mouse_pos: [f32; 2]) -> bool {
    class_variant_hover_needs_frame_inner(el, mouse_pos, [0.0, 0.0], None)
}

fn class_variant_hover_needs_frame_inner(
    el: &Element,
    mouse_pos: [f32; 2],
    scroll_acc: [f32; 2],
    clip: Option<[f32; 4]>,
) -> bool {
    let is_portal = el.template_id.as_deref() == Some(crate::layout::layout_tree::PORTAL_LAYER_ID);
    let (scroll_acc, clip) = if el.style.position_fixed || is_portal {
        ([0.0_f32, 0.0], None)
    } else {
        (scroll_acc, clip)
    };

    let b = el.layout_bounds;
    let lx = b[0] - scroll_acc[0] + el.sticky_visual_offset[0];
    let ly = b[1] - scroll_acc[1] + el.sticky_visual_offset[1];
    let layout_box = [lx, ly, b[2], b[3]];
    let vis_box = crate::core::tree::visual_paint_aabb(el, scroll_acc);
    let in_box = crate::core::tree::point_in_aabb(mouse_pos, vis_box);
    let in_clip = clip.map_or(true, |c| {
        c[2] > 0.0 && c[3] > 0.0 && crate::core::tree::point_in_aabb(mouse_pos, c)
    });
    let inside = in_box && in_clip;

    if let Some(cs) = el.class_set.as_ref() {
        if !cs.variants.is_empty() {
            match cs.last_pointer_hover {
                Some(prev) if prev == inside => {}
                Some(_) => return true,
                None if inside => return true,
                None => {}
            }
        }
    }

    let child_clip = if is_portal {
        None
    } else if node_clips_pointer(el) {
        intersect_clip(clip, layout_box)
    } else {
        clip
    };
    let child_scroll_acc = if is_portal {
        scroll_acc
    } else {
        crate::core::tree::child_scroll_accumulation(el, scroll_acc)
    };
    for child in &el.children {
        if class_variant_hover_needs_frame_inner(child, mouse_pos, child_scroll_acc, child_clip) {
            return true;
        }
    }
    false
}

fn intersect_clip(clip: Option<[f32; 4]>, rect: [f32; 4]) -> Option<[f32; 4]> {
    let c = match clip {
        Some(c) => c,
        None => return Some(rect),
    };
    let x1 = c[0].max(rect[0]);
    let y1 = c[1].max(rect[1]);
    let x2 = (c[0] + c[2]).min(rect[0] + rect[2]);
    let y2 = (c[1] + c[3]).min(rect[1] + rect[3]);
    Some([x1, y1, (x2 - x1).max(0.0), (y2 - y1).max(0.0)])
}

fn node_clips_pointer(el: &Element) -> bool {
    crate::core::tree::element_clips_overflow(el)
}

fn sync_class_variants_subtree_inner(
    el: &mut Element,
    mouse_pos: [f32; 2],
    focused_id: &str,
    dark: bool,
    viewport_w: f32,
    viewport_h: f32,
    ancestor_group_hovered: &[Option<&str>],
    peer_hovered: &[Option<&str>],
    scroll_acc: [f32; 2],
    clip: Option<[f32; 4]>,
    layout_tree: &mut crate::layout::layout_tree::LayoutTree,
    impact: &mut LayoutStatus,
) {
    // Reset accumulated scroll for fixed-position or portal-layer elements,
    // mirroring the behaviour in pick_hover_target / hit_path.
    let is_portal = el.template_id.as_deref()
        == Some(crate::layout::layout_tree::PORTAL_LAYER_ID);
    let (scroll_acc, clip) = if el.style.position_fixed || is_portal {
        ([0.0_f32, 0.0], None)
    } else {
        (scroll_acc, clip)
    };

    if crate::core::tree::element_is_offscreen(el, scroll_acc, viewport_w, viewport_h) {
        return;
    }

    let b = el.layout_bounds;
    let vx = b[0] - scroll_acc[0] + el.sticky_visual_offset[0];
    let vy = b[1] - scroll_acc[1] + el.sticky_visual_offset[1];
    let layout_box = [vx, vy, b[2], b[3]];
    let vis_box = crate::core::tree::visual_paint_aabb(el, scroll_acc);
    // Clip `:hover` to ancestor overflow / scroll viewports (layout border box).
    // The node's own hover uses the painted AABB so `hover:scale-*` stays hovered
    // while the pointer is on the scaled ink (not the unscaled Taffy box).
    let in_box = crate::core::tree::point_in_aabb(mouse_pos, vis_box);
    let in_clip = clip.map_or(true, |c| c[2] > 0.0 && c[3] > 0.0 && crate::core::tree::point_in_aabb(mouse_pos, c));
    let inside = in_box && in_clip;

    let child_clip = if is_portal {
        None
    } else if node_clips_pointer(el) {
        intersect_clip(clip, layout_box)
    } else {
        clip
    };

    let has_variants = el
        .class_set
        .as_ref()
        .is_some_and(|cs| !cs.variants.is_empty());
    let has_spin = el.attrs.get("rui_animate").map(|v| v.as_str()) == Some("spin");
    // Only wake the per-frame resolve/tick path while a tween is actually running,
    // variants may match, or spin needs a tick.
    // Idle `transition-transform` tiles used to force resolve+taffy every frame on the
    // gallery layout page — enough work to jitter hit-test and flash sidebar hover.
    let has_active_css = el.transition_snapshot.has_active_tweens();
    let css_needs = super::animation::css_transition_needs_frame(&el.style, &el.transition_snapshot);
    if has_variants || has_spin || has_active_css || css_needs {
        let hovered = inside;
        if has_variants {
            if let Some(cs) = el.class_set.as_mut() {
                cs.last_pointer_hover = Some(hovered);
            }
        }
        let focused = el
            .focus_id
            .as_deref()
            .is_some_and(|id| !id.is_empty() && id == focused_id);
        let disabled = el.attrs.get("disabled").is_some_and(|v| v != "false")
            || el.attrs.get("aria-disabled").is_some_and(|v| v == "true");
        let ctx_fp = variant_ctx_fingerprint(
            hovered,
            focused,
            disabled,
            dark,
            viewport_w,
            ancestor_group_hovered,
            peer_hovered,
            &el.attrs,
        );
        let ctx_unchanged = el
            .class_set
            .as_ref()
            .and_then(|cs| cs.last_ctx_fp)
            .is_some_and(|fp| fp == ctx_fp);
        // Idle variants with unchanged hover/focus ctx: skip. Active CSS tweens or spin still need a tick.
        if !ctx_unchanged || has_spin || has_active_css || css_needs {
            let attrs_snapshot = el.attrs.clone();
            let ctx = VariantContext {
                hovered,
                focused,
                disabled,
                active: false,
                dark,
                viewport_w,
                group_hovered: ancestor_group_hovered,
                peer_hovered,
                attrs: Some(&attrs_snapshot),
            };
            let before = el.style.clone();
            let before_text = el.text_style.clone();
            let local = resolve_class_variants(el, &ctx);
            let spun = tick_animate_spin(el);
            let css_trans = super::animation::tick_element_transitions(
                &mut el.style,
                &mut el.transition_snapshot,
                frame_dt(),
            );
            if let Some(cs) = el.class_set.as_mut() {
                cs.last_ctx_fp = Some(ctx_fp);
            }
            if local != LayoutStatus::Clean
                || spun
                || css_trans
                || el.style != before
                || el.text_style != before_text
            {
                *impact = impact.merge(if spun || css_trans {
                    local.merge(LayoutStatus::StyleChanged)
                } else {
                    local
                });
                mark_hover_paint_dirty();
                // scale/rotate/translate/opacity are paint-only. Pushing them into Taffy every
                // transition frame dirties flex geometry and can make sidebar hit-test flicker.
                let need_taffy = matches!(
                    local,
                    LayoutStatus::LayoutChanged | LayoutStatus::StructureChanged
                ) || style_needs_taffy_sync(&before, &el.style);
                if need_taffy && el.layout_node.is_some() {
                    layout_tree.update_taffy_node_style(el);
                }
            }
        }
    }

    let mut pushed_group: Option<Option<String>> = None;
    if inside {
        if let Some(name) = el.attrs.get("rui_group") {
            if name == "1" {
                pushed_group = Some(None);
            } else if !name.is_empty() {
                pushed_group = Some(Some(name.clone()));
            }
        }
    }
    let owned_group_names: Vec<Option<String>> = if let Some(pushed) = pushed_group {
        let mut names: Vec<Option<String>> = ancestor_group_hovered
            .iter()
            .map(|n| n.map(|s| s.to_string()))
            .collect();
        names.push(pushed);
        names
    } else {
        Vec::new()
    };
    let child_group_refs_owned: Vec<Option<&str>> =
        owned_group_names.iter().map(|n| n.as_deref()).collect();
    let child_group_refs: &[Option<&str>] = if owned_group_names.is_empty() {
        ancestor_group_hovered
    } else {
        &child_group_refs_owned
    };

    let mut child_scroll_acc = scroll_acc;
    if !is_portal {
        child_scroll_acc = crate::core::tree::child_scroll_accumulation(el, scroll_acc);
    }

    let mut prev_peer_owned: Vec<Option<String>> = Vec::new();
    for child in &mut el.children {
        let peer_refs: Vec<Option<&str>> = prev_peer_owned.iter().map(|n| n.as_deref()).collect();
        sync_class_variants_subtree_inner(
            child,
            mouse_pos,
            focused_id,
            dark,
            viewport_w,
            viewport_h,
            &child_group_refs,
            &peer_refs,
            child_scroll_acc,
            child_clip,
            layout_tree,
            impact,
        );
        let child_paint = crate::core::tree::visual_paint_aabb(child, child_scroll_acc);
        let child_hovered = crate::core::tree::point_in_aabb(mouse_pos, child_paint)
            && child_clip.map_or(true, |c| {
                c[2] > 0.0 && c[3] > 0.0 && crate::core::tree::point_in_aabb(mouse_pos, c)
            });
        prev_peer_owned.clear();
        if child_hovered {
            if let Some(name) = child.attrs.get("rui_peer") {
                if name == "1" {
                    prev_peer_owned.push(None);
                } else if !name.is_empty() {
                    prev_peer_owned.push(Some(name.clone()));
                }
            }
        }
    }
}

/// Advance `rotate_deg` when `rui_animate=spin` (≈360°/s at 60fps).
fn tick_animate_spin(el: &mut Element) -> bool {
    if el.attrs.get("rui_animate").map(|v| v.as_str()) != Some("spin") {
        return false;
    }
    el.style.rotate_deg = (el.style.rotate_deg + 6.0) % 360.0;
    true
}


fn apply_text_overflow_utility(el: &mut Element, mode: crate::style::TextOverflow) -> LayoutStatus {
    let before = (
        el.text_style.overflow,
        el.text_style.white_space,
        el.style.overflow.x,
        el.style.overflow.y,
    );
    el.apply_text_overflow_mode(mode);
    let after = (
        el.text_style.overflow,
        el.text_style.white_space,
        el.style.overflow.x,
        el.style.overflow.y,
    );
    if before == after {
        LayoutStatus::Clean
    } else {
        LayoutStatus::LayoutChanged
    }
}

fn ensure_transition_entry(el: &mut Element, prop: crate::style::TransitionProperty) {
    if !el.style.transition.iter().any(|e| e.property == prop) {
        el.style.transition.push(crate::style::TransitionEntry {
            property: prop,
            duration_ms: 150.0,
            delay_ms: 0.0,
            timing: crate::style::TimingFunction::Ease,
        });
    }
}

fn apply_known_utility(el: &mut Element, token: &str) -> LayoutStatus {
    let Some(known) = classify_utility(token) else {
        return LayoutStatus::Clean;
    };
    match known {
        KnownUtility::FlexRow => {
            *el = std::mem::take(el).flex_row();
            LayoutStatus::LayoutChanged
        }
        KnownUtility::FlexCol => {
            *el = std::mem::take(el).flex_col();
            LayoutStatus::LayoutChanged
        }
        KnownUtility::FlexWrap => {
            *el = std::mem::take(el).flex_wrap_kw("wrap");
            LayoutStatus::LayoutChanged
        }
        KnownUtility::FlexNoWrap => {
            *el = std::mem::take(el).flex_wrap_kw("nowrap");
            LayoutStatus::LayoutChanged
        }
        KnownUtility::DisplayGrid => {
            *el = std::mem::take(el).display_grid();
            LayoutStatus::LayoutChanged
        }
        KnownUtility::ItemsCenter => {
            *el = std::mem::take(el).items_center();
            LayoutStatus::LayoutChanged
        }
        KnownUtility::JustifyCenter => {
            *el = std::mem::take(el).justify_center();
            LayoutStatus::LayoutChanged
        }
        KnownUtility::Grow(v) => {
            *el = std::mem::take(el).flex_grow(v);
            LayoutStatus::LayoutChanged
        }
        KnownUtility::Shrink(v) => {
            *el = std::mem::take(el).flex_shrink(v);
            LayoutStatus::LayoutChanged
        }
        KnownUtility::Overflow(v) => {
            *el = std::mem::take(el).overflow(v);
            LayoutStatus::LayoutChanged
        }
        KnownUtility::OverflowX(v) => {
            *el = std::mem::take(el).overflow_x(v);
            LayoutStatus::LayoutChanged
        }
        KnownUtility::OverflowY(v) => {
            *el = std::mem::take(el).overflow_y(v);
            LayoutStatus::LayoutChanged
        }
        KnownUtility::RoundedFull => {
            *el = std::mem::take(el).rounded_full();
            LayoutStatus::LayoutChanged
        }
        KnownUtility::Rounded(px) => {
            *el = std::mem::take(el).rounded(px);
            LayoutStatus::LayoutChanged
        }
        KnownUtility::GridCols(n) => {
            *el = std::mem::take(el)
                .display_grid()
                .grid_template_columns(cols_equal_fr(n));
            LayoutStatus::LayoutChanged
        }
        KnownUtility::GridRows(n) => {
            *el = std::mem::take(el)
                .display_grid()
                .grid_template_rows(cols_equal_fr(n));
            LayoutStatus::LayoutChanged
        }
        KnownUtility::GapX(v) => {
            *el = std::mem::take(el).gap_x(v.as_str());
            LayoutStatus::LayoutChanged
        }
        KnownUtility::GapY(v) => {
            *el = std::mem::take(el).gap_y(v.as_str());
            LayoutStatus::LayoutChanged
        }
        KnownUtility::Gap(px) => {
            *el = std::mem::take(el).gap(px);
            LayoutStatus::LayoutChanged
        }
        KnownUtility::Width(v) => {
            *el = std::mem::take(el).w(v.as_str());
            LayoutStatus::LayoutChanged
        }
        KnownUtility::Height(v) => {
            *el = std::mem::take(el).h(v.as_str());
            LayoutStatus::LayoutChanged
        }
        KnownUtility::MinWidth(v) => {
            *el = std::mem::take(el).min_w(v.as_str());
            LayoutStatus::LayoutChanged
        }
        KnownUtility::MaxWidth(v) => {
            *el = std::mem::take(el).max_w(v.as_str());
            LayoutStatus::LayoutChanged
        }
        KnownUtility::MinHeight(v) => {
            *el = std::mem::take(el).min_h(v.as_str());
            LayoutStatus::LayoutChanged
        }
        KnownUtility::MaxHeight(v) => {
            *el = std::mem::take(el).max_h(v.as_str());
            LayoutStatus::LayoutChanged
        }
        KnownUtility::Padding(v) => {
            *el = std::mem::take(el).p(v.as_str());
            LayoutStatus::LayoutChanged
        }
        KnownUtility::PaddingX(v) => {
            *el = std::mem::take(el).px(v.as_str());
            LayoutStatus::LayoutChanged
        }
        KnownUtility::PaddingY(v) => {
            *el = std::mem::take(el).py(v.as_str());
            LayoutStatus::LayoutChanged
        }
        KnownUtility::PaddingT(v) => {
            *el = std::mem::take(el).pt(v.as_str());
            LayoutStatus::LayoutChanged
        }
        KnownUtility::PaddingR(v) => {
            *el = std::mem::take(el).pr(v.as_str());
            LayoutStatus::LayoutChanged
        }
        KnownUtility::PaddingB(v) => {
            *el = std::mem::take(el).pb(v.as_str());
            LayoutStatus::LayoutChanged
        }
        KnownUtility::PaddingL(v) => {
            *el = std::mem::take(el).pl(v.as_str());
            LayoutStatus::LayoutChanged
        }
        KnownUtility::Margin(v) => {
            *el = std::mem::take(el).m(v.as_str());
            LayoutStatus::LayoutChanged
        }
        KnownUtility::MarginX(v) => {
            *el = std::mem::take(el).mx(v.as_str());
            LayoutStatus::LayoutChanged
        }
        KnownUtility::MarginY(v) => {
            *el = std::mem::take(el).my(v.as_str());
            LayoutStatus::LayoutChanged
        }
        KnownUtility::MarginT(v) => {
            *el = std::mem::take(el).mt(v.as_str());
            LayoutStatus::LayoutChanged
        }
        KnownUtility::MarginR(v) => {
            *el = std::mem::take(el).mr(v.as_str());
            LayoutStatus::LayoutChanged
        }
        KnownUtility::MarginB(v) => {
            *el = std::mem::take(el).mb(v.as_str());
            LayoutStatus::LayoutChanged
        }
        KnownUtility::MarginL(v) => {
            *el = std::mem::take(el).ml(v.as_str());
            LayoutStatus::LayoutChanged
        }
        KnownUtility::BgColor(name) => {
            let color = parse_color(name);
            if el.style.bg_color != color {
                el.style.bg_color = color;
                LayoutStatus::StyleChanged
            } else {
                LayoutStatus::Clean
            }
        }
        KnownUtility::TextColor(name) => {
            let color = parse_text_color(name);
            if el.text_style.color != color {
                el.text_style.color = color;
                LayoutStatus::StyleChanged
            } else {
                LayoutStatus::Clean
            }
        }
        KnownUtility::TextSize(px) => {
            if (el.text_style.size - px).abs() > f32::EPSILON {
                el.text_style.size = px;
                LayoutStatus::LayoutChanged
            } else {
                LayoutStatus::Clean
            }
        }
        KnownUtility::FontWeight(w) => {
            let w = w.weight_u16();
            if el.text_style.weight != w {
                el.text_style.weight = w;
                LayoutStatus::LayoutChanged
            } else {
                LayoutStatus::Clean
            }
        }
        KnownUtility::AlignSelf(kw) => {
            *el = std::mem::take(el).align_self_kw(kw);
            LayoutStatus::LayoutChanged
        }
        KnownUtility::Opacity(v) => {
            if (el.style.opacity - v).abs() > f32::EPSILON {
                el.style.opacity = v;
                LayoutStatus::StyleChanged
            } else {
                LayoutStatus::Clean
            }
        }
        KnownUtility::TranslateX(px) => {
            if (el.style.translate[0] - px).abs() > f32::EPSILON {
                el.style.translate[0] = px;
                LayoutStatus::StyleChanged
            } else {
                LayoutStatus::Clean
            }
        }
        KnownUtility::TranslateXPct(p) => {
            if (el.style.translate_pct[0] - p).abs() > f32::EPSILON {
                el.style.translate_pct[0] = p;
                LayoutStatus::StyleChanged
            } else {
                LayoutStatus::Clean
            }
        }
        KnownUtility::TranslateY(px) => {
            if (el.style.translate[1] - px).abs() > f32::EPSILON {
                el.style.translate[1] = px;
                LayoutStatus::StyleChanged
            } else {
                LayoutStatus::Clean
            }
        }
        KnownUtility::TranslateYPct(p) => {
            if (el.style.translate_pct[1] - p).abs() > f32::EPSILON {
                el.style.translate_pct[1] = p;
                LayoutStatus::StyleChanged
            } else {
                LayoutStatus::Clean
            }
        }
        KnownUtility::Truncate => {
            apply_text_overflow_utility(el, crate::style::TextOverflow::Ellipsis)
        }
        KnownUtility::TextOverflow(kw) => {
            apply_text_overflow_utility(el, crate::style::TextOverflow::parse(kw))
        }
        KnownUtility::WhitespaceNowrap => {
            use crate::style::WhiteSpace;
            if el.text_style.white_space != WhiteSpace::Nowrap {
                el.text_style.white_space = WhiteSpace::Nowrap;
                LayoutStatus::LayoutChanged
            } else {
                LayoutStatus::Clean
            }
        }

        KnownUtility::PositionFixed => {
            *el = std::mem::take(el).fixed();
            LayoutStatus::LayoutChanged
        }
        KnownUtility::PositionAbsolute => {
            *el = std::mem::take(el).absolute();
            LayoutStatus::LayoutChanged
        }
        KnownUtility::PositionRelative => {
            *el = std::mem::take(el).relative();
            LayoutStatus::LayoutChanged
        }
        KnownUtility::PositionSticky => {
            *el = std::mem::take(el).sticky();
            LayoutStatus::LayoutChanged
        }
        KnownUtility::PositionStatic => {
            *el = std::mem::take(el).static_position();
            LayoutStatus::LayoutChanged
        }
        KnownUtility::InsetAll(v) => {
            *el = std::mem::take(el).inset(v.as_str());
            LayoutStatus::LayoutChanged
        }
        KnownUtility::InsetX(v) => {
            *el = std::mem::take(el).left(v.as_str()).right(v.as_str());
            LayoutStatus::LayoutChanged
        }
        KnownUtility::InsetY(v) => {
            *el = std::mem::take(el).top(v.as_str()).bottom(v.as_str());
            LayoutStatus::LayoutChanged
        }
        KnownUtility::InsetTop(v) => {
            *el = std::mem::take(el).top(v.as_str());
            LayoutStatus::LayoutChanged
        }
        KnownUtility::InsetRight(v) => {
            *el = std::mem::take(el).right(v.as_str());
            LayoutStatus::LayoutChanged
        }
        KnownUtility::InsetBottom(v) => {
            *el = std::mem::take(el).bottom(v.as_str());
            LayoutStatus::LayoutChanged
        }
        KnownUtility::InsetLeft(v) => {
            *el = std::mem::take(el).left(v.as_str());
            LayoutStatus::LayoutChanged
        }
        KnownUtility::PointerEventsAuto => {
            *el = std::mem::take(el).pointer_events("auto");
            LayoutStatus::Clean
        }
        KnownUtility::PointerEventsNone => {
            *el = std::mem::take(el).pointer_events_none();
            LayoutStatus::Clean
        }
        KnownUtility::PointerEventsBoxNone => {
            *el = std::mem::take(el).pointer_events_box_none();
            LayoutStatus::Clean
        }
        KnownUtility::Cursor(kw) => {
            *el = std::mem::take(el).cursor(kw);
            LayoutStatus::Clean
        }
        KnownUtility::AspectRatio(r) => {
            if el.style.aspect_ratio != Some(r) {
                el.style.aspect_ratio = Some(r);
                LayoutStatus::LayoutChanged
            } else {
                LayoutStatus::Clean
            }
        }
        KnownUtility::Rotate(deg) => {
            if (el.style.rotate_deg - deg).abs() > f32::EPSILON {
                el.style.rotate_deg = deg;
                LayoutStatus::StyleChanged
            } else {
                LayoutStatus::Clean
            }
        }
        KnownUtility::Scale(s) => {
            let new = [s, s];
            if (el.style.scale[0] - new[0]).abs() > f32::EPSILON
                || (el.style.scale[1] - new[1]).abs() > f32::EPSILON
            {
                el.style.scale = new;
                LayoutStatus::StyleChanged
            } else {
                LayoutStatus::Clean
            }
        }
        KnownUtility::ScaleX(s) => {
            if (el.style.scale[0] - s).abs() > f32::EPSILON {
                el.style.scale[0] = s;
                LayoutStatus::StyleChanged
            } else {
                LayoutStatus::Clean
            }
        }
        KnownUtility::ScaleY(s) => {
            if (el.style.scale[1] - s).abs() > f32::EPSILON {
                el.style.scale[1] = s;
                LayoutStatus::StyleChanged
            } else {
                LayoutStatus::Clean
            }
        }
        KnownUtility::Zoom(z) => {
            if (el.style.zoom - z).abs() > f32::EPSILON {
                el.style.zoom = z;
                LayoutStatus::LayoutChanged
            } else {
                LayoutStatus::Clean
            }
        }
        KnownUtility::SkewX(deg) => {
            use crate::layout::dimension::CssAngle;
            use super::transform::TransformFunction;
            let func = TransformFunction::SkewX(CssAngle::Deg(deg));
            let list = el.style.transform.get_or_insert_with(Vec::new);
            // Replace existing SkewX or append
            if let Some(pos) = list.iter().position(|f| matches!(f, TransformFunction::SkewX(_))) {
                list[pos] = func;
            } else {
                list.push(func);
            }
            LayoutStatus::StyleChanged
        }
        KnownUtility::SkewY(deg) => {
            use crate::layout::dimension::CssAngle;
            use super::transform::TransformFunction;
            let func = TransformFunction::SkewY(CssAngle::Deg(deg));
            let list = el.style.transform.get_or_insert_with(Vec::new);
            if let Some(pos) = list.iter().position(|f| matches!(f, TransformFunction::SkewY(_))) {
                list[pos] = func;
            } else {
                list.push(func);
            }
            LayoutStatus::StyleChanged
        }
        KnownUtility::TransformOrigin(kw) => {
            el.style.transform_origin = super::transform::parse_transform_origin(kw);
            LayoutStatus::StyleChanged
        }
        KnownUtility::BgGradientTo(angle) => {
            let mut g = el.style.bg_gradient.unwrap_or_default();
            g.angle_deg = angle;
            el.style.bg_gradient = Some(g);
            LayoutStatus::StyleChanged
        }
        KnownUtility::GradientFrom(name) => {
            let color = parse_color(name);
            let mut g = el.style.bg_gradient.unwrap_or_default();
            g.from = color;
            el.style.bg_gradient = Some(g);
            LayoutStatus::StyleChanged
        }
        KnownUtility::GradientTo(name) => {
            let color = parse_color(name);
            let mut g = el.style.bg_gradient.unwrap_or_default();
            g.to = color;
            el.style.bg_gradient = Some(g);
            LayoutStatus::StyleChanged
        }
        KnownUtility::LetterSpacing(px) => {
            if (el.text_style.letter_spacing - px).abs() > f32::EPSILON {
                el.text_style.letter_spacing = px;
                LayoutStatus::LayoutChanged
            } else {
                LayoutStatus::Clean
            }
        }
        KnownUtility::TextDirectionLtr => {
            use crate::style::TextDirection;
            if el.text_style.direction != TextDirection::Ltr {
                el.text_style.direction = TextDirection::Ltr;
                LayoutStatus::StyleChanged
            } else {
                LayoutStatus::Clean
            }
        }
        KnownUtility::TextDirectionRtl => {
            use crate::style::TextDirection;
            if el.text_style.direction != TextDirection::Rtl {
                el.text_style.direction = TextDirection::Rtl;
                LayoutStatus::StyleChanged
            } else {
                LayoutStatus::Clean
            }
        }
        KnownUtility::TransitionOpacity => {
            ensure_transition_entry(el, crate::style::TransitionProperty::Opacity);
            LayoutStatus::StyleChanged
        }
        KnownUtility::TransitionTransform => {
            ensure_transition_entry(el, crate::style::TransitionProperty::Transform);
            LayoutStatus::StyleChanged
        }
        KnownUtility::TransitionAll => {
            ensure_transition_entry(el, crate::style::TransitionProperty::All);
            LayoutStatus::StyleChanged
        }
        KnownUtility::TransitionColors => {
            ensure_transition_entry(el, crate::style::TransitionProperty::BackgroundColor);
            ensure_transition_entry(el, crate::style::TransitionProperty::BorderColor);
            ensure_transition_entry(el, crate::style::TransitionProperty::OutlineColor);
            LayoutStatus::StyleChanged
        }
        KnownUtility::TransitionNone => {
            el.style.transition.clear();
            LayoutStatus::StyleChanged
        }
        KnownUtility::TransitionDuration(ms) => {
            let ms_f = ms as f32;
            for entry in &mut el.style.transition {
                entry.duration_ms = ms_f;
            }
            LayoutStatus::StyleChanged
        }
        KnownUtility::TransitionDelay(ms) => {
            let ms_f = ms as f32;
            for entry in &mut el.style.transition {
                entry.delay_ms = ms_f;
            }
            LayoutStatus::StyleChanged
        }
        KnownUtility::TransitionEase(name) => {
            let tf = crate::style::TimingFunction::from_css(name);
            for entry in &mut el.style.transition {
                entry.timing = tf;
            }
            LayoutStatus::StyleChanged
        }
        KnownUtility::AnimateSpin => {
            let prev = el
                .attrs
                .insert("rui_animate".to_string(), "spin".to_string());
            if prev.as_deref() == Some("spin") {
                LayoutStatus::Clean
            } else {
                LayoutStatus::StyleChanged
            }
        }
        KnownUtility::AnimateNone => {
            let prev = el
                .attrs
                .insert("rui_animate".to_string(), "none".to_string());
            if prev.as_deref() == Some("none") {
                LayoutStatus::Clean
            } else {
                LayoutStatus::StyleChanged
            }
        }
        KnownUtility::GroupMarker { name } => {
            let value = name.unwrap_or("1");
            let prev = el.attrs.insert("rui_group".to_string(), value.to_string());
            if prev.as_deref() == Some(value) {
                LayoutStatus::Clean
            } else {
                LayoutStatus::StyleChanged
            }
        }
        KnownUtility::PeerMarker { name } => {
            let value = name.unwrap_or("1");
            let prev = el.attrs.insert("rui_peer".to_string(), value.to_string());
            if prev.as_deref() == Some(value) {
                LayoutStatus::Clean
            } else {
                LayoutStatus::StyleChanged
            }
        }
        KnownUtility::BoxBorder => {
            let prev = el.style.box_sizing;
            *el = std::mem::take(el).border_box();
            if prev == taffy::style::BoxSizing::BorderBox {
                LayoutStatus::Clean
            } else {
                LayoutStatus::LayoutChanged
            }
        }
        KnownUtility::BoxContent => {
            let prev = el.style.box_sizing;
            *el = std::mem::take(el).content_box();
            if prev == taffy::style::BoxSizing::ContentBox {
                LayoutStatus::Clean
            } else {
                LayoutStatus::LayoutChanged
            }
        }
        KnownUtility::OutlineStyle(kw) => {
            use crate::style::LineStyle;
            let s = LineStyle::from_css_keyword(kw);
            if el.style.outline_style == s {
                LayoutStatus::Clean
            } else {
                el.style.outline_style = s;
                // Tailwind `outline` implies currentColor; default transparent → foreground.
                if !s.is_invisible() && el.style.outline_color == [0.0, 0.0, 0.0, 0.0] {
                    el.style.outline_color = parse_color("foreground");
                }
                LayoutStatus::StyleChanged
            }
        }
        KnownUtility::OutlineWidth(px) => {
            if (el.style.outline_width - px).abs() < f32::EPSILON {
                LayoutStatus::Clean
            } else {
                el.style.outline_width = px;
                // Ensure visible color when width is set without explicit color.
                if px > 0.0 && el.style.outline_color == [0.0, 0.0, 0.0, 0.0] {
                    el.style.outline_color = parse_color("foreground");
                }
                LayoutStatus::StyleChanged
            }
        }
        KnownUtility::OutlineOffset(px) => {
            if (el.style.outline_offset - px).abs() < f32::EPSILON {
                LayoutStatus::Clean
            } else {
                el.style.outline_offset = px;
                LayoutStatus::StyleChanged
            }
        }
        KnownUtility::OutlineColor(c) => {
            let color = parse_color(c);
            if el.style.outline_color == color {
                LayoutStatus::Clean
            } else {
                el.style.outline_color = color;
                LayoutStatus::StyleChanged
            }
        }
        KnownUtility::FilterNone => {
            if el.style.filter.is_none() {
                LayoutStatus::Clean
            } else {
                el.style.filter = None;
                LayoutStatus::StyleChanged
            }
        }
        KnownUtility::FilterBlur(px) => set_single_filter(
            el,
            crate::style::FilterFunction::Blur(Some(crate::layout::dimension::CssDim::Px(px))),
        ),
        KnownUtility::FilterBrightness(v) => {
            set_single_filter(el, crate::style::FilterFunction::Brightness(Some(v)))
        }
        KnownUtility::FilterContrast(v) => {
            set_single_filter(el, crate::style::FilterFunction::Contrast(Some(v)))
        }
        KnownUtility::FilterGrayscale(v) => {
            set_single_filter(el, crate::style::FilterFunction::Grayscale(Some(v)))
        }
        KnownUtility::FilterSepia(v) => {
            set_single_filter(el, crate::style::FilterFunction::Sepia(Some(v)))
        }
        KnownUtility::FilterInvert(v) => {
            set_single_filter(el, crate::style::FilterFunction::Invert(Some(v)))
        }
        KnownUtility::FilterSaturate(v) => {
            set_single_filter(el, crate::style::FilterFunction::Saturate(Some(v)))
        }
        KnownUtility::FilterHueRotate(deg) => set_single_filter(
            el,
            crate::style::FilterFunction::HueRotate(Some(
                crate::layout::dimension::CssAngle::Deg(deg),
            )),
        ),
        KnownUtility::FilterDropShadow(spec) => {
            let css = drop_shadow_utility_css(spec);
            let next = crate::style::parse_filter(&css);
            if el.style.filter == next {
                LayoutStatus::Clean
            } else {
                el.style.filter = next;
                LayoutStatus::StyleChanged
            }
        }
    }
}

fn set_single_filter(el: &mut Element, func: crate::style::FilterFunction) -> LayoutStatus {
    let next = Some(vec![func]);
    if el.style.filter == next {
        LayoutStatus::Clean
    } else {
        el.style.filter = next;
        LayoutStatus::StyleChanged
    }
}

/// Tailwind v3 `drop-shadow*` presets → CSS `filter` strings (hex alpha for parser support).
fn drop_shadow_utility_css(spec: &str) -> String {
    match spec {
        "" => {
            // default `drop-shadow`
            "drop-shadow(0 1px 2px #0000001a) drop-shadow(0 1px 1px #0000000f)".into()
        }
        "sm" => "drop-shadow(0 1px 1px #0000000d)".into(),
        "md" => {
            "drop-shadow(0 4px 3px #00000012) drop-shadow(0 2px 2px #0000000f)".into()
        }
        "lg" => {
            "drop-shadow(0 10px 8px #0000000a) drop-shadow(0 4px 3px #0000001a)".into()
        }
        "xl" => {
            "drop-shadow(0 20px 13px #00000008) drop-shadow(0 8px 5px #00000014)".into()
        }
        "2xl" => "drop-shadow(0 25px 25px #00000026)".into(),
        "none" => "drop-shadow(0 0 transparent)".into(),
        // Arbitrary: `drop-shadow-[0_35px_35px_#000]` or already `drop-shadow(...)`.
        other => {
            let other = other.replace('_', " ");
            if other.starts_with("drop-shadow(") {
                other
            } else {
                format!("drop-shadow({other})")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rui_style_tokens::parse_class_token;

    #[test]
    fn apply_base_and_store_variants() {
        let mut el = Element::new();
        let impact = apply_class_list(&mut el, "flex p-4 hover:bg-accent dark:bg-primary");
        assert_ne!(impact, LayoutStatus::Clean);
        let cs = el.class_set.as_ref().unwrap();
        assert!(cs.base.iter().any(|t| t == "flex"));
        assert_eq!(cs.variants.len(), 2);
    }

    #[test]
    fn resolve_hover() {
        let mut el = Element::new();
        apply_class_list(&mut el, "bg-transparent hover:bg-accent");
        let base_bg = el.style.bg_color;
        let _ = resolve_class_variants(
            &mut el,
            &VariantContext {
                hovered: true,
                ..Default::default()
            },
        );
        assert_ne!(el.style.bg_color, base_bg);
    }

    #[test]
    fn apply_opacity_tokens() {
        let mut el = Element::new();
        let impact = apply_class_list(&mut el, "opacity-50");
        assert_eq!(impact, LayoutStatus::StyleChanged);
        assert_eq!(el.style.opacity, 0.5);

        let mut el = Element::new();
        apply_class_list(&mut el, "opacity-[0.4]");
        assert_eq!(el.style.opacity, 0.4);
    }

    #[test]
    fn apply_filter_utilities() {
        use crate::style::FilterFunction;
        use crate::layout::dimension::CssDim;

        let mut el = Element::new();
        let impact = apply_class_list(&mut el, "blur-md");
        assert_eq!(impact, LayoutStatus::StyleChanged);
        assert_eq!(
            el.style.filter,
            Some(vec![FilterFunction::Blur(Some(CssDim::Px(12.0)))])
        );

        apply_class_list(&mut el, "grayscale");
        assert_eq!(
            el.style.filter,
            Some(vec![FilterFunction::Grayscale(Some(1.0))])
        );

        apply_class_list(&mut el, "filter-none");
        assert!(el.style.filter.is_none());

        apply_class_list(&mut el, "drop-shadow-sm");
        assert!(el.style.filter.is_some());
        assert!(matches!(
            el.style.filter.as_ref().unwrap().as_slice(),
            [FilterFunction::DropShadow { .. }]
        ));
    }

    #[test]
    fn apply_box_sizing_utilities() {
        let mut el = Element::new();
        assert_eq!(el.style.box_sizing, taffy::style::BoxSizing::BorderBox);
        let impact = apply_class_list(&mut el, "box-content");
        assert_eq!(impact, LayoutStatus::LayoutChanged);
        assert_eq!(el.style.box_sizing, taffy::style::BoxSizing::ContentBox);

        let impact = apply_class_list(&mut el, "box-border");
        assert_eq!(impact, LayoutStatus::LayoutChanged);
        assert_eq!(el.style.box_sizing, taffy::style::BoxSizing::BorderBox);

        let impact = apply_class_list(&mut el, "box-border");
        assert_eq!(impact, LayoutStatus::Clean);
    }

    #[test]
    fn apply_translate_and_truncate() {
        let mut el = Element::new();
        apply_class_list(&mut el, "translate-x-4 translate-y-2");
        assert_eq!(el.style.translate, [16.0, 8.0]);

        let mut el = Element::new().text("long");
        apply_class_list(&mut el, "truncate");
        assert_eq!(el.text_style.overflow, crate::style::TextOverflow::Ellipsis);
        assert_eq!(el.style.overflow.x, taffy::style::Overflow::Hidden);
        assert_eq!(el.text_style.white_space, crate::style::WhiteSpace::Nowrap);

        apply_class_list(&mut el, "text-overflow-newline");
        assert_eq!(el.text_style.overflow, crate::style::TextOverflow::Newline);
        assert_eq!(el.text_style.white_space, crate::style::WhiteSpace::Normal);
        assert_eq!(el.style.overflow.x, taffy::style::Overflow::Visible);

        apply_class_list(&mut el, "text-overflow-clip");
        assert_eq!(el.text_style.overflow, crate::style::TextOverflow::Clip);
        assert_eq!(el.text_style.white_space, crate::style::WhiteSpace::Nowrap);
        assert_eq!(el.style.overflow.x, taffy::style::Overflow::Hidden);

        apply_class_list(&mut el, "text-overflow-hidden");
        assert_eq!(el.text_style.overflow, crate::style::TextOverflow::Hidden);
        assert_eq!(el.text_style.white_space, crate::style::WhiteSpace::Nowrap);
        assert_eq!(el.style.overflow.x, taffy::style::Overflow::Hidden);
    }

    #[test]
    fn parse_stacked() {
        let t = parse_class_token("dark:hover:bg-muted");
        assert_eq!(t.utility, "bg-muted");
        assert_eq!(t.variants.len(), 2);
    }
}

#[cfg(test)]
mod p1_utility_tests {
    use super::*;

    #[test]
    fn apply_aspect_ratio_utility() {
        let mut el = Element::new();
        let impact = apply_class_list(&mut el, "aspect-square");
        assert_eq!(impact, LayoutStatus::LayoutChanged);
        assert_eq!(el.style.aspect_ratio, Some(1.0));

        let mut el = Element::new();
        apply_class_list(&mut el, "aspect-[16/9]");
        assert!((el.style.aspect_ratio.unwrap() - 16.0 / 9.0).abs() < 0.001);
    }

    #[test]
    fn apply_fixed_and_rtl() {
        let mut el = Element::new();
        apply_class_list(&mut el, "fixed");
        assert!(el.style.position_fixed);
        assert_eq!(el.style.position, taffy::style::Position::Absolute);

        let mut el = Element::new().text("x");
        apply_class_list(&mut el, "rtl");
        assert_eq!(el.text_style.direction, crate::style::TextDirection::Rtl);
    }

    #[test]
    fn apply_negative_margin() {
        let mut el = Element::new();
        let impact = apply_class_list(&mut el, "-m-4");
        assert_eq!(impact, LayoutStatus::LayoutChanged);
        assert_eq!(
            el.style.margin.left,
            taffy::style::LengthPercentageAuto::length(-16.0)
        );
        assert_eq!(
            el.style.margin.top,
            taffy::style::LengthPercentageAuto::length(-16.0)
        );
    }

    #[test]
    fn apply_transition_and_animate_attrs() {
        let mut el = Element::new();
        apply_class_list(&mut el, "transition-opacity");
        assert!(
            el.style
                .transition
                .iter()
                .any(|e| e.property == crate::style::TransitionProperty::Opacity),
            "transition-opacity must register a CSS opacity tween, got {:?}",
            el.style.transition
        );

        let mut el = Element::new();
        apply_class_list(&mut el, "animate-spin");
        assert_eq!(
            el.attrs.get("rui_animate").map(|s| s.as_str()),
            Some("spin")
        );

        let mut el = Element::new();
        apply_class_list(&mut el, "transition-transform animate-none");
        assert!(
            el.style
                .transition
                .iter()
                .any(|e| e.property == crate::style::TransitionProperty::Transform),
            "transition-transform must register a CSS transform tween, got {:?}",
            el.style.transition
        );
        assert_eq!(
            el.attrs.get("rui_animate").map(|s| s.as_str()),
            Some("none")
        );
        // Must not also set `rui_transition` — that dual-drove UI springs against the
        // CSS tween and kept gallery `hover:scale-*` tiles dirty while hovering the sidebar.
        assert!(el.attrs.get("rui_transition").is_none());
    }

    #[test]
    fn hover_scale_outside_scroll_clip_does_not_match() {
        let mut tile = Element::new();
        apply_class_list(
            &mut tile,
            "scale-100 transition-transform hover:scale-x-150",
        );
        // Stale abs at the sidebar (x≈0) while the live scroll viewport is the content column.
        tile.layout_bounds = [8.0, 80.0, 64.0, 64.0];
        let mut tree = crate::layout::layout_tree::LayoutTree::new();
        let mut impact = LayoutStatus::Clean;
        let content_clip = Some([208.0, 0.0, 600.0, 800.0]);
        sync_class_variants_subtree_inner(
            &mut tile,
            [40.0, 100.0],
            "",
            false,
            1024.0,
            800.0,
            &[],
            &[],
            [0.0, 0.0],
            content_clip,
            &mut tree,
            &mut impact,
        );
        assert!(
            (tile.style.scale[0] - 1.0).abs() < 0.01,
            "sidebar pointer must not apply hover:scale-x-150 when outside the scroll clip, scale={:?}",
            tile.style.scale
        );
        assert!(
            !tile.transition_snapshot.has_active_tweens(),
            "must not start a transform tween from a clipped false-hover"
        );
    }

    #[test]
    fn scroll_paint_freeze_does_not_start_hover_tween_from_pointer() {
        // scroll_paint_only path must not re-sample :hover — a tile under the pointer
        // should stay idle if it was never hovered via a full sync.
        crate::style::set_css_transition_virtual_timeline(true);
        crate::style::set_css_transition_timeline_ms(0.0);
        let mut tile = Element::new();
        apply_class_list(
            &mut tile,
            "scale-100 transition-transform hover:scale-x-150",
        );
        tile.layout_bounds = [400.0, 200.0, 64.0, 64.0];
        let mut tree = crate::layout::layout_tree::LayoutTree::new();
        let mut impact = LayoutStatus::Clean;
        tick_active_css_transitions_subtree(
            &mut tile,
            1024.0,
            800.0,
            [0.0, 0.0],
            &mut tree,
            &mut impact,
        );
        assert!(
            (tile.style.scale[0] - 1.0).abs() < 0.01,
            "freeze path must not apply hover:scale, got {:?}",
            tile.style.scale
        );
        assert!(
            !tile.transition_snapshot.has_active_tweens(),
            "freeze path must not start a tween from pointer geometry"
        );
        crate::style::set_css_transition_virtual_timeline(false);
    }

    #[test]
    fn scroll_paint_freeze_still_advances_active_tween() {
        crate::style::set_css_transition_virtual_timeline(true);
        crate::style::set_css_transition_timeline_ms(0.0);
        let mut tile = Element::new();
        apply_class_list(
            &mut tile,
            "scale-100 transition-transform hover:scale-x-150",
        );
        tile.layout_bounds = [400.0, 200.0, 64.0, 64.0];
        let mut tree = crate::layout::layout_tree::LayoutTree::new();
        let mut impact = LayoutStatus::Clean;
        // Seed idle tween slot at scale 1.0 (same as a prior settled frame).
        sync_class_variants_subtree(
            &mut tile,
            [10.0, 10.0],
            "",
            false,
            1024.0,
            800.0,
            &mut tree,
            &mut impact,
        );
        // Full sync: pointer on tile → retarget hover tween from 1.0 → 1.5.
        impact = LayoutStatus::Clean;
        sync_class_variants_subtree(
            &mut tile,
            [432.0, 232.0],
            "",
            false,
            1024.0,
            800.0,
            &mut tree,
            &mut impact,
        );
        assert!(
            tile.transition_snapshot.has_active_tweens(),
            "expected an active transform tween after hover enter"
        );
        let scale_after_enter = tile.style.scale[0];
        // Freeze path: advance timeline without re-resolving hover.
        crate::style::advance_css_transition_timeline_ms(80.0);
        impact = LayoutStatus::Clean;
        tick_active_css_transitions_subtree(
            &mut tile,
            1024.0,
            800.0,
            [0.0, 0.0],
            &mut tree,
            &mut impact,
        );
        assert!(
            tile.style.scale[0] > scale_after_enter + 0.01,
            "active tween must keep advancing during scroll freeze, before={scale_after_enter} after={}",
            tile.style.scale[0]
        );
        crate::style::set_css_transition_virtual_timeline(false);
    }

    #[test]
    fn idle_hover_scale_transition_settles() {
        crate::style::set_css_transition_virtual_timeline(true);
        crate::style::set_css_transition_timeline_ms(0.0);
        let mut el = Element::new();
        apply_class_list(
            &mut el,
            "scale-100 transition-transform hover:scale-x-150",
        );
        el.layout_bounds = [400.0, 200.0, 64.0, 64.0];
        let mut tree = crate::layout::layout_tree::LayoutTree::new();
        for _ in 0..20 {
            crate::style::advance_css_transition_timeline_ms(1000.0 / 60.0);
            let mut impact = LayoutStatus::Clean;
            sync_class_variants_subtree(
                &mut el,
                [10.0, 10.0],
                "",
                false,
                1024.0,
                800.0,
                &mut tree,
                &mut impact,
            );
        }
        assert!(
            !el.transition_snapshot.has_active_tweens(),
            "unhovered transition-transform tile must not keep an active tween"
        );
        assert!((el.style.scale[0] - 1.0).abs() < 0.01);
        crate::style::set_css_transition_virtual_timeline(false);
    }

    #[test]
    fn hover_scale_keeps_pointer_on_painted_aabb() {
        crate::style::set_css_transition_virtual_timeline(true);
        crate::style::set_css_transition_timeline_ms(0.0);
        let mut el = Element::new();
        apply_class_list(
            &mut el,
            "scale-100 transition-transform hover:scale-x-150",
        );
        // 64px box at x=400. After scale-x 1.5 the paint AABB starts at 384.
        el.layout_bounds = [400.0, 200.0, 64.0, 64.0];
        el.style.scale = [1.5, 1.0];
        let paint = crate::core::tree::visual_paint_aabb(&el, [0.0, 0.0]);
        assert!(
            (paint[0] - 384.0).abs() < 0.5 && (paint[2] - 96.0).abs() < 0.5,
            "scale-x 1.5 AABB should be 96px wide starting 16px left, got {paint:?}"
        );
        let mut tree = crate::layout::layout_tree::LayoutTree::new();
        // Pointer on the scaled overflow (left of the Taffy box) must still count as hover.
        for _ in 0..20 {
            crate::style::advance_css_transition_timeline_ms(1000.0 / 60.0);
            let mut impact = LayoutStatus::Clean;
            sync_class_variants_subtree(
                &mut el,
                [390.0, 232.0],
                "",
                false,
                1024.0,
                800.0,
                &mut tree,
                &mut impact,
            );
        }
        assert!(
            (el.style.scale[0] - 1.5).abs() < 0.05,
            "pointer on painted scale overflow must keep hover:scale-x-150, scale={:?}",
            el.style.scale
        );
        crate::style::set_css_transition_virtual_timeline(false);
    }

    #[test]
    fn hover_scale_leave_uses_painted_aabb_not_layout_box() {
        crate::style::set_css_transition_virtual_timeline(true);
        crate::style::set_css_transition_timeline_ms(0.0);
        let mut el = Element::new();
        apply_class_list(
            &mut el,
            "scale-100 transition-transform hover:scale-x-150",
        );
        el.layout_bounds = [400.0, 200.0, 64.0, 64.0];
        let mut tree = crate::layout::layout_tree::LayoutTree::new();
        // Enter and settle at 1.5×.
        for _ in 0..20 {
            crate::style::advance_css_transition_timeline_ms(1000.0 / 60.0);
            let mut impact = LayoutStatus::Clean;
            sync_class_variants_subtree(
                &mut el,
                [432.0, 232.0],
                "",
                false,
                1024.0,
                800.0,
                &mut tree,
                &mut impact,
            );
        }
        assert!(
            (el.style.scale[0] - 1.5).abs() < 0.05,
            "expected settled hover scale, got {:?}",
            el.style.scale
        );
        // Pointer on the left overhang of the painted 96px box (layout starts at 400).
        for _ in 0..5 {
            crate::style::advance_css_transition_timeline_ms(1000.0 / 60.0);
            let mut impact = LayoutStatus::Clean;
            sync_class_variants_subtree(
                &mut el,
                [390.0, 232.0],
                "",
                false,
                1024.0,
                800.0,
                &mut tree,
                &mut impact,
            );
        }
        assert!(
            (el.style.scale[0] - 1.5).abs() < 0.05,
            "leave must use painted AABB; overhang at x=390 must stay hovered, scale={:?}",
            el.style.scale
        );
        crate::style::set_css_transition_virtual_timeline(false);
    }

    #[test]
    fn class_variant_hover_needs_frame_on_enter_leave() {
        let mut el = Element::new();
        apply_class_list(
            &mut el,
            "scale-100 transition-transform hover:scale-x-150",
        );
        el.layout_bounds = [400.0, 200.0, 64.0, 64.0];
        assert!(
            class_variant_hover_needs_frame(&el, [432.0, 232.0]),
            "first enter over a hover: variant tile must request a frame"
        );
        let mut tree = crate::layout::layout_tree::LayoutTree::new();
        let mut impact = LayoutStatus::Clean;
        sync_class_variants_subtree(
            &mut el,
            [432.0, 232.0],
            "",
            false,
            1024.0,
            800.0,
            &mut tree,
            &mut impact,
        );
        assert!(
            !class_variant_hover_needs_frame(&el, [432.0, 232.0]),
            "steady hover must not keep waking frames"
        );
        assert!(
            class_variant_hover_needs_frame(&el, [10.0, 10.0]),
            "leave must wake a frame so hover:scale can shrink"
        );
    }

    #[test]
    fn resolve_breakpoint_sm() {
        let mut el = Element::new();
        apply_class_list(&mut el, "bg-transparent sm:bg-accent");
        let base_bg = el.style.bg_color;
        let _ = resolve_class_variants(
            &mut el,
            &VariantContext {
                viewport_w: 639.0,
                ..Default::default()
            },
        );
        assert_eq!(el.style.bg_color, base_bg);
        let _ = resolve_class_variants(
            &mut el,
            &VariantContext {
                viewport_w: 640.0,
                ..Default::default()
            },
        );
        assert_ne!(el.style.bg_color, base_bg);
    }

    #[test]
    fn group_and_peer_markers_set_attrs() {
        let mut el = Element::new();
        apply_class_list(&mut el, "group");
        assert_eq!(el.attrs.get("rui_group").map(|s| s.as_str()), Some("1"));
        let mut el = Element::new();
        apply_class_list(&mut el, "group/card peer/input");
        assert_eq!(el.attrs.get("rui_group").map(|s| s.as_str()), Some("card"));
        assert_eq!(el.attrs.get("rui_peer").map(|s| s.as_str()), Some("input"));
    }

    #[test]
    fn resolve_group_hover_with_context() {
        let mut el = Element::new();
        apply_class_list(&mut el, "bg-transparent group-hover:bg-accent");
        let base_bg = el.style.bg_color;
        let groups = [None];
        let _ = resolve_class_variants(
            &mut el,
            &VariantContext {
                group_hovered: &groups,
                ..Default::default()
            },
        );
        assert_ne!(el.style.bg_color, base_bg);
    }
}
