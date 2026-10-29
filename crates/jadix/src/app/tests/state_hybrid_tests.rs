//! Native frame oracle for the retained/immediate invalidation contract.
use super::*;
use crate::app::{invalidation, reactive, runtime, window_scope};
use crate::core::reactivity::{create_effect, create_root, current_effect_id};
use crate::renderer::offscreen::{images_match, readback_texture, RgbaImage};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

#[test]
#[ignore = "requires a native Windows GPU surface; run explicitly with --ignored --nocapture"]
fn hybrid_invalidation_native_frames_preserve_retained_work_and_pixels() {
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
                            .with_title("Jadix hybrid invalidation regression")
                            .with_surface_size(winit::dpi::LogicalSize::new(360.0, 240.0))
                            .with_visible(false),
                    )
                    .expect("hidden test window"),
            );
            let secondary = Arc::from(
                event_loop
                    .create_window(
                        WindowAttributes::default()
                            .with_title("Jadix secondary hybrid regression")
                            .with_surface_size(winit::dpi::LogicalSize::new(360.0, 240.0))
                            .with_visible(false),
                    )
                    .expect("hidden secondary window"),
            );
            run_hybrid_frames(window, secondary);
            event_loop.exit();
        }

        fn window_event(&mut self, _: &dyn ActiveEventLoop, _: WindowId, _: WindowEvent) {}
    }

    let mut builder = EventLoop::builder();
    builder.with_any_thread(true);
    builder.build().unwrap().run_app(Harness).unwrap();
}

type State = Rc<RefCell<Option<AppState>>>;

fn render(state: &State) {
    state
        .borrow_mut()
        .as_mut()
        .unwrap()
        .render(
            [-1.0; 2],
            ScrollbarUpdateMode::ViewportsOnly,
            None,
            false,
            false,
            false,
        )
        .expect("native hybrid frame");
}

fn frame(state: &State) {
    invalidation::begin_frame();
    reactive::drain_deferred_view_remounts();
    render(state);
}

fn read_frame(state: &State) -> RgbaImage {
    let state = state.borrow();
    let state = state.as_ref().unwrap();
    assert!(state.present_retain.contents_valid());
    let texture = state.present_retain.color_texture_for_test().unwrap();
    readback_texture(
        &state.device,
        &state.queue,
        texture,
        state.config.width,
        state.config.height,
        state.device.create_command_encoder(&Default::default()),
    )
    .expect("native retained readback")
}

fn assert_matches_full(state: &State, stage: &str) {
    let retained = read_frame(state);
    state.borrow_mut().as_mut().unwrap().mark_full_redraw();
    // Repaint the same logical frame without ticking declarations again.
    render(state);
    assert!(
        images_match(&retained, &read_frame(state), 6),
        "{stage}: retained/full mismatch"
    );
}

fn text_buffers(state: &State) -> Vec<(u64, usize)> {
    state
        .borrow()
        .as_ref()
        .unwrap()
        .text_renderer
        .shaped_cache
        .iter()
        .map(|entry| (entry.shape_hash, entry.buffer.lines.as_ptr() as usize))
        .collect()
}

fn run_hybrid_frames(window: Arc<dyn Window>, secondary_window: Arc<dyn Window>) {
    let _scope = window_scope::enter_primary();
    runtime::clear_app_runtime();
    reactive::reset_registered_anchors();
    crate::style::set_css_transition_virtual_timeline(true);
    crate::style::set_css_transition_timeline_ms(0.0);
    crate::style::utility::set_frame_dt(0.0);
    crate::app::damage::clear_frame_paint_nodes();
    runtime::register_window(&window, true);
    let state = Rc::new(RefCell::new(Some(crate::utils::block_on::block_on(
        AppState::new_bootstrap(window, crate::app::window_chrome::WindowOptions::default()),
    ))));
    let secondaries = Rc::new(RefCell::new(HashMap::new()));
    runtime::set_app_runtime(runtime::AppRuntime::new(
        Rc::downgrade(&state),
        Rc::downgrade(&secondaries),
    ));

    let root_runs = Rc::new(Cell::new(0));
    let region_runs = Rc::new(Cell::new(0));
    let items = Rc::new(Cell::new(2));
    let clock_active = Rc::new(Cell::new(false));
    let paint_color = Rc::new(Cell::new([0.1, 0.3, 0.8, 1.0]));
    let paint_width = Rc::new(Cell::new(80.0));
    let (owner, ()) = create_root(|| {
        let state = Rc::clone(&state);
        let roots = Rc::clone(&root_runs);
        let regions = Rc::clone(&region_runs);
        let items = Rc::clone(&items);
        let clock = Rc::clone(&clock_active);
        let color = Rc::clone(&paint_color);
        let width = Rc::clone(&paint_width);
        crate::core::reactivity::mark_next_effect_as_ui_root();
        create_effect(move || {
            roots.set(roots.get() + 1);
            invalidation::register_root(current_effect_id().unwrap());
            let regions = Rc::clone(&regions);
            let items = Rc::clone(&items);
            let clock = Rc::clone(&clock);
            let dynamic = reactive::immediate_region("hybrid-immediate", move || {
                regions.set(regions.get() + 1);
                if clock.get() {
                    invalidation::request_frame();
                }
                Element::new()
                    .absolute()
                    .left(8.0)
                    .top(100.0)
                    .w(300.0)
                    .h(80.0)
                    .children((0..items.get()).map(|i| {
                        Element::new()
                            .template_id(format!("hybrid-item-{i}"))
                            .absolute()
                            .left(i as f32 * 28.0)
                            .top(0.0)
                            .w(20.0)
                            .h(20.0)
                            .bg([0.9, 0.2, 0.1, 1.0])
                    }))
            });
            state.borrow_mut().as_mut().unwrap().rebuild_ui(
                Element::new()
                    .w("100%")
                    .h("100%")
                    .bg([1.0; 4])
                    .child(
                        Element::new()
                            .template_id("hybrid-label")
                            .absolute()
                            .left(8.0)
                            .top(8.0)
                            .w(300.0)
                            .h(28.0)
                            .text_size(16.0)
                            .text_shadow("2px 2px 2px #000000")
                            .text("Retained text stays warm"),
                    )
                    .child(
                        Element::new()
                            .template_id("hybrid-paint")
                            .absolute()
                            .left(8.0)
                            .top(48.0)
                            .w(width.get())
                            .h(36.0)
                            .bg(color.get()),
                    )
                    .child(dynamic)
                    .child(
                        Element::new()
                            .absolute()
                            .left(8.0)
                            .top(190.0)
                            .w(300.0)
                            .h(28.0)
                            .rotate(3.0)
                            .text("Transformed text cache"),
                    ),
            );
        });
    });
    for _ in 0..3 {
        frame(&state);
    }
    assert_matches_full(&state, "initial");
    let warm_text = text_buffers(&state);
    assert!(!warm_text.is_empty(), "oracle must exercise shaped text");
    {
        let state = state.borrow();
        let state = state.as_ref().unwrap();
        assert!(
            !state.shadow_tex_cache.is_empty(),
            "exercise text shadow bitmaps"
        );
        assert!(!state.shadow_rect_cache.is_empty());
        assert!(
            state
                .image_textures
                .keys()
                .any(|key| { crate::utils::text_texture::is_text_xform_texture_key(key) }),
            "exercise GPU Bake/MSDF text textures"
        );
    }
    let root_count = root_runs.get();
    let region_count = region_runs.get();
    let layout_rebuilds = state.borrow().as_ref().unwrap().layout_tree.full_rebuilds;
    let invalidations = invalidation::invalidation_stats();

    // L0 must actually present even when the retained scene has identical bytes.
    invalidation::request_frame();
    assert!(invalidation::pending(window_scope::WindowScope::Primary));
    frame(&state);
    assert!(state.borrow().as_ref().unwrap().gpu_presented_this_frame);
    assert_eq!(root_runs.get(), root_count);
    assert_eq!(region_runs.get(), region_count + 1);
    assert_eq!(text_buffers(&state), warm_text);
    assert!(!invalidation::pending(window_scope::WindowScope::Primary));

    // An unsolicited unchanged redraw can tick the immediate island once, but
    // does not present or start a permanent frame pump.
    frame(&state);
    assert!(!state.borrow().as_ref().unwrap().gpu_presented_this_frame);
    assert_eq!(crate::last_scene_uploads().write_calls, 0);
    assert!(!invalidation::pending(window_scope::WindowScope::Primary));

    // L1 changes pixels while retaining layout ids and shaped buffers.
    paint_color.set([0.1, 0.8, 0.2, 1.0]);
    let before_paint = read_frame(&state);
    let (paint_id, dirty_calls) = {
        let mut state = state.borrow_mut();
        let state = state.as_mut().unwrap();
        let dirty_calls = state.layout_tree.mark_dirty_calls;
        let el = crate::core::tree::find_in_tree_mut(&mut state.root_element, |el| {
            el.template_id.as_deref() == Some("hybrid-paint")
        })
        .unwrap();
        el.style.bg_color = paint_color.get();
        (el.layout_node, dirty_calls)
    };
    invalidation::node("hybrid-paint").mark_paint();
    frame(&state);
    assert!(!images_match(&before_paint, &read_frame(&state), 6));
    assert_eq!(
        state.borrow().as_ref().unwrap().layout_tree.full_rebuilds,
        layout_rebuilds
    );
    assert_eq!(
        state
            .borrow()
            .as_ref()
            .unwrap()
            .layout_tree
            .mark_dirty_calls,
        dirty_calls
    );
    assert_eq!(text_buffers(&state), warm_text);
    assert_matches_full(&state, "L1 paint");

    // Paint transforms repair both the old and new coverage without layout.
    {
        let mut state = state.borrow_mut();
        let el =
            crate::core::tree::find_in_tree_mut(&mut state.as_mut().unwrap().root_element, |el| {
                el.template_id.as_deref() == Some("hybrid-paint")
            })
            .unwrap();
        el.style.rotate_deg = 17.0;
    }
    invalidation::node("hybrid-paint").mark_paint();
    frame(&state);
    assert_eq!(
        state
            .borrow()
            .as_ref()
            .unwrap()
            .layout_tree
            .mark_dirty_calls,
        dirty_calls
    );
    assert_matches_full(&state, "L1 transform coverage");

    // L2 must recompute the marked subtree's dimensions without declarations.
    paint_width.set(112.0);
    {
        let mut state = state.borrow_mut();
        let el =
            crate::core::tree::find_in_tree_mut(&mut state.as_mut().unwrap().root_element, |el| {
                el.template_id.as_deref() == Some("hybrid-paint")
            })
            .unwrap();
        el.style = Element::new()
            .absolute()
            .left(8.0)
            .top(48.0)
            .w(paint_width.get())
            .h(36.0)
            .bg(paint_color.get())
            .style
            .clone();
    }
    invalidation::node("hybrid-paint").mark_layout();
    frame(&state);
    {
        let state = state.borrow();
        let el = crate::core::tree::find_in_tree(&state.as_ref().unwrap().root_element, |el| {
            el.template_id.as_deref() == Some("hybrid-paint")
        })
        .unwrap();
        assert_eq!(el.layout_node, paint_id);
        assert!((el.layout_bounds[2] - paint_width.get()).abs() < 0.1);
    }
    assert_eq!(root_runs.get(), root_count);
    assert_matches_full(&state, "L2 layout");

    // Structural immediate changes restate only that island. Its own frame
    // request keeps the clock alive until the producer stops requesting frames.
    let old_region_runs = region_runs.get();
    items.set(4);
    clock_active.set(true);
    invalidation::request_frame();
    frame(&state);
    assert_eq!(root_runs.get(), root_count);
    assert_eq!(region_runs.get(), old_region_runs + 1);
    assert!(invalidation::pending(window_scope::WindowScope::Primary));
    assert!(crate::core::tree::find_in_tree(
        &state.borrow().as_ref().unwrap().root_element,
        |el| el.template_id.as_deref() == Some("hybrid-item-3")
    )
    .is_some());
    assert_matches_full(&state, "immediate insertion");
    clock_active.set(false);
    frame(&state);
    assert!(!invalidation::pending(window_scope::WindowScope::Primary));

    // L3 reruns declarations once, retaining stable identities and text storage.
    let warm_text = text_buffers(&state);
    invalidation::rebuild_all();
    frame(&state);
    assert_eq!(root_runs.get(), root_count + 1);
    assert_eq!(text_buffers(&state), warm_text);
    assert_eq!(
        invalidation::invalidation_stats().cache_busts,
        invalidations.cache_busts
    );
    assert_matches_full(&state, "L3 warm rebuild");

    // L4 is the explicit resource boundary. Check before render repopulates it.
    invalidation::bust_caches();
    invalidation::begin_frame();
    {
        let state = state.borrow();
        let state = state.as_ref().unwrap();
        assert!(state.shadow_tex_cache.is_empty());
        assert!(state.shadow_rect_cache.is_empty());
        assert!(
            !state
                .image_textures
                .keys()
                .any(|key| { crate::utils::text_texture::is_text_xform_texture_key(key) }),
            "L4 must remove GPU text textures as well as CPU pixels"
        );
    }
    assert!(state
        .borrow()
        .as_ref()
        .unwrap()
        .text_renderer
        .shaped_cache
        .is_empty());
    assert_eq!(
        invalidation::invalidation_stats().cache_busts,
        invalidations.cache_busts + 1
    );
    reactive::drain_deferred_view_remounts();
    render(&state);
    assert!(!text_buffers(&state).is_empty());
    assert_matches_full(&state, "L4 cache rebuild");
    eprintln!(
        "hybrid invalidation native oracle: {:?}",
        invalidation::invalidation_stats()
    );

    // Same template id in two real windows: invalidation and hit geometry must
    // stay local, including after layout changes and shared-font cache resets.
    let secondary_id = secondary_window.id();
    runtime::register_window(&secondary_window, false);
    let secondary_runs = Rc::new(Cell::new(0));
    let secondary_width = Rc::new(Cell::new(80.0));
    let secondary_root = |width: f32| {
        Element::new().w("100%").h("100%").bg([1.0; 4]).child(
            Element::new()
                .template_id("hybrid-paint")
                .focus_id("secondary-leaf")
                .absolute()
                .left(8.0)
                .top(48.0)
                .w(width)
                .h(36.0)
                .bg([0.8, 0.2, 0.4, 1.0])
                .text("secondary"),
        )
    };
    let secondary_owner = {
        let _scope = window_scope::enter_secondary_id(secondary_id);
        let sec = {
            let st = state.borrow();
            let st = st.as_ref().unwrap();
            crate::app::secondary_window::SecondaryWindowState::new(
                secondary_window,
                st.device.clone(),
                st.queue.clone(),
                &st.instance,
                &st.adapter,
                &st.config,
                secondary_root(80.0),
                Default::default(),
            )
            .expect("secondary surface")
        };
        secondaries.borrow_mut().insert(secondary_id, sec);
        let secs = secondaries.clone();
        let runs = secondary_runs.clone();
        let width = secondary_width.clone();
        create_root(move || {
            create_effect(move || {
                let _scope = window_scope::enter_secondary_id(secondary_id);
                invalidation::register_root(current_effect_id().unwrap());
                runs.set(runs.get() + 1);
                secs.borrow_mut()
                    .get_mut(&secondary_id)
                    .unwrap()
                    .replace_root(secondary_root(width.get()));
            });
        })
        .0
    };
    let secondary_frame = || {
        let _scope = window_scope::enter_secondary_id(secondary_id);
        invalidation::begin_frame();
        secondaries
            .borrow_mut()
            .get_mut(&secondary_id)
            .unwrap()
            .render([-1.0; 2])
            .unwrap();
    };
    secondary_frame();
    let rebuilds = secondaries.borrow()[&secondary_id]
        .layout_tree
        .full_rebuilds;
    let leaf_id = secondaries.borrow()[&secondary_id].root_element.children[0].layout_node;
    secondaries
        .borrow_mut()
        .get_mut(&secondary_id)
        .unwrap()
        .layout_tree
        .last_compute_root = None;
    secondary_frame();
    assert_eq!(
        secondaries.borrow()[&secondary_id]
            .layout_tree
            .last_compute_root,
        None,
        "clean secondary frames must not run Taffy compute"
    );
    let primary_pixels = read_frame(&state);
    secondary_width.set(112.0);
    {
        let _scope = window_scope::enter_secondary_id(secondary_id);
        secondaries
            .borrow_mut()
            .get_mut(&secondary_id)
            .unwrap()
            .root_element
            .children[0]
            .style
            .size
            .width = taffy::prelude::Dimension::length(112.0);
        invalidation::node("hybrid-paint").mark_layout();
    }
    secondary_frame();
    {
        let secs = secondaries.borrow();
        let sec = &secs[&secondary_id];
        assert_eq!(
            sec.layout_tree.full_rebuilds, rebuilds,
            "L2 must retain the secondary tree"
        );
        assert_eq!(sec.root_element.children[0].layout_node, leaf_id);
        assert!((sec.root_element.children[0].layout_bounds[2] - 112.0).abs() < 0.1);
        assert_eq!(
            crate::app::tree_hit::find_focus_id_at(&sec.root_element, 100.0, 56.0, [0.0; 2]),
            Some("secondary-leaf".into())
        );
        let texture = sec.present_retain.color_texture_for_test().unwrap();
        let st = state.borrow();
        let st = st.as_ref().unwrap();
        let image = readback_texture(
            &st.device,
            &st.queue,
            texture,
            texture.width(),
            texture.height(),
            st.device.create_command_encoder(&Default::default()),
        )
        .unwrap();
        assert!(
            image.pixels.iter().any(|&v| v != 255),
            "secondary must contain painted content"
        );
    }
    assert!(
        images_match(&primary_pixels, &read_frame(&state), 0),
        "secondary L2 cannot change primary pixels"
    );
    let primary_runs = root_runs.get();
    {
        let _scope = window_scope::enter_secondary_id(secondary_id);
        invalidation::rebuild_all();
    }
    secondary_frame();
    assert_eq!(secondary_runs.get(), 2);
    assert_eq!(
        root_runs.get(),
        primary_runs,
        "secondary L3 cannot rerun primary declarations"
    );
    {
        let _scope = window_scope::enter_secondary_id(secondary_id);
        invalidation::bust_caches();
    }
    secondary_frame();
    assert!(invalidation::pending(window_scope::WindowScope::Primary));
    frame(&state);
    assert_eq!(
        root_runs.get(),
        primary_runs + 1,
        "L4 must reach both font-sharing windows"
    );
    assert_eq!(secondary_runs.get(), 3);
    secondary_owner.dispose();
    runtime::unregister_window(secondary_id);
    window_scope::forget_secondary(secondary_id);
    secondaries.borrow_mut().remove(&secondary_id);

    runtime::clear_app_runtime();
    owner.dispose();
    reactive::reset_registered_anchors();
    crate::style::set_css_transition_virtual_timeline(false);
}
