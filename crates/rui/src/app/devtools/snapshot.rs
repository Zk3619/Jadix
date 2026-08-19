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

pub fn find_element_by_template<'a>(root: &'a Element, id: &str) -> Option<&'a Element> {
    if root.template_id.as_deref() == Some(id) {
        return Some(root);
    }
    for child in &root.children {
        if let Some(found) = find_element_by_template(child, id) {
            return Some(found);
        }
    }
    None
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
    if el.template_id.as_deref() == Some(id) {
        return true;
    }
    for (i, child) in el.children.iter().enumerate() {
        acc.push(i);
        if find_template_path(child, id, acc) {
            return true;
        }
        acc.pop();
    }
    false
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
        border_radius: format!("{:.0} {:.0} {:.0} {:.0}", style.radii[2], style.radii[0], style.radii[1], style.radii[3]),
        border_collapse: style.border_collapse,
        border_spacing: format!("{:.0} {:.0}", style.border_spacing[0], style.border_spacing[1]),
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

fn tag_hint_from(el: &Element) -> Option<String> {
    el.attrs
        .get("data-slot")
        .cloned()
        .or_else(|| el.attrs.get("role").cloned())
        .or_else(|| {
            el.class_set
                .as_ref()
                .and_then(|s| s.base.first().cloned())
        })
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
        style: style_snapshot(&el.style),
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
    if is_inspector_chrome(el) {
        return;
    }
    out.push(snapshot_node(el, path.clone(), depth));
    for (i, child) in el.children.iter().enumerate() {
        let mut child_path = path.clone();
        child_path.push(i);
        collect_tree(child, child_path, depth + 1, out);
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

/// Reconstruct a CSS-like declaration list from a style snapshot (P2 editor seed).
pub fn cssom_text_from_style(st: &StyleSnapshot) -> String {
    format!(
        "display: {};\nflex-direction: {};\nposition: {};\nwidth: {};\nheight: {};\nmin-width: {};\nmin-height: {};\npadding: {};\nmargin: {};\ngap: {};\nbackground: {};\nborder-width: {}px;\nborder-color: {};\nborder-style: {};\nborder-radius: {};\nopacity: {:.2};\nflex-grow: {};\nflex-shrink: {};\nz-index: {};\noverflow-x: {};\noverflow-y: {};\n",
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
        st.opacity,
        st.flex_grow,
        st.flex_shrink,
        st.z_index,
        css_ident(&st.overflow_x),
        css_ident(&st.overflow_y),
    )
}

fn css_ident(dbg: &str) -> String {
    dbg.trim_matches(|c| c == '"' || c == '\'').to_ascii_lowercase()
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
    fn normalize_promotes_class_parent_over_text_leaf() {
        let mut label_el = Element::new().text("Nav");
        label_el.layout_bounds = [12.0, 12.0, 40.0, 16.0];
        let mut host = Element::new();
        host.class_set = Some(crate::style::ClassSet {
            base: vec!["gal-nav-text".into()],
            ..Default::default()
        });
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
        assert!(nodes.iter().all(|n| n.focus_id.as_deref() != Some("secret")));
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
