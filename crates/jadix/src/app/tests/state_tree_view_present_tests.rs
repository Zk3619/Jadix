#![cfg(feature = "product-tests")]
//! Exercise nested TreeView presence, animations and retained native presentation together.
use super::*;
use crate::core::reactivity::{create_effect, create_root, use_signal};
use crate::renderer::offscreen::RgbaImage;
use std::{cell::RefCell, collections::HashSet, rc::Rc};
use taffy::style::Display;

#[test]
#[ignore = "requires a native Windows GPU surface; run explicitly with --ignored --nocapture"]
fn tree_view_native_expand_collapse_frames_match_full_composite() {
    use winit::application::ApplicationHandler;
    use winit::event::WindowEvent;
    use winit::event_loop::{ActiveEventLoop, EventLoop};
    use winit::platform::windows::EventLoopBuilderExtWindows;
    use winit::window::{WindowAttributes, WindowId};
    struct Harness;
    impl ApplicationHandler for Harness {
        fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
            let window: Arc<dyn Window> = Arc::from(
                event_loop
                    .create_window(
                        WindowAttributes::default()
                            .with_title("Jadix TreeView frame regression")
                            .with_surface_size(winit::dpi::LogicalSize::new(720.0, 560.0))
                            .with_visible(false),
                    )
                    .unwrap(),
            );
            run_frames(window);
            event_loop.exit();
        }
        fn window_event(&mut self, _: &dyn ActiveEventLoop, _: WindowId, _: WindowEvent) {}
    }
    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    builder.build().unwrap().run_app(Harness).unwrap();
}

fn row<'a>(root: &'a Element, id: &str, label: &str) -> &'a Element {
    let tree =
        crate::core::tree::find_in_tree(root, |el| el.template_id.as_deref() == Some(id)).unwrap();
    crate::core::tree::find_in_tree(tree, |el| {
        el.on_click.is_some()
            && el.attrs.get("data-slot").map(String::as_str) == Some("collapsible-trigger")
            && crate::core::tree::find_in_tree(el, |leaf| leaf.text.as_deref() == Some(label))
                .is_some()
    })
    .expect("mounted branch trigger")
}

fn assert_visible_text(root: &Element, id: &str, text: &str, context: &str) {
    fn path<'a>(el: &'a Element, text: &str, out: &mut Vec<&'a Element>) -> bool {
        out.push(el);
        if el.text.as_deref() == Some(text) {
            return true;
        }
        for child in &el.children {
            if path(child, text, out) {
                return true;
            }
        }
        out.pop();
        false
    }
    let tree =
        crate::core::tree::find_in_tree(root, |el| el.template_id.as_deref() == Some(id)).unwrap();
    let mut ancestors = Vec::new();
    assert!(
        path(tree, text, &mut ancestors),
        "{context}: missing text node {text:?}"
    );
    let leaf = *ancestors.last().unwrap();
    let b = leaf.layout_bounds;
    assert!(
        b[2] > 0.0 && b[3] > 0.0,
        "{context}: zero text bounds {b:?}"
    );
    for el in ancestors {
        assert_ne!(
            el.style.display,
            Display::None,
            "{context}: hidden ancestor {:?}",
            el.template_id
        );
        assert!(
            el.style.opacity > 0.0,
            "{context}: transparent ancestor {:?}",
            el.template_id
        );
        if el.attrs.get("data-slot").map(String::as_str) == Some("collapsible-content") {
            let clip = el.layout_bounds;
            assert!(
                b[1] >= clip[1] - 1.0 && b[1] + b[3] <= clip[1] + clip[3] + 1.0,
                "{context}: text {b:?} clipped by {:?} {:?}, max-height {:?}, runs {:?}",
                el.template_id,
                clip,
                el.style.max_size.height,
                el.animation_snapshot.runs
            );
        }
    }
}

fn read_frame(s: &AppState) -> RgbaImage {
    let texture = s.present_retain.color_texture_for_test().unwrap();
    let mut img = readback_texture(
        &s.device,
        &s.queue,
        texture,
        s.size.width,
        s.size.height,
        s.device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default()),
    )
    .unwrap();
    if matches!(
        s.config.format,
        wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
    ) {
        for px in img.pixels.chunks_exact_mut(4) {
            px.swap(0, 2);
        }
    }
    img
}

fn frame(state: &Rc<RefCell<Option<AppState>>>, mouse: [f32; 2], action: &str, tick: usize) {
    fn geometry(root: &Element) -> Vec<(Option<String>, [f32; 4])> {
        let mut result = Vec::new();
        crate::core::tree::walk_elements(root, |element| {
            if element.get_attr("data-slot").map(String::as_str) == Some("collapsible-content") {
                result.push((element.template_id.clone(), element.layout_bounds));
            }
        });
        result
    }
    crate::style::advance_css_transition_timeline_ms(16.0);
    crate::utils::collapse_panel::tick_collapse_panel_presence();
    crate::app::reactive::drain_deferred_view_remounts();
    let mut opt = state.borrow_mut();
    let s = opt.as_mut().unwrap();
    s.render(
        mouse,
        ScrollbarUpdateMode::ViewportsOnly,
        None,
        false,
        false,
        false,
    )
    .unwrap();
    let partial = read_frame(s);
    let before_geometry = geometry(&s.root_element);
    if tick == 21 {
        let id = if action.starts_with("native-tree-builder") {
            "native-tree-builder"
        } else {
            "native-tree-rsx"
        };
        let key = if id.ends_with("builder") {
            "native-tree-builder-exp"
        } else {
            "native-tree-rsx-exp"
        };
        let expanded = use_signal(key, HashSet::<String>::new).peek();
        if expanded.contains("root") {
            if expanded.contains("docs") {
                assert_visible_text(&s.root_element, id, "README.md", action);
            }
            if expanded.contains("src") {
                assert_visible_text(&s.root_element, id, "main.rs", action);
                assert_visible_text(&s.root_element, id, "lib.rs", action);
            }
        }
    }
    // Exercise consecutive retained frames without a full redraw repairing each one.
    if tick != 21 && std::env::var_os("JADIX_TREE_RETAINED_SEQUENCE").is_some() {
        return;
    }
    crate::style::utility::set_frame_dt(0.0);
    s.mark_full_redraw();
    s.render(
        mouse,
        ScrollbarUpdateMode::ViewportsOnly,
        None,
        false,
        false,
        false,
    )
    .unwrap();
    let full = read_frame(s);
    let after_geometry = geometry(&s.root_element);
    assert_eq!(before_geometry.len(), after_geometry.len());
    for ((before_id, before), (after_id, after)) in before_geometry.iter().zip(&after_geometry) {
        assert_eq!(before_id, after_id);
        assert!(
            before.iter().zip(after).all(|(before, after)| (before - after).abs() < 0.001),
            "{action} tick {tick}: a repaint at the same time must preserve collapse geometry: {before:?} => {after:?}"
        );
    }
    crate::style::utility::set_frame_dt(0.016);
    let changed = partial
        .pixels
        .chunks_exact(4)
        .zip(full.pixels.chunks_exact(4))
        .filter(|(a, b)| a.iter().zip(b.iter()).any(|(a, b)| a.abs_diff(*b) > 6))
        .count();
    if changed > 0 {
        let dir = std::path::Path::new("target/treeview-debug");
        std::fs::create_dir_all(dir).unwrap();
        for (kind, img) in [("partial", &partial), ("full", &full)] {
            image::save_buffer(
                dir.join(format!("{action}-{tick}-{kind}.png")),
                &img.pixels,
                img.width,
                img.height,
                image::ColorType::Rgba8,
            )
            .unwrap();
        }
    }
    assert_eq!(
        changed, 0,
        "{action} tick {tick}: retained frame must equal full composite"
    );
}
fn run_frames(window: Arc<dyn Window>) {
    use crate::app::runtime::{clear_app_runtime, set_app_runtime, AppRuntime};
    use crate::components::*;
    crate::app::reactive::reset_registered_anchors();
    crate::style::set_css_transition_virtual_timeline(true);
    crate::style::set_css_transition_timeline_ms(0.0);
    crate::style::utility::set_frame_dt(0.016);
    let state = Rc::new(RefCell::new(Some(crate::utils::block_on::block_on(
        AppState::new_bootstrap(
            window,
            super::super::super::window_chrome::WindowOptions::default(),
        ),
    ))));
    let secondaries = Rc::new(RefCell::new(std::collections::HashMap::new()));
    set_app_runtime(AppRuntime::new(
        Rc::downgrade(&state),
        Rc::downgrade(&secondaries),
    ));
    let (owner, ()) = create_root(|| {
        let builder_exp = use_signal("native-tree-builder-exp", || {
            HashSet::from(["root".into(), "src".into()])
        });
        let rsx_exp = use_signal("native-tree-rsx-exp", || {
            HashSet::<String>::from(["root".into(), "src".into()])
        });
        let live = Rc::clone(&state);
        crate::core::reactivity::mark_next_effect_as_ui_root();
        create_effect(move || {
            let roots = [tree_node("root", "Workspace").children([
                tree_node("src", "src")
                    .children([tree_node("main", "main.rs"), tree_node("lib", "lib.rs")]),
                tree_node("docs", "Docs").children([tree_node("readme", "README.md")]),
            ])];
            let builder: Element = tree_view("native-tree-builder")
                .variant("collapsible")
                .exclusive(true)
                .height(160.0)
                .expanded_signal(builder_exp)
                .expanded_storage_key("native-tree-builder-exp")
                .roots(roots)
                .into();
            let rsx = crate::ui! {
                <TreeView id="native-tree-rsx" variant="collapsible" height={160.0}
                    expanded_signal={rsx_exp} expanded_storage_key="native-tree-rsx-exp">
                    <TreeViewItem id="root" label="Workspace">
                        <TreeViewItem id="src" label="src">
                            <TreeViewItem id="main" label="main.rs" />
                            <TreeViewItem id="lib" label="lib.rs" />
                        </TreeViewItem>
                        <TreeViewItem id="docs" label="Docs">
                            <TreeViewItem id="readme" label="README.md" />
                        </TreeViewItem>
                    </TreeViewItem>
                </TreeView>
            };
            let root: Element = crate::primitives::r#box::vstack()
                .w("100%")
                .h("100%")
                .p(12.0)
                .gap(12.0)
                .bg("background")
                .children([
                    Element::new().text("Builder · exclusive"),
                    builder,
                    Element::new().text("RSX · independent"),
                    rsx,
                    Element::new().text("Stable footer · 收起后不能残留节点"),
                    crate::ui! {
                        <Box id="native-hover-padding" w="128px" h="56px"
                            class="p-1 transition-padding duration-300 hover:p-4">
                            <Label>"Padding motion"</Label>
                        </Box>
                    },
                ])
                .into();
            live.borrow_mut().as_mut().unwrap().rebuild_ui(root);
        });
    });
    for tick in 0..6 {
        frame(&state, [-1.0, -1.0], "initial", tick);
    }
    let padding_bounds = {
        let state_ref = state.borrow();
        crate::core::tree::find_in_tree(&state_ref.as_ref().unwrap().root_element, |element| {
            element.template_id.as_deref() == Some("native-hover-padding")
        })
        .unwrap()
        .layout_bounds
    };
    for hovered in [true, false, true, false, true, false] {
        let mouse = if hovered {
            [padding_bounds[0] + 64.0, padding_bounds[1] + 28.0]
        } else {
            [-1.0; 2]
        };
        for _ in 0..23 {
            crate::style::advance_css_transition_timeline_ms(16.0);
            let mut state_ref = state.borrow_mut();
            let app = state_ref.as_mut().unwrap();
            app.render(
                mouse,
                ScrollbarUpdateMode::ViewportsOnly,
                None,
                false,
                false,
                true,
            )
            .unwrap();
            let target = crate::core::tree::find_in_tree(&app.root_element, |element| {
                element.template_id.as_deref() == Some("native-hover-padding")
            })
            .unwrap();
            let padding = crate::layout::lp_length_px(target.style.padding.left).unwrap();
            let offset = target.children[0].layout_bounds[0] - target.layout_bounds[0];
            assert!(
                (offset - padding).abs() < 0.02,
                "animated padding must update text geometry on fast frames: {padding} != {offset}"
            );
        }
        let mut state_ref = state.borrow_mut();
        let app = state_ref.as_mut().unwrap();
        let target = crate::core::tree::find_in_tree(&app.root_element, |element| {
            element.template_id.as_deref() == Some("native-hover-padding")
        })
        .unwrap();
        let padding = crate::layout::lp_length_px(target.style.padding.left).unwrap();
        assert!((padding - if hovered { 16.0 } else { 4.0 }).abs() < 0.02);
        let partial = read_frame(app);
        app.mark_full_redraw();
        crate::style::utility::set_frame_dt(0.0);
        app.render(
            mouse,
            ScrollbarUpdateMode::ViewportsOnly,
            None,
            false,
            false,
            true,
        )
        .unwrap();
        let full = read_frame(app);
        crate::style::utility::set_frame_dt(0.016);
        let changed = partial
            .pixels
            .iter()
            .zip(&full.pixels)
            .filter(|(before, after)| before.abs_diff(**after) > 6)
            .count();
        assert_eq!(
            changed, 0,
            "padding motion must repaint moved text without stale pixels"
        );
    }
    for id in ["native-tree-builder", "native-tree-rsx"] {
        for (action, label, ticks) in [
            ("docs-open", "Docs", 22),
            ("src-toggle", "src", 22),
            ("root-close", "Workspace", 22),
            ("root-reopen", "Workspace", 22),
            ("rapid-close", "Workspace", 3),
            ("rapid-reopen", "Workspace", 22),
            ("close-again", "Workspace", 22),
            ("reopen-again", "Workspace", 22),
            ("docs-after-remount", "Docs", 22),
            ("src-after-remount", "src", 22),
        ] {
            let exp_key = if id.ends_with("builder") {
                "native-tree-builder-exp"
            } else {
                "native-tree-rsx-exp"
            };
            let node = match label {
                "Docs" => "docs",
                "src" => "src",
                _ => "root",
            };
            let was_open = use_signal(exp_key, HashSet::<String>::new)
                .peek()
                .contains(node);
            let (mouse, click) = {
                let opt = state.borrow();
                let trigger = row(&opt.as_ref().unwrap().root_element, id, label);
                let b = trigger.layout_bounds;
                (
                    [b[0] + 60.0, b[1] + b[3] * 0.5],
                    trigger.on_click.clone().unwrap(),
                )
            };
            click.borrow_mut()();
            let open = use_signal(exp_key, HashSet::<String>::new)
                .peek()
                .contains(node);
            assert_eq!(open, !was_open, "click must toggle {node} after {action}");
            assert_eq!(
                open,
                use_signal(&format!("{id}::open::{node}"), || false).peek(),
                "trigger state and expanded set must agree after {action}"
            );
            for tick in 0..ticks {
                frame(&state, mouse, &format!("{id}-{action}"), tick);
                if action == "docs-open" && tick + 1 == ticks {
                    let opt = state.borrow();
                    assert_visible_text(
                        &opt.as_ref().unwrap().root_element,
                        id,
                        "README.md",
                        &format!("{id}-{action}"),
                    );
                }
                if action == "rapid-reopen" && tick == 0 {
                    let opt = state.borrow();
                    let panel_id = format!("{id}::root:content");
                    let panel = crate::core::tree::find_in_tree(
                        &opt.as_ref().unwrap().root_element,
                        |el| el.template_id.as_deref() == Some(panel_id.as_str()),
                    )
                    .unwrap();
                    assert!(
                        panel.layout_bounds[3] > 1.0,
                        "reversing a partially closed panel must continue from its current height"
                    );
                }
            }
        }
        // Reopen in the gap between a completed closing present and unmount drain.
        let presence = crate::utils::use_collapse_panel_presence(&format!("{id}::root:presence"));
        let click = {
            let opt = state.borrow();
            row(&opt.as_ref().unwrap().root_element, id, "Workspace")
                .on_click
                .clone()
                .unwrap()
        };
        click.borrow_mut()();
        let mut finished = false;
        for tick in 0..30 {
            frame(&state, [-1.0, -1.0], &format!("{id}-exit-boundary"), tick);
            if presence.phase() == crate::utils::presence::PresencePhase::Absent {
                assert!(
                    presence.mounted().peek(),
                    "unmount must be deferred until the next frame"
                );
                finished = true;
                break;
            }
        }
        assert!(finished, "closing animation must finish");
        click.borrow_mut()();
        for tick in 0..22 {
            frame(
                &state,
                [-1.0, -1.0],
                &format!("{id}-reopen-at-exit-boundary"),
                tick,
            );
        }
        assert!(presence.mounted().peek(), "reopened root must stay mounted");
    }
    clear_app_runtime();
    owner.dispose();
    crate::app::reactive::reset_registered_anchors();
    crate::style::utility::set_frame_dt(0.0);
    crate::style::set_css_transition_virtual_timeline(false);
}
