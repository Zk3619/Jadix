//! GPU differential tests: retained partial updates must match a fresh full draw.
use super::*;
use crate::app::{damage::*, draw_cull::*};
use crate::renderer::offscreen::{images_match, readback_texture, OffscreenGpu};

#[test]
#[ignore = "requires a GPU adapter; run explicitly with --ignored --nocapture"]
fn finalized_scroll_pin_clear_cannot_touch_header() {
    let gpu = OffscreenGpu::new().expect("GPU adapter");
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("scroll pin header boundary"),
        size: wgpu::Extent3d {
            width: 320,
            height: 200,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    let pipeline =
        RuiPipeline::from_format(&gpu.device, wgpu::TextureFormat::Rgba8UnormSrgb, 320, 200);
    pipeline.update_resolution(&gpu.queue, 320, 200);
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("scroll repair"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        let rects = scroll_repair_rects_physical(&[], &[[120.0, 64.0, 80.0, 32.0]], 1.0).unwrap();
        for rect in rects.strips() {
            let (x, y, w, h) = physical_xywh_to_scissor(rect, 320, 200).unwrap();
            pass.set_scissor_rect(x, y, w, h);
            draw_scissor_fill(&mut pass, &pipeline, &gpu.device, rect, wgpu::Color::WHITE);
        }
    }
    let image = readback_texture(&gpu.device, &gpu.queue, &texture, 320, 200, encoder).unwrap();
    assert!(
        image
            .pixels
            .chunks_exact(4)
            .take(320 * 64)
            .all(|p| p[0] == 0),
        "scroll repair erased header pixels"
    );
    assert_eq!(
        image.pixel(150, 64).unwrap()[0],
        255,
        "first repaired row must be opaque, not an AA fringe"
    );
}

#[test]
#[ignore = "requires a GPU adapter; run explicitly with --ignored --nocapture"]
fn culled_latest_repair_must_preserve_header_pixels() {
    let gpu = OffscreenGpu::new().expect("GPU adapter");
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("scroll repair header regression"),
        size: wgpu::Extent3d {
            width: 320,
            height: 200,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    let pipeline =
        RuiPipeline::from_format(&gpu.device, wgpu::TextureFormat::Rgba8UnormSrgb, 320, 200);
    pipeline.update_resolution(&gpu.queue, 320, 200);
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("scroll pin clear"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        let mut root = Element::new()
            .w(320.0)
            .h(200.0)
            .flex_col()
            .child(Element::new().w(320.0).h(64.0).flex_shrink(0.0))
            .child(
                Element::new()
                    .w(320.0)
                    .h(136.0)
                    .relative()
                    .overflow_y("hidden")
                    .child(
                        Element::new()
                            .absolute()
                            .left(120.0)
                            .top(-22.0)
                            .w(80.0)
                            .h(32.0),
                    ),
            );
        let mut tree = LayoutTree::new();
        let _ = tree.build_and_layout(&mut root, 320.0, 200.0, [-1.0, -1.0]);
        let key = paint_element_key(&root.children[1].children[0]);
        let mut dirties = [PaintDirty {
            key,
            bounds: [120.0, 28.0, 80.0, 60.0],
            kind: PaintDirtyKind::Node,
        }];
        let mut paints = Vec::new(); // Real failure: Latest was culled; slots=[].
        restore_culled_dirty_clips(&root, &mut paints, &dirties);
        prepare_node_present_dirties(&mut dirties, &paints, &[], &[], &[], &[], 1.0, None);
        let hole = node_present_scissor_logical(
            dirties[0].kind,
            key,
            dirties[0].bounds,
            &[],
            &paints,
            0.0,
        );
        if let Some((x, y, w, h)) = damage_to_scissor_physical(hole, 0.0, 1.0, 320, 200) {
            pass.set_scissor_rect(x, y, w, h);
            draw_scissor_fill(&mut pass, &pipeline, &gpu.device, hole, wgpu::Color::WHITE);
        }
    }
    let image = readback_texture(&gpu.device, &gpu.queue, &texture, 320, 200, encoder).unwrap();
    let header_damage = image
        .pixels
        .chunks_exact(4)
        .take(320 * 64)
        .filter(|p| p[0] > 2)
        .count();
    assert_eq!(
        header_damage, 0,
        "scroll pin clear changed fixed-header pixels"
    );
    assert!(
        image.pixel(150, 70).unwrap()[0] > 250,
        "repair hole must actually clear"
    );
}

#[test]
#[ignore = "requires a GPU adapter; run explicitly with --ignored --nocapture"]
fn latest_fade_over_wrapping_text_matches_full_composite() {
    let gpu = OffscreenGpu::new().expect("GPU adapter");
    let config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        width: 320,
        height: 200,
        present_mode: wgpu::PresentMode::Fifo,
        alpha_mode: wgpu::CompositeAlphaMode::Opaque,
        color_space: wgpu::SurfaceColorSpace::Auto,
        view_formats: vec![],
        desired_maximum_frame_latency: 1,
    };
    let pipeline = RuiPipeline::from_format(&gpu.device, config.format, 320, 200);
    pipeline.update_resolution(&gpu.queue, 320, 200);
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Latest retained fade"),
        size: wgpu::Extent3d {
            width: 320,
            height: 200,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: config.format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let mut root = Element::new().w(320.0).h(200.0).relative().bg([1.0;4])
        .child(Element::new().w(280.0).text("MessageScrollerContent sets role=log and aria-relevant=additions. The button leaves no ghost focus stops."))
        .child(Element::new().absolute().left(110.0).top(40.0).w(100.0).h(32.0)
            .z_index(20).bg([1.0;4]).rounded(12.0).child(Element::new().text("Latest")));
    let mut tree = LayoutTree::new();
    let mut previous = Vec::new();
    for (frame, alpha) in [0.0, 0.12, 0.45, 0.8, 1.0, 0.45, 0.08, 0.015, 0.0]
        .into_iter()
        .enumerate()
    {
        root.children[1].style.opacity = alpha;
        root.children[1].style.inset.top =
            taffy::style::LengthPercentageAuto::length(40.0 + (1.0 - alpha) * 8.0);
        let scene = tree.build_and_layout(&mut root, 320.0, 200.0, [-1.0, -1.0]);
        let key = paint_element_key(&root.children[1]);
        let mut dirty = [PaintDirty {
            key,
            bounds: root.children[1].layout_bounds,
            kind: PaintDirtyKind::Node,
        }];
        prepare_node_present_dirties(
            &mut dirty,
            &scene.node_paints,
            &previous,
            &scene.instances,
            &[],
            &[],
            1.0,
            None,
        );
        let clip = if frame == 0 {
            None
        } else {
            Some(node_present_scissor_logical(
                dirty[0].kind,
                key,
                dirty[0].bounds,
                &previous,
                &scene.node_paints,
                0.0,
            ))
        };
        if frame > 0 {
            assert_eq!(dirty[0].kind, PaintDirtyKind::Region);
        }
        draw_latest_scene_for_test(&gpu, &config, &pipeline, &texture, &scene, clip);
        let partial = readback_texture(
            &gpu.device,
            &gpu.queue,
            &texture,
            320,
            200,
            gpu.device.create_command_encoder(&Default::default()),
        )
        .unwrap();
        draw_latest_scene_for_test(&gpu, &config, &pipeline, &texture, &scene, None);
        let full = readback_texture(
            &gpu.device,
            &gpu.queue,
            &texture,
            320,
            200,
            gpu.device.create_command_encoder(&Default::default()),
        )
        .unwrap();
        let diffs: Vec<_> = partial
            .pixels
            .chunks_exact(4)
            .zip(full.pixels.chunks_exact(4))
            .enumerate()
            .filter(|(_, (a, b))| a.iter().zip(b.iter()).any(|(x, y)| x.abs_diff(*y) > 2))
            .take(12)
            .map(|(i, (a, b))| (i % 320, i / 320, a, b))
            .collect();
        assert!(images_match(&partial,&full,2),"Latest frame={frame} alpha={alpha}: partial enter/exit composite differs from full draw; clip={clip:?} diffs={diffs:?}");
        previous = last_present_visual_slots(
            &scene.node_paints,
            &scene.instances,
            &[],
            &scene.text_nodes,
            &[],
        );
    }
}

fn draw_latest_scene_for_test(
    gpu: &OffscreenGpu,
    config: &wgpu::SurfaceConfiguration,
    pipeline: &RuiPipeline,
    texture: &wgpu::Texture,
    scene: &crate::layout::layout_tree::RenderResult,
    clip: Option<[f32; 4]>,
) {
    let bands = scene.overlay_split.page_stack_band_ranges();
    let mut renderers = Vec::new();
    for band in &bands {
        let mut renderer = RuiTextRenderer::new(&gpu.device, &gpu.queue, config);
        crate::renderer::prepare_text::prepare_text_frame_at(
            &mut renderer,
            &gpu.device,
            &gpu.queue,
            320,
            200,
            &scene.text_nodes[band.text.0..band.text.1],
            1.0,
            false,
            &Default::default(),
            &Default::default(),
        );
        renderers.push(renderer);
    }
    let view = texture.create_view(&Default::default());
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Latest ordered composite"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: if clip.is_some() {
                        wgpu::LoadOp::Load
                    } else {
                        wgpu::LoadOp::Clear(wgpu::Color::WHITE)
                    },
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        if let Some(clip) = clip {
            let (x, y, w, h) = damage_to_scissor_physical(clip, 0.0, 1.0, 320, 200).unwrap();
            pass.set_scissor_rect(x, y, w, h);
            draw_scissor_fill(&mut pass, pipeline, &gpu.device, clip, wgpu::Color::WHITE);
        }
        for (band, renderer) in bands.iter().zip(&renderers) {
            draw_sdf_range_with_clip(
                &mut pass,
                pipeline,
                &gpu.device,
                pipeline.dummy_backdrop(),
                &scene.instances,
                &scene.clip_polys,
                band.instance.0 as u32,
                (band.instance.1 - band.instance.0) as u32,
                None,
            );
            renderer.render(&mut pass);
        }
    }
    gpu.queue.submit(Some(encoder.finish()));
}

#[test]
#[ignore = "requires a GPU adapter; run explicitly with --ignored --nocapture"]
fn scrolled_out_message_capture_must_not_replay_previous_glyph_mesh() {
    let gpu = OffscreenGpu::new().expect("GPU adapter");
    let config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        width: 320,
        height: 200,
        present_mode: wgpu::PresentMode::Fifo,
        alpha_mode: wgpu::CompositeAlphaMode::Opaque,
        color_space: wgpu::SurfaceColorSpace::Auto,
        view_formats: vec![],
        desired_maximum_frame_latency: 1,
    };
    let mut renderer = RuiTextRenderer::new(&gpu.device, &gpu.queue, &config);
    let mut root = Element::new()
        .w(320.0)
        .h(200.0)
        .flex_col()
        .child(Element::new().text("Builder"))
        .child(Element::new().text("On it, one-line change."))
        .child(Element::new().text("Delivered"));
    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut root, 320.0, 200.0, [-1.0, -1.0]);
    let mut texts = result.text_nodes.clone();
    let prepare = |renderer: &mut RuiTextRenderer,
                   texts: &[crate::layout::layout_tree::TextLayoutInfo]| {
        crate::renderer::prepare_text::prepare_text_frame_at(
            renderer,
            &gpu.device,
            &gpu.queue,
            320,
            200,
            texts,
            1.0,
            false,
            &Default::default(),
            &Default::default(),
        )
    };
    let first = prepare(&mut renderer, &texts);
    assert!(!first.culled_skip_prepare);
    let image = draw_message_capture_for_test(&gpu, &renderer);
    assert!(
        image.pixels.chunks_exact(4).any(|p| p[0] < 200),
        "fixture must rasterize Message glyphs"
    );
    let mut overlay =
        RuiTextRenderer::new_sharing_resources(&gpu.device, &gpu.queue, &config, &renderer);
    let _ = prepare(&mut overlay, &texts);
    overlay.prepare_resolution(&gpu.device, &gpu.queue, 320, 200, std::iter::empty());
    let cleared_overlay = draw_message_capture_for_test(&gpu, &overlay);
    assert!(
        cleared_overlay.pixels.chunks_exact(4).all(|p| p[0] == 255),
        "clearing the overlay must discard its own glyph vertices"
    );
    let page_after_overlay_clear = draw_message_capture_for_test(&gpu, &renderer);
    assert!(
        images_match(&image, &page_after_overlay_clear, 0),
        "sharing GPU resources must keep page and overlay vertex batches independent"
    );
    for text in &mut texts {
        text.y -= 400.0;
        text.clip_bounds = Some([0.0, 50.0, 320.0, 150.0]);
    }
    let hidden = prepare(&mut renderer, &texts);
    assert!(
        hidden.culled_skip_prepare,
        "fixture must reach all-clipped prepare after scrolling"
    );
    let image = draw_message_capture_for_test(&gpu, &renderer);
    let ghosts = image.pixels.chunks_exact(4).filter(|p| p[0] < 250).count();
    assert_eq!(
        ghosts, 0,
        "all-clipped Message prepare must not retain glyph vertices from before scrolling"
    );
    assert!(
        !hidden.needs_warmup,
        "fully culled text must not request endless warmup frames"
    );
    for text in &mut texts {
        text.y += 480.0;
        text.clip_bounds = Some([0.0, 50.0, 320.0, 150.0]);
    }
    let visible_again = prepare(&mut renderer, &texts);
    assert!(
        !visible_again.culled_skip_prepare,
        "clearing draw vertices must preserve reusable shaping/atlas state"
    );
    let image = draw_message_capture_for_test(&gpu, &renderer);
    assert!(
        image
            .pixels
            .chunks_exact(4)
            .enumerate()
            .any(|(i, p)| i / 320 >= 80 && p[0] < 200),
        "Message text must reappear at its new position"
    );
    assert!(
        image
            .pixels
            .chunks_exact(4)
            .enumerate()
            .all(|(i, p)| i / 320 >= 75 || p[0] >= 250),
        "resuming a capture must not revive text at its old position"
    );
}

fn draw_message_capture_for_test(
    gpu: &OffscreenGpu,
    renderer: &RuiTextRenderer,
) -> crate::renderer::offscreen::RgbaImage {
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Message capture stale-mesh regression"),
        size: wgpu::Extent3d {
            width: 320,
            height: 200,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Message capture"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        renderer.render(&mut pass);
    }
    readback_texture(&gpu.device, &gpu.queue, &texture, 320, 200, encoder).unwrap()
}

#[test]
#[ignore = "requires a GPU adapter; run explicitly with --ignored --nocapture"]
fn retained_node_and_region_match_full_clear_pixels() {
    let gpu = OffscreenGpu::new().expect("GPU differential test requires an adapter");
    eprintln!("GPU differential adapter: {:?}", gpu.adapter_info);
    for (background, portal) in [(false, false), (true, false), (true, true)] {
        check_partial_pixels(&gpu, background, portal);
    }
}

fn check_partial_pixels(gpu: &OffscreenGpu, background: bool, portal: bool) {
    const W: u32 = 128;
    const H: u32 = 96;
    let clear = wgpu::Color::WHITE;
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let pipeline = RuiPipeline::from_format(&gpu.device, format, W, H);
    pipeline.update_resolution(&gpu.queue, W, H);
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("partial/full differential retain"),
        size: wgpu::Extent3d {
            width: W,
            height: H,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let mut root = Element::new().w(W as f32).h(H as f32).relative().child(
        Element::new()
            .absolute()
            .left(12.0)
            .top(18.0)
            .w(104.0)
            .h(56.0)
            .bg([1.0, 0.0, 0.0, 0.8]),
    );
    if background {
        root.style.bg_color = [1.0; 4];
    }
    let mut tree = LayoutTree::new();
    let initial = tree.build_and_layout(&mut root, W as f32, H as f32, [-1.0, -1.0]);
    // A static portal bisects the Node dirty. Region case has no portal so the
    // test isolates the missing-backdrop fallback and repeated alpha updates.
    let overlay = portal.then_some([54.0, 0.0, 20.0, H as f32]);
    let overlay_inst = overlay.map(|b| {
        let mut i = initial.instances.last().copied().expect("painted child");
        i.rect_pos = [b[0], b[1]];
        i.rect_size = [b[2], b[3]];
        i.color = [0.0, 0.8, 0.0, 0.65];
        i
    });
    let occupancy = overlay_inst.as_ref().map(instance_paint_bounds);
    let mut prev = last_present_visual_slots(
        &initial.node_paints,
        &initial.instances,
        &[],
        &initial.text_nodes,
        &[],
    );
    let mut encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("initial full draw"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(clear),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        draw_cpu_sdf_instances(
            &mut pass,
            &pipeline,
            &gpu.device,
            pipeline.dummy_backdrop(),
            &initial.instances,
        );
        if let Some(i) = overlay_inst {
            draw_cpu_sdf_instances(
                &mut pass,
                &pipeline,
                &gpu.device,
                pipeline.dummy_backdrop(),
                &[i],
            );
        }
    }
    gpu.queue.submit(Some(encoder.finish()));
    for (frame, opacity) in [0.2, 0.65, 0.0, 0.45].into_iter().enumerate() {
        root.children[0].style.bg_color = [0.0, 0.0, 1.0, opacity];
        tree.set_collect_skip_allowed(false);
        let current =
            tree.build_and_layout_fast(&mut root, W as f32, H as f32, [-1.0, -1.0], false, false);
        let key = paint_element_key(&root.children[0]);
        let bounds = root.children[0].layout_bounds;
        let mut dirty = [PaintDirty {
            key,
            bounds,
            kind: PaintDirtyKind::Node,
        }];
        prepare_node_present_dirties(
            &mut dirty,
            &current.node_paints,
            &prev,
            &current.instances,
            &[],
            &[],
            1.0,
            occupancy,
        );
        let reset = node_present_dest_reset_full(
            key,
            bounds,
            &current.node_paints,
            &current.instances,
            &[],
            &[],
            1.0,
            prev.iter().find(|(k, _)| *k == key).map(|(_, r)| *r),
        );
        let hole = if dirty[0].kind == PaintDirtyKind::Region {
            dirty[0].bounds
        } else {
            if reset.is_none() {
                assert!(layered_src_over_skips_dest_reset(key,bounds,&current.node_paints,
                    prev.iter().find(|(k, _)| *k == key).map(|(_, r)| *r)),
                    "Node must have a backdrop unless this is its first visible paint: frame={frame}");
            }
            reset.as_ref().map(|r| r.rect).unwrap_or_else(|| {
                node_present_scissor_logical(
                    dirty[0].kind,
                    key,
                    bounds,
                    &prev,
                    &current.node_paints,
                    node_present_scissor_pad(dirty[0].kind),
                )
            })
        };
        let mut encoder = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("retained partial update"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            let clips = if dirty[0].kind == PaintDirtyKind::Region {
                vec![hole]
            } else {
                page_scissors_excluding_overlay(hole, occupancy)
            };
            for clip in clips {
                let (x, y, w, h) = damage_to_scissor_physical(clip, 0.0, 1.0, W, H).unwrap();
                pass.set_scissor_rect(x, y, w, h);
                if dirty[0].kind == PaintDirtyKind::Region || reset.is_some() {
                    draw_scissor_fill(&mut pass, &pipeline, &gpu.device, hole, clear);
                }
                let range = if dirty[0].kind == PaintDirtyKind::Region {
                    (0, current.instances.len())
                } else {
                    current
                        .node_paints
                        .iter()
                        .find(|p| p.key == key)
                        .unwrap()
                        .instance
                };
                draw_sdf_range_with_clip(
                    &mut pass,
                    &pipeline,
                    &gpu.device,
                    pipeline.dummy_backdrop(),
                    &current.instances,
                    &current.clip_polys,
                    range.0 as u32,
                    (range.1 - range.0) as u32,
                    None,
                );
                if dirty[0].kind == PaintDirtyKind::Region {
                    if let Some(i) = overlay_inst {
                        draw_cpu_sdf_instances(
                            &mut pass,
                            &pipeline,
                            &gpu.device,
                            pipeline.dummy_backdrop(),
                            &[i],
                        );
                    }
                }
            }
        }
        let partial = readback_texture(&gpu.device, &gpu.queue, &texture, W, H, encoder).unwrap();
        let mut full_instances = current.instances.clone();
        full_instances.extend(overlay_inst);
        let full = gpu
            .render_instances_readback(W, H, clear, &full_instances)
            .unwrap();
        let different: Vec<_> = partial
            .pixels
            .chunks_exact(4)
            .zip(full.pixels.chunks_exact(4))
            .enumerate()
            .filter(|(_, (a, b))| a.iter().zip(b.iter()).any(|(x, y)| x.abs_diff(*y) > 2))
            .take(12)
            .map(|(i, (a, b))| ((i as u32 % W, i as u32 / W), a.to_vec(), b.to_vec()))
            .collect();
        assert!(images_match(&partial,&full,2),
            "retained/full pixels differ: background={background}, portal={portal}, frame={frame}, alpha={opacity}, hole={hole:?}, diffs={different:?}");
        prev = last_present_visual_slots(
            &current.node_paints,
            &current.instances,
            &[],
            &current.text_nodes,
            &[],
        );
    }
}
