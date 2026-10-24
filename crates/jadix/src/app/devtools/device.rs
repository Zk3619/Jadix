use std::cell::{Cell, RefCell};

use super::controls::{button, input, label, select};
use crate::core::reactivity::use_signal;
use crate::core::tree::{Element, LayoutStatus, LayoutTree};
use crate::primitives::{hstack, vstack};
use crate::style::{parse_color, Style as ElementStyle};
use crate::utils::text_field::input_value_signal_key;

use super::{locale::text, snapshot::find_element_by_template_mut, DevToolsState};

const TOOLBAR: &str = "__jadix_devtools_device_toolbar";
pub(crate) const PORTAL_HOST: &str = "__jadix_device_portals";
const FRAME: &str = "__jadix_devtools_device_frame";
const ACTIVE: &str = "data-jadix-device-preview";
const BAR_HEIGHT: f32 = 78.0;
const PRESETS: &[(&str, f32, f32)] = &[
    ("Responsive", 390.0, 844.0),
    ("iPhone SE", 375.0, 667.0),
    ("iPhone 16", 393.0, 852.0),
    ("Pixel 9", 412.0, 924.0),
    ("Tablet", 768.0, 1024.0),
    ("Desktop", 1440.0, 900.0),
    ("Full HD", 1920.0, 1080.0),
];

#[derive(Clone, Copy)]
struct Viewport {
    width: f32,
    height: f32,
    zoom: f32,
}

thread_local! {
    static RESIZE: Cell<Option<(f32, f32, Viewport, f32)>> = const { Cell::new(None) };
    static ENABLED: Cell<bool> = const { Cell::new(false) };
    static VIEWPORT: Cell<Viewport> = const { Cell::new(Viewport { width: 390.0, height: 844.0, zoom: 0.0 }) };
    static ORIGINAL: RefCell<Option<Box<ElementStyle>>> = const { RefCell::new(None) };
    static LAST_PRESET: RefCell<String> = RefCell::new("Responsive".into());
    static PAINT_SCALE: Cell<f32> = const { Cell::new(1.0) };
    static RESIZE_LIMIT: Cell<[f32; 2]> = const { Cell::new([7680.0, 4320.0]) };
}

pub(super) fn enabled() -> bool {
    ENABLED.get()
}

pub(crate) fn viewport_size() -> Option<(f32, f32)> {
    if !enabled()
        || !DevToolsState::is_open()
        || crate::app::window_scope::active() != crate::app::window_scope::WindowScope::Primary
    {
        return None;
    }
    let viewport = VIEWPORT.get();
    Some((viewport.width, viewport.height))
}

pub(super) fn disable() {
    ENABLED.set(false);
    stop_resize();
}

pub(super) fn toggle() {
    ENABLED.set(!ENABLED.get());
    DevToolsState::bump_chrome();
}

fn seed_dimensions(viewport: Viewport) {
    for (id, value) in [
        ("dt-device-width", viewport.width),
        ("dt-device-height", viewport.height),
    ] {
        use_signal(&input_value_signal_key(id), String::new).set_silent(format!("{value:.0}"));
    }
}

fn toolbar() -> Element {
    let viewport = VIEWPORT.get();
    let preset = use_signal("dt-device-preset-value", || "Responsive".to_string());
    let zoom = use_signal("dt-device-zoom-value", || "fit".to_string());
    let controls: Element = hstack()
        .w("100%")
        .gap(6.0)
        .items("center")
        .children([
            select("dt-device-preset")
                .value_signal(preset)
                .trigger_width("140px")
                .options(
                    PRESETS
                        .iter()
                        .map(|(name, _, _)| (name.to_string(), name.to_string()))
                        .collect(),
                )
                .into(),
            select("dt-device-zoom")
                .value_signal(zoom)
                .trigger_width("90px")
                .options(
                    [
                        ("fit", "Fit".to_string()),
                        ("25", "25%".into()),
                        ("50", "50%".into()),
                        ("75", "75%".into()),
                        ("100", "100%".into()),
                        ("150", "150%".into()),
                    ]
                    .into_iter()
                    .map(|(key, value)| (key.to_string(), value))
                    .collect(),
                )
                .into(),
            button("dt-device-rotate")
                .size("sm")
                .variant("ghost")
                .text(text("Rotate"))
                .on_click(|| {
                    let current = VIEWPORT.get();
                    let next = Viewport {
                        width: current.height,
                        height: current.width,
                        ..current
                    };
                    VIEWPORT.set(next);
                    seed_dimensions(next);
                    crate::app::runtime::request_redraw();
                })
                .into(),
        ])
        .into();
    let dimensions: Element = hstack()
        .w("100%")
        .gap(5.0)
        .items("center")
        .children([
            input("dt-device-width")
                .h(27.0)
                .w(72.0)
                .text_size(11.0)
                .into(),
            label("dt-device-by").text("×").text_size(12.0).into(),
            input("dt-device-height")
                .h(27.0)
                .w(72.0)
                .text_size(11.0)
                .into(),
            button("dt-device-apply")
                .size("sm")
                .variant("outline")
                .focus_ring(false)
                .text(text("Apply"))
                .on_click(|| {
                    let width =
                        use_signal(&input_value_signal_key("dt-device-width"), String::new).peek();
                    let height =
                        use_signal(&input_value_signal_key("dt-device-height"), String::new).peek();
                    if let (Ok(width), Ok(height)) =
                        (width.trim().parse::<f32>(), height.trim().parse::<f32>())
                    {
                        if width.is_finite() && height.is_finite() {
                            VIEWPORT.set(Viewport {
                                width: width.clamp(240.0, 7680.0),
                                height: height.clamp(240.0, 4320.0),
                                ..VIEWPORT.get()
                            });
                            seed_dimensions(VIEWPORT.get());
                            crate::app::runtime::request_redraw();
                        }
                    }
                })
                .into(),
            label("dt-device-size")
                .text(format!("{:.0} × {:.0}", viewport.width, viewport.height))
                .text_size(10.0)
                .into(),
        ])
        .into();
    Element::from(
        vstack()
            .w("100%")
            .h(BAR_HEIGHT)
            .p(5.0)
            .gap(3.0)
            .bg(parse_color("background"))
            .children([controls, dimensions]),
    )
    .template_id(TOOLBAR)
    .absolute()
    .left(0.0)
    .top(0.0)
    .z_index(30)
    .border_bottom(parse_color("border"), 1.0)
}

pub(crate) fn scope_portals(root: &mut Element) -> bool {
    use crate::core::scaled_viewport::ATTR_SCOPE;
    fn configure(node: &mut Element, active: bool, preview: bool) {
        if matches!(node.template_id.as_deref(), Some("__jadix_app_host") | Some(TOOLBAR)) {
            if active {
                let mount = if preview && node.template_id.as_deref() != Some(TOOLBAR) { PORTAL_HOST } else { "__jadix_app_host" };
                node.set_attr(ATTR_SCOPE, mount);
            } else {
                node.attrs.remove(ATTR_SCOPE);
            }
        }
        for child in &mut node.children { configure(child, active, preview); }
    }
    let active = DevToolsState::is_open();
    configure(root, active, enabled() && active);
    crate::core::scaled_viewport::scope_portals(root)
}

fn stop_resize() {
    if RESIZE.replace(None).is_some() {
        crate::app::release_pointer_capture();
    }
}

fn resize_grip(id: &'static str, horizontal: bool, vertical: bool) -> Element {
    use crate::app::pointer_events::{EventPhase, PointerEvent};
    let cursor = match (horizontal, vertical) {
        (true, true) => crate::style::CursorKind::NwseResize,
        (true, false) => crate::style::CursorKind::ColResize,
        _ => crate::style::CursorKind::RowResize,
    };
    Element::new()
        .focus_id(id)
        .absolute()
        .cursor_kind(cursor)
        .bg(parse_color("border"))
        .on_pointer_down(move |event: &PointerEvent| {
            if event.phase == EventPhase::Capture || event.button != winit::event::MouseButton::Left
            {
                return;
            }
            RESIZE.set(Some((event.x, event.y, VIEWPORT.get(), PAINT_SCALE.get())));
            crate::app::set_pointer_capture(id);
            event.stop_propagation();
            event.prevent_default();
        })
        .on_mousemove(move |event: &PointerEvent| {
            if let Some((x, y, start, scale)) = RESIZE.get() {
                if event.buttons & 1 == 0 {
                    stop_resize();
                    return;
                }
                let next = Viewport {
                    width: if horizontal {
                        (start.width + 2.0 * (event.x - x) / scale).clamp(1.0, RESIZE_LIMIT.get()[0])
                    } else {
                        start.width
                    },
                    height: if vertical {
                        (start.height + (event.y - y) / scale).clamp(1.0, RESIZE_LIMIT.get()[1])
                    } else {
                        start.height
                    },
                    ..start
                };
                VIEWPORT.set(next);
                seed_dimensions(next);
                crate::app::runtime::request_redraw();
                event.stop_propagation();
            }
        })
        .on_pointer_up(|_| stop_resize())
}

fn preview_frame(left: f32, top: f32, width: f32, height: f32) -> Element {
    let line = || {
        Element::new()
            .bg(parse_color("muted-foreground"))
            .pointer_events_none()
    };
    let right = resize_grip("dt-device-resize-right", true, false)
        .left(width)
        .top(0.0)
        .w(14.0)
        .h(height)
        .child(
            line()
                .absolute()
                .left(5.0)
                .top((height - 30.0).max(0.0) * 0.5)
                .w(3.0)
                .h(30.0),
        );
    let bottom = resize_grip("dt-device-resize-bottom", false, true)
        .left(0.0)
        .top(height)
        .w(width)
        .h(14.0)
        .child(
            line()
                .absolute()
                .left((width - 30.0).max(0.0) * 0.5)
                .top(5.0)
                .w(30.0)
                .h(3.0),
        );
    let corner = resize_grip("dt-device-resize-corner", true, true)
        .left(width)
        .top(height)
        .w(14.0)
        .h(14.0)
        .children([
            line()
                .absolute()
                .left(2.0)
                .top(6.0)
                .w(12.0)
                .h(2.0)
                .rotate(-45.0),
            line()
                .absolute()
                .left(6.0)
                .top(9.0)
                .w(6.0)
                .h(2.0)
                .rotate(-45.0),
        ]);
    Element::new()
        .template_id(FRAME)
        .absolute()
        .left(left)
        .top(top)
        .w(width + 14.0)
        .h(height + 14.0)
        .z_index(25)
        .pointer_events_box_none()
        .children([right, bottom, corner])
}

fn sync_bounds(element: &mut Element, tree: &mut LayoutTree, bounds: [f32; 4]) -> bool {
    let target = Element::new().absolute().left(bounds[0]).top(bounds[1]).w(bounds[2]).h(bounds[3]);
    if element.style.inset == target.style.inset && element.style.size == target.style.size {
        return false;
    }
    use crate::layout::dimension::CssDim;
    use crate::style::UnitSlot;
    for (slot, value) in [
        (UnitSlot::InsetL, bounds[0]), (UnitSlot::InsetT, bounds[1]),
        (UnitSlot::Width, bounds[2]), (UnitSlot::Height, bounds[3]),
    ] {
        element.style.set_css_dim(slot, CssDim::Px(value));
    }
    tree.mark_element_layout_dirty(element);
    true
}

pub(super) fn sync(root: &mut Element, tree: &mut LayoutTree) -> LayoutStatus {
    let Some(host) = find_element_by_template_mut(root, "__jadix_app_host") else {
        return LayoutStatus::Clean;
    };
    let active = enabled() && DevToolsState::is_open();
    if !active {
        let mut changed = false;
        if let Some(app) = host.children.first_mut() {
            if app.attrs.remove(ACTIVE).is_some() {
                app.attrs.remove("data-jadix-viewport-width");
                app.attrs.remove(crate::core::scaled_viewport::ATTR_SCOPE);
                if let Some(host) = app.children.iter_mut().find(|child| child.template_id.as_deref() == Some(PORTAL_HOST)) {
                    host.attrs.remove(crate::core::scaled_viewport::ATTR_PORTALS);
                }
                if let Some(original) = ORIGINAL.with(|saved| saved.borrow_mut().take()) {
                    app.style.position = original.position;
                    app.style.inset = original.inset;
                    app.style.size = original.size;
                    app.style.min_size = original.min_size;
                    app.style.max_size = original.max_size;
                    app.style.scale = original.scale;
                    app.style.overflow = original.overflow;
                    app.style.transform_origin = original.transform_origin;
                }
                tree.mark_element_layout_dirty(app);
                changed = true;
            }
        }
        if host
            .children
            .iter()
            .any(|child| child.template_id.as_deref() == Some(TOOLBAR))
        {
            host.children
                .retain(|child| !matches!(child.template_id.as_deref(), Some(TOOLBAR | FRAME)));
            host.style.bg_color = [0.0; 4];
            changed = true;
        }
        if changed {
            tree.mark_element_layout_dirty(host);
        }
        return if changed { LayoutStatus::StructureChanged } else { LayoutStatus::Clean };
    }
    let first = host
        .children
        .first()
        .is_some_and(|app| !app.attrs.contains_key(ACTIVE));
    if first {
        seed_dimensions(VIEWPORT.get());
    }
    let selected = use_signal("dt-device-preset-value", || "Responsive".to_string()).peek();
    LAST_PRESET.with(|last| {
        if *last.borrow() != selected {
            if let Some((_, width, height)) = PRESETS.iter().find(|(name, _, _)| *name == selected)
            {
                VIEWPORT.set(Viewport {
                    width: *width,
                    height: *height,
                    ..VIEWPORT.get()
                });
                seed_dimensions(VIEWPORT.get());
            }
            *last.borrow_mut() = selected.clone();
        }
    });
    let zoom = use_signal("dt-device-zoom-value", || "fit".to_string())
        .peek()
        .parse::<f32>()
        .unwrap_or(0.0)
        / 100.0;
    if zoom != VIEWPORT.get().zoom {
        VIEWPORT.set(Viewport {
            zoom,
            ..VIEWPORT.get()
        });
    }
    let viewport = VIEWPORT.get();
    let available = host.layout_bounds[2].max(1.0);
    let height = host.layout_bounds[3].max(1.0);
    let fit = ((available - 32.0) / viewport.width)
        .min((height - BAR_HEIGHT - 32.0) / viewport.height)
        .clamp(0.001, 8.0);
    let scale = if viewport.zoom > 0.0 {
        viewport.zoom.min(fit)
    } else {
        fit.min(1.0)
    };
    PAINT_SCALE.set(scale);
    if RESIZE.get().is_none() {
        RESIZE_LIMIT.set([((available - 32.0) / scale).max(1.0).min(7680.0),
            ((height - BAR_HEIGHT - 32.0) / scale).max(1.0).min(4320.0)]);
    }
    let previous_frame = host.children.iter().find(|child| child.template_id.as_deref() == Some(FRAME))
        .or_else(|| host.children.first()).map(|element| crate::core::tree::visual_paint_aabb(element, element.paint_scroll_acc));
    let mut changed = false;
    if let Some(app) = host.children.first_mut() {
        if first {
            ORIGINAL.with(|saved| *saved.borrow_mut() = Some(app.style.clone()));
            app.attrs.insert(ACTIVE.into(), "1".into());
        }
        let mut style = app.style.clone();
        let previous_style = style.clone();
        crate::core::scaled_viewport::configure_viewport(app, [viewport.width, viewport.height], scale, PORTAL_HOST);
        let viewport_style = app.style.clone();
        let frame = Element::new()
            .absolute()
            .w(viewport.width)
            .h(viewport.height)
            .left(((available - viewport.width * scale) * 0.5).max(0.0))
            .top(BAR_HEIGHT + 16.0)
            .scale(scale)
            .transform_origin("left top");
        style.position = frame.style.position;
        style.inset = frame.style.inset;
        style.size = viewport_style.size;
        style.min_size = viewport_style.min_size;
        style.max_size = viewport_style.max_size;
        style.scale = viewport_style.scale;
        style.overflow = viewport_style.overflow;
        style.transform_origin = viewport_style.transform_origin;
        if previous_style != style {
            app.style = style;
            tree.mark_element_layout_dirty(app);
            changed = true;
        }
    }
    let structure_changed = !host.children.iter().any(|child| child.template_id.as_deref() == Some(TOOLBAR));
    if structure_changed {
        host.children.push(toolbar());
        host.children.push(preview_frame(
            ((available - viewport.width * scale) * 0.5).max(0.0),
            BAR_HEIGHT + 16.0,
            viewport.width * scale,
            viewport.height * scale,
        ));
        host.style.bg_color = parse_color("muted");
        tree.mark_element_layout_dirty(host);
        changed = true;
    }
    let left = ((available - viewport.width * scale) * 0.5).max(0.0);
    let top = BAR_HEIGHT + 16.0;
    let width = viewport.width * scale;
    let height = viewport.height * scale;
    if let Some(frame) = find_element_by_template_mut(host, FRAME) {
        changed |= sync_bounds(frame, tree, [left, top, width + 14.0, height + 14.0]);
        changed |= sync_bounds(&mut frame.children[0], tree, [width, 0.0, 14.0, height]);
        changed |= sync_bounds(&mut frame.children[0].children[0], tree, [5.0, (height - 30.0).max(0.0) * 0.5, 3.0, 30.0]);
        changed |= sync_bounds(&mut frame.children[1], tree, [0.0, height, width, 14.0]);
        changed |= sync_bounds(&mut frame.children[1].children[0], tree, [(width - 30.0).max(0.0) * 0.5, 5.0, 30.0, 3.0]);
        changed |= sync_bounds(&mut frame.children[2], tree, [width, height, 14.0, 14.0]);
    }
    let geometry_changed = changed;
    for (id, value) in [
        ("dt-device-rotate", text("Rotate")), ("dt-device-apply", text("Apply")),
        ("dt-device-preset:value", text(&selected)),
        ("dt-device-zoom:value", if viewport.zoom > 0.0 { format!("{:.0}%", viewport.zoom * 100.0) } else { text("Fit") }),
    ] {
        if let Some(caption) = find_element_by_template_mut(host, id) {
            if caption.text.as_deref() != Some(value.as_str()) {
                caption.text = Some(value);
                tree.mark_element_layout_dirty(caption);
                changed = true;
            }
        }
    }
    if let Some(caption) = find_element_by_template_mut(host, "dt-device-size") {
        let value = format!("{:.0} × {:.0}", viewport.width, viewport.height);
        if caption.text.as_deref() != Some(value.as_str()) {
            caption.text = Some(value);
            tree.mark_element_layout_dirty(caption);
            changed = true;
        }
    }
    if geometry_changed {
        let current = [host.layout_bounds[0] + left, host.layout_bounds[1] + top, width + 14.0, height + 14.0];
        let occupied = previous_frame.map_or(current, |previous| crate::app::damage::union_rects(previous, current));
        if let Some(damage) = crate::app::damage::intersect_rects(occupied, host.layout_bounds) {
            crate::app::damage::push_frame_region_paint(crate::app::damage::paint_element_key(host), damage);
        }
    }
    if structure_changed { LayoutStatus::StructureChanged }
    else if changed { LayoutStatus::LayoutChanged }
    else { LayoutStatus::Clean }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resizing_preview_keeps_one_frame_and_preserves_control_nodes() {
        crate::core::reactivity::create_root(|| {
            DevToolsState::enable_for_window();
            DevToolsState::set_open(true);
            ENABLED.set(true);
            let mut root = super::super::panel::wrap_app(Element::new().w("100%").h("100%"));
            root.children[0].layout_bounds = [0.0, 0.0, 500.0, 900.0];
            let mut tree = LayoutTree::new();
            sync(&mut root, &mut tree);
            let control = super::super::snapshot::find_element_by_template(
                &root, "dt-device-rotate",
            ).expect("rotate control").on_click.clone().unwrap();
            for width in [390.0, 360.0, 420.0, 300.0] {
                VIEWPORT.set(Viewport { width, ..VIEWPORT.get() });
                DevToolsState::bump();
                sync(&mut root, &mut tree);
                assert_eq!(
                    root.children[0].children.iter().filter(|node| node.template_id.as_deref() == Some(FRAME)).count(),
                    1,
                    "resizing must update the existing frame rather than append grips"
                );
                let current = super::super::snapshot::find_element_by_template(
                    &root, "dt-device-rotate",
                ).expect("rotate control");
                assert!(std::rc::Rc::ptr_eq(&control, current.on_click.as_ref().unwrap()));
            }
            disable();
            DevToolsState::set_open(false);
            VIEWPORT.set(Viewport { width: 390.0, height: 844.0, zoom: 0.0 });
        });
    }

    #[test]
    fn preview_toolbar_portals_stay_outside_the_scaled_host() {
        crate::core::reactivity::create_root(|| {
            use crate::layout::layout_tree::ATTR_PORTAL_MOUNT;
            disable();
            DevToolsState::enable_for_window();
            DevToolsState::set_open(true);
            toggle();
            let mut toolbar_popup = Element::new().template_id("toolbar-menu");
            toolbar_popup.set_attr("jadix_portal", "1");
            let mut app_popup = Element::new().template_id("app-menu");
            app_popup.set_attr("jadix_portal", "1");
            let mut root = Element::new().template_id("__jadix_app_host")
                .child(Element::new().child(app_popup))
                .child(Element::new().template_id(TOOLBAR).child(toolbar_popup));
            assert!(scope_portals(&mut root));
            assert_eq!(root.children[0].children[0].attrs.get(ATTR_PORTAL_MOUNT)
                .map(String::as_str), Some(PORTAL_HOST));
            assert_eq!(root.children[1].children[0].attrs.get(ATTR_PORTAL_MOUNT)
                .map(String::as_str), Some("__jadix_app_host"));
            assert!(!scope_portals(&mut root));
            disable();
            DevToolsState::set_open(false);
        });
    }

    #[test]
    fn preview_moves_open_portals_locally_and_restores_them_on_close() {
        crate::core::reactivity::create_root(|| {
            use super::super::snapshot::find_element_by_template;
            use crate::layout::layout_tree::{hoist_portal_subtrees, ATTR_HOISTED_MOUNT};
            disable();
            DevToolsState::enable_for_window();
            DevToolsState::set_open(true);
            let mut portal = Element::new()
                .template_id("preview-test-popup")
                .w(100.0)
                .h(80.0);
            portal.set_attr("jadix_portal", "1");
            portal.set_attr("jadix_portal_key", "preview-test-popup");
            let app = Element::new().w("100%").h("100%").child(portal);
            let mut root = super::super::panel::wrap_app(app);
            let mut tree = LayoutTree::new();
            super::super::panel::sync_overlay_layout(&mut root, &mut tree);
            tree.build_and_layout(&mut root, 1200.0, 800.0, [-1.0; 2]);
            assert_eq!(find_element_by_template(&root, "preview-test-popup").unwrap()
                .attrs.get(ATTR_HOISTED_MOUNT).map(String::as_str), Some("__jadix_app_host"));
            toggle();
            super::super::panel::sync_overlay_layout(&mut root, &mut tree);
            assert!(hoist_portal_subtrees(&mut root) > 0);
            tree.build_and_layout(&mut root, 1200.0, 800.0, [-1.0; 2]);
            let mount = find_element_by_template(&root, PORTAL_HOST).unwrap();
            assert_eq!(mount.layout_bounds[2..], [390.0, 844.0]);
            let popup = find_element_by_template(mount, "preview-test-popup").unwrap();
            assert_eq!(
                popup.attrs.get(ATTR_HOISTED_MOUNT).map(String::as_str),
                Some(PORTAL_HOST)
            );
            assert!(!crate::layout::layout_tree::escapes_ancestor_overflow(
                popup
            ));
            assert!(find_element_by_template(&root, FRAME).is_some());
            disable();
            super::super::panel::sync_overlay_layout(&mut root, &mut tree);
            assert!(hoist_portal_subtrees(&mut root) > 0);
            let popup = find_element_by_template(&root, "preview-test-popup").unwrap();
            assert_eq!(popup.attrs.get(ATTR_HOISTED_MOUNT).map(String::as_str), Some("__jadix_app_host"));
            assert!(find_element_by_template(&root, FRAME).is_none());
            DevToolsState::set_open(false);
            assert!(hoist_portal_subtrees(&mut root) > 0);
            let popup = find_element_by_template(&root, "preview-test-popup").unwrap();
            assert!(popup.style.position_fixed);
            assert!(!popup.attrs.contains_key(ATTR_HOISTED_MOUNT));
        });
    }

    #[test]
    fn preview_resize_capture_updates_dimensions_in_logical_pixels() {
        crate::core::reactivity::create_root(|| {
            use crate::app::pointer_events::{
                dispatch_pointer, hit_path, PointerEvent, PointerKind,
            };
            use winit::event::MouseButton;
            VIEWPORT.set(Viewport {
                width: 390.0,
                height: 844.0,
                zoom: 0.5,
            });
            PAINT_SCALE.set(0.5);
            RESIZE_LIMIT.set([500.0, 910.0]);
            let mut grip = resize_grip("dt-device-resize-test", true, true)
                .w(14.0)
                .h(14.0);
            grip.layout_bounds = [400.0, 500.0, 14.0, 14.0];
            let mut root = Element::new().child(grip);
            root.layout_bounds = [0.0, 0.0, 1200.0, 900.0];
            let mut event = PointerEvent::new(405.0, 505.0, MouseButton::Left, 1);
            event.buttons = 1;
            dispatch_pointer(
                &hit_path(&root, event.x, event.y),
                &mut event,
                PointerKind::Down,
            );
            let path = crate::app::pointer_events::captured_path(&root).unwrap();
            let mut event = PointerEvent::new(430.0, 535.0, MouseButton::Left, 0);
            event.buttons = 1;
            dispatch_pointer(&path, &mut event, PointerKind::Move);
            assert_eq!(VIEWPORT.get().width, 490.0);
            assert_eq!(VIEWPORT.get().height, 904.0);
            let mut event = PointerEvent::new(5000.0, 5000.0, MouseButton::Left, 0);
            event.buttons = 1;
            dispatch_pointer(&path, &mut event, PointerKind::Move);
            assert_eq!(VIEWPORT.get().width, 500.0);
            assert_eq!(VIEWPORT.get().height, 910.0);
            dispatch_pointer(&path, &mut event, PointerKind::Up);
            RESIZE_LIMIT.set([7680.0, 4320.0]);
            assert!(RESIZE.get().is_none());
            assert!(crate::app::pointer_capture_id().is_none());
            VIEWPORT.set(Viewport {
                width: 390.0,
                height: 844.0,
                zoom: 0.0,
            });
        });
    }

    #[test]
    fn responsive_width_override_is_scoped_and_reversible() {
        let mut content = Element::new().child(Element::new().class("w-10 md:w-32"));
        content
            .attrs
            .insert("data-jadix-viewport-width".into(), "390".into());
        let mut root = Element::new().children(vec![content, Element::new().class("w-10 md:w-32")]);
        let mut tree = LayoutTree::new();
        let mut impact = crate::core::tree::LayoutStatus::Clean;
        crate::style::sync_class_variants_subtree(
            &mut root,
            [-1.0; 2],
            "",
            false,
            1200.0,
            800.0,
            &mut tree,
            &mut impact,
        );
        assert_eq!(
            root.children[0].children[0].style.size.width,
            taffy::style::Dimension::length(40.0)
        );
        assert_eq!(
            root.children[1].style.size.width,
            taffy::style::Dimension::length(128.0)
        );
        root.children[0].attrs.remove("data-jadix-viewport-width");
        crate::style::sync_class_variants_subtree(
            &mut root,
            [-1.0; 2],
            "",
            false,
            1200.0,
            800.0,
            &mut tree,
            &mut impact,
        );
        assert_eq!(
            root.children[0].children[0].style.size.width,
            taffy::style::Dimension::length(128.0)
        );
    }

    #[test]
    fn device_preview_overrides_the_docked_content_viewport() {
        crate::core::reactivity::create_root(|| {
            DevToolsState::enable_for_window();
            DevToolsState::set_open(true);
            DevToolsState::set_panel_width(640.0);
            crate::utils::modal_motion::set_modal_viewport_size(1500.0, 900.0);
            let app = Element::new()
                .w("100%")
                .h("100%")
                .child(Element::new().class("w-24 md:w-48"));
            let mut root = super::super::panel::wrap_app(app);
            let mut tree = LayoutTree::new();
            for (preview, expected_viewport, expected_width) in [
                (false, (860.0, 900.0), 192.0),
                (true, (390.0, 844.0), 96.0),
                (false, (860.0, 900.0), 192.0),
            ] {
                ENABLED.set(preview);
                VIEWPORT.set(Viewport {
                    width: 390.0,
                    height: 844.0,
                    zoom: 0.0,
                });
                super::super::panel::sync_overlay_layout(&mut root, &mut tree);
                let mut impact = LayoutStatus::Clean;
                crate::style::sync_class_variants_subtree(
                    &mut root,
                    [-1.0; 2],
                    "",
                    false,
                    1500.0,
                    900.0,
                    &mut tree,
                    &mut impact,
                );
                assert_eq!(
                    crate::utils::modal_motion::modal_viewport_size(),
                    expected_viewport
                );
                assert_eq!(
                    root.children[0].children[0].children[0].style.size.width,
                    taffy::style::Dimension::length(expected_width)
                );
                tree.build_and_layout(&mut root, 1500.0, 900.0, [-1.0; 2]);
            }
            DevToolsState::set_open(false);
        });
    }

    #[test]
    fn preview_uses_real_device_layout_and_close_restores_style_and_width() {
        crate::core::reactivity::create_root(|| {
            DevToolsState::enable_for_window();
            DevToolsState::set_open(true);
            DevToolsState::set_panel_width(400.0);
            let app = Element::new().w("100%").h("100%").bg([0.2, 0.3, 0.4, 1.0]);
            let original = app.style.clone();
            let mut root = super::super::panel::wrap_app(app);
            let mut tree = LayoutTree::new();
            super::super::panel::sync_overlay_layout(&mut root, &mut tree);
            tree.build_and_layout(&mut root, 1200.0, 800.0, [-1.0; 2]);
            toggle();
            assert_ne!(super::super::panel::sync_overlay_layout(
                &mut root, &mut tree
            ), LayoutStatus::Clean);
            tree.build_and_layout(&mut root, 1200.0, 800.0, [-1.0; 2]);
            let app = &root.children[0].children[0];
            assert!((app.layout_bounds[2] - 390.0).abs() < 1.0);
            assert!((app.layout_bounds[3] - 844.0).abs() < 1.0);
            assert!(app.style.scale[0] < 1.0);
            DevToolsState::set_open(false);
            assert_ne!(super::super::panel::sync_overlay_layout(
                &mut root, &mut tree
            ), LayoutStatus::Clean);
            tree.build_and_layout_fast(&mut root, 1200.0, 800.0, [-1.0; 2], false, false);
            assert_eq!(root.children[0].children[0].style, original);
            assert!((root.children[0].children[0].layout_bounds[2] - 1200.0).abs() < 1.0);
            assert!(!root.children[0]
                .children
                .iter()
                .any(|child| child.template_id.as_deref() == Some(TOOLBAR)));
        });
    }
}
