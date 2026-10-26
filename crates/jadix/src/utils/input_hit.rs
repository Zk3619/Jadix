use crate::core::tree::{CursorValue, Element};

pub fn pick_cursor(el: &Element, x: f32, y: f32) -> Option<CursorValue> {
    enum Phase {
        Enter,
        After,
    }
    struct Frame<'a> {
        el: &'a Element,
        phase: Phase,
    }
    let mut stack = vec![Frame {
        el,
        phase: Phase::Enter,
    }];
    let mut result = None;
    while let Some(frame) = stack.pop() {
        match frame.phase {
            Phase::Enter => {
                if result.is_some() {
                    continue;
                }
                stack.push(Frame {
                    el: frame.el,
                    phase: Phase::After,
                });
                for child in &frame.el.children {
                    stack.push(Frame {
                        el: child,
                        phase: Phase::Enter,
                    });
                }
            }
            Phase::After => {
                if result.is_some() {
                    continue;
                }
                let b = frame.el.layout_bounds;
                if x >= b[0] && x <= b[0] + b[2] && y >= b[1] && y <= b[1] + b[3] {
                    result = frame.el.cursor.clone();
                }
            }
        }
    }
    result
}

pub fn find_bounds_by_focus_id(el: &Element, focus_id: &str) -> Option<[f32; 4]> {
    crate::core::tree::find_in_tree(el, |n| n.focus_id.as_deref() == Some(focus_id))
        .map(|n| n.layout_bounds)
}

pub fn find_input_text_leaf_for_rest(
    el: &Element,
    inherited_fid: Option<&str>,
    rest: &str,
) -> Option<([f32; 4], String, crate::style::TextStyle)> {
    enum Phase {
        Enter,
        After,
    }
    struct Frame<'a> {
        el: &'a Element,
        fid: Option<&'a str>,
        phase: Phase,
    }
    let mut stack = vec![Frame {
        el,
        fid: inherited_fid,
        phase: Phase::Enter,
    }];
    let mut result = None;
    while let Some(frame) = stack.pop() {
        match frame.phase {
            Phase::Enter => {
                if result.is_some() {
                    continue;
                }
                let fid = frame.el.focus_id.as_deref().or(frame.fid);
                stack.push(Frame {
                    el: frame.el,
                    fid: frame.fid,
                    phase: Phase::After,
                });
                for child in &frame.el.children {
                    stack.push(Frame {
                        el: child,
                        fid,
                        phase: Phase::Enter,
                    });
                }
            }
            Phase::After => {
                if result.is_some() {
                    continue;
                }
                let fid = frame.el.focus_id.as_deref().or(frame.fid);
                if let Some(text) = &frame.el.text {
                    if let Some(id) = fid {
                        if id == format!("{}{}", crate::utils::text_field::INPUT_FOCUS_PREFIX, rest)
                        {
                            result = Some((
                                frame.el.layout_bounds,
                                text.clone(),
                                frame.el.text_style.as_ref().clone(),
                            ));
                        }
                    }
                }
            }
        }
    }
    result
}

pub fn find_input_text_at(
    el: &Element,
    inherited_fid: Option<&str>,
    x: f32,
    y: f32,
) -> Option<(String, [f32; 4], String, crate::style::TextStyle)> {
    enum Phase {
        Enter,
        After,
    }
    struct Frame<'a> {
        el: &'a Element,
        fid: Option<&'a str>,
        phase: Phase,
    }
    let mut stack = vec![Frame {
        el,
        fid: inherited_fid,
        phase: Phase::Enter,
    }];
    let mut result = None;
    while let Some(frame) = stack.pop() {
        match frame.phase {
            Phase::Enter => {
                if result.is_some() {
                    continue;
                }
                let fid = frame.el.focus_id.as_deref().or(frame.fid);
                stack.push(Frame {
                    el: frame.el,
                    fid: frame.fid,
                    phase: Phase::After,
                });
                for child in &frame.el.children {
                    stack.push(Frame {
                        el: child,
                        fid,
                        phase: Phase::Enter,
                    });
                }
            }
            Phase::After => {
                if result.is_some() {
                    continue;
                }
                let fid = frame.el.focus_id.as_deref().or(frame.fid);
                if let Some(text) = &frame.el.text {
                    if let Some(id) = fid {
                        if let Some(rest) =
                            id.strip_prefix(crate::utils::text_field::INPUT_FOCUS_PREFIX)
                        {
                            let b = frame.el.layout_bounds;
                            if x >= b[0] && x <= b[0] + b[2] && y >= b[1] && y <= b[1] + b[3] {
                                result = Some((
                                    rest.to_string(),
                                    b,
                                    text.clone(),
                                    frame.el.text_style.as_ref().clone(),
                                ));
                            }
                        }
                    }
                }
            }
        }
    }
    result
}

pub fn collect_focusable_ids(el: &Element, ids: &mut Vec<String>) {
    crate::core::tree::walk_elements(el, |el| {
        if el.focusable {
            if let Some(id) = &el.focus_id {
                ids.push(id.clone());
            }
        }
    });
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
    crate::core::tree::find_in_tree(el, |n| {
        n.template_id.as_deref() == Some(crate::layout::layout_tree::PORTAL_LAYER_ID)
    })
}

fn subtree_has_z(el: &Element, z: i32) -> bool {
    crate::core::tree::find_in_tree(el, |n| n.style.z_index == z).is_some()
}
