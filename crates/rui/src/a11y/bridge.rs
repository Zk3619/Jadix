//! Build an [`accesskit::TreeUpdate`] snapshot from the RUI element tree for wiring to `accesskit_winit`.
//!
//! Part of [`crate::a11y`]; Adapter lifecycle remains in `app`.

use accesskit::{
    Action, ActionRequest, Live, Node, NodeId, Rect, Role, Toggled, Tree, TreeUpdate,
};
use std::hash::{Hash, Hasher};

use crate::components::label::ATTR_LABEL_FOR;
use crate::components::tabs::TABSTRIP_FOCUS_PREFIX;
use crate::core::reactivity::use_signal;
use crate::core::tree::Element;
use crate::utils::text_field::{
    input_value_signal_key, INPUT_FOCUS_PREFIX, TEXTAREA_FOCUS_PREFIX,
};
use std::collections::HashMap;

const ROOT_ID: NodeId = NodeId(1);
/// Stable key for the Document node under Window (Narrator / browse content).
pub(crate) const DOCUMENT_NODE_KEY: &str = "rui:document";
/// App focus signal value when AccessKit focuses the Document (reading mode entry).
pub(crate) const DOCUMENT_FOCUS_ID: &str = "__rui_a11y_document";

fn document_node_id() -> NodeId {
    node_id_for_key(DOCUMENT_NODE_KEY)
}

/// Attrs that force an AccessKit node even without focus/click/text.
const A11Y_NAME_DESC_LIVE_ATTRS: &[&str] = &[
    "aria-label",
    "aria-description",
    "aria-labelledby",
    "aria-describedby",
    "aria-live",
    "accesskit_name",
    "accesskit_description",
    "accesskit_live",
    "accesskit_id",
    "accesskit_labelled_by",
];

pub(crate) fn node_id_for_key(key: &str) -> NodeId {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    key.hash(&mut h);
    NodeId(h.finish().max(2))
}

fn node_id_for_focus(fid: &str) -> NodeId {
    node_id_for_key(fid)
}

/// Stable key for AccessKit nodes.
///
/// Order: non-empty `focus_id` → `accesskit_id` (Label / labelled-by targets) →
/// path-qualified `template_id` → path. Empty strings are never keys.
pub(crate) fn stable_node_key(el: &Element, path: &str) -> String {
    if let Some(fid) = el.focus_id.as_deref().filter(|s| !s.is_empty()) {
        fid.to_string()
    } else if let Some(aid) = el
        .attrs
        .get("accesskit_id")
        .map(|s| s.as_str())
        .filter(|s| !s.is_empty())
    {
        aid.to_string()
    } else if let Some(tid) = el.template_id.as_deref().filter(|s| !s.is_empty()) {
        // template_id is reconcile identity within a slot, not a global a11y id.
        format!("rui:tpl:{tid}@{path}")
    } else {
        format!("rui:path:{path}")
    }
}

fn parse_accesskit_role(raw: &str) -> Option<Role> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "button" => Some(Role::Button),
        "checkbox" => Some(Role::CheckBox),
        "switch" => Some(Role::Switch),
        "textinput" | "text_input" | "textfield" => Some(Role::TextInput),
        "label" => Some(Role::Label),
        "group" => Some(Role::Group),
        "tablist" => Some(Role::TabList),
        "link" => Some(Role::Link),
        "list" => Some(Role::List),
        "listitem" | "list_item" => Some(Role::ListItem),
        "listbox" | "list_box" => Some(Role::ListBox),
        "combobox" | "combo_box" => Some(Role::ComboBox),
        "menu" => Some(Role::Menu),
        "menuitem" | "menu_item" => Some(Role::MenuItem),
        "menubar" | "menu_bar" => Some(Role::MenuBar),
        "option" => Some(Role::ListBoxOption),
        "image" => Some(Role::Image),
        "paragraph" => Some(Role::Paragraph),
        _ => None,
    }
}

fn has_a11y_name_desc_live_attr(el: &Element) -> bool {
    A11Y_NAME_DESC_LIVE_ATTRS
        .iter()
        .any(|k| el.attrs.contains_key(*k))
}

fn should_be_accesskit_node(el: &Element) -> bool {
    // Empty `focus_id` is not an identity (see `docs/element_identity.md` I-1); ignore it here.
    el.focus_id.as_deref().is_some_and(|s| !s.is_empty())
        || el.on_click.is_some()
        || el.text.is_some()
        || el.attrs.contains_key("accesskit_role")
        || has_a11y_name_desc_live_attr(el)
        || el.switch_checked_sync.is_some()
        || el.checkbox_sync.is_some()
}

fn parse_live(raw: &str) -> Option<Live> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "polite" => Some(Live::Polite),
        "assertive" => Some(Live::Assertive),
        "off" => Some(Live::Off),
        _ => None,
    }
}

fn live_attr_value(el: &Element) -> Option<&str> {
    el.attrs
        .get("accesskit_live")
        .or_else(|| el.attrs.get("aria-live"))
        .map(|s| s.as_str())
}

fn infer_role(el: &Element, fid: Option<&str>) -> Role {
    if let Some(raw) = el.attrs.get("accesskit_role") {
        if let Some(role) = parse_accesskit_role(raw) {
            return role;
        }
    }
    if el.switch_checked_sync.is_some() {
        Role::Switch
    } else if el.checkbox_sync.is_some() {
        Role::CheckBox
    } else if fid.is_some_and(|f| {
        f.starts_with(INPUT_FOCUS_PREFIX) || f.starts_with(TEXTAREA_FOCUS_PREFIX)
    }) {
        Role::TextInput
    } else if fid.is_some_and(|f| f.starts_with(TABSTRIP_FOCUS_PREFIX)) {
        Role::TabList
    } else if el.on_click.is_some() {
        Role::Button
    } else if let Some(live) = live_attr_value(el).and_then(parse_live) {
        match live {
            Live::Assertive => Role::Alert,
            Live::Polite => Role::Status,
            Live::Off => {
                if el.text.is_some() {
                    Role::Label
                } else {
                    Role::GenericContainer
                }
            }
        }
    } else if el.text.is_some() {
        Role::Label
    } else {
        Role::GenericContainer
    }
}

fn bounds_from_layout(b: [f32; 4]) -> Option<Rect> {
    if b[2] > 0.0 && b[3] > 0.0 {
        Some(Rect {
            x0: f64::from(b[0]),
            y0: f64::from(b[1]),
            x1: f64::from(b[0] + b[2]),
            y1: f64::from(b[1] + b[3]),
        })
    } else {
        None
    }
}

fn union_rect(a: Rect, b: Rect) -> Rect {
    Rect {
        x0: a.x0.min(b.x0),
        y0: a.y0.min(b.y0),
        x1: a.x1.max(b.x1),
        y1: a.y1.max(b.y1),
    }
}

/// Prefer this node's layout box; otherwise union descendants (text often lays out on children).
fn effective_bounds(el: &Element) -> Option<Rect> {
    if let Some(r) = bounds_from_layout(el.layout_bounds) {
        return Some(r);
    }
    let mut acc: Option<Rect> = None;
    for child in &el.children {
        if let Some(cb) = effective_bounds(child) {
            acc = Some(match acc {
                None => cb,
                Some(a) => union_rect(a, cb),
            });
        }
    }
    acc
}

/// Visible text on this element and descendants (Button captions live on child text nodes).
fn collect_visible_text(el: &Element) -> String {
    let mut parts = Vec::new();
    collect_visible_text_into(el, &mut parts);
    parts.join(" ")
}

fn collect_visible_text_into(el: &Element, parts: &mut Vec<String>) {
    if let Some(t) = el.text.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        parts.push(t.to_string());
    }
    for child in &el.children {
        collect_visible_text_into(child, parts);
    }
}

/// Controls that expose a name from descendant text (ARIA name-from-contents).
fn uses_name_from_contents(el: &Element) -> bool {
    if el.on_click.is_some() {
        return true;
    }
    matches!(
        el.attrs
            .get("accesskit_role")
            .and_then(|r| parse_accesskit_role(r)),
        Some(
            Role::Button
                | Role::MenuItem
                | Role::ListBoxOption
                | Role::Link
                | Role::CheckBox
                | Role::Switch
        )
    )
}

/// Plain text leaf that should fold into an ancestor's accessible name, not a sibling Label.
fn is_plain_text_name_leaf(el: &Element) -> bool {
    if el.text.as_deref().map(str::trim).is_none_or(|s| s.is_empty()) {
        return false;
    }
    if el.on_click.is_some() {
        return false;
    }
    if el.focus_id.as_deref().is_some_and(|s| !s.is_empty()) {
        return false;
    }
    if el.attrs.contains_key("accesskit_role")
        || el.attrs.contains_key("accesskit_id")
        || has_a11y_name_desc_live_attr(el)
    {
        return false;
    }
    if el.switch_checked_sync.is_some() || el.checkbox_sync.is_some() {
        return false;
    }
    el.children.iter().all(is_plain_text_name_leaf_or_empty)
}

fn is_plain_text_name_leaf_or_empty(el: &Element) -> bool {
    if el.text.as_deref().map(str::trim).is_some_and(|s| !s.is_empty()) {
        return is_plain_text_name_leaf(el);
    }
    el.on_click.is_none()
        && el.focus_id.as_deref().map_or(true, |s| s.is_empty())
        && !el.attrs.contains_key("accesskit_role")
        && !has_a11y_name_desc_live_attr(el)
        && el.switch_checked_sync.is_none()
        && el.checkbox_sync.is_none()
        && el.children.iter().all(is_plain_text_name_leaf_or_empty)
}

fn resolve_accessible_text(el: &Element, role: Role) -> Option<String> {
    if let Some(text) = el
        .attrs
        .get("accesskit_name")
        .or_else(|| el.attrs.get("aria-label"))
        .map(|s| s.as_str())
        .filter(|t| !t.is_empty())
    {
        return Some(text.to_string());
    }

    // Label：用自身文案；勿把内嵌 Checkbox 勾选字形拼进读屏名。
    if role == Role::Label {
        if let Some(t) = el
            .text
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            return Some(t.to_string());
        }
        return collect_label_caption(el);
    }

    // 已有 labelledby 时，名称交给关联 Label（避免勾选 glyph 当可读名）。
    let has_labelled_by = el.attrs.contains_key("accesskit_labelled_by")
        || el.attrs.contains_key("aria-labelledby");
    if has_labelled_by && matches!(role, Role::CheckBox | Role::Switch | Role::TextInput) {
        return None;
    }

    if matches!(role, Role::CheckBox | Role::Switch) {
        let cleaned = collect_visible_text_excluding_lucide_root(el);
        if !cleaned.is_empty() {
            return Some(cleaned);
        }
    } else {
        let collected = collect_visible_text(el);
        if !collected.is_empty() {
            return Some(collected);
        }
    }

    if role == Role::TextInput || role == Role::ComboBox {
        return el
            .attrs
            .get("placeholder")
            .map(|s| s.as_str())
            .filter(|t| !t.is_empty())
            .map(|s| s.to_string());
    }
    None
}

fn collect_label_caption(el: &Element) -> Option<String> {
    let mut parts = Vec::new();
    collect_label_caption_into(el, &mut parts, true);
    let s = parts.join(" ");
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

fn collect_label_caption_into(el: &Element, parts: &mut Vec<String>, is_root: bool) {
    if !is_root
        && (el.checkbox_sync.is_some()
            || el.switch_checked_sync.is_some()
            || el.on_click.is_some())
    {
        return;
    }
    if let Some(t) = el.text.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        if el.text_style.font_family.as_deref() != Some("lucide") {
            parts.push(t.to_string());
        }
    }
    for child in &el.children {
        collect_label_caption_into(child, parts, false);
    }
}

fn collect_visible_text_skip_lucide(el: &Element, parts: &mut Vec<String>) {
    if el.text_style.font_family.as_deref() == Some("lucide") {
        // 勾选 / 图标字形不当 accessible name；仍可下钻非 lucide 子节点。
        for child in &el.children {
            collect_visible_text_skip_lucide(child, parts);
        }
        return;
    }
    if let Some(t) = el.text.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        parts.push(t.to_string());
    }
    for child in &el.children {
        collect_visible_text_skip_lucide(child, parts);
    }
}

fn collect_visible_text_excluding_lucide_root(el: &Element) -> String {
    let mut parts = Vec::new();
    collect_visible_text_skip_lucide(el, &mut parts);
    parts.join(" ")
}

fn apply_labelled_by(el: &Element, node: &mut Node, for_map: &HashMap<String, String>) {
    let from_attr = el
        .attrs
        .get("accesskit_labelled_by")
        .or_else(|| el.attrs.get("aria-labelledby"))
        .map(|s| s.as_str());
    let ids: Vec<NodeId> = if let Some(refs) = from_attr {
        refs.split_whitespace()
            .filter(|s| !s.is_empty())
            .map(node_id_for_key)
            .collect()
    } else if let Some(fid) = el.focus_id.as_deref().filter(|s| !s.is_empty()) {
        for_map
            .get(fid)
            .map(|lbl_key| vec![node_id_for_key(lbl_key)])
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    if !ids.is_empty() {
        node.set_labelled_by(ids);
    }
}

fn build_node(el: &Element, for_map: &HashMap<String, String>) -> Node {
    let fid = el.focus_id.as_deref();
    let role = infer_role(el, fid);
    let mut node = Node::new(role);

    if let Some(bounds) = effective_bounds(el) {
        node.set_bounds(bounds);
    }

    // AccessKit: Role::Label Name comes from `value` (`label_comes_from_value`); controls use `label`.
    let named_by_label = el.attrs.contains_key("accesskit_labelled_by")
        || el.attrs.contains_key("aria-labelledby")
        || fid.is_some_and(|f| for_map.contains_key(f));
    let explicit_name = el
        .attrs
        .get("accesskit_name")
        .or_else(|| el.attrs.get("aria-label"))
        .map(|s| s.as_str())
        .filter(|t| !t.is_empty());

    if role == Role::Label {
        if let Some(text) = resolve_accessible_text(el, role) {
            node.set_value(text);
        }
    } else if let Some(text) = explicit_name {
        node.set_label(text.to_string());
    } else if !named_by_label {
        if let Some(text) = resolve_accessible_text(el, role) {
            node.set_label(text);
        }
    }

    apply_labelled_by(el, &mut node, for_map);

    if let Some(desc) = el
        .attrs
        .get("accesskit_description")
        .or_else(|| el.attrs.get("aria-description"))
        .map(|s| s.as_str())
        .filter(|t| !t.is_empty())
    {
        node.set_description(desc.to_string());
    }

    if let Some(live) = live_attr_value(el).and_then(parse_live) {
        node.set_live(live);
    }

    if el.on_click.is_some() {
        node.add_action(Action::Click);
    }
    if el.focusable {
        node.add_action(Action::Focus);
    }

    if let Some(s) = el.switch_checked_sync {
        node.set_toggled(if s.get() {
            Toggled::True
        } else {
            Toggled::False
        });
    } else if let Some(s) = el.checkbox_sync {
        node.set_toggled(if s.get() {
            Toggled::True
        } else {
            Toggled::False
        });
    } else if role == Role::TextInput {
        if let Some(rest) = fid.and_then(|f| {
            f.strip_prefix(INPUT_FOCUS_PREFIX)
                .or_else(|| f.strip_prefix(TEXTAREA_FOCUS_PREFIX))
        }) {
            let vk = input_value_signal_key(rest);
            let v = use_signal(&vk, String::new).get();
            node.set_value(v);
        }
    }

    node
}

fn alloc_unique_node_id(
    preferred_key: &str,
    path: &str,
    used: &mut std::collections::HashSet<u64>,
) -> (String, NodeId) {
    let mut key = preferred_key.to_string();
    let mut id = node_id_for_key(&key);
    if used.insert(id.0) {
        return (key, id);
    }
    // Duplicate focus_id / hash collision: path keeps the TreeUpdate valid.
    key = format!("rui:path:{path}");
    id = node_id_for_key(&key);
    let mut n = 0u32;
    while !used.insert(id.0) {
        n = n.saturating_add(1);
        key = format!("rui:path:{path}#{n}");
        id = node_id_for_key(&key);
    }
    (key, id)
}

fn walk_subtree(
    el: &Element,
    path: &str,
    nodes: &mut Vec<(NodeId, Node)>,
    parent_child_ids: &mut Vec<NodeId>,
    used: &mut std::collections::HashSet<u64>,
    under_name_from_contents: bool,
    for_map: &HashMap<String, String>,
) {
    // Fold plain caption text into the ancestor control name (avoid "按钮" + nested Label).
    if under_name_from_contents && is_plain_text_name_leaf(el) {
        return;
    }

    let absorb = under_name_from_contents || uses_name_from_contents(el);
    let mut child_ids: Vec<NodeId> = Vec::new();
    for (i, child) in el.children.iter().enumerate() {
        let child_path = if path.is_empty() {
            i.to_string()
        } else {
            format!("{path}/{i}")
        };
        walk_subtree(
            child,
            &child_path,
            nodes,
            &mut child_ids,
            used,
            absorb,
            for_map,
        );
    }

    if should_be_accesskit_node(el) {
        let preferred = stable_node_key(el, path);
        let (_key, id) = alloc_unique_node_id(&preferred, path, used);
        let mut node = build_node(el, for_map);
        node.set_children(child_ids);
        nodes.push((id, node));
        parent_child_ids.push(id);
    } else {
        parent_child_ids.extend(child_ids);
    }
}

/// Collect `Label[for=target] → accesskit_id` so sibling controls get `labelled_by` without attrs.
fn collect_label_for_map(el: &Element, map: &mut HashMap<String, String>) {
    if let Some(for_id) = el
        .attrs
        .get(ATTR_LABEL_FOR)
        .map(|s| s.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        let lbl_key = el
            .attrs
            .get("accesskit_id")
            .map(|s| s.as_str())
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| format!("lbl:for:{for_id}"));
        map.entry(for_id.to_string()).or_insert(lbl_key);
    }
    for child in &el.children {
        collect_label_for_map(child, map);
    }
}

/// Hierarchical tree: Window → Document → element a11y nodes.
///
/// A Document child is required for Narrator/browse-style reading of static Labels;
/// Tab alone only visits focusable controls (by design).
pub fn tree_update_from_elements(root: &Element, focused_fid: Option<&str>) -> TreeUpdate {
    let mut for_map = HashMap::new();
    collect_label_for_map(root, &mut for_map);

    let mut nodes: Vec<(NodeId, Node)> = Vec::new();
    let mut content_children: Vec<NodeId> = Vec::new();
    let mut used = std::collections::HashSet::new();
    let doc_id = document_node_id();
    used.insert(ROOT_ID.0);
    used.insert(doc_id.0);

    for (i, child) in root.children.iter().enumerate() {
        walk_subtree(
            child,
            &i.to_string(),
            &mut nodes,
            &mut content_children,
            &mut used,
            false,
            &for_map,
        );
    }

    // Root element itself may be an a11y node (unusual but supported).
    if should_be_accesskit_node(root) {
        let preferred = stable_node_key(root, "root");
        let (_key, id) = alloc_unique_node_id(&preferred, "root", &mut used);
        let mut node = build_node(root, &for_map);
        node.set_children(content_children);
        nodes.push((id, node));
        content_children = vec![id];
    }

    let mut document = Node::new(Role::Document);
    if let Some(bounds) = effective_bounds(root) {
        document.set_bounds(bounds);
    }
    // Lets AT request Focus to enter reading/browse context (not app Tab order).
    document.add_action(Action::Focus);
    document.set_children(content_children);
    nodes.push((doc_id, document));

    let mut root_node = Node::new(Role::Window);
    if let Some(bounds) = effective_bounds(root) {
        root_node.set_bounds(bounds);
    }
    root_node.set_children(vec![doc_id]);
    nodes.push((ROOT_ID, root_node));

    let focus = match focused_fid.map(str::trim).filter(|s| !s.is_empty()) {
        Some(DOCUMENT_FOCUS_ID) => doc_id,
        Some(fid) => node_id_for_focus(fid),
        // Prefer Document so Narrator starts in content/reading context, not empty Window.
        None => doc_id,
    };

    TreeUpdate {
        nodes,
        tree: Some(Tree::new(ROOT_ID)),
        focus,
    }
}

fn find_element_by_node_id<'a>(
    el: &'a Element,
    target: NodeId,
    path: &str,
    used: &mut std::collections::HashSet<u64>,
    under_name_from_contents: bool,
) -> Option<&'a Element> {
    // Must mirror [`walk_subtree`] folding so NodeId allocation stays aligned.
    if under_name_from_contents && is_plain_text_name_leaf(el) {
        return None;
    }
    let absorb = under_name_from_contents || uses_name_from_contents(el);
    for (i, child) in el.children.iter().enumerate() {
        let child_path = if path.is_empty() {
            i.to_string()
        } else {
            format!("{path}/{i}")
        };
        if let Some(found) =
            find_element_by_node_id(child, target, &child_path, used, absorb)
        {
            return Some(found);
        }
    }
    if should_be_accesskit_node(el) {
        let preferred = stable_node_key(el, path);
        let (_key, id) = alloc_unique_node_id(&preferred, path, used);
        if id == target {
            return Some(el);
        }
    }
    None
}

fn apply_accesskit_action(el: &Element, request: &ActionRequest) -> bool {
    match request.action {
        Action::Click => {
            if let Some(cb) = &el.on_click {
                cb.borrow_mut()();
                return true;
            }
        }
        Action::Focus if el.focusable => {
            if let Some(fid) = el.focus_id.as_deref().filter(|s| !s.is_empty()) {
                use_signal("__rui_button_focus_id", String::new).set(fid.to_string());
                return true;
            }
        }
        _ => {}
    }
    false
}

/// Handle an AccessKit action request against the current element tree.
pub fn dispatch_accesskit_action(root: &Element, request: &ActionRequest) -> bool {
    if request.target == document_node_id() && matches!(request.action, Action::Focus) {
        use_signal("__rui_button_focus_id", String::new).set(DOCUMENT_FOCUS_ID.to_string());
        return true;
    }

    let mut used = std::collections::HashSet::new();
    used.insert(ROOT_ID.0);
    used.insert(document_node_id().0);
    // Same path scheme as [`tree_update_from_elements`].
    for (i, child) in root.children.iter().enumerate() {
        if let Some(el) =
            find_element_by_node_id(child, request.target, &i.to_string(), &mut used, false)
        {
            return apply_accesskit_action(el, request);
        }
    }
    if should_be_accesskit_node(root) {
        let preferred = stable_node_key(root, "root");
        let (_key, id) = alloc_unique_node_id(&preferred, "root", &mut used);
        if id == request.target {
            return apply_accesskit_action(root, request);
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::tree::Element;

    fn child_ids_of(nodes: &[(NodeId, Node)], id: NodeId) -> Vec<NodeId> {
        nodes
            .iter()
            .find(|(nid, _)| *nid == id)
            .map(|(_, n)| n.children().to_vec())
            .unwrap_or_default()
    }

    fn document_children(update: &TreeUpdate) -> Vec<NodeId> {
        child_ids_of(&update.nodes, document_node_id())
    }

    #[test]
    fn hierarchical_tree_nests_children() {
        let mut panel = Element::new().template_id("panel");
        panel
            .attrs
            .insert("accesskit_role".to_string(), "group".to_string());
        let btn = Element::new().focus_id("btn1").on_click(|| {});
        panel = panel.child(btn);
        let root = Element::new().child(panel);

        let update = tree_update_from_elements(&root, None);
        let panel_id = node_id_for_key("rui:tpl:panel@0");
        let btn_id = node_id_for_key("btn1");
        let doc_id = document_node_id();

        assert!(update.nodes.iter().any(|(id, _)| *id == panel_id));
        assert!(update.nodes.iter().any(|(id, _)| *id == btn_id));
        assert_eq!(child_ids_of(&update.nodes, ROOT_ID), vec![doc_id]);
        assert_eq!(document_children(&update), vec![panel_id]);
        assert_eq!(child_ids_of(&update.nodes, panel_id), vec![btn_id]);
        assert_eq!(update.focus, doc_id);
    }

    fn assert_no_duplicate_children(update: &TreeUpdate) {
        let mut seen = std::collections::HashSet::new();
        for (_, node) in &update.nodes {
            for child in node.children() {
                assert!(
                    seen.insert(*child),
                    "duplicate AccessKit child {:?}",
                    child
                );
            }
        }
    }

    #[test]
    fn anon_empty_template_ids_do_not_duplicate_children() {
        // RSX `<Label>` / `<Button>` omit `id` → Default "" → shared template_id/focus_id.
        let a = Element::new().template_id("").text("One");
        let b = Element::new().template_id("").text("Two");
        let c = Element::new().focus_id("").on_click(|| {}).text("Btn");
        let root = Element::new().child(a).child(b).child(c);

        let update = tree_update_from_elements(&root, None);
        assert_no_duplicate_children(&update);
        let kids = document_children(&update);
        assert_eq!(kids.len(), 3);
        assert_ne!(kids[0], kids[1]);
        assert_ne!(kids[0], kids[2]);
        assert_ne!(kids[1], kids[2]);
    }

    #[test]
    fn reused_template_id_disambiguated_by_path() {
        let mut x = Element::new().template_id("shared");
        x.attrs
            .insert("accesskit_role".to_string(), "group".to_string());
        let mut y = Element::new().template_id("shared");
        y.attrs
            .insert("accesskit_role".to_string(), "group".to_string());
        let root = Element::new().child(x).child(y);
        let update = tree_update_from_elements(&root, None);
        assert_no_duplicate_children(&update);
        assert_eq!(
            document_children(&update),
            vec![
                node_id_for_key("rui:tpl:shared@0"),
                node_id_for_key("rui:tpl:shared@1"),
            ]
        );
    }

    #[test]
    fn duplicate_focus_id_second_node_gets_path_key() {
        let a = Element::new().focus_id("same").on_click(|| {});
        let b = Element::new().focus_id("same").on_click(|| {});
        let root = Element::new().child(a).child(b);
        let update = tree_update_from_elements(&root, None);
        assert_no_duplicate_children(&update);
        let kids = document_children(&update);
        // Post-order: child 0 allocated before child 1; first keeps bare focus_id.
        assert_eq!(kids[0], node_id_for_key("same"));
        assert_eq!(kids[1], node_id_for_key("rui:path:1"));
    }

    #[test]
    fn a11y_root_is_not_listed_as_its_own_child() {
        let mut root = Element::new().text("root-label");
        root = root.child(Element::new().focus_id("inner").on_click(|| {}));
        let update = tree_update_from_elements(&root, None);
        assert_no_duplicate_children(&update);
        let root_a11y = node_id_for_key("rui:path:root");
        assert_eq!(child_ids_of(&update.nodes, ROOT_ID), vec![document_node_id()]);
        assert_eq!(document_children(&update), vec![root_a11y]);
        assert_eq!(
            child_ids_of(&update.nodes, root_a11y),
            vec![node_id_for_key("inner")]
        );
    }

    #[test]
    fn document_contains_static_label_with_value() {
        let label = Element::new().text("Card title").accesskit_role("label");
        let root = Element::new().child(label);
        let update = tree_update_from_elements(&root, None);
        let doc = update
            .nodes
            .iter()
            .find(|(id, _)| *id == document_node_id())
            .map(|(_, n)| n)
            .expect("document");
        assert_eq!(doc.role(), Role::Document);
        assert!(doc.supports_action(Action::Focus));

        let lid = node_id_for_key("rui:path:0");
        let node = update
            .nodes
            .iter()
            .find(|(id, _)| *id == lid)
            .map(|(_, n)| n)
            .expect("label");
        assert_eq!(node.role(), Role::Label);
        assert_eq!(node.value(), Some("Card title"));
        assert_eq!(document_children(&update), vec![lid]);
    }

    #[test]
    fn dispatch_click_invokes_on_click() {
        use std::cell::Cell;
        use std::rc::Rc;

        let fired = Rc::new(Cell::new(false));
        let fired2 = fired.clone();
        let btn = Element::new().focus_id("act-btn").on_click(move || {
            fired2.set(true);
        });
        let root = Element::new().child(btn);

        let target = node_id_for_key("act-btn");
        let handled = dispatch_accesskit_action(
            &root,
            &ActionRequest {
                action: Action::Click,
                target,
                data: None,
            },
        );
        assert!(handled);
        assert!(fired.get());
    }

    #[test]
    fn text_node_gets_label_role() {
        let label = Element::new().text("Hello");
        let root = Element::new().child(label);
        let update = tree_update_from_elements(&root, None);
        let label_id = node_id_for_key("rui:path:0");
        let node = update
            .nodes
            .iter()
            .find(|(id, _)| *id == label_id)
            .map(|(_, n)| n);
        assert!(node.is_some());
        assert_eq!(node.unwrap().role(), Role::Label);
        // AccessKit: Label role exposes caption via `value`, not `label`.
        assert_eq!(node.unwrap().value(), Some("Hello"));
        assert_eq!(node.unwrap().label(), None);
    }

    #[test]
    fn aria_label_and_description_override_text() {
        let mut el = Element::new().text("fallback");
        el.attrs
            .insert("aria-label".to_string(), "Named".to_string());
        el.attrs
            .insert("aria-description".to_string(), "Described".to_string());
        let root = Element::new().child(el);
        let update = tree_update_from_elements(&root, None);
        let id = node_id_for_key("rui:path:0");
        let node = update
            .nodes
            .iter()
            .find(|(nid, _)| *nid == id)
            .map(|(_, n)| n)
            .expect("a11y node");
        assert_eq!(node.role(), Role::Label);
        assert_eq!(node.value(), Some("Named"));
        assert_eq!(node.description(), Some("Described"));
    }

    #[test]
    fn accesskit_name_description_preferred_over_aria() {
        let mut el = Element::new().text("fallback");
        el.attrs
            .insert("aria-label".to_string(), "Aria".to_string());
        el.attrs
            .insert("accesskit_name".to_string(), "AccessKit".to_string());
        el.attrs
            .insert("aria-description".to_string(), "AriaDesc".to_string());
        el.attrs.insert(
            "accesskit_description".to_string(),
            "AccessKitDesc".to_string(),
        );
        let root = Element::new().child(el);
        let update = tree_update_from_elements(&root, None);
        let id = node_id_for_key("rui:path:0");
        let node = update
            .nodes
            .iter()
            .find(|(nid, _)| *nid == id)
            .map(|(_, n)| n)
            .expect("a11y node");
        assert_eq!(node.role(), Role::Label);
        assert_eq!(node.value(), Some("AccessKit"));
        assert_eq!(node.description(), Some("AccessKitDesc"));
    }

    #[test]
    fn live_region_attrs_include_node_and_set_live() {
        let mut polite = Element::new();
        polite
            .attrs
            .insert("aria-live".to_string(), "polite".to_string());
        let mut assertive = Element::new();
        assertive
            .attrs
            .insert("accesskit_live".to_string(), "assertive".to_string());
        let root = Element::new().child(polite).child(assertive);
        let update = tree_update_from_elements(&root, None);

        let polite_id = node_id_for_key("rui:path:0");
        let assertive_id = node_id_for_key("rui:path:1");
        let polite_node = update
            .nodes
            .iter()
            .find(|(id, _)| *id == polite_id)
            .map(|(_, n)| n)
            .expect("polite live node");
        let assertive_node = update
            .nodes
            .iter()
            .find(|(id, _)| *id == assertive_id)
            .map(|(_, n)| n)
            .expect("assertive live node");

        assert_eq!(polite_node.role(), Role::Status);
        assert_eq!(polite_node.live(), Some(Live::Polite));
        assert_eq!(assertive_node.role(), Role::Alert);
        assert_eq!(assertive_node.live(), Some(Live::Assertive));
    }

    #[test]
    fn parses_menu_listbox_and_menubar_roles() {
        assert_eq!(parse_accesskit_role("menu"), Some(Role::Menu));
        assert_eq!(parse_accesskit_role("menuitem"), Some(Role::MenuItem));
        assert_eq!(parse_accesskit_role("listbox"), Some(Role::ListBox));
        assert_eq!(parse_accesskit_role("menubar"), Some(Role::MenuBar));
        assert_eq!(parse_accesskit_role("combobox"), Some(Role::ComboBox));
        assert_eq!(parse_accesskit_role("textfield"), Some(Role::TextInput));

        let mut menu = Element::new().template_id("m");
        menu.attrs
            .insert("accesskit_role".to_string(), "menu".to_string());
        let root = Element::new().child(menu);
        let update = tree_update_from_elements(&root, None);
        let id = node_id_for_key("rui:tpl:m@0");
        let node = update
            .nodes
            .iter()
            .find(|(nid, _)| *nid == id)
            .map(|(_, n)| n)
            .expect("menu node");
        assert_eq!(node.role(), Role::Menu);
    }

    #[test]
    fn input_prefix_is_text_input_with_value_and_placeholder_name() {
        use crate::components::input::input_value_signal_key;
        use crate::core::reactivity::use_signal;

        let id = "a11y_smoke_input";
        use_signal(&input_value_signal_key(id), String::new).set("hello".to_string());
        let mut el = Element::new()
            .focus_id(format!("input:{id}"))
            .focusable(true);
        el.attrs
            .insert("placeholder".to_string(), "Search…".to_string());
        let root = Element::new().child(el);
        let update = tree_update_from_elements(&root, Some(&format!("input:{id}")));
        let nid = node_id_for_key(&format!("input:{id}"));
        let node = update
            .nodes
            .iter()
            .find(|(i, _)| *i == nid)
            .map(|(_, n)| n)
            .expect("input node");
        assert_eq!(node.role(), Role::TextInput);
        assert_eq!(node.label(), Some("Search…"));
        assert_eq!(node.value(), Some("hello"));
        assert_eq!(update.focus, nid);
    }

    #[test]
    fn button_role_advertises_click_and_focus() {
        let btn = Element::new()
            .focus_id("a11y-btn")
            .focusable(true)
            .on_click(|| {});
        let root = Element::new().child(btn);
        let update = tree_update_from_elements(&root, None);
        let nid = node_id_for_key("a11y-btn");
        let node = update
            .nodes
            .iter()
            .find(|(i, _)| *i == nid)
            .map(|(_, n)| n)
            .expect("button node");
        assert_eq!(node.role(), Role::Button);
        assert!(node.supports_action(Action::Click));
        assert!(node.supports_action(Action::Focus));
    }

    #[test]
    fn button_name_from_descendant_text_not_nested_label() {
        // Matches real Button chrome: focusable root + child text node (no root.el.text).
        let caption = Element::new().text("Save draft");
        let btn = Element::new()
            .focus_id("save-btn")
            .focusable(true)
            .on_click(|| {})
            .child(caption);
        let root = Element::new().child(btn);
        let update = tree_update_from_elements(&root, None);
        let nid = node_id_for_key("save-btn");
        let node = update
            .nodes
            .iter()
            .find(|(i, _)| *i == nid)
            .map(|(_, n)| n)
            .expect("button node");
        assert_eq!(node.role(), Role::Button);
        assert_eq!(node.label(), Some("Save draft"));
        // Caption folded into name — no extra Label child under the button.
        assert!(node.children().is_empty());
    }

    #[test]
    fn label_associates_via_aria_labelledby_and_accesskit_id() {
        let mut lbl = Element::new().text("Email");
        lbl.attrs
            .insert("accesskit_id".to_string(), "email-lbl".to_string());
        let mut input = Element::new()
            .focus_id("input:email")
            .focusable(true);
        input
            .attrs
            .insert("aria-labelledby".to_string(), "email-lbl".to_string());
        let root = Element::new().child(lbl).child(input);
        let update = tree_update_from_elements(&root, None);

        let lid = node_id_for_key("email-lbl");
        let iid = node_id_for_key("input:email");
        let label = update
            .nodes
            .iter()
            .find(|(i, _)| *i == lid)
            .map(|(_, n)| n)
            .expect("label");
        let field = update
            .nodes
            .iter()
            .find(|(i, _)| *i == iid)
            .map(|(_, n)| n)
            .expect("input");
        assert_eq!(label.role(), Role::Label);
        assert_eq!(label.value(), Some("Email"));
        assert_eq!(field.labelled_by(), &[lid]);
    }

    #[test]
    fn label_for_maps_to_labelled_by_without_control_attr() {
        let mut lbl = Element::new().text("Accept terms").accesskit_role("label");
        lbl.attrs
            .insert("for".to_string(), "terms-cb".to_string());
        lbl.attrs.insert(
            "accesskit_id".to_string(),
            "lbl:for:terms-cb".to_string(),
        );
        let mut cb = Element::new().focus_id("terms-cb").focusable(true);
        cb.checkbox_sync = Some(crate::core::reactivity::use_signal(
            "a11y_label_for_cb",
            || false,
        ));
        let root = Element::new().child(lbl).child(cb);
        let update = tree_update_from_elements(&root, None);

        let lid = node_id_for_key("lbl:for:terms-cb");
        let cid = node_id_for_key("terms-cb");
        let label = update
            .nodes
            .iter()
            .find(|(i, _)| *i == lid)
            .map(|(_, n)| n)
            .expect("label");
        let box_node = update
            .nodes
            .iter()
            .find(|(i, _)| *i == cid)
            .map(|(_, n)| n)
            .expect("checkbox");
        assert_eq!(label.value(), Some("Accept terms"));
        assert_eq!(box_node.role(), Role::CheckBox);
        assert_eq!(box_node.labelled_by(), &[lid]);
        assert!(box_node.label().is_none());
    }
}
