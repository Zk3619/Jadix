#![cfg(feature = "product-tests")]
//! Diagnose: gallery local scrim frost has no high-contrast host glyphs.
//!
//! Loop = layout the *real* `label()` + Portal tree (not a `.text()` stub) and check
//! the exact windows `AppState` uses to prepare / capture prefix glyphon.
use crate::components::label::label;
use crate::core::tree::Element;
use crate::layout::layout_tree::{
    hoist_portal_subtrees, LayoutTree, ATTR_PORTAL_KEY, ATTR_PORTAL_MOUNT,
};
use crate::style::parse_color;

fn u8_color(css: &str) -> [u8; 4] {
    let c = parse_color(css);
    [
        (c[0] * 255.0).round() as u8,
        (c[1] * 255.0).round() as u8,
        (c[2] * 255.0).round() as u8,
        (c[3] * 255.0).round() as u8,
    ]
}

fn contrast_label(id: &str, color: &str, px: f32, weight: f32, text: &str) -> Element {
    label(id)
        .text(text)
        .color(u8_color(color))
        .text_size(px)
        .font_weight(weight)
        .leading(1.25)
        .text_align("center")
        .into()
}

fn gallery_local_scrim_host() -> Element {
    let labels = Element::new()
        .absolute()
        .inset(0.0)
        .w("100%")
        .h("100%")
        .flex_col()
        .justify("center")
        .p(0.35 * 16.0)
        .gap(0.12 * 16.0)
        .z_index(0)
        .pointer_events_none()
        .children([
            contrast_label("y1", "#fde047", 1.15 * 16.0, 800.0, "YELLOW 高对比 BLUR"),
            contrast_label(
                "y2",
                "#ffffff",
                1.05 * 16.0,
                800.0,
                "WHITE ABCXYZ 1234567890",
            ),
            contrast_label(
                "y3",
                "#facc15",
                0.95 * 16.0,
                800.0,
                "The quick brown fox 衬线对比",
            ),
        ]);
    let caption: Element = label("cap")
        .text("蓝色宿主（四周 12.5% 露出）")
        .color(u8_color("#ffffff"))
        .text_size(0.75 * 16.0)
        .into();
    let caption = caption.p(0.6 * 16.0).z_index(1).pointer_events_none();

    let inner_label: Element = label("scrim-txt")
        .text("遮罩 50% · 上层黑底")
        .color(u8_color("#ffffff"))
        .text_size(0.75 * 16.0)
        .font_weight(600.0)
        .text_align("center")
        .into();
    let black = Element::new()
        .w("50%")
        .h("50%")
        .rounded(8.0)
        .bg(parse_color("#000000"))
        .flex_items("center")
        .justify("center")
        .p(8.0)
        .child(inner_label);
    let scrim = Element::new()
        .w("75%")
        .h("75%")
        .rounded(10.0)
        .bg([1.0, 1.0, 1.0, 0.55])
        .backdrop_blur(16.0)
        .flex_items("center")
        .justify("center")
        .child(black);
    let mut overlay = Element::new()
        .w("100%")
        .h("100%")
        .absolute()
        .inset(0.0)
        .z_index(2)
        .flex_items("center")
        .justify("center")
        .child(scrim);
    overlay.set_attr("jadix_portal", "1");
    overlay.set_attr(ATTR_PORTAL_MOUNT, "gal-local-scrim-host");
    overlay.set_attr(ATTR_PORTAL_KEY, "gal-local-scrim");

    let host = Element::new()
        .relative()
        .w(22.0 * 16.0)
        .h(14.0 * 16.0)
        .rounded(12.0)
        .bg(parse_color("#2563eb"))
        .overflow_x("hidden")
        .overflow_y("hidden")
        .template_id("gal-local-scrim-host")
        .children([labels, caption, overlay]);

    host
}

fn gallery_local_scrim_tree() -> Element {
    let host = gallery_local_scrim_host();
    let mut page = Element::new()
        .w(800.0)
        .h(600.0)
        .child(Element::from(label("sidebar").text("SIDEBAR chrome")))
        .child(host);
    page.set_attr("data-stacking-context", "1");
    page
}

#[test]
fn rsx_unitless_backdrop_blur_is_rem_like_spacing() {
    let rem = Element::new().backdrop_blur("1");
    assert!(
        (rem.style.backdrop_blur - 16.0).abs() < 0.01,
        "RSX unitless \"1\" is 1rem = 16px, got {}",
        rem.style.backdrop_blur
    );
    let sixteen = Element::new().backdrop_blur("16");
    assert!(
        (sixteen.style.backdrop_blur - 256.0).abs() < 0.01,
        "RSX unitless \"16\" is 16rem = 256px, got {}",
        sixteen.style.backdrop_blur
    );
    let px = Element::new().backdrop_blur("16px");
    assert!((px.style.backdrop_blur - 16.0).abs() < 0.01);
    let builder = Element::new().backdrop_blur(16.0);
    assert!(
        (builder.style.backdrop_blur - 16.0).abs() < 0.01,
        "builder 16.0 is 16px, got {}",
        builder.style.backdrop_blur
    );
}

fn rects_intersect(rect: [f32; 4], clip: Option<[f32; 4]>) -> bool {
    let Some(c) = clip else {
        return rect[2] > 0.0 && rect[3] > 0.0;
    };
    let x0 = rect[0].max(c[0]);
    let y0 = rect[1].max(c[1]);
    let x1 = (rect[0] + rect[2]).min(c[0] + c[2]);
    let y1 = (rect[1] + rect[3]).min(c[1] + c[3]);
    x1 > x0 && y1 > y0
}

fn dump_split(result: &crate::layout::layout_tree::RenderResult) -> String {
    let overlay_text_start = result.overlay_split.text_start.min(result.text_nodes.len());
    let bands = result.overlay_split.page_stack_band_ranges();
    let blur_at = result.instances.iter().position(|i| i.backdrop_blur > 0.5);
    let yellow_at = result
        .text_nodes
        .iter()
        .position(|t| t.text.contains("YELLOW"));
    let mut s = String::new();
    s.push_str(&format!(
        "overlay_text_start={}/{} page_stack_bands_raw={} ranges={} blur_inst={:?} yellow_text={:?}\n",
        overlay_text_start,
        result.text_nodes.len(),
        result.overlay_split.page_stack_bands.len(),
        bands.len(),
        blur_at,
        yellow_at,
    ));
    for (i, t) in result.text_nodes.iter().enumerate() {
        let col = t.style.color_resolved();
        s.push_str(&format!(
            "  text[{i}] {:?} xy=({:.1},{:.1}) {}x{} clip={:?} color={:?} raster={:?}\n",
            t.text.chars().take(24).collect::<String>(),
            t.x,
            t.y,
            t.width,
            t.height,
            t.clip_bounds,
            col,
            t.glyph_raster,
        ));
    }
    for (i, b) in bands.iter().enumerate() {
        let lo = b.text.0.min(overlay_text_start);
        let hi = b.text.1.min(overlay_text_start);
        s.push_str(&format!(
            "  band[{i}] inst={:?} tex={:?} text={:?} paint={:?} chrome={:?} prepare={lo}..{hi} empty={} blur_in={} yellow_in_range={} yellow_in_prepare={}\n",
            b.instance,
            b.textured,
            b.text,
            b.paint_ops,
            b.chrome,
            lo >= hi,
            blur_at.is_some_and(|p| b.instance.0 <= p && p < b.instance.1),
            yellow_at.is_some_and(|p| b.text.0 <= p && p < b.text.1),
            yellow_at.is_some_and(|p| lo <= p && p < hi),
        ));
    }
    if let Some(yi) = yellow_at {
        let y = &result.text_nodes[yi];
        let yrect = [y.x, y.y, y.width.max(0.0), y.height.max(0.0)];
        let yband = bands.iter().position(|b| b.text.0 <= yi && yi < b.text.1);
        let bband = blur_at.and_then(|p| {
            bands
                .iter()
                .position(|b| b.instance.0 <= p && p < b.instance.1)
        });
        s.push_str("  prefix cover over YELLOW (later band, a>0.05, intersects):\n");
        for (bi, b) in bands.iter().enumerate() {
            if yband.is_some_and(|yb| bi <= yb) {
                continue;
            }
            if bband.is_some_and(|bb| bi >= bb) {
                continue;
            }
            for ii in b.instance.0..b.instance.1.min(result.instances.len()) {
                let inst = &result.instances[ii];
                if inst.color[3] <= 0.05 {
                    continue;
                }
                let irect = [
                    inst.rect_pos[0],
                    inst.rect_pos[1],
                    inst.rect_size[0],
                    inst.rect_size[1],
                ];
                if rects_intersect(yrect, Some(irect)) {
                    s.push_str(&format!(
                        "    band{bi} inst{ii} pos={:?} size={:?} color={:?} blur={}\n",
                        inst.rect_pos, inst.rect_size, inst.color, inst.backdrop_blur
                    ));
                }
            }
        }
    }
    s
}

#[test]
fn gallery_label_portal_yellow_is_in_capture_prepare_window() {
    let mut root = Element::new().child(gallery_local_scrim_tree());
    assert!(
        hoist_portal_subtrees(&mut root) >= 1,
        "local Portal mount must hoist onto the host"
    );

    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut root, 800.0, 600.0, [-1.0, -1.0]);
    let dump = dump_split(&result);

    let overlay_text_start = result.overlay_split.text_start.min(result.text_nodes.len());
    let yellow_at = result
        .text_nodes
        .iter()
        .position(|t| t.text.contains("YELLOW"));
    assert!(
        yellow_at.is_some(),
        "YELLOW label never collected as a text node\n{dump}"
    );
    let yellow_at = yellow_at.unwrap();
    let yellow = &result.text_nodes[yellow_at];

    assert!(
        yellow_at < overlay_text_start,
        "YELLOW is overlay text (index {yellow_at} >= overlay_text_start {overlay_text_start}); page-stack prepare skips it\n{dump}"
    );

    let col = yellow.style.color_resolved();
    assert!(
        col[0] > 200 && col[1] > 180 && col[3] > 200,
        "YELLOW resolved color is not high-contrast yellow: {col:?}\n{dump}"
    );

    let node_rect = [
        yellow.x,
        yellow.y,
        yellow.width.max(0.0),
        yellow.height.max(0.0),
    ];
    let vis = rects_intersect(node_rect, yellow.clip_bounds)
        && rects_intersect(node_rect, Some([0.0, 0.0, 800.0, 600.0]));
    assert!(
        vis,
        "YELLOW is clip-culled at layout (would culled_skip_prepare / empty glyphon)\n{dump}"
    );
    if let Some(clip) = yellow.clip_bounds {
        assert!(
            crate::renderer::prepare_text::integerize_clip_rect(clip).is_some(),
            "YELLOW clip integerizes to empty TextBounds (glyphon would emit no vertices)\n{dump}"
        );
    }

    let bands = result.overlay_split.page_stack_band_ranges();
    assert!(
        !bands.is_empty(),
        "expected page_stack bands for local backdrop capture\n{dump}"
    );
    let blur_at = result
        .instances
        .iter()
        .position(|i| i.backdrop_blur > 0.5)
        .unwrap_or_else(|| panic!("scrim must stamp backdrop_blur\n{dump}"));
    let blur_band = bands
        .iter()
        .position(|b| b.instance.0 <= blur_at && blur_at < b.instance.1)
        .unwrap_or_else(|| panic!("blur instance not in any page_stack band\n{dump}"));
    let text_band = bands
        .iter()
        .position(|b| b.text.0 <= yellow_at && yellow_at < b.text.1)
        .unwrap_or_else(|| panic!("YELLOW not in any page_stack band\n{dump}"));
    assert!(
        text_band < blur_band,
        "YELLOW band {text_band} is not before scrim band {blur_band}; capture prefix misses it\n{dump}"
    );

    let prep_lo = bands[text_band].text.0.min(overlay_text_start);
    let prep_hi = bands[text_band].text.1.min(overlay_text_start);
    assert!(
        prep_lo <= yellow_at && yellow_at < prep_hi,
        "YELLOW in band {text_band} but overlay_text_start clips prepare to {prep_lo}..{prep_hi}\n{dump}"
    );
}

/// Matches `capture_page_stack_local_backdrop`: band 0 skips glyphon entirely.
fn capture_draws_band_text(blur_band: usize, text_band: usize) -> bool {
    if blur_band == 0 {
        false
    } else {
        text_band < blur_band
    }
}

/// Gallery「浮层 / 反馈」: ScrollArea (content + z=1000 overlay scrollbar) already
/// opens `page_stack_depth` before the local host.
fn gallery_overlays_scroll_shell(inner: Element) -> Element {
    let content = Element::new()
        .w("100%")
        .flex_col()
        .gap(16.0)
        .p(24.0)
        .child(Element::from(label("above").text("ViewportModal chrome")))
        .child(inner);
    let scrollbar = Element::new()
        .absolute()
        .w(8.0)
        .h("100%")
        .z_index(1000)
        .bg([0.0, 0.0, 0.0, 0.2]);
    Element::new()
        .relative()
        .w(800.0)
        .h(600.0)
        .overflow_y("auto")
        .children([content, scrollbar])
}

fn assert_yellow_lands_in_capture_prefix(mut root: Element) {
    assert!(
        hoist_portal_subtrees(&mut root) >= 1,
        "local Portal mount must hoist onto the host"
    );
    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut root, 800.0, 600.0, [-1.0, -1.0]);
    let dump = dump_split(&result);
    let overlay_text_start = result.overlay_split.text_start.min(result.text_nodes.len());
    let yellow_at = result
        .text_nodes
        .iter()
        .position(|t| t.text.contains("YELLOW"))
        .unwrap_or_else(|| panic!("YELLOW missing\n{dump}"));
    let bands = result.overlay_split.page_stack_band_ranges();
    let blur_at = result
        .instances
        .iter()
        .position(|i| i.backdrop_blur > 0.5)
        .unwrap_or_else(|| panic!("no blur instance\n{dump}"));
    let blur_band = bands
        .iter()
        .position(|b| b.instance.0 <= blur_at && blur_at < b.instance.1)
        .unwrap_or_else(|| panic!("blur instance not in a page_stack band\n{dump}"));
    let text_band = bands
        .iter()
        .position(|b| b.text.0 <= yellow_at && yellow_at < b.text.1)
        .unwrap_or_else(|| panic!("YELLOW not in a page_stack band\n{dump}"));
    let host_blue = result
        .instances
        .iter()
        .position(|i| i.color[2] > 0.7 && i.color[0] < 0.3 && i.color[1] < 0.5 && i.color[3] > 0.9);
    let host_in_prefix = host_blue.is_some_and(|p| p < blur_at);
    eprintln!(
        "[local-scrim] blur_band={blur_band} text_band={text_band} host_in_prefix={host_in_prefix} host_blue={host_blue:?} capture_draws={} bands={}\n{dump}",
        capture_draws_band_text(blur_band, text_band),
        bands.len()
    );
    let dump_path = std::env::temp_dir().join("local_scrim_dump.txt");
    let _ = std::fs::write(
        &dump_path,
        format!(
            "blur_band={blur_band} text_band={text_band} host_in_prefix={host_in_prefix}\n{dump}"
        ),
    );
    assert!(
        capture_draws_band_text(blur_band, text_band),
        "capture_page_stack_local_backdrop would skip YELLOW glyphon \
         (blur_band={blur_band} text_band={text_band} host_in_prefix={host_in_prefix} host_blue={host_blue:?})\n{dump}"
    );
    let prep_lo = bands[text_band].text.0.min(overlay_text_start);
    let prep_hi = bands[text_band].text.1.min(overlay_text_start);
    assert!(
        prep_lo <= yellow_at && yellow_at < prep_hi,
        "YELLOW prepare window empty for capture renderer\n{dump}"
    );
}

#[test]
fn gallery_scrollarea_shell_capture_includes_yellow_glyphon() {
    // This is the tree shape that actually ships in GallerySectionOverlays, not the
    // isolated host used by local_backdrop_blur_splits_from_host_text_inside_nested_stack.
    let root = Element::new().child(gallery_overlays_scroll_shell(gallery_local_scrim_host()));
    assert_yellow_lands_in_capture_prefix(root);
}

#[test]
fn gallery_real_scroll_area_capture_includes_yellow_glyphon() {
    use crate::components::scroll_area::scroll_area;
    use crate::core::reactivity::{create_effect, create_root};
    use crate::primitives::r#box::vstack;

    let (_handle, _) = create_root(|| {
        create_effect(|| {
            let body = vstack()
                .w("100%")
                .gap(16.0)
                .p(24.0)
                .child(Element::from(label("above").text("ViewportModal chrome")))
                .child(gallery_local_scrim_host());
            let scroll = Element::from(
                scroll_area("scroll-overlays")
                    .flex_grow(1.0)
                    .min_h(0.0)
                    .overflow_y("auto")
                    .overflow_x("auto")
                    .child(body),
            );
            let mut page = Element::new().w(800.0).h(600.0).child(scroll);
            page.set_attr("data-stacking-context", "1");
            assert_yellow_lands_in_capture_prefix(Element::new().child(page));
        });
    });
}

#[test]
#[cfg_attr(
    not(any(
        all(windows, feature = "backend-dx12"),
        all(target_vendor = "apple", feature = "backend-metal"),
        feature = "backend-gles",
        feature = "backend-vulkan",
    )),
    ignore = "requires a wgpu backend for this target"
)]
fn gallery_yellow_glyphon_prepare_is_not_culled() {
    use crate::renderer::offscreen::OffscreenGpu;
    use crate::renderer::prepare_text::prepare_text_frame;
    use crate::renderer::JadixTextRenderer;
    use std::collections::{HashMap, HashSet};
    use wgpu::util::DeviceExt;

    let Some(gpu) = OffscreenGpu::new() else {
        eprintln!("skip gallery_yellow_glyphon_prepare_is_not_culled: no wgpu adapter");
        return;
    };

    let mut root = Element::new().child(gallery_local_scrim_tree());
    assert!(hoist_portal_subtrees(&mut root) >= 1);

    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut root, 800.0, 600.0, [-1.0, -1.0]);
    let dump = dump_split(&result);
    let overlay_text_start = result.overlay_split.text_start.min(result.text_nodes.len());
    let bands = result.overlay_split.page_stack_band_ranges();
    let yellow_at = result
        .text_nodes
        .iter()
        .position(|t| t.text.contains("YELLOW"))
        .unwrap_or_else(|| panic!("YELLOW missing\n{dump}"));
    let text_band = bands
        .iter()
        .position(|b| b.text.0 <= yellow_at && yellow_at < b.text.1)
        .unwrap_or_else(|| panic!("YELLOW not in a band\n{dump}"));
    let lo = bands[text_band].text.0.min(overlay_text_start);
    let hi = bands[text_band].text.1.min(overlay_text_start);
    assert!(lo < hi, "empty prepare window for YELLOW band\n{dump}");

    let config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        width: 800,
        height: 600,
        present_mode: wgpu::PresentMode::Fifo,
        desired_maximum_frame_latency: 2,
        alpha_mode: wgpu::CompositeAlphaMode::Auto,
        color_space: wgpu::SurfaceColorSpace::Auto,
        view_formats: vec![],
    };
    let mut text_renderer = JadixTextRenderer::new(&gpu.device, &gpu.queue, &config);
    let prep = prepare_text_frame(
        &mut text_renderer,
        &gpu.device,
        &gpu.queue,
        &config,
        &result.text_nodes[lo..hi],
        1.0,
        false,
        &HashSet::new(),
        &HashMap::new(),
    );
    assert!(
        !prep.culled_skip_prepare,
        "YELLOW band prepare was fully clip-culled — capture would draw empty glyphon\n{dump}"
    );

    let yellow = &result.text_nodes[yellow_at];
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("local-scrim-scene"),
        size: wgpu::Extent3d {
            width: 800,
            height: 600,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let mut encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("local-scrim-scene-enc"),
        });
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Local Backdrop Scene Pass (diag)"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        text_renderer.render(&mut pass);
    }
    let img = crate::renderer::offscreen::readback_texture(
        &gpu.device,
        &gpu.queue,
        &texture,
        800,
        600,
        encoder,
    )
    .expect("readback");
    let x0 = yellow.x.floor().max(0.0) as u32;
    let y0 = yellow.y.floor().max(0.0) as u32;
    let x1 = (yellow.x + yellow.width.max(1.0)).ceil().min(800.0) as u32;
    let y1 = (yellow.y + yellow.height.max(1.0)).ceil().min(600.0) as u32;
    let count_yellow = |img: &crate::renderer::offscreen::RgbaImage| -> (u32, [u8; 4]) {
        let mut yellow_px = 0u32;
        let mut sample = [0u8; 4];
        for y in y0..y1 {
            for x in x0..x1 {
                if let Some(p) = img.pixel(x, y) {
                    if p[0] > 180 && p[1] > 140 && p[2] < 140 && p[3] > 8 {
                        yellow_px += 1;
                        sample = p;
                    }
                }
            }
        }
        (yellow_px, sample)
    };
    let (yellow_only, sample_only) = count_yellow(&img);
    let gpu_log = format!(
        "glyphon-only yellow_px={yellow_only} sample={sample_only:?} box=({x0},{y0})-({x1},{y1})\n{dump}\n"
    );
    let dump_path = std::env::temp_dir().join("local_scrim_gpu.txt");
    let _ = std::fs::write(&dump_path, &gpu_log);
    assert!(
        yellow_only > 20,
        "glyphon-only offscreen has no yellow ink (px={yellow_only} sample={sample_only:?})\n{dump}"
    );

    // Same pass as capture: prefix SDF (everything before the frost instance) then glyphon.
    let blur_at = result
        .instances
        .iter()
        .position(|i| i.backdrop_blur > 0.5)
        .unwrap_or(result.instances.len());
    let prefix = &result.instances[..blur_at];
    let pipeline = crate::renderer::JadixPipeline::from_format(
        &gpu.device,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        800,
        600,
    );
    pipeline.update_resolution(&gpu.queue, 800, 600);
    let inst_buf = gpu
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("prefix-sdf"),
            contents: bytemuck::cast_slice(prefix),
            usage: wgpu::BufferUsages::VERTEX,
        });
    let texture2 = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("local-scrim-scene-sdf"),
        size: wgpu::Extent3d {
            width: 800,
            height: 600,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view2 = texture2.create_view(&wgpu::TextureViewDescriptor::default());
    let mut encoder2 = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("local-scrim-sdf-enc"),
        });
    {
        let mut pass = encoder2.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Local Backdrop Scene Pass (sdf+glyphon)"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view2,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        if !prefix.is_empty() {
            pipeline.bind_sdf(&mut pass, true, pipeline.dummy_backdrop());
            pass.set_vertex_buffer(0, pipeline.vertex_buffer.slice(..));
            pass.set_vertex_buffer(1, inst_buf.slice(..));
            pass.set_index_buffer(pipeline.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
            pass.draw_indexed(0..pipeline.num_indices, 0, 0..prefix.len() as u32);
        }
        text_renderer.render(&mut pass);
    }
    let img2 = crate::renderer::offscreen::readback_texture(
        &gpu.device,
        &gpu.queue,
        &texture2,
        800,
        600,
        encoder2,
    )
    .expect("readback sdf+glyphon");
    let (yellow_after_sdf, sample_sdf) = count_yellow(&img2);
    let mut blue_px = 0u32;
    for y in y0..y1 {
        for x in x0..x1 {
            if let Some(p) = img2.pixel(x, y) {
                if p[2] > 140 && p[0] < 120 && p[3] > 200 {
                    blue_px += 1;
                }
            }
        }
    }
    let _ = std::fs::write(
        std::env::temp_dir().join("local_scrim_gpu.txt"),
        format!(
            "{gpu_log}sdf-then-glyphon yellow_px={yellow_after_sdf} blue_px={blue_px} sample={sample_sdf:?} prefix_insts={}\n",
            prefix.len()
        ),
    );
    assert!(
        yellow_after_sdf > 20,
        "prefix SDF then glyphon lost yellow (px={yellow_after_sdf} blue={blue_px} sample={sample_sdf:?}) — later prefix fills cover the glyphs\n{dump}"
    );

    // Production local frost: same prefix → scene RT → Gaussian → sample in the scrim SDF.
    let mut backdrop = crate::renderer::BackdropBlurPipeline::new(
        &gpu.device,
        wgpu::TextureFormat::Rgba8UnormSrgb,
    );
    backdrop.ensure_size(&gpu.device, 800, 600, wgpu::TextureFormat::Rgba8UnormSrgb);
    {
        let scene_view = backdrop.scene_view().expect("scene");
        let mut encoder = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("local-scrim-backdrop-scene"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("backdrop scene"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: scene_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            if !prefix.is_empty() {
                pipeline.bind_sdf(&mut pass, true, pipeline.dummy_backdrop());
                pass.set_vertex_buffer(0, pipeline.vertex_buffer.slice(..));
                pass.set_vertex_buffer(1, inst_buf.slice(..));
                pass.set_index_buffer(pipeline.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
                pass.draw_indexed(0..pipeline.num_indices, 0, 0..prefix.len() as u32);
            }
            text_renderer.render(&mut pass);
        }
        backdrop.blur(&gpu.device, &gpu.queue, &mut encoder, 16.0, None);
        // Production submits the capture encoder *before* anyone copies `full`.
        // Ping-pong Gaussian + COPY_SRC in the same GLES encoder leaves `full` as
        // the Clear (a=0) — this assert then reports sample=[0,0,0,0].
        gpu.queue.submit(Some(encoder.finish()));
        let full_tex = backdrop.blurred_texture().expect("full").clone();
        let read_enc = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("local-scrim-blur-readback"),
            });
        let img_blur = crate::renderer::offscreen::readback_texture(
            &gpu.device,
            &gpu.queue,
            &full_tex,
            800,
            600,
            read_enc,
        )
        .expect("readback blur");
        let (yellow_blur, sample_blur) = count_yellow(&img_blur);
        let mut smear = 0u32;
        let mut opaque = 0u32;
        let corner = img_blur.pixel(x0, y0).unwrap_or([0; 4]);
        for y in y0..y1 {
            for x in x0..x1 {
                if let Some(p) = img_blur.pixel(x, y) {
                    if p[3] > 8 {
                        opaque += 1;
                    }
                    if p[0] > p[2] && p[1] > 80 || pixel_is_yellow_smear(p) {
                        smear += 1;
                    }
                }
            }
        }
        let _ = std::fs::write(
            std::env::temp_dir().join("local_scrim_gpu.txt"),
            format!(
                "{gpu_log}sdf-then-glyphon yellow_px={yellow_after_sdf} blue_px={blue_px}\nblur yellow_px={yellow_blur} smear_r>b={smear} opaque={opaque} corner={corner:?} sample={sample_blur:?} adapter={:?}\n",
                gpu.adapter_info
            ),
        );
        assert!(
            smear > 20 || yellow_blur > 5,
            "blur wiped host glyphs (yellow={yellow_blur} smear={smear} opaque={opaque} corner={corner:?} sample={sample_blur:?})\n{dump}"
        );

        let frost_inst = result
            .instances
            .iter()
            .copied()
            .find(|i| i.backdrop_blur > 0.5)
            .expect("frost instance");
        let frost_bg = pipeline.make_backdrop_bind_group(
            &gpu.device,
            backdrop.blurred_view().expect("blur view"),
            backdrop.sampler(),
        );
        let frost_buf = gpu
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("frost-inst"),
                contents: bytemuck::cast_slice(&[frost_inst]),
                usage: wgpu::BufferUsages::VERTEX,
            });
        let texture_f = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("frost-out"),
            size: wgpu::Extent3d {
                width: 800,
                height: 600,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view_f = texture_f.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder_f = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frost-enc"),
            });
        {
            let mut pass = encoder_f.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("frost pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view_f,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pipeline.bind_sdf(&mut pass, true, &frost_bg);
            pass.set_vertex_buffer(0, pipeline.vertex_buffer.slice(..));
            pass.set_vertex_buffer(1, frost_buf.slice(..));
            pass.set_index_buffer(pipeline.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
            pass.draw_indexed(0..pipeline.num_indices, 0, 0..1);
        }
        gpu.queue.submit(Some(encoder_f.finish()));
        let read_f = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frost-readback"),
            });
        let img_f = crate::renderer::offscreen::readback_texture(
            &gpu.device,
            &gpu.queue,
            &texture_f,
            800,
            600,
            read_f,
        )
        .expect("readback frost");
        let fx0 = frost_inst.rect_pos[0].floor().max(0.0) as u32;
        let fy0 = frost_inst.rect_pos[1].floor().max(0.0) as u32;
        let fx1 = (frost_inst.rect_pos[0] + frost_inst.rect_size[0])
            .ceil()
            .min(800.0) as u32;
        let fy1 = (frost_inst.rect_pos[1] + frost_inst.rect_size[1])
            .ceil()
            .min(600.0) as u32;
        let cx = (fx0 + fx1) / 2;
        let cy = (fy0 + fy1) / 2;
        let frost_center = img_f.pixel(cx, cy).unwrap_or([0; 4]);
        let mut frost_smear = 0u32;
        let mut frost_sample = frost_center;
        let decode = |channel: f32| {
            if channel <= 0.04045 {
                channel / 12.92
            } else {
                ((channel + 0.055) / 1.055).powf(2.4)
            }
        };
        let encode = |channel: f32| {
            if channel <= 0.0031308 {
                channel * 12.92
            } else {
                1.055 * channel.powf(1.0 / 2.4) - 0.055
            }
        };
        let mut interior = 0;
        for y in fy0 + 16..fy1 - 16 {
            for x in fx0 + 16..fx1 - 16 {
                if let Some(p) = img_f.pixel(x, y) {
                    let source = img_blur.pixel(x, y).unwrap();
                    let matches = (0..3).all(|channel| {
                        let expected = encode(
                            decode(f32::from(source[channel]) / 255.0)
                                * (1.0 - frost_inst.color[3])
                                + decode(frost_inst.color[channel]) * frost_inst.color[3],
                        ) * 255.0;
                        (f32::from(p[channel]) - expected).abs() <= 2.0
                    });
                    interior += 1;
                    if matches && p[3] == 255 {
                        frost_smear += 1;
                        frost_sample = p;
                    }
                }
            }
        }
        let _ = std::fs::write(
            std::env::temp_dir().join("local_scrim_gpu.txt"),
            format!(
                "{gpu_log}sdf-then-glyphon yellow_px={yellow_after_sdf}\nblur yellow_px={yellow_blur} smear={smear} sample={sample_blur:?}\nfrost smear_r>b={frost_smear} center={frost_center:?} sample={frost_sample:?} rect=({fx0},{fy0})-({fx1},{fy1}) inst_a={}\n",
                frost_inst.color[3]
            ),
        );
        assert!(
            frost_smear * 100 >= interior * 99,
            "frost must preserve captured glyph detail with linear-light tint compositing (matched={frost_smear}/{interior} center={frost_center:?} sample={frost_sample:?})\n{dump}"
        );

        // RSX unitless `"16"` is 16rem (clamped to 64px). Gallery writes `"1"` (1rem = 16px).
        // The 64px kernel washes 18px glyphs; builder 16px must not.
        let mut encoder_64 = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("local-scrim-backdrop-64"),
            });
        backdrop.blur(&gpu.device, &gpu.queue, &mut encoder_64, 64.0, None);
        gpu.queue.submit(Some(encoder_64.finish()));
        let full_64 = backdrop.blurred_texture().expect("full 64").clone();
        let read_64 = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("local-scrim-blur-64-readback"),
            });
        let img_64 = crate::renderer::offscreen::readback_texture(
            &gpu.device,
            &gpu.queue,
            &full_64,
            800,
            600,
            read_64,
        )
        .expect("readback blur 64");
        let mut smear_64 = 0u32;
        let mut sample_64 = [0u8; 4];
        for y in y0..y1 {
            for x in x0..x1 {
                if let Some(p) = img_64.pixel(x, y) {
                    sample_64 = p;
                    if p[0] > p[2] && p[1] > 80 {
                        smear_64 += 1;
                    }
                }
            }
        }
        assert!(
            smear > smear_64.saturating_mul(3) && smear > 20,
            "16px frost must keep glyph chroma that a 64px kernel (RSX unitless \"16\" = 16rem, clamped) washes out \
             (smear16={smear} smear64={smear_64} sample16={sample_blur:?} sample64={sample_64:?})\n{dump}"
        );
    }
}

#[test]
#[cfg_attr(
    not(any(
        all(windows, feature = "backend-dx12"),
        all(target_vendor = "apple", feature = "backend-metal"),
        feature = "backend-gles",
        feature = "backend-vulkan",
    )),
    ignore = "requires a wgpu backend for this target"
)]
fn gallery_capture_interleaved_prefix_bands_keep_yellow() {
    use crate::renderer::offscreen::OffscreenGpu;
    use crate::renderer::prepare_text::prepare_text_frame;
    use crate::renderer::JadixTextRenderer;
    use std::collections::{HashMap, HashSet};
    use wgpu::util::DeviceExt;

    let Some(gpu) = OffscreenGpu::new() else {
        return;
    };
    let mut root = Element::new().child(gallery_overlays_scroll_shell(gallery_local_scrim_host()));
    assert!(hoist_portal_subtrees(&mut root) >= 1);
    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut root, 800.0, 600.0, [-1.0, -1.0]);
    let dump = dump_split(&result);
    let overlay_text_start = result.overlay_split.text_start.min(result.text_nodes.len());
    let bands = result.overlay_split.page_stack_band_ranges();
    let yellow_at = result
        .text_nodes
        .iter()
        .position(|t| t.text.contains("YELLOW"))
        .unwrap();
    let yellow = &result.text_nodes[yellow_at];
    let blur_band = bands
        .iter()
        .position(|b| {
            (b.instance.0..b.instance.1).any(|i| {
                result
                    .instances
                    .get(i)
                    .is_some_and(|n| n.backdrop_blur > 0.5)
            })
        })
        .unwrap();
    let config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        width: 800,
        height: 600,
        present_mode: wgpu::PresentMode::Fifo,
        desired_maximum_frame_latency: 2,
        alpha_mode: wgpu::CompositeAlphaMode::Auto,
        color_space: wgpu::SurfaceColorSpace::Auto,
        view_formats: vec![],
    };
    let mut text_renderer = JadixTextRenderer::new(&gpu.device, &gpu.queue, &config);
    let font_ptr = text_renderer.font_system_mut() as *mut _;
    let mut extra: Vec<JadixTextRenderer> = Vec::new();
    for _ in 1..blur_band.max(1) {
        extra.push(JadixTextRenderer::new_sharing_font_system(
            &gpu.device,
            &gpu.queue,
            &config,
            font_ptr,
        ));
    }
    let empty_keys = HashSet::new();
    let empty_map = HashMap::new();
    for (band_i, band) in bands.iter().enumerate().take(blur_band) {
        let lo = band.text.0.min(overlay_text_start);
        let hi = band.text.1.min(overlay_text_start);
        if lo >= hi {
            continue;
        }
        let nodes = &result.text_nodes[lo..hi];
        if band_i == 0 {
            prepare_text_frame(
                &mut text_renderer,
                &gpu.device,
                &gpu.queue,
                &config,
                nodes,
                1.0,
                false,
                &empty_keys,
                &empty_map,
            );
        } else {
            prepare_text_frame(
                &mut extra[band_i - 1],
                &gpu.device,
                &gpu.queue,
                &config,
                nodes,
                1.0,
                false,
                &empty_keys,
                &empty_map,
            );
        }
    }
    let pipeline = crate::renderer::JadixPipeline::from_format(
        &gpu.device,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        800,
        600,
    );
    pipeline.update_resolution(&gpu.queue, 800, 600);
    let inst_buf = gpu
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("all-inst"),
            contents: bytemuck::cast_slice(&result.instances),
            usage: wgpu::BufferUsages::VERTEX,
        });
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("interleaved-scene"),
        size: wgpu::Extent3d {
            width: 800,
            height: 600,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let mut encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("interleaved-enc"),
        });
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("interleaved capture"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        for (j, band) in bands.iter().enumerate().take(blur_band) {
            if band.instance.0 < band.instance.1 {
                pipeline.bind_sdf(&mut pass, true, pipeline.dummy_backdrop());
                pass.set_vertex_buffer(0, pipeline.vertex_buffer.slice(..));
                pass.set_vertex_buffer(1, inst_buf.slice(..));
                pass.set_index_buffer(pipeline.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
                pass.draw_indexed(
                    0..pipeline.num_indices,
                    0,
                    band.instance.0 as u32..band.instance.1 as u32,
                );
            }
            if band.text.0 < band.text.1 {
                if j == 0 {
                    text_renderer.render(&mut pass);
                } else {
                    extra[j - 1].render(&mut pass);
                }
            }
        }
    }
    let img = crate::renderer::offscreen::readback_texture(
        &gpu.device,
        &gpu.queue,
        &texture,
        800,
        600,
        encoder,
    )
    .expect("readback");
    let x0 = yellow.x.floor().max(0.0) as u32;
    let y0 = yellow.y.floor().max(0.0) as u32;
    let x1 = (yellow.x + yellow.width.max(1.0)).ceil().min(800.0) as u32;
    let y1 = (yellow.y + yellow.height.max(1.0)).ceil().min(600.0) as u32;
    let mut yellow_px = 0u32;
    let mut sample = [0u8; 4];
    for y in y0..y1 {
        for x in x0..x1 {
            if let Some(p) = img.pixel(x, y) {
                sample = p;
                if p[0] > 180 && p[1] > 140 && p[2] < 140 && p[3] > 8 {
                    yellow_px += 1;
                }
            }
        }
    }
    let _ = std::fs::write(
        std::env::temp_dir().join("local_scrim_interleaved.txt"),
        format!(
            "yellow_px={yellow_px} sample={sample:?} blur_band={blur_band} box=({x0},{y0})-({x1},{y1})\n{dump}"
        ),
    );
    assert!(
        yellow_px > 20,
        "interleaved prefix capture lost yellow (px={yellow_px} sample={sample:?} blur_band={blur_band})\n{dump}"
    );
}

fn pixel_is_yellow_smear(p: [u8; 4]) -> bool {
    // RGBA yellow, or raw BGRA bytes from a Bgra8 copy (R/B swapped).
    let a = p[3] > 8;
    let g = p[1] > 140;
    let rgba = p[0] > 180 && p[2] < 140;
    let bgra = p[2] > 180 && p[0] < 140;
    a && g && (rgba || bgra)
}

/// Windows Gallery GL swapchain is typically `Bgra8UnormSrgb`. Isolated tests used
/// `Rgba8UnormSrgb` — if glyphon mask glyphs cannot write that offscreen format,
/// frost samples host blue only. One variable: format.
#[test]
#[cfg_attr(
    not(any(
        all(windows, feature = "backend-dx12"),
        all(target_vendor = "apple", feature = "backend-metal"),
        feature = "backend-gles",
        feature = "backend-vulkan",
    )),
    ignore = "requires a wgpu backend for this target"
)]
fn gallery_glyphon_draws_to_bgra_offscreen_rt() {
    use crate::renderer::offscreen::OffscreenGpu;
    use crate::renderer::prepare_text::prepare_text_frame;
    use crate::renderer::JadixTextRenderer;
    use std::collections::{HashMap, HashSet};

    let Some(gpu) = OffscreenGpu::new() else {
        return;
    };
    let mut root = Element::new().child(gallery_overlays_scroll_shell(gallery_local_scrim_host()));
    assert!(hoist_portal_subtrees(&mut root) >= 1);
    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut root, 800.0, 600.0, [-1.0, -1.0]);
    let dump = dump_split(&result);
    let overlay_text_start = result.overlay_split.text_start.min(result.text_nodes.len());
    let bands = result.overlay_split.page_stack_band_ranges();
    let yellow_at = result
        .text_nodes
        .iter()
        .position(|t| t.text.contains("YELLOW"))
        .unwrap();
    let yellow = &result.text_nodes[yellow_at];
    let text_band = bands
        .iter()
        .position(|b| b.text.0 <= yellow_at && yellow_at < b.text.1)
        .unwrap();
    let lo = bands[text_band].text.0.min(overlay_text_start);
    let hi = bands[text_band].text.1.min(overlay_text_start);
    let format = wgpu::TextureFormat::Bgra8UnormSrgb;
    let config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format,
        width: 800,
        height: 600,
        present_mode: wgpu::PresentMode::Fifo,
        desired_maximum_frame_latency: 2,
        alpha_mode: wgpu::CompositeAlphaMode::Auto,
        color_space: wgpu::SurfaceColorSpace::Auto,
        view_formats: vec![],
    };
    let mut text_renderer = JadixTextRenderer::new(&gpu.device, &gpu.queue, &config);
    let prep = prepare_text_frame(
        &mut text_renderer,
        &gpu.device,
        &gpu.queue,
        &config,
        &result.text_nodes[lo..hi],
        1.0,
        false,
        &HashSet::new(),
        &HashMap::new(),
    );
    assert!(
        !prep.culled_skip_prepare,
        "YELLOW prepare culled before Bgra draw\n{dump}"
    );
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("bgra-glyphon-rt"),
        size: wgpu::Extent3d {
            width: 800,
            height: 600,
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
    let mut encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("bgra-glyphon-enc"),
        });
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("bgra glyphon"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        text_renderer.render(&mut pass);
    }
    let img = crate::renderer::offscreen::readback_texture(
        &gpu.device,
        &gpu.queue,
        &texture,
        800,
        600,
        encoder,
    )
    .expect("readback Bgra glyphon");
    let x0 = yellow.x.floor().max(0.0) as u32;
    let y0 = yellow.y.floor().max(0.0) as u32;
    let x1 = (yellow.x + yellow.width.max(1.0)).ceil().min(800.0) as u32;
    let y1 = (yellow.y + yellow.height.max(1.0)).ceil().min(600.0) as u32;
    let mut yellow_px = 0u32;
    let mut sample = [0u8; 4];
    let mut any_nonzero = 0u32;
    for y in y0..y1 {
        for x in x0..x1 {
            if let Some(p) = img.pixel(x, y) {
                sample = p;
                if p[3] > 8 {
                    any_nonzero += 1;
                }
                if pixel_is_yellow_smear(p) {
                    yellow_px += 1;
                }
            }
        }
    }
    let _ = std::fs::write(
        std::env::temp_dir().join("local_scrim_bgra.txt"),
        format!(
            "yellow_px={yellow_px} any_a={any_nonzero} sample={sample:?} box=({x0},{y0})-({x1},{y1})\n{dump}"
        ),
    );
    assert!(
        yellow_px > 20,
        "glyphon wrote no yellow into Bgra8UnormSrgb offscreen RT \
         (px={yellow_px} any_a={any_nonzero} sample={sample:?}) — Gallery surface format hypothesis\n{dump}"
    );
}

fn count_yellow_box(
    img: &crate::renderer::offscreen::RgbaImage,
    yellow: &crate::layout::layout_tree::TextLayoutInfo,
    scale: f32,
    w: u32,
    h: u32,
) -> (u32, [u8; 4]) {
    let x0 = (yellow.x * scale).floor().max(0.0) as u32;
    let y0 = (yellow.y * scale).floor().max(0.0) as u32;
    let x1 = ((yellow.x + yellow.width.max(1.0)) * scale)
        .ceil()
        .min(w as f32) as u32;
    let y1 = ((yellow.y + yellow.height.max(1.0)) * scale)
        .ceil()
        .min(h as f32) as u32;
    let mut yellow_px = 0u32;
    let mut sample = [0u8; 4];
    for y in y0..y1 {
        for x in x0..x1 {
            if let Some(p) = img.pixel(x, y) {
                sample = p;
                if pixel_is_yellow_smear(p) {
                    yellow_px += 1;
                }
            }
        }
    }
    (yellow_px, sample)
}

/// AppState: prepare once, `render` into the capture encoder, submit, then `render`
/// the same glyphon buffers into the surface pass. Isolated tests only rendered once.
#[test]
#[cfg_attr(
    not(any(
        all(windows, feature = "backend-dx12"),
        all(target_vendor = "apple", feature = "backend-metal"),
        feature = "backend-gles",
        feature = "backend-vulkan",
    )),
    ignore = "requires a wgpu backend for this target"
)]
fn gallery_glyphon_second_render_does_not_empty_first_capture() {
    use crate::renderer::offscreen::OffscreenGpu;
    use crate::renderer::prepare_text::prepare_text_frame;
    use crate::renderer::JadixTextRenderer;
    use std::collections::{HashMap, HashSet};

    let Some(gpu) = OffscreenGpu::new() else {
        return;
    };
    let mut root = Element::new().child(gallery_overlays_scroll_shell(gallery_local_scrim_host()));
    assert!(hoist_portal_subtrees(&mut root) >= 1);
    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut root, 800.0, 600.0, [-1.0, -1.0]);
    let dump = dump_split(&result);
    let overlay_text_start = result.overlay_split.text_start.min(result.text_nodes.len());
    let bands = result.overlay_split.page_stack_band_ranges();
    let yellow_at = result
        .text_nodes
        .iter()
        .position(|t| t.text.contains("YELLOW"))
        .unwrap();
    let yellow = &result.text_nodes[yellow_at];
    let text_band = bands
        .iter()
        .position(|b| b.text.0 <= yellow_at && yellow_at < b.text.1)
        .unwrap();
    let lo = bands[text_band].text.0.min(overlay_text_start);
    let hi = bands[text_band].text.1.min(overlay_text_start);
    let config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        width: 800,
        height: 600,
        present_mode: wgpu::PresentMode::Fifo,
        desired_maximum_frame_latency: 2,
        alpha_mode: wgpu::CompositeAlphaMode::Auto,
        color_space: wgpu::SurfaceColorSpace::Auto,
        view_formats: vec![],
    };
    let mut text_renderer = JadixTextRenderer::new(&gpu.device, &gpu.queue, &config);
    prepare_text_frame(
        &mut text_renderer,
        &gpu.device,
        &gpu.queue,
        &config,
        &result.text_nodes[lo..hi],
        1.0,
        false,
        &HashSet::new(),
        &HashMap::new(),
    );
    let mk_rt = |label: &str| {
        gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width: 800,
                height: 600,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        })
    };
    let cap_tex = mk_rt("double-render-capture");
    let surf_tex = mk_rt("double-render-surface");
    let cap_view = cap_tex.create_view(&wgpu::TextureViewDescriptor::default());
    let surf_view = surf_tex.create_view(&wgpu::TextureViewDescriptor::default());

    let mut cap_enc = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("double-render-cap"),
        });
    {
        let mut pass = cap_enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("capture like AppState"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &cap_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        text_renderer.render(&mut pass);
    }
    gpu.queue.submit(std::iter::once(cap_enc.finish()));

    let mut surf_enc = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("double-render-surf"),
        });
    {
        let mut pass = surf_enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("surface like AppState"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &surf_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        text_renderer.render(&mut pass);
    }
    gpu.queue.submit(std::iter::once(surf_enc.finish()));

    let cap_img = crate::renderer::offscreen::readback_texture(
        &gpu.device,
        &gpu.queue,
        &cap_tex,
        800,
        600,
        gpu.device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None }),
    )
    .expect("readback capture");
    let surf_img = crate::renderer::offscreen::readback_texture(
        &gpu.device,
        &gpu.queue,
        &surf_tex,
        800,
        600,
        gpu.device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None }),
    )
    .expect("readback surface");
    let (cap_y, cap_s) = count_yellow_box(&cap_img, yellow, 1.0, 800, 600);
    let (surf_y, surf_s) = count_yellow_box(&surf_img, yellow, 1.0, 800, 600);
    let _ = std::fs::write(
        std::env::temp_dir().join("local_scrim_double_render.txt"),
        format!(
            "capture yellow={cap_y} sample={cap_s:?}\nsurface yellow={surf_y} sample={surf_s:?}\n{dump}"
        ),
    );
    assert!(
        cap_y > 20,
        "first glyphon render (capture) empty after a second render was also recorded \
         (cap={cap_y} {cap_s:?} surf={surf_y} {surf_s:?})\n{dump}"
    );
    assert!(
        surf_y > 20,
        "second glyphon render (surface) empty (cap={cap_y} surf={surf_y} {surf_s:?})\n{dump}"
    );
}

/// Physical pixels in Gallery are `logical * ui_scale`. Tests used scale=1.
#[test]
#[cfg_attr(
    not(any(
        all(windows, feature = "backend-dx12"),
        all(target_vendor = "apple", feature = "backend-metal"),
        feature = "backend-gles",
        feature = "backend-vulkan",
    )),
    ignore = "requires a wgpu backend for this target"
)]
fn gallery_glyphon_capture_keeps_yellow_at_ui_scale_1_5() {
    use crate::renderer::offscreen::OffscreenGpu;
    use crate::renderer::prepare_text::prepare_text_frame;
    use crate::renderer::JadixTextRenderer;
    use std::collections::{HashMap, HashSet};

    let Some(gpu) = OffscreenGpu::new() else {
        return;
    };
    let mut root = Element::new().child(gallery_overlays_scroll_shell(gallery_local_scrim_host()));
    assert!(hoist_portal_subtrees(&mut root) >= 1);
    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut root, 800.0, 600.0, [-1.0, -1.0]);
    let dump = dump_split(&result);
    let overlay_text_start = result.overlay_split.text_start.min(result.text_nodes.len());
    let bands = result.overlay_split.page_stack_band_ranges();
    let yellow_at = result
        .text_nodes
        .iter()
        .position(|t| t.text.contains("YELLOW"))
        .unwrap();
    let yellow = &result.text_nodes[yellow_at];
    let text_band = bands
        .iter()
        .position(|b| b.text.0 <= yellow_at && yellow_at < b.text.1)
        .unwrap();
    let lo = bands[text_band].text.0.min(overlay_text_start);
    let hi = bands[text_band].text.1.min(overlay_text_start);
    let scale = 1.5_f32;
    let pw = (800.0 * scale).round() as u32;
    let ph = (600.0 * scale).round() as u32;
    let config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        width: pw,
        height: ph,
        present_mode: wgpu::PresentMode::Fifo,
        desired_maximum_frame_latency: 2,
        alpha_mode: wgpu::CompositeAlphaMode::Auto,
        color_space: wgpu::SurfaceColorSpace::Auto,
        view_formats: vec![],
    };
    let mut text_renderer = JadixTextRenderer::new(&gpu.device, &gpu.queue, &config);
    prepare_text_frame(
        &mut text_renderer,
        &gpu.device,
        &gpu.queue,
        &config,
        &result.text_nodes[lo..hi],
        scale,
        false,
        &HashSet::new(),
        &HashMap::new(),
    );
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("scale-1.5-rt"),
        size: wgpu::Extent3d {
            width: pw,
            height: ph,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let mut encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("scale-1.5-enc"),
        });
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("scale 1.5 glyphon"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        text_renderer.render(&mut pass);
    }
    let img = crate::renderer::offscreen::readback_texture(
        &gpu.device,
        &gpu.queue,
        &texture,
        pw,
        ph,
        encoder,
    )
    .expect("readback scale 1.5");
    let (yellow_px, sample) = count_yellow_box(&img, yellow, scale, pw, ph);
    let _ = std::fs::write(
        std::env::temp_dir().join("local_scrim_scale15.txt"),
        format!("yellow_px={yellow_px} sample={sample:?} scale={scale} rt={pw}x{ph}\n{dump}"),
    );
    assert!(
        yellow_px > 20,
        "ui_scale=1.5 capture lost yellow (px={yellow_px} sample={sample:?})\n{dump}"
    );
}
