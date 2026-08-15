use crate::core::tree::{CursorValue, Element};

pub fn pick_cursor(el: &Element, x: f32, y: f32) -> Option<CursorValue> {
    for child in el.children.iter().rev() {
        if let Some(c) = pick_cursor(child, x, y) {
            return Some(c);
        }
    }
    let b = el.layout_bounds;
    if x >= b[0] && x <= b[0] + b[2] && y >= b[1] && y <= b[1] + b[3] {
        el.cursor.clone()
    } else {
        None
    }
}

pub fn find_bounds_by_focus_id(el: &Element, focus_id: &str) -> Option<[f32; 4]> {
    if el.focus_id.as_deref() == Some(focus_id) {
        return Some(el.layout_bounds);
    }
    for c in &el.children {
        if let Some(b) = find_bounds_by_focus_id(c, focus_id) {
            return Some(b);
        }
    }
    None
}

pub fn find_input_text_leaf_for_rest(
    el: &Element,
    inherited_fid: Option<&str>,
    rest: &str,
) -> Option<([f32; 4], String, crate::style::TextStyle)> {
    let fid = el.focus_id.as_deref().or(inherited_fid);
    for child in el.children.iter().rev() {
        if let Some(r) = find_input_text_leaf_for_rest(child, fid, rest) {
            return Some(r);
        }
    }
    if let Some(text) = &el.text {
        if let Some(id) = fid {
            if id == format!("{}{}", crate::components::INPUT_FOCUS_PREFIX, rest) {
                return Some((el.layout_bounds, text.clone(), el.text_style.clone()));
            }
        }
    }
    None
}

pub fn find_input_text_at(
    el: &Element,
    inherited_fid: Option<&str>,
    x: f32,
    y: f32,
) -> Option<(String, [f32; 4], String, crate::style::TextStyle)> {
    let fid = el.focus_id.as_deref().or(inherited_fid);
    for child in el.children.iter().rev() {
        if let Some(r) = find_input_text_at(child, fid, x, y) {
            return Some(r);
        }
    }
    if let Some(text) = &el.text {
        if let Some(id) = fid {
            if let Some(rest) = id.strip_prefix(crate::components::INPUT_FOCUS_PREFIX) {
                let b = el.layout_bounds;
                if x >= b[0] && x <= b[0] + b[2] && y >= b[1] && y <= b[1] + b[3] {
                    return Some((rest.to_string(), b, text.clone(), el.text_style.clone()));
                }
            }
        }
    }
    None
}

pub fn collect_focusable_ids(el: &Element, ids: &mut Vec<String>) {
    if el.focusable {
        if let Some(id) = &el.focus_id {
            ids.push(id.clone());
        }
    }
    for child in &el.children {
        collect_focusable_ids(child, ids);
    }
}

/// Collect focusable ids under the root portal layer whose panel `z_index` matches `panel_z`.
///
/// Used so Tab cycles inside the top stacked overlay (Dialog / Menu / Popover) first.
pub fn collect_focusable_ids_in_portal_z(root: &Element, panel_z: i32, ids: &mut Vec<String>) {
    let Some(layer) = find_portal_layer(root) else {
        return;
    };
    for child in &layer.children {
        if subtree_has_z(child, panel_z) {
            collect_focusable_ids(child, ids);
        }
    }
}

fn find_portal_layer(el: &Element) -> Option<&Element> {
    if el.template_id.as_deref() == Some(crate::layout::layout_tree::PORTAL_LAYER_ID) {
        return Some(el);
    }
    for c in &el.children {
        if let Some(found) = find_portal_layer(c) {
            return Some(found);
        }
    }
    None
}

fn subtree_has_z(el: &Element, z: i32) -> bool {
    if el.style.z_index == z {
        return true;
    }
    el.children.iter().any(|c| subtree_has_z(c, z))
}
