//! Measured counterpart of the uniform list; both use the shared core geometry.
use super::*;
use jadix_core::virtualization::VirtualAxis;
use std::cell::RefCell;

pub(super) fn build(builder: VirtualListBuilder) -> Element {
    let VirtualListBuilder { state, count, extent, gap, viewport, height, width,
        axis, order, retain_on_move, measure_rows, data_revision, follow_end,
        overscan, predict, smooth, key, render, focus, visible_range, mut element } = builder;
    UniformList::new(0, extent, gap).expect("invalid virtual list geometry");
    let count: Rc<dyn Fn() -> usize> = Rc::from(count);
    let key: Rc<dyn Fn(usize) -> String> = Rc::from(key);
    let offset = state.offset;
    let view = Rc::new(Cell::new(state.viewport.get().unwrap_or_else(|| {
        (if axis == Axis::Row { width } else { height }).map(f64::from).unwrap_or(extent)
    })));
    let cross = Rc::new(Cell::new(None));
    let cross_changed = Rc::new(Cell::new(false));
    let measurements = Rc::new(RefCell::new(BTreeMap::<String, f64>::new()));
    let geometry = Rc::new(RefCell::new(VirtualAxis::<String>::estimated([], extent).unwrap()));
    let coordinates = move |value| if axis == Axis::Row { [value, 0.] } else { [0., value] };
    if offset.get() > 0. { element.descriptor.style.scroll_to = Some(coordinates(offset.get())); }
    let manual = Rc::new(Cell::new(false));
    let position = offset.clone();
    let did_scroll = manual.clone();
    element = element.on_scroll(move |x, y| {
        let next = (if axis == Axis::Row { x } else { y }).max(0.);
        if (next - position.get()).abs() > 0.01 { did_scroll.set(true); }
        position.set(next);
    });
    let native_view = view.clone();
    element = element.on_viewport(move |w, h| {
        let size = if axis == Axis::Row { w } else { h };
        native_view.set(size); state.viewport.set(Some(size));
    });
    if measure_rows {
        let resized = cross_changed.clone();
        let pending = measurements.clone();
        element = element.on_layout(move |frame| {
            let next = if axis == Axis::Row { frame.height } else { frame.width };
            let changed = cross.replace(Some(next)).is_some_and(|old: f64| (old-next).abs() > 0.01);
            if changed { resized.set(true); pending.borrow_mut().clear(); }
            changed
        });
    }
    let index = geometry.clone();
    let measured = measurements.clone();
    let view_size = view.clone();
    let position = offset.clone();
    let keys = key.clone();
    let item_count = count.clone();
    let row_resize = cross_changed.clone();
    let previous_revision = Cell::new(None);
    let previous_focus = Cell::new(None);
    // Retain the old anchor before any batch of measurements or dataset edits.
    // Applying all changes together prevents order-dependent compensation.
    element.descriptor_binding = Some(Rc::new(move |descriptor| {
        if manual.replace(false) { descriptor.style.scroll_to = None; }
        let count = item_count();
        let revision = (count, data_revision.map(|revision| revision.get()).unwrap_or(0));
        let viewport = viewport.map(|v| v.get()).unwrap_or_else(|| view_size.get());
        let mut geometry = index.borrow_mut();
        let anchor = geometry.anchor(position.get()).unwrap();
        let at_end = position.get() + viewport >= geometry.total() - 1.;
        let mut changed = false;
        if previous_revision.replace(Some(revision)) != Some(revision) {
            geometry.replace((0..count).map(|visual| keys(order.index(visual, count).unwrap())), extent)
                .expect("invalid virtual list keys");
            geometry.set_gap(gap).unwrap();
            changed = true;
        }
        if cross_changed.replace(false) {
            let keys: Vec<_> = (0..geometry.len()).map(|i| geometry.key(i).unwrap().clone()).collect();
            let mut fresh = VirtualAxis::estimated(keys, extent).unwrap(); fresh.set_gap(gap).unwrap();
            *geometry = fresh; changed = true;
        }
        for (key, size) in std::mem::take(&mut *measured.borrow_mut()) {
            if geometry.index(&key).is_some() { changed |= geometry.measure(&key, size).unwrap(); }
        }
        if measure_rows { changed |= geometry.estimate_from_measurements().unwrap(); }
        assert!(geometry.total() <= f32::MAX as f64, "virtual list extent exceeds layout coordinates");
        if changed {
            let target = if follow_end && at_end { (geometry.total() - viewport).max(0.) }
                else { anchor.as_ref().and_then(|a| geometry.restore_anchor(a, viewport).ok())
                    .unwrap_or_else(|| position.get().min((geometry.total()-viewport).max(0.))) };
            if (target-position.get()).abs() > 0.01 {
                position.set(target);
                descriptor.style.scroll_behavior = Some(ScrollBehavior::Instant);
                descriptor.style.scroll_to = Some(coordinates(target));
            }
        }
        let next = focus.as_ref().and_then(|f| f());
        if previous_focus.replace(next) != next {
            if let Some((logical, _, align, behavior)) = next {
                if let Some(index) = order.index(logical, count) {
                    let behavior = behavior.unwrap_or(if smooth { ScrollBehavior::Smooth } else { ScrollBehavior::Instant });
                    if let Ok(target) = geometry.scroll_to_index(index, position.get(), viewport, align, 0.) {
                        if behavior == ScrollBehavior::Instant { position.set(target); }
                        descriptor.style.scroll_behavior = Some(behavior);
                        descriptor.style.scroll_to = Some(coordinates(target));
                    }
                }
            }
        }
    }));
    let row_geometry = geometry.clone();
    let mut content = Element::new().flex_shrink(0.);
    content = if axis == Axis::Row { content.flex_row() } else { content.flex_col() };
    let size_row = move |row: Element, size: f64| if axis == Axis::Row { row.w(size as f32) } else { row.h(size as f32) };
    let mut previous: Vec<(String, usize, Signal<usize>)> = Vec::new();
    let mut previous_visible = None;
    let mut ahead = ListScrollAhead::default();
    content.children_binding = Some(Box::new(move |children| {
        let count = count();
        let geometry = geometry.borrow();
        let viewport = viewport.map(|v| v.get()).unwrap_or_else(|| view.get());
        let visible = geometry.viewport_geometry(offset.get(), viewport).unwrap();
        let lead = ahead.update(visible.offset, viewport).unwrap();
        let lead = if predict { lead } else { 0. };
        let start_pixel = (visible.offset + lead.min(0.)).max(0.);
        let pre = geometry.viewport_geometry(start_pixel, visible.offset+viewport+lead.max(0.)-start_pixel).unwrap();
        let start = pre.visible.start.saturating_sub(overscan);
        let end = pre.visible.end.saturating_add(overscan).min(count);
        let old_children = std::mem::take(children);
        let mut old: BTreeMap<_, _> = previous.drain(..).zip(old_children.into_iter().skip(1))
            .map(|((key, index, live), row)| (key, (index, live, row))).collect();
        children.push(size_row(Element::new().flex_shrink(0.), geometry.position(start).unwrap()));
        for visual in start..end {
            let logical = order.index(visual, count).unwrap();
            let key = geometry.key(visual).unwrap().clone();
            let retained = old.remove(&key).filter(|(index, _, _)| retain_on_move || *index == logical);
            let (live, mut row) = if let Some((_, live, row)) = retained {
                if live.get() != logical { live.set(logical); } (live, row)
            } else {
                let measured = measurements.clone();
                let key = key.clone();
                let (owner, (live, child)) = create_scope(|| {
                    let live = create_signal(logical);
                    let child = render(live);
                    (live, child)
                });
                let mut row = Element::new().child(child).flex_shrink(0.);
                if measure_rows {
                    let index = row_geometry.clone();
                    let resized = row_resize.clone();
                    row = row.on_layout(move |frame| {
                        let size = if axis == Axis::Row { frame.width } else { frame.height };
                        if !size.is_finite() || size <= 0. { return false; }
                        let geometry = index.borrow();
                        let expected = geometry.extent(&key).unwrap_or(extent);
                        if !resized.get() && geometry.is_measured(&key) && (size-expected).abs() <= 0.01 { return false; }
                        drop(geometry);
                        measured.borrow_mut().insert(key.clone(), size); true
                    });
                } else { row = size_row(row, extent); }
                let mut wrapper = if axis == Axis::Row { Element::new().flex_row() } else { Element::new().flex_col() };
                wrapper = wrapper.flex_shrink(0.).child(row).child(Element::new().flex_shrink(0.));
                let mut row = wrapper;
                row.owned_scopes.push(owner);
                (live, row)
            };
            // Keep inter-row spacing outside the measured box.
            let spacer = row.children.last_mut().unwrap();
            let size = if visual+1<count { gap as f32 } else { 0. };
            if axis == Axis::Row { spacer.descriptor.style.width = Some(size); }
            else { spacer.descriptor.style.height = Some(size); }
            children.push(row);
            previous.push((key, logical, live));
        }
        children.push(size_row(Element::new().flex_shrink(0.), geometry.total()-geometry.position(end).unwrap()));
        drop(old);
        let logical_visible = order.range(visible.visible, count).unwrap();
        if previous_visible.as_ref() != Some(&logical_visible) {
            previous_visible = Some(logical_visible.clone());
            if let Some(callback) = &visible_range { callback(Some(logical_visible)); }
        }
    }));
    element.children.push(content); element
}
