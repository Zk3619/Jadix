//! Element-tree resident footprint counters (Wave P P.2.2).

use crate::core::tree::Element;
use crate::layout::layout_tree::{LayoutTree, LayoutTreeStats};

/// Depth-first aggregate over an [`Element`] tree.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ElementTreeStats {
    pub nodes: usize,
    pub max_depth: usize,
    pub text_bytes: usize,
    pub attr_entries: usize,
    pub handler_nodes: usize,
    pub signal_nodes: usize,
    pub image_texture_nodes: usize,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AppTreeStats {
    pub element: ElementTreeStats,
    pub layout: LayoutTreeStats,
}

impl AppTreeStats {
    pub fn from_app(root: &Element, layout: &LayoutTree) -> Self {
        Self {
            element: count_element_tree(root),
            layout: layout.stats(),
        }
    }

    pub fn format_line(&self) -> String {
        let e = &self.element;
        let l = &self.layout;
        format!(
            "elements={} depth={} text_bytes={} attrs={} handlers={} signals={} images={} | taffy={} text_measure={} root={}",
            e.nodes,
            e.max_depth,
            e.text_bytes,
            e.attr_entries,
            e.handler_nodes,
            e.signal_nodes,
            e.image_texture_nodes,
            l.taffy_nodes,
            l.text_measurements,
            l.has_root as u8,
        )
    }
}

pub fn count_element_tree(root: &Element) -> ElementTreeStats {
    let mut stats = ElementTreeStats::default();
    let mut stack: Vec<(&Element, usize)> = vec![(root, 0)];
    while let Some((el, depth)) = stack.pop() {
        stats.nodes += 1;
        stats.max_depth = stats.max_depth.max(depth);
        if let Some(ref t) = el.text {
            stats.text_bytes += t.len();
        }
        if let Some(ref spans) = el.text_spans {
            for sp in spans {
                stats.text_bytes += sp.text.len();
            }
        }
        if let Some(ref sfx) = el.text_shape_suffix {
            stats.text_bytes += sfx.len();
        }
        stats.attr_entries += el.attrs.len();
        if el.image_texture.is_some() {
            stats.image_texture_nodes += 1;
        }
        if element_has_handlers(el) {
            stats.handler_nodes += 1;
        }
        if element_has_signals(el) {
            stats.signal_nodes += 1;
        }
        for child in el.children.iter().rev() {
            stack.push((child, depth + 1));
        }
    }
    stats
}

#[inline]
fn element_has_handlers(el: &Element) -> bool {
    el.on_click.is_some()
        || el.on_hover.is_some()
        || el.on_mousedown.is_some()
        || el.on_mouseup.is_some()
        || el.on_context_menu.is_some()
        || el.on_double_click.is_some()
        || el.on_mousemove.is_some()
        || el.on_mouseover.is_some()
        || el.on_mouseout.is_some()
        || el.on_key_down.is_some()
        || el.on_unmount.is_some()
        || el.reactive_sync.is_some()
}

#[inline]
fn element_has_signals(el: &Element) -> bool {
    el.switch_checked_sync.is_some()
        || el.checkbox_sync.is_some()
        || el.radio_selected_sync.is_some()
}

pub fn process_private_mb() -> f64 {
    crate::platform::process_private_mb()
}

pub fn tree_stats_enabled() -> bool {
    matches!(
        std::env::var("RUI_TREE_STATS").as_deref(),
        Ok("1") | Ok("true") | Ok("TRUE")
    )
}

pub fn emit_tree_stats(root: &Element, layout: &LayoutTree) {
    if !tree_stats_enabled() {
        return;
    }
    let stats = AppTreeStats::from_app(root, layout);
    #[cfg(windows)]
    {
        log::info!(
            target: "rui::tree_stats",
            "private={:.1} MiB {}",
            process_private_mb(),
            stats.format_line()
        );
    }
    #[cfg(not(windows))]
    {
        log::info!(target: "rui::tree_stats", "{}", stats.format_line());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::tree::Element;

    #[test]
    fn count_small_tree() {
        let mut root = Element::new().w("100%").h("100%");
        root.children.push(Element::new().text("hi"));
        let mut leaf = Element::new();
        leaf.set_attr("role", "button");
        leaf.on_click = Some(std::rc::Rc::new(std::cell::RefCell::new(|| {})));
        root.children.push(leaf);

        let s = count_element_tree(&root);
        assert_eq!(s.nodes, 3);
        assert_eq!(s.max_depth, 1);
        assert_eq!(s.text_bytes, 2);
        assert_eq!(s.attr_entries, 1);
        assert_eq!(s.handler_nodes, 1);
    }

    #[test]
    fn layout_stats_tracks_taffy_nodes() {
        let mut root = Element::new().w("200px").h("200px");
        root.children.push(Element::new().w("50px").h("50px"));
        let mut layout = LayoutTree::new();
        layout.build_and_layout(&mut root, 200.0, 200.0, [0.0, 0.0]);
        let s = layout.stats();
        let e = count_element_tree(&root);
        assert!(s.has_root);
        assert!(s.taffy_nodes >= 2);
        assert_eq!(s.taffy_nodes, e.nodes);
        assert!(s.text_measurements <= s.taffy_nodes);
    }
}
