//! Inspector dock + highlight overlay (P0/P1/P2).
//!
//! Overlay is a **placeholder** in the retained tree (`wrap_app`). Content is injected
//! from `AppState::render` via [`sync_overlay`] — not a `reactive_view` — so F12 / Pick
//! never flush render effects while `AppState` is borrowed.

use super::controls::{button, input, label, scroll_area, textarea};
use crate::app::runtime;
use crate::core::reactivity::use_signal;
use crate::core::tree::{Element, LayoutStatus};
use crate::primitives::{hstack, r#box, vstack};
use crate::style::{collect_matching_rules, parse_color, parse_text_color, soft_semantic_fill};
use crate::utils::text_field::{focus_id_for_text_edit, input_value_signal_key};
use crate::utils::build_lucide_icon_text_node_or as inspector_icon;

use super::locale::text;
use super::overlay::{box_model_diagram, highlight_slot};
use super::reactivity_panel::build_reactivity_panel;
use super::snapshot::{
    cssom_text_from_style, find_element_by_path, find_element_by_template_mut, node_label,
    snapshot_hover_anchor, snapshot_subtree, snapshot_tree, source_location_from_template_id,
    InspectorNodeSnapshot,
};
use super::state::{DevToolsState, DevToolsTab};

pub(super) const PANEL_ID: &str = "__jadix_devtools_panel";
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

pub(crate) fn sync_content_viewport(root: &mut Element) {
    let Some(host) = find_element_by_template_mut(root, "__jadix_app_host") else {
        return;
    };
    if DevToolsState::is_open() {
        let width = crate::utils::modal_motion::modal_viewport_size().0.to_string();
        if host.attrs.get("data-jadix-viewport-width") != Some(&width) {
            host.set_attr("data-jadix-viewport-width", width);
        }
    } else {
        host.attrs.remove("data-jadix-viewport-width");
    }
}

/// Rebuild the docked panel when Inspector chrome generation changes.
pub(crate) fn sync_overlay_layout(
    root: &mut Element,
    tree: &mut crate::core::tree::LayoutTree,
) -> LayoutStatus {
    super::locale::sync();
    sync_content_viewport(root);
    let preview_impact = super::device::sync(root, tree);
    let panel_impact = sync_overlay(root);
    if matches!(
        panel_impact,
        LayoutStatus::LayoutChanged | LayoutStatus::StructureChanged
    ) {
        for id in [PANEL_SLOT_ID, PANEL_ID] {
            if let Some(element) = find_element_by_template_mut(root, id) {
                tree.mark_element_layout_dirty(element);
            }
        }
    }
    preview_impact.merge(panel_impact)
}

pub fn sync_overlay(root: &mut Element) -> LayoutStatus {
    if !DevToolsState::is_enabled() {
        return LayoutStatus::Clean;
    }
    let gen = DevToolsState::generation();
    if gen == DevToolsState::last_sync_gen() {
        return LayoutStatus::Clean;
    }
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
                super::controls::dismiss_tooltips(slot);
                slot.children.clear();
                impact = LayoutStatus::StructureChanged;
            }
        }
    } else {
        let revisions = DevToolsState::revisions();
        let panel = super::snapshot::find_element_by_template(root, PANEL_ID).unwrap();
        let changed = |index: usize| {
            panel.attrs.get(&format!("data-dt-revision-{index}"))
                != Some(&revisions[index].to_string())
        };
        let body_changed = changed(0);
        let outline_changed = changed(1);
        let details_changed = changed(2);
        let locale_changed = panel.attrs.get("data-dt-locale") != Some(&crate::chrome_locale());
        let tab = DevToolsState::active_tab();
        if body_changed || details_changed || (tab == DevToolsTab::Elements && outline_changed) {
            let path = DevToolsState::selected_path();
            let tree = snapshot_tree(root);
            let selected = if DevToolsState::selected_path_key().is_empty() {
                None
            } else {
                tree.iter()
                    .find(|node| node.path == path)
                    .cloned()
                    .or_else(|| snapshot_subtree(root, &path))
            };
            if body_changed || tab != DevToolsTab::Elements {
                let body = panel_body(
                    tab,
                    &tree,
                    selected.as_ref(),
                    root,
                    &path,
                    DevToolsState::panel_width(),
                );
                replace_panel_region(root, "dt-body", body);
            } else {
                if outline_changed {
                    let rows = tree_body(&tree, &DevToolsState::tree_filter());
                    replace_panel_region(root, "dt-tree-rows", rows);
                    refresh_tree_find_handlers(root, &tree);
                }
                if details_changed {
                    let crumbs = breadcrumbs(&tree, selected.as_ref());
                    replace_panel_region(root, "dt-breadcrumbs", crumbs);
                }
                if details_changed {
                    let details =
                        inspector_card("Properties", details_pane(selected.as_ref(), root))
                            .template_id("dt-details")
                            .border_left(parse_color("border"), 1.0);
                    replace_panel_region(root, "dt-details", details);
                }
            }
            impact = LayoutStatus::StructureChanged;
        }
        if locale_changed {
            replace_panel_region(
                root,
                "dt-header",
                panel_header(DevToolsState::is_pick(), tab),
            );
            impact = LayoutStatus::StructureChanged;
        } else {
            for (id, selected) in [
                ("dt-pick", DevToolsState::is_pick()),
                ("dt-device", super::device::enabled()),
            ] {
                let fresh: Element = button(id)
                    .size("icon-sm")
                    .variant(if selected { "secondary" } else { "ghost" })
                    .into();
                impact = impact.merge(patch_control_chrome(root, id, fresh));
            }
            for (id, title, candidate) in devtools_tabs() {
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
    if let Some(panel) = find_element_by_template_mut(root, PANEL_ID) {
        stamp_panel_revisions(panel);
    }
    if impact == LayoutStatus::StructureChanged {
        crate::app::state::recompute_scroll_viewport_flags(root);
    }
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
    let Some(bounds) = super::snapshot::find_element_by_template(root, id).map(|element| {
        let bounds = crate::core::tree::visual_paint_aabb(element, element.paint_scroll_acc);
        if id == "dt-tree-rows" {
            crate::core::tree::overflow_clip_bounds_for(root, element).unwrap_or(bounds)
        } else {
            bounds
        }
    }) else {
        return;
    };
    if let Some(element) = find_element_by_template_mut(root, id) {
        if id == "dt-tree-rows" {
            crate::app::damage::push_frame_region_damage(bounds);
        } else {
            crate::app::damage::push_frame_region_paint(
                crate::app::damage::paint_element_key(element), bounds);
        }
        *element = replacement;
    }
}

fn patch_control_chrome(root: &mut Element, id: &str, fresh: Element) -> LayoutStatus {
    let Some(element) = find_element_by_template_mut(root, id) else {
        return LayoutStatus::Clean;
    };
    let fresh_caption_color = fresh
        .children
        .first()
        .and_then(|child| child.text_style.color);
    let current_caption_color = element
        .children
        .first()
        .and_then(|child| child.text_style.color);
    let caption_hover_changed = element
        .children
        .first()
        .and_then(|child| child.class_set.as_ref())
        .map(|classes| classes.has_pointer_hover_variants())
        != fresh
            .children
            .first()
            .and_then(|child| child.class_set.as_ref())
            .map(|classes| classes.has_pointer_hover_variants());
    let aria_changed = element.attrs.get("aria-selected") != fresh.attrs.get("aria-selected");
    let style_changed = element.style.bg_color != fresh.style.bg_color
        || element.style.border_color != fresh.style.border_color
        || element.style.border_width != fresh.style.border_width
        || element.text_style.color != fresh.text_style.color
        || current_caption_color != fresh_caption_color
        || caption_hover_changed;
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
    if let (Some(caption), Some(new_caption)) =
        (element.children.first_mut(), fresh.children.first())
    {
        caption.text_style.color = new_caption.text_style.color;
        caption.class_set = new_caption.class_set.clone();
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
    let header = panel_header(pick, tab);
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
            .children([header, body, footer]),
    )
    .border_left(parse_color("border"), 1.0);
    if !super::presentation::is_detached() {
        panel = panel.relative().child(super::presentation::resize_handle());
    }
    panel.template_id = Some(PANEL_ID.into());
    stamp_panel_revisions(&mut panel);
    panel
}

fn panel_header(pick: bool, tab: DevToolsTab) -> Element {
    let pick_icon = inspector_icon("square-dashed-mouse-pointer", "⌖")
    .text_size(16.0);
    let header: Element = hstack()
        .w("100%")
        .items("center")
        .gap(2.0)
        .px(6.0)
        .h(34.0)
        .flex_shrink(0.0)
        .bg(parse_color("background"))
        .children(
            [
                button("dt-pick")
                    .size("icon-sm")
                    .tooltip("Select an element (Ctrl+Shift+C)")
                    .variant(if pick { "secondary" } else { "ghost" })
                    .child(pick_icon)
                    .on_click(|| {
                        DevToolsState::toggle_pick();
                        runtime::request_redraw();
                    })
                    .into(),
                button("dt-device")
                    .size("icon-sm")
                    .tooltip("Toggle device toolbar")
                    .variant(if super::device::enabled() {
                        "secondary"
                    } else {
                        "ghost"
                    })
                    .child(
                        inspector_icon("monitor-smartphone", "▣")
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
                .flex_shrink(0.0),
                panel_navigation(tab),
            ]
            .into_iter()
            .chain([
                button("dt-language")
                    .tooltip("Switch language")
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
                    .tooltip("Narrow DevTools")
                    .size("icon-sm")
                    .variant("ghost")
                    .text("−")
                    .on_click(|| {
                        DevToolsState::set_panel_width(DevToolsState::panel_width() - 80.0);
                        runtime::request_redraw();
                    })
                    .into(),
                button("dt-widen")
                    .tooltip("Widen DevTools")
                    .size("icon-sm")
                    .variant("ghost")
                    .text("+")
                    .on_click(|| {
                        DevToolsState::set_panel_width(DevToolsState::panel_width() + 80.0);
                        runtime::request_redraw();
                    })
                    .into(),
                button("dt-detach")
                    .tooltip(if super::presentation::is_detached() { "Dock DevTools" } else { "Detach DevTools" })
                    .size("icon-sm")
                    .variant("ghost")
                    .child(
                        inspector_icon(if super::presentation::is_detached() {
                                "panel-right-close"
                            } else {
                                "external-link"
                            }, ">")
                        .text_size(15.0),
                    )
                    .on_click(super::presentation::toggle_detached)
                    .into(),
                button("dt-close")
                    .tooltip("Close DevTools")
                    .size("icon-sm")
                    .variant("ghost")
                    .child(
                        inspector_icon("x", "×")
                            .text_size(15.0),
                    )
                    .on_click(|| {
                        DevToolsState::set_open(false);
                        runtime::request_redraw();
                    })
                    .into(),
            ]),
        )
        .into();
    header.template_id("dt-header")
}

fn panel_navigation(tab: DevToolsTab) -> Element {
    scroll_area("dt-nav")
        .w(0.0)
        .h(30.0)
        .flex_grow(1.0)
        .flex_shrink(1.0)
        .scrollbar_visibility("never")
        .overflow_x("auto")
        .overflow_y("hidden")
        .child(
            hstack()
                .w("max-content")
                .h(30.0)
                .flex_shrink(0.0)
                .gap(2.0)
                .children(
                    devtools_tabs().map(|(id, title, candidate)| {
                        tab_btn(id, title, tab == candidate, candidate)
                    }),
                ),
        )
        .into()
}

fn devtools_tabs() -> [(&'static str, &'static str, DevToolsTab); 5] {
    [
        ("dt-tab-el", "Elements", DevToolsTab::Elements),
        ("dt-tab-src", "Sources", DevToolsTab::Sources),
        ("dt-tab-lay", "Layout", DevToolsTab::Layout),
        ("dt-tab-rx", "Reactivity", DevToolsTab::Reactivity),
        ("dt-tab-changes", "Changes", DevToolsTab::Changes),
    ]
}

fn panel_body(
    tab: DevToolsTab,
    tree: &[InspectorNodeSnapshot],
    selected: Option<&InspectorNodeSnapshot>,
    root: &Element,
    path: &[usize],
    panel_w: f32,
) -> Element {
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
    let hover_classes = if active {
        "hover:bg-info/30 hover:text-info"
    } else {
        "hover:bg-info/20 hover:text-foreground"
    };
    let text_color = parse_text_color(if active { "info" } else { "foreground" });
    let mut tab = Element::from(
        hstack()
            .h(30.0)
            .px(10.0)
            .items("center")
            .bg(if active {
                soft_semantic_fill(parse_color("info"))
            } else {
                [0.0; 4]
            })
            .text_color(text_color)
            .child(
                label(format!("{id}-label"))
                    .text(text(caption))
                    .text_size(12.0)
                    .text_color(text_color)
                    .class(hover_classes),
            ),
    )
    .template_id(id)
    .focus_id(id)
    .flex_shrink(0.0)
    .cursor("pointer")
    .class(hover_classes)
    .border_bottom(
        if active {
            parse_color("info")
        } else {
            [0.0; 4]
        },
        2.0,
    );
    tab.set_attr("role", "tab");
    tab.set_attr("aria-selected", if active { "true" } else { "false" });
    keyboard_activatable(tab.on_click(click))
}

fn tab_btn(id: &str, caption: &str, active: bool, tab: DevToolsTab) -> Element {
    let hover_classes = if active {
        "hover:bg-info/30 hover:text-info"
    } else {
        "hover:bg-info/20 hover:text-foreground"
    };
    let text_color = parse_text_color(if active { "info" } else { "foreground" });
    let mut button: Element = button(id)
        .variant("ghost")
        .child(
            label(format!("{id}-label"))
                .text(text(caption))
                .text_color(text_color)
                .class(hover_classes),
        )
        .on_click(move || DevToolsState::set_tab(tab))
        .into();
    button = button
        .class(if active {
            "bg-info/20 hover:bg-info/30 hover:text-info"
        } else {
            "hover:bg-info/20 hover:text-foreground"
        })
        .text_color(text_color);
    button.set_attr("role", "tab");
    button.set_attr("aria-label", caption);
    button.set_attr("aria-selected", if active { "true" } else { "false" });
    button
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
        .scrollbar_visibility(if title == "Element tree" { "always" } else { "auto" })
        .overflow_x(if title == "Element tree" { "hidden" } else { "auto" })
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
    _query: &str,
) -> Vec<&'a InspectorNodeSnapshot> {
    tree
            .iter()
            .filter(|node| {
                !(0..node.path.len()).any(|depth| DevToolsState::is_collapsed(&node.path[..depth]))
            })
            .collect()
}

fn find_next_tree_node(tree: &[InspectorNodeSnapshot], query: String) {
    let query = query.trim().to_lowercase();
    let previous = DevToolsState::tree_filter();
    DevToolsState::set_tree_filter(query.clone());
    if query.is_empty() { return; }
    let matches: Vec<_> = tree.iter().filter(|node| {
        format!("{} {:?} {:?} {:?} {:?}", node_label(node), node.tag_hint,
            node.focus_id, node.text_preview, node.template_id).to_lowercase().contains(&query)
    }).collect();
    let current = DevToolsState::selected_path();
    let index = if previous == query {
        matches.iter().position(|node| node.path == current).map_or(0, |index| (index + 1) % matches.len())
    } else { 0 };
    if let Some(node) = matches.get(index) {
        let (identity, relative) = snapshot_hover_anchor(tree, node);
        DevToolsState::set_selected(node.path.clone(), identity, relative,
            (node.box_model.width, node.box_model.height));
        DevToolsState::request_tree_reveal(&node.path);
        reveal_page_node(&node.path);
    }
}

fn reveal_page_node(path: &[usize]) {
    DevToolsState::set_hover_highlight_path(path.to_vec());
    runtime::with_primary_root(|root| reveal_page_node_in_root(root, path));
    runtime::request_redraw_all();
}

fn reveal_page_node_in_root(root: &Element, path: &[usize]) {
    let Some(target) = find_element_by_path(root, path) else {
        return;
    };
    let mut target_box = crate::app::tree_hit::visual_border_box(
        target,
        super::snapshot::scroll_acc_along_path(root, path),
    );
    for depth in (0..path.len()).rev() {
        let Some(viewport) = find_element_by_path(root, &path[..depth]) else {
            continue;
        };
        let Some(rest) = viewport.focus_id.as_deref().and_then(
            crate::core::scroll_viewport::strip_scroll_viewport_focus_id,
        ) else {
            continue;
        };
        let bounds = crate::app::tree_hit::visual_border_box(
            viewport,
            super::snapshot::scroll_acc_along_path(root, &path[..depth]),
        );
        if target_box[1] >= bounds[1]
            && target_box[1] + target_box[3] <= bounds[1] + bounds[3]
        {
            continue;
        }
        let keys = crate::core::scroll_viewport::ScrollViewportSignalKeys::new(rest);
        let old = use_signal(&keys.offset_y, || 0.0_f32).peek();
        let maximum = (use_signal(&keys.content_h, || 0.0_f32).peek()
            - use_signal(&keys.viewport_h, || bounds[3]).peek())
        .max(0.0);
        let scale = crate::core::tree::paint_scale_with_ancestor(
            viewport, viewport.paint_ancestor_scale,
        )[1].abs().max(1e-6);
        let next = (old + (target_box[1] + target_box[3] * 0.5 - bounds[1]
            - bounds[3] * 0.5) / scale)
            .clamp(0.0, maximum);
        crate::app::scroll_support::set_scroll_offset_with_behavior(
            rest,
            use_signal(&keys.offset_x, || 0.0_f32).peek(),
            next,
            true,
        );
        target_box[1] -= (next - old) * scale;
    }
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

fn tree_outline(
    tree: &[InspectorNodeSnapshot],
    selected: Option<&InspectorNodeSnapshot>,
) -> Element {
    let query = DevToolsState::tree_filter();
    let search: Element = input("dt-tree-query")
        .placeholder(text("Find tag, id, text, source"))
        .text_size(11.0).h(28.0).flex_grow(1.0).into();
    let search = search.template_id("dt-tree-query");
    let filter = hstack()
        .w("100%")
        .gap(4.0)
        .p(4.0)
        .items("center")
        .children([
            search,
            button("dt-tree-filter")
                .size("sm")
                .variant("ghost")
                .text(text("Find"))
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
    let tree_el = tree_body(tree, &query);
    let mut outline = Element::from(
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
    refresh_tree_find_handlers(&mut outline, tree);
    outline.template_id("dt-outline")
}

fn refresh_tree_find_handlers(root: &mut Element, tree: &[InspectorNodeSnapshot]) {
    if let Some(button) = find_element_by_template_mut(root, "dt-tree-filter") {
        let tree = tree.to_vec();
        button.on_click = Element::new().on_click(move || {
            find_next_tree_node(&tree, field_store_value("dt-tree-query"));
        }).on_click.clone();
    }
    if let Some(input) = find_element_by_template_mut(root, "dt-tree-query") {
        let tree = tree.to_vec();
        input.on_key_down = Element::new().on_key_down(move |event| {
            if event.state == winit::event::ElementState::Pressed
                && event.logical_key == winit::keyboard::Key::Named(winit::keyboard::NamedKey::Enter) {
                find_next_tree_node(&tree, field_store_value("dt-tree-query"));
                true
            } else { false }
        }).on_key_down.clone();
    }
}

fn tree_body(tree: &[InspectorNodeSnapshot], query: &str) -> Element {
    let mut rows = Vec::new();
    let indices: std::collections::HashMap<_, _> = tree.iter().enumerate()
        .map(|(index, node)| (node.path.as_slice(), index)).collect();
    let mut closing: Vec<usize> = Vec::new();
    for node in visible_tree(tree, query) {
        while closing.last().is_some_and(|index| tree[*index].depth >= node.depth) {
            rows.push((closing.pop().unwrap(), true));
        }
        let index = indices[node.path.as_slice()];
        rows.push((index, false));
        if node.child_count > 0 && !DevToolsState::is_collapsed(&node.path) {
            closing.push(index);
        }
    }
    while let Some(index) = closing.pop() {
        rows.push((index, true));
    }
    if rows.is_empty() {
        return kv("No matches", query).template_id("dt-tree-rows").reactive_sync(|_, _| {
            let rest = super::overlay::ELEMENTS_TREE_SCROLL_ID;
            let visibility = use_signal(
                &crate::core::scrollbar::scrollbar_visibility_signal_key(rest, "y"),
                || "always".to_string(),
            );
            if visibility.peek() == "never" {
                LayoutStatus::Clean
            } else {
                visibility.set_silent("never".to_string());
                LayoutStatus::LayoutChanged
            }
        });
    }
    let tree = std::rc::Rc::new(tree.to_vec());
    let mut geometry = (0.0_f32, 0.0_f32, Vec::new());
    let mut body = Element::from(vstack().w("100%").min_w(0.0))
        .template_id("dt-tree-rows")
        .h(rows.len() as f32 * 22.0)
        .min_h(rows.len() as f32 * 22.0)
        .flex_shrink(0.0);
    sync_tree_window(&mut body, &tree, &rows, &mut geometry);
    let geometry = std::cell::RefCell::new(geometry);
    body.reactive_sync(move |body, _| sync_tree_window(body, &tree, &rows, &mut geometry.borrow_mut()))
}

fn sync_tree_window(
    body: &mut Element,
    tree: &[InspectorNodeSnapshot],
    rows: &[(usize, bool)],
    geometry: &mut (f32, f32, Vec<f32>),
) -> LayoutStatus {
    use crate::core::scroll_viewport::ScrollViewportSignalKeys;
    let rest = super::overlay::ELEMENTS_TREE_SCROLL_ID;
    let keys = ScrollViewportSignalKeys::new(rest);
    let offset = use_signal(&keys.offset_y, || 0.0_f32);
    let viewport_h = use_signal(&keys.viewport_h, || 0.0_f32).peek();
    let viewport_w = use_signal(&keys.viewport_w, || 0.0_f32).peek().max(1.0);
    let width = if viewport_w > 1.0 { viewport_w } else { 480.0 };
    let geometry_changed = geometry.0 != width || geometry.1 != viewport_h;
    if geometry_changed {
        let measure = |width: f32| {
            let mut offsets = vec![0.0];
            for (index, closing) in rows {
                let node = &tree[*index];
                let caption = if *closing {
                    format!("</{}>", node.tag_hint.as_deref().unwrap_or("box"))
                } else { tree_row_caption(node) };
                let available = (width - node.depth as f32 * 10.0 - 16.0).max(1.0);
                let prepared = crate::text::prepare_with_family(
                    &caption, 11.0, 400, 22.0, glyphon::Family::Name("Consolas"));
                let height = crate::text::layout(&prepared, available).height.max(22.0);
                offsets.push(offsets.last().copied().unwrap() + height);
            }
            offsets
        };
        let mut offsets = measure(width);
        let gutter = if viewport_h > 1.0 && *offsets.last().unwrap() > viewport_h {
            crate::core::scrollbar::SCROLLBAR_DEFAULT_CROSS_PX
                + crate::core::scrollbar::SCROLLBAR_EDGE_PAD * 2.0
        } else { 0.0 };
        if gutter > 0.0 { offsets = measure(width - gutter); }
        body.style.padding.right = taffy::style::LengthPercentage::length(gutter);
        let height = *offsets.last().unwrap();
        body.style.size.height = taffy::style::Dimension::length(height);
        body.style.min_size.height = taffy::style::Dimension::length(height);
        *geometry = (width, viewport_h, offsets);
    }
    let offsets = &geometry.2;
    let content_h = *offsets.last().unwrap();
    let visibility = use_signal(
        &crate::core::scrollbar::scrollbar_visibility_signal_key(rest, "y"),
        || "always".to_string(),
    );
    let mode = if viewport_h > 1.0 && content_h > viewport_h { "always" } else { "never" };
    let visibility_changed = visibility.peek() != mode;
    if visibility_changed {
        visibility.set_silent(mode.to_string());
    }
    let mut scroll_y = offset.peek();
    if viewport_h > 1.0 {
        if let Some(key) = DevToolsState::pending_tree_reveal() {
            if let Some(index) = rows.iter().position(|(index, closing)| {
                !closing && super::snapshot::path_to_key(&tree[*index].path) == key
            }) {
                scroll_y = super::overlay::scroll_offset_to_center_rect(
                    offsets[index], offsets[index + 1] - offsets[index], viewport_h, content_h,
                );
                let scroll_x = use_signal(&keys.offset_x, || 0.0_f32).peek();
                crate::app::scroll_support::set_scroll_offset_with_behavior(rest, scroll_x, scroll_y, true);
                crate::app::scroll_support::cancel_scroll_item_into_view(rest);
                DevToolsState::finish_tree_reveal();
            }
        }
    }
    let height = if viewport_h > 1.0 { viewport_h } else { 480.0 };
    let start = if rows.len() <= 200 { 0 } else {
        offsets.partition_point(|top| *top <= scroll_y).saturating_sub(9).min(rows.len())
    };
    let end = if rows.len() <= 200 { rows.len() } else {
        (offsets.partition_point(|top| *top < scroll_y + height) + 8).min(rows.len())
    };
    let range = format!("{start}:{end}:{width}:{viewport_h}");
    if body.attrs.get("data-dt-window") == Some(&range) {
        return if visibility_changed { LayoutStatus::LayoutChanged } else { LayoutStatus::Clean };
    }
    let mut retained: std::collections::HashMap<_, _> = std::mem::take(&mut body.children)
        .into_iter().filter_map(|row| row.template_id.clone().map(|key| (key, row))).collect();
    let selected = DevToolsState::selected_path_key();
    let hover = DevToolsState::hover_path_key();
    let mut children = Vec::with_capacity(end.saturating_sub(start) + 2);
    let spacer = |id: &str, height: f32| Element::new().template_id(id).h(height).min_h(height).flex_shrink(0.0);
    if start > 0 {
        children.push(spacer("dt-tree-before", offsets[start]));
    }
    for (row_index, (index, closing)) in rows.iter().enumerate().take(end).skip(start) {
        let node = &tree[*index];
        let key = format!("dt-{}-{}", if *closing { "end" } else { "row" }, super::snapshot::path_to_key(&node.path));
        let mut row = retained.remove(&key).unwrap_or_else(|| {
            if *closing { closing_tree_row(node).template_id(&key) }
            else { tree_row(tree, node, Some(&selected), Some(&hover)) }
        });
        row.style.size.height = taffy::style::Dimension::length(offsets[row_index + 1] - offsets[row_index]);
        children.push(row);
    }
    if end < rows.len() {
        children.push(spacer("dt-tree-after", content_h - offsets[end]));
    }
    body.children = children;
    body.set_attr("data-dt-window", range);
    LayoutStatus::StructureChanged
}

fn breadcrumbs(
    tree: &[InspectorNodeSnapshot],
    selected: Option<&InspectorNodeSnapshot>,
) -> Element {
    let mut crumbs = Vec::new();
    let selected_key = selected.map(|node| super::snapshot::path_to_key(&node.path));
    if let Some(key) = selected_key.as_deref() {
        crate::app::scroll_support::request_scroll_descendant_into_view_when(
            "dt-breadcrumbs",
            format!("dt-crumb-{key}"),
            crate::app::scroll_support::ScrollItemAlign::Center,
            crate::app::scroll_support::ScrollAlignWhen::Before,
        );
    }
    if let Some(selected) = selected {
        for node in tree
            .iter()
            .filter(|node| selected.path.starts_with(&node.path))
        {
            let path = node.path.clone();
            let (identity, relative) = snapshot_hover_anchor(tree, node);
            let size = (node.box_model.width, node.box_model.height);
            let key = super::snapshot::path_to_key(&path);
            if !crumbs.is_empty() {
                crumbs.push(
                    Element::from(
                        hstack()
                            .w(18.0)
                            .h(28.0)
                            .items("center")
                            .justify("center")
                            .pointer_events("none")
                            .child(inspector_icon("chevron-right", ">"))
                            .text_size(13.0)
                            .text_color(parse_text_color("muted-foreground")),
                    )
                    .template_id(format!("dt-crumb-sep-{key}"))
                    .flex_shrink(0.0),
                );
            }
            crumbs.push(
                button(format!("dt-crumb-{key}"))
                    .size("sm")
                    .variant(if selected_key.as_deref() == Some(key.as_str()) {
                        "secondary"
                    } else {
                        "ghost"
                    })
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
                        DevToolsState::request_tree_reveal(&path);
                    })
                    .flex_shrink(0.0)
                    .into(),
            );
        }
    }
    scroll_area("dt-breadcrumbs")
        .w("100%")
        .h(32.0)
        .border_top(parse_color("border"), 1.0)
        .scrollbar_visibility("never")
        .flex_shrink(0.0)
        .overflow_x("auto")
        .overflow_y("hidden")
        .child(
            hstack()
                .w("max-content")
                .min_w("max-content")
                .h("100%")
                .px(4.0)
                .items("center")
                .gap(2.0)
                .children(crumbs),
        )
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
        .text_color(parse_text_color("muted-foreground")),
    )
    .font_family("Consolas")
    .min_w(0.0)
    .flex_grow(1.0)
    .flex_shrink(1.0)
    .flex_basis(0.0)
    .text_wrap("char")
    .text_leading(2.0)
    .overflow_x("hidden");
    Element::from(
        hstack()
            .w("100%")
            .h(22.0)
            .flex_shrink(0.0)
            .pl(node.depth as f32 * 10.0 + 16.0)
            .items("start")
            .child(caption),
    )
    .font_family("Consolas")
    .bg(parse_color("background"))
}

fn tree_row_caption(node: &InspectorNodeSnapshot) -> String {
    match (&node.tag_hint, &node.focus_id, &node.text_preview) {
        (Some(tag), _, Some(text)) => format!("<{tag}> {text} </{tag}>"),
        (Some(tag), Some(id), _) if node.child_count == 0 => format!("<{tag} id=\"{id}\" />"),
        (Some(tag), Some(id), _) if DevToolsState::is_collapsed(&node.path) => {
            format!("<{tag} id=\"{id}\"> … </{tag}>")
        }
        (Some(tag), Some(id), _) => format!("<{tag} id=\"{id}\">"),
        (Some(tag), _, _) if node.child_count == 0 => format!("<{tag} />"),
        (Some(tag), _, _) if DevToolsState::is_collapsed(&node.path) => format!("<{tag}> … </{tag}>"),
        (Some(tag), _, _) => format!("<{tag}>"),
        _ => node_label(node),
    }
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
    let hover_signal = use_signal(&format!("dt-row-{key}_hover"), || hovered);
    hover_signal.set_silent(hovered);
    let indent = node.depth as f32 * 10.0;
    let caption = tree_row_caption(node);
    let fold_path = path.clone();
    let disclosure: Element = if node.child_count > 0 {
        let arrow = inspector_icon(if DevToolsState::is_collapsed(&path) {
                "chevron-right"
            } else {
                "chevron-down"
            }, ">")
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
    let path_double_click = path.clone();
    let has_children = node.child_count > 0;
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
            .text_color(parse_text_color(if selected {
                "accent-foreground"
            } else {
                "primary"
            }))
            .text_size(11.0),
    )
    .text_wrap("char")
    .text_leading(2.0)
    .min_w(0.0)
    .flex_grow(1.0)
    .flex_shrink(1.0)
    .flex_basis(0.0);
    let mut row: Element = hstack()
        .id(format!("dt-row-{key}"))
        .w("100%")
        .min_w(0.0)
        .items("start")
        .h(22.0)
        .flex_shrink(0.0)
        .pl(indent)
        .bg(bg)
        .children([
            disclosure,
            text.font_family("Consolas"),
        ])
        .into();
    row.set_attr("jadix_devtools_composite", "1");
    crate::primitives::interactive::interactive(row)
        .focus_id(format!("dt-row-{key}"))
        .on_click(move || {
            DevToolsState::set_selected(
                path_click.clone(),
                identity_click.clone(),
                relative_click.clone(),
                size_click,
            );
            reveal_page_node(&path_click);
        })
        .on_hover(move |on| {
            if hover_signal.try_peek() != Some(on) {
                hover_signal.try_set(on);
                crate::primitives::interactive::mark_hover_paint_dirty();
            }
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
        .el
        .on_double_click(move |event| {
            if event.phase != crate::app::pointer_events::EventPhase::Capture
                && has_children
                && !event.target_focus_id.as_deref().is_some_and(|id| id.starts_with("dt-fold-"))
            {
                DevToolsState::toggle_collapsed(&path_double_click);
            }
        })
        .reactive_sync(move |row, _| {
            let selected = DevToolsState::is_selected_path_key(&key);
            let hovered = hover_signal.try_peek().unwrap_or(false)
                || (DevToolsState::is_pick() && DevToolsState::is_hover_path_key(&key));
            let bg = if selected {
                super::overlay::tree_row_color(true)
            } else if hovered {
                super::overlay::tree_row_color(false)
            } else {
                parse_color("background")
            };
            if row.style.bg_color == bg {
                LayoutStatus::Clean
            } else {
                row.style.bg_color = bg;
                LayoutStatus::StyleChanged
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

fn computed_pane(node: &InspectorNodeSnapshot, root: &Element) -> Element {
    let live_style = find_element_by_path(root, &node.path)
        .map(|element| super::snapshot::style_snapshot(&element.style));
    let style = live_style.as_ref().unwrap_or(&node.style);
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
    if let Some(element) = find_element_by_path(root, &node.path) {
        let text_style = &element.text_style;
        properties.extend([
            ("color", super::snapshot::format_text_rgba(text_style.color_resolved())),
            ("font-size", format!("{}px", text_style.size_resolved())),
            ("font-weight", text_style.weight_resolved().to_string()),
            ("font-family", text_style.font_family.clone().unwrap_or_default()),
        ]);
    }
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

fn element_style_declarations(element: &Element) -> String {
    let defaults = super::snapshot::cssom_text_from_element(&Element::new());
    let defaults: std::collections::HashSet<_> = defaults.lines().collect();
    super::snapshot::cssom_text_from_element(element)
        .lines()
        .filter(|line| !defaults.contains(line))
        .map(|line| format!("{line}\n"))
        .collect()
}

fn inherited_text_rows(root: &Element, path: &[usize]) -> Vec<(String, String)> {
    let Some(element) = find_element_by_path(root, path) else { return Vec::new(); };
    let specified = element.specified_text_style();
    let mut properties = std::collections::HashSet::new();
    if specified.color.is_none() { properties.insert("color".to_string()); }
    if specified.size.is_none() { properties.insert("font-size".to_string()); }
    if specified.weight.is_none() { properties.insert("font-weight".to_string()); }
    if specified.font_family.is_none() { properties.insert("font-family".to_string()); }
    let mut rows = Vec::new();
    for depth in (0..path.len()).rev() {
        let ancestor = find_element_by_path(root, &path[..depth]).unwrap();
        let declarations = super::snapshot::cssom_text_from_element(ancestor);
        for (name, value) in super::style_edit::parse_declarations(&declarations) {
            if properties.remove(&name) {
                rows.push((format!("{name} ← {}", super::snapshot::path_to_key(&path[..depth])), value));
            }
        }
    }
    rows
}

#[test]
fn element_style_does_not_claim_inherited_text_as_local() {
    let mut root = Element::new()
        .text_size(27.0)
        .font_weight(700.0)
        .text_color(parse_text_color("#123456"))
        .children([
            Element::new().text("inherited"),
            Element::new().text("explicit").text_size(27.0),
        ]);
    crate::layout::layout_tree::cascade_inheritable_text_style_for_test(&mut root);
    let inherited = element_style_declarations(&root.children[0]);
    assert!(!inherited.contains("font-size:"), "{inherited}");
    assert!(!inherited.contains("font-weight:"), "{inherited}");
    assert!(!inherited.contains("color:"), "{inherited}");
    assert!(element_style_declarations(&root.children[1]).contains("font-size: 27px;"));
    let inherited = inherited_text_rows(&root, &[0]);
    assert!(inherited.iter().any(|(name, value)| name.starts_with("font-size") && value == "27px"));
    root.text_style.size = Some(31.0);
    crate::layout::layout_tree::cascade_inheritable_text_style_for_test(&mut root);
    assert_eq!(root.children[0].text_style.size, Some(31.0));
    assert_eq!(root.children[1].text_style.size, Some(27.0));
    assert!(!element_style_declarations(&root.children[0]).contains("font-size:"));
}

fn element_style_editor(path: &[usize], instance_id: u64, css: &str) -> Element {
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
                                inspector_icon("check", "+")
                                .text_size(12.0),
                            )
                            .on_click(move || {
                                let current =
                                    super::style_edit::committed_inspector_cssom(&path_click, instance_id)
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
                                apply_cssom(path_click.clone(), instance_id, css);
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
            computed_pane(node, root),
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
    let instance_id = find_element_by_path(root, &path).map(|el| el.inspector_instance_id).unwrap_or(0);
    let cssom_seed = super::style_edit::committed_inspector_cssom(&path, instance_id)
        .or_else(|| {
            super::snapshot::find_element_by_path(root, &path)
                .map(element_style_declarations)
        })
        .unwrap_or_else(|| cssom_text_from_style(st));
    fields.push(element_style_editor(&path, instance_id, &cssom_seed));
    fields.push(section("Declarations"));
    fields.push(cssom_editor(path, instance_id, cssom_seed));

    let inherited = inherited_text_rows(root, &node.path);
    if !inherited.is_empty() {
        fields.push(section("Inherited text (read-only)"));
        for (name, value) in inherited {
            fields.push(kv(&name, &value));
        }
    }

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

fn apply_cssom(path: Vec<usize>, instance_id: u64, css: String) {
    super::queue_inspector_cssom(path, instance_id, css);
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

fn cssom_editor(path: Vec<usize>, instance_id: u64, seed: String) -> Element {
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
                    apply_cssom(path.clone(), instance_id, field_store_value(&field_id_click));
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
    fn properties_scrollbar_stays_stable_after_mount_and_rebuild() {
        use crate::app::state::{update_scrollbar_elements_for_test, ScrollbarUpdateMode};
        use crate::core::scroll_viewport::ScrollViewportSignalKeys;

        crate::core::reactivity::create_root(|| {
            DevToolsState::enable_for_window();
            DevToolsState::set_open(true);
            DevToolsState::set_tab(DevToolsTab::Elements);
            DevToolsState::set_panel_width(900.0);
            DevToolsState::set_computed(false);
            DevToolsState::set_selected(vec![0, 0, 0], "scroll-target", vec![], (0.0, 0.0));
            DevToolsState::bump();
            let app = Element::new().w("100%").h("100%").child(
                Element::new()
                    .template_id("scroll-target")
                    .text("Selected text")
                    .text_size(14.0),
            );
            let mut root = wrap_app(app);
            crate::app::state::recompute_scroll_viewport_flags(&mut root);
            let mut layout = crate::core::tree::LayoutTree::new();
            sync_overlay_layout(&mut root, &mut layout);
            let keys = ScrollViewportSignalKeys::new("devtools-card-properties");
            let mut expected = None;
            let mut expected_thumb = None;
            for frame in 0..16 {
                if frame == 8 {
                    DevToolsState::set_computed(false);
                    sync_overlay_layout(&mut root, &mut layout);
                }
                let overlay = frame % 3 == 1;
                let scroll_only = frame % 3 != 0;
                let offset = if frame < 3 { 0.0 } else { 120.0 };
                use_signal(&keys.offset_y, || 0.0_f32).set_silent(offset);
                update_scrollbar_elements_for_test(
                    &mut root,
                    &mut layout,
                    if overlay {
                        ScrollbarUpdateMode::ViewportsOnly
                    } else {
                        ScrollbarUpdateMode::Full
                    },
                );
                let result = if frame == 0 {
                    layout.build_and_layout(&mut root, 1400.0, 760.0, [-1.0; 2])
                } else {
                    layout.build_and_layout_fast(
                        &mut root,
                        1400.0,
                        760.0,
                        [-1.0; 2],
                        scroll_only && frame != 8,
                        overlay,
                    )
                };
                let (_, bounds, max_scroll, content) = result
                    .scroll_metrics
                    .iter()
                    .find(|entry| entry.0 == "devtools-card-properties")
                    .expect("properties viewport metrics");
                use_signal(&keys.viewport_w, || 0.0_f32).set_silent(bounds[2]);
                use_signal(&keys.viewport_h, || 0.0_f32).set_silent(bounds[3]);
                use_signal(&keys.content_w, || 0.0_f32).set_silent(content[0]);
                use_signal(&keys.content_h, || 0.0_f32).set_silent(content[1]);
                let metrics = [bounds[2], bounds[3], content[0], content[1], max_scroll[1]];
                if frame >= 3 {
                    let thumb = crate::core::tree::find_in_tree(&root, |element| {
                        element.focus_id.as_deref()
                            == Some("scrollbar-thumb:devtools-card-properties:y")
                    })
                    .expect("properties scrollbar thumb");
                    if let Some(previous) = expected_thumb {
                        assert_eq!(
                            thumb.layout_bounds, previous,
                            "thumb jumped on frame {frame}"
                        );
                    } else {
                        expected_thumb = Some(thumb.layout_bounds);
                    }
                }
                if frame >= 2 {
                    if let Some(previous) = expected {
                        assert_eq!(metrics, previous, "scroll metrics jumped on frame {frame}");
                    } else {
                        assert!(max_scroll[1] > 120.0, "fixture must scroll: {metrics:?}");
                        expected = Some(metrics);
                    }
                }
            }
            DevToolsState::set_open(false);
            sync_overlay_layout(&mut root, &mut layout);
            assert!(!root.contains_scroll_viewport);
            assert_eq!(root.scroll_viewport_subtree_count, 0);
        });
    }

    #[test]
    fn complete_tree_starts_collapsed_without_pagination() {
        crate::core::reactivity::create_root(|| {
            if !DevToolsState::is_collapsed(&[]) {
                DevToolsState::toggle_collapsed(&[]);
            }
            let root = Element::new().children(
                (0..600).map(|index| Element::new().focus_id(format!("complete-{index}"))),
            );
            let tree = snapshot_tree(&root);
            let outline = tree_outline(&tree, None);
            let mut row_count = 0;
            crate::core::tree::walk_elements(&outline, |element| {
                assert_ne!(element.template_id.as_deref(), Some("dt-tree-more"));
                if element.template_id.as_deref().is_some_and(|id| id.starts_with("dt-row-")) {
                    row_count += 1;
                }
            });
            assert_eq!(row_count, 1, "initial tree must show its collapsed root");
        });
    }

    #[test]
    fn element_style_editor_does_not_repeat_framework_defaults() {
        crate::core::reactivity::create_root(|| {
            let root = Element::new().children([
                Element::new(),
                Element::new().w(120.0).bg([1.0, 0.0, 0.0, 1.0]),
            ]);
            let tree = snapshot_tree(&root);
            for path in [vec![0], vec![1]] {
                let node = tree.iter().find(|node| node.path == path).unwrap();
                let details = details_pane(Some(node), &root);
                let key = super::super::snapshot::path_to_key(&path);
                assert!(super::super::snapshot::find_element_by_template(
                    &details, &format!("dt-decl-{key}-box-sizing-name")).is_none(),
                    "implicit defaults belong in Computed, not every element.style");
                let width = super::super::snapshot::find_element_by_template(
                    &details, &format!("dt-decl-{key}-width-name"));
                assert_eq!(width.is_some(), path == [1]);
            }
        });
    }

    #[test]
    fn expanded_large_tree_virtualizes_all_rows_and_reveals_unmounted_selection() {
        crate::core::reactivity::create_root(|| {
            let root = Element::new().children((0..5000).map(|index| {
                Element::new().focus_id(format!("large-{index}"))
                    .child(Element::new().text("leaf"))
            }));
            let tree = snapshot_tree(&root);
            for node in &tree {
                if node.child_count > 0 && DevToolsState::is_collapsed(&node.path) {
                    DevToolsState::toggle_collapsed(&node.path);
                }
            }
            let rest = super::super::overlay::ELEMENTS_TREE_SCROLL_ID;
            let keys = crate::core::scroll_viewport::ScrollViewportSignalKeys::new(rest);
            use_signal(&keys.viewport_w, || 0.0_f32).set_silent(1000.0);
            use_signal(&keys.viewport_h, || 0.0_f32).set_silent(220.0);
            use_signal(&keys.offset_y, || 0.0_f32).set_silent(0.0);
            DevToolsState::finish_tree_reveal();
            let mut body = tree_body(&tree, "");
            assert!(body.children.len() < 40, "must not mount the whole expanded tree");
            assert_eq!(crate::layout::dim_length_px(body.style.size.height), Some(330044.0));
            let target = vec![4900, 0];
            let key = format!("dt-row-{}", super::super::snapshot::path_to_key(&target));
            assert!(super::super::snapshot::find_element_by_template(&body, &key).is_none());
            DevToolsState::set_selected_path(target.clone());
            DevToolsState::request_tree_reveal(&target);
            let mut layout = crate::core::tree::LayoutTree::new();
            let sync = body.reactive_sync.clone().unwrap();
            assert_eq!(sync.run(&mut body, &mut layout), LayoutStatus::StructureChanged);
            assert!(super::super::snapshot::find_element_by_template(&body, &key).is_some());
            assert!(body.children.len() < 40);
            let expected = (4900.0 * 3.0 + 2.0) * 22.0 + 11.0 - 110.0;
            assert!((use_signal(&keys.offset_y, || 0.0_f32).peek() - expected).abs() < 0.5);
            assert!(DevToolsState::pending_tree_reveal().is_none());
            use_signal(&keys.offset_y, || 0.0_f32).set_silent(330000.0);
            sync.run(&mut body, &mut layout);
            assert!(super::super::snapshot::find_element_by_template(&body, "dt-row-4999,0").is_some());
            crate::app::scroll_support::cancel_scroll_item_into_view(rest);
        });
    }

    #[test]
    fn tree_row_selection_does_not_request_tree_scroll() {
        crate::core::reactivity::create_root(|| {
            let rest = super::super::overlay::ELEMENTS_TREE_SCROLL_ID;
            crate::app::scroll_support::cancel_scroll_item_into_view(rest);
            let tree = snapshot_tree(&Element::new().focus_id("selection-target"));
            let mut row = tree_row(&tree, &tree[0], None, None);
            row.layout_bounds = [0.0, 400.0, 800.0, 22.0];
            let viewport = Element::new()
                .focus_id(crate::core::scroll_viewport::focus_id_for_scroll_viewport(
                    rest,
                ))
                .child(row);
            for _ in 0..3 {
                (viewport.children[0].on_click.as_ref().unwrap().borrow_mut())();
                assert_eq!(DevToolsState::selected_path(), tree[0].path);
                let pending =
                    crate::app::scroll_support::resolve_pending_descendant_bounds(&viewport);
                crate::app::scroll_support::cancel_scroll_item_into_view(rest);
                assert!(
                    !pending.contains_key(rest),
                    "selecting a visible tree row must not scroll it"
                );
            }
        });
    }

    #[test]
    fn tree_scrollbars_stay_visible_only_while_scrollable() {
        crate::core::reactivity::create_root(|| {
            let card = inspector_card("Element tree", Element::new());
            let rest = super::super::overlay::ELEMENTS_TREE_SCROLL_ID;
            let key = crate::core::scrollbar::scrollbar_visibility_signal_key(rest, "y");
            assert_eq!(use_signal(&key, String::new).peek(), "always");
            assert!(super::super::snapshot::find_element_by_template(
                &card, &format!("__jadix_h_scrollbar_{rest}")).is_none());
            let keys = crate::core::scroll_viewport::ScrollViewportSignalKeys::new(rest);
            use_signal(&keys.viewport_w, || 0.0_f32).set_silent(480.0);
            let viewport_h = use_signal(&keys.viewport_h, || 0.0_f32);
            viewport_h.set_silent(100.0);
            let tree = snapshot_tree(&Element::new().children((0..20).map(|_| Element::new())));
            let rows: Vec<_> = (0..tree.len()).map(|index| (index, false)).collect();
            let mut body = Element::new();
            let mut geometry = (0.0, 0.0, Vec::new());
            sync_tree_window(&mut body, &tree, &rows, &mut geometry);
            assert_eq!(use_signal(&key, String::new).peek(), "always");
            assert!(body.style.padding.right != taffy::style::LengthPercentage::length(0.0));
            viewport_h.set_silent(1000.0);
            sync_tree_window(&mut body, &tree, &rows, &mut geometry);
            assert_eq!(use_signal(&key, String::new).peek(), "never");
            assert_eq!(body.style.padding.right, taffy::style::LengthPercentage::length(0.0));
            use_signal(&key, String::new).set_silent("always".to_string());
            let mut empty = tree_body(&tree, "missing-tree-node");
            let mut layout = crate::core::tree::LayoutTree::new();
            empty.reactive_sync.clone().unwrap().run(&mut empty, &mut layout);
            assert_eq!(use_signal(&key, String::new).peek(), "never");
        });
    }

    #[test]
    fn tree_double_click_toggles_branches_but_not_leaves_or_disclosures() {
        crate::core::reactivity::create_root(|| {
            let root = Element::new().child(Element::new());
            let tree = snapshot_tree(&root);
            let collapsed = DevToolsState::is_collapsed(&tree[0].path);
            let row = tree_row(&tree, &tree[0], None, None);
            let handler = row.on_double_click.as_ref().unwrap();
            let mut event = crate::app::pointer_events::MouseEvent::new(
                10.0, 10.0, winit::event::MouseButton::Left, 2);
            handler.borrow_mut()(&event);
            assert_eq!(DevToolsState::is_collapsed(&tree[0].path), !collapsed);
            event.target_focus_id = Some("dt-fold-".into());
            handler.borrow_mut()(&event);
            assert_eq!(DevToolsState::is_collapsed(&tree[0].path), !collapsed);
            event.target_focus_id = None;
            handler.borrow_mut()(&event);
            assert_eq!(DevToolsState::is_collapsed(&tree[0].path), collapsed);
            let leaf = tree_row(&tree, &tree[1], None, None);
            leaf.on_double_click.as_ref().unwrap().borrow_mut()(&event);
            assert!(DevToolsState::is_collapsed(&tree[1].path));
            let path = [
                crate::app::pointer_events::HitPathEntry::from_element(&row, [0.0, 0.0, 200.0, 22.0]),
                crate::app::pointer_events::HitPathEntry::from_element(&row.children[1], [16.0, 0.0, 184.0, 22.0]),
            ];
            crate::app::pointer_events::dispatch_pointer(
                &path, &mut event, crate::app::pointer_events::PointerKind::DoubleClick);
            assert_eq!(DevToolsState::is_collapsed(&tree[0].path), !collapsed,
                "caption bubbling must toggle the branch only once");
        });
    }

    #[test]
    fn wrapped_tree_rows_keep_disclosure_indent_and_only_reserve_scrollable_gutter() {
        crate::core::reactivity::create_root(|| {
            let root = Element::new().children((0..20).map(|index| {
                Element::new().focus_id(format!("wrapped-tree-node-{index}-with-a-long-identifier"))
            }));
            let tree = snapshot_tree(&root);
            if DevToolsState::is_collapsed(&[]) { DevToolsState::toggle_collapsed(&[]); }
            let keys = crate::core::scroll_viewport::ScrollViewportSignalKeys::new(
                super::super::overlay::ELEMENTS_TREE_SCROLL_ID);
            use_signal(&keys.viewport_w, || 0.0_f32).set_silent(220.0);
            use_signal(&keys.viewport_h, || 0.0_f32).set_silent(100.0);
            let mut body = tree_body(&tree, "");
            assert_ne!(body.style.padding.right, taffy::style::LengthPercentage::length(0.0));
            let mut layout = crate::core::tree::LayoutTree::new();
            let result = layout.build_and_layout(&mut body, 220.0, 2000.0, [-1.0; 2]);
            let row = &body.children[1];
            assert!(row.layout_bounds[3] > 22.0, "long node must wrap");
            let caption = &row.children[1];
            assert!((caption.layout_bounds[0] - row.layout_bounds[0] - 26.0).abs() < 0.1);
            assert!(caption.layout_bounds[0] + caption.layout_bounds[2] <= 220.1);
            assert!(result.text_nodes.iter().any(|text| text.text.contains("wrapped-tree-node")));
            use_signal(&keys.viewport_h, || 0.0_f32).set_silent(10000.0);
            body.reactive_sync.clone().unwrap().run(&mut body, &mut layout);
            assert_eq!(body.style.padding.right, taffy::style::LengthPercentage::length(0.0));
        });
    }

    #[test]
    fn narrow_header_navigation_retains_horizontal_scroll_extent() {
        crate::core::reactivity::create_root(|| {
            let mut root = panel_header(false, DevToolsTab::Elements);
            let mut tree = crate::core::tree::LayoutTree::new();
            let result = tree.build_and_layout(&mut root, 400.0, 34.0, [-1.0; 2]);
            let (_, bounds, max_scroll, content) = result
                .scroll_metrics
                .iter()
                .find(|(id, _, _, _)| id == "dt-nav")
                .unwrap();
            assert!(bounds[2] > 0.0);
            assert!(
                max_scroll[0] > 100.0,
                "bounds={bounds:?} max={max_scroll:?} content={content:?}"
            );
            assert!(max_scroll[1] < 0.5);
            let first =
                super::super::snapshot::find_element_by_template(&root, "dt-tab-el").unwrap();
            let last =
                super::super::snapshot::find_element_by_template(&root, "dt-tab-changes").unwrap();
            assert!(last.layout_bounds[0] > first.layout_bounds[0] + bounds[2]);
        });
    }

    #[test]
    fn tree_selection_repaints_the_entire_row_including_indentation() {
        crate::core::reactivity::create_root(|| {
            let app = Element::new().child(Element::new());
            let tree = snapshot_tree(&app);
            DevToolsState::set_selected_path(tree[0].path.clone());
            let row = tree_row(&tree, &tree[1], None, None);
            let mut root = Element::new().w(220.0).h(100.0).child(row);
            let mut layout = crate::core::tree::LayoutTree::new();
            layout.build_and_layout(&mut root, 220.0, 100.0, [-1.0; 2]);
            crate::app::damage::take_frame_paint_nodes();
            DevToolsState::set_selected_path(tree[1].path.clone());
            let mut impact = LayoutStatus::Clean;
            crate::core::tree::sync_reactive_elements_subtree(
                &mut root, &mut layout, &mut impact, 220.0, 100.0, [0.0; 2]);
            let damage = crate::app::damage::take_frame_paint_nodes();
            assert!(damage.iter().any(|dirty| {
                dirty.kind == crate::app::damage::PaintDirtyKind::Region
                    && dirty.bounds[0] <= 0.0
                    && dirty.bounds[0] + dirty.bounds[2] >= 220.0
            }), "selection damage must restore the full row: {damage:?}");
        });
    }

    #[test]
    fn replacing_tree_rows_invalidates_the_visible_viewport_not_only_old_content() {
        crate::core::reactivity::create_root(|| {
            let mut root = Element::new().w(320.0).h(300.0).child(
                Element::new().w(320.0).h(240.0).overflow("hidden").child(
                    Element::new().template_id("dt-tree-rows").w(320.0).h(22.0)));
            let mut layout = crate::core::tree::LayoutTree::new();
            layout.build_and_layout(&mut root, 320.0, 300.0, [-1.0; 2]);
            crate::app::damage::take_frame_paint_nodes();
            replace_panel_region(&mut root, "dt-tree-rows",
                Element::new().template_id("dt-tree-rows").w(320.0).h(220.0));
            let damage = crate::app::damage::take_frame_paint_nodes();
            assert!(damage.iter().any(|dirty| {
                dirty.bounds[0] <= 0.0 && dirty.bounds[1] <= 0.0
                    && dirty.bounds[0] + dirty.bounds[2] >= 320.0
                    && dirty.bounds[1] + dirty.bounds[3] >= 240.0
            }), "expanded rows below the previous body must repaint: {damage:?}");
        });
    }

    #[test]
    fn chrome_and_selection_updates_preserve_unaffected_controls() {
        crate::core::reactivity::create_root(|| {
            DevToolsState::enable_for_window();
            DevToolsState::set_open(true);
            DevToolsState::set_panel_width(640.0);
            DevToolsState::set_tab(DevToolsTab::Elements);
            DevToolsState::set_pick(false);
            DevToolsState::bump();
            let app = Element::new().w("100%").h("100%").child(
                Element::new()
                    .template_id("scoped-selection-target")
                    .text("target"),
            );
            let mut root = wrap_app(app);
            let mut layout = crate::core::tree::LayoutTree::new();
            sync_overlay_layout(&mut root, &mut layout);
            layout.build_and_layout(&mut root, 1400.0, 900.0, [-1.0; 2]);
            let query_id = crate::utils::text_field::input_text_template_id("dt-tree-query");
            let query_node = super::super::snapshot::find_element_by_template(&root, &query_id)
                .unwrap()
                .layout_node;
            let picker = super::super::snapshot::find_element_by_template(&root, "dt-pick")
                .unwrap()
                .on_click
                .clone()
                .unwrap();
            DevToolsState::set_pick(true);
            assert_eq!(
                sync_overlay_layout(&mut root, &mut layout),
                LayoutStatus::StyleChanged
            );
            assert_eq!(
                super::super::snapshot::find_element_by_template(&root, &query_id)
                    .unwrap()
                    .layout_node,
                query_node
            );
            assert!(std::rc::Rc::ptr_eq(
                &picker,
                super::super::snapshot::find_element_by_template(&root, "dt-pick")
                    .unwrap()
                    .on_click
                    .as_ref()
                    .unwrap()
            ));
            DevToolsState::set_selected(
                vec![0, 0, 0],
                "scoped-selection-target",
                vec![],
                (0.0, 0.0),
            );
            sync_overlay_layout(&mut root, &mut layout);
            assert_eq!(
                super::super::snapshot::find_element_by_template(&root, &query_id)
                    .unwrap()
                    .layout_node,
                query_node
            );
            layout.build_and_layout_fast(&mut root, 1400.0, 900.0, [-1.0; 2], false, false);
            let editor_id =
                crate::utils::text_field::input_text_template_id(&cssom_input_id(&[0, 0, 0]));
            let editor = super::super::snapshot::find_element_by_template(&root, &editor_id)
                .unwrap()
                .layout_node;
            DevToolsState::set_pick(false);
            sync_overlay_layout(&mut root, &mut layout);
            assert_eq!(
                super::super::snapshot::find_element_by_template(&root, &editor_id)
                    .unwrap()
                    .layout_node,
                editor
            );
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
        for source in [
            include_str!("mod.rs"),
            include_str!("controls.rs"),
            include_str!("device.rs"),
            include_str!("locale.rs"),
            include_str!("overlay.rs"),
            include_str!("panel.rs"),
            include_str!("presentation.rs"),
            include_str!("reactivity_panel.rs"),
            include_str!("snapshot.rs"),
            include_str!("source.rs"),
            include_str!("state.rs"),
            include_str!("style_edit.rs"),
        ] {
            assert!(
                !source.contains(concat!("crate::", "components")),
                "DevTools must use the shared foundation, not product components"
            );
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
            assert_ne!(
                sync_overlay_layout(&mut root, &mut tree),
                LayoutStatus::Clean
            );
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
                assert_ne!(
                    sync_overlay_layout(&mut root, &mut tree),
                    LayoutStatus::Clean
                );
                tree.build_and_layout_fast(&mut root, 1200.0, 700.0, [-1.0, -1.0], false, false);
                assert!(
                    (root.children[0].layout_bounds[2] - expected).abs() < 1.0,
                    "app did not reclaim width after open={open}"
                );
                if open {
                    let panel = super::super::snapshot::find_element_by_template(&root, PANEL_ID)
                        .expect("open DevTools panel missing");
                    assert!(
                        (panel.layout_bounds[2] - width).abs() < 1.0,
                        "DevTools panel width did not follow state: got={} want={width}",
                        panel.layout_bounds[2]
                    );
                }
            }
        });
    }

    #[test]
    fn closing_devtools_dismisses_hovered_retained_tooltips() {
        crate::core::reactivity::create_root(|| {
            DevToolsState::enable_for_window();
            DevToolsState::set_open(true);
            DevToolsState::bump();
            let mut root = wrap_app(Element::new());
            sync_overlay(&mut root);
            let close = super::super::snapshot::find_element_by_template(&root, "dt-close")
                .expect("close button");
            let hover = close.on_hover.clone().unwrap();
            (hover.borrow_mut())(true);
            let open = use_signal("dt-close:tooltip:open", || false);
            assert!(open.peek());
            DevToolsState::set_open(false);
            sync_overlay(&mut root);
            assert!(!open.peek(), "unmount must close the retained tooltip without mouseleave");
            assert!(super::super::snapshot::find_element_by_template(&root, PANEL_ID).is_none());
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
    fn breadcrumbs_reveal_selected_node_in_horizontal_only_viewport() {
        crate::core::reactivity::create_root(|| {
            let mut root = Element::new().focus_id("selected-leaf");
            for index in 0..12 {
                let mut parent = Element::new().child(root);
                parent.set_attr("data-slot", format!("ancestor-{index}"));
                root = parent;
            }
            let tree = snapshot_tree(&root);
            let selected = tree.iter().find(|node| {
                node.focus_id.as_deref() == Some("selected-leaf")
            });
            let mut crumbs = breadcrumbs(&tree, selected);
            let mut layout = crate::core::tree::LayoutTree::new();
            let result = layout.build_and_layout(&mut crumbs, 240.0, 32.0, [-1.0; 2]);
            let (rest, viewport, max_scroll, content) = result.scroll_metrics.iter()
                .find(|(rest, _, _, _)| rest == "dt-breadcrumbs")
                .expect("breadcrumb scroll metrics missing");
            assert!(max_scroll[0] > 0.0);
            assert_eq!(max_scroll[1], 0.0);
            let targets = crate::app::scroll_support::resolve_pending_descendant_bounds(&crumbs);
            let target = targets[rest];
            assert!(crate::app::scroll_support::apply_pending_scroll_align(
                rest, *viewport, content[0], content[1], Some(target),
            ));
            let offset = crate::core::reactivity::use_signal(
                &crate::core::scroll_viewport::scroll_offset_x_signal_key(rest),
                || 0.0_f32,
            ).peek();
            assert!(offset > 0.0, "selected breadcrumb was not revealed");
            assert!(target[0] - offset >= viewport[0] - 0.5);
            assert!(target[0] + target[2] - offset <= viewport[0] + viewport[2] + 0.5);
        });
    }

    #[test]
    fn breadcrumbs_keep_arrows_aligned_and_mark_the_selected_node() {
        crate::core::reactivity::create_root(|| {
            let root = Element::new().focus_id("root").child(
                Element::new()
                    .focus_id("parent")
                    .child(Element::new().focus_id("child")),
            );
            let tree = snapshot_tree(&root);
            let selected = tree
                .iter()
                .find(|node| node.focus_id.as_deref() == Some("child"));
            let mut breadcrumbs = breadcrumbs(&tree, selected);
            let mut layout = crate::core::tree::LayoutTree::new();
            layout.build_and_layout(&mut breadcrumbs, 360.0, 32.0, [-1.0; 2]);

            let selected_key = super::super::snapshot::path_to_key(&selected.unwrap().path);
            let selected_crumb = super::super::snapshot::find_element_by_template(
                &breadcrumbs,
                &format!("dt-crumb-{selected_key}"),
            )
            .expect("selected breadcrumb missing");
            assert_eq!(
                selected_crumb.style.bg_color[3],
                parse_color("secondary")[3]
            );
            for focus_id in ["root", "parent", "child"] {
                let node = tree
                    .iter()
                    .find(|node| node.focus_id.as_deref() == Some(focus_id))
                    .unwrap();
                let key = super::super::snapshot::path_to_key(&node.path);
                assert!(
                    super::super::snapshot::find_element_by_template(
                        &breadcrumbs,
                        &format!("dt-crumb-{key}"),
                    )
                    .is_some(),
                    "breadcrumb node {focus_id} missing"
                );
            }
        });
    }

    #[test]
    fn closing_tag_uses_available_row_width_and_supports_wrapping() {
        crate::core::reactivity::create_root(|| {
            let mut parent = Element::new().child(Element::new());
            parent.set_attr("data-slot", "sidebar-menu-button");
            let tree = snapshot_tree(&parent);
            let mut row = closing_tree_row(&tree[0]);
            let mut layout = crate::core::tree::LayoutTree::new();
            layout.build_and_layout(&mut row, 360.0, 22.0, [-1.0; 2]);
            let caption = &row.children[0];
            assert_eq!(
                caption.text_style.white_space,
                crate::style::WhiteSpace::Normal
            );
            assert!(
                caption.layout_bounds[2] >= 300.0,
                "closing tag prematurely constrained: {:?}",
                caption.layout_bounds
            );
            assert!(caption.layout_bounds[3] <= 22.0);
        });
    }

    #[test]
    fn tree_find_preserves_the_search_draft_when_the_find_button_takes_focus() {
        crate::core::reactivity::create_root(|| {
            DevToolsState::set_tree_filter(String::new());
            let tree = snapshot_tree(&Element::new().child(Element::new().focus_id("checkbox-target")));
            let field = use_signal(&input_value_signal_key("dt-tree-query"), String::new);
            field.set_silent("checkbox".into());
            use_signal("__jadix_button_focus_id", String::new).set_silent("dt-tree-filter".into());
            let outline = tree_outline(&tree, None);
            assert_eq!(field.peek(), "checkbox");
            let find = super::super::snapshot::find_element_by_template(&outline, "dt-tree-filter").unwrap();
            (find.on_click.as_ref().unwrap().borrow_mut())();
            assert_eq!(DevToolsState::selected_path(), vec![0]);
            field.set_silent(String::new());
            DevToolsState::set_tree_filter(String::new());
            DevToolsState::finish_tree_reveal();
        });
    }

    #[test]
    fn tree_find_uses_updated_nodes_after_outline_refresh() {
        crate::core::reactivity::create_root(|| {
            DevToolsState::enable_for_window();
            DevToolsState::set_open(true);
            DevToolsState::set_tab(DevToolsTab::Elements);
            DevToolsState::bump();
            DevToolsState::set_selected_path(Vec::new());
            DevToolsState::set_tree_filter(String::new());
            let mut root = wrap_app(Element::new().template_id("search-page")
                .child(Element::new().focus_id("old-search-target")));
            sync_overlay(&mut root);
            let page = find_element_by_template_mut(&mut root, "search-page").unwrap();
            page.children = vec![Element::new().focus_id("fresh-search-target")];
            DevToolsState::refresh_outline();
            sync_overlay(&mut root);
            let expected = snapshot_tree(&root).into_iter()
                .find(|node| node.focus_id.as_deref() == Some("fresh-search-target")).unwrap().path;
            use_signal(&input_value_signal_key("dt-tree-query"), String::new)
                .set_silent("fresh-search-target".to_string());
            let find = super::super::snapshot::find_element_by_template(&root, "dt-tree-filter").unwrap();
            (find.on_click.as_ref().unwrap().borrow_mut())();
            assert_eq!(DevToolsState::selected_path(), expected);
            DevToolsState::set_tree_filter(String::new());
            DevToolsState::finish_tree_reveal();
            DevToolsState::set_open(false);
        });
    }

    #[test]
    fn tree_find_selects_collapsed_descendants_in_document_order_and_wraps() {
        let root = Element::new().children(vec![
            Element::new()
                .focus_id("parent")
                .child(Element::new().focus_id("needle")),
            Element::new().focus_id("other-needle"),
        ]);
        let tree = snapshot_tree(&root);
        if DevToolsState::is_collapsed(&[]) {
            DevToolsState::toggle_collapsed(&[]);
        }
        if DevToolsState::is_collapsed(&[0]) {
            DevToolsState::toggle_collapsed(&[0]);
        }
        DevToolsState::toggle_collapsed(&[0]);
        assert_eq!(visible_tree(&tree, "").len(), 3);
        DevToolsState::set_tree_filter(String::new());
        find_next_tree_node(&tree, "NEEDLE".into());
        assert_eq!(DevToolsState::selected_path(), vec![0, 0]);
        assert!(!DevToolsState::is_collapsed(&[0]));
        assert_eq!(DevToolsState::pending_tree_reveal().as_deref(), Some("0,0"));
        find_next_tree_node(&tree, "needle".into());
        assert_eq!(DevToolsState::selected_path(), vec![1]);
        find_next_tree_node(&tree, "needle".into());
        assert_eq!(DevToolsState::selected_path(), vec![0, 0]);
        find_next_tree_node(&tree, "missing".into());
        assert_eq!(DevToolsState::selected_path(), vec![0, 0]);
        DevToolsState::finish_tree_reveal();
        DevToolsState::set_tree_filter(String::new());
    }

    #[test]
    fn selecting_offscreen_page_node_centers_only_its_scroll_viewport() {
        crate::core::reactivity::create_root(|| {
            use crate::core::scroll_viewport::{activate_scroll_viewport, ScrollViewportSignalKeys};
            let rest = "devtools-page-reveal-test";
            let mut root = activate_scroll_viewport(Element::new().w(300.0).h(200.0)
                .child(Element::new().w(300.0).h(1000.0).min_h(1000.0).flex_shrink(0.0)
                    .child(Element::new().template_id("offscreen-target").absolute()
                        .top(600.0).w(80.0).h(40.0))), rest, "auto", "auto", "jump", "scrollbar");
            let mut layout = crate::core::tree::LayoutTree::new();
            let result = layout.build_and_layout(&mut root, 300.0, 200.0, [-1.0; 2]);
            let keys = ScrollViewportSignalKeys::new(rest);
            use_signal(&keys.content_h, || 0.0_f32).set_silent(result.scroll_metrics[0].3[1]);
            use_signal(&keys.viewport_h, || 0.0_f32).set_silent(200.0);
            reveal_page_node_in_root(&root, &[0, 0]);
            assert_eq!(use_signal(&keys.offset_y, || 0.0_f32).peek(), 520.0);
            layout.build_and_layout(&mut root, 300.0, 200.0, [-1.0; 2]);
            reveal_page_node_in_root(&root, &[0, 0]);
            assert_eq!(use_signal(&keys.offset_y, || 0.0_f32).peek(), 520.0);
        });
    }

    #[test]
    fn scaled_page_reveal_keeps_the_selected_target_visible_after_scroll() {
        crate::core::reactivity::create_root(|| {
            use crate::core::scroll_viewport::{activate_scroll_viewport, ScrollViewportSignalKeys};
            let rest = "devtools-scaled-page-reveal";
            let viewport = activate_scroll_viewport(Element::new().w(300.0).h(200.0)
                .child(Element::new().w(300.0).h(1000.0).min_h(1000.0).flex_shrink(0.0)
                    .child(Element::new().template_id("scaled-offscreen-target").absolute()
                        .top(600.0).w(80.0).h(40.0))), rest, "auto", "auto", "jump", "scrollbar");
            let mut root = Element::new().w(800.0).h(700.0).child(
                Element::new().absolute().left(80.0).top(90.0).w(390.0).h(844.0)
                    .scale(0.57).transform_origin("left top").child(viewport));
            let mut layout = crate::core::tree::LayoutTree::new();
            let result = layout.build_and_layout(&mut root, 800.0, 700.0, [-1.0; 2]);
            let keys = ScrollViewportSignalKeys::new(rest);
            use_signal(&keys.content_h, || 0.0_f32).set_silent(result.scroll_metrics[0].3[1]);
            use_signal(&keys.viewport_h, || 0.0_f32).set_silent(200.0);
            reveal_page_node_in_root(&root, &[0, 0, 0, 0]);
            layout.build_and_layout(&mut root, 800.0, 700.0, [-1.0; 2]);
            let viewport = &root.children[0].children[0];
            let target = &viewport.children[0].children[0];
            let bounds = crate::core::tree::visual_paint_aabb(viewport, viewport.paint_scroll_acc);
            let target_box = crate::core::tree::visual_paint_aabb(target, target.paint_scroll_acc);
            assert!((target_box[1] + target_box[3] * 0.5 - bounds[1] - bounds[3] * 0.5).abs() < 1.0,
                "search scrolled away from the target: viewport={bounds:?} target={target_box:?}");
        });
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
                assert_eq!(
                    panel.children.len(),
                    4,
                    "header, body, footer and resize handle"
                );
                assert!(
                    super::super::snapshot::find_element_by_template(&panel, "dt-tabs").is_none()
                );
                let header = super::super::snapshot::find_element_by_template(&panel, "dt-header")
                    .expect("header is present");
                assert!(
                    super::super::snapshot::find_element_by_template(header, "dt-nav").is_some()
                );
                for (id, title, candidate) in devtools_tabs() {
                    let button = super::super::snapshot::find_element_by_template(&header, id)
                        .unwrap_or_else(|| panic!("missing navigation button {id}"));
                    assert_eq!(
                        button.attrs.get("aria-label").map(String::as_str),
                        Some(title)
                    );
                    assert_eq!(button.attrs.get("role").map(String::as_str), Some("tab"));
                    assert_eq!(
                        button.attrs.get("aria-selected").map(String::as_str),
                        Some(if tab == candidate { "true" } else { "false" })
                    );
                    assert!(button
                        .class_set
                        .as_ref()
                        .is_some_and(|set| set.has_pointer_hover_variants()));
                }
            }
        });
    }

    #[test]
    fn inspector_tabs_hover_for_selected_and_unselected_items() {
        let selected = inspector_tab("selected", "Styles", true, || {});
        let unselected = inspector_tab("unselected", "Computed", false, || {});
        for tab in [&selected, &unselected] {
            let classes = tab.class_set.as_ref().expect("hover variants are retained");
            assert!(classes.has_pointer_hover_variants());
            assert_eq!(tab.attrs.get("role").map(String::as_str), Some("tab"));
        }
        assert_ne!(selected.style.bg_color, unselected.style.bg_color);
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
    fn docked_content_breakpoints_use_the_remaining_window_width() {
        crate::core::reactivity::create_root(|| {
            DevToolsState::enable_for_window();
            DevToolsState::set_open(true);
            DevToolsState::set_panel_width(640.0);
            super::super::device::disable();
            let mut app = Element::new().focus_id("responsive-app");
            crate::style::apply_class_list(&mut app, "w-24 md:w-48 p-2 md:p-8 hover:bg-red-500");
            let mut root = wrap_app(app);
            let mut layout = crate::core::tree::LayoutTree::new();
            for (window_width, open, panel_width, expected_width) in [
                (1200.0, true, 640.0, 96.0),
                (1500.0, true, 640.0, 192.0),
                (1500.0, true, 900.0, 96.0),
                (1200.0, false, 900.0, 192.0),
                (1200.0, true, 320.0, 192.0),
                (600.0, true, 900.0, 96.0),
            ] {
                crate::utils::modal_motion::set_modal_viewport_size(window_width, 900.0);
                DevToolsState::set_open(open);
                DevToolsState::set_panel_width(panel_width);
                sync_overlay_layout(&mut root, &mut layout);
                let expected_viewport = if open {
                    (window_width - panel_width).max(1.0)
                } else {
                    window_width
                };
                assert_eq!(
                    crate::utils::modal_motion::modal_viewport_size(),
                    (expected_viewport, 900.0)
                );
                let mut inspector = Element::new();
                crate::style::apply_class_list(&mut inspector, "w-24 md:w-48");
                root.children[1].children.push(inspector);
                let mut impact = LayoutStatus::Clean;
                crate::style::sync_class_variants_subtree(
                    &mut root,
                    [-1.0; 2],
                    "",
                    false,
                    window_width,
                    900.0,
                    &mut layout,
                    &mut impact,
                );
                assert_eq!(
                    root.children[0].children[0].style.size.width,
                    taffy::style::Dimension::length(expected_width),
                    "window={window_width}, dock={panel_width}, open={open}"
                );
                assert_eq!(
                    root.children[1].children.last().unwrap().style.size.width,
                    taffy::style::Dimension::length(if window_width >= 768.0 { 192.0 } else { 96.0 }),
                    "Inspector must keep its own breakpoint scope"
                );
            }
            DevToolsState::set_open(false);
        });
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
        use crate::core::reactivity::create_root;
        use crate::style::WhiteSpace;
        use crate::utils::text_field::input_text_template_id;
        create_root(|| {
            let path = vec![0usize];
            let field = cssom_input_id(&path);
            let tid = input_text_template_id(&field);
            let el = cssom_editor(
                path,
                0,
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
        use crate::style::parse_color;
        use crate::utils::text_field::input_value_signal_key;

        create_root(|| {
            super::super::style_edit::clear_committed_inspector_edits_for_test();
            let path = vec![0usize];
            let applied = "background: #00ff00;\nopacity: 0.5;\n";

            // Live style has the apply; authored button dump would show ghost transparent.
            let mut el = Element::new();
            el.authored_style = Some(|element| {
                let mut style = element.style.as_ref().clone();
                style.bg_color = [0.0; 4];
                style.border_width = 0.0;
                Some(style)
            });
            el.style.bg_color = parse_color("#00ff00");
            el.style.opacity = 0.5;
            let root = Element::new().child(el);
            super::super::style_edit::commit_inspector_cssom(&root, &path, applied.into());
            let authored =
                super::super::snapshot::cssom_text_from_element(root.children.first().unwrap());
            assert_ne!(
                authored, applied,
                "authored dump must differ from applied text to prove preference"
            );

            let seed = super::super::style_edit::committed_inspector_cssom(&path, root.children[0].inspector_instance_id)
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
