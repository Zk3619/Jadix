//! Inspector dock + highlight overlay (P0/P1/P2).
//!
//! Overlay is a **placeholder** in the retained tree (`wrap_app`). Content is injected
//! from `AppState::render` via [`sync_overlay`] — not a `reactive_view` — so F12 / Pick
//! never flush render effects while `AppState` is borrowed.

use crate::app::runtime;
use crate::components::{
    button, input, label, scroll_area, textarea, Card, CardContent, CardContentProps, CardHeader,
    CardHeaderProps, CardProps, CardTitle, CardTitleProps,
};
use crate::core::reactivity::use_signal;
use crate::core::tree::Element;
use crate::primitives::{hstack, r#box, vstack};
use crate::style::{parse_color, parse_text_color};
use crate::utils::{build_icon_text_node, lucide_glyph};
use crate::utils::text_field::{focus_id_for_text_edit, input_value_signal_key};

use super::overlay::{box_model_diagram, highlight_slot};
use super::reactivity_panel::build_reactivity_panel;
use super::snapshot::{
    cssom_text_from_style, find_element_by_template_mut, node_label, snapshot_subtree,
    snapshot_tree, source_location_from_template_id, InspectorNodeSnapshot, StyleSnapshot,
};
use super::state::{DevToolsState, DevToolsTab};
use super::style_edit::StylePatch;

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

    let mut panel_slot = Element::from(
        r#box()
            .h("100%")
            .w(0.0)
            .flex_shrink(0.0)
            .overflow("hidden"),
    );
    panel_slot.template_id = Some(PANEL_SLOT_ID.into());

    Element::from(
        hstack()
            .w("100%")
            .h("100%")
            .children([host, panel_slot]),
    )
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
            tab_btn("dt-tab-el", "Elements", tab == DevToolsTab::Elements, DevToolsTab::Elements),
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
        DevToolsTab::Elements => elements_body(tree, selected),
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

fn elements_body(tree: &[InspectorNodeSnapshot], selected: Option<&InspectorNodeSnapshot>) -> Element {
    let selected_key = selected.map(|n| super::snapshot::path_to_key(&n.path));
    let hover_key_str = DevToolsState::hover_path_key();
    let hover_key = if hover_key_str.is_empty() {
        None
    } else {
        Some(hover_key_str)
    };
    let mut rows = Vec::new();
    for node in tree.iter().take(400) {
        rows.push(tree_row(node, selected_key.as_deref(), hover_key.as_deref()));
    }

    let tree_el: Element = crate::primitives::interactive::interactive(
        vstack()
            .w("100%")
            .children(rows)
            .into(),
    )
    .into();

    let details = details_pane(selected);

    card_stack([
        inspector_card("Elements", tree_el),
        inspector_card("Styles", details),
    ])
}

fn tree_row(
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
            DevToolsState::set_selected_path(path_click.clone());
        })
        .on_hover(move |on| {
            if on {
                DevToolsState::set_hover_highlight_path(path_hover.clone());
            } else {
                DevToolsState::clear_hover_highlight_if_path(&path_hover);
            }
        })
        .into()
}

fn details_pane(selected: Option<&InspectorNodeSnapshot>) -> Element {
    let Some(node) = selected else {
        return Element::from(
            r#box()
                .w("100%")
                .flex_grow(1.0)
                .p("0.75rem")
                .child(
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

    fields.push(section("Live edit (whitelist)"));
    let path_key = super::snapshot::path_to_key(&path);
    fields.push(edit_row(
        "pad",
        &path_key,
        "padding px",
        format!("{:.0}", bm.padding_top),
        {
            let path = path.clone();
            move |v| apply_patch(path.clone(), StylePatch::PaddingUniform(v))
        },
    ));
    fields.push(edit_row(
        "w",
        &path_key,
        "width px",
        parse_px_hint(&st.width),
        {
            let path = path.clone();
            move |v| apply_patch(path.clone(), StylePatch::WidthPx(v))
        },
    ));
    fields.push(edit_row(
        "h",
        &path_key,
        "height px",
        parse_px_hint(&st.height),
        {
            let path = path.clone();
            move |v| apply_patch(path.clone(), StylePatch::HeightPx(v))
        },
    ));
    fields.push(edit_row(
        "op",
        &path_key,
        "opacity 0–1",
        format!("{:.2}", st.opacity),
        {
            let path = path.clone();
            move |v| apply_patch(path.clone(), StylePatch::Opacity(v))
        },
    ));
    fields.push(token_row(path.clone(), &path_key, st));
    fields.push(section("CSSOM"));
    fields.push(cssom_editor(path, st));

    Element::from(
        vstack()
            .w("100%")
            .children(fields),
    )
}

fn layout_body(selected: Option<&InspectorNodeSnapshot>) -> Element {
    let Some(node) = selected else {
        return inspector_card(
            "Layout",
            Element::from(
                r#box()
                    .w("100%")
                    .p("0.75rem")
                    .child(
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
            &format!("{:.1}×{:.1} @ ({:.1},{:.1})", bm.width, bm.height, bm.x, bm.y),
        ),
        kv("scroll_offset", &format!("x {:.1}  y {:.1}", sx, sy)),
        kv("child_count", &node.child_count.to_string()),
    ];
    inspector_card(
        "Layout",
        Element::from(vstack().w("100%").children(fields)),
    )
}

fn parse_px_hint(s: &str) -> String {
    s.trim_end_matches("px").trim().to_string()
}

fn apply_patch(path: Vec<usize>, patch: StylePatch) {
    runtime::patch_devtools_style(&path, patch);
    runtime::request_redraw();
}

fn apply_cssom(path: Vec<usize>, css: String) {
    if let Some(dump) = runtime::patch_devtools_cssom(&path, &css) {
        let key = cssom_signal_key(&path);
        use_signal(&key, String::new).set(dump);
    }
    runtime::request_redraw();
}

fn edit_signal_key(id: &str, path_key: &str) -> String {
    format!("__rui_dt_edit_{id}_{path_key}")
}

fn edit_input_id(id: &str, path_key: &str) -> String {
    format!("dt-in-{id}-{path_key}")
}

fn cssom_signal_key(path: &[usize]) -> String {
    format!(
        "__rui_dt_cssom_{}",
        super::snapshot::path_to_key(path)
    )
}

fn cssom_input_id(path: &[usize]) -> String {
    format!(
        "dt-cssom-{}",
        super::snapshot::path_to_key(path)
    )
}

/// Keep inspector edit fields in sync with the selected node without stealing focus.
fn seed_edit_field(signal_key: &str, field_id: &str, current: &str) {
    let buf = use_signal(signal_key, || current.to_string());
    let focused = use_signal("__rui_button_focus_id", String::new);
    let fid = focus_id_for_text_edit(field_id, false);
    if focused.peek() == fid {
        return;
    }
    if buf.peek() != current {
        buf.set(current.to_string());
        let store = use_signal(&input_value_signal_key(field_id), String::new);
        if store.peek() != current {
            store.set_silent(current.to_string());
        }
    }
}

fn kv(k: &str, v: &str) -> Element {
    Element::from(
        vstack()
            .w("100%")
            .px("0.75rem")
            .py("0.2rem")
            .children([
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

fn edit_row(
    id: &str,
    path_key: &str,
    caption: &str,
    current: String,
    on_apply: impl Fn(f32) + Clone + 'static,
) -> Element {
    let signal_key = edit_signal_key(id, path_key);
    let field_id = edit_input_id(id, path_key);
    seed_edit_field(&signal_key, &field_id, &current);
    let buf = use_signal(&signal_key, move || current.clone());
    let apply = on_apply.clone();
    Element::from(
        hstack()
            .w("100%")
            .px("0.75rem")
            .py("0.2rem")
            .gap("0.25rem")
            .items("center")
            .children([
                label(format!("dt-ed-{id}-{path_key}"))
                    .text(caption)
                    .text_size("0.6875rem")
                    .w("6.5rem")
                    .into(),
                input(&field_id)
                    .value_signal(buf)
                    .w("100%")
                    .into(),
                button(format!("dt-apply-{id}-{path_key}"))
                    .size("sm")
                    .text("Set")
                    .on_click(move || {
                        if let Ok(v) = buf.peek().trim().parse::<f32>() {
                            apply(v);
                        }
                    })
                    .into(),
            ]),
    )
}

fn token_row(path: Vec<usize>, path_key: &str, st: &StyleSnapshot) -> Element {
    let signal_key = edit_signal_key("bg", path_key);
    let field_id = edit_input_id("bg", path_key);
    seed_edit_field(&signal_key, &field_id, &st.bg);
    let buf = use_signal(&signal_key, || st.bg.clone());
    Element::from(
        hstack()
            .w("100%")
            .px("0.75rem")
            .py("0.2rem")
            .gap("0.25rem")
            .items("center")
            .children([
                label(format!("dt-ed-bg-{path_key}"))
                    .text("bg token")
                    .text_size("0.6875rem")
                    .w("6.5rem")
                    .into(),
                input(&field_id).value_signal(buf).w("100%").into(),
                button(format!("dt-apply-bg-{path_key}"))
                    .size("sm")
                    .text("Set")
                    .on_click(move || {
                        let token = buf.peek();
                        apply_patch(path.clone(), StylePatch::BackgroundToken(token));
                    })
                    .into(),
            ]),
    )
}

fn cssom_editor(path: Vec<usize>, st: &StyleSnapshot) -> Element {
    let seed = cssom_text_from_style(st);
    let key = cssom_signal_key(&path);
    let field_id = cssom_input_id(&path);
    seed_edit_field(&key, &field_id, &seed);
    let buf = use_signal(&key, move || seed.clone());
    Element::from(
        vstack()
            .w("100%")
            .px("0.75rem")
            .py("0.25rem")
            .gap("0.25rem")
            .children([
                {
                    let mut editor: Element = textarea(&field_id)
                        .rows(10)
                        .resizable(false)
                        .value_signal(buf)
                        .into();
                    editor = editor.w("100%");
                    editor
                },
                button(format!("dt-cssom-apply-{}", super::snapshot::path_to_key(&path)))
                    .size("sm")
                    .text("Apply CSS")
                    .on_click(move || {
                        apply_cssom(path.clone(), buf.peek());
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
        assert!(host.children.iter().any(|c| {
            c.template_id.as_deref() == Some("__rui_devtools_highlight")
        }));
    }
}
