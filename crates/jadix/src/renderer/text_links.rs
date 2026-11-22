//! Link hit geometry comes from the painted buffer, never a second text layout.

use crate::{
    app::pointer_events::{HitPathEntry, TextLinkHit},
    layout::layout_tree::TextLayoutInfo,
    style::{TextSpan, TextStyle},
};
use std::collections::HashMap;

#[derive(Default)]
pub(crate) struct TextLinkCache(HashMap<taffy::NodeId, LinkLayout>);

struct LinkLayout {
    // Scratch membership mark, cleared by every retain_nodes sweep.
    present: bool,
    // Updated with membership, so pointer moves need not scan all text nodes.
    snapshot_index: Option<usize>,
    shape_key: u64,
    text: String,
    spans: Vec<TextSpan>,
    style: TextStyle,
    size: [f32; 2],
    ui_scale: f32,
    offset: [f32; 2],
    regions: Vec<(usize, [f32; 4])>,
}

fn contains(rect: [f32; 4], point: [f32; 2]) -> bool {
    point[0] >= rect[0]
        && point[1] >= rect[1]
        && point[0] < rect[0] + rect[2]
        && point[1] < rect[1] + rect[3]
}

impl TextLinkCache {
    /// Layout/scroll can move a link under a stationary pointer. Ordinary pages
    /// keep their existing cursor fast path without an additional hit walk.
    pub(crate) fn resolve_cursor(
        &self,
        root: &crate::core::tree::Element,
        nodes: &[TextLayoutInfo],
        point: [f32; 2],
        ui_scale: f32,
        fallback: Option<crate::style::CursorValue>,
        had_link: bool,
    ) -> (Option<crate::style::CursorValue>, Option<Vec<HitPathEntry>>) {
        if self.0.is_empty() && !had_link {
            return (fallback, None);
        }
        let mut path = if point.iter().all(|v| v.is_finite()) {
            crate::app::pointer_events::hit_path(root, point[0], point[1])
        } else {
            Vec::new()
        };
        self.annotate_path(nodes, &mut path, point, ui_scale);
        (
            crate::app::pointer_events::path_resolved_cursor(&path),
            Some(path),
        )
    }

    pub(crate) fn retain_nodes(&mut self, nodes: &[TextLayoutInfo]) {
        if self.0.is_empty() {
            return;
        }
        for (index, id) in nodes
            .iter()
            .enumerate()
            .filter_map(|(index, node)| node.node_id.map(|id| (index, id)))
        {
            if let Some(layout) = self.0.get_mut(&id) {
                layout.present = true;
                layout.snapshot_index = Some(index);
            }
        }
        self.0
            .retain(|_, layout| std::mem::take(&mut layout.present));
    }

    pub(crate) fn record(
        &mut self,
        info: &TextLayoutInfo,
        buffer: &glyphon::Buffer,
        shape_key: u64,
        ui_scale: f32,
        offset: [f32; 2],
    ) {
        let Some(id) = info.node_id else { return };
        let Some(spans) = info
            .spans
            .as_ref()
            .filter(|spans| spans.iter().any(|s| s.href.is_some()))
        else {
            self.0.remove(&id);
            return;
        };
        if let Some(layout) = self.0.get_mut(&id) {
            if layout.shape_key == shape_key
                && layout.text == info.text
                && &layout.spans == spans
                && layout.style == info.style
                && layout.size == [info.width, info.height]
                && layout.ui_scale == ui_scale
            {
                layout.offset = offset;
                return;
            }
        }
        let mut regions = Vec::new();
        for run in buffer.layout_runs() {
            // Individual advances preserve disjoint visual regions in bidi text.
            for glyph in run.glyphs {
                let Some(index) = glyph.metadata.checked_sub(1) else {
                    continue;
                };
                if spans.get(index).is_some_and(|s| s.href.is_some()) && glyph.w > 0.0 {
                    regions.push((
                        index,
                        [
                            glyph.x / ui_scale,
                            run.line_top / ui_scale,
                            glyph.w / ui_scale,
                            run.line_height / ui_scale,
                        ],
                    ));
                }
            }
        }
        let snapshot_index = self.0.get(&id).and_then(|layout| layout.snapshot_index);
        self.0.insert(
            id,
            LinkLayout {
                present: false,
                snapshot_index,
                shape_key,
                text: info.text.clone(),
                spans: spans.clone(),
                style: info.style.clone(),
                size: [info.width, info.height],
                ui_scale,
                offset,
                regions,
            },
        );
    }

    fn hit(
        &self,
        info: &TextLayoutInfo,
        mut point: [f32; 2],
        ui_scale: f32,
    ) -> Option<TextLinkHit> {
        let layout = self.0.get(&info.node_id?)?;
        // Reject stale geometry after a declaration update, before the next paint.
        if layout.text != info.text
            || Some(&layout.spans) != info.spans.as_ref()
            || layout.style != info.style
            || layout.size != [info.width, info.height]
            || layout.ui_scale != ui_scale
            || info.clip_bounds.is_some_and(|clip| !contains(clip, point))
        {
            return None;
        }
        let transform = &info.paint_transform;
        let det = crate::style::transform::mat2d_det(transform);
        if !det.is_finite() || det.abs() < 1e-12 {
            return None;
        }
        let (x, y) = crate::style::transform::mat2d_transform_point(
            &crate::style::transform::mat2d_inverse(transform),
            point[0] - info.paint_origin[0],
            point[1] - info.paint_origin[1],
        );
        let scale = if info.paint_scale.is_finite() && info.paint_scale > 0.0 {
            info.paint_scale
        } else {
            1.0
        };
        point = [
            (x + info.paint_origin[0] - info.x) / scale - layout.offset[0],
            (y + info.paint_origin[1] - info.y) / scale - layout.offset[1],
        ];
        let (span_index, _) = layout
            .regions
            .iter()
            .find(|(_, rect)| contains(*rect, point))?;
        let href = layout.spans[*span_index].href.as_ref()?;
        // Raw TextSpan authors get the same destination policy as document projection.
        if !jadix_document::is_safe_document_destination(href) {
            return None;
        }
        Some(TextLinkHit {
            href: href.clone(),
            span_index: *span_index,
        })
    }

    pub(crate) fn annotate_path(
        &self,
        nodes: &[TextLayoutInfo],
        path: &mut [HitPathEntry],
        point: [f32; 2],
        ui_scale: f32,
    ) {
        let Some(leaf) = path.last_mut() else { return };
        leaf.text_link = None;
        let Some(id) = leaf.node_id else { return };
        let Some(layout) = self.0.get(&id) else {
            return;
        };
        if let Some(info) = layout
            .snapshot_index
            .and_then(|index| nodes.get(index))
            .filter(|info| info.node_id == Some(id))
            .or_else(|| nodes.iter().rev().find(|info| info.node_id == Some(id)))
        {
            leaf.text_link = self.hit(info, point, ui_scale);
        }
    }
}

#[cfg(all(test, feature = "product-tests"))]
mod tests {
    use super::*;
    use crate::{
        app::pointer_events::hit_path,
        core::tree::{Element, LayoutTree},
    };

    #[test]
    fn retention_follows_each_snapshot_without_creating_unpainted_entries() {
        let mut root = Element::new()
            .w(300.0)
            .h(200.0)
            .child(Element::new().text("first"))
            .child(Element::new().text("second"))
            .child(Element::new().text("unpainted"));
        let nodes = LayoutTree::new()
            .build_and_layout(&mut root, 300.0, 200.0, [-1.0; 2])
            .text_nodes;
        assert_eq!(nodes.len(), 3);
        let ids: Vec<_> = nodes.iter().map(|n| n.node_id.unwrap()).collect();
        let mut cache = TextLinkCache::default();
        for info in &nodes[..2] {
            cache.0.insert(
                info.node_id.unwrap(),
                LinkLayout {
                    present: false,
                    snapshot_index: None,
                    shape_key: 7,
                    text: info.text.clone(),
                    spans: Vec::new(),
                    style: info.style.clone(),
                    size: [info.width, info.height],
                    ui_scale: 1.0,
                    offset: [3.0, 4.0],
                    regions: vec![(0, [0.0, 0.0, 1.0, 1.0])],
                },
            );
        }
        for _ in 0..2 {
            cache.retain_nodes(&nodes);
            assert_eq!(cache.0.len(), 2);
            assert!(cache.0.values().all(|layout| !layout.present));
            assert!(!cache.0.contains_key(&ids[2]));
        }
        let mut anonymous = nodes[0].clone();
        anonymous.node_id = None;
        let remaining = [
            nodes[1].clone(),
            anonymous,
            nodes[1].clone(),
            nodes[2].clone(),
        ];
        cache.retain_nodes(&remaining);
        assert_eq!(cache.0.len(), 1);
        assert!(!cache.0.contains_key(&ids[0]));
        let kept = &cache.0[&ids[1]];
        assert_eq!(kept.offset, [3.0, 4.0]);
        assert_eq!(kept.regions, [(0, [0.0, 0.0, 1.0, 1.0])]);
        cache.retain_nodes(&[]);
        assert!(cache.0.is_empty());
        cache.retain_nodes(&nodes);
        assert!(
            cache.0.is_empty(),
            "membership must not resurrect stale geometry"
        );
    }

    #[test]
    #[ignore = "native GPU text preparation; run explicitly"]
    fn painted_link_regions_follow_wrapping_dpi_and_exclusive_hit_paths() {
        let gpu = crate::renderer::offscreen::OffscreenGpu::new().expect("native GPU");
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            width: 800,
            height: 800,
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 1,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            color_space: Default::default(),
            view_formats: vec![],
        };
        let renderer =
            crate::renderer::text::JadixTextRenderer::new(&gpu.device, &gpu.queue, &config);
        let mut overlay = crate::renderer::text::JadixTextRenderer::new_sharing_resources(
            &gpu.device,
            &gpu.queue,
            &config,
            &renderer,
        );
        for (width, dpi) in [(260.0, 1.0), (110.0, 1.5)] {
            let mut linked = TextSpan::new("link words wrap across lines שלום");
            linked.href = Some("https://example.com/one".into());
            // No decoration: href itself must request range metadata.
            let mut unsafe_link = TextSpan::new(" bad");
            unsafe_link.href = Some("javascript:run".into());
            let mut root = Element::new().w(width).h(400.0).child(
                Element::new().w(width).text_size(18.0).text_spans(vec![
                    TextSpan::new("plain "),
                    linked,
                    unsafe_link,
                ]),
            );
            let nodes = LayoutTree::new()
                .build_and_layout(&mut root, width, 400.0, [-1.0; 2])
                .text_nodes;
            assert_eq!(nodes.len(), 1);
            let info = &nodes[0];
            renderer.text_links.borrow_mut().retain_nodes(&nodes);
            assert!(
                renderer
                    .text_links
                    .borrow()
                    .hit(info, [2.0, 2.0], dpi)
                    .is_none(),
                "unpainted text must not activate"
            );
            crate::renderer::prepare_text::prepare_text_frame_at(
                &mut overlay,
                &gpu.device,
                &gpu.queue,
                800,
                800,
                &nodes,
                dpi,
                false,
                &Default::default(),
                &Default::default(),
            );
            // Same-device overlay paint must populate the owning window's hits.
            let buffer = &overlay.shaped_cache[0].buffer;
            assert!(
                buffer.layout_runs().count() > 1,
                "fixture must exercise wrapping"
            );
            let mut hits = Vec::new();
            for run in buffer.layout_runs() {
                for glyph in run.glyphs.iter().filter(|glyph| glyph.w > 0.0) {
                    let point = [
                        info.x + (glyph.x + glyph.w * 0.5) / dpi,
                        info.y + (run.line_top + run.line_height * 0.5) / dpi,
                    ];
                    let mut path = hit_path(&root, point[0], point[1]);
                    renderer
                        .text_links
                        .borrow()
                        .annotate_path(&nodes, &mut path, point, dpi);
                    let hit = path.last().and_then(|entry| entry.text_link.as_ref());
                    let mut event = crate::app::pointer_events::MouseEvent::new(
                        point[0],
                        point[1],
                        winit::event::MouseButton::Left,
                        0,
                    );
                    crate::app::pointer_events::dispatch_pointer(
                        &path,
                        &mut event,
                        crate::app::pointer_events::PointerKind::Move,
                    );
                    assert_eq!(event.hovered_text_link.as_ref(), hit);
                    assert!(event.text_link.is_none(), "hover is not activation");
                    if glyph.metadata == 2 {
                        assert_eq!(
                            hit.map(|link| link.href.as_str()),
                            Some("https://example.com/one"),
                            "point={point:?}"
                        );
                        hits.push(point);
                    } else {
                        assert!(
                            hit.is_none(),
                            "plain text and blocked destinations must not activate"
                        );
                    }
                    assert_eq!(
                        renderer
                            .text_links
                            .borrow()
                            .resolve_cursor(&root, &nodes, point, dpi, None, false)
                            .0,
                        (glyph.metadata == 2).then(|| crate::style::CursorKind::Pointer.into()),
                        "cursor must follow the painted span, not the whole paragraph"
                    );
                }
            }
            assert!(
                hits.len() > 10,
                "must validate actual linked glyphs; spans={:?}; glyph metadata={:?}",
                info.spans,
                buffer
                    .layout_runs()
                    .flat_map(|run| run.glyphs.iter().map(|g| g.metadata))
                    .collect::<Vec<_>>()
            );
            let point = hits[0];
            // Redraw samples use painted regions at the same pointer position.
            let (_, stationary) = renderer
                .text_links
                .borrow()
                .resolve_cursor(&root, &nodes, point, dpi, None, false);
            assert!(stationary.unwrap().last().unwrap().text_link.is_some());
            let mut clipped = nodes.clone();
            clipped[0].clip_bounds = Some([point[0] + 1.0, point[1], 40.0, 40.0]);
            let (_, stationary) = renderer
                .text_links
                .borrow()
                .resolve_cursor(&root, &clipped, point, dpi, None, true);
            assert!(
                stationary.unwrap().last().unwrap().text_link.is_none(),
                "stationary pointer notices clipping"
            );
            // Snapshot indices are a lookup shortcut, never node identity. A
            // reordered snapshot before retention must still find the right run.
            let mut anonymous = info.clone();
            anonymous.node_id = None;
            let mut reordered = vec![anonymous, info.clone()];
            let mut path = hit_path(&root, point[0], point[1]);
            renderer.text_links.borrow_mut().retain_nodes(&nodes);
            renderer
                .text_links
                .borrow()
                .annotate_path(&reordered, &mut path, point, dpi);
            assert_eq!(
                path.last().unwrap().text_link.as_ref().unwrap().span_index,
                1
            );
            renderer.text_links.borrow_mut().retain_nodes(&reordered);
            renderer
                .text_links
                .borrow()
                .annotate_path(&reordered, &mut path, point, dpi);
            assert!(path.last().unwrap().text_link.is_some());
            // If a node occurs twice, the last collected instance owns the hit,
            // just as in the reverse-search path. Its clipping must be respected.
            let mut duplicate = info.clone();
            duplicate.clip_bounds = Some([point[0] + 1.0, point[1], 40.0, 40.0]);
            reordered.push(duplicate);
            renderer.text_links.borrow_mut().retain_nodes(&reordered);
            renderer
                .text_links
                .borrow()
                .annotate_path(&reordered, &mut path, point, dpi);
            assert!(path.last().unwrap().text_link.is_none());
            renderer.text_links.borrow_mut().retain_nodes(&nodes);
            let cache = renderer.text_links.borrow();
            assert!(cache.hit(info, point, dpi + 0.25).is_none());
            let mut changed = info.clone();
            changed.spans.as_mut().unwrap()[1].href = Some("https://example.com/new".into());
            assert!(
                cache.hit(&changed, point, dpi).is_none(),
                "declaration changes invalidate old paint"
            );
            changed = info.clone();
            changed.clip_bounds = Some([point[0] + 1.0, point[1], 40.0, 40.0]);
            assert!(cache.hit(&changed, point, dpi).is_none());
            changed = info.clone();
            changed.y -= 12.0;
            assert!(
                cache
                    .hit(&changed, [point[0], point[1] - 12.0], dpi)
                    .is_some(),
                "scroll translation reuses local glyphs"
            );
            let mut cover = Element::new();
            cover.layout_bounds = [0.0, 0.0, width, 400.0];
            root.children.push(cover);
            let mut path = hit_path(&root, point[0], point[1]);
            cache.annotate_path(&nodes, &mut path, point, dpi);
            assert!(
                path.last().unwrap().text_link.is_none(),
                "covered text must not activate"
            );
            assert_eq!(
                cache
                    .resolve_cursor(&root, &nodes, point, dpi, None, false)
                    .0,
                None
            );
            drop(cache);
            renderer.text_links.borrow_mut().retain_nodes(&[]);
            assert!(
                renderer.text_links.borrow().hit(info, point, dpi).is_none(),
                "unmounted nodes release regions"
            );
            let (_, cleared) =
                renderer
                    .text_links
                    .borrow()
                    .resolve_cursor(&root, &[], point, dpi, None, true);
            assert!(cleared
                .unwrap()
                .iter()
                .all(|entry| entry.text_link.is_none()));
            assert!(
                renderer
                    .text_links
                    .borrow()
                    .resolve_cursor(&root, &[], point, dpi, None, false)
                    .1
                    .is_none(),
                "plain pages keep the fast path"
            );
        }
    }
}
