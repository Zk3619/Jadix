use super::runtime::{
    self, CollectGeometry, DebugRect, DragKey, DragRuntime, MoveMode, RuntimeState, Update,
};
use super::session;
use crate::core::tree::Element;
use std::rc::Rc;

struct SessionRuntime;

crate::authoring::window::window_local! {
    static SESSION_RUNTIME: Rc<dyn DragRuntime> = Rc::new(SessionRuntime);
}

pub(super) fn install() {
    SESSION_RUNTIME.with(|runtime| runtime::register(runtime.clone()));
}

impl DragRuntime for SessionRuntime {
    fn state(&self) -> RuntimeState {
        RuntimeState {
            dragging: session::is_dragging(),
            active: session::is_active(),
            coasting: session::is_coasting(),
            activation_deadline: session::activation_next_wake_at(),
            pointer: session::last_pointer(),
        }
    }

    fn prepare_move(
        &self,
        root: Option<&Element>,
        pointer: [f32; 2],
        viewport: [f32; 2],
        mode: MoveMode,
    ) -> Update {
        let hits = if session::is_dragging() && !(mode == MoveMode::Follow && session::is_active())
        {
            root.map(session::collect_hits).unwrap_or_default()
        } else {
            Vec::new()
        };
        session::guard_update(Box::new(move || {
            if mode == MoveMode::Follow && session::is_active() {
                session::tick_follow(pointer[0], pointer[1], viewport)
            } else {
                session::tick(pointer[0], pointer[1], &hits, viewport)
            }
        }))
    }

    fn prepare_end(&self, root: &Element) -> Update {
        let hits = if session::is_dragging() {
            session::collect_hits(root)
        } else {
            Vec::new()
        };
        session::guard_update(Box::new(move || session::end(&hits)))
    }

    fn refresh_geometry(&self, root: &Element) {
        session::refresh_hit_cache(root);
    }

    fn prepare_autoscroll(&self, root: &Element, viewport: [f32; 2]) -> Update {
        session::prepare_autoscroll(root, viewport)
    }

    fn prepare_inertia(&self, root: &Element, viewport: [f32; 2]) -> Update {
        session::prepare_inertia(root, viewport)
    }

    fn key(&self, key: DragKey) -> bool {
        match key {
            DragKey::Cancel => session::cancel(),
            DragKey::Next => session::is_active() && session::nudge_insert(1),
            DragKey::Previous => session::is_active() && session::nudge_insert(-1),
        }
    }

    fn sync_tree(&self, root: &mut Element) {
        if session::is_active() {
            super::overlay::sync_overlay_ghost_insets(root);
        }
    }

    fn after_layout(&self, root: &mut Element) {
        if session::is_active() {
            session::resync_overlay_follow_after_layout(root);
            super::overlay::sync_overlay_ghost_insets(root);
        }
    }

    fn keeps_collect_live(&self, element: &Element) -> bool {
        element.attrs.contains_key(super::ATTR_DRAG_OVERLAY)
    }

    fn collect_geometry(
        &self,
        element: &mut Element,
        size: [f32; 2],
        scale: [f32; 2],
    ) -> CollectGeometry {
        let active = session::is_active();
        let size = if active && element.attrs.contains_key(super::ATTR_DRAG_OVERLAY_CLIP) {
            [size[0] / scale[0], size[1] / scale[1]]
        } else {
            size
        };
        let origin = if active {
            super::overlay::adjust_overlay_collect_origin(element, size)
        } else {
            None
        };
        CollectGeometry { size, origin }
    }

    fn ancestor_clip(&self, element: &Element) -> Option<[f32; 4]> {
        element
            .attrs
            .contains_key(super::ATTR_DRAG_OVERLAY_CLIP)
            .then(session::peek_restrict_ancestor_clip)
            .flatten()
    }

    fn debug_rects(&self, root: &Element) -> Vec<DebugRect> {
        self.debug_rects_with_clips(root)
            .into_iter()
            .map(|(rect, _)| rect)
            .collect()
    }

    fn debug_rects_with_clips(&self, root: &Element) -> Vec<(DebugRect, Option<[f32; 4]>)> {
        super::debug::debug_frames_for_tree(root)
            .into_iter()
            .filter_map(|frame| {
                let stroke = frame.stroke();
                let clip = session::debug_slot_clip(&frame.scope_id, &frame.id);
                let debug_rect = session::last_draggable_bounds(&frame.scope_id, &frame.id)
                    .unwrap_or(frame.rect);
                if clip.is_some_and(|clip| {
                    crate::core::geometry::Rect::from_bounds(clip)
                        .overlap_fraction_in(crate::core::geometry::Rect::from_bounds(debug_rect))
                        <= 0.0
                }) {
                    return None;
                }
                Some((
                    DebugRect {
                        rect: debug_rect,
                        fill: stroke.fill,
                        border: stroke.border,
                        width: stroke.width,
                        dashed: stroke.dashed,
                    },
                    clip,
                ))
            })
            .collect()
    }
}
