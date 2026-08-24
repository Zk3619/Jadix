//! Inspector dock + highlight overlay (P0/P1/P2).
//!
//! Overlay is a **placeholder** in the retained tree (`wrap_app`). Content is injected
//! from `AppState::render` via [`sync_overlay`] — not a `reactive_view` — so F12 / Pick
//! never flush render effects while `AppState` is borrowed.

use crate::app::runtime;
use crate::components::{
    button, label, scroll_area, textarea, Card, CardContent, CardContentProps, CardHeader,
    CardHeaderProps, CardProps, CardTitle, CardTitleProps,
};
use crate::core::reactivity::use_signal;
use crate::core::tree::Element;
use crate::primitives::{hstack, r#box, vstack};
use crate::style::{parse_color, parse_text_color};
use crate::utils::text_field::{focus_id_for_text_edit, input_value_signal_key};
use crate::utils::{build_icon_text_node, lucide_glyph};

use super::overlay::{box_model_diagram, highlight_slot};
use super::reactivity_panel::build_reactivity_panel;
use super::snapshot::{
    cssom_text_from_style, find_element_by_template_mut, node_label, snapshot_hover_anchor,
    snapshot_subtree, snapshot_tree, source_location_from_template_id, InspectorNodeSnapshot,
};
use super::state::{DevToolsState, DevToolsTab};

const PANEL_ID: &str = "__rui_devtools_panel";
pub(crate) const PANEL_SLOT_ID: &str = "__rui_devtools_panel_slot";

/// Wrap the app in a **row**: app host (flex-grow) + docked Inspector slot (layout space).
pub fn wrap_app(app: Element) -> Element {
    if !DevToolsState::is_enabled() {
        return app;
    }
    let mut host = Element::from(
        r#box()
            .relative(true)
            .flex_grow(1.0)
            .min_w(0.0)
            .h("100%")
            .overflow("hidden")
            .children([app, highlight_slot()]),
    );
    host.template_id = Some("__rui_app_host".into());

    let mut panel_slot =
        Element::from(r#box().h("100%").w(0.0).flex_shrink(0.0).overflow("hidden"));
    panel_slot.template_id = Some(PANEL_SLOT_ID.into());

    Element::from(hstack().w("100%").h("100%").children([host, panel_slot]))
}

/// Rebuild the docked panel when Inspector chrome generation changes.
pub fn sync_overlay(root: &mut Element) -> bool {
    if !DevToolsState::is_enabled() {
        return false;
    }
    let gen = DevToolsState::generation();
    if gen == DevToolsState::last_sync_gen() {
        return false;
    }
    let open = DevToolsState::is_open();
    let panel_w = DevToolsState::panel_width();
    let dock = if open {
        // Named button hover / spring keys outlive the previous dock element. Reset so a rebuild
        // cannot paint every Inspector chrome control as hovered until the pointer moves.
        reset_inspector_button_hover_state();
        Some(dock_panel_from_root(root))
    } else {
        None
    };
    let Some(slot) = find_element_by_template_mut(root, PANEL_SLOT_ID) else {
        return false;
    };
    let target_w = if open { panel_w } else { 0.0 };
    slot.style.size.width = taffy::style::Dimension::length(target_w);
    let had_children = !slot.children.is_empty();
    slot.children.clear();
    let mut changed = had_children || open;
    if let Some(dock) = dock {
        slot.children.push(dock);
        changed = true;
    }
    DevToolsState::mark_synced(gen);
    super::overlay::reset_box_model_visual_cache();
    super::overlay::reset_tree_row_hover_cache();
    changed
}

fn reset_inspector_button_hover_state() {
    use crate::core::reactivity::use_signal;
    const IDS: &[&str] = &[
        "dt-pick",
        "dt-close",
        "dt-tab-el",
        "dt-tab-lay",
        "dt-tab-rx",
    ];
    for id in IDS {
        let hover = use_signal(&format!("{id}_hover"), || false);
        if hover.peek() {
            hover.set_silent(false);
        }
    }
}

fn dock_panel_from_root(root: &Element) -> Element {
    let tab = DevToolsState::active_tab();
    let path = DevToolsState::selected_path();
    let pick = DevToolsState::is_pick();
    let panel_w = DevToolsState::panel_width();
    let tree_snap = snapshot_tree(root);
    let selected = snapshot_subtree(root, &path);
    dock_panel(
        tab,
        &tree_snap,
        selected.as_ref(),
        pick,
        panel_w,
        root,
        &path,
    )
}

fn dock_panel(
    tab: DevToolsTab,
    tree: &[InspectorNodeSnapshot],
    selected: Option<&InspectorNodeSnapshot>,
    pick: bool,
    panel_w: f32,
    root: &Element,
    path: &[usize],
) -> Element {
    let pick_glyph = lucide_glyph("square-dashed-mouse-pointer")
        .or_else(|| lucide_glyph("mouse-pointer-click"))
        .unwrap_or_else(|| "⌖".into());
    let pick_icon = build_icon_text_node(pick_glyph).text_size(16.0);
    let close_glyph = lucide_glyph("x").unwrap_or_else(|| "×".into());
    let close_icon = build_icon_text_node(close_glyph).text_size(16.0);

    let header: Element = hstack()
        .w("100%")
        .items("center")
        .justify("between")
        .gap("0.5rem")
        .px("0.75rem")
        .py("0.5rem")
        .flex_shrink(0.0)
        .children([
            label("dt-title")
                .text("RUI Inspector")
                .text_size("0.875rem")
                .font_weight("700")
                .into(),
            hstack()
                .gap("0.25rem")
                .items("center")
                .children([
                    button("dt-pick")
                        .size("icon-sm")
                        .variant(if pick { "secondary" } else { "ghost" })
                        .child(pick_icon)
                        .on_click(|| {
                            DevToolsState::toggle_pick();
                            runtime::request_redraw();
                        })
                        .into(),
                    button("dt-close")
                        .size("icon-sm")
                        .variant("ghost")
                        .child(close_icon)
                        .on_click(|| {
                            DevToolsState::set_open(false);
                            runtime::request_redraw();
                        })
                        .into(),
                ])
                .into(),
        ])
        .into();

    let tabs_el: Element = hstack()
        .w("100%")
        .gap("0.25rem")
        .px("0.5rem")
        .flex_shrink(0.0)
        .children([
            tab_btn(
                "dt-tab-el",
                "Elements",
                tab == DevToolsTab::Elements,
                DevToolsTab::Elements,
            ),
            tab_btn(
                "dt-tab-lay",
                "Layout",
                tab == DevToolsTab::Layout,
                DevToolsTab::Layout,
            ),
            tab_btn(
                "dt-tab-rx",
                "Reactivity",
                tab == DevToolsTab::Reactivity,
                DevToolsTab::Reactivity,
            ),
        ])
        .into();

    let mut body: Element = match tab {
        DevToolsTab::Elements => elements_body(tree, selected, root),
        DevToolsTab::Layout => card_stack([layout_body(selected)]),
        DevToolsTab::Reactivity => card_stack([inspector_card(
            "Reactivity",
            build_reactivity_panel(Some(root), path),
        )]),
    };
    body = body
        .flex_grow(1.0)
        .flex_basis(0.0)
        .min_h(0.0)
        .overflow("hidden");

    let inner = vstack()
        .w("100%")
        .h("100%")
        .min_h(0.0)
        .flex_grow(1.0)
        .overflow("hidden")
        .children([header, tabs_el, body])
        .into();

    let mut card = Card(CardProps {
        children: Some(inner),
        ..Default::default()
    });
    card = card
        .flex_grow(1.0)
        .min_h(0.0)
        .w("100%")
        .h("100%")
        .overflow("hidden")
        .py("0.35rem")
        .gap("0.35rem");

    let mut panel = Element::from(
        r#box()
            .w(panel_w)
            .h("100%")
            .p("0.35rem")
            .bg(parse_color("muted"))
            .flex_col()
            .min_h("0")
            .child(card),
    );
    panel.template_id = Some(PANEL_ID.into());
    panel
}

fn tab_btn(id: &str, text: &str, active: bool, tab: DevToolsTab) -> Element {
    button(id)
        .size("sm")
        .variant(if active { "secondary" } else { "ghost" })
        .text(text)
        .on_click(move || {
            DevToolsState::set_tab(tab);
            runtime::request_redraw();
        })
        .into()
}

fn inspector_card(title: &str, body: Element) -> Element {
    let slug = title
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    let mut header = CardHeader(CardHeaderProps {
        children: Some(CardTitle(CardTitleProps {
            children: Some(
                label(format!("dt-card-{title}"))
                    .text(title)
                    .text_size("0.75rem")
                    .into(),
            ),
            ..Default::default()
        })),
        ..Default::default()
    });
    header = header.flex_shrink(0.0);

    let scroll: Element = scroll_area(&format!("devtools-card-{slug}"))
        .w("100%")
        .h("100%")
        .min_h(0.0)
        .flex_grow(1.0)
        .overflow_x("hidden")
        .overflow_y("auto")
        .child(body)
        .into();

    let mut content = CardContent(CardContentProps {
        children: Some(scroll),
        ..Default::default()
    });
    content = content
        .flex_grow(1.0)
        .flex_basis(0.0)
        .min_h(0.0)
        .overflow("hidden");

    let inner: Element = vstack()
        .w("100%")
        .min_h(0.0)
        .flex_grow(1.0)
        .overflow("hidden")
        .children([header, content])
        .into();

    let mut card = Card(CardProps {
        children: Some(inner),
        ..Default::default()
    });
    card = card
        .py("0.5rem")
        .gap("0.35rem")
        .flex_grow(1.0)
        .flex_basis(0.0)
        .min_h(0.0)
        .min_w(0.0)
        .w("100%")
        .overflow("hidden");
    card
}

/// Shared flex column so Inspector cards can shrink and scroll instead of growing the dock.
fn card_stack(cards: impl IntoIterator<Item = Element>) -> Element {
    Element::from(
        vstack()
            .w("100%")
            .flex_grow(1.0)
            .flex_basis(0.0)
            .min_h(0.0)
            .gap("0.5rem")
            .p("0.5rem")
            .overflow("hidden")
            .children(cards),
    )
}

fn elements_body(
    tree: &[InspectorNodeSnapshot],
    selected: Option<&InspectorNodeSnapshot>,
    root: &Element,
) -> Element {
    let selected_key = selected.map(|n| super::snapshot::path_to_key(&n.path));
    let hover_key_str = DevToolsState::hover_path_key();
    let hover_key = if hover_key_str.is_empty() {
        None
    } else {
        Some(hover_key_str)
    };
    let mut rows = Vec::new();
    for node in tree.iter().take(400) {
        rows.push(tree_row(
            tree,
            node,
            selected_key.as_deref(),
            hover_key.as_deref(),
        ));
    }

    let tree_el: Element =
        crate::primitives::interactive::interactive(vstack().w("100%").children(rows).into())
            .into();

    let details = details_pane(selected, root);

    card_stack([
        inspector_card("Elements", tree_el),
        inspector_card("Styles", details),
    ])
}

fn tree_row(
    tree: &[InspectorNodeSnapshot],
    node: &InspectorNodeSnapshot,
    selected_key: Option<&str>,
    hover_key: Option<&str>,
) -> Element {
    let path = node.path.clone();
    let key = super::snapshot::path_to_key(&path);
    let selected = selected_key == Some(key.as_str());
    let hovered = !selected && hover_key == Some(key.as_str());
    let indent = format!("{}rem", node.depth as f32 * 0.5);
    let caption = node_label(node);
    let path_click = path.clone();
    let path_hover = path.clone();
    let (identity_hover, relative_hover) = snapshot_hover_anchor(tree, node);
    let identity_click = identity_hover.clone();
    let relative_click = relative_hover.clone();
    // Snapshot border-box rejects drifted absolute paths (Outline vs 1b card).
    let size_hint = (node.box_model.width, node.box_model.height);
    let size_click = size_hint;
    let size_hover = size_hint;
    let bg = if selected {
        parse_color("accent")
    } else if hovered {
        parse_color("muted")
    } else {
        [0.0, 0.0, 0.0, 0.0]
    };
    let row: Element = hstack()
        .id(format!("dt-row-{key}"))
        .w("100%")
        .py("0.125rem")
        .pl(indent)
        .bg(bg)
        .children([label(format!("dt-lbl-{key}"))
            .text(caption)
            .text_size("0.75rem")
            .w("100%")
            .into()])
        .into();
    crate::primitives::interactive::interactive(row)
        .on_click(move || {
            DevToolsState::set_selected(
                path_click.clone(),
                identity_click.clone(),
                relative_click.clone(),
                size_click,
            );
        })
        .on_hover(move |on| {
            if on {
                DevToolsState::set_hover_highlight(
                    path_hover.clone(),
                    identity_hover.clone(),
                    relative_hover.clone(),
                    size_hover,
                );
            } else if !DevToolsState::is_pick() {
                // In pick mode the page pointer owns HOVER_PATH; clearing here would wipe
                // pick hover because tree rows share the same path keys as app nodes.
                DevToolsState::clear_hover_highlight_if_path(&path_hover);
            }
        })
        .into()
}

fn details_pane(selected: Option<&InspectorNodeSnapshot>, root: &Element) -> Element {
    let Some(node) = selected else {
        return Element::from(
            r#box().w("100%").flex_grow(1.0).p("0.75rem").child(
                label("dt-empty")
                    .text("Select a node or use Pick (Ctrl+Shift+C).")
                    .text_size("0.75rem"),
            ),
        );
    };

    let bm = &node.box_model;
    let st = &node.style;
    let path = node.path.clone();
    let source = node
        .template_id
        .as_deref()
        .and_then(source_location_from_template_id)
        .map(|l| l.display())
        .unwrap_or_else(|| "—".into());

    let mut fields: Vec<Element> = vec![
        box_model_diagram(bm, DevToolsState::highlight_layer()),
        kv("path", &super::snapshot::path_to_key(&node.path)),
        kv("focus_id", node.focus_id.as_deref().unwrap_or("—")),
        kv("template_id", node.template_id.as_deref().unwrap_or("—")),
        kv("source", &source),
        kv(
            "bounds",
            &format!(
                "{:.1} × {:.1} @ ({:.1},{:.1})",
                bm.width, bm.height, bm.x, bm.y
            ),
        ),
        kv(
            "padding",
            &format!(
                "T{:.1} R{:.1} B{:.1} L{:.1}",
                bm.padding_top, bm.padding_right, bm.padding_bottom, bm.padding_left
            ),
        ),
        kv(
            "border",
            &format!(
                "T{:.1} R{:.1} B{:.1} L{:.1}",
                bm.border_top, bm.border_right, bm.border_bottom, bm.border_left
            ),
        ),
        kv("bg", &st.bg),
        kv("opacity", &format!("{:.2}", st.opacity)),
        kv(
            "flex",
            &format!("grow {:.2} shrink {:.2}", st.flex_grow, st.flex_shrink),
        ),
    ];

    fields.push(section("CSSOM"));
    // Prefer the last Apply payload so a panel rebuild cannot reseat the textarea
    // from authored/rest chrome (which hides Inspector edits that live style still has).
    let cssom_seed = super::style_edit::committed_inspector_cssom(&path)
        .or_else(|| {
            super::snapshot::find_element_by_path(root, &path)
                .map(super::snapshot::cssom_text_from_element)
        })
        .unwrap_or_else(|| cssom_text_from_style(st));
    fields.push(cssom_editor(path, cssom_seed));

    Element::from(vstack().w("100%").children(fields))
}

fn layout_body(selected: Option<&InspectorNodeSnapshot>) -> Element {
    let Some(node) = selected else {
        return inspector_card(
            "Layout",
            Element::from(
                r#box().w("100%").p("0.75rem").child(
                    label("dt-lay-empty")
                        .text("Select a node to inspect layout.")
                        .text_size("0.75rem"),
                ),
            ),
        );
    };
    let st = &node.style;
    let bm = &node.box_model;
    let [sx, sy] = node.scroll_offset;
    let fields = vec![
        kv("display", &st.display),
        kv("flex-direction", &st.flex_direction),
        kv("position", &st.position),
        kv("overflow-x", &st.overflow_x),
        kv("overflow-y", &st.overflow_y),
        kv("z-index", &st.z_index.to_string()),
        kv("gap", &st.gap),
        kv("width", &st.width),
        kv("height", &st.height),
        kv("min-width", &st.min_width),
        kv("min-height", &st.min_height),
        kv(
            "layout_bounds",
            &format!(
                "{:.1},{:.1}  {:.1}×{:.1}",
                node.layout_bounds[0],
                node.layout_bounds[1],
                node.layout_bounds[2],
                node.layout_bounds[3]
            ),
        ),
        kv(
            "box",
            &format!(
                "{:.1}×{:.1} @ ({:.1},{:.1})",
                bm.width, bm.height, bm.x, bm.y
            ),
        ),
        kv("scroll_offset", &format!("x {:.1}  y {:.1}", sx, sy)),
        kv("child_count", &node.child_count.to_string()),
    ];
    inspector_card("Layout", Element::from(vstack().w("100%").children(fields)))
}

/// After CSSOM Apply, keep the textarea on the **applied source text**.
///
/// Do not reseat from [`super::snapshot::cssom_text_from_element`]: that dump uses
/// authored / Button rest chrome and can overwrite the declarations the user just
/// applied (looks like a revert) while live style stays correct via apply + reapply.
pub(crate) fn sync_inspector_applied_cssom(path: &[usize], applied_css: &str) {
    seed_edit_field(&cssom_input_id(path), applied_css, true);
}

fn apply_cssom(path: Vec<usize>, css: String) {
    super::queue_inspector_cssom(path, css);
}

fn cssom_input_id(path: &[usize]) -> String {
    format!("dt-cssom-{}", super::snapshot::path_to_key(path))
}

fn field_store_value(field_id: &str) -> String {
    use_signal(&input_value_signal_key(field_id), String::new).peek()
}

/// Seed the CSSOM textarea value **store** from the selected node.
///
/// Uncontrolled (no `value_signal`): only `set_silent` — panel build runs under
/// `AppState` borrow, so `Signal::set` must not flush effects.
fn seed_edit_field(field_id: &str, current: &str, multiline: bool) {
    crate::utils::text_field::unbind_controlled_value(field_id);
    let focused = use_signal("__rui_button_focus_id", String::new);
    let fid = focus_id_for_text_edit(field_id, multiline);
    if focused.peek() == fid {
        return;
    }
    let store = use_signal(&input_value_signal_key(field_id), String::new);
    if store.peek() != current {
        store.set_silent(current.to_string());
    }
}

fn kv(k: &str, v: &str) -> Element {
    Element::from(
        vstack().w("100%").px("0.75rem").py("0.2rem").children([
            label(format!("dt-k-{k}"))
                .text(k)
                .text_size("0.625rem")
                .color(parse_text_color("muted-foreground"))
                .into(),
            label(format!("dt-v-{k}"))
                .text(v)
                .text_size("0.75rem")
                .w("100%")
                .into(),
        ]),
    )
}

fn section(title: &str) -> Element {
    Element::from(
        r#box()
            .w("100%")
            .px("0.75rem")
            .pt("0.75rem")
            .pb("0.25rem")
            .child(
                label(format!("dt-sec-{title}"))
                    .text(title)
                    .text_size("0.75rem")
                    .font_weight("700"),
            ),
    )
}

fn cssom_editor(path: Vec<usize>, seed: String) -> Element {
    let field_id = cssom_input_id(&path);
    seed_edit_field(&field_id, &seed, true);
    let field_id_click = field_id.clone();
    Element::from(
        vstack()
            .w("100%")
            .px("0.75rem")
            .py("0.25rem")
            .gap("0.25rem")
            .children([
                {
                    // Long declarations stay on hard `\n` lines and scroll horizontally
                    // (wrap=off) — same path as gallery wrap=off + drag edge follow.
                    let mut editor: Element = textarea(&field_id)
                        .rows(18)
                        .resizable(false)
                        .wrap("off")
                        .into();
                    editor = editor.w("100%").min_w("0");
                    editor
                },
                button(format!(
                    "dt-cssom-apply-{}",
                    super::snapshot::path_to_key(&path)
                ))
                .size("sm")
                .text("Apply CSS")
                .on_click(move || {
                    apply_cssom(path.clone(), field_store_value(&field_id_click));
                })
                .into(),
            ]),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::tree::Element;

    #[test]
    fn inspector_seed_unbinds_stale_controlled_pairing() {
        use crate::core::reactivity::{create_root, use_signal};
        use crate::utils::text_field::{
            bind_controlled_value, input_value_signal_key, pull_controlled_into_store,
            unbind_controlled_value,
        };

        create_root(|| {
            let id = "dt-cssom-test-unbind";
            let external = use_signal("__rui_dt_cssom_test_unbind", || "from-external".into());
            bind_controlled_value(id, external);
            assert_eq!(
                use_signal(&input_value_signal_key(id), String::new).peek(),
                "from-external"
            );

            // Simulate Inspector switching to uncontrolled: drop pairing, seed store.
            unbind_controlled_value(id);
            use_signal(&input_value_signal_key(id), String::new).set_silent("from-store".into());

            // Paint sync must NOT pull the old external back over the store.
            let pulled = pull_controlled_into_store(id, "from-store", false);
            assert!(pulled.is_none());
            assert_eq!(
                use_signal(&input_value_signal_key(id), String::new).peek(),
                "from-store"
            );
        });
    }

    #[test]
    fn wrap_app_inserts_docked_panel_slot() {
        DevToolsState::enable_for_window();
        let root = wrap_app(Element::new().focus_id("app"));
        assert!(root
            .children
            .iter()
            .any(|c| c.template_id.as_deref() == Some(PANEL_SLOT_ID)));
        let host = root
            .children
            .iter()
            .find(|c| c.template_id.as_deref() == Some("__rui_app_host"))
            .expect("app host");
        assert!(host
            .children
            .iter()
            .any(|c| { c.template_id.as_deref() == Some("__rui_devtools_highlight") }));
    }

    #[test]
    fn sync_inspector_applied_cssom_keeps_applied_source() {
        use crate::core::reactivity::create_root;
        use crate::utils::text_field::input_value_signal_key;

        create_root(|| {
            let path = vec![0usize];
            let path_key = super::super::snapshot::path_to_key(&path);
            let field = format!("dt-cssom-{path_key}");
            let applied = "opacity: 0.25;\npadding: 20px;\n";
            use_signal(&input_value_signal_key(&field), String::new)
                .set_silent("opacity: 1.00;\n".into());

            // Simulate post-Apply: store must keep the applied source, not an
            // authored dump that can hide live edits (Button rest chrome, etc.).
            sync_inspector_applied_cssom(&path, applied);

            let cssom = use_signal(&input_value_signal_key(&field), String::new).peek();
            assert_eq!(cssom, applied);
        });
    }

    #[test]
    fn cssom_editor_uses_nowrap_horizontal_scroll() {
        use crate::components::input::input_text_template_id;
        use crate::core::reactivity::create_root;
        use crate::style::WhiteSpace;

        create_root(|| {
            let path = vec![0usize];
            let field = cssom_input_id(&path);
            let tid = input_text_template_id(&field);
            let el = cssom_editor(
                path,
                "padding: 8px 16px; background: #ffffff; border-radius: 6px".into(),
            );
            fn find_tid<'a>(el: &'a Element, tid: &str) -> Option<&'a Element> {
                if el.template_id.as_deref() == Some(tid) {
                    return Some(el);
                }
                el.children.iter().find_map(|c| find_tid(c, tid))
            }
            let text = find_tid(&el, &tid).expect("cssom text leaf");
            assert_eq!(
                text.text_style.white_space,
                WhiteSpace::Nowrap,
                "CSSOM editor must wrap=off (horizontal scroll, not soft wrap)"
            );
        });
    }

    #[test]
    fn cssom_seed_prefers_committed_apply_text() {
        use crate::core::reactivity::create_root;
        use crate::core::tree::ButtonHoverPaint;
        use crate::style::parse_color;
        use crate::utils::text_field::input_value_signal_key;

        create_root(|| {
            super::super::style_edit::clear_committed_inspector_edits_for_test();
            let path = vec![0usize];
            let applied = "background: #00ff00;\nopacity: 0.5;\n";
            super::super::style_edit::commit_inspector_cssom(&path, applied.into());

            // Live style has the apply; authored button dump would show ghost transparent.
            let mut el = Element::new();
            el.button_hover_paint = Some(ButtonHoverPaint {
                variant: "ghost".into(),
                disabled: false,
                hover_easing: "spring".into(),
                text_color: None,
            });
            el.style.bg_color = parse_color("#00ff00");
            el.style.opacity = 0.5;
            let root = Element::new().child(el);
            let authored =
                super::super::snapshot::cssom_text_from_element(root.children.first().unwrap());
            assert_ne!(
                authored, applied,
                "authored dump must differ from applied text to prove preference"
            );

            let seed = super::super::style_edit::committed_inspector_cssom(&path)
                .or_else(|| {
                    super::super::snapshot::find_element_by_path(&root, &path)
                        .map(super::super::snapshot::cssom_text_from_element)
                })
                .expect("seed");
            assert_eq!(seed, applied);

            let field = format!("dt-cssom-{}", super::super::snapshot::path_to_key(&path));
            seed_edit_field(&field, &seed, true);
            assert_eq!(
                use_signal(&input_value_signal_key(&field), String::new).peek(),
                applied
            );
            super::super::style_edit::clear_committed_inspector_edits_for_test();
        });
    }
}
