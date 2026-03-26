use crate::core::tree::{CursorKind, Element};

pub fn pick_cursor(el: &Element, x: f32, y: f32) -> Option<CursorKind> {
    for child in el.children.iter().rev() {
        if let Some(c) = pick_cursor(child, x, y) {
            return Some(c);
        }
    }
    let b = el.layout_bounds;
    if x >= b[0] && x <= b[0] + b[2] && y >= b[1] && y <= b[1] + b[3] {
        el.cursor
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
