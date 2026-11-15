//! Title-region pointer routing; zero threshold preserves press-time native moves.
use crate::app::{pointer_capture_id, release_pointer_capture, set_pointer_capture};
use crate::core::tree::Element;
use crate::EventPhase;
use std::{
    cell::Cell,
    rc::Rc,
    sync::atomic::{AtomicUsize, Ordering},
};

pub(crate) fn drag_region_handlers(
    mut child: Element,
    threshold: [f32; 2],
    action: impl Fn(bool) + 'static,
) -> Element {
    static NEXT_DRAG_ID: AtomicUsize = AtomicUsize::new(1);
    let id = child
        .focus_id
        .get_or_insert_with(|| {
            format!(
                "title-drag-{}",
                NEXT_DRAG_ID.fetch_add(1, Ordering::Relaxed)
            )
        })
        .clone();
    let origin = Rc::new(Cell::new(None::<[f32; 2]>));
    let down = origin.clone();
    let down_id = id.clone();
    let moving = origin.clone();
    let move_id = id.clone();
    let action = Rc::new(action);
    let down_action = action.clone();
    child
        .on_mousedown(move |event| {
            if event.phase == EventPhase::Capture || event.button_index() != 0 {
                return;
            }
            down.set(None);
            event.stop_propagation();
            // Native move loops can swallow mouse-up; act on the second press.
            if event.detail.is_multiple_of(2) {
                if pointer_capture_id().as_deref() == Some(&down_id) {
                    release_pointer_capture();
                }
                down_action(true);
            } else if threshold == [0.0; 2] {
                down_action(false);
            } else {
                down.set(Some([event.screen_x, event.screen_y]));
                set_pointer_capture(down_id.clone());
            }
        })
        .on_mousemove(move |event| {
            if event.phase == EventPhase::Capture {
                return;
            }
            if let Some(start) = moving.get() {
                let owns_capture = pointer_capture_id().as_deref() == Some(&move_id);
                if event.buttons & 1 == 0 || !owns_capture {
                    moving.set(None);
                    if owns_capture {
                        release_pointer_capture();
                    }
                } else if (event.screen_x - start[0]).abs() >= threshold[0]
                    || (event.screen_y - start[1]).abs() >= threshold[1]
                {
                    moving.set(None);
                    release_pointer_capture();
                    action(false);
                }
            }
        })
        .on_mouseup(move |event| {
            if event.phase != EventPhase::Capture && event.button_index() == 0 {
                origin.set(None);
                if pointer_capture_id().as_deref() == Some(&id) {
                    release_pointer_capture();
                }
            }
        })
}
