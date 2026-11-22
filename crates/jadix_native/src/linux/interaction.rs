//! GTK recognizers observe native editors and claim only opted-in gestures.
use super::*;
use jadix_core::presentation::interaction::*;

#[derive(Default)]
struct Active(RefCell<Vec<InteractionEvent>>);

fn time() -> f64 {
    thread_local! { static START: std::time::Instant = std::time::Instant::now(); }
    START.with(|start| start.elapsed().as_secs_f64())
}

fn sample(
    control: &Control,
    controller: &impl IsA<gtk::EventController>,
    kind: InteractionKind,
    phase: InteractionPhase,
    local: [f64; 2],
) -> InteractionEvent {
    let frame = control.descriptor.borrow().frame;
    let position = controller
        .widget()
        .and_then(|widget| {
            let root = widget.root()?.dynamic_cast::<gtk::Widget>().ok()?;
            widget
                .compute_point(
                    &root,
                    &gtk::graphene::Point::new(local[0] as f32, local[1] as f32),
                )
                .map(|point| [f64::from(point.x()), f64::from(point.y())])
        })
        .unwrap_or([frame.x + local[0], frame.y + local[1]]);
    InteractionEvent {
        kind,
        phase,
        position,
        local_position: Some(local),
        timestamp: time(),
        modifiers: keyboard::modifiers(controller.current_event_state()),
        translation: None,
        velocity: None,
        value: None,
        rate: None,
        touches: vec![],
        active_touches: vec![],
    }
}

fn emit(queue: &Events, control: &Control, active: &Active, event: InteractionEvent) -> bool {
    let descriptor = control.descriptor.borrow();
    if !descriptor.accepts_interaction() || !descriptor.interaction.accepts(&event) {
        return false;
    }
    let mut held = active.0.borrow_mut();
    held.retain(|old| old.kind != event.kind);
    if (matches!(
        event.phase,
        InteractionPhase::Started | InteractionPhase::Changed
    ) || event.kind == InteractionKind::Touch && !event.active_touches.is_empty())
        && !matches!(
            event.kind,
            InteractionKind::LongPress | InteractionKind::ContextMenu
        )
    {
        held.push(event.clone());
    }
    drop(held);
    queue.push(control, EventPayload::Interaction(event));
    true
}

fn cancel(queue: &Events, control: &Control, active: &Active, kind: Option<InteractionKind>) {
    let mut held = active.0.borrow_mut();
    let mut cancelled = Vec::new();
    held.retain(|event| {
        if kind.is_none_or(|kind| kind == event.kind) {
            cancelled.push(event.clone());
            false
        } else {
            true
        }
    });
    drop(held);
    for mut event in cancelled {
        event.phase = InteractionPhase::Cancelled;
        event.timestamp = time();
        if event.kind == InteractionKind::Touch {
            event.touches = std::mem::take(&mut event.active_touches);
        }
        // A cancellation must survive controlled publication/hiding, so consumers
        // can clear their active state before a subsequent sequence starts.
        queue.enqueue(control, EventPayload::Interaction(event));
    }
}

pub(super) fn bind(widget: &gtk::Widget, queue: Rc<Events>, control: Rc<Control>) {
    bind_file_drop(widget, queue.clone(), control.clone());
    let active = Rc::new(Active::default());
    let contacts = Rc::new(RefCell::new(TouchContacts::default()));
    let touch = gtk::EventControllerLegacy::new();
    touch.set_propagation_phase(gtk::PropagationPhase::Capture);
    let (q, c, a, held) = (
        queue.clone(),
        control.clone(),
        active.clone(),
        contacts.clone(),
    );
    touch.connect_event(move |controller, native| {
        let phase = match native.event_type() {
            gdk::EventType::TouchBegin => InteractionPhase::Started,
            gdk::EventType::TouchUpdate => InteractionPhase::Changed,
            gdk::EventType::TouchEnd => InteractionPhase::Ended,
            gdk::EventType::TouchCancel => InteractionPhase::Cancelled,
            _ => return glib::Propagation::Proceed,
        };
        let sequence = native.event_sequence();
        let id = q.touch_ids.borrow().get(&sequence).copied();
        let id = match id {
            Some(id) => id,
            None if phase == InteractionPhase::Started && q.touch_ids.borrow().len() < 10 => {
                let Some(id) = q.touch_serial.get().checked_add(1) else {
                    return glib::Propagation::Proceed;
                };
                q.touch_serial.set(id);
                q.touch_ids.borrow_mut().insert(sequence.clone(), id);
                id
            }
            _ => return glib::Propagation::Proceed,
        };
        if matches!(phase, InteractionPhase::Ended | InteractionPhase::Cancelled) {
            // Retire after dispatch so ancestors and descendants see the same ID.
            let weak = Rc::downgrade(&q);
            glib::idle_add_local_once(move || {
                if let Some(q) = weak.upgrade() {
                    if q.touch_ids.borrow().get(&sequence) == Some(&id) {
                        q.touch_ids.borrow_mut().remove(&sequence);
                    }
                }
            });
        }
        let descriptor = c.descriptor.borrow();
        let enabled = descriptor.accepts_interaction()
            && descriptor.interaction.touch
            && descriptor.interaction.pointer_events == PointerEvents::Auto;
        drop(descriptor);
        if !enabled {
            held.borrow_mut().cancel();
            cancel(&q, &c, &a, Some(InteractionKind::Touch));
            return glib::Propagation::Proceed;
        }
        let local = controller
            .widget()
            .and_then(|widget| {
                let native_widget = widget.native()?;
                let (x, y) = native.position()?;
                let (dx, dy) = native_widget.surface_transform();
                native_widget
                    .upcast::<gtk::Widget>()
                    .compute_point(
                        &widget,
                        &gtk::graphene::Point::new((x - dx) as f32, (y - dy) as f32),
                    )
                    .map(|point| [f64::from(point.x()), f64::from(point.y())])
            })
            .unwrap_or([0.; 2]);
        let mut event = sample(&c, controller, InteractionKind::Touch, phase, local);
        let touch = TouchPoint {
            id,
            position: event.position,
            force: native
                .axis(gdk::AxisUse::Pressure)
                .filter(|v| v.is_finite())
                .unwrap_or(1.)
                .clamp(0., 1.),
            tilt: [0.; 2],
            kind: TouchKind::Direct,
        };
        if held.borrow_mut().apply(phase, touch.clone()) {
            event.touches = vec![touch];
            event.active_touches = held.borrow().active().to_vec();
            emit(&q, &c, &a, event);
        }
        glib::Propagation::Proceed
    });
    widget.add_controller(touch);
    let motion = gtk::EventControllerMotion::new();
    let point = Rc::new(Cell::new([0.; 2]));
    let (q, c, a, p) = (
        queue.clone(),
        control.clone(),
        active.clone(),
        point.clone(),
    );
    motion.connect_enter(move |motion, x, y| {
        p.set([x, y]);
        if c.descriptor.borrow().accepts_interaction()
            && matches!(c.descriptor.borrow().content, NodeContent::Button { .. })
        {
            q.push(&c, EventPayload::Hover { inside: true });
        }
        emit(
            &q,
            &c,
            &a,
            sample(
                &c,
                motion,
                InteractionKind::Hover,
                InteractionPhase::Started,
                [x, y],
            ),
        );
    });
    let (q, c, a, p) = (
        queue.clone(),
        control.clone(),
        active.clone(),
        point.clone(),
    );
    motion.connect_motion(move |motion, x, y| {
        p.set([x, y]);
        if c.descriptor.borrow().accepts_interaction() {
            let event = sample(
                &c,
                motion,
                InteractionKind::Hover,
                InteractionPhase::Changed,
                [x, y],
            );
            if matches!(c.descriptor.borrow().content, NodeContent::Button { .. }) {
                q.push(
                    &c,
                    EventPayload::PointerMove {
                        x: event.position[0],
                        y: event.position[1],
                    },
                );
            }
            emit(&q, &c, &a, event);
        }
    });
    let (q, c, a, p) = (
        queue.clone(),
        control.clone(),
        active.clone(),
        point.clone(),
    );
    motion.connect_leave(move |motion| {
        if c.descriptor.borrow().accepts_interaction()
            && matches!(c.descriptor.borrow().content, NodeContent::Button { .. })
        {
            q.push(&c, EventPayload::Hover { inside: false });
        }
        emit(
            &q,
            &c,
            &a,
            sample(
                &c,
                motion,
                InteractionKind::Hover,
                InteractionPhase::Ended,
                p.get(),
            ),
        );
    });
    widget.add_controller(motion);

    let click = gtk::GestureClick::new();
    click.set_button(0);
    let (q, c, a) = (queue.clone(), control.clone(), active.clone());
    click.connect_pressed(move |gesture, _, x, y| {
        let kind = match gesture.current_button() {
            1 => InteractionKind::Press,
            3 => InteractionKind::ContextMenu,
            _ => return,
        };
        emit(
            &q,
            &c,
            &a,
            sample(&c, gesture, kind, InteractionPhase::Started, [x, y]),
        );
    });
    let (q, c, a) = (queue.clone(), control.clone(), active.clone());
    click.connect_released(move |gesture, _, x, y| {
        if gesture.current_button() != 1
            || !a
                .0
                .borrow()
                .iter()
                .any(|e| e.kind == InteractionKind::Press)
        {
            return;
        }
        let inside = gesture.widget().is_some_and(|w| {
            x >= 0. && y >= 0. && x < f64::from(w.width()) && y < f64::from(w.height())
        });
        if inside {
            emit(
                &q,
                &c,
                &a,
                sample(
                    &c,
                    gesture,
                    InteractionKind::Press,
                    InteractionPhase::Ended,
                    [x, y],
                ),
            );
        } else {
            cancel(&q, &c, &a, Some(InteractionKind::Press));
        }
    });
    let (q, c, a) = (queue.clone(), control.clone(), active.clone());
    click.connect_cancel(move |_, _| cancel(&q, &c, &a, Some(InteractionKind::Press)));
    let (q, c, a) = (queue.clone(), control.clone(), active.clone());
    click.connect_stopped(move |_| cancel(&q, &c, &a, Some(InteractionKind::Press)));
    widget.add_controller(click);

    let long = gtk::GestureLongPress::new();
    let (q, c, a) = (queue.clone(), control.clone(), active.clone());
    long.connect_pressed(move |gesture, x, y| {
        if c.descriptor
            .borrow()
            .interaction
            .long_press_millis
            .is_none()
        {
            return;
        }
        cancel(&q, &c, &a, Some(InteractionKind::Press));
        if emit(
            &q,
            &c,
            &a,
            sample(
                &c,
                gesture,
                InteractionKind::LongPress,
                InteractionPhase::Started,
                [x, y],
            ),
        ) {
            gesture.set_state(gtk::EventSequenceState::Claimed);
        }
    });
    // Read the current policy for each sequence, including changes made after creation.
    let c = control.clone();
    long.connect_begin(move |gesture, _| {
        let millis = c.descriptor.borrow().interaction.long_press_millis;
        if let Some(millis) = millis {
            let native =
                gtk::Settings::default().map_or(500, |s| s.property::<u32>("gtk-long-press-time"));
            gesture.set_delay_factor(f64::from(millis) / f64::from(native.max(1)));
        } else {
            gesture.set_state(gtk::EventSequenceState::Denied);
        }
    });
    widget.add_controller(long);

    let drag = gtk::GestureDrag::new();
    let start = Rc::new(Cell::new([0.; 2]));
    let last = Rc::new(Cell::new(([0.; 2], 0.)));
    let (s, l) = (start.clone(), last.clone());
    drag.connect_drag_begin(move |_, x, y| {
        s.set([x, y]);
        l.set(([0.; 2], time()));
    });
    let (q, c, a, s, l) = (
        queue.clone(),
        control.clone(),
        active.clone(),
        start.clone(),
        last.clone(),
    );
    drag.connect_drag_update(move |gesture, x, y| {
        let policy = c.descriptor.borrow().interaction.clone();
        let Some(axis) = policy.pan else { return };
        let running = a.0.borrow().iter().any(|e| e.kind == InteractionKind::Pan);
        if !running {
            match policy
                .pan_activation
                .unwrap_or_default()
                .decide(axis, [x, y])
            {
                PanDecision::Wait => return,
                PanDecision::Fail => {
                    gesture.set_state(gtk::EventSequenceState::Denied);
                    return;
                }
                PanDecision::Activate => {}
            }
        }
        let now = time();
        let (previous, then) = l.replace(([x, y], now));
        let phase = if running {
            InteractionPhase::Changed
        } else {
            InteractionPhase::Started
        };
        let mut event = sample(
            &c,
            gesture,
            InteractionKind::Pan,
            phase,
            [s.get()[0] + x, s.get()[1] + y],
        );
        event.translation = Some([x, y]);
        event.velocity = Some(velocity(previous, [x, y], now - then));
        if !running {
            cancel(&q, &c, &a, Some(InteractionKind::Press));
        }
        if emit(&q, &c, &a, event) {
            gesture.set_state(gtk::EventSequenceState::Claimed);
        }
    });
    let (q, c, a, s) = (
        queue.clone(),
        control.clone(),
        active.clone(),
        start.clone(),
    );
    drag.connect_drag_end(move |gesture, x, y| {
        if !a.0.borrow().iter().any(|e| e.kind == InteractionKind::Pan) {
            return;
        }
        let mut event = sample(
            &c,
            gesture,
            InteractionKind::Pan,
            InteractionPhase::Ended,
            [s.get()[0] + x, s.get()[1] + y],
        );
        event.translation = Some([x, y]);
        event.velocity = Some([0.; 2]);
        emit(&q, &c, &a, event);
    });
    let (q, c, a) = (queue.clone(), control.clone(), active.clone());
    drag.connect_cancel(move |_, _| cancel(&q, &c, &a, Some(InteractionKind::Pan)));
    widget.add_controller(drag);

    let zoom = gtk::GestureZoom::new();
    continuous(
        &zoom,
        InteractionKind::Pinch,
        queue.clone(),
        control.clone(),
        active.clone(),
    );
    let (q, c, a) = (queue.clone(), control.clone(), active.clone());
    zoom.connect_scale_changed(move |gesture, scale| {
        changed(gesture, InteractionKind::Pinch, scale, &q, &c, &a)
    });
    widget.add_controller(zoom);
    let rotate = gtk::GestureRotate::new();
    continuous(
        &rotate,
        InteractionKind::Rotation,
        queue.clone(),
        control.clone(),
        active.clone(),
    );
    let (q, c, a) = (queue.clone(), control.clone(), active.clone());
    rotate.connect_angle_changed(move |gesture, _, delta| {
        changed(gesture, InteractionKind::Rotation, delta, &q, &c, &a)
    });
    widget.add_controller(rotate);
    let (q, c, a) = (queue, control, active);
    widget.connect_unmap(move |_| {
        let retired = contacts.borrow_mut().cancel();
        q.touch_ids
            .borrow_mut()
            .retain(|_, id| !retired.iter().any(|contact| contact.id == *id));
        cancel(&q, &c, &a, None);
    });
}

fn velocity(previous: [f64; 2], current: [f64; 2], seconds: f64) -> [f64; 2] {
    if seconds <= 0. {
        [0.; 2]
    } else {
        [0, 1].map(|i| ((current[i] - previous[i]) / seconds).clamp(-1e6, 1e6))
    }
}

fn continuous(
    gesture: &impl IsA<gtk::Gesture>,
    kind: InteractionKind,
    queue: Rc<Events>,
    control: Rc<Control>,
    active: Rc<Active>,
) {
    let (q, c, a) = (queue.clone(), control.clone(), active.clone());
    gesture.connect_begin(move |gesture, _| {
        let (x, y) = gesture.bounding_box_center().unwrap_or_default();
        let mut event = sample(
            &c,
            gesture.as_ref(),
            kind,
            InteractionPhase::Started,
            [x, y],
        );
        event.value = Some(if kind == InteractionKind::Pinch {
            1.
        } else {
            0.
        });
        event.rate = Some(0.);
        emit(&q, &c, &a, event);
    });
    let (q, c, a) = (queue.clone(), control.clone(), active.clone());
    gesture.connect_end(move |_, _| {
        let last = a.0.borrow().iter().find(|e| e.kind == kind).cloned();
        if let Some(mut event) = last {
            event.phase = InteractionPhase::Ended;
            event.timestamp = time();
            event.rate = Some(0.);
            emit(&q, &c, &a, event);
        }
    });
    gesture.connect_cancel(move |_, _| cancel(&queue, &control, &active, Some(kind)));
}

fn changed(
    gesture: &impl IsA<gtk::Gesture>,
    kind: InteractionKind,
    value: f64,
    queue: &Events,
    control: &Control,
    active: &Active,
) {
    let last = active.0.borrow().iter().find(|e| e.kind == kind).cloned();
    let Some(last) = last else { return };
    let (x, y) = gesture.bounding_box_center().unwrap_or_default();
    let mut event = sample(
        control,
        gesture.as_ref(),
        kind,
        InteractionPhase::Changed,
        [x, y],
    );
    event.value = Some(value);
    event.rate = Some(
        ((value - last.value.unwrap_or(value)) / (event.timestamp - last.timestamp).max(1e-6))
            .clamp(-1e6, 1e6),
    );
    emit(queue, control, active, event);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pointer_velocity_is_finite_and_bounded() {
        assert_eq!(velocity([0.; 2], [2., -4.], 0.5), [4., -8.]);
        assert_eq!(velocity([0.; 2], [1.; 2], 0.), [0.; 2]);
        assert_eq!(velocity([0.; 2], [1e6; 2], 1e-6), [1e6; 2]);
    }
}

fn bind_file_drop(widget: &gtk::Widget, queue: Rc<Events>, control: Rc<Control>) {
    let target = gtk::DropTarget::new(gdk::FileList::static_type(), gdk::DragAction::COPY);
    target.set_preload(true);
    let position = Rc::new(Cell::new(None::<[f64; 2]>));
    let send = Rc::new(
        move |controller: &gtk::DropTarget,
              phase: FileDropPhase,
              local: [f64; 2],
              value: Option<&glib::Value>| {
            let descriptor = control.descriptor.borrow();
            if !descriptor.accepts_interaction()
                || !descriptor.interaction.file_drop
                || descriptor.interaction.pointer_events != PointerEvents::Auto
            {
                return false;
            }
            drop(descriptor);
            let uris: Vec<String> = match value {
                Some(value) => match value.get::<gdk::FileList>() {
                    Ok(files) => {
                        let files = files.files();
                        if files.len() > 64 {
                            return false;
                        }
                        files.iter().map(|file| file.uri().to_string()).collect()
                    }
                    Err(_) => return false,
                },
                None => Vec::new(),
            };
            let point = sample(
                &control,
                controller,
                InteractionKind::Hover,
                InteractionPhase::Changed,
                local,
            )
            .position;
            let event = FileDropEvent {
                phase,
                position: point,
                uris,
            };
            if !event.is_valid() {
                return false;
            }
            queue.push(&control, EventPayload::FileDrop(event));
            true
        },
    );
    let (emit, point) = (send.clone(), position.clone());
    target.connect_enter(move |controller, x, y| {
        if emit(
            controller,
            FileDropPhase::Entered,
            [x, y],
            controller.value().as_ref(),
        ) {
            point.set(Some([x, y]));
            gdk::DragAction::COPY
        } else {
            gdk::DragAction::empty()
        }
    });
    let (emit, point) = (send.clone(), position.clone());
    target.connect_motion(move |controller, x, y| {
        if emit(
            controller,
            FileDropPhase::Moved,
            [x, y],
            controller.value().as_ref(),
        ) {
            point.set(Some([x, y]));
            gdk::DragAction::COPY
        } else {
            point.set(None);
            gdk::DragAction::empty()
        }
    });
    let (emit, point) = (send.clone(), position.clone());
    target.connect_value_notify(move |controller| {
        if let Some(local) = point.get() {
            emit(
                controller,
                FileDropPhase::Moved,
                local,
                controller.value().as_ref(),
            );
        }
    });
    let (emit, point) = (send.clone(), position.clone());
    target.connect_leave(move |controller| {
        if let Some(local) = point.take() {
            emit(controller, FileDropPhase::Left, local, None);
        }
    });
    let point = position.clone();
    target.connect_drop(move |controller, value, x, y| {
        point.set(None);
        send(controller, FileDropPhase::Dropped, [x, y], Some(value))
    });
    widget.connect_unmap(move |_| position.set(None));
    widget.add_controller(target);
}
