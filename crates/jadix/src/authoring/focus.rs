//! Focus policies that custom controls opt into independently of component names.

use crate::core::tree::{find_in_tree, Element};

/// Blur the focused control when a primary pointer press lands outside its tree or paint box.
pub const ATTR_BLUR_ON_OUTSIDE_PRESS: &str = "jadix_blur_on_outside_press";

pub fn blur_on_outside_press(root: &Element, focused: &str) -> bool {
    !focused.is_empty()
        && find_in_tree(root, |node| node.focus_id.as_deref() == Some(focused)).is_some_and(
            |node| {
                node.attrs
                    .get(ATTR_BLUR_ON_OUTSIDE_PRESS)
                    .map(String::as_str)
                    == Some("true")
            },
        )
}
