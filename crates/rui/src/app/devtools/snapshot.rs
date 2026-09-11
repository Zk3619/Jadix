//! Read-only Element tree / layout / style introspection for Inspector (B1 P0).

use crate::core::tree::Element;
use crate::layout::dimension::{dim_length_px, lp_length_px, lpa_length_px};
use crate::style::Style;
use taffy::prelude::*;

/// Stable child-index path from the app root.
pub type ElementPath = Vec<usize>;

/// Box model layers in logical px (from `layout_bounds` + `Style` padding/border).
#[derive(Clone, Debug, PartialEq)]
pub struct BoxModelSnapshot {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub padding_top: f32,
    pub padding_right: f32,
    pub padding_bottom: f32,
    pub padding_left: f32,
    pub border_top: f32,
    pub border_right: f32,
    pub border_bottom: f32,
    pub border_left: f32,
    pub margin_top: f32,
    pub margin_right: f32,
    pub margin_bottom: f32,
    pub margin_left: f32,
    /// Resolved `top` / `right` / `bottom` / `left` for positioned elements.
    pub position_top: f32,
    pub position_right: f32,
    pub position_bottom: f32,
    pub position_left: f32,
}

/// JSON-friendly style summary (not full CSSOM).
#[derive(Clone, Debug, PartialEq)]
pub struct StyleSnapshot {
    pub display: String,
    pub flex_direction: String,
    pub width: String,
    pub height: String,
    pub min_width: String,
    pub min_height: String,
    pub padding: String,
    pub margin: String,
    pub bg: String,
    pub color_hint: String,
    pub opacity: f32,
    pub border_width: f32,
    pub border_color: String,
    pub border_style: String,
    pub border_radius: String,
    pub border_collapse: bool,
    pub border_spacing: String,
    pub border_image_source: String,
    pub corner_shape: String,
    pub flex_grow: f32,
    pub flex_shrink: f32,
    pub z_index: i32,
    pub overflow_x: String,
    pub overflow_y: String,
    pub position: String,
    pub gap: String,
}

/// One node in a depth-first flattened inspector tree.
#[derive(Clone, Debug, PartialEq)]
pub struct InspectorNodeSnapshot {
    pub path: ElementPath,
    pub depth: usize,
    pub focus_id: Option<String>,
    pub template_id: Option<String>,
    pub tag_hint: Option<String>,
    pub text_preview: Option<String>,
    pub layout_bounds: [f32; 4],
    pub scroll_offset: [f32; 2],
    pub box_model: BoxModelSnapshot,
    pub style: StyleSnapshot,
    pub child_count: usize,
}

/// Sentinel key for the tree root (`path == []`).
///
/// Must stay distinct from the empty string, which means “no selection / no hover”
/// in [`super::state::DevToolsState`] (`PATH` / `HOVER_PATH`).
pub const ROOT_PATH_KEY: &str = "root";

pub fn path_to_key(path: &[usize]) -> String {
    if path.is_empty() {
        return ROOT_PATH_KEY.to_string();
    }
    path.iter()
        .map(|i| i.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

pub fn key_to_path(key: &str) -> ElementPath {
    let key = key.trim();
    if key.is_empty() || key == ROOT_PATH_KEY {
        return Vec::new();
    }
    key.split(',')
        .filter_map(|s| s.trim().parse::<usize>().ok())
        .collect()
}

pub fn find_element_by_path<'a>(root: &'a Element, path: &[usize]) -> Option<&'a Element> {
    let mut el = root;
    for &idx in path {
        el = el.children.get(idx)?;
    }
    Some(el)
}

pub fn find_element_by_path_mut<'a>(
    root: &'a mut Element,
    path: &[usize],
) -> Option<&'a mut Element> {
    let mut el = root;
    for &idx in path {
        el = el.children.get_mut(idx)?;
    }
    Some(el)
}

/// Match `template_id`, `focus_id`, or reactive `rui_ui_template_id` (source before overwrite).
pub fn element_identity_matches(el: &Element, identity: &str) -> bool {
    if identity.is_empty() {
        return false;
    }
    let want = normalize_identity_key(identity);
    el.template_id
        .as_deref()
        .is_some_and(|t| normalize_identity_key(t) == want)
        || el
            .focus_id
            .as_deref()
            .is_some_and(|f| normalize_identity_key(f) == want)
        || el
            .attrs
            .get(crate::app::reactive::ATTR_SOURCE_TEMPLATE_ID)
            .is_some_and(|s| normalize_identity_key(s) == want)
}

fn normalize_identity_key(id: &str) -> String {
    id.replace('\\', "/").to_ascii_lowercase()
}

fn path_prefix_distance(a: &[usize], b: &[usize]) -> usize {
    let common = a.iter().zip(b.iter()).take_while(|(x, y)| x == y).count();
    a.len().saturating_sub(common) + b.len().saturating_sub(common)
}

/// DFS lookup by stable inspector identity (preferred over child-index paths).
#[allow(dead_code)]
pub fn find_element_by_identity<'a>(root: &'a Element, identity: &str) -> Option<&'a Element> {
    find_element_by_identity_near(root, identity, &[])
}

/// Prefer the match whose border-box is closest to `hint_size` (and near `hint_path`).
pub fn find_element_by_identity_near_size<'a>(
    root: &'a Element,
    identity: &str,
    hint_path: &[usize],
    relative: &[usize],
    hint_size: (f32, f32),
) -> Option<&'a Element> {
    if identity.is_empty() {
        return None;
    }
    let mut best: Option<(usize, usize, &'a Element)> = None;
    fn walk<'a>(
        el: &'a Element,
        identity: &str,
        path: &mut ElementPath,
        hint_path: &[usize],
        relative: &[usize],
        hint_size: (f32, f32),
        best: &mut Option<(usize, usize, &'a Element)>,
    ) {
        if element_identity_matches(el, identity) {
            let candidate = if relative.is_empty() {
                Some(el)
            } else {
                find_element_by_path(el, relative)
            };
            if let Some(cand) = candidate {
                if border_box_size_matches(cand, hint_size) {
                    let dist = path_prefix_distance(path, hint_path);
                    let size_pen = size_mismatch_score(cand, hint_size);
                    if best
                        .as_ref()
                        .is_none_or(|(d, s, _)| dist < *d || (dist == *d && size_pen < *s))
                    {
                        *best = Some((dist, size_pen, cand));
                    }
                } else if !relative.is_empty() {
                    // Relative child drifted; still try the identity anchor itself.
                    if border_box_size_matches(el, hint_size) {
                        let dist = path_prefix_distance(path, hint_path);
                        let size_pen = size_mismatch_score(el, hint_size);
                        if best
                            .as_ref()
                            .is_none_or(|(d, s, _)| dist < *d || (dist == *d && size_pen < *s))
                        {
                            *best = Some((dist, size_pen, el));
                        }
                    }
                }
            }
            // Shared tid (e.g. MyCard body): search descendants for the snapshot size.
            if hint_size.0 > 1.0 || hint_size.1 > 1.0 {
                if let Some(desc) = find_size_in_subtree(el, hint_size, path, hint_path) {
                    let dist = path_prefix_distance(path, hint_path);
                    let size_pen = size_mismatch_score(desc, hint_size);
                    if best
                        .as_ref()
                        .is_none_or(|(d, s, _)| dist < *d || (dist == *d && size_pen < *s))
                    {
                        *best = Some((dist, size_pen, desc));
                    }
                }
            }
        }
        for (i, child) in el.children.iter().enumerate() {
            path.push(i);
            walk(child, identity, path, hint_path, relative, hint_size, best);
            path.pop();
        }
    }
    walk(
        root,
        identity,
        &mut Vec::new(),
        hint_path,
        relative,
        hint_size,
        &mut best,
    );
    best.map(|(_, _, el)| el)
}

fn find_size_in_subtree<'a>(
    root: &'a Element,
    hint_size: (f32, f32),
    anchor_path: &[usize],
    hint_path: &[usize],
) -> Option<&'a Element> {
    let mut best: Option<(usize, usize, &'a Element)> = None;
    fn walk<'a>(
        el: &'a Element,
        path: &mut ElementPath,
        hint_size: (f32, f32),
        hint_path: &[usize],
        best: &mut Option<(usize, usize, &'a Element)>,
    ) {
        if border_box_size_matches(el, hint_size) {
            let dist = path_prefix_distance(path, hint_path);
            let size_pen = size_mismatch_score(el, hint_size);
            if best
                .as_ref()
                .is_none_or(|(d, s, _)| dist < *d || (dist == *d && size_pen < *s))
            {
                *best = Some((dist, size_pen, el));
            }
        }
        for (i, child) in el.children.iter().enumerate() {
            path.push(i);
            walk(child, path, hint_size, hint_path, best);
            path.pop();
        }
    }
    let mut path = anchor_path.to_vec();
    walk(root, &mut path, hint_size, hint_path, &mut best);
    best.map(|(_, _, el)| el)
}

/// Absolute-path fallback when identity is missing: nearest node matching snapshot size.
pub fn find_element_by_size_near<'a>(
    root: &'a Element,
    hint_size: (f32, f32),
    hint_path: &[usize],
) -> Option<&'a Element> {
    if hint_size.0 <= 1.0 && hint_size.1 <= 1.0 {
        return None;
    }
    find_size_in_subtree(root, hint_size, &[], hint_path)
}

/// Snapshot / live border-box `[w,h]` agreement (rejects Outline vs card).
pub fn border_box_size_matches(el: &Element, hint: (f32, f32)) -> bool {
    if hint.0 <= 1.0 && hint.1 <= 1.0 {
        return true;
    }
    let w = el.layout_bounds[2].max(0.0);
    let h = el.layout_bounds[3].max(0.0);
    size_close(w, hint.0) && size_close(h, hint.1)
}

fn size_close(a: f32, b: f32) -> bool {
    let tol = (b.abs() * 0.2).max(12.0);
    (a - b).abs() <= tol
}

fn size_mismatch_score(el: &Element, hint: (f32, f32)) -> usize {
    let w = el.layout_bounds[2].max(0.0);
    let h = el.layout_bounds[3].max(0.0);
    (((w - hint.0).abs() + (h - hint.1).abs()) * 10.0) as usize
}

/// Like [`find_element_by_identity`], but when several nodes share an id (e.g. `MyCard`
/// body `template_id`), prefer the match closest to `hint_path`.
pub fn find_element_by_identity_near<'a>(
    root: &'a Element,
    identity: &str,
    hint_path: &[usize],
) -> Option<&'a Element> {
    find_element_by_identity_near_size(root, identity, hint_path, &[], (0.0, 0.0))
}

/// Best stable identity string for hover / pick (`template_id`, else `focus_id`).
pub fn element_hover_identity(el: &Element) -> String {
    el.template_id
        .as_deref()
        .filter(|s| !s.is_empty())
        .or_else(|| el.focus_id.as_deref().filter(|s| !s.is_empty()))
        .unwrap_or("")
        .to_string()
}

/// Snapshot-row identity for hover (same priority as [`element_hover_identity`]).
pub fn snapshot_hover_identity(node: &InspectorNodeSnapshot) -> String {
    node.template_id
        .as_deref()
        .filter(|s| !s.is_empty())
        .or_else(|| node.focus_id.as_deref().filter(|s| !s.is_empty()))
        .unwrap_or("")
        .to_string()
}

/// Nearest stable ancestor identity + relative child path to `node`.
///
/// Anonymous labels under a `ui!` / component root keep working after absolute
/// child-index paths drift (portal hoist, scrollbar insert, reactive rebuild).
pub fn snapshot_hover_anchor(
    tree: &[InspectorNodeSnapshot],
    node: &InspectorNodeSnapshot,
) -> (String, ElementPath) {
    let self_id = snapshot_hover_identity(node);
    if !self_id.is_empty() {
        return (self_id, Vec::new());
    }
    for len in (0..node.path.len()).rev() {
        let prefix = &node.path[..len];
        if let Some(anc) = tree.iter().find(|n| n.path.as_slice() == prefix) {
            let id = snapshot_hover_identity(anc);
            if !id.is_empty() {
                return (id, node.path[len..].to_vec());
            }
        }
    }
    (String::new(), Vec::new())
}

#[allow(dead_code)]
pub fn find_element_by_template<'a>(root: &'a Element, id: &str) -> Option<&'a Element> {
    crate::core::tree::find_in_tree(root, |el| el.template_id.as_deref() == Some(id))
}

pub fn find_element_by_template_mut<'a>(
    root: &'a mut Element,
    id: &str,
) -> Option<&'a mut Element> {
    let mut path = Vec::new();
    if !find_template_path(root, id, &mut path) {
        return None;
    }
    find_element_by_path_mut(root, &path)
}

fn find_template_path(el: &Element, id: &str, acc: &mut Vec<usize>) -> bool {
    match crate::core::tree::find_path_in_tree(el, |n| n.template_id.as_deref() == Some(id)) {
        Some(path) => {
            *acc = path;
            true
        }
        None => false,
    }
}

fn length_pct_to_px(lp: LengthPercentage, _base: f32) -> f32 {
    lp_length_px(lp).unwrap_or(0.0)
}

fn length_pct_auto_to_px(lpa: LengthPercentageAuto, _base: f32) -> f32 {
    lpa_length_px(lpa).unwrap_or(0.0)
}

fn side_border(style: &Style, side: usize) -> f32 {
    style
        .border_width_sides
        .map(|s| s[side])
        .unwrap_or(style.border_width)
}

fn format_dim(d: &Dimension) -> String {
    dim_length_px(*d)
        .map(|v| format!("{v:.2}px"))
        .unwrap_or_else(|| "auto".into())
}

fn format_rgba(c: [f32; 4]) -> String {
    if c[3] <= 0.001 {
        return "transparent".into();
    }
    let r = (c[0] * 255.0).round() as u8;
    let g = (c[1] * 255.0).round() as u8;
    let b = (c[2] * 255.0).round() as u8;
    if (c[3] - 1.0).abs() < 0.01 {
        format!("#{r:02x}{g:02x}{b:02x}")
    } else {
        format!("rgba({r},{g},{b},{:.2})", c[3])
    }
}

fn format_px4(a: f32, b: f32, c: f32, d: f32) -> String {
    if (a - b).abs() < 0.05 && (b - c).abs() < 0.05 && (c - d).abs() < 0.05 {
        format!("{:.0}px", a)
    } else {
        format!("{:.0}px {:.0}px {:.0}px {:.0}px", a, b, c, d)
    }
}

fn format_lp_rect(rect: &Rect<LengthPercentage>) -> String {
    format_px4(
        length_pct_to_px(rect.top, 0.0),
        length_pct_to_px(rect.right, 0.0),
        length_pct_to_px(rect.bottom, 0.0),
        length_pct_to_px(rect.left, 0.0),
    )
}

fn format_lpa_rect(rect: &Rect<LengthPercentageAuto>) -> String {
    format_px4(
        length_pct_auto_to_px(rect.top, 0.0),
        length_pct_auto_to_px(rect.right, 0.0),
        length_pct_auto_to_px(rect.bottom, 0.0),
        length_pct_auto_to_px(rect.left, 0.0),
    )
}

pub fn style_snapshot(style: &Style) -> StyleSnapshot {
    StyleSnapshot {
        display: format!("{:?}", style.display),
        flex_direction: format!("{:?}", style.flex_direction),
        width: format_dim(&style.size.width),
        height: format_dim(&style.size.height),
        min_width: format_dim(&style.min_size.width),
        min_height: format_dim(&style.min_size.height),
        padding: format_lp_rect(&style.padding),
        margin: format_lpa_rect(&style.margin),
        bg: format_rgba(style.bg_color),
        color_hint: format_rgba(style.border_color),
        opacity: style.opacity,
        border_width: style.border_width,
        border_color: format_rgba(style.border_color),
        border_style: format!("{:?}", style.border_style),
        border_radius: format!(
            "{:.0} {:.0} {:.0} {:.0}",
            style.radii[0], style.radii[1], style.radii[2], style.radii[3]
        ),
        border_collapse: style.border_collapse,
        border_spacing: format!(
            "{:.0} {:.0}",
            style.border_spacing[0], style.border_spacing[1]
        ),
        border_image_source: style.border_image_source.clone().unwrap_or_default(),
        corner_shape: format!("{:?}", style.corner_shape),
        flex_grow: style.flex_grow,
        flex_shrink: style.flex_shrink,
        z_index: style.z_index,
        overflow_x: format!("{:?}", style.overflow.x),
        overflow_y: format!("{:?}", style.overflow.y),
        position: format!("{:?}", style.position),
        gap: {
            let gw = lp_length_px(style.gap.width).unwrap_or(0.0);
            let gh = lp_length_px(style.gap.height).unwrap_or(0.0);
            if (gw - gh).abs() < 0.05 {
                format!("{:.0}px", gw)
            } else {
                format!("{:.0}px {:.0}px", gh, gw)
            }
        },
    }
}

/// Retained [`Style`] for Inspector readouts — excludes transient hover/focus paint.
pub fn inspector_authored_style(el: &Element) -> Style {
    if let Some(cs) = el.class_set.as_ref() {
        if let Some(base) = cs.base_style.as_ref() {
            return base.clone();
        }
    }
    if let Some(desc) = el.button_hover_paint.as_ref() {
        let mut style = el.style.as_ref().clone();
        let (bg, _, border) =
            crate::components::button::button_chrome_for_variant(&desc.variant, 0.0);
        style.bg_color = bg;
        match border {
            Some(b) => {
                style.border_color = b;
                style.border_width = 1.0;
            }
            None => {
                style.border_width = 0.0;
            }
        }
        return style;
    }
    if el
        .template_id
        .as_deref()
        .is_some_and(|id| id.starts_with("dt-row-"))
    {
        let mut style = el.style.as_ref().clone();
        style.bg_color = [0.0, 0.0, 0.0, 0.0];
        return style;
    }
    el.style.as_ref().clone()
}

/// Like [`style_snapshot`] but reads authored/rest chrome, not live hover paint.
pub fn style_snapshot_for_element(el: &Element) -> StyleSnapshot {
    style_snapshot(&inspector_authored_style(el))
}

pub fn box_model_snapshot(el: &Element) -> BoxModelSnapshot {
    let b = el.layout_bounds;
    let w = b[2].max(0.0);
    let h = b[3].max(0.0);
    BoxModelSnapshot {
        x: b[0],
        y: b[1],
        width: w,
        height: h,
        padding_top: length_pct_to_px(el.style.padding.top, h),
        padding_right: length_pct_to_px(el.style.padding.right, w),
        padding_bottom: length_pct_to_px(el.style.padding.bottom, h),
        padding_left: length_pct_to_px(el.style.padding.left, w),
        border_top: side_border(&el.style, 0),
        border_right: side_border(&el.style, 1),
        border_bottom: side_border(&el.style, 2),
        border_left: side_border(&el.style, 3),
        margin_top: length_pct_auto_to_px(el.style.margin.top, h),
        margin_right: length_pct_auto_to_px(el.style.margin.right, w),
        margin_bottom: length_pct_auto_to_px(el.style.margin.bottom, h),
        margin_left: length_pct_auto_to_px(el.style.margin.left, w),
        position_top: length_pct_auto_to_px(el.style.inset.top, h),
        position_right: length_pct_auto_to_px(el.style.inset.right, w),
        position_bottom: length_pct_auto_to_px(el.style.inset.bottom, h),
        position_left: length_pct_auto_to_px(el.style.inset.left, w),
    }
}

/// Ancestor scroll accumulation along `path` (same rules as layout / hit-test).
///
/// Highlight overlays sit outside scroll viewports, so box-model stamps must use
/// **visual** coordinates (`layout_bounds − scroll_acc`), not raw abs layout.
pub fn scroll_acc_along_path(root: &Element, path: &[usize]) -> [f32; 2] {
    let mut el = root;
    let mut acc = [0.0_f32, 0.0_f32];
    for &idx in path {
        // Match `pick_element_path_acc`: fixed / portal escape ancestor scroll.
        acc = crate::app::tree_hit::escape_scroll_acc(el, acc);
        acc = crate::app::tree_hit::next_hit_scroll_acc(el, acc);
        el = match el.children.get(idx) {
            Some(c) => c,
            None => break,
        };
    }
    acc
}

/// Box model in window/visual space for Inspector highlight overlays.
pub fn box_model_snapshot_visual(el: &Element, scroll_acc: [f32; 2]) -> BoxModelSnapshot {
    let mut bm = box_model_snapshot(el);
    let tx = el.style.translate[0] + bm.width * el.style.translate_pct[0];
    let ty = el.style.translate[1] + bm.height * el.style.translate_pct[1];
    bm.x = bm.x - scroll_acc[0] + el.sticky_visual_offset[0] + tx;
    bm.y = bm.y - scroll_acc[1] + el.sticky_visual_offset[1] + ty;
    bm
}

/// Test helper: path → visual box model (production uses `overlay::highlight_box_model`).
#[cfg(test)]
pub(crate) fn box_model_for_highlight(root: &Element, path: &[usize]) -> Option<BoxModelSnapshot> {
    let el = find_element_by_path(root, path)?;
    if is_inspector_chrome(el) {
        return None;
    }
    let acc = scroll_acc_along_path(root, path);
    Some(box_model_snapshot_visual(el, acc))
}

fn tag_hint_from(el: &Element) -> Option<String> {
    el.attrs
        .get("data-slot")
        .cloned()
        .or_else(|| el.attrs.get("role").cloned())
        .or_else(|| el.class_set.as_ref().and_then(|s| s.base.first().cloned()))
}

fn text_preview_from(el: &Element) -> Option<String> {
    if let Some(t) = &el.text {
        let s = t.chars().take(48).collect::<String>();
        return Some(if t.chars().count() > 48 {
            format!("{s}…")
        } else {
            s
        });
    }
    None
}

pub fn snapshot_node(el: &Element, path: ElementPath, depth: usize) -> InspectorNodeSnapshot {
    InspectorNodeSnapshot {
        path: path.clone(),
        depth,
        focus_id: el.focus_id.clone(),
        template_id: el.template_id.clone(),
        tag_hint: tag_hint_from(el),
        text_preview: text_preview_from(el),
        layout_bounds: el.layout_bounds,
        scroll_offset: el.scroll_offset,
        box_model: box_model_snapshot(el),
        style: style_snapshot_for_element(el),
        child_count: el.children.len(),
    }
}

pub fn snapshot_tree(root: &Element) -> Vec<InspectorNodeSnapshot> {
    let mut out = Vec::new();
    collect_tree(root, Vec::new(), 0, &mut out);
    out
}

pub fn is_inspector_chrome(el: &Element) -> bool {
    el.template_id
        .as_deref()
        .is_some_and(|id| id.starts_with("__rui_devtools"))
}

fn collect_tree(
    el: &Element,
    path: ElementPath,
    depth: usize,
    out: &mut Vec<InspectorNodeSnapshot>,
) {
    let mut stack: Vec<(&Element, ElementPath, usize)> = vec![(el, path, depth)];
    while let Some((el, path, depth)) = stack.pop() {
        if is_inspector_chrome(el) {
            continue;
        }
        out.push(snapshot_node(el, path.clone(), depth));
        for (i, child) in el.children.iter().enumerate().rev() {
            let mut child_path = path.clone();
            child_path.push(i);
            stack.push((child, child_path, depth + 1));
        }
    }
}

pub fn snapshot_subtree(root: &Element, path: &[usize]) -> Option<InspectorNodeSnapshot> {
    if path.is_empty() {
        return None;
    }
    let el = find_element_by_path(root, path)?;
    Some(snapshot_node(el, path.to_vec(), path.len()))
}

/// Hit-test path in the same logical space as [`crate::app::tree_hit`].
pub fn pick_element_path(
    el: &Element,
    x: f32,
    y: f32,
    scroll_acc: [f32; 2],
) -> Option<ElementPath> {
    crate::app::tree_hit::pick_element_path(el, x, y, scroll_acc)
        .map(|path| normalize_devtools_pick(el, path))
}

/// Prefer the nearest inspectable ancestor over anonymous text-only leaves.
pub fn normalize_devtools_pick(root: &Element, mut path: ElementPath) -> ElementPath {
    loop {
        let Some(el) = find_element_by_path(root, &path) else {
            break;
        };
        if !is_anonymous_text_leaf(el) {
            break;
        }
        if path.is_empty() {
            break;
        }
        path.pop();
    }
    path
}

fn is_anonymous_text_leaf(el: &Element) -> bool {
    el.text.is_some()
        && el.children.is_empty()
        && el.focus_id.as_ref().is_none_or(|s| s.is_empty())
        && el.template_id.as_ref().is_none_or(|s| s.is_empty())
        && el.class_set.as_ref().is_none_or(|c| c.base.is_empty())
        && el.on_click.is_none()
        && !el.attrs.contains_key("data-slot")
}

/// Display label for tree rows.
pub fn node_label(node: &InspectorNodeSnapshot) -> String {
    if let Some(fid) = &node.focus_id {
        if !fid.is_empty() {
            return fid.clone();
        }
    }
    if let Some(tid) = &node.template_id {
        if !tid.is_empty() {
            return tid.clone();
        }
    }
    if let Some(tag) = &node.tag_hint {
        return tag.clone();
    }
    if let Some(text) = &node.text_preview {
        return format!("\"{text}\"");
    }
    format!("<node {}>", path_to_key(&node.path))
}

/// `file:line:col` parsed from `ui!` / CLI `template_id` (hot-reload identity).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceLocation {
    pub path: String,
    pub line: usize,
    pub column: usize,
}

impl SourceLocation {
    pub fn display(&self) -> String {
        if self.column > 0 {
            format!("{}:{}:{}", self.path, self.line, self.column)
        } else {
            format!("{}:{}", self.path, self.line)
        }
    }
}

/// Best-effort source location from a hot-reload `template_id`.
pub fn source_location_from_template_id(id: &str) -> Option<SourceLocation> {
    let loc = parse_template_id_parts(id);
    if loc.line == 0 {
        None
    } else {
        Some(loc)
    }
}

fn parse_template_id_parts(id: &str) -> SourceLocation {
    let mut parts: Vec<String> = id.split(':').map(|s| s.to_string()).collect();
    if parts.len() >= 3 {
        let head = &parts[0];
        if head.len() == 1
            && head.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
            && parts[1].starts_with('/')
        {
            let mut tail = parts.split_off(2);
            let drive = parts.remove(0);
            let path_rest = parts.remove(0);
            parts.push(format!("{drive}:{path_rest}"));
            parts.append(&mut tail);
        }
    }
    if parts.len() >= 3 {
        if let (Ok(line), Ok(column)) = (
            parts[parts.len() - 2].parse::<usize>(),
            parts[parts.len() - 1].parse::<usize>(),
        ) {
            if line > 0 {
                return SourceLocation {
                    path: parts[..parts.len() - 2].join(":"),
                    line,
                    column,
                };
            }
        }
    }
    if parts.len() >= 2 {
        if let Ok(line) = parts[parts.len() - 1].parse::<usize>() {
            if line > 0 {
                return SourceLocation {
                    path: parts[..parts.len() - 1].join(":"),
                    line,
                    column: 0,
                };
            }
        }
    }
    SourceLocation {
        path: id.to_string(),
        line: 0,
        column: 0,
    }
}

/// Reconstruct a CSS-like declaration list from a style snapshot (compat / summary seed).
///
/// Prefer [`cssom_text_from_element`] for the Inspector editor — it dumps the full
/// authored component style (layout + visual + class + text), not this summary subset.
pub fn cssom_text_from_style(st: &StyleSnapshot) -> String {
    format!(
        "display: {};\nflex-direction: {};\nposition: {};\nwidth: {};\nheight: {};\nmin-width: {};\nmin-height: {};\npadding: {};\nmargin: {};\ngap: {};\nbackground: {};\nborder-width: {}px;\nborder-color: {};\nborder-style: {};\nborder-radius: {};\nborder-collapse: {};\nborder-spacing: {};\nborder-image-source: {};\ncorner-shape: {};\nopacity: {:.2};\nflex-grow: {};\nflex-shrink: {};\nz-index: {};\noverflow-x: {};\noverflow-y: {};\n",
        css_ident(&st.display),
        css_ident(&st.flex_direction),
        css_ident(&st.position),
        st.width,
        st.height,
        st.min_width,
        st.min_height,
        st.padding,
        st.margin,
        st.gap,
        st.bg,
        st.border_width,
        st.border_color,
        css_ident(&st.border_style),
        st.border_radius,
        if st.border_collapse { "collapse" } else { "separate" },
        st.border_spacing,
        if st.border_image_source.is_empty() {
            "none".into()
        } else {
            st.border_image_source.clone()
        },
        css_ident(&st.corner_shape),
        st.opacity,
        st.flex_grow,
        st.flex_shrink,
        st.z_index,
        css_ident(&st.overflow_x),
        css_ident(&st.overflow_y),
    )
}

/// Full authored CSSOM dump for Inspector — layout, visual, class list, and text chrome.
///
/// Uses [`inspector_authored_style`] so transient hover/focus paint is not shown as
/// `background-color`. Every declaration is listed (including defaults) so the editor
/// is a complete component style checklist, not a sparse summary.
pub fn cssom_text_from_element(el: &Element) -> String {
    let style = inspector_authored_style(el);
    let mut lines: Vec<String> = Vec::with_capacity(64);
    push_decl(&mut lines, "display", &css_dbg(&style.display));
    push_decl(&mut lines, "box-sizing", &css_dbg(&style.box_sizing));
    push_decl(
        &mut lines,
        "flex-direction",
        &css_dbg(&style.flex_direction),
    );
    push_decl(&mut lines, "flex-wrap", &css_dbg(&style.flex_wrap));
    push_decl(
        &mut lines,
        "justify-content",
        &opt_css_dbg(style.justify_content.as_ref()),
    );
    push_decl(
        &mut lines,
        "align-items",
        &opt_css_dbg(style.align_items.as_ref()),
    );
    push_decl(
        &mut lines,
        "align-self",
        &opt_css_dbg(style.align_self.as_ref()),
    );
    push_decl(
        &mut lines,
        "align-content",
        &opt_css_dbg(style.align_content.as_ref()),
    );
    push_decl(
        &mut lines,
        "justify-self",
        &opt_css_dbg(style.justify_self.as_ref()),
    );
    push_decl(
        &mut lines,
        "justify-items",
        &opt_css_dbg(style.justify_items.as_ref()),
    );
    push_decl(&mut lines, "position", &position_css(&style));
    push_decl(&mut lines, "top", &format_lpa(style.inset.top));
    push_decl(&mut lines, "right", &format_lpa(style.inset.right));
    push_decl(&mut lines, "bottom", &format_lpa(style.inset.bottom));
    push_decl(&mut lines, "left", &format_lpa(style.inset.left));
    push_decl(&mut lines, "z-index", &style.z_index.to_string());
    push_decl(&mut lines, "width", &format_dim(&style.size.width));
    push_decl(&mut lines, "height", &format_dim(&style.size.height));
    push_decl(&mut lines, "min-width", &format_dim(&style.min_size.width));
    push_decl(
        &mut lines,
        "min-height",
        &format_dim(&style.min_size.height),
    );
    push_decl(&mut lines, "max-width", &format_dim(&style.max_size.width));
    push_decl(
        &mut lines,
        "max-height",
        &format_dim(&style.max_size.height),
    );
    push_decl(&mut lines, "flex-grow", &format_num(style.flex_grow));
    push_decl(&mut lines, "flex-shrink", &format_num(style.flex_shrink));
    push_decl(&mut lines, "flex-basis", &format_dim(&style.flex_basis));
    push_decl(&mut lines, "padding", &format_lp_rect(&style.padding));
    push_decl(&mut lines, "margin", &format_lpa_rect(&style.margin));
    push_decl(&mut lines, "gap", &{
        let gw = lp_length_px(style.gap.width).unwrap_or(0.0);
        let gh = lp_length_px(style.gap.height).unwrap_or(0.0);
        if (gw - gh).abs() < 0.05 {
            format!("{:.0}px", gw)
        } else {
            format!("{:.0}px {:.0}px", gh, gw)
        }
    });
    push_decl(&mut lines, "overflow-x", &overflow_css(style.overflow.x));
    push_decl(&mut lines, "overflow-y", &overflow_css(style.overflow.y));
    push_decl(&mut lines, "background", &format_rgba(style.bg_color));
    push_decl(&mut lines, "opacity", &format!("{:.2}", style.opacity));
    push_decl(
        &mut lines,
        "filter",
        &crate::style::filter_list_to_css(style.filter.as_ref()),
    );
    push_decl(&mut lines, "border-width", &border_width_css(&style));
    push_decl(
        &mut lines,
        "border-style",
        &line_style_css(style.border_style),
    );
    push_decl(&mut lines, "border-color", &format_rgba(style.border_color));
    push_decl(
        &mut lines,
        "border-radius",
        &format!(
            "{:.0}px {:.0}px {:.0}px {:.0}px",
            style.radii[0], style.radii[1], style.radii[2], style.radii[3]
        ),
    );
    push_decl(
        &mut lines,
        "border-collapse",
        if style.border_collapse {
            "collapse"
        } else {
            "separate"
        },
    );
    push_decl(
        &mut lines,
        "border-spacing",
        &format!(
            "{:.0}px {:.0}px",
            style.border_spacing[0], style.border_spacing[1]
        ),
    );
    push_decl(
        &mut lines,
        "border-image-source",
        style.border_image_source.as_deref().unwrap_or("none"),
    );
    push_decl(&mut lines, "corner-shape", &css_dbg(&style.corner_shape));
    push_decl(
        &mut lines,
        "outline-width",
        &format!("{:.0}px", style.outline_width),
    );
    push_decl(
        &mut lines,
        "outline-style",
        &line_style_css(style.outline_style),
    );
    push_decl(
        &mut lines,
        "outline-color",
        &format_rgba(style.outline_color),
    );
    push_decl(
        &mut lines,
        "outline-offset",
        &format!("{:.0}px", style.outline_offset),
    );
    push_decl(&mut lines, "box-shadow", &box_shadow_css(&style));
    push_decl(
        &mut lines,
        "translate",
        &format!("{:.1}px {:.1}px", style.translate[0], style.translate[1]),
    );
    if style.translate_pct[0].abs() > 0.0001 || style.translate_pct[1].abs() > 0.0001 {
        push_decl(
            &mut lines,
            "translate-percent",
            &format!(
                "{:.0}% {:.0}%",
                style.translate_pct[0] * 100.0,
                style.translate_pct[1] * 100.0
            ),
        );
    }
    push_decl(&mut lines, "rotate", &format!("{:.1}deg", style.rotate_deg));
    push_decl(
        &mut lines,
        "scale",
        &format!("{:.3} {:.3}", style.scale[0], style.scale[1]),
    );
    push_decl(&mut lines, "zoom", &format_num(style.zoom));
    if let Some(ar) = style.aspect_ratio {
        push_decl(&mut lines, "aspect-ratio", &format!("{ar:.4}"));
    } else {
        push_decl(&mut lines, "aspect-ratio", "auto");
    }
    let backdrop = if style.backdrop_blur > 0.01 {
        format!("blur({:.1}px)", style.backdrop_blur)
    } else {
        "none".into()
    };
    push_decl(&mut lines, "backdrop-filter", &backdrop);

    if let Some(class) = authored_class_list(el) {
        push_decl(&mut lines, "class", &class);
    }

    // Text chrome (authored base when class_set has text snapshot).
    let text = el
        .class_set
        .as_ref()
        .and_then(|cs| cs.base_text_style.as_ref())
        .unwrap_or(&el.text_style);
    push_decl(&mut lines, "color", &format_text_rgba(text.color));
    push_decl(&mut lines, "font-size", &format!("{:.0}px", text.size));
    push_decl(&mut lines, "font-weight", &text.weight.to_string());
    if let Some(family) = text.font_family.as_deref() {
        push_decl(&mut lines, "font-family", family);
    }

    let mut out = lines.join("\n");
    out.push('\n');
    out
}

fn push_decl(lines: &mut Vec<String>, prop: &str, value: &str) {
    lines.push(format!("{prop}: {value};"));
}

fn css_dbg<T: std::fmt::Debug>(v: &T) -> String {
    css_ident(&format!("{v:?}"))
}

fn opt_css_dbg<T: std::fmt::Debug>(v: Option<&T>) -> String {
    match v {
        Some(v) => css_dbg(v),
        None => "normal".into(),
    }
}

fn position_css(style: &Style) -> String {
    if style.position_fixed {
        "fixed".into()
    } else if style.position_sticky {
        "sticky".into()
    } else {
        css_dbg(&style.position)
    }
}

fn overflow_css(o: taffy::style::Overflow) -> String {
    match o {
        taffy::style::Overflow::Visible => "visible".into(),
        taffy::style::Overflow::Hidden => "hidden".into(),
        taffy::style::Overflow::Clip => "clip".into(),
        taffy::style::Overflow::Scroll => "scroll".into(),
    }
}

fn line_style_css(s: crate::style::LineStyle) -> String {
    match s {
        crate::style::LineStyle::None => "none",
        crate::style::LineStyle::Hidden => "hidden",
        crate::style::LineStyle::Solid => "solid",
        crate::style::LineStyle::Dotted => "dotted",
        crate::style::LineStyle::Dashed => "dashed",
        crate::style::LineStyle::Double => "double",
        crate::style::LineStyle::Groove => "groove",
        crate::style::LineStyle::Ridge => "ridge",
        crate::style::LineStyle::Inset => "inset",
        crate::style::LineStyle::Outset => "outset",
    }
    .into()
}

fn border_width_css(style: &Style) -> String {
    if let Some(sides) = style.border_width_sides {
        format!(
            "{:.0}px {:.0}px {:.0}px {:.0}px",
            sides[0], sides[1], sides[2], sides[3]
        )
    } else {
        format!("{:.0}px", style.border_width)
    }
}

fn box_shadow_css(style: &Style) -> String {
    let has = style.shadow_blur > 0.01
        || style.shadow_spread.abs() > 0.01
        || style.shadow_offset[0].abs() > 0.01
        || style.shadow_offset[1].abs() > 0.01
        || style.shadow_color[3] > 0.001
        || !style.extra_shadows.is_empty();
    if !has {
        return "none".into();
    }
    let inset = if style.shadow_inset { "inset " } else { "" };
    format!(
        "{}{:.0}px {:.0}px {:.0}px {:.0}px {}",
        inset,
        style.shadow_offset[0],
        style.shadow_offset[1],
        style.shadow_blur,
        style.shadow_spread,
        format_rgba(style.shadow_color)
    )
}

fn format_lpa(v: LengthPercentageAuto) -> String {
    if let Some(px) = lpa_length_px(v) {
        format!("{px:.0}px")
    } else {
        "auto".into()
    }
}

fn format_num(v: f32) -> String {
    if (v - v.round()).abs() < 0.001 {
        format!("{:.0}", v)
    } else {
        format!("{:.3}", v)
    }
}

fn format_text_rgba(c: [u8; 4]) -> String {
    if c[3] == 0 {
        return "transparent".into();
    }
    if c[3] == 255 {
        format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
    } else {
        format!(
            "rgba({},{},{},{:.2})",
            c[0],
            c[1],
            c[2],
            c[3] as f32 / 255.0
        )
    }
}

fn authored_class_list(el: &Element) -> Option<String> {
    if let Some(raw) = el.attrs.get("rui_utility_class") {
        if !raw.trim().is_empty() {
            return Some(raw.clone());
        }
    }
    let cs = el.class_set.as_ref()?;
    let mut parts = cs.base.clone();
    for (sels, util) in &cs.variants {
        let prefix = sels
            .iter()
            .map(variant_selector_css)
            .collect::<Vec<_>>()
            .join(":");
        if prefix.is_empty() {
            parts.push(util.clone());
        } else {
            parts.push(format!("{prefix}:{util}"));
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" "))
    }
}

fn variant_selector_css(sel: &rui_style_tokens::VariantSelector) -> String {
    use rui_style_tokens::VariantSelector;
    match sel {
        VariantSelector::Hover => "hover".into(),
        VariantSelector::Focus => "focus".into(),
        VariantSelector::FocusVisible => "focus-visible".into(),
        VariantSelector::Active => "active".into(),
        VariantSelector::Disabled => "disabled".into(),
        VariantSelector::Dark => "dark".into(),
        VariantSelector::MotionReduce => "motion-reduce".into(),
        VariantSelector::MotionSafe => "motion-safe".into(),
        VariantSelector::Breakpoint { min_width } => match *min_width {
            640 => "sm".into(),
            768 => "md".into(),
            1024 => "lg".into(),
            1280 => "xl".into(),
            1536 => "2xl".into(),
            w => format!("min-[{w}px]"),
        },
        VariantSelector::MaxBreakpoint { max_width } => match *max_width {
            639 => "max-sm".into(),
            767 => "max-md".into(),
            1023 => "max-lg".into(),
            1279 => "max-xl".into(),
            1535 => "max-2xl".into(),
            w => format!("max-[{w}px]"),
        },
        VariantSelector::GroupHover { name: None } => "group-hover".into(),
        VariantSelector::GroupHover { name: Some(n) } => format!("group-hover/{n}"),
        VariantSelector::PeerHover { name: None } => "peer-hover".into(),
        VariantSelector::PeerHover { name: Some(n) } => format!("peer-hover/{n}"),
        VariantSelector::Data { key, value } => {
            if value == "true" {
                format!("data-[{key}]")
            } else {
                format!("data-[{key}={value}]")
            }
        }
    }
}

fn css_ident(dbg: &str) -> String {
    dbg.trim_matches(|c| c == '"' || c == '\'')
        .to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::tree::Element;

    #[test]
    fn path_roundtrip() {
        let p = vec![0, 2, 1];
        assert_eq!(key_to_path(&path_to_key(&p)), p);
        assert_eq!(path_to_key(&[]), ROOT_PATH_KEY);
        assert_eq!(key_to_path(ROOT_PATH_KEY), Vec::<usize>::new());
        assert_eq!(key_to_path(""), Vec::<usize>::new());
    }

    #[test]
    fn snapshot_tree_depth_first() {
        let root = Element::new()
            .focus_id("root")
            .child(Element::new().focus_id("a"))
            .child(Element::new().focus_id("b").text("hello"));
        let nodes = snapshot_tree(&root);
        assert_eq!(nodes.len(), 3);
        assert_eq!(nodes[0].focus_id.as_deref(), Some("root"));
        assert_eq!(nodes[2].text_preview.as_deref(), Some("hello"));
    }

    #[test]
    fn find_by_path() {
        let root = Element::new().children(vec![
            Element::new().focus_id("a"),
            Element::new().children(vec![Element::new().focus_id("b")]),
        ]);
        let b = find_element_by_path(&root, &[1, 0]).unwrap();
        assert_eq!(b.focus_id.as_deref(), Some("b"));
    }

    #[test]
    fn snapshot_hover_anchor_uses_nearest_identity_ancestor() {
        let root = Element::new().template_id("card-root").children(vec![
            Element::new().text("title"),
            Element::new().children(vec![Element::new().text("desc")]),
        ]);
        let nodes = snapshot_tree(&root);
        let desc = nodes
            .iter()
            .find(|n| n.text_preview.as_deref() == Some("desc"))
            .expect("desc");
        let (id, rel) = snapshot_hover_anchor(&nodes, desc);
        assert_eq!(id, "card-root");
        assert_eq!(rel, vec![1, 0]);
    }

    #[test]
    fn normalize_promotes_class_parent_over_text_leaf() {
        let mut label_el = Element::new().text("Nav");
        label_el.layout_bounds = [12.0, 12.0, 40.0, 16.0];
        let mut host = Element::new();
        let _ = crate::style::apply_class_list(&mut host, "gal-nav-text");
        host.layout_bounds = [10.0, 10.0, 60.0, 20.0];
        host = host.child(label_el);
        let path = normalize_devtools_pick(&host, vec![0]);
        assert_eq!(path, Vec::<usize>::new());
    }

    #[test]
    fn pick_hits_nested_child() {
        let mut leaf = Element::new().focus_id("leaf");
        leaf.layout_bounds = [10.0, 10.0, 20.0, 20.0];
        let mut parent = Element::new().focus_id("parent").child(leaf);
        parent.layout_bounds = [0.0, 0.0, 100.0, 100.0];
        assert_eq!(
            pick_element_path(&parent, 15.0, 15.0, [0.0, 0.0]),
            Some(vec![0])
        );
    }

    #[test]
    fn pick_and_highlight_use_visual_coords_under_scroll() {
        use taffy::style::Overflow;

        let mut target = Element::new().focus_id("btn").template_id("btn");
        target.layout_bounds = [10.0, 250.0, 80.0, 40.0];

        let mut viewport = Element::new();
        viewport.focus_id = Some("scroll:main".into());
        viewport.style.overflow.y = Overflow::Scroll;
        viewport.scroll_offset = [0.0, 100.0];
        viewport.layout_bounds = [0.0, 0.0, 400.0, 200.0];
        viewport = viewport.child(target);

        // Visual y = 250 − 100 = 150
        assert_eq!(
            pick_element_path(&viewport, 40.0, 150.0, [0.0, 0.0]),
            Some(vec![0]),
            "pick must hit scrolled visual position"
        );
        assert!(
            pick_element_path(&viewport, 40.0, 250.0, [0.0, 0.0]) != Some(vec![0]),
            "abs layout y must not win after scroll"
        );

        let acc = scroll_acc_along_path(&viewport, &[0]);
        assert!(
            (acc[1] - 100.0).abs() < 0.5,
            "scroll_acc along path={acc:?}"
        );
        let bm = box_model_for_highlight(&viewport, &[0]).expect("bm");
        assert!(
            (bm.y - 150.0).abs() < 0.5,
            "highlight must stamp visual y, got {}",
            bm.y
        );

        // Translate is part of paint-space geometry (collect `render_*`).
        viewport.children[0].style.translate = [0.0, 12.0];
        let bm_tx = box_model_for_highlight(&viewport, &[0]).expect("bm+translate");
        assert!(
            (bm_tx.y - 162.0).abs() < 0.5,
            "highlight must include translate, got {}",
            bm_tx.y
        );
        assert_eq!(
            pick_element_path(&viewport, 40.0, 162.0, [0.0, 0.0]),
            Some(vec![0]),
            "pick must include translate like paint"
        );

        // `scroll:` still contributes when overflow was cleared to Visible (layout rule).
        viewport.children[0].style.translate = [0.0, 0.0];
        viewport.style.overflow.y = Overflow::Visible;
        assert_eq!(
            pick_element_path(&viewport, 40.0, 150.0, [0.0, 0.0]),
            Some(vec![0]),
            "scroll: viewport must contribute offset even if overflow is Visible"
        );
    }

    #[test]
    fn pick_and_highlight_align_under_docked_host_and_scroll() {
        use taffy::style::Overflow;

        // wrap_app shape: host (overflow hidden, squeezed) + panel slot.
        let mut target = Element::new().focus_id("card").template_id("card");
        target.layout_bounds = [20.0, 300.0, 100.0, 40.0];

        let mut viewport = Element::new();
        viewport.focus_id = Some("scroll:gallery".into());
        viewport.style.overflow.y = Overflow::Scroll;
        viewport.scroll_offset = [0.0, 120.0];
        viewport.layout_bounds = [0.0, 0.0, 680.0, 800.0];
        viewport = viewport.child(target);

        let mut host = Element::new();
        host.template_id = Some("__rui_app_host".into());
        host.style.overflow.x = Overflow::Hidden;
        host.style.overflow.y = Overflow::Hidden;
        host.layout_bounds = [0.0, 0.0, 680.0, 800.0];
        let mut hl = Element::new();
        hl.template_id = Some(crate::app::devtools::HIGHLIGHT_SLOT_ID.into());
        hl.layout_bounds = [0.0, 0.0, 680.0, 800.0];
        host = host.children(vec![viewport, hl]);

        let mut panel = Element::new();
        panel.template_id = Some("__rui_devtools_panel_slot".into());
        panel.layout_bounds = [680.0, 0.0, 320.0, 800.0];

        let mut root = Element::new().children(vec![host, panel]);
        root.layout_bounds = [0.0, 0.0, 1000.0, 800.0];

        // Visual y = 300 − 120 = 180
        let path = pick_element_path(&root, 50.0, 180.0, [0.0, 0.0]).expect("pick");
        let hit = find_element_by_path(&root, &path).expect("resolve");
        assert_eq!(hit.focus_id.as_deref(), Some("card"));
        assert!(
            !path_is_devtools_chrome_for_test(&root, &path),
            "must not pick highlight/panel chrome"
        );

        let bm = box_model_for_highlight(&root, &path).expect("bm");
        assert!(
            (bm.y - 180.0).abs() < 0.5,
            "docked+scrolled highlight must match pick visual y, got {}",
            bm.y
        );
        assert!(
            (bm.x - 20.0).abs() < 0.5,
            "highlight x must stay in squeezed host, got {}",
            bm.x
        );
    }

    fn path_is_devtools_chrome_for_test(root: &Element, path: &[usize]) -> bool {
        let mut el = root;
        if el
            .template_id
            .as_deref()
            .is_some_and(|id| id.starts_with("__rui_devtools"))
        {
            return true;
        }
        for &idx in path {
            el = match el.children.get(idx) {
                Some(c) => c,
                None => return false,
            };
            if el
                .template_id
                .as_deref()
                .is_some_and(|id| id.starts_with("__rui_devtools"))
            {
                return true;
            }
        }
        false
    }

    #[test]
    fn pick_skips_portal_layer_shell() {
        let mut leaf = Element::new().focus_id("leaf");
        leaf.layout_bounds = [10.0, 10.0, 20.0, 20.0];
        let mut content = Element::new().focus_id("content").child(leaf);
        content.layout_bounds = [0.0, 0.0, 100.0, 100.0];
        let mut portal = Element::new();
        portal.template_id = Some(crate::layout::layout_tree::PORTAL_LAYER_ID.into());
        portal.layout_bounds = [0.0, 0.0, 100.0, 100.0];
        let mut root = Element::new().children(vec![content, portal]);
        root.layout_bounds = [0.0, 0.0, 100.0, 100.0];
        assert_eq!(
            pick_element_path(&root, 15.0, 15.0, [0.0, 0.0]),
            Some(vec![0, 0]),
            "portal shell must not steal the pick"
        );
    }

    #[test]
    fn cssom_full_element_dump_lists_layout_and_text() {
        let mut el = Element::new().focus_id("btn");
        el.style.bg_color = [0.0, 0.0, 0.0, 0.0];
        el.text_style.size = Some(14.0);
        el.text_style.weight = Some(600);
        let css = cssom_text_from_element(&el);
        for needle in [
            "display:",
            "flex-direction:",
            "position:",
            "width:",
            "height:",
            "padding:",
            "margin:",
            "background:",
            "border-width:",
            "opacity:",
            "box-shadow:",
            "color:",
            "font-size:",
            "font-weight:",
            "z-index:",
            "overflow-x:",
        ] {
            assert!(css.contains(needle), "missing {needle} in:\n{css}");
        }
        assert!(css.contains("background: transparent;"));
    }

    #[test]
    fn cssom_uses_button_rest_chrome_not_hover_paint() {
        use crate::core::tree::ButtonHoverPaint;
        use crate::style::parse_color;

        let mut el = Element::new().focus_id("gal-nav-data");
        el.button_hover_paint = Some(ButtonHoverPaint {
            variant: "ghost".into(),
            disabled: false,
            hover_easing: "spring".into(),
            text_color: None,
        });
        // Live paint after hover: accent fill (what the bug was dumping into CSSOM).
        el.style.bg_color = parse_color("accent");
        el.style.border_width = 0.0;

        let live = style_snapshot(&el.style);
        assert_ne!(live.bg, "transparent");

        let authored = style_snapshot_for_element(&el);
        assert_eq!(authored.bg, "transparent");
        let css = cssom_text_from_style(&authored);
        assert!(
            css.contains("background: transparent;"),
            "cssom must dump rest chrome, got:\n{css}"
        );
    }

    #[test]
    fn cssom_prefers_class_set_base_style_over_live_hover() {
        let mut el = Element::new();
        let _ = crate::style::apply_class_list(&mut el, "bg-transparent hover:bg-accent");
        // Simulate resolved hover variant writing into live style.
        el.style.bg_color = crate::style::parse_color("accent");

        let snap = style_snapshot_for_element(&el);
        assert_eq!(snap.bg, "transparent");
    }

    #[test]
    fn box_model_reads_padding() {
        let el = Element::new().p(8.0);
        let bm = box_model_snapshot(&el);
        assert!((bm.padding_top - 8.0).abs() < 0.01);
    }

    #[test]
    fn snapshot_skips_inspector_chrome() {
        let mut overlay = Element::new().focus_id("secret");
        overlay.template_id = Some("__rui_devtools_overlay".into());
        overlay = overlay.child(Element::new().focus_id("dock"));
        let root = Element::new()
            .focus_id("app")
            .child(Element::new().focus_id("leaf"))
            .child(overlay);
        let nodes = snapshot_tree(&root);
        assert!(nodes.iter().all(|n| n.focus_id.as_deref() != Some("dock")));
        assert!(nodes
            .iter()
            .all(|n| n.focus_id.as_deref() != Some("secret")));
        assert_eq!(nodes.len(), 2);
    }

    #[test]
    fn template_id_source_location() {
        let loc = source_location_from_template_id("src/main.rs:42:8").unwrap();
        assert_eq!(loc.line, 42);
        assert_eq!(loc.column, 8);
        assert!(source_location_from_template_id("opaque-id").is_none());
    }
}
