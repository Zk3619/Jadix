//! Inspector dock + highlight overlay (P0/P1/P2).
//!
//! Overlay is a **placeholder** in the retained tree (`wrap_app`). Content is injected
//! from `AppState::render` via [`sync_overlay`] — not a `reactive_view` — so F12 / Pick
//! never flush render effects while `AppState` is borrowed.

use crate::app::runtime;
use super::controls::{button, input, label, scroll_area, textarea};
use crate::core::reactivity::use_signal;
use crate::core::tree::{Element, LayoutStatus};
use crate::primitives::{hstack, r#box, vstack};
use crate::style::{
    collect_matching_rules, parse_color, parse_text_color, soft_semantic_fill,
};
use crate::utils::text_field::{focus_id_for_text_edit, input_value_signal_key};
use crate::utils::{build_icon_text_node, lucide_glyph};

use super::locale::text;
use super::overlay::{box_model_diagram, highlight_slot};
use super::reactivity_panel::build_reactivity_panel;
use super::snapshot::{
    cssom_text_from_style, find_element_by_path, find_element_by_template_mut, node_label,
    snapshot_hover_anchor, snapshot_subtree, snapshot_tree, source_location_from_template_id,
    InspectorNodeSnapshot,
};
use super::state::{DevToolsState, DevToolsTab};

const PANEL_ID: &str = "__jadix_devtools_panel";
pub(crate) const PANEL_SLOT_ID: &str = "__jadix_devtools_panel_slot";

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
    host.template_id = Some("__jadix_app_host".into());

    let mut panel_slot =
        Element::from(r#box().h("100%").w(0.0).flex_shrink(0.0).overflow("hidden"));
    panel_slot.template_id = Some(PANEL_SLOT_ID.into());

    Element::from(hstack().w("100%").h("100%").children([host, panel_slot]))
}

/// Rebuild the docked panel when Inspector chrome generation changes.
pub(crate) fn sync_overlay_layout(
    root: &mut Element,
    tree: &mut crate::core::tree::LayoutTree,
) -> LayoutStatus {
    super::locale::sync();
    let preview_impact = super::device::sync(root, tree);
    let panel_impact = sync_overlay(root);
    if matches!(panel_impact, LayoutStatus::LayoutChanged | LayoutStatus::StructureChanged) {
        if let Some(slot) = find_element_by_template_mut(root, PANEL_SLOT_ID) {
            tree.mark_element_layout_dirty(slot);
        }
    }
    let resize_changed = super::presentation::sync_resize_hover(root);
    preview_impact.merge(panel_impact).merge(if resize_changed {
        LayoutStatus::StyleChanged
    } else { LayoutStatus::Clean })
}

pub fn sync_overlay(root: &mut Element) -> LayoutStatus {
    if !DevToolsState::is_enabled() { return LayoutStatus::Clean; }
    let gen = DevToolsState::generation();
    if gen == DevToolsState::last_sync_gen() { return LayoutStatus::Clean; }
    super::presentation::sync(root);
    let open = DevToolsState::is_open() && !super::presentation::is_detached();
    let mounted = super::snapshot::find_element_by_template(root, PANEL_ID).is_some();
    let mut impact = LayoutStatus::Clean;
    if open && !mounted {
        let dock = dock_panel_from_root(root);
        if let Some(slot) = find_element_by_template_mut(root, PANEL_SLOT_ID) {
            slot.style.size.width = taffy::style::Dimension::length(DevToolsState::panel_width());
            slot.children.push(dock);
            impact = LayoutStatus::StructureChanged;
        }
    } else if !open {
        if let Some(slot) = find_element_by_template_mut(root, PANEL_SLOT_ID) {
            if mounted || slot.style.size.width != taffy::style::Dimension::length(0.0) {
                slot.style.size.width = taffy::style::Dimension::length(0.0);
                slot.children.clear();
                impact = LayoutStatus::StructureChanged;
            }
        }
    } else {
        let revisions = DevToolsState::revisions();
        let panel = super::snapshot::find_element_by_template(root, PANEL_ID).unwrap();
        let changed = |index: usize| panel.attrs.get(&format!("data-dt-revision-{index}"))
            != Some(&revisions[index].to_string());
        let body_changed = changed(0);
        let outline_changed = changed(1);
        let details_changed = changed(2);
        let locale_changed = panel.attrs.get("data-dt-locale") != Some(&crate::chrome_locale());
        let tab = DevToolsState::active_tab();
        if body_changed || details_changed || (tab == DevToolsTab::Elements && outline_changed) {
            let path = DevToolsState::selected_path();
            let tree = snapshot_tree(root);
            let selected = if DevToolsState::selected_path_key().is_empty() { None }
                else { tree.iter().find(|node| node.path == path).cloned().or_else(|| snapshot_subtree(root, &path)) };
            if body_changed || tab != DevToolsTab::Elements {
                let body = panel_body(tab, &tree, selected.as_ref(), root, &path, DevToolsState::panel_width());
                replace_panel_region(root, "dt-body", body);
            } else {
                if outline_changed {
                    let outline = tree_outline(&tree, selected.as_ref());
                    replace_panel_region(root, "dt-outline", outline);
                } else if details_changed {
                    let crumbs = breadcrumbs(&tree, selected.as_ref());
                    replace_panel_region(root, "dt-breadcrumbs", crumbs);
                }
                if details_changed {
                    let details = inspector_card("Properties", details_pane(selected.as_ref(), root))
                        .template_id("dt-details").border_left(parse_color("border"), 1.0);
                    replace_panel_region(root, "dt-details", details);
                }
            }
            impact = LayoutStatus::StructureChanged;
            super::overlay::reset_box_model_visual_cache();
            super::overlay::reset_tree_row_hover_cache();
        }
        if locale_changed {
            replace_panel_region(root, "dt-header", panel_header(DevToolsState::is_pick()));
            let tabs = panel_tabs(tab);
            replace_panel_region(root, "dt-tabs", tabs);
            impact = LayoutStatus::StructureChanged;
        } else {
            for (id, selected) in [("dt-pick", DevToolsState::is_pick()), ("dt-device", super::device::enabled())] {
                let fresh: Element = button(id).size("icon-sm").variant(if selected { "secondary" } else { "ghost" }).into();
                impact = impact.merge(patch_control_chrome(root, id, fresh));
            }
            for (id, title, candidate) in [("dt-tab-el", "Elements", DevToolsTab::Elements), ("dt-tab-src", "Sources", DevToolsTab::Sources), ("dt-tab-lay", "Layout", DevToolsTab::Layout), ("dt-tab-rx", "Reactivity", DevToolsTab::Reactivity), ("dt-tab-changes", "Changes", DevToolsTab::Changes)] {
                let fresh = tab_btn(id, title, tab == candidate, candidate);
                impact = impact.merge(patch_control_chrome(root, id, fresh));
            }
        }
        let width = taffy::style::Dimension::length(DevToolsState::panel_width());
        for id in [PANEL_SLOT_ID, PANEL_ID] {
            if let Some(element) = find_element_by_template_mut(root, id) {
                if element.style.size.width != width {
                    element.style.size.width = width;
                    impact = impact.merge(LayoutStatus::LayoutChanged);
                }
            }
        }
    }
    if let Some(panel) = find_element_by_template_mut(root, PANEL_ID) { stamp_panel_revisions(panel); }
    DevToolsState::mark_synced(gen);
    impact
}

fn stamp_panel_revisions(panel: &mut Element) {
    for (index, revision) in DevToolsState::revisions().into_iter().enumerate() {
        panel.set_attr(format!("data-dt-revision-{index}"), revision.to_string());
    }
    panel.set_attr("data-dt-locale", crate::chrome_locale());
}

fn replace_panel_region(root: &mut Element, id: &str, replacement: Element) {
    if let Some(element) = find_element_by_template_mut(root, id) {
        crate::app::damage::push_frame_region_paint(crate::app::damage::paint_element_key(element),
            crate::core::tree::visual_paint_aabb(element, element.paint_scroll_acc));
        *element = replacement;
    }
}

fn patch_control_chrome(root: &mut Element, id: &str, fresh: Element) -> LayoutStatus {
    let Some(element) = find_element_by_template_mut(root, id) else {
        return LayoutStatus::Clean;
    };
    let fresh_caption_color = fresh.children.first().and_then(|child| child.text_style.color);
    let current_caption_color = element.children.first().and_then(|child| child.text_style.color);
    let aria_changed = element.attrs.get("aria-selected") != fresh.attrs.get("aria-selected");
    let style_changed = element.style.bg_color != fresh.style.bg_color
        || element.style.border_color != fresh.style.border_color
        || element.style.border_width != fresh.style.border_width
        || element.text_style.color != fresh.text_style.color
        || current_caption_color != fresh_caption_color;
    if !style_changed && !aria_changed {
        return LayoutStatus::Clean;
    }
    element.style.bg_color = fresh.style.bg_color;
    element.style.border_color = fresh.style.border_color;
    element.style.border_width = fresh.style.border_width;
    element.text_style.color = fresh.text_style.color;
    element.class_set = fresh.class_set.clone();
    if let Some(value) = fresh.attrs.get("aria-selected") {
        element.set_attr("aria-selected", value);
    }
    if let (Some(caption), Some(new_caption)) = (element.children.first_mut(), fresh.children.first()) {
        caption.text_style.color = new_caption.text_style.color;
    }
    crate::app::damage::push_frame_paint_element(element, element.paint_scroll_acc);
    LayoutStatus::StyleChanged
}

pub(crate) fn dock_panel_from_root(root: &Element) -> Element {
    let tab = DevToolsState::active_tab();
    let path = DevToolsState::selected_path();
    let pick = DevToolsState::is_pick();
    let panel_w = super::presentation::panel_width();
    let tree_snap = if tab == DevToolsTab::Elements {
        snapshot_tree(root)
    } else {
        Vec::new()
    };
    let selected = if DevToolsState::selected_path_key().is_empty() {
        None
    } else {
        tree_snap
            .iter()
            .find(|node| node.path == path)
            .cloned()
            .or_else(|| snapshot_subtree(root, &path))
    };
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
    let header = panel_header(pick);
    let tabs = panel_tabs(tab);
    let body = panel_body(tab, tree, selected, root, path, panel_w);
    let status = if cfg!(all(debug_assertions, feature = "hot-reload")) {
        "HMR enabled  ·  F12 toggle  ·  Ctrl+Shift+C pick"
    } else {
        "F12 toggle  ·  Ctrl+Shift+C pick"
    };
    let footer = Element::from(
        hstack().w("100%").h(24.0).items("center").child(
            label("dt-status")
                .text(text(status))
                .text_size(10.0)
                .text_color(parse_text_color("muted-foreground")),
        ),
    )
    .w("100%")
    .h(24.0)
    .px(8.0)
    .flex_shrink(0.0)
    .border_top(parse_color("border"), 1.0)
    .template_id("__jadix_devtools_status");
    let mut panel = Element::from(
        vstack()
            .w(panel_w)
            .h("100%")
            .min_h(0.0)
            .bg(parse_color("background"))
            .children([header, tabs, body, footer]),
    )
    .border_left(parse_color("border"), 1.0);
    if !super::presentation::is_detached() {
        panel = panel.relative().child(super::presentation::resize_handle());
    }
    panel.template_id = Some(PANEL_ID.into());
    stamp_panel_revisions(&mut panel);
    panel
}

fn panel_header(pick: bool) -> Element {
    let pick_icon = build_icon_text_node(
        lucide_glyph("square-dashed-mouse-pointer").unwrap_or_else(|| "⌖".into()),
    )
    .text_size(16.0);
    let header: Element = hstack()
        .w("100%")
        .items("center")
        .gap(2.0)
        .px(6.0)
        .h(34.0)
        .flex_shrink(0.0)
        .bg(parse_color("background"))
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
            button("dt-device")
                .size("icon-sm")
                .variant(if super::device::enabled() {
                    "secondary"
                } else {
                    "ghost"
                })
                .child(
                    build_icon_text_node(
                        lucide_glyph("monitor-smartphone").unwrap_or_else(|| "▣".into()),
                    )
                    .text_size(16.0),
                )
                .on_click(super::device::toggle)
                .into(),
            Element::from(
                label("dt-title")
                    .text("DevTools")
                    .text_size(12.0)
                    .font_weight("600"),
            )
            .flex_grow(1.0),
            button("dt-language")
                .size("icon-sm")
                .w(30.0)
                .h(30.0)
                .p(0.0)
                .variant("ghost")
                .text(if crate::chrome_locale().starts_with("zh") {
                    "EN"
                } else {
                    "中"
                })
                .on_click(|| {
                    crate::set_chrome_locale(if crate::chrome_locale().starts_with("zh") {
                        "en-US"
                    } else {
                        "zh-CN"
                    });
                    DevToolsState::bump();
                })
                .into(),
            button("dt-narrow")
                .size("icon-sm")
                .variant("ghost")
                .text("−")
                .on_click(|| {
                    DevToolsState::set_panel_width(DevToolsState::panel_width() - 80.0);
                    runtime::request_redraw();
                })
                .into(),
            button("dt-widen")
                .size("icon-sm")
                .variant("ghost")
                .text("+")
                .on_click(|| {
                    DevToolsState::set_panel_width(DevToolsState::panel_width() + 80.0);
                    runtime::request_redraw();
                })
                .into(),
            button("dt-detach")
                .size("icon-sm")
                .variant("ghost")
                .child(
                    build_icon_text_node(
                        lucide_glyph(if super::presentation::is_detached() {
                            "panel-right-close"
                        } else {
                            "external-link"
                        })
                        .unwrap_or_else(|| ">".into()),
                    )
                    .text_size(15.0),
                )
                .on_click(super::presentation::toggle_detached)
                .into(),
            button("dt-close")
                .size("icon-sm")
                .variant("ghost")
                .child(
                    build_icon_text_node(lucide_glyph("x").unwrap_or_else(|| "×".into()))
                        .text_size(15.0),
                )
                .on_click(|| {
                    DevToolsState::set_open(false);
                    runtime::request_redraw();
                })
                .into(),
        ])
        .into();
    header.template_id("dt-header")
}

fn panel_tabs(tab: DevToolsTab) -> Element {
    let tabs: Element = scroll_area("dt-tabs")
        .w("100%")
        .h(34.0)
        .scrollbar_visibility("never")
        .flex_shrink(0.0)
        .overflow_x("auto")
        .overflow_y("hidden")
        .child(
            hstack()
                .h(32.0)
                .px(4.0)
                .gap(0.0)
                .bg(parse_color("muted"))
                .children([
                    tab_btn(
                        "dt-tab-el",
                        "Elements",
                        tab == DevToolsTab::Elements,
                        DevToolsTab::Elements,
                    ),
                    tab_btn(
                        "dt-tab-src",
                        "Sources",
                        tab == DevToolsTab::Sources,
                        DevToolsTab::Sources,
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
                    tab_btn(
                        "dt-tab-changes",
                        "Changes",
                        tab == DevToolsTab::Changes,
                        DevToolsTab::Changes,
                    ),
                ]),
        )
        .into();
    tabs
}

fn panel_body(tab: DevToolsTab, tree: &[InspectorNodeSnapshot], selected: Option<&InspectorNodeSnapshot>, root: &Element, path: &[usize], panel_w: f32) -> Element {
    let body = match tab {
        DevToolsTab::Elements => elements_body(tree, selected, root, panel_w),
        DevToolsTab::Layout => card_stack([layout_body(selected)]),
        DevToolsTab::Reactivity => card_stack([inspector_card(
            "Signal graph",
            build_reactivity_panel(Some(root), path),
        )]),
        DevToolsTab::Sources => sources_body(root, path),
        DevToolsTab::Changes => changes_body(),
    }
    .flex_grow(1.0)
    .flex_basis(0.0)
    .min_h(0.0)
    .overflow("hidden");
    body.template_id("dt-body")
}

fn keyboard_activatable(element: Element) -> Element {
    let activate = element.on_click.clone();
    element.focusable(true).on_key_down(move |event| {
        if event.state == winit::event::ElementState::Pressed
            && !event.repeat
            && (matches!(
                event.logical_key,
                winit::keyboard::Key::Named(
                    winit::keyboard::NamedKey::Enter
                )
            ) || matches!(&event.logical_key, winit::keyboard::Key::Character(value) if value == " "))
        {
            if let Some(activate) = &activate {
                activate.borrow_mut()();
            }
            true
        } else {
            false
        }
    })
}

fn inspector_tab(id: &str, caption: &str, active: bool, click: impl FnMut() + 'static) -> Element {
    let mut tab = Element::from(
        hstack()
            .h(30.0)
            .px(10.0)
            .items("center")
            .bg(if active {
                soft_semantic_fill(parse_color("info"))
            } else {
                parse_color("muted")
            })
            .child(
                label(format!("{id}-label"))
                    .text(text(caption))
                    .text_size(12.0)
                    .text_color(parse_text_color(if active { "info" } else { "foreground" })),
            ),
    )
    .template_id(id)
    .focus_id(id)
    .flex_shrink(0.0)
    .cursor("pointer")
    .class(if active {
        "hover:bg-info/30 hover:text-info"
    } else {
        "hover:bg-accent hover:text-foreground"
    })
    .border_bottom(
        if active { parse_color("info") } else { [0.0; 4] },
        2.0,
    );
    tab.set_attr("role", "tab");
    tab.set_attr("aria-selected", if active { "true" } else { "false" });
    keyboard_activatable(tab.on_click(click))
}

fn tab_btn(id: &str, caption: &str, active: bool, tab: DevToolsTab) -> Element {
    inspector_tab(id, caption, active, move || DevToolsState::set_tab(tab))
}

fn inspector_card(title: &str, body: Element) -> Element {
    let slug = title.to_ascii_lowercase().replace(' ', "-");
    let header = Element::from(
        hstack()
            .w("100%")
            .h(30.0)
            .items("center")
            .justify("center")
            .child(
                label(format!("dt-card-{slug}"))
                    .text(text(title))
                    .w("100%")
                    .text_align("center")
                    .text_size(11.0)
                    .font_weight("600"),
            ),
    )
    .w("100%")
    .px(8.0)
    .h(30.0)
    .flex_items("center")
    .justify("center")
    .bg(parse_color("muted"))
    .flex_shrink(0.0)
    .border_bottom(parse_color("border"), 1.0);
    let scroll: Element = scroll_area(&format!("devtools-card-{slug}"))
        .w("100%")
        .min_h(0.0)
        .flex_grow(1.0)
        .overflow_x("auto")
        .overflow_y("auto")
        .child(body)
        .into();
    Element::from(
        vstack()
            .w("100%")
            .min_w(0.0)
            .min_h(0.0)
            .flex_grow(1.0)
            .flex_basis(0.0)
            .overflow("hidden")
            .children([header, scroll.flex_basis(0.0)]),
    )
    .overflow("hidden")
}

fn card_stack(cards: impl IntoIterator<Item = Element>) -> Element {
    Element::from(
        vstack()
            .w("100%")
            .flex_grow(1.0)
            .flex_basis(0.0)
            .min_h(0.0)
            .overflow("hidden")
            .children(cards),
    )
}

fn visible_tree<'a>(
    tree: &'a [InspectorNodeSnapshot],
    query: &str,
) -> Vec<&'a InspectorNodeSnapshot> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return tree
            .iter()
            .filter(|node| {
                !(0..node.path.len()).any(|depth| DevToolsState::is_collapsed(&node.path[..depth]))
            })
            .collect();
    }
    let mut matches = std::collections::HashSet::new();
    for node in tree {
        let haystack = format!(
            "{} {:?} {:?} {:?}",
            node_label(node),
            node.tag_hint,
            node.text_preview,
            node.template_id
        )
        .to_lowercase();
        if haystack.contains(&query) {
            for depth in 0..=node.path.len() {
                matches.insert(node.path[..depth].to_vec());
            }
        }
    }
    tree.iter()
        .filter(|node| matches.contains(&node.path))
        .collect()
}

fn elements_body(
    tree: &[InspectorNodeSnapshot],
    selected: Option<&InspectorNodeSnapshot>,
    root: &Element,
    panel_w: f32,
) -> Element {
    let outline = tree_outline(tree, selected);
    let details = inspector_card("Properties", details_pane(selected, root))
        .template_id("dt-details")
        .border_left(parse_color("border"), 1.0);
    if panel_w >= 600.0 {
        Element::from(hstack().w("100%").min_h(0.0).children([outline, details]))
    } else {
        card_stack([outline, details])
    }
}

fn tree_outline(tree: &[InspectorNodeSnapshot], selected: Option<&InspectorNodeSnapshot>) -> Element {
    let selected_key = selected.map(|node| super::snapshot::path_to_key(&node.path));
    let hover_key = DevToolsState::hover_path_key();
    let query = DevToolsState::tree_filter();
    seed_edit_field("dt-tree-query", &query, false);
    let filter = hstack()
        .w("100%")
        .gap(4.0)
        .p(4.0)
        .items("center")
        .children([
            input("dt-tree-query")
                .placeholder(text("Find tag, id, text, source"))
                .text_size(11.0)
                .h(28.0)
                .flex_grow(1.0)
                .into(),
            button("dt-tree-filter")
                .size("sm")
                .variant("ghost")
                .text(text("Find"))
                .on_click(|| {
                    DevToolsState::set_tree_filter(field_store_value("dt-tree-query"));
                })
                .into(),
            button("dt-tree-clear")
                .size("sm")
                .variant("ghost")
                .text(text("Clear"))
                .on_click(|| {
                    use_signal(&input_value_signal_key("dt-tree-query"), String::new)
                        .set_silent(String::new());
                    DevToolsState::set_tree_filter(String::new());
                })
                .into(),
        ])
        .into();
    let visible = visible_tree(tree, &query);
    let mut rows = Vec::new();
    let limit = visible
        .iter()
        .position(|node| {
            selected_key.as_deref() == Some(super::snapshot::path_to_key(&node.path).as_str())
        })
        .map_or(400, |index| (index + 1).max(400))
        .max(DevToolsState::tree_limit());
    let mut closing: Vec<&InspectorNodeSnapshot> = Vec::new();
    for node in visible.iter().take(limit) {
        while closing
            .last()
            .is_some_and(|parent| parent.depth >= node.depth)
        {
            rows.push(closing_tree_row(closing.pop().unwrap()));
        }
        rows.push(tree_row(
            tree,
            node,
            selected_key.as_deref(),
            Some(&hover_key),
        ));
        if node.child_count > 0 && !DevToolsState::is_collapsed(&node.path) {
            closing.push(node);
        }
    }
    while let Some(parent) = closing.pop() {
        rows.push(closing_tree_row(parent));
    }
    if visible.len() > limit {
        rows.push(
            button("dt-tree-more")
                .size("sm")
                .variant("ghost")
                .text(crate::tr_with_default_zh(
                    "jadix.devtools.tree.more",
                    "Show more ({count} remaining)",
                    "显示更多（剩余 {count} 个）",
                    &[("count", &(visible.len() - limit).to_string())],
                ))
                .on_click(move || {
                    DevToolsState::set_tree_limit(limit + 400);
                })
                .into(),
        );
    }
    if visible.is_empty() {
        rows.push(kv("No matches", &query));
    }
    let tree_el = Element::from(vstack().w("100%").children(rows));
    let outline = Element::from(
        vstack()
            .w("100%")
            .min_w(0.0)
            .min_h(0.0)
            .flex_grow(1.1)
            .flex_basis(0.0)
            .overflow("hidden")
            .children([
                filter,
                inspector_card("Element tree", tree_el),
                breadcrumbs(tree, selected),
            ]),
    );
    outline.template_id("dt-outline")
}

fn breadcrumbs(
    tree: &[InspectorNodeSnapshot],
    selected: Option<&InspectorNodeSnapshot>,
) -> Element {
    let mut crumbs = Vec::new();
    if let Some(selected) = selected {
        for node in tree
            .iter()
            .filter(|node| selected.path.starts_with(&node.path))
        {
            let path = node.path.clone();
            let (identity, relative) = snapshot_hover_anchor(tree, node);
            let size = (node.box_model.width, node.box_model.height);
            crumbs.push(
                button(format!("dt-crumb-{}", super::snapshot::path_to_key(&path)))
                    .size("sm")
                    .variant("ghost")
                    .text_size(11.0)
                    .text(
                        node.focus_id
                            .as_deref()
                            .or(node.tag_hint.as_deref())
                            .unwrap_or("node"),
                    )
                    .on_click(move || {
                        DevToolsState::set_selected(
                            path.clone(),
                            identity.clone(),
                            relative.clone(),
                            size,
                        );
                    })
                    .into(),
            );
        }
    }
    scroll_area("dt-breadcrumbs")
        .w("100%")
        .h(32.0)
        .scrollbar_visibility("never")
        .flex_shrink(0.0)
        .overflow_x("auto")
        .overflow_y("hidden")
        .child(hstack().children(crumbs))
        .into()
}

fn closing_tree_row(node: &InspectorNodeSnapshot) -> Element {
    let tag = node.tag_hint.as_deref().unwrap_or("box");
    let caption = Element::from(
        label(format!(
            "dt-closing-{}",
            super::snapshot::path_to_key(&node.path)
        ))
        .text(format!("</{tag}>"))
        .text_size(11.0)
        .text_overflow_ellipsis()
        .text_color(parse_text_color("muted-foreground")),
    )
    .font_family("Consolas")
    .min_w(0.0)
    .flex_grow(1.0)
    .whitespace_nowrap()
    .overflow_x("hidden");
    Element::from(
        hstack()
            .w("100%")
            .h(22.0)
            .flex_shrink(0.0)
            .pl(node.depth as f32 * 10.0 + 16.0)
            .items("center")
            .child(caption),
    )
    .font_family("Consolas")
    .bg(parse_color("background"))
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
    let indent = node.depth as f32 * 10.0;
    let caption = match (&node.tag_hint, &node.focus_id, &node.text_preview) {
        (Some(tag), _, Some(text)) => format!("<{tag}> {text} </{tag}>"),
        (Some(tag), Some(id), _) if node.child_count == 0 => format!("<{tag} id=\"{id}\" />"),
        (Some(tag), Some(id), _) if DevToolsState::is_collapsed(&path) => {
            format!("<{tag} id=\"{id}\"> … </{tag}>")
        }
        (Some(tag), Some(id), _) => format!("<{tag} id=\"{id}\">"),
        (Some(tag), _, _) if node.child_count == 0 => format!("<{tag} />"),
        (Some(tag), _, _) if DevToolsState::is_collapsed(&path) => format!("<{tag}> … </{tag}>"),
        (Some(tag), _, _) => format!("<{tag}>"),
        _ => node_label(node),
    };
    let fold_path = path.clone();
    let disclosure: Element = if node.child_count > 0 {
        let arrow = build_icon_text_node(
            lucide_glyph(if DevToolsState::is_collapsed(&path) {
                "chevron-right"
            } else {
                "chevron-down"
            })
            .unwrap_or_else(|| ">".into()),
        )
        .text_size(11.0);
        let fold = Element::from(
            hstack()
                .w(16.0)
                .h(22.0)
                .items("center")
                .justify("center")
                .child(arrow),
        )
        .focus_id(format!("dt-fold-{key}"))
        .flex_shrink(0.0);
        keyboard_activatable(fold.on_click(move || DevToolsState::toggle_collapsed(&fold_path)))
    } else {
        r#box().w(16.0).h(22.0).flex_shrink(0.0).into()
    };
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
        super::overlay::tree_row_color(true)
    } else if hovered {
        super::overlay::tree_row_color(false)
    } else {
        parse_color("background")
    };
    let text = Element::from(
        label(format!("dt-lbl-{key}"))
            .text(caption)
            .text_overflow_ellipsis()
            .text_color(parse_text_color(if selected {
                "accent-foreground"
            } else {
                "primary"
            }))
            .text_size(11.0),
    )
    .whitespace_nowrap()
    .overflow_x("hidden");
    let row: Element = hstack()
        .id(format!("dt-row-{key}"))
        .w("100%")
        .items("center")
        .h(22.0)
        .flex_shrink(0.0)
        .pl(indent)
        .bg(bg)
        .children([
            disclosure,
            text
            .font_family("Consolas")
            .min_w(0.0)
            .flex_grow(1.0),
        ])
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

fn source_for_path(root: &Element, path: &[usize]) -> Option<super::snapshot::SourceLocation> {
    for depth in (0..=path.len()).rev() {
        if let Some(element) = find_element_by_path(root, &path[..depth]) {
            for id in [
                element
                    .attrs
                    .get(crate::app::reactive::ATTR_SOURCE_TEMPLATE_ID)
                    .map(String::as_str),
                element.template_id.as_deref(),
            ] {
                if let Some(location) = id.and_then(source_location_from_template_id) {
                    return Some(location);
                }
            }
        }
    }
    None
}

fn sources_body(root: &Element, path: &[usize]) -> Element {
    if DevToolsState::selected_path_key().is_empty() {
        return card_stack([kv(
            "Sources",
            "Pick an element to inspect its nearest ui! source.",
        )]);
    }
    let Some(location) = source_for_path(root, path) else {
        return card_stack([kv(
            "Sources",
            "No source anchor on this node or its ancestors.",
        )]);
    };
    let (document, fresh) = match super::source::document(&location) {
        Ok(document) => document,
        Err(err) => return card_stack([kv("Source unavailable", &err)]),
    };
    let field_id = document.field_id();
    if fresh {
        seed_edit_field(&field_id, &document.text, true);
    }
    let save_document = document.clone();
    let save_field = field_id.clone();
    let reload_field = field_id.clone();
    let toolbar = hstack()
        .w("100%")
        .gap(4.0)
        .p(4.0)
        .children([
            button("dt-source-enable")
                .size("sm")
                .variant(if super::source::writes_enabled() {
                    "secondary"
                } else {
                    "ghost"
                })
                .disabled(!super::source::can_write())
                .text(text(if super::source::writes_enabled() {
                    "Source writes ON"
                } else {
                    "Enable source writes"
                }))
                .on_click(super::source::toggle_writes)
                .into(),
            button("dt-source-save")
                .size("sm")
                .text(text("Save file"))
                .disabled(!super::source::writes_enabled())
                .on_click(move || {
                    super::source::save(&save_document, field_store_value(&save_field));
                })
                .into(),
            button("dt-source-reload")
                .size("sm")
                .variant("ghost")
                .text(text("Reload from disk"))
                .on_click(move || match super::source::reload(&document) {
                    Ok(text) => {
                        use_signal(&input_value_signal_key(&reload_field), String::new)
                            .set_silent(text);
                        super::source::report(
                            "Reloaded source from disk; previous draft discarded.".into(),
                        );
                    }
                    Err(err) => super::source::report(err),
                })
                .into(),
        ])
        .into();
    let editor = Element::from(textarea(&field_id).rows(28).resizable(false).wrap("off"))
        .w("100%")
        .min_w(0.0)
        .font_family("Consolas");
    let body = Element::from(vstack().w("100%").p(8.0).gap(6.0).children([
        kv("Source", &location.display()),
        kv("Save policy", "Writes the full file after checking disk conflicts. Reload discards the draft."),
        kv("Hot reload", "Start jadix_hot_reload_cli separately. ui! text/styles patch live; Rust logic needs rebuilding."),
        toolbar, editor, kv("Last result", &super::source::status()),
    ]));
    card_stack([inspector_card("Sources", body)])
}

fn changes_body() -> Element {
    let mut rows = vec![button("dt-changes-clear")
        .size("sm")
        .variant("ghost")
        .text(text("Clear history"))
        .on_click(DevToolsState::clear_changes)
        .into()];
    let changes = DevToolsState::changes();
    if changes.is_empty() {
        rows.push(kv("Changes", "CSS applies, source saves and hot-reload batches appear here. Latest 128 entries retained."));
    }
    for (index, (kind, detail)) in changes.iter().enumerate() {
        rows.push(kv(
            &format!("{} · {}", text(kind), changes.len() - index),
            detail,
        ));
    }
    card_stack([inspector_card(
        "Changes",
        Element::from(vstack().w("100%").children(rows)),
    )])
}

fn computed_pane(node: &InspectorNodeSnapshot) -> Element {
    let style = &node.style;
    let mut properties = vec![
        ("display", style.display.clone()),
        ("position", style.position.clone()),
        ("width", format!("{:.2}px", node.box_model.width)),
        ("height", format!("{:.2}px", node.box_model.height)),
        ("min-width", style.min_width.clone()),
        ("min-height", style.min_height.clone()),
        ("padding", style.padding.clone()),
        ("margin", style.margin.clone()),
        ("background", style.bg.clone()),
        ("color", style.color_hint.clone()),
        ("opacity", format!("{}", style.opacity)),
        ("flex-direction", style.flex_direction.clone()),
        ("flex-grow", format!("{}", style.flex_grow)),
        ("flex-shrink", format!("{}", style.flex_shrink)),
        ("gap", style.gap.clone()),
        ("overflow-x", style.overflow_x.clone()),
        ("overflow-y", style.overflow_y.clone()),
        ("z-index", style.z_index.to_string()),
        ("border-width", format!("{}px", style.border_width)),
        ("border-color", style.border_color.clone()),
        ("border-radius", style.border_radius.clone()),
    ];
    properties.sort_by_key(|(name, _)| *name);
    Element::from(
        vstack()
            .w("100%")
            .children(properties.into_iter().map(|(name, value)| {
                Element::from(
                    hstack().w("100%").px(8.0).py(3.0).gap(8.0).children([
                        Element::from(
                            label(format!("dt-computed-{name}"))
                                .text(name)
                                .text_size(11.0)
                                .text_color(parse_text_color("#2563eb")),
                        )
                        .w("48%")
                        .flex_shrink(0.0),
                        Element::from(
                            label(format!("dt-computed-value-{name}"))
                                .text(value)
                                .text_size(11.0),
                        )
                        .min_w(0.0)
                        .flex_grow(1.0),
                    ]),
                )
                .font_family("Consolas")
            })),
    )
}

fn element_style_editor(path: &[usize], css: &str) -> Element {
    let key = super::snapshot::path_to_key(path);
    let declarations = super::style_edit::parse_declarations(css);
    let rows = declarations
        .iter()
        .map(|(property, value)| {
            let field = format!("dt-decl-{key}-{property}");
            seed_edit_field(&field, value, false);
            let field_click = field.clone();
            let path_click = path.to_vec();
            let property_click = property.clone();
            let fallback = css.to_string();
            let mut property_label = Element::from(
                label(format!("{field}-name"))
                    .text(format!("{property}:"))
                    .text_size(11.0)
                    .text_color(parse_text_color("#2563eb")),
            )
            .font_family("Consolas")
            .w("42%")
            .min_w(0.0);
            if property.contains("color") || property == "background" {
                property_label = Element::from(
                    hstack().w("42%").items("center").gap(4.0).children([
                        Element::new()
                            .w(10.0)
                            .h(10.0)
                            .bg(parse_color(value))
                            .border(parse_color("border"), 1.0),
                        property_label.flex_grow(1.0),
                    ]),
                );
            }
            Element::from(
                hstack()
                    .w("100%")
                    .min_h(28.0)
                    .items("center")
                    .gap(3.0)
                    .children([
                        property_label,
                        Element::from(input(&field).h(24.0).text_size(11.0))
                            .min_w(0.0)
                            .flex_grow(1.0),
                        button(format!("{field}-apply"))
                            .size("icon-sm")
                            .variant("ghost")
                            .child(
                                build_icon_text_node(
                                    lucide_glyph("check").unwrap_or_else(|| "+".into()),
                                )
                                .text_size(12.0),
                            )
                            .on_click(move || {
                                let current =
                                    super::style_edit::committed_inspector_cssom(&path_click)
                                        .unwrap_or_else(|| fallback.clone());
                                let mut declarations =
                                    super::style_edit::parse_declarations(&current);
                                let value = field_store_value(&field_click);
                                if let Some((_, old)) = declarations
                                    .iter_mut()
                                    .find(|(name, _)| name == &property_click)
                                {
                                    *old = value;
                                } else {
                                    declarations.push((property_click.clone(), value));
                                }
                                let css = declarations
                                    .iter()
                                    .map(|(name, value)| format!("{name}: {value};"))
                                    .collect::<Vec<_>>()
                                    .join("\n");
                                apply_cssom(path_click.clone(), css);
                            })
                            .into(),
                    ]),
            )
            .border_bottom(parse_color("border"), 0.5)
        })
        .collect::<Vec<_>>();
    Element::from(vstack().w("100%").px(8.0).py(4.0).children(rows))
}

fn property_tabs() -> Element {
    Element::from(hstack().w("100%").children([
        inspector_tab(
            "dt-style-tab",
            "Styles",
            !DevToolsState::show_computed(),
            || DevToolsState::set_computed(false),
        ),
        inspector_tab(
            "dt-computed-tab",
            "Computed",
            DevToolsState::show_computed(),
            || DevToolsState::set_computed(true),
        ),
    ]))
    .border_bottom(parse_color("border"), 1.0)
}

fn details_pane(selected: Option<&InspectorNodeSnapshot>, root: &Element) -> Element {
    let Some(node) = selected else {
        return Element::from(
            r#box().w("100%").flex_grow(1.0).p("0.75rem").child(
                label("dt-empty")
                    .text(text("Select a node or use Pick (Ctrl+Shift+C)."))
                    .text_size("0.75rem"),
            ),
        );
    };

    let bm = &node.box_model;
    let st = &node.style;
    let path = node.path.clone();
    if DevToolsState::show_computed() {
        return Element::from(vstack().w("100%").children([
            property_tabs(),
            box_model_diagram(bm, DevToolsState::highlight_layer()),
            computed_pane(node),
        ]));
    }
    let source = node
        .template_id
        .as_deref()
        .and_then(source_location_from_template_id)
        .map(|l| l.display())
        .unwrap_or_else(|| "—".into());

    let mut fields: Vec<Element> = vec![
        property_tabs(),
        section("Box model"),
        box_model_diagram(bm, DevToolsState::highlight_layer()),
        section("element.style"),
        kv(
            "Scope",
            "Selected element only; no global stylesheet changes.",
        ),
    ];
    // Prefer the last Apply payload so a panel rebuild cannot reseat the textarea
    // from authored/rest chrome (which hides Inspector edits that live style still has).
    let cssom_seed = super::style_edit::committed_inspector_cssom(&path)
        .or_else(|| {
            super::snapshot::find_element_by_path(root, &path)
                .map(super::snapshot::cssom_text_from_element)
        })
        .unwrap_or_else(|| cssom_text_from_style(st));
    fields.push(element_style_editor(&path, &cssom_seed));
    fields.push(section("Declarations"));
    fields.push(cssom_editor(path, cssom_seed));

    fields.push(section("Matched rules (read-only)"));
    if let Some(el) = find_element_by_path(root, &node.path) {
        let hits = collect_matching_rules(el);
        if hits.is_empty() {
            fields.push(kv("sheet", "no matching rules"));
        } else {
            for (i, hit) in hits.iter().enumerate() {
                let src = hit
                    .source
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "—".into());
                fields.push(kv(
                    &format!("rule-{i}"),
                    &format!(
                        "{} → {} (p={}, {src})",
                        hit.selector, hit.classes, hit.priority
                    ),
                ));
            }
        }
    } else {
        fields.push(kv("sheet", "—"));
    }

    fields.push(kv("Source", &source));
    Element::from(vstack().w("100%").children(fields))
}

fn layout_body(selected: Option<&InspectorNodeSnapshot>) -> Element {
    let Some(node) = selected else {
        return inspector_card(
            "Layout",
            Element::from(
                r#box().w("100%").p("0.75rem").child(
                    label("dt-lay-empty")
                        .text(text("Select a node to inspect layout."))
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
    let focused = use_signal("__jadix_button_focus_id", String::new);
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
                .text(text(k))
                .text_size("0.625rem")
                .text_color(parse_text_color("muted-foreground"))
                .into(),
            label(format!("dt-v-{k}"))
                .text(text(v))
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
            .h(32.0)
            .items("center")
            .justify("center")
            .bg(parse_color("muted"))
            .child(
                label(format!("dt-sec-{title}"))
                    .text(text(title))
                    .text_size("0.75rem")
                    .font_weight("700")
                    .w("100%")
                    .text_align("center"),
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
                .text(text("Apply CSS"))
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
    fn chrome_and_selection_updates_preserve_unaffected_controls() {
        crate::core::reactivity::create_root(|| {
            DevToolsState::enable_for_window();
            DevToolsState::set_open(true);
            DevToolsState::set_panel_width(640.0);
            DevToolsState::set_tab(DevToolsTab::Elements);
            DevToolsState::set_pick(false);
            DevToolsState::bump();
            let app = Element::new().w("100%").h("100%")
                .child(Element::new().template_id("scoped-selection-target").text("target"));
            let mut root = wrap_app(app);
            let mut layout = crate::core::tree::LayoutTree::new();
            sync_overlay_layout(&mut root, &mut layout);
            layout.build_and_layout(&mut root, 1400.0, 900.0, [-1.0; 2]);
            let query_id = crate::utils::text_field::input_text_template_id("dt-tree-query");
            let query_node = super::super::snapshot::find_element_by_template(&root, &query_id).unwrap().layout_node;
            let picker = super::super::snapshot::find_element_by_template(&root, "dt-pick").unwrap().on_click.clone().unwrap();
            DevToolsState::set_pick(true);
            assert_eq!(sync_overlay_layout(&mut root, &mut layout), LayoutStatus::StyleChanged);
            assert_eq!(super::super::snapshot::find_element_by_template(&root, &query_id).unwrap().layout_node, query_node);
            assert!(std::rc::Rc::ptr_eq(&picker, super::super::snapshot::find_element_by_template(&root, "dt-pick").unwrap().on_click.as_ref().unwrap()));
            DevToolsState::set_selected(vec![0, 0, 0], "scoped-selection-target", vec![], (0.0, 0.0));
            sync_overlay_layout(&mut root, &mut layout);
            assert_eq!(super::super::snapshot::find_element_by_template(&root, &query_id).unwrap().layout_node, query_node);
            layout.build_and_layout_fast(&mut root, 1400.0, 900.0, [-1.0; 2], false, false);
            let editor_id = crate::utils::text_field::input_text_template_id(&cssom_input_id(&[0, 0, 0]));
            let editor = super::super::snapshot::find_element_by_template(&root, &editor_id).unwrap().layout_node;
            DevToolsState::set_pick(false);
            sync_overlay_layout(&mut root, &mut layout);
            assert_eq!(super::super::snapshot::find_element_by_template(&root, &editor_id).unwrap().layout_node, editor);
            DevToolsState::set_open(false);
        });
    }

    #[test]
    fn status_changes_only_rebuild_the_visible_status_panel() {
        crate::core::reactivity::create_root(|| {
            DevToolsState::set_tab(DevToolsTab::Elements);
            let before = DevToolsState::revisions();
            DevToolsState::record_change("CSS", "updated");
            let after = DevToolsState::revisions();
            assert_eq!(after[0], before[0]);
            assert_eq!(after[1], before[1]);
            assert_eq!(after[2], before[2]);

            DevToolsState::set_tab(DevToolsTab::Changes);
            let before = DevToolsState::revisions();
            DevToolsState::record_change("CSS", "updated");
            let after = DevToolsState::revisions();
            assert_eq!(after[0], before[0].wrapping_add(1));
            assert_eq!(after[1], before[1]);
            assert_eq!(after[2], before[2]);
            DevToolsState::set_tab(DevToolsTab::Elements);
        });
    }

    #[test]
    fn devtools_sources_do_not_depend_on_component_library() {
        for source in [include_str!("panel.rs"), include_str!("device.rs"), include_str!("controls.rs"), include_str!("overlay.rs"), include_str!("snapshot.rs"), include_str!("reactivity_panel.rs")] {
            assert!(!source.contains(concat!("crate::", "components")), "DevTools must use the shared foundation, not product components");
        }
    }

    #[test]
    fn closing_resizing_and_reopening_restore_app_geometry_on_fast_layout() {
        crate::core::reactivity::create_root(|| {
            DevToolsState::enable_for_window();
            DevToolsState::set_panel_width(400.0);
            DevToolsState::set_open(true);
            DevToolsState::bump();
            let app = Element::new().w("100%").h("100%").children(
                (0..30)
                    .map(|index| Element::new().focus_id(format!("app-row-{index}")))
                    .collect::<Vec<_>>(),
            );
            let mut root = wrap_app(app);
            let mut tree = crate::core::tree::LayoutTree::new();
            assert_ne!(sync_overlay_layout(&mut root, &mut tree), LayoutStatus::Clean);
            tree.build_and_layout(&mut root, 1200.0, 700.0, [-1.0, -1.0]);
            assert!((root.children[0].layout_bounds[2] - 800.0).abs() < 1.0);
            let hit = crate::app::pointer_events::hit_path(&root, 803.0, 300.0);
            assert!(
                hit.iter()
                    .any(|entry| entry.focus_id.as_deref() == Some("__jadix_devtools_resize")),
                "resize handle must be reachable through the real hit-test path"
            );
            for (open, width, expected) in [
                (false, 400.0, 1200.0),
                (true, 520.0, 680.0),
                (false, 520.0, 1200.0),
            ] {
                DevToolsState::set_open(open);
                DevToolsState::set_panel_width(width);
                assert_ne!(sync_overlay_layout(&mut root, &mut tree), LayoutStatus::Clean);
                tree.build_and_layout_fast(&mut root, 1200.0, 700.0, [-1.0, -1.0], false, false);
                assert!(
                    (root.children[0].layout_bounds[2] - expected).abs() < 1.0,
                    "app did not reclaim width after open={open}"
                );
            }
        });
    }

    #[test]
    fn status_bar_spans_the_bottom_of_the_docked_panel() {
        crate::core::reactivity::create_root(|| {
            DevToolsState::enable_for_window();
            DevToolsState::set_open(true);
            DevToolsState::set_panel_width(700.0);
            DevToolsState::set_tab(DevToolsTab::Elements);
            let mut root = wrap_app(Element::new().w("100%").h("100%"));
            let mut tree = crate::core::tree::LayoutTree::new();
            sync_overlay_layout(&mut root, &mut tree);
            tree.build_and_layout(&mut root, 1400.0, 800.0, [-1.0; 2]);
            let footer =
                super::super::snapshot::find_element_by_template(&root, "__jadix_devtools_status")
                    .unwrap();
            let [left, top, width, height] = footer.layout_bounds;
            assert!(
                (top + height - 800.0).abs() < 1.0,
                "footer={:?}",
                footer.layout_bounds
            );
            assert!((height - 24.0).abs() < 1.0);
            assert!(left >= 700.0 && width >= 698.0);
            DevToolsState::set_open(false);
        });
    }

    #[test]
    fn tree_rows_include_closing_tags_and_stable_opaque_backgrounds() {
        crate::core::reactivity::create_root(|| {
            let mut leaf = Element::new().text("leaf");
            leaf.set_attr("data-slot", "text");
            let mut root = Element::new().focus_id("tree-parent").child(leaf);
            root.set_attr("data-slot", "parent");
            let tree = snapshot_tree(&root);
            if DevToolsState::is_collapsed(&[]) {
                DevToolsState::toggle_collapsed(&[]);
            }
            let row = tree_row(&tree, &tree[0], None, None);
            assert_eq!(row.style.bg_color[3], 1.0);
            let closing = closing_tree_row(&tree[0]);
            let mut captions = Vec::new();
            crate::core::tree::walk_elements(&closing, |node| {
                if let Some(value) = &node.text {
                    captions.push(value.clone());
                }
            });
            assert!(captions.iter().any(|caption| caption == "</parent>"));
            DevToolsState::toggle_collapsed(&[]);
            let collapsed = tree_row(&tree, &tree[0], None, None);
            let mut closed = false;
            crate::core::tree::walk_elements(&collapsed, |node| {
                closed |= node
                    .text
                    .as_ref()
                    .is_some_and(|value| value.contains("</parent>"));
            });
            assert!(closed);
            DevToolsState::toggle_collapsed(&[]);
        });
    }

    #[test]
    fn closing_tag_uses_available_row_width_without_wrapping() {
        crate::core::reactivity::create_root(|| {
            let mut parent = Element::new().child(Element::new());
            parent.set_attr("data-slot", "sidebar-menu-button");
            let tree = snapshot_tree(&parent);
            let mut row = closing_tree_row(&tree[0]);
            let mut layout = crate::core::tree::LayoutTree::new();
            layout.build_and_layout(&mut row, 360.0, 22.0, [-1.0; 2]);
            let caption = &row.children[0];
            assert_eq!(caption.text_style.white_space, crate::style::WhiteSpace::Nowrap);
            assert!(caption.layout_bounds[2] >= 300.0, "closing tag prematurely constrained: {:?}", caption.layout_bounds);
            assert!(caption.layout_bounds[3] <= 22.0);
        });
    }

    #[test]
    fn tree_filter_keeps_ancestors_and_searches_collapsed_descendants() {
        let root = Element::new().children(vec![
            Element::new()
                .focus_id("parent")
                .child(Element::new().focus_id("needle")),
            Element::new().focus_id("other"),
        ]);
        let tree = snapshot_tree(&root);
        if DevToolsState::is_collapsed(&[0]) {
            DevToolsState::toggle_collapsed(&[0]);
        }
        DevToolsState::toggle_collapsed(&[0]);
        assert_eq!(visible_tree(&tree, "").len(), 3);
        let matches = visible_tree(&tree, "NEEDLE");
        assert_eq!(
            matches
                .iter()
                .map(|node| node.path.clone())
                .collect::<Vec<_>>(),
            vec![vec![], vec![0], vec![0, 0]]
        );
        DevToolsState::set_selected_path(vec![0, 0]);
        assert!(!DevToolsState::is_collapsed(&[0]));
        assert!(visible_tree(&tree, "missing").is_empty());
    }

    #[test]
    fn sources_resolves_nearest_preserved_reactive_anchor() {
        let mut parent = Element::new().template_id("gallery:section");
        parent.set_attr(
            crate::app::reactive::ATTR_SOURCE_TEMPLATE_ID,
            "examples/gallery/src/app.rs:21:5",
        );
        let root = Element::new().child(parent.child(Element::new().focus_id("button")));
        let source = source_for_path(&root, &[0, 0]).unwrap();
        assert_eq!(source.path, "examples/gallery/src/app.rs");
        assert_eq!(source.line, 21);
    }

    #[test]
    fn developer_panels_build_without_source_selection() {
        crate::core::reactivity::create_root(|| {
            DevToolsState::enable_for_window();
            let root = Element::new().child(Element::new().focus_id("app"));
            let tree = snapshot_tree(&root);
            for tab in [
                DevToolsTab::Elements,
                DevToolsTab::Layout,
                DevToolsTab::Reactivity,
                DevToolsTab::Sources,
                DevToolsTab::Changes,
            ] {
                let panel = dock_panel(tab, &tree, tree.get(1), false, 720.0, &root, &[0]);
                assert_eq!(panel.template_id.as_deref(), Some(PANEL_ID));
                assert!(!panel.children.is_empty());
            }
        });
    }

    #[test]
    fn inspector_seed_unbinds_stale_controlled_pairing() {
        use crate::core::reactivity::{create_root, use_signal};
        use crate::utils::text_field::{
            bind_controlled_value, input_value_signal_key, pull_controlled_into_store,
            unbind_controlled_value,
        };

        create_root(|| {
            let id = "dt-cssom-test-unbind";
            let external = use_signal("__jadix_dt_cssom_test_unbind", || "from-external".into());
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
            .find(|c| c.template_id.as_deref() == Some("__jadix_app_host"))
            .expect("app host");
        assert!(host
            .children
            .iter()
            .any(|c| { c.template_id.as_deref() == Some("__jadix_devtools_highlight") }));
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
    fn cssom_editor_preserves_nested_horizontal_and_vertical_scroll() {
        use crate::utils::text_field::input_text_template_id;
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
            assert_eq!(text.text_style.white_space, WhiteSpace::Nowrap);
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
