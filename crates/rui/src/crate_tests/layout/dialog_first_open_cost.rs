//! First vs second Dialog-open cost: CPU layout vs first GPU draw of blur / capture text.
//!
//! The Gallery symptom is a ~500ms empty wait on the process's first Dialog, then instant
//! later opens. These timings decide whether that wait is Taffy/body() or a cold GPU path.

use std::time::Instant;

use crate::app::reactive::{drain_deferred_view_remounts, reset_registered_anchors};
use crate::app::runtime::{clear_test_reconcile_root, install_test_reconcile_root};
use crate::components::dialog::{dialog, dialog_content, DialogContentOpts};
use crate::components::internal::modal_motion::{
    flush_modal_shell_motion_subtree, tick_hoisted_modal_css,
};
use crate::core::reactivity::{create_effect, create_root, use_signal};
use crate::core::tree::{Element, LayoutTree};
use crate::layout::layout_tree::hoist_portal_subtrees;
use crate::renderer::font_loader::{
    apply_default_families, create_font_system, ensure_coverage_for_text,
};

fn has_tid(el: &Element, id: &str) -> bool {
    if el.template_id.as_deref() == Some(id) {
        return true;
    }
    el.children.iter().any(|c| has_tid(c, id))
}

/// Gallery-like Dialog body (Chinese + several rows). Select is omitted so this crate
/// test does not depend on the Select silent-`body()` path; the gallery mount test covers that.
fn gallery_like_dialog_body() -> Element {
    Element::new().flex_col().gap(8.0).w("100%").children([
        Element::new().text("Gallery Dialog"),
        Element::new().text("desktop_smoke #7：Body 内含 Select；先开下拉再 Esc，应只关顶层浮层。"),
        Element::new().text("嵌套浮层（Select）"),
        Element::new().text("行 1 — 演示长内容"),
        Element::new().text("行 2"),
        Element::new().text("行 3"),
        Element::new().text("行 4"),
        Element::new().text("行 5"),
        Element::new().text("行 6"),
    ])
}

#[test]
fn dialog_first_vs_second_open_cpu_after_cjk_warm() {
    use std::cell::RefCell;
    use std::rc::Rc;

    crate::app::portal_overlay::clear_overlay_stack();
    reset_registered_anchors();
    clear_test_reconcile_root();

    let mut fs = create_font_system();
    apply_default_families(fs.db_mut());
    let t_cjk = Instant::now();
    ensure_coverage_for_text(&mut fs, "桌面嵌套浮层演示长内容");
    let cjk = t_cjk.elapsed();
    crate::text::install_shared_font_system(&mut fs);
    eprintln!("lazy CJK load (msyh.ttc etc.): {cjk:?}");

    let live: Rc<RefCell<Element>> = Rc::new(RefCell::new(Element::new()));
    install_test_reconcile_root(Rc::clone(&live));

    let (owner, open) = create_root(|| {
        let open = use_signal("dlg_first_cost__open", || false);
        let live_e = Rc::clone(&live);
        create_effect(move || {
            let el = dialog("dlg_first_cost").open(open).child(move || {
                dialog_content(DialogContentOpts::default(), gallery_like_dialog_body)
            });
            *live_e.borrow_mut() = el;
        });
        open
    });

    let layout_once = |live: &RefCell<Element>, layout: &mut LayoutTree| {
        drain_deferred_view_remounts();
        let mut tree = live.borrow_mut();
        let _ = hoist_portal_subtrees(&mut tree);
        let _ = flush_modal_shell_motion_subtree(&mut tree);
        let _ = tick_hoisted_modal_css(&mut tree);
        let _ = layout.build_and_layout(&mut tree, 1280.0, 800.0, [0.0, 0.0]);
    };

    drain_deferred_view_remounts();
    let mut layout = LayoutTree::new();
    layout_once(&live, &mut layout);

    let t0 = Instant::now();
    open.set(true);
    layout_once(&live, &mut layout);
    let first = t0.elapsed();
    assert!(
        has_tid(&live.borrow(), "dlg_first_cost__modal_overlay"),
        "first open must put overlay in the tree"
    );

    open.set(false);
    layout_once(&live, &mut layout);

    let t1 = Instant::now();
    open.set(true);
    layout_once(&live, &mut layout);
    let second = t1.elapsed();

    eprintln!("dialog CPU after CJK warm: first={first:?} second={second:?} cjk={cjk:?}");

    clear_test_reconcile_root();
    drop(live);
    owner.dispose();
    crate::app::portal_overlay::clear_overlay_stack();
    reset_registered_anchors();

    assert!(
        first.as_millis() < 400,
        "first Dialog open CPU {first:?} after CJK warm is the ~500ms empty wait (second={second:?}, cjk={cjk:?})"
    );
}

#[test]
fn dialog_first_gpu_blur_and_capture_text_ms() {
    use std::collections::{HashMap, HashSet};

    use crate::renderer::offscreen::OffscreenGpu;
    use crate::renderer::prepare_text::prepare_text_frame;
    use crate::renderer::{BackdropBlurPipeline, RuiTextRenderer};

    let Some(gpu) = OffscreenGpu::new() else {
        eprintln!("skip dialog_first_gpu_blur_and_capture_text_ms: no wgpu adapter");
        return;
    };

    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let w = 1280u32;
    let h = 800u32;
    let config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format,
        width: w,
        height: h,
        present_mode: wgpu::PresentMode::Fifo,
        desired_maximum_frame_latency: 2,
        alpha_mode: wgpu::CompositeAlphaMode::Auto,
        color_space: wgpu::SurfaceColorSpace::Auto,
        view_formats: vec![],
    };

    // Match AppState::new: pipelines exist before the first Dialog click.
    let mut backdrop = BackdropBlurPipeline::new(&gpu.device, format);
    let mut main_text = RuiTextRenderer::new(&gpu.device, &gpu.queue, &config);
    crate::text::install_shared_font_system(main_text.font_system_mut());
    ensure_coverage_for_text(
        main_text.font_system_mut(),
        "桌面嵌套浮层演示长内容 Gallery Dialog",
    );
    let font_ptr = main_text.font_system_mut() as *mut _;
    let mut capture_text =
        RuiTextRenderer::new_sharing_font_system(&gpu.device, &gpu.queue, &config, font_ptr);

    let dummy_nodes = {
        let mut tree = LayoutTree::new();
        let mut root = Element::new()
            .w(240.0)
            .h(120.0)
            .child(Element::new().text("Gallery Dialog 行 1 — 演示长内容"));
        tree.build_and_layout(&mut root, 240.0, 120.0, [-1.0, -1.0])
            .text_nodes
    };

    let run_blur = |backdrop: &mut BackdropBlurPipeline| {
        let t = Instant::now();
        backdrop.ensure_size(&gpu.device, w, h, format);
        let mut encoder = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("diag-first-blur"),
            });
        backdrop.blur(&gpu.device, &gpu.queue, &mut encoder, 16.0);
        gpu.queue.submit(Some(encoder.finish()));
        let _ = gpu.device.poll(wgpu::PollType::wait_indefinitely());
        t.elapsed()
    };

    let blur1 = run_blur(&mut backdrop);
    let blur2 = run_blur(&mut backdrop);

    let run_capture_prepare = |text: &mut RuiTextRenderer| {
        let t = Instant::now();
        let _ = prepare_text_frame(
            text,
            &gpu.device,
            &gpu.queue,
            &config,
            &dummy_nodes,
            1.0,
            false,
            &HashSet::new(),
            &HashMap::new(),
        );
        let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("diag-cap-rt"),
            size: wgpu::Extent3d {
                width: 256,
                height: 128,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("diag-capture-text"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("diag-capture-text-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            text.render(&mut pass);
        }
        gpu.queue.submit(Some(encoder.finish()));
        let _ = gpu.device.poll(wgpu::PollType::wait_indefinitely());
        drop(texture);
        t.elapsed()
    };

    // Warm the *main* atlas (page already painted). Capture atlas is still cold.
    let _ = prepare_text_frame(
        &mut main_text,
        &gpu.device,
        &gpu.queue,
        &config,
        &dummy_nodes,
        1.0,
        false,
        &HashSet::new(),
        &HashMap::new(),
    );

    let cap1 = run_capture_prepare(&mut capture_text);
    let cap2 = run_capture_prepare(&mut capture_text);

    eprintln!(
        "dialog GPU: first_blur={blur1:?} second_blur={blur2:?} first_capture_text={cap1:?} second_capture_text={cap2:?}"
    );

    let first_gpu = blur1 + cap1;
    assert!(
        first_gpu.as_millis() < 400,
        "first Dialog GPU (blur+capture text) {first_gpu:?} looks like the ~500ms empty wait \
         (blur {blur1:?}/{blur2:?}, capture {cap1:?}/{cap2:?})"
    );
}
