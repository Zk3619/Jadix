//! Measured variable-height virtualization shared by DocumentView and DocumentEditor.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;

use crate::components::scroll_area::scroll_area;
use crate::core::reactivity::use_signal;
use crate::core::scroll_viewport::{
    scroll_offset_y_signal_key, scroll_target_y_signal_key, scroll_viewport_h_signal_key,
    scroll_viewport_w_signal_key,
};
use crate::core::tree::{Element, LayoutStatus};
use crate::text::{Document, DocumentBlockKind, DocumentHeightIndex};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ActiveSource {
    pub range: Range<usize>,
    pub mount: u64,
}

#[derive(Clone)]
pub(crate) struct DocumentSurface {
    pub document: Document,
    pub active: Option<ActiveSource>,
}

pub(crate) type SurfaceSource = Rc<dyn Fn() -> DocumentSurface>;
pub(crate) type BlockRenderer = Rc<dyn Fn(&DocumentSurface, usize, f32, bool) -> Element>;

#[derive(Clone)]
struct Item {
    block: usize,
    key: u64,
    active: bool,
}

#[derive(Default)]
struct ViewportState {
    revision: u64,
    active: Option<ActiveSource>,
    items: Vec<Item>,
    heights: DocumentHeightIndex,
    measured: HashMap<u64, f64>,
    width: f32,
    range: Range<usize>,
}

fn estimate(surface: &DocumentSurface, item: &Item, width: f32) -> f64 {
    if item.active {
        let active = surface.active.as_ref().unwrap();
        let text = &surface.document.source().text()[active.range.clone()];
        return (text.lines().count().max(1) as f64 * 24.0 + 16.0).min(1024.0);
    }
    let block = &surface.document.blocks()[item.block];
    match &block.kind {
        DocumentBlockKind::Blank => 12.0,
        DocumentBlockKind::Syntax => 8.0,
        DocumentBlockKind::Heading(level) => 52.0 - *level as f64 * 4.0,
        DocumentBlockKind::Math | DocumentBlockKind::Chemistry => 80.0,
        DocumentBlockKind::Molecule { .. } => 280.0,
        DocumentBlockKind::Rule => 16.0,
        DocumentBlockKind::TableRow { .. } => 36.0,
        DocumentBlockKind::Code { .. } => 26.0,
        DocumentBlockKind::Html => 320.0,
        _ => {
            let bytes: usize = block.inlines.iter().map(|inline| inline.text.len()).sum();
            let rows = (bytes as f32 * 8.0 / width.max(80.0)).ceil().max(1.0);
            f64::from(rows * 24.0 + 4.0)
        }
    }
}

fn project(surface: &DocumentSurface) -> Vec<Item> {
    let mut items = Vec::with_capacity(surface.document.blocks().len());
    let mut active_emitted = false;
    let owner = surface
        .active
        .as_ref()
        .and_then(|active| surface.document.block_at(active.range.start));
    for (position, block) in surface.document.blocks().iter().enumerate() {
        if let Some(active) = &surface.active {
            let owns_caret = owner == Some(position);
            let intersects =
                block.source.start < active.range.end && block.source.end > active.range.start;
            if owns_caret || intersects {
                if !active_emitted {
                    items.push(Item {
                        block: position,
                        key: u64::MAX - active.mount,
                        active: true,
                    });
                    active_emitted = true;
                }
                continue;
            }
        }
        if crate::components::document_view::is_table_separator(&surface.document, block) {
            continue;
        }
        items.push(Item {
            block: position,
            key: block.id,
            active: false,
        });
    }
    items
}

fn spacer(height: f64, role: &str) -> Element {
    let mut element = Element::new()
        .w("100%")
        .h(height as f32)
        .min_h(height as f32)
        .flex_shrink(0.0);
    element.set_attr("data-document-spacer", role);
    element
}

fn mount_window(
    body: &mut Element,
    state: &mut ViewportState,
    surface: &DocumentSurface,
    render: &BlockRenderer,
    range: Range<usize>,
) {
    let mut previous: HashMap<u64, Element> = std::mem::take(&mut body.children)
        .into_iter()
        .filter_map(|element| {
            element
                .attrs
                .get("data-document-key")
                .and_then(|key| key.parse::<u64>().ok())
                .map(|key| (key, element))
        })
        .collect();
    let next_keys: std::collections::HashSet<_> = state.items[range.clone()]
        .iter()
        .map(|item| item.key)
        .collect();
    previous.retain(|key, _| next_keys.contains(key));
    let mut children = Vec::with_capacity(range.len() + 2);
    let top = state.heights.top(range.start);
    if top > 0.0 {
        children.push(spacer(top, "top"));
    }
    for position in range.clone() {
        let item = &state.items[position];
        let mut element = previous
            .remove(&item.key)
            .unwrap_or_else(|| render(surface, item.block, state.width, item.active));
        element = element.w("100%").flex_shrink(0.0);
        element.set_attr("data-document-key", item.key.to_string());
        element.set_attr("data-document-index", position.to_string());
        element.set_attr(
            "data-source-start",
            surface.document.blocks()[item.block]
                .source
                .start
                .to_string(),
        );
        element.set_attr(
            "data-source-end",
            surface.document.blocks()[item.block].source.end.to_string(),
        );
        children.push(element);
    }
    let bottom = state.heights.total() - state.heights.top(range.end);
    if bottom > 0.0 {
        children.push(spacer(bottom, "bottom"));
    }
    body.children = children;
    *body = std::mem::take(body)
        .h(state.heights.total() as f32)
        .min_h(state.heights.total() as f32);
    body.set_attr("data-document-materialized", range.len().to_string());
    body.set_attr("data-document-count", state.items.len().to_string());
    state.range = range;
}

pub(crate) fn document_viewport(
    id: &str,
    viewport_h: f32,
    overscan: usize,
    source: SurfaceSource,
    render: BlockRenderer,
    before_sync: Option<Rc<dyn Fn()>>,
) -> Element {
    let offset = use_signal(&scroll_offset_y_signal_key(id), || 0.0_f32);
    let target = use_signal(&scroll_target_y_signal_key(id), || 0.0_f32);
    let viewport = use_signal(&scroll_viewport_h_signal_key(id), || viewport_h);
    let width = use_signal(&scroll_viewport_w_signal_key(id), || 640.0_f32);
    let initial = source();
    let items = project(&initial);
    let heights =
        DocumentHeightIndex::new(items.iter().map(|item| estimate(&initial, item, 640.0)));
    let mut state = ViewportState {
        revision: initial.document.revision(),
        active: initial.active.clone(),
        items,
        heights,
        width: 640.0,
        ..Default::default()
    };
    let range = state
        .heights
        .visible(offset.peek() as f64, viewport_h as f64, overscan);
    let mut body = Element::new().flex_col().w("100%").flex_shrink(0.0);
    mount_window(&mut body, &mut state, &initial, &render, range);
    let state = Rc::new(RefCell::new(state));
    let body = body.reactive_sync(move |body, _layout| {
        if let Some(before_sync) = &before_sync {
            before_sync();
        }
        let surface = source();
        let mut state = state.borrow_mut();
        let viewport = viewport.peek().max(1.0);
        let width = width.peek().max(80.0);
        let width_changed = (width - state.width).abs() > 1.0;
        let changed = surface.document.revision() != state.revision
            || surface.active != state.active
            || width_changed;
        let old_offset = offset.peek().max(0.0) as f64;
        let anchor = state.heights.anchor(old_offset);
        let anchor_key = state.items.get(anchor.block).map(|item| item.key);
        let mut measured = false;
        if !width_changed {
            for element in &body.children {
                if let Some(position) = element
                    .attrs
                    .get("data-document-index")
                    .and_then(|index| index.parse::<usize>().ok())
                {
                    if let Some(item) = state.items.get(position) {
                        let key = item.key;
                        let height = f64::from(element.layout_bounds[3]);
                        if height > 0.0 {
                            measured |= state.heights.measure(position, height);
                            state.measured.insert(key, height);
                        }
                    }
                }
            }
        }
        let active_changed = surface.active != state.active;
        if changed {
            if width_changed {
                state.measured.clear();
            }
            state.items = project(&surface);
            state.heights = DocumentHeightIndex::new(state.items.iter().map(|item| {
                state
                    .measured
                    .get(&item.key)
                    .copied()
                    .unwrap_or_else(|| estimate(&surface, item, width))
            }));
            state.revision = surface.document.revision();
            state.active = surface.active.clone();
            state.width = width;
            let live_keys: std::collections::HashSet<_> =
                state.items.iter().map(|item| item.key).collect();
            state.measured.retain(|key, _| live_keys.contains(key));
            if width_changed {
                body.children.clear();
            }
        }
        let anchor_position = if changed {
            anchor_key.and_then(|key| state.items.iter().position(|item| item.key == key))
        } else {
            Some(anchor.block)
        };
        let mut next_offset = if changed || measured {
            state.heights.restore(crate::text::DocumentScrollAnchor {
                block: anchor_position.unwrap_or(anchor.block),
                offset: anchor.offset,
            })
        } else {
            old_offset
        };
        if active_changed {
            if let Some(position) = state.items.iter().position(|item| item.active) {
                let top = state.heights.top(position);
                let bottom = state.heights.top(position + 1);
                if top < next_offset || bottom - top > f64::from(viewport) {
                    next_offset = top;
                } else if bottom > next_offset + f64::from(viewport) {
                    next_offset = bottom - f64::from(viewport);
                }
            }
        }
        next_offset =
            next_offset.clamp(0.0, (state.heights.total() - f64::from(viewport)).max(0.0));
        if (next_offset - old_offset).abs() > 0.5 {
            offset.set_silent(next_offset as f32);
            target.set_silent(next_offset as f32);
        }
        let range = state
            .heights
            .visible(next_offset, f64::from(viewport), overscan);
        if changed || measured || range != state.range {
            mount_window(body, &mut state, &surface, &render, range);
            return LayoutStatus::StructureChanged;
        }
        LayoutStatus::Clean
    });
    scroll_area(id)
        .overflow_y("auto")
        .overflow_x("auto")
        .w("100%")
        .h(viewport_h.max(1.0))
        .child(body)
        .into()
}
