use crate::core::reactivity::{use_signal, Signal};

const STATEFUL_BODY: &str = r##"<Box w=96.0 h=64.0 bg="#ff0000">"HR6 A" {stateful_scene()}</Box>"##;

#[cfg(feature = "macros")]
fn compiled_stateful_static_scene() -> Element {
    crate::ui! { <Box w=96.0 h=64.0 bg="#ff0000">"HR6 A" {stateful_scene()}</Box> }
}

#[cfg(feature = "macros")]
#[test]
fn compiled_opaque_signature_matches_watcher_parser() {
    let compiled = crate::ui! { <Box>"old" {Element::new().text("captured")}</Box> };
    let description: jadix_hot_reload::TemplateDescription = serde_json::from_str(compiled.attrs.get("jadix_ui_description").unwrap()).unwrap();
    let watched = r#"<Box>"old" {Element::new().text("captured")}</Box>"#;
    assert_eq!(description.signature, jadix_hot_reload::syntax::template_signature(watched).unwrap());
    assert_eq!(description.tree, jadix_hot_reload::syntax::template_tree(watched));
}

fn registered_stateful_scene() -> Element {
    let mut description = jadix_hot_reload::TemplateDescription::new(
        jadix_hot_reload::syntax::template_signature(STATEFUL_BODY).unwrap(),
        jadix_hot_reload::syntax::declarations_signature(STATEFUL_BODY).unwrap(),
    );
    description.tree = jadix_hot_reload::syntax::template_tree(STATEFUL_BODY);
    Element::new().hot_reload_node_type("Box")
        .hot_reload_declaration_base(&["w", "h", "bg"], false)
        .w(96.0).h(64.0).bg("#ff0000")
        .child(Element::new().text("HR6 A"))
        .child(stateful_scene())
        .hot_reload_description(description)
}

struct StateProbe {
    value: Signal<String>,
    preedit: Signal<String>,
    caret: Signal<Option<usize>>,
    selection: Signal<Option<(usize, usize)>>,
    ime_selection: Signal<Option<(usize, usize)>>,
    focus: Signal<String>,
    clicks: std::rc::Rc<std::cell::Cell<usize>>,
    visible_start: Signal<usize>,
    deferred: Signal<usize>,
    tween: crate::style::CubicTween,
    animation_visible: Signal<bool>,
    task_scope: std::rc::Rc<super::tasks::TaskScope>,
    task_done: std::sync::Arc<std::sync::atomic::AtomicBool>,
    task_callbacks: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

thread_local! {
    static STATE_PROBE: std::cell::RefCell<Option<StateProbe>> = const { std::cell::RefCell::new(None) };
}

fn stateful_scene() -> Element {
    use crate::utils::text_field::*;
    use crate::utils::text_edit::{build_text_edit_element, TextEditBuildParams};
    let field = "hr6-state-field";
    let value = use_signal(&input_value_signal_key(field), || "seed中文".to_string());
    let preedit = use_signal(&input_preedit_signal_key(field), String::new);
    let caret = use_signal(&input_caret_signal_key(field), || None::<usize>);
    let ime_selection = use_signal(&input_selection_signal_key(field), || None::<(usize,usize)>);
    let selection = use_signal(&input_selection_signal_key("hr6-selection-field"), || None::<(usize,usize)>);
    use_signal(&input_value_signal_key("hr6-selection-field"), || "selection seed".to_string());
    let focus = use_signal("__jadix_button_focus_id", String::new);
    let clicks = std::rc::Rc::new(std::cell::Cell::new(0));
    let count = clicks.clone();
    let make_input = |field: &str| build_text_edit_element(TextEditBuildParams {
        field_id: field.into(), placeholder: "HR6 input".into(), disabled: false, invalid: false,
        input_type: crate::utils::text_edit::field::InputType::Text, clearable: false,
        password_toggle: false, number_steppers: false, step: 1.0, text_size_px: 14.0,
        font_weight: 400, line_height_px: None, leading_scale: None, value: None,
        text_align: crate::style::TextAlign::Start, single_line_container: Some(crate::primitives::hstack()),
        multiline: false, rows: 1, resizable: false, resize_max_h: 0.0,
        wrap_mode: TextareaWrapMode::Word, plain_chrome: false, caret_color: None,
        selection_bg: None, selection_foreground: None, selection_on_blur: SelectionOnBlur::Retain,
        undo: crate::utils::text_edit_history::TextUndoConfig::input_default(), drop_undo_on_unmount: true,
    }).w(260.0).h(36.0);
    let input = make_input(field);
    let selection_input = make_input("hr6-selection-field");
    let button = Element::new().w(150.0).h(32.0).focus_id("hr6-drag")
        .accesskit_role("button").accesskit_name("HR6 retained event")
        .bg("#55aa88").text("HR6 event")
        .on_click(move || count.set(count.get() + 1));
    let mut rows = Element::new().flex_col().w(260.0);
    for index in 0..40 {
        rows = rows.child(Element::new().w(250.0).h(20.0).flex_shrink(0.0).focus_id(format!("hr6-state-row-{index}")).text(format!("state row {index:02}")));
    }
    let viewport = crate::core::scroll_viewport::activate_scroll_viewport(
        Element::new().w(280.0).h(130.0).child(rows), "hr6-state-scroll", "hidden", "scroll", "jump", "scrollbar");
    let portal: Element = crate::primitives::portal("hr6-state-portal").child(
        Element::new().child(Element::new().absolute().left(400.0).top(180.0).w(110.0).h(40.0)
            .focus_id("hr6-overlay-hit").bg("#3377cc").text("HR6 overlay")
            .accesskit_name("HR6 overlay").on_click(|| {})),
    ).into();
    let visible_start=use_signal("hr6-visible-start",||0_usize);
    let deferred=crate::core::reactivity::create_deferred(move||visible_start.get());
    let windowed=super::keyed_list::for_keyed_reactive("hr6-windowed-list",
        move||(deferred.get()..deferred.get()+6).collect::<Vec<usize>>(),|index|*index,
        |index|Element::new().w(250.0).h(10.0).focus_id(format!("hr6-windowed-{index}")).text(format!("keyed row {index}")))
        .w(260.0).h(60.0).flex_col();
    let tween=crate::style::CubicTween::new_with_curve(0.0,60.0,(0.0,0.0,1.0,1.0));
    let animation_visible=use_signal("hr6-animation-visible",||false);
    let animated=tween.clone();
    let animation=super::reactive::reactive_view_sized("hr6-animation",16.0,8.0,move||
        Element::new().w(16.0).h(8.0).bg("#2288dd").opacity(if animation_visible.get(){animated.get()}else{1.0}));
    let (task_owner,task_scope)=crate::core::reactivity::create_root(||std::rc::Rc::new(super::tasks::TaskScope::new()));
    let (release_task,wait_task)=std::sync::mpsc::channel();
    let task_done=std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let completed=task_done.clone();
    let task_callbacks=std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let callbacks=task_callbacks.clone();
    let task=task_scope.spawn_blocking(move|cancel| {
        wait_task.recv().unwrap();
        completed.store(true,std::sync::atomic::Ordering::SeqCst);
        cancel.checkpoint()?;
        Ok(())
    },move|_|{callbacks.fetch_add(1,std::sync::atomic::Ordering::SeqCst);}).unwrap();
    STATE_PROBE.with(|probe| *probe.borrow_mut() = Some(StateProbe { value, preedit, caret, selection, ime_selection, focus, clicks,
        visible_start,deferred,tween,animation_visible,task_scope,task_done,task_callbacks }));
    Element::new().absolute().left(16.0).top(90.0).w(600.0).h(340.0).flex_col().gap(8.0)
        .focus_id("hr6-state-root")
        .child(input).child(selection_input).child(button)
        .child(viewport)
        .child(windowed).child(animation).child(portal)
        .on_unmount(move|| {task_owner.dispose();release_task.send(()).unwrap();drop(task);})
}

fn probe_snapshot(root: &Element) -> serde_json::Value {
    STATE_PROBE.with(|probe| {
        let probe = probe.borrow();
        let probe = probe.as_ref().unwrap();
        let input = super::pointer_events::find_by_focus_id(root, "input:hr6-state-field").unwrap();
        let scroll = super::pointer_events::find_by_focus_id(root, "scroll:hr6-state-scroll").unwrap();
        let button = super::pointer_events::find_by_focus_id(root, "hr6-drag").unwrap();
        assert!(super::pointer_events::find_by_focus_id(root, "hr6-overlay-hit").is_some());
        serde_json::json!({"value":probe.value.peek(),"preedit":probe.preedit.peek(),
            "caret":probe.caret.peek(),"selection":probe.selection.peek(),"ime_selection":probe.ime_selection.peek(),"focus":probe.focus.peek(),
            "pointer_capture":super::pointer_capture_id(),"scroll":scroll.scroll_offset,
            "input_instance":input.inspector_instance_id,"button_instance":button.inspector_instance_id,
            "clicks":probe.clicks.get(),"visible_start":probe.visible_start.peek(),"deferred":probe.deferred.peek(),
            "windowed_instances":(probe.deferred.peek()..probe.deferred.peek()+6).map(|index|
                super::pointer_events::find_by_focus_id(root,&format!("hr6-windowed-{index}")).unwrap().inspector_instance_id).collect::<Vec<_>>(),
            "task_scope_closed":probe.task_scope.is_closed(),"task_done":probe.task_done.load(std::sync::atomic::Ordering::SeqCst),
            "task_callbacks":probe.task_callbacks.load(std::sync::atomic::Ordering::SeqCst)})
    })
}

impl Harness {
    fn arm_state(&mut self, event_loop: &dyn ActiveEventLoop) {
        STATE_PROBE.with(|probe| {
            let probe = probe.borrow(); let probe = probe.as_ref().unwrap();
            probe.visible_start.set(6);
            probe.animation_visible.set(true);probe.tween.set_at_frame_start(1.0);
            probe.focus.set("input:hr6-state-field".into());
            probe.value.set("seed中文".into()); probe.caret.set(Some(4)); probe.ime_selection.set(Some((0,4))); probe.selection.set(Some((0,4)));
        });
        let app = self.app.as_mut().unwrap();
        let window = app.state.borrow().as_ref().unwrap().window.id();
        app.window_event(event_loop,window,WindowEvent::Ime(winit::event::Ime::Enabled));
        app.window_event(event_loop,window,WindowEvent::Ime(winit::event::Ime::Preedit("拼".into(),Some((3,3)))));
        super::set_pointer_capture("hr6-drag");
        set_native_scroll("hr6-state-scroll",72.0);
        app.state.borrow_mut().as_mut().unwrap().update_scroll_offset_in_place("hr6-state-scroll",[0.0,72.0]);
        app.window_event(event_loop,window,WindowEvent::RedrawRequested);
        let state = app.state.borrow(); let root = &state.as_ref().unwrap().root_element;
        let snapshot = probe_snapshot(root);
        assert_eq!(snapshot["preedit"], "拼");
        assert_eq!(snapshot["selection"], serde_json::json!([0,4]));
        assert_eq!(snapshot["scroll"], serde_json::json!([0.0,72.0]));
        std::fs::write(self.directory.join("state-before.json"),serde_json::to_vec_pretty(&snapshot).unwrap()).unwrap();
        self.state_expected = Some(snapshot);
    }

    fn finish_state(&mut self, event_loop: &dyn ActiveEventLoop, name: &str) {
        if !self.mode.starts_with("cold-") {
            let state = self.app.as_ref().unwrap().state.borrow();
            let snapshot = probe_snapshot(&state.as_ref().unwrap().root_element);
            assert_eq!(Some(&snapshot),self.state_expected.as_ref(),"all active editing state must survive the real source transaction");
            std::fs::write(self.directory.join("state-after.json"),serde_json::to_vec_pretty(&snapshot).unwrap()).unwrap();
        }
        super::release_pointer_capture();
        let app = self.app.as_mut().unwrap();
        let window = app.state.borrow().as_ref().unwrap().window.id();
        if !self.mode.starts_with("cold-") {
            app.window_event(event_loop,window,WindowEvent::Ime(winit::event::Ime::Commit("测".into())));
            STATE_PROBE.with(|probe| assert_eq!(probe.borrow().as_ref().unwrap().value.peek(), "测中文"));
        }
        STATE_PROBE.with(|probe| {
            let probe = probe.borrow(); let probe = probe.as_ref().unwrap();
            probe.visible_start.set(0);probe.animation_visible.set(false);
            probe.focus.set(String::new()); probe.preedit.set(String::new());
            probe.selection.set(None); probe.caret.set(None); probe.value.set("seed中文".into());
        });
        set_native_scroll("hr6-state-scroll",0.0);
        app.state.borrow_mut().as_mut().unwrap().update_scroll_offset_in_place("hr6-state-scroll",[0.0,0.0]);
        app.about_to_wait(event_loop);
        app.state.borrow_mut().as_mut().unwrap().mark_full_redraw();
        app.window_event(event_loop,window,WindowEvent::RedrawRequested);
        let (bounds,scale) = {
            let state = app.state.borrow(); let state = state.as_ref().unwrap();
            let button = super::pointer_events::find_by_focus_id(&state.root_element,"hr6-drag").unwrap();
            (button.viewport_layout_bounds(),state.window.scale_factor())
        };
        let position = winit::dpi::PhysicalPosition::new((bounds[0]+10.0) as f64*scale,(bounds[1]+10.0) as f64*scale);
        app.window_event(event_loop,window,WindowEvent::PointerMoved {
            device_id:None,position,primary:true,source:winit::event::PointerSource::Mouse,
        });
        app.window_event(event_loop,window,WindowEvent::RedrawRequested);
        for state in [winit::event::ElementState::Pressed,winit::event::ElementState::Released] {
            app.window_event(event_loop,window,WindowEvent::PointerButton {
                device_id:None,position,primary:true,
                button:winit::event::ButtonSource::Mouse(winit::event::MouseButton::Left),state,is_macos_activation_click:false,
            });
        }
        let clicks = STATE_PROBE.with(|probe| probe.borrow().as_ref().unwrap().clicks.get());
        assert_eq!(clicks,1,"native hit testing must reach the original captured event after patch/recompile");
        let hit_proof={
            let state=app.state.borrow();let root=&state.as_ref().unwrap().root_element;
            let scroll=super::pointer_events::find_by_focus_id(root,"scroll:hr6-state-scroll").unwrap().viewport_layout_bounds();
            let outside=super::pointer_events::hit_path(root,scroll[0]+10.0,scroll[1]+scroll[3]+8.0);
            assert!(!outside.iter().any(|entry|entry.focus_id.as_deref().is_some_and(|id|id.starts_with("hr6-state-row-"))));
            let overlay=super::pointer_events::find_by_focus_id(root,"hr6-overlay-hit").unwrap().viewport_layout_bounds();
            let hits=super::pointer_events::hit_path(root,overlay[0]+10.0,overlay[1]+10.0);
            assert!(hits.iter().any(|entry|entry.focus_id.as_deref()==Some("hr6-overlay-hit")));
            serde_json::json!({"clipped_rows_excluded":true,"portal_hit":true,"scroll_viewport":scroll,"overlay":overlay})
        };
        std::fs::write(self.directory.join(format!("{name}-events.json")),serde_json::to_vec_pretty(&serde_json::json!({"native_clicks":clicks,"ime_commit_checked":!self.mode.starts_with("cold-"),"button_bounds":bounds,"clip_and_portal":hit_proof})).unwrap()).unwrap();
        STATE_PROBE.with(|probe| probe.borrow().as_ref().unwrap().focus.set(String::new()));
        app.about_to_wait(event_loop);
        app.state.borrow_mut().as_mut().unwrap().mark_full_redraw();
        app.window_event(event_loop,window,WindowEvent::RedrawRequested);
    }
}

fn set_native_scroll(id: &str, offset: f32) {
    use crate::core::scroll_viewport::{scroll_offset_y_signal_key,scroll_target_y_signal_key};
    use_signal(&scroll_offset_y_signal_key(id),||0.0_f32).set_silent(offset);
    use_signal(&scroll_target_y_signal_key(id),||0.0_f32).set_silent(offset);
}

impl Harness {
    fn animation_value(&self) -> f32 {
        STATE_PROBE.with(|probe| {
            let probe=probe.borrow();let probe=probe.as_ref().unwrap();
            let value=probe.tween.peek();
            assert!(self.animation_samples.last().is_none_or(|previous|value>=*previous));
            value
        })
    }

    fn finish_or_wait(&mut self,event_loop:&dyn ActiveEventLoop) {
        if !self.mode.contains("stateful") {event_loop.exit();return;}
        if !self.mode.starts_with("cold-") && cfg!(feature="animation-base") {
            assert!(self.animation_samples.last().is_some_and(|value|*value>0.0));
        }
        std::fs::write(self.directory.join("animation.json"),serde_json::to_vec_pretty(&serde_json::json!({
            "feature":cfg!(feature="animation-base"),"native_present_samples":self.animation_samples,
            "monotonic":true,"visual_sampling":"animation hidden only for the deterministic B/C sample"})).unwrap()).unwrap();
        let app=self.app.as_mut().unwrap();
        let window=app.state.borrow().as_ref().unwrap().window.id();
        app.state.borrow_mut().as_mut().unwrap().rebuild_ui(Element::new().w("100%").h("100%"));
        app.window_event(event_loop,window,WindowEvent::RedrawRequested);
        self.async_wait_started=Some(Instant::now());
    }

    fn finish_async(&mut self,event_loop:&dyn ActiveEventLoop) {
        let started=self.async_wait_started.unwrap();
        let done=STATE_PROBE.with(|probe| {
            let probe=probe.borrow();let probe=probe.as_ref().unwrap();
            let done=probe.task_done.load(std::sync::atomic::Ordering::SeqCst);
            if done {
                assert!(probe.task_scope.is_closed());
                assert_eq!(probe.task_callbacks.load(std::sync::atomic::Ordering::SeqCst),0);
            }
            done
        });
        assert!(started.elapsed()<Duration::from_secs(5),"unmounted owner must release blocked task");
        if done && started.elapsed()>Duration::from_millis(100) {
            std::fs::write(self.directory.join("async-unmount.json"),serde_json::to_vec_pretty(&serde_json::json!({
                "passed":true,"native_route_unmount":true,"owner_task_scope_closed":true,"worker_completed":true,"callbacks_after_unmount":0})).unwrap()).unwrap();
            event_loop.exit();
        }
    }
}
