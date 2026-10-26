//! Node-owned icon playback supplied by an icon library.

use crate::core::tree::Element;

#[derive(Clone, Copy, Debug)]
pub struct Playback {
    pub tick: fn(&mut Element, f32) -> bool,
    pub needs_frame: fn(&Element) -> bool,
    pub texture: fn(&Element, f32, [u8; 4], f32) -> Option<(String, f32)>,
    pub hover_texture: fn(&Element, f32, [u8; 4], f32) -> Option<(String, f32)>,
    pub set_state: fn(&mut Element, &str),
    pub reset: fn(&Element),
    pub detach: fn(&mut Element),
    pub controlled: bool,
}

pub fn tick_tree(root: &mut Element, dt: f32) -> bool {
    let mut active = false;
    crate::core::tree::walk_elements_mut_pruned(root, |node| {
        if node.collect_paint_hash != 0 && !node.collect_skip_live {
            return false;
        }
        if let Some(playback) = node.icon_playback {
            active |= (playback.tick)(node, dt);
        }
        true
    });
    active
}

pub fn needs_frame(root: &Element) -> bool {
    let mut active = false;
    crate::core::tree::walk_elements_pruned(root, |node| {
        if active || (node.collect_paint_hash != 0 && !node.collect_skip_live) {
            return false;
        }
        if let Some(playback) = node.icon_playback {
            active |= (playback.needs_frame)(node);
        }
        !active
    });
    active
}

pub fn reset(element: &Element) {
    if let Some(playback) = element.icon_playback {
        (playback.reset)(element);
    }
}
