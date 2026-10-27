use jadix::core::reactivity::create_root;
use jadix::core::tree::Element;
use jadix_dnd::utils::drag::runtime::{
    self, CollectGeometry, DebugRect, DragKey, DragRuntime, MoveMode, RuntimeState, Update,
};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

struct ExternalRuntime {
    tree: Rc<RefCell<Element>>,
    events: Rc<RefCell<Vec<String>>>,
    active: Rc<Cell<bool>>,
    remove_on_sample: Cell<bool>,
    remove_on_state: Cell<bool>,
    remove_on_drop: Cell<bool>,
    deadline: Instant,
}

impl ExternalRuntime {
    fn install() -> Rc<Self> {
        let runtime = Rc::new(Self {
            tree: Rc::new(RefCell::new(Element::new().text("original"))),
            events: Rc::default(),
            active: Rc::new(Cell::new(true)),
            remove_on_sample: Cell::new(false),
            remove_on_state: Cell::new(false),
            remove_on_drop: Cell::new(false),
            deadline: Instant::now() + Duration::from_secs(1),
        });
        runtime::register(runtime.clone());
        runtime
    }
}

impl Drop for ExternalRuntime {
    fn drop(&mut self) {
        self.events.borrow_mut().push("drop".into());
        if self.remove_on_drop.get() {
            runtime::unregister();
        }
    }
}

impl DragRuntime for ExternalRuntime {
    fn state(&self) -> RuntimeState {
        if self.remove_on_state.get() {
            runtime::unregister();
        }
        RuntimeState {
            dragging: self.active.get(),
            active: self.active.get(),
            coasting: false,
            activation_deadline: Some(self.deadline),
            pointer: Some([10.0, 20.0]),
        }
    }

    fn prepare_move(
        &self,
        root: Option<&Element>,
        pointer: [f32; 2],
        viewport: [f32; 2],
        mode: MoveMode,
    ) -> Update {
        if self.remove_on_sample.get() {
            runtime::unregister();
        }
        let sampled = root.and_then(|root| root.text.clone());
        let tree = self.tree.clone();
        let events = self.events.clone();
        Box::new(move || {
            tree.borrow_mut().text = Some(format!("{pointer:?}"));
            events
                .borrow_mut()
                .push(format!("{sampled:?}:{viewport:?}:{mode:?}"));
            true
        })
    }

    fn prepare_end(&self, root: &Element) -> Update {
        let sampled = root.text.clone();
        let tree = self.tree.clone();
        let active = self.active.clone();
        Box::new(move || {
            tree.borrow_mut().text = sampled;
            active.set(false);
            runtime::unregister();
            true
        })
    }

    fn refresh_geometry(&self, root: &Element) {
        self.events
            .borrow_mut()
            .push(format!("geometry:{:?}", root.text));
    }

    fn prepare_autoscroll(&self, _root: &Element, viewport: [f32; 2]) -> Update {
        let events = self.events.clone();
        let tree = self.tree.clone();
        Box::new(move || {
            tree.borrow_mut().text = Some("scroll".into());
            events.borrow_mut().push(format!("scroll:{viewport:?}"));
            true
        })
    }

    fn prepare_inertia(&self, _root: &Element, viewport: [f32; 2]) -> Update {
        let events = self.events.clone();
        let tree = self.tree.clone();
        Box::new(move || {
            tree.borrow_mut().text = Some("inertia".into());
            events.borrow_mut().push(format!("inertia:{viewport:?}"));
            true
        })
    }

    fn key(&self, key: DragKey) -> bool {
        self.events.borrow_mut().push(format!("key:{key:?}"));
        if key == DragKey::Cancel {
            self.active.set(false);
            runtime::unregister();
        }
        true
    }

    fn sync_tree(&self, root: &mut Element) {
        root.set_attr("board-synced", "1");
    }

    fn after_layout(&self, root: &mut Element) {
        root.set_attr("board-laid-out", "1");
    }

    fn keeps_collect_live(&self, element: &Element) -> bool {
        element.attrs.contains_key("board-live")
    }

    fn collect_geometry(
        &self,
        element: &mut Element,
        size: [f32; 2],
        _scale: [f32; 2],
    ) -> CollectGeometry {
        CollectGeometry {
            size,
            origin: self.keeps_collect_live(element).then_some([5.0, 10.0]),
        }
    }

    fn ancestor_clip(&self, element: &Element) -> Option<[f32; 4]> {
        self.keeps_collect_live(element)
            .then_some([0.0, 0.0, 100.0, 100.0])
    }

    fn debug_rects(&self, root: &Element) -> Vec<DebugRect> {
        if !root.attrs.contains_key("board-debug") {
            return Vec::new();
        }
        vec![DebugRect {
            rect: [6.0, 11.0, 12.0, 13.0],
            fill: [0.2, 0.3, 0.4, 0.5],
            border: [0.6, 0.7, 0.8, 0.9],
            width: 2.0,
            dashed: true,
        }]
    }
}

#[test]
fn host_without_a_product_runtime_is_idle() {
    runtime::unregister();
    let tree = Element::new();
    assert!(!runtime::is_dragging());
    assert!(!runtime::is_active());
    assert!(!runtime::is_coasting());
    assert!(runtime::state().activation_deadline.is_none());
    assert!(
        !runtime::prepare_move(Some(&tree), [1.0, 2.0], [30.0, 40.0], MoveMode::Follow).dispatch()
    );
    assert!(!runtime::prepare_end(&tree).dispatch());
    assert!(!runtime::key(DragKey::Cancel));
    assert!(!runtime::pump_autoscroll(&tree, [30.0, 40.0]));
    assert!(!runtime::pump_inertia(&tree, [30.0, 40.0]));
    runtime::refresh_geometry(&tree);
}

#[test]
fn sampled_moves_and_release_can_rebuild_the_host_tree_after_its_borrow_ends() {
    let product = ExternalRuntime::install();
    let update = {
        let tree = product.tree.borrow();
        let update =
            runtime::prepare_move(Some(&tree), [3.0, 7.0], [300.0, 200.0], MoveMode::Follow);
        assert_eq!(tree.text.as_deref(), Some("original"));
        update
    };
    assert!(update.dispatch());
    assert_eq!(product.tree.borrow().text.as_deref(), Some("[3.0, 7.0]"));
    assert_eq!(
        product.events.borrow()[0],
        "Some(\"original\"):[300.0, 200.0]:Follow"
    );
    let release = runtime::prepare_end(&product.tree.borrow());
    assert!(release.dispatch());
    assert!(!product.active.get());
    assert!(!runtime::is_dragging());
}

#[test]
fn replacing_a_runtime_discards_its_prepared_moves_and_release() {
    let first = ExternalRuntime::install();
    let movement = runtime::prepare_move(None, [1.0, 2.0], [30.0, 40.0], MoveMode::Reconcile);
    let release = runtime::prepare_end(&first.tree.borrow());
    let second = ExternalRuntime::install();
    assert!(!movement.dispatch());
    assert!(!release.dispatch());
    assert!(first.active.get());
    assert!(second.active.get());
    assert_eq!(first.tree.borrow().text.as_deref(), Some("original"));
    runtime::unregister();
}

#[test]
fn same_runtime_keeps_work_until_its_registration_lifetime_ends() {
    let product = ExternalRuntime::install();
    let movement = runtime::prepare_move(None, [1.0, 2.0], [30.0, 40.0], MoveMode::Reconcile);
    runtime::register(product.clone());
    assert!(movement.dispatch());
    let stale = runtime::prepare_move(None, [8.0, 9.0], [30.0, 40.0], MoveMode::Follow);
    runtime::unregister();
    runtime::register(product.clone());
    assert!(!stale.dispatch());
    assert_eq!(product.tree.borrow().text.as_deref(), Some("[1.0, 2.0]"));
    runtime::unregister();
}

#[test]
fn sampling_and_state_queries_can_remove_their_runtime_without_a_registry_borrow_panic() {
    let product = ExternalRuntime::install();
    product.remove_on_sample.set(true);
    let update = runtime::prepare_move(None, [1.0, 2.0], [30.0, 40.0], MoveMode::Follow);
    assert!(!update.dispatch());
    runtime::register(product.clone());
    product.remove_on_state.set(true);
    assert!(runtime::state().active);
    assert!(!runtime::state().active);
}

#[test]
fn product_owns_keyboard_scroll_and_activation_policy() {
    let product = ExternalRuntime::install();
    assert_eq!(runtime::state().activation_deadline, Some(product.deadline));
    assert_eq!(runtime::state().pointer, Some([10.0, 20.0]));
    runtime::refresh_geometry(&product.tree.borrow());
    let scroll = runtime::prepare_autoscroll(&product.tree.borrow(), [300.0, 200.0]);
    assert!(scroll.dispatch());
    let inertia = runtime::prepare_inertia(&product.tree.borrow(), [300.0, 200.0]);
    assert!(inertia.dispatch());
    assert!(runtime::key(DragKey::Next));
    assert!(runtime::key(DragKey::Previous));
    assert!(runtime::key(DragKey::Cancel));
    assert!(!product.active.get());
    assert!(!runtime::key(DragKey::Next));
    assert_eq!(
        *product.events.borrow(),
        [
            "geometry:Some(\"original\")",
            "scroll:[300.0, 200.0]",
            "inertia:[300.0, 200.0]",
            "key:Next",
            "key:Previous",
            "key:Cancel",
        ]
    );
}

#[test]
fn runtime_teardown_can_reenter_the_registry_after_removal() {
    let product = ExternalRuntime::install();
    product.remove_on_drop.set(true);
    let events = product.events.clone();
    drop(product);
    runtime::unregister();
    assert_eq!(*events.borrow(), ["drop"]);
    assert!(!runtime::is_active());
}

#[test]
fn external_runtime_controls_layout_collect_clipping_and_debug_paint() {
    let _product = ExternalRuntime::install();
    let mut child = Element::new().w(40.0).h(20.0);
    child.set_attr("board-live", "1");
    let mut root = Element::new().w(100.0).h(100.0).child(child);
    root.set_attr("board-debug", "1");
    let rendered = jadix::layout::layout_tree::LayoutTree::new()
        .build_and_layout(&mut root, 100.0, 100.0, [-1.0; 2]);
    assert_eq!(
        root.attrs.get("board-synced").map(String::as_str),
        Some("1")
    );
    assert_eq!(
        root.attrs.get("board-laid-out").map(String::as_str),
        Some("1")
    );
    let child = root
        .children
        .iter()
        .find(|child| child.attrs.contains_key("board-live"))
        .unwrap();
    assert!(runtime::keeps_collect_live(child));
    assert_eq!(child.layout_bounds, [5.0, 10.0, 40.0, 20.0]);
    assert_eq!(
        runtime::ancestor_clip(child),
        Some([0.0, 0.0, 100.0, 100.0])
    );
    assert!(rendered.instances.iter().any(|instance| {
        instance.rect_pos == [6.0, 11.0]
            && instance.rect_size == [12.0, 13.0]
            && instance.color == [0.2, 0.3, 0.4, 0.5]
            && instance.border_color == [0.6, 0.7, 0.8, 0.9]
            && instance.border_width == 2.0
    }));
    runtime::unregister();
}

#[test]
fn built_in_dnd_registers_and_runs_through_the_same_host_contract() {
    use jadix_dnd::utils::drag::{self, BeginDrag};
    let (_owner, ()) = create_root(|| {
        runtime::unregister();
        let ended = Rc::new(Cell::new(false));
        let callback = ended.clone();
        let tree = Rc::new(RefCell::new(Element::new()));
        let callback_tree = tree.clone();
        *tree.borrow_mut() = drag::drag_scope("runtime-dnd")
            .threshold(4.0)
            .autoscroll(false)
            .on_drag_end(move |_| {
                callback_tree.borrow_mut().text = Some("committed".into());
                callback.set(true);
            })
            .child(Element::new);
        drag::begin(BeginDrag {
            scope_id: "runtime-dnd".into(),
            draggable_id: "card".into(),
            pointer: [10.0, 10.0],
            grab_offset: [0.0, 0.0],
            start_bounds: [10.0, 10.0, 40.0, 24.0],
            from_list: Some("list".into()),
            from_index: Some(0),
            item_type: "card".into(),
            group: String::new(),
            capture_id: "runtime-card".into(),
        });
        assert!(runtime::is_dragging());
        assert!(!runtime::is_active());
        let update = runtime::prepare_move(
            Some(&tree.borrow()),
            [30.0, 10.0],
            [400.0, 300.0],
            MoveMode::Follow,
        );
        assert!(update.dispatch());
        assert!(runtime::is_active());
        let follow = runtime::prepare_move(
            Some(&tree.borrow()),
            [35.0, 10.0],
            [400.0, 300.0],
            MoveMode::Follow,
        );
        assert!(follow.dispatch());
        let release = runtime::prepare_end(&tree.borrow());
        assert!(release.dispatch());
        assert!(ended.get());
        assert_eq!(tree.borrow().text.as_deref(), Some("committed"));
        assert!(!runtime::is_dragging());
        runtime::unregister();
    });
}

#[test]
fn disposing_an_old_scope_owner_keeps_the_replacement_provider() {
    use jadix_dnd::utils::drag;
    let (old_owner, old_root) =
        create_root(|| drag::drag_scope("replaced-owner").child(Element::new));
    let (new_owner, new_root) =
        create_root(|| drag::drag_scope("replaced-owner").child(Element::new));
    old_owner.dispose();
    assert!(drag::session::scope_exists("replaced-owner"));
    begin_scroll_card("replaced-owner");
    assert!(drag::is_dragging());
    new_owner.dispose();
    assert!(!drag::is_dragging());
    assert!(!drag::session::scope_exists("replaced-owner"));
    drop((old_root, new_root));
    runtime::unregister();
}

#[test]
fn scope_cleanup_releases_metadata_and_allows_its_destructor_to_reenter() {
    use jadix_dnd::utils::drag;
    struct Metadata {
        drops: Rc<Cell<usize>>,
    }
    impl Drop for Metadata {
        fn drop(&mut self) {
            self.drops.set(self.drops.get() + 1);
            assert!(drag::registry::item_data("metadata-owner", "card").is_none());
            drag::registry::set_item_data("other-owner", "survivor", Rc::new(7usize));
        }
    }
    let drops = Rc::new(Cell::new(0));
    let (owner, root) = create_root(|| drag::drag_scope("metadata-owner").child(Element::new));
    drag::registry::set_item_data(
        "metadata-owner",
        "card",
        Rc::new(Metadata {
            drops: drops.clone(),
        }),
    );
    owner.dispose();
    assert_eq!(drops.get(), 1);
    assert!(drag::registry::item_data("metadata-owner", "card").is_none());
    assert!(drag::registry::item_data("other-owner", "survivor").is_some());
    drop(root);
    runtime::unregister();
}

#[test]
fn cancelling_a_runtime_retires_all_prepared_work_before_callbacks() {
    let runtime = ExternalRuntime::install();
    let root = Element::new();
    let movement =
        runtime::prepare_move(Some(&root), [20.0, 30.0], [400.0, 300.0], MoveMode::Follow);
    let ending = runtime::prepare_end(&root);
    let scroll = runtime::prepare_autoscroll(&root, [400.0, 300.0]);
    assert!(runtime::cancel());
    assert!(!movement.dispatch());
    assert!(!ending.dispatch());
    assert!(!scroll.dispatch());
    assert_eq!(runtime.events.borrow().as_slice(), ["key:Cancel"]);
}

#[test]
fn disposed_dnd_owner_cancels_armed_session_without_end_callback() {
    use jadix_dnd::utils::drag;
    let ended = Rc::new(Cell::new(false));
    let (owner, root) = create_root({
        let ended = ended.clone();
        move || {
            drag::drag_scope("owner-cleanup")
                .threshold(10.0)
                .on_drag_end(move |_| ended.set(true))
                .child(Element::new)
        }
    });
    begin_scroll_card("owner-cleanup");
    let update = runtime::prepare_move(Some(&root), [30.0, 10.0], [400.0, 300.0], MoveMode::Follow);
    owner.dispose();
    assert!(!drag::session::scope_exists("owner-cleanup"));
    assert!(!drag::is_dragging());
    assert!(!ended.get());
    assert!(!update.dispatch());
    runtime::unregister();
}

#[test]
fn a_new_dnd_session_rejects_old_move_and_end_samples() {
    use jadix_dnd::utils::drag;
    let (_owner, ()) = create_root(|| {
        let root = drag::drag_scope("move-generation")
            .autoscroll(false)
            .child(Element::new);
        begin_scroll_card("move-generation");
        let movement = runtime::prepare_move(
            Some(&root),
            [30.0, 10.0],
            [400.0, 300.0],
            MoveMode::Reconcile,
        );
        let ending = runtime::prepare_end(&root);
        drag::cancel();
        begin_scroll_card("move-generation");
        assert!(!movement.dispatch());
        assert!(!ending.dispatch());
        assert!(drag::is_dragging());
        assert!(runtime::cancel());
        assert!(!drag::is_dragging());
        runtime::unregister();
    });
}
#[test]
fn inertia_callback_unwind_restores_the_coasting_input_guard() {
    use jadix::utils::pointer_action::InertiaOpts;
    use jadix_dnd::utils::drag::{self, SnapGrid};
    use std::panic::{catch_unwind, AssertUnwindSafe};
    let (_owner, ()) = create_root(|| {
        let armed = Rc::new(Cell::new(false));
        let callback_armed = armed.clone();
        let tree = drag::drag_scope("unwind-inertia")
            .threshold(0.0)
            .autoscroll(false)
            .snap(SnapGrid::new(50.0, 50.0).end_only(true))
            .inertia_opts(InertiaOpts {
                enabled: true,
                min_speed: f32::MAX,
                ..Default::default()
            })
            .on_drag_move(move |_| {
                assert!(!callback_armed.get(), "product callback panic");
            })
            .child(Element::new);
        begin_scroll_card("unwind-inertia");
        let activation = runtime::prepare_move(
            Some(&tree),
            [133.0, 50.0],
            [400.0, 300.0],
            MoveMode::Reconcile,
        );
        assert!(activation.dispatch());
        assert!(runtime::prepare_end(&tree).dispatch());
        assert!(runtime::is_coasting());
        armed.set(true);
        let update = runtime::prepare_inertia(&tree, [400.0, 300.0]);
        assert!(catch_unwind(AssertUnwindSafe(|| update.dispatch())).is_err());
        armed.set(false);
        let pointer = runtime::state().pointer;
        let ordinary_move = runtime::prepare_move(
            Some(&tree),
            [250.0, 250.0],
            [400.0, 300.0],
            MoveMode::Reconcile,
        );
        assert!(!ordinary_move.dispatch());
        assert_eq!(runtime::state().pointer, pointer);
        drag::cancel();
        runtime::unregister();
    });
}

#[test]
fn scroll_updates_mutate_the_tree_only_after_sampling_releases_it() {
    let product = ExternalRuntime::install();
    let (scroll, inertia) = {
        let tree = product.tree.borrow();
        let scroll = runtime::prepare_autoscroll(&tree, [300.0, 200.0]);
        let inertia = runtime::prepare_inertia(&tree, [300.0, 200.0]);
        assert_eq!(tree.text.as_deref(), Some("original"));
        assert!(product.events.borrow().is_empty());
        (scroll, inertia)
    };
    assert!(scroll.dispatch());
    assert_eq!(product.tree.borrow().text.as_deref(), Some("scroll"));
    assert!(inertia.dispatch());
    assert_eq!(product.tree.borrow().text.as_deref(), Some("inertia"));
    runtime::unregister();
}

#[test]
fn replaced_and_reregistered_runtimes_discard_prepared_scroll_work() {
    let first = ExternalRuntime::install();
    let scroll = runtime::prepare_autoscroll(&first.tree.borrow(), [300.0, 200.0]);
    let inertia = runtime::prepare_inertia(&first.tree.borrow(), [300.0, 200.0]);
    let second = ExternalRuntime::install();
    assert!(!scroll.dispatch());
    assert!(!inertia.dispatch());
    assert!(first.events.borrow().is_empty());
    let scroll = runtime::prepare_autoscroll(&second.tree.borrow(), [300.0, 200.0]);
    let inertia = runtime::prepare_inertia(&second.tree.borrow(), [300.0, 200.0]);
    runtime::unregister();
    runtime::register(second.clone());
    assert!(!scroll.dispatch());
    assert!(!inertia.dispatch());
    assert!(second.events.borrow().is_empty());
    runtime::unregister();
}

fn begin_scroll_card(scope: &str) {
    jadix_dnd::utils::drag::begin(jadix_dnd::utils::drag::BeginDrag {
        scope_id: scope.into(),
        draggable_id: "card".into(),
        pointer: [110.0, 50.0],
        grab_offset: [10.0, 10.0],
        start_bounds: [100.0, 40.0, 80.0, 24.0],
        from_list: None,
        from_index: None,
        item_type: "card".into(),
        group: String::new(),
        capture_id: "scroll-card".into(),
    });
}

#[test]
fn built_in_autoscroll_samples_after_scroll_and_dispatches_without_a_tree_borrow() {
    use jadix::core::geometry::Axis;
    use jadix::core::reactivity::use_signal;
    use jadix::core::scroll_viewport::{
        scroll_content_h_signal_key, scroll_offset_x_signal_key, scroll_offset_y_signal_key,
        scroll_viewport_h_signal_key, SCROLL_VIEWPORT_FOCUS_PREFIX,
    };
    use jadix_dnd::utils::drag;
    let (_owner, ()) = create_root(|| {
        let tree = Rc::new(RefCell::new(Element::new()));
        let armed = Rc::new(Cell::new(false));
        let callback_tree = tree.clone();
        let callback_armed = armed.clone();
        let mut card = Element::new();
        card.layout_bounds = [100.0, 40.0, 80.0, 24.0];
        card.set_attr(drag::ATTR_DRAG_DRAGGABLE, "card");
        card.set_attr(drag::ATTR_DRAG_SCOPE, "scroll-contract");
        let product = drag::drag_scope("scroll-contract")
            .axis(Axis::Y)
            .threshold(0.0)
            .on_drag_move(move |_| {
                if callback_armed.get() {
                    callback_tree.borrow_mut().text = Some("scroll callback".into());
                }
            })
            .child(move || card);
        let mut page = Element::new().child(product);
        page.focus_id = Some(format!("{SCROLL_VIEWPORT_FOCUS_PREFIX}scroll-contract"));
        page.layout_bounds = [0.0, 0.0, 400.0, 300.0];
        *tree.borrow_mut() = page;
        let offset_x = use_signal(&scroll_offset_x_signal_key("scroll-contract"), || 0.0_f32);
        let offset_y = use_signal(&scroll_offset_y_signal_key("scroll-contract"), || 0.0_f32);
        use_signal(&scroll_viewport_h_signal_key("scroll-contract"), || {
            300.0_f32
        });
        use_signal(&scroll_content_h_signal_key("scroll-contract"), || {
            900.0_f32
        });
        begin_scroll_card("scroll-contract");
        let activation = runtime::prepare_move(
            Some(&tree.borrow()),
            [110.0, 290.0],
            [400.0, 300.0],
            MoveMode::Reconcile,
        );
        assert!(activation.dispatch());
        armed.set(true);
        offset_x.set_silent(60.0);
        let update = runtime::prepare_autoscroll(&tree.borrow(), [400.0, 300.0]);
        assert!(offset_y.peek() > 0.0);
        assert_eq!(tree.borrow().text, None);
        assert!(update.dispatch());
        assert_eq!(tree.borrow().text.as_deref(), Some("scroll callback"));
        assert!((drag::last_overlay_pos("scroll-contract").unwrap()[0] - 40.0).abs() < 0.5);
        let stale = runtime::prepare_autoscroll(&tree.borrow(), [400.0, 300.0]);
        drag::cancel();
        armed.set(false);
        begin_scroll_card("scroll-contract");
        let activation = runtime::prepare_move(
            Some(&tree.borrow()),
            [110.0, 290.0],
            [400.0, 300.0],
            MoveMode::Reconcile,
        );
        assert!(activation.dispatch());
        armed.set(true);
        tree.borrow_mut().text = Some("new session".into());
        assert!(!stale.dispatch());
        assert_eq!(tree.borrow().text.as_deref(), Some("new session"));
        drag::cancel();
        runtime::unregister();
    });
}

#[test]
fn built_in_inertia_defers_movement_and_end_callbacks_until_dispatch() {
    use jadix::utils::pointer_action::InertiaOpts;
    use jadix_dnd::utils::drag::{self, SnapGrid};
    let (_owner, ()) = create_root(|| {
        let tree = Rc::new(RefCell::new(Element::new()));
        let armed = Rc::new(Cell::new(false));
        let ended = Rc::new(Cell::new(0usize));
        let callback_tree = tree.clone();
        let callback_armed = armed.clone();
        let end_tree = tree.clone();
        let end_count = ended.clone();
        *tree.borrow_mut() = drag::drag_scope("inertia-contract")
            .threshold(0.0)
            .autoscroll(false)
            .snap(SnapGrid::new(50.0, 50.0).end_only(true))
            .inertia_opts(InertiaOpts {
                enabled: true,
                min_speed: f32::MAX,
                smooth_end_duration: 0.016,
                ..Default::default()
            })
            .on_drag_move(move |_| {
                if callback_armed.get() {
                    callback_tree.borrow_mut().text = Some("inertia callback".into());
                }
            })
            .on_drag_end(move |_| {
                end_tree.borrow_mut().text = Some("inertia ended".into());
                end_count.set(end_count.get() + 1);
            })
            .child(Element::new);
        begin_scroll_card("inertia-contract");
        let activation = runtime::prepare_move(
            Some(&tree.borrow()),
            [133.0, 50.0],
            [400.0, 300.0],
            MoveMode::Reconcile,
        );
        assert!(activation.dispatch());
        let release = runtime::prepare_end(&tree.borrow());
        assert!(release.dispatch());
        assert!(runtime::is_coasting());
        armed.set(true);
        let before = runtime::state().pointer;
        let update = runtime::prepare_inertia(&tree.borrow(), [400.0, 300.0]);
        assert_eq!(runtime::state().pointer, before);
        assert_eq!(ended.get(), 0);
        assert!(update.dispatch());
        assert_ne!(runtime::state().pointer, before);
        assert_eq!(tree.borrow().text.as_deref(), Some("inertia callback"));
        let finish = runtime::prepare_inertia(&tree.borrow(), [400.0, 300.0]);
        assert_eq!(ended.get(), 0);
        assert!(finish.dispatch());
        assert_eq!(ended.get(), 1);
        assert_eq!(tree.borrow().text.as_deref(), Some("inertia ended"));
        assert!(!runtime::is_dragging());
        runtime::unregister();
    });
}

#[test]
fn cancelled_inertia_cannot_advance_a_new_session_on_the_same_runtime() {
    use jadix::utils::pointer_action::InertiaOpts;
    use jadix_dnd::utils::drag::{self, SnapGrid};
    let (_owner, ()) = create_root(|| {
        let tree = drag::drag_scope("stale-inertia")
            .threshold(0.0)
            .autoscroll(false)
            .snap(SnapGrid::new(50.0, 50.0).end_only(true))
            .inertia_opts(InertiaOpts {
                enabled: true,
                min_speed: f32::MAX,
                ..Default::default()
            })
            .child(Element::new);
        let start_coast = || {
            begin_scroll_card("stale-inertia");
            assert!(runtime::prepare_move(
                Some(&tree),
                [133.0, 50.0],
                [400.0, 300.0],
                MoveMode::Reconcile,
            )
            .dispatch());
            assert!(runtime::prepare_end(&tree).dispatch());
            assert!(runtime::is_coasting());
        };
        start_coast();
        let stale = runtime::prepare_inertia(&tree, [400.0, 300.0]);
        drag::cancel();
        start_coast();
        let pointer = runtime::state().pointer;
        assert!(!stale.dispatch());
        assert_eq!(runtime::state().pointer, pointer);
        assert!(runtime::is_coasting());
        drag::cancel();
        runtime::unregister();
    });
}

#[test]
fn activation_during_input_dispatch_does_not_reconcile_the_same_move_twice() {
    use jadix_dnd::utils::drag::{self, BeginDrag};
    let (_owner, ()) = create_root(|| {
        let moves = Rc::new(Cell::new(0));
        let counter = moves.clone();
        let root = drag::drag_scope("runtime-activation")
            .threshold(4.0)
            .autoscroll(false)
            .on_drag_move(move |_| counter.set(counter.get() + 1))
            .child(Element::new);
        drag::begin(BeginDrag {
            scope_id: "runtime-activation".into(),
            draggable_id: "card".into(),
            pointer: [10.0, 10.0],
            grab_offset: [0.0, 0.0],
            start_bounds: [10.0, 10.0, 40.0, 24.0],
            from_list: Some("list".into()),
            from_index: Some(0),
            item_type: "card".into(),
            group: String::new(),
            capture_id: "runtime-activation-card".into(),
        });
        let update =
            runtime::prepare_move(Some(&root), [30.0, 10.0], [400.0, 300.0], MoveMode::Follow);
        let hits = drag::collect_hits(&root);
        assert!(drag::tick(30.0, 10.0, &hits, [400.0, 300.0]));
        let after_activation = moves.get();
        assert!(after_activation > 0);
        assert!(update.dispatch());
        assert_eq!(moves.get(), after_activation);
        drag::cancel();
        runtime::unregister();
    });
}
