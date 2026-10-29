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
    let panel_id = node_id_for_key("jadix:tpl:panel@0");
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
            assert!(seen.insert(*child), "duplicate AccessKit child {:?}", child);
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
            node_id_for_key("jadix:tpl:shared@0"),
            node_id_for_key("jadix:tpl:shared@1"),
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
    assert_eq!(kids[1], node_id_for_key("jadix:path:1"));
}

#[test]
fn a11y_root_is_not_listed_as_its_own_child() {
    let mut root = Element::new().text("root-label");
    root = root.child(Element::new().focus_id("inner").on_click(|| {}));
    let update = tree_update_from_elements(&root, None);
    assert_no_duplicate_children(&update);
    let root_a11y = node_id_for_key("jadix:path:root");
    assert_eq!(
        child_ids_of(&update.nodes, ROOT_ID),
        vec![document_node_id()]
    );
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

    let lid = node_id_for_key("jadix:path:0");
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
            target_tree: TreeId::ROOT,
            target_node: target,
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
    let label_id = node_id_for_key("jadix:path:0");
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
    let id = node_id_for_key("jadix:path:0");
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
    let id = node_id_for_key("jadix:path:0");
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

    let polite_id = node_id_for_key("jadix:path:0");
    let assertive_id = node_id_for_key("jadix:path:1");
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
    let id = node_id_for_key("jadix:tpl:m@0");
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
    use crate::core::reactivity::use_signal;
    use crate::utils::text_field::input_value_signal_key;

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
fn stale_focus_id_falls_back_to_document() {
    let btn = Element::new().focus_id("alive").on_click(|| {});
    let root = Element::new().child(btn);
    let update = tree_update_from_elements(&root, Some("gone-popover__cal_focus"));
    let ids: std::collections::HashSet<_> = update.nodes.iter().map(|(id, _)| *id).collect();
    assert!(
        ids.contains(&update.focus),
        "AccessKit focus {:?} must be in node list",
        update.focus
    );
    assert_eq!(update.focus, document_node_id());
}

#[test]
fn live_focus_id_stays_on_control() {
    let btn = Element::new().focus_id("alive").on_click(|| {});
    let root = Element::new().child(btn);
    let update = tree_update_from_elements(&root, Some("alive"));
    assert_eq!(update.focus, node_id_for_key("alive"));
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
    let mut input = Element::new().focus_id("input:email").focusable(true);
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
    lbl.attrs.insert("for".to_string(), "terms-cb".to_string());
    lbl.attrs
        .insert("accesskit_id".to_string(), "lbl:for:terms-cb".to_string());
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

fn slot(name: &str, el: Element) -> Element {
    let mut el = el;
    el.set_attr("data-slot", name);
    el
}

#[test]
fn table_slots_are_rows_and_cells() {
    let head = slot("table-head", Element::new().text("Name"));
    let header_row = slot("table-row", Element::new().child(head));
    let header = slot("table-header", Element::new().child(header_row));
    let cell = slot("table-cell", Element::new().text("Ada"));
    let body_row = slot("table-row", Element::new().child(cell));
    let body = slot("table-body", Element::new().child(body_row));
    let table = slot("table", Element::new().child(header).child(body));
    let root = Element::new().child(table);
    let update = tree_update_from_elements(&root, None);

    let table_id = node_id_for_key("jadix:path:0");
    let table_node = update
        .nodes
        .iter()
        .find(|(id, _)| *id == table_id)
        .map(|(_, n)| n)
        .expect("table");
    assert_eq!(table_node.role(), Role::Table);
    assert_eq!(table_node.children().len(), 2);

    let header_id = table_node.children()[0];
    let header_node = update
        .nodes
        .iter()
        .find(|(id, _)| *id == header_id)
        .map(|(_, n)| n)
        .expect("header");
    assert_eq!(header_node.role(), Role::RowGroup);
    let row_id = header_node.children()[0];
    let row = update
        .nodes
        .iter()
        .find(|(id, _)| *id == row_id)
        .map(|(_, n)| n)
        .expect("header row");
    assert_eq!(row.role(), Role::Row);
    let head_id = row.children()[0];
    let head_node = update
        .nodes
        .iter()
        .find(|(id, _)| *id == head_id)
        .map(|(_, n)| n)
        .expect("column header");
    assert_eq!(head_node.role(), Role::ColumnHeader);
    assert_eq!(head_node.label(), Some("Name"));
    // Caption folded into the header — not a second Label child.
    assert!(head_node.children().is_empty());
}

#[test]
fn bare_table_gets_a_synthetic_row() {
    let table = Element::new()
        .accesskit_role("table")
        .child(Element::new().text("Only"));
    let root = Element::new().child(table);
    let update = tree_update_from_elements(&root, None);
    let table_id = node_id_for_key("jadix:path:0");
    let table_node = update
        .nodes
        .iter()
        .find(|(id, _)| *id == table_id)
        .map(|(_, n)| n)
        .expect("table");
    assert_eq!(table_node.role(), Role::Table);
    assert_eq!(table_node.children().len(), 1);
    let row = update
        .nodes
        .iter()
        .find(|(id, _)| *id == table_node.children()[0])
        .map(|(_, n)| n)
        .expect("synthetic row");
    assert_eq!(row.role(), Role::Row);
    let cell = update
        .nodes
        .iter()
        .find(|(id, _)| *id == row.children()[0])
        .map(|(_, n)| n)
        .expect("synthetic cell");
    assert_eq!(cell.role(), Role::Cell);
    assert_eq!(cell.label(), Some("Only"));
}

#[test]
fn menu_checkbox_keeps_role_and_bare_menu_synthesizes_items() {
    assert_eq!(
        parse_accesskit_role("menuitemcheckbox"),
        Some(Role::MenuItemCheckBox)
    );
    assert_eq!(
        parse_accesskit_role("menuitemradio"),
        Some(Role::MenuItemRadio)
    );
    assert_eq!(parse_accesskit_role("dialog"), Some(Role::Dialog));
    assert_eq!(parse_accesskit_role("alertdialog"), Some(Role::AlertDialog));

    let item = Element::new()
        .accesskit_role("menuitemcheckbox")
        .child(Element::new().text("Pinned"));
    let menu = Element::new().accesskit_role("menu").child(item);
    let root = Element::new().child(menu);
    let update = tree_update_from_elements(&root, None);
    let menu_node = update
        .nodes
        .iter()
        .find(|(id, _)| *id == node_id_for_key("jadix:path:0"))
        .map(|(_, n)| n)
        .expect("menu");
    assert_eq!(menu_node.role(), Role::Menu);
    let item_node = update
        .nodes
        .iter()
        .find(|(id, _)| *id == menu_node.children()[0])
        .map(|(_, n)| n)
        .expect("item");
    assert_eq!(item_node.role(), Role::MenuItemCheckBox);
    assert_eq!(item_node.label(), Some("Pinned"));

    let bare = Element::new()
        .accesskit_role("menu")
        .child(Element::new().text("Open"));
    let bare_root = Element::new().child(bare);
    let bare_update = tree_update_from_elements(&bare_root, None);
    let bare_menu = bare_update
        .nodes
        .iter()
        .find(|(id, _)| *id == node_id_for_key("jadix:path:0"))
        .map(|(_, n)| n)
        .expect("bare menu");
    assert_eq!(bare_menu.children().len(), 1);
    let synth = bare_update
        .nodes
        .iter()
        .find(|(id, _)| *id == bare_menu.children()[0])
        .map(|(_, n)| n)
        .expect("synthetic item");
    assert_eq!(synth.role(), Role::MenuItem);
    assert_eq!(synth.label(), Some("Open"));
}

#[test]
fn dialog_title_labels_the_dialog() {
    let title = slot("dialog-title", Element::new().text("Confirm"));
    let dialog = slot("dialog-content", Element::new().child(title));
    let root = Element::new().child(dialog);
    let update = tree_update_from_elements(&root, None);
    let dialog_node = update
        .nodes
        .iter()
        .find(|(id, _)| *id == node_id_for_key("jadix:path:0"))
        .map(|(_, n)| n)
        .expect("dialog");
    assert_eq!(dialog_node.role(), Role::Dialog);
    assert_eq!(dialog_node.children().len(), 1);
    let title_id = dialog_node.children()[0];
    let title_node = update
        .nodes
        .iter()
        .find(|(id, _)| *id == title_id)
        .map(|(_, n)| n)
        .expect("title");
    assert_eq!(title_node.role(), Role::Heading);
    assert_eq!(title_node.label(), Some("Confirm"));
    assert_eq!(dialog_node.labelled_by(), &[title_id]);
}

#[test]
fn bare_dialog_gets_a_synthetic_title() {
    let dialog = Element::new()
        .accesskit_role("dialog")
        .child(Element::new().text("Discard changes"));
    let root = Element::new().child(dialog);
    let update = tree_update_from_elements(&root, None);
    let dialog_node = update
        .nodes
        .iter()
        .find(|(id, _)| *id == node_id_for_key("jadix:path:0"))
        .map(|(_, n)| n)
        .expect("dialog");
    assert_eq!(dialog_node.role(), Role::Dialog);
    let title_id = dialog_node.labelled_by()[0];
    assert_eq!(dialog_node.children()[0], title_id);
    let title = update
        .nodes
        .iter()
        .find(|(id, _)| *id == title_id)
        .map(|(_, n)| n)
        .expect("synthetic title");
    assert_eq!(title.role(), Role::Heading);
    assert_eq!(title.label(), Some("Discard changes"));
}
