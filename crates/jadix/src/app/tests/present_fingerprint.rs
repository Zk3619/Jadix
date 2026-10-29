use super::hash_present_underline_layers;
use crate::renderer::InstanceInput;
use bytemuck::Zeroable;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

fn fp(page: &[InstanceInput], page_stack: &[Vec<InstanceInput>], underline: bool) -> u64 {
    let mut h = DefaultHasher::new();
    hash_present_underline_layers(&mut h, page, page_stack, &[]);
    underline.hash(&mut h);
    h.finish()
}

#[test]
fn page_stack_underline_leave_changes_present_hash() {
    // Gallery / z-index pages use page_stack bands: `underline_instances` is empty.
    let line = InstanceInput::zeroed();
    let hovered = fp(&[], &[vec![line]], true);
    let rest = fp(&[], &[Vec::new()], false);
    assert_ne!(
        hovered, rest,
        "Link hover:underline leave must bust last_gpu_frame_fp on the page-stack path"
    );
}

#[test]
fn page_stack_reuse_keeps_last_presented_underlines() {
    let line = InstanceInput::zeroed();
    let last = vec![line];
    let reused = super::current_underline_layer(true, true, &[], &[], &last, None);
    assert_eq!(reused.len(), 1);
    let left = super::current_underline_layer(true, false, &[], &[], &last, None);
    assert!(
        left.is_empty(),
        "leave (not reuse) must drop decorations so node present can erase"
    );
    let hovered = super::current_underline_layer(true, false, &[line], &[], &[], None);
    assert_eq!(hovered.len(), 1);
}

#[test]
fn node_capture_replaces_only_refreshed_decorations() {
    let mut changed = InstanceInput::zeroed();
    changed.rect_pos = [10.0, 20.0];
    changed.rect_size = [30.0, 1.0];
    let mut neighbor = changed;
    neighbor.rect_pos[0] = 100.0;
    let bounds = [[10.0, 0.0, 30.0, 24.0]];
    let previous = [changed, neighbor];
    let left =
        super::current_underline_layer(true, false, &[], &[], &previous, Some((&[], &bounds)));
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].rect_pos, neighbor.rect_pos);
    let hovered = super::current_underline_layer(
        true,
        false,
        &[],
        &[],
        &previous,
        Some((&[changed], &bounds)),
    );
    assert_eq!(
        hovered.len(),
        2,
        "refresh must not duplicate old decorations"
    );
    assert_ne!(fp(&hovered, &[], false), fp(&left, &[], false));
}

#[test]
#[ignore = "requires GPU adapter; run with --ignored --nocapture"]
#[cfg(feature = "product-tests")]
fn breadcrumb_node_capture_draws_hover_underline_and_clears_on_leave() {
    use crate::app::damage::{clear_frame_paint_nodes, take_frame_paint_nodes};
    use crate::app::draw_cull::node_present_clipped_capture_text;
    use crate::app::tree_hit::{apply_pointer_hover_reconcile, clear_hover_tracking};
    use crate::components::breadcrumb::breadcrumb;
    use crate::components::button::sync_button_hover_paint_subtree;
    use crate::core::reactivity::create_root;
    use crate::core::tree::{Element, LayoutStatus};
    use crate::layout::layout_tree::LayoutTree;
    use crate::renderer::{offscreen::OffscreenGpu, prepare_text_frame, JadixTextRenderer};
    use crate::style::sync_class_variants_subtree;

    const W: u32 = 400;
    const H: u32 = 120;
    let gpu = OffscreenGpu::new().expect("GPU adapter");
    crate::style::set_theme_mode(crate::style::ThemeMode::Light);
    for scale in [1.0, 1.5] {
        clear_hover_tracking();
        let (_handle, mut root) = create_root(|| {
            Element::new()
                .w(W as f32 / scale)
                .h(H as f32 / scale)
                .child(
                    breadcrumb()
                        .link("Home", || {})
                        .link("Components", || {})
                        .page("Breadcrumb"),
                )
        });
        let mut tree = LayoutTree::new();
        let initial =
            tree.build_and_layout(&mut root, W as f32 / scale, H as f32 / scale, [-1.0; 2]);
        tree.set_collect_skip_allowed(true);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            width: W,
            height: H,
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: wgpu::CompositeAlphaMode::Opaque,
            view_formats: vec![],
            color_space: Default::default(),
        };
        let mut renderer = JadixTextRenderer::new(&gpu.device, &gpu.queue, &config);
        let mut previous = Vec::new();
        for target in ["Home", "Components", "Breadcrumb", "Home", ""] {
            let mouse = initial
                .text_nodes
                .iter()
                .find(|n| n.text == target)
                .map(|n| [n.x + n.width * 0.5, n.y + n.height * 0.5])
                .unwrap_or([-1.0; 2]);
            clear_frame_paint_nodes();
            apply_pointer_hover_reconcile(&root, mouse[0], mouse[1], true);
            let mut impact = LayoutStatus::Clean;
            sync_button_hover_paint_subtree(
                &mut root,
                &mut tree,
                &mut impact,
                W as f32 / scale,
                H as f32 / scale,
                [0.0; 2],
            );
            sync_class_variants_subtree(
                &mut root,
                mouse,
                "",
                false,
                W as f32 / scale,
                H as f32 / scale,
                &mut tree,
                &mut impact,
            );
            let frame = tree.build_and_layout_fast(
                &mut root,
                W as f32 / scale,
                H as f32 / scale,
                mouse,
                true,
                false,
            );
            let dirty = take_frame_paint_nodes();
            let captured =
                node_present_clipped_capture_text(&dirty, &frame.text_nodes, &frame.node_paints);
            let prep = prepare_text_frame(
                &mut renderer,
                &gpu.device,
                &gpu.queue,
                &config,
                &captured,
                scale,
                false,
                &Default::default(),
                &Default::default(),
            );
            let bounds: Vec<_> = captured
                .iter()
                .map(|n| super::captured_text_decoration_bounds(n, scale))
                .collect();
            let layer = super::current_underline_layer(
                true,
                false,
                &[],
                &[],
                &previous,
                Some((&prep.underline_instances, &bounds)),
            );
            let hovered_link = target == "Home" || target == "Components";
            assert_eq!(
                layer.len(),
                usize::from(hovered_link),
                "target={target} scale={scale} captured={:?} bounds={bounds:?}",
                captured
                    .iter()
                    .map(|n| (
                        &n.text,
                        n.x,
                        n.y,
                        n.width,
                        n.height,
                        n.style.underline_resolved()
                    ))
                    .collect::<Vec<_>>()
            );
            assert!(
                !frame
                    .text_nodes
                    .iter()
                    .find(|n| n.text == "Breadcrumb")
                    .unwrap()
                    .style
                    .underline_resolved(),
                "current page stays undecorated"
            );
            let image = gpu
                .render_instances_readback(W, H, wgpu::Color::WHITE, &layer)
                .expect("underline pixels");
            let dark = (0..H)
                .flat_map(|y| (0..W).map(move |x| (x, y)))
                .filter(|&(x, y)| image.pixel(x, y).unwrap()[0] < 220)
                .count();
            assert_eq!(
                dark > 10,
                hovered_link,
                "visible underline target={target} scale={scale}"
            );
            previous = layer;
        }
    }
    clear_hover_tracking();
}
