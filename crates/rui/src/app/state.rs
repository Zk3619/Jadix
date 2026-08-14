use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use wgpu::util::DeviceExt;
use winit::dpi::PhysicalSize;
use winit::window::Window;

use crate::core::tree::{Element, LayoutTree};
use crate::renderer::{
    cpu_text::CpuTextProfiler, prepare_text_frame, scale_instances, scale_textured_draws,
    BackdropBlurPipeline, InstanceInput, PresentRetainPipeline, RuiPipeline, RuiTextRenderer,
    ScaledTexturedDraw, TexturedInstanceInput, TexturedQuadPipeline,
};
use crate::{
    components::{
        scrollbar::scrollbar_visibility_signal_key,
        scrollbar::{
            scrollbar_min_thumb_along_track, SCROLLBAR_THUMB_FOCUS_PREFIX,
            SCROLLBAR_TRACK_FOCUS_PREFIX,
        },
    },
    core::reactivity::use_signal,
    core::scroll_viewport::{
        scroll_content_h_signal_key, scroll_content_w_signal_key, scroll_offset_x_signal_key,
        scroll_offset_y_signal_key, scroll_overflow_x_signal_key, scroll_overflow_y_signal_key,
        scroll_scrollbar_inset_x_signal_key, scroll_scrollbar_inset_y_signal_key,
        scroll_scrollbar_x_side_signal_key, scroll_scrollbar_y_side_signal_key,
        scroll_viewport_h_signal_key, scroll_viewport_w_signal_key, SCROLL_VIEWPORT_FOCUS_PREFIX,
    },
};
use std::collections::hash_map::Entry;
#[cfg(debug_assertions)]
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::OnceLock;
use std::time::Instant;
use taffy::prelude::Dimension;

#[cfg(debug_assertions)]
static SCROLL_PREPASS_RECURSE_COUNT: AtomicUsize = AtomicUsize::new(0);
#[cfg(debug_assertions)]
static SCROLL_PREPASS_RECURSE_VIEWPORTS_ONLY: AtomicUsize = AtomicUsize::new(0);
#[cfg(debug_assertions)]
static SCROLL_PREPASS_RECURSE_FULL: AtomicUsize = AtomicUsize::new(0);

/// How much scroll-related work to do before layout each frame.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ScrollbarUpdateMode {
    /// Sync `scroll:` offsets + scrollbar track/thumb geometry and Taffy (springs, drag, smooth).
    Full,
    /// Only apply signal → `Element.scroll_offset` for `scroll:` viewports. Skips expensive
    /// per-scrollbar track/thumb updates when idle (reduces nested ScrollArea cost).
    ViewportsOnly,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TextRendererBackend {
    Gpu,
    CpuTextHybrid,
}

pub(super) struct AppState {
    // Field drop order is the reverse of declaration. wgpu Surface must drop while Device
    // and the winit Window/HWND are still alive (Windows GL/WGL otherwise panics in
    // make_current → workspace `panic = "abort"` → STATUS_STACK_BUFFER_OVERRUN 0xC0000409).
    pub(super) window: Arc<Window>,
    /// Retained so secondary windows can `create_surface` on the same GPU backend.
    pub(super) instance: wgpu::Instance,
    pub(super) device: Arc<wgpu::Device>,
    pub(super) queue: Arc<wgpu::Queue>,
    pub(super) surface: wgpu::Surface<'static>,
    pub(super) config: wgpu::SurfaceConfiguration,
    pub(super) size: winit::dpi::PhysicalSize<u32>,
    pub(super) pipeline: RuiPipeline,
    textured_pipeline: TexturedQuadPipeline,
    backdrop_blur: BackdropBlurPipeline,
    /// Offscreen color target so PartialLoad+scissor does not hit a non-retaining swapchain.
    present_retain: PresentRetainPipeline,
    image_textures: std::collections::HashMap<String, super::image_registry::GpuImageTexture>,
    textured_instance_buffer: wgpu::Buffer,
    textured_instance_capacity: usize,
    pub(super) instance_buffer: wgpu::Buffer,
    instance_buffer_capacity: usize,
    pub(super) layout_tree: LayoutTree,
    pub(super) text_renderer: RuiTextRenderer,
    /// Separate glyphon pipeline for portal overlay glyphs so page + overlay text can be
    /// prepared together and drawn in one Clear pass (page → overlay rects → overlay text)
    /// without a GLES-fragile `LoadOp::Load` second pass that erased sidebar chrome.
    ///
    /// Band 0 of stacked portals. Additional bands use [`Self::overlay_band_text_renderers`]
    /// so each portal child can paint `rects → text` before the next child (correct occlusion).
    overlay_text_renderer: RuiTextRenderer,
    /// Extra glyphon pipelines for portal paint bands beyond the first (share FontSystem).
    overlay_band_text_renderers: Vec<RuiTextRenderer>,
    pub(super) root_element: Element,
    /// `template_id` -> child-index path from [`Self::root_element`]. Validated on each use instead
    /// of invalidated on reconcile, so a stale entry only costs one failed walk before the DFS.
    template_paths: HashMap<String, Vec<usize>>,
    /// Taffy node ids that need `mark_dirty` + style/measure sync on the next LayoutChanged frame
    /// (reconcile / bind paths). Cleared after apply. StructureChanged still does a full rebuild.
    pending_layout_dirty: Vec<taffy::prelude::NodeId>,
    pub(super) num_instances: u32,
    pub(super) text_nodes: Vec<crate::core::tree::TextLayoutInfo>,
    pub(super) layout_status: crate::core::tree::LayoutStatus,
    scrollbar_base_style: std::collections::HashMap<String, ScrollbarBaseStyle>,
    #[cfg(debug_assertions)]
    pub(super) hot_reload_overrides: Vec<crate::app::hot_reload::TemplateFragment>,
    /// 避免 CLI 对同一文件连续发送「内容相同、无 diff」的补丁时二次注入搞乱布局。
    #[cfg(debug_assertions)]
    last_applied_template_hash: HashMap<String, String>,
    pub(super) last_render_scroll_metrics: Vec<(String, [f32; 4], [f32; 2], [f32; 2])>,
    pub(super) last_hover_cursor: Option<crate::core::tree::CursorKind>,
    pub(super) text_backend: TextRendererBackend,
    /// Latest `WindowEvent::Resized` size. `inner_size()` can briefly lag during maximize; merging
    /// avoids a one-frame swapchain smaller than the DWM client area (black letterboxing at edges).
    last_resized_event: Option<PhysicalSize<u32>>,
    /// After live resize ends, the matching frame still ran `layout_fast`; the next frame must
    /// remasure text leaves with glyphon_fit and reshape (not reuse the text prepare cache).
    needs_text_settle: bool,
    #[cfg(debug_assertions)]
    scroll_prepass_log_last: Instant,
    #[cfg(debug_assertions)]
    scroll_prepass_log_frames: u32,
    #[cfg(debug_assertions)]
    render_prof_last: Instant,
    #[cfg(debug_assertions)]
    render_prof_frames: u32,
    #[cfg(debug_assertions)]
    render_prof_pre_ms: f32,
    #[cfg(debug_assertions)]
    render_prof_layout_ms: f32,
    /// `RenderResult.prof`：`build_and_layout` 建/剪 Taffy（与总 `layout` 互补读）。
    #[cfg(debug_assertions)]
    render_prof_layout_rebuild_ms: f32,
    #[cfg(debug_assertions)]
    render_prof_layout_setup_ms: f32,
    #[cfg(debug_assertions)]
    render_prof_layout_taffy_ms: f32,
    #[cfg(debug_assertions)]
    render_prof_layout_collect_ms: f32,
    #[cfg(debug_assertions)]
    render_prof_draw_ms: f32,
    #[cfg(debug_assertions)]
    render_prof_acquire_ms: f32,
    #[cfg(debug_assertions)]
    render_prof_submit_ms: f32,
    #[cfg(debug_assertions)]
    render_prof_present_ms: f32,
    #[cfg(debug_assertions)]
    render_prof_upload_ms: f32,
    #[cfg(debug_assertions)]
    render_prof_text_ms: f32,
    #[cfg(debug_assertions)]
    render_prof_encode_ms: f32,
    last_text_nodes_cache: Vec<crate::core::tree::TextLayoutInfo>,
    last_text_ui_scale: f32,
    last_text_surface_size: [u32; 2],
    last_underline_instances: Vec<crate::renderer::InstanceInput>,
    last_caret_instances: Vec<crate::renderer::InstanceInput>,
    last_caret_blink_phase: u128,
    cpu_text_profiler: CpuTextProfiler,
    shadow_tex_cache: std::collections::HashMap<u64, super::image_registry::GpuImageTexture>,
    shadow_rect_cache: std::collections::HashMap<u64, [f32; 4]>,
    last_shadow_tex_draws: Vec<crate::renderer::prepare_text::ShadowTexDraw>,
    /// Accumulated dirty rect in logical px for partial redraw (`x,y,w,h`).
    pub(super) damage_rect: Option<[f32; 4]>,
    pub(super) force_full_redraw: bool,
    /// After the first successful present, partial `LoadOp::Load` is allowed.
    has_presented_frame: bool,
    /// Fingerprint of the last submitted GPU frame (instances + text chrome + caret).
    /// Used to skip acquire/submit/present when a redraw was requested but nothing visual changed.
    last_gpu_frame_fp: u64,
    /// Hash of all `scroll:` viewport offsets after prepass. Any change invalidates PartialLoad
    /// (pixels move across the viewport; scissor from hover/bind_text would paint scroll only
    /// inside the old dirty rect).
    last_scroll_offset_fp: u64,
}

/// `queue.write_buffer` 在 submit 时先于 render pass 执行；同一 buffer 多次写 offset 0
/// 会只留下最后一次（饼图一进视口，折线实例被盖成饼图矩形）。须一次打包、按 instance range 画。
fn upload_textured_instance_buffer(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    buffer: &mut wgpu::Buffer,
    capacity: &mut usize,
    insts: &[TexturedInstanceInput],
) {
    if insts.is_empty() {
        return;
    }
    let stride = std::mem::size_of::<TexturedInstanceInput>();
    if insts.len() > *capacity {
        let new_cap = insts.len().max(256);
        *buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Textured instance buffer"),
            size: (stride * new_cap) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        *capacity = new_cap;
    }
    queue.write_buffer(buffer, 0, bytemuck::cast_slice(insts));
}

fn draw_packed_textured_runs<'pass>(
    pass: &mut wgpu::RenderPass<'pass>,
    pipeline: &'pass TexturedQuadPipeline,
    instance_buffer: &'pass wgpu::Buffer,
    image_textures: &'pass HashMap<String, super::image_registry::GpuImageTexture>,
    scaled_tex: &[ScaledTexturedDraw],
    start: usize,
    end: usize,
) {
    let end = end.min(scaled_tex.len());
    let mut ti = start.min(end);
    while ti < end {
        let key = scaled_tex[ti].texture_key.as_str();
        let Some(gpu_img) = image_textures.get(key) else {
            ti += 1;
            continue;
        };
        let run = scaled_tex[ti..end]
            .iter()
            .take_while(|d| d.texture_key == key)
            .count();
        if run == 0 {
            ti += 1;
            continue;
        }
        pass.set_pipeline(&pipeline.render_pipeline);
        pass.set_bind_group(0, &gpu_img.bind_group, &[]);
        pass.set_vertex_buffer(0, pipeline.vertex_buffer.slice(..));
        pass.set_vertex_buffer(1, instance_buffer.slice(..));
        pass.set_index_buffer(pipeline.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
        pass.draw_indexed(0..pipeline.num_indices, 0, ti as u32..(ti + run) as u32);
        ti += run;
    }
}

fn draw_paint_ops<'pass>(
    pass: &mut wgpu::RenderPass<'pass>,
    ops: &[crate::layout::layout_tree::PaintOp],
    lo: usize,
    hi: usize,
    pipeline: &'pass RuiPipeline,
    instance_buffer: &'pass wgpu::Buffer,
    textured_pipeline: &'pass TexturedQuadPipeline,
    textured_instance_buffer: &'pass wgpu::Buffer,
    image_textures: &'pass HashMap<String, super::image_registry::GpuImageTexture>,
    scaled_tex: &'pass [ScaledTexturedDraw],
) {
    use crate::layout::layout_tree::PaintOp;
    let hi = hi.min(ops.len());
    let lo = lo.min(hi);
    for op in &ops[lo..hi] {
        match *op {
            PaintOp::Rects { start, count } if count > 0 => {
                pass.set_pipeline(&pipeline.render_pipeline);
                pass.set_bind_group(0, &pipeline.bind_group, &[]);
                pass.set_vertex_buffer(0, pipeline.vertex_buffer.slice(..));
                pass.set_vertex_buffer(1, instance_buffer.slice(..));
                pass.set_index_buffer(pipeline.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
                pass.draw_indexed(0..pipeline.num_indices, 0, start..(start + count));
            }
            PaintOp::Textured { start, count } if count > 0 => {
                let s = start as usize;
                let e = s + count as usize;
                draw_packed_textured_runs(
                    pass,
                    textured_pipeline,
                    textured_instance_buffer,
                    image_textures,
                    scaled_tex,
                    s,
                    e,
                );
            }
            _ => {}
        }
    }
}

impl AppState {
    fn cpu_hybrid_text_only() -> bool {
        static ENABLED: OnceLock<bool> = OnceLock::new();
        *ENABLED.get_or_init(|| {
            std::env::var("RUI_CPU_TEXT_HYBRID_TEXT_ONLY")
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or(false)
        })
    }

    #[inline]
    fn approx_eq(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() <= eps
    }

    #[inline]
    fn approx_clip_eq(a: Option<[f32; 4]>, b: Option<[f32; 4]>, eps: f32) -> bool {
        match (a, b) {
            (None, None) => true,
            (Some(x), Some(y)) => {
                Self::approx_eq(x[0], y[0], eps)
                    && Self::approx_eq(x[1], y[1], eps)
                    && Self::approx_eq(x[2], y[2], eps)
                    && Self::approx_eq(x[3], y[3], eps)
            }
            _ => false,
        }
    }

    fn text_nodes_equal(
        a: &[crate::core::tree::TextLayoutInfo],
        b: &[crate::core::tree::TextLayoutInfo],
    ) -> bool {
        // 略放宽：flex/滚动条亚像素抖动不应每帧打穿文本缓存触发整帧 prepare。
        const TEXT_GEOM_EPS: f32 = 0.5;
        if a.len() != b.len() {
            return false;
        }
        a.iter().zip(b.iter()).all(|(x, y)| {
            x.text == y.text
                && x.shape_suffix == y.shape_suffix
                && x.template_id == y.template_id
                && x.style == y.style
                && Self::approx_clip_eq(x.clip_bounds, y.clip_bounds, TEXT_GEOM_EPS)
                && Self::approx_eq(x.x, y.x, TEXT_GEOM_EPS)
                && Self::approx_eq(x.y, y.y, TEXT_GEOM_EPS)
                && Self::approx_eq(x.width, y.width, TEXT_GEOM_EPS)
                && Self::approx_eq(x.height, y.height, TEXT_GEOM_EPS)
        })
    }

    pub(super) fn note_winit_resized(&mut self, physical_size: PhysicalSize<u32>) {
        self.last_resized_event = Some(physical_size);
        // New resize aborts a pending settle; drag frames stay on the fast path.
        self.needs_text_settle = false;
    }

    /// Pick the swapchain / layout size for this frame.
    ///
    /// Prefer the latest `WindowEvent::Resized` over `inner_size()`. Using
    /// `max(inner, resized)` (old behavior) correctly covered grow-lag, but on
    /// **shrink** a still-large `inner_size` kept the layout viewport tall: content
    /// still "fit" (`can_scroll_y=false`) while the OS window already clipped the
    /// bottom — gallery looked like it entered a broken scroll/clip state.
    pub(super) fn resolve_surface_size(
        inner: PhysicalSize<u32>,
        last_resized: Option<PhysicalSize<u32>>,
    ) -> PhysicalSize<u32> {
        match last_resized {
            Some(e) if e.width > 0 && e.height > 0 => e,
            _ => inner,
        }
    }

    pub(super) fn ui_scale(&self) -> f32 {
        self.window.scale_factor() as f32
    }

    /// Snapshot for [`accesskit_winit::Adapter::update_if_active`].
    pub(super) fn accesskit_tree_update(&self) -> accesskit::TreeUpdate {
        let focused = use_signal("__rui_button_focus_id", String::new).get();
        let fid = if focused.is_empty() {
            None
        } else {
            Some(focused.as_str())
        };
        crate::a11y::tree_update_from_elements(&self.root_element, fid)
    }

    fn logical_size(&self) -> (f32, f32) {
        let scale = self.ui_scale().max(1.0);
        (
            self.config.width as f32 / scale,
            self.config.height as f32 / scale,
        )
    }

    /// Backends to try, in order. `RUI_GPU_BACKEND` (`gl` / `dx12` / `vulkan` / `metal`)
    /// moves one to the front; the rest stay as fallbacks so an unsupported pick
    /// degrades instead of aborting.
    fn backend_candidates() -> Vec<(wgpu::Backends, &'static str)> {
        let mut order: Vec<(wgpu::Backends, &'static str)> = if cfg!(target_os = "macos") {
            vec![(wgpu::Backends::METAL, "metal")]
        } else if cfg!(windows) {
            // Prefer GL by default: smallest resident memory for a 2D UI.
            // Override with RUI_GPU_BACKEND=dx12|vulkan when lower CPU overhead matters more.
            vec![
                (wgpu::Backends::GL, "gl"),
                (wgpu::Backends::DX12, "dx12"),
                (wgpu::Backends::VULKAN, "vulkan"),
            ]
        } else {
            vec![
                (wgpu::Backends::GL, "gl"),
                (wgpu::Backends::VULKAN, "vulkan"),
            ]
        };

        let requested = std::env::var("RUI_GPU_BACKEND").unwrap_or_default();
        let requested = match requested.trim().to_ascii_lowercase().as_str() {
            "gl" | "gles" | "opengl" => Some(wgpu::Backends::GL),
            "dx12" | "d3d12" => Some(wgpu::Backends::DX12),
            "vulkan" | "vk" => Some(wgpu::Backends::VULKAN),
            "metal" => Some(wgpu::Backends::METAL),
            _ => None,
        };
        if let Some(front) = requested {
            let label = order
                .iter()
                .find(|(b, _)| *b == front)
                .map_or("requested", |(_, name)| *name);
            order.retain(|(b, _)| *b != front);
            order.insert(0, (front, label));
        }
        order
    }

    /// Create the instance / surface / adapter triple from the first backend that works.
    async fn acquire_adapter(
        window: &Arc<Window>,
        force_software: bool,
    ) -> (wgpu::Instance, wgpu::Surface<'static>, wgpu::Adapter) {
        let candidates = Self::backend_candidates();
        if !force_software {
            for (backends, name) in &candidates {
                let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
                    backends: *backends,
                    ..Default::default()
                });
                let Ok(surface) = instance.create_surface(window.clone()) else {
                    log::debug!("backend {name}: no surface");
                    continue;
                };
                let adapter = instance
                    .request_adapter(&wgpu::RequestAdapterOptions {
                        // Integrated GPU for 2D UI — lower working set than discrete.
                        power_preference: wgpu::PowerPreference::LowPower,
                        compatible_surface: Some(&surface),
                        force_fallback_adapter: false,
                    })
                    .await;
                if let Some(adapter) = adapter {
                    log::info!("gpu backend: {name} ({})", adapter.get_info().name);
                    return (instance, surface, adapter);
                }
                log::debug!("backend {name}: no adapter");
            }
        }

        // WARP / llvmpipe last: only DX12 and Vulkan expose a fallback adapter.
        for backends in [wgpu::Backends::DX12, wgpu::Backends::VULKAN] {
            let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
                backends,
                ..Default::default()
            });
            let Ok(surface) = instance.create_surface(window.clone()) else {
                continue;
            };
            let adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::LowPower,
                    compatible_surface: Some(&surface),
                    force_fallback_adapter: true,
                })
                .await;
            if let Some(adapter) = adapter {
                log::warn!(
                    "falling back to software adapter: {}",
                    adapter.get_info().name
                );
                return (instance, surface, adapter);
            }
        }

        panic!("no compatible GPU or software adapter found");
    }

    /// Initialize renderer/window state with an internal empty root.
    ///
    /// The user app tree is mounted later by the tracked root effect. Keeping it out of this
    /// constructor guarantees that `app_builder` is not evaluated once for GPU initialization and
    /// again for reactivity.
    /// Attr marker on the private bootstrap root created by [`Self::new_bootstrap`].
    /// Cleared on the first real [`Self::rebuild_ui`] mount.
    pub(super) const BOOTSTRAP_ROOT_ATTR: &'static str = "rui_bootstrap";

    #[inline]
    pub(super) fn is_bootstrap_root(el: &Element) -> bool {
        el.get_attr(Self::BOOTSTRAP_ROOT_ATTR).map(|s| s.as_str()) == Some("1")
    }

    pub(super) async fn new_bootstrap(window: Arc<Window>) -> Self {
        let mut root_element = Element::new().w("100%").h("100%");
        root_element.set_attr(Self::BOOTSTRAP_ROOT_ATTR, "1");
        let size = window.inner_size();
        let force_software = std::env::var("RUI_FORCE_SOFTWARE")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);

        // Backend choice dominates resident memory: on Windows the D3D12 and Vulkan
        // user-mode drivers commit ~80 MB of private heaps at device creation, while
        // the OpenGL ICD commits ~5 MB for the same clear-screen workload. 2D UI does
        // not need what DX12 buys, so GL leads and the others stay as fallbacks.
        let (instance, surface, adapter) = Self::acquire_adapter(&window, force_software).await;
        let text_backend = if adapter.get_info().device_type == wgpu::DeviceType::Cpu {
            TextRendererBackend::CpuTextHybrid
        } else {
            TextRendererBackend::Gpu
        };
        log::warn!("text renderer backend: {:?}", text_backend);

        let adapter_limits = adapter.limits();
        // Keep lean 2D defaults, but lift 2D texture cap to the adapter — otherwise maximize /
        // large windows (>2048px) panic when configuring the swapchain.
        let mut required_limits = wgpu::Limits::downlevel_defaults();
        required_limits.max_texture_dimension_1d = adapter_limits.max_texture_dimension_1d;
        required_limits.max_texture_dimension_2d = adapter_limits.max_texture_dimension_2d;
        required_limits.max_texture_dimension_3d = adapter_limits
            .max_texture_dimension_3d
            .min(required_limits.max_texture_dimension_3d.max(2048));

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    required_features: wgpu::Features::empty(),
                    required_limits,
                    label: None,
                    memory_hints: wgpu::MemoryHints::MemoryUsage,
                },
                None,
            )
            .await
            .unwrap();
        let device = Arc::new(device);
        let queue = Arc::new(queue);

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(surface_caps.formats[0]);

        // Prefer vsync'd modes so continuous redraw (scroll/hover anim) tracks the display
        // refresh instead of uncapped Mailbox (~150+ fps on some GL adapters).
        let present_mode = surface_caps
            .present_modes
            .iter()
            .copied()
            .find(|m| matches!(m, wgpu::PresentMode::Fifo))
            .or_else(|| {
                surface_caps
                    .present_modes
                    .iter()
                    .copied()
                    .find(|m| matches!(m, wgpu::PresentMode::FifoRelaxed))
            })
            .or_else(|| {
                surface_caps
                    .present_modes
                    .iter()
                    .copied()
                    .find(|m| matches!(m, wgpu::PresentMode::Mailbox))
            })
            .unwrap_or(surface_caps.present_modes[0]);
        log::info!("surface present_mode: {present_mode:?}");

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode,
            // Prefer opaque: on Windows GLES the first listed mode is often premultiplied
            // transparency, which shows the desktop/terminal through undrawn pixels.
            alpha_mode: surface_caps
                .alpha_modes
                .iter()
                .copied()
                .find(|m| *m == wgpu::CompositeAlphaMode::Opaque)
                .unwrap_or(surface_caps.alpha_modes[0]),
            view_formats: vec![],
            desired_maximum_frame_latency: 1,
        };
        surface.configure(&device, &config);

        let pipeline = RuiPipeline::new(&device, &config);
        let textured_pipeline = TexturedQuadPipeline::new(&device, &config);
        let backdrop_blur = BackdropBlurPipeline::new(&device, config.format);
        let mut present_retain = PresentRetainPipeline::new(&device, config.format);
        let _ = present_retain.ensure_size(&device, config.width, config.height);
        let tex_inst_stride = std::mem::size_of::<TexturedInstanceInput>();
        let textured_instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Textured instance buffer"),
            size: (tex_inst_stride * 256) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let mut layout_tree = LayoutTree::new();
        let scale = window.scale_factor() as f32;
        let logical_w = config.width as f32 / scale.max(1.0);
        let logical_h = config.height as f32 / scale.max(1.0);
        let _portal = hoist_portal_subtrees_refreshing_scroll_flags(&mut root_element);
        let render_result =
            layout_tree.build_and_layout(&mut root_element, logical_w, logical_h, [0.0, 0.0]);
        let scaled_instances = scale_instances(&render_result.instances, scale);
        let num_instances = render_result.instances.len() as u32;
        let last_render_scroll_metrics = render_result.scroll_metrics;
        let last_hover_cursor = render_result.hover_cursor;

        let instance_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Instance Buffer"),
            contents: bytemuck::cast_slice(&scaled_instances),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });

        let mut text_renderer = RuiTextRenderer::new(&device, &queue, &config);
        // Layout measure and glyph shaping share one FontSystem (fontdb / coverage).
        crate::text::install_shared_font_system(text_renderer.font_system_mut());
        let font_ptr: *mut glyphon::FontSystem = text_renderer.font_system_mut();
        let overlay_text_renderer =
            RuiTextRenderer::new_sharing_font_system(&device, &queue, &config, font_ptr);
        crate::debug_stack::crumb("new_bootstrap: after text_renderer");

        Self {
            window,
            instance,
            device,
            queue,
            surface,
            config,
            size,
            pipeline,
            textured_pipeline,
            backdrop_blur,
            present_retain,
            image_textures: std::collections::HashMap::new(),
            textured_instance_buffer,
            textured_instance_capacity: 256,
            instance_buffer,
            instance_buffer_capacity: scaled_instances.len(),
            layout_tree,
            text_renderer,
            overlay_text_renderer,
            overlay_band_text_renderers: Vec::new(),
            root_element,
            template_paths: HashMap::new(),
            pending_layout_dirty: Vec::new(),
            num_instances,
            text_nodes: render_result.text_nodes,
            layout_status: crate::core::tree::LayoutStatus::StructureChanged,
            scrollbar_base_style: std::collections::HashMap::new(),
            #[cfg(debug_assertions)]
            hot_reload_overrides: Vec::new(),
            #[cfg(debug_assertions)]
            last_applied_template_hash: HashMap::new(),
            last_render_scroll_metrics,
            last_hover_cursor,
            text_backend,
            last_resized_event: None,
            needs_text_settle: false,
            #[cfg(debug_assertions)]
            scroll_prepass_log_last: Instant::now(),
            #[cfg(debug_assertions)]
            scroll_prepass_log_frames: 0,
            #[cfg(debug_assertions)]
            render_prof_last: Instant::now(),
            #[cfg(debug_assertions)]
            render_prof_frames: 0,
            #[cfg(debug_assertions)]
            render_prof_pre_ms: 0.0,
            #[cfg(debug_assertions)]
            render_prof_layout_ms: 0.0,
            #[cfg(debug_assertions)]
            render_prof_layout_rebuild_ms: 0.0,
            #[cfg(debug_assertions)]
            render_prof_layout_setup_ms: 0.0,
            #[cfg(debug_assertions)]
            render_prof_layout_taffy_ms: 0.0,
            #[cfg(debug_assertions)]
            render_prof_layout_collect_ms: 0.0,
            #[cfg(debug_assertions)]
            render_prof_draw_ms: 0.0,
            #[cfg(debug_assertions)]
            render_prof_acquire_ms: 0.0,
            #[cfg(debug_assertions)]
            render_prof_submit_ms: 0.0,
            #[cfg(debug_assertions)]
            render_prof_present_ms: 0.0,
            #[cfg(debug_assertions)]
            render_prof_upload_ms: 0.0,
            #[cfg(debug_assertions)]
            render_prof_text_ms: 0.0,
            #[cfg(debug_assertions)]
            render_prof_encode_ms: 0.0,
            last_text_nodes_cache: Vec::new(),
            last_text_ui_scale: 0.0,
            last_text_surface_size: [0, 0],
            last_underline_instances: Vec::new(),
            last_caret_instances: Vec::new(),
            last_caret_blink_phase: 0,
            cpu_text_profiler: CpuTextProfiler::new(),
            shadow_tex_cache: std::collections::HashMap::new(),
            shadow_rect_cache: std::collections::HashMap::new(),
            last_shadow_tex_draws: Vec::new(),
            damage_rect: None,
            force_full_redraw: true,
            has_presented_frame: false,
            last_gpu_frame_fp: 0,
            last_scroll_offset_fp: 0,
        }
    }

    /// Fingerprint scroll viewport offsets (logical). Used to force FullClear when content moves.
    fn scroll_offset_fingerprint(root: &Element) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        let mut hasher = DefaultHasher::new();
        fn walk(el: &Element, hasher: &mut DefaultHasher) {
            if let Some(fid) = el.focus_id.as_deref() {
                if let Some(rest) = fid.strip_prefix(SCROLL_VIEWPORT_FOCUS_PREFIX) {
                    rest.hash(hasher);
                    el.scroll_offset[0].to_bits().hash(hasher);
                    el.scroll_offset[1].to_bits().hash(hasher);
                }
            }
            for child in &el.children {
                walk(child, hasher);
            }
        }
        walk(root, &mut hasher);
        hasher.finish()
    }

    pub(super) fn union_damage(&mut self, rect: [f32; 4]) {
        if rect[2] <= 0.0 || rect[3] <= 0.0 {
            return;
        }
        self.damage_rect = Some(match self.damage_rect {
            Some(existing) => {
                // Prefer multi-rect helper so Stage-8 API stays live on the hot path.
                super::damage::rects_union(&[existing, rect])
                    .unwrap_or_else(|| super::damage::union_rects(existing, rect))
            }
            None => rect,
        });
    }

    pub(super) fn mark_full_redraw(&mut self) {
        self.force_full_redraw = true;
        self.damage_rect = None;
    }

    /// Logical padding around paint damage for shadows / AA. Fixed cap — do not scan every
    /// instance (that made scissor huge and expensive on page switches).
    const DAMAGE_SCISSOR_PAD_LOGICAL: f32 = 16.0;

    /// Opt-in GPU PartialLoad (`RUI_PARTIAL_DAMAGE=1`). Default off: Solid-style coherent
    /// full present; retain+blit without draw culling regresses forms Mark invalid.
    fn gpu_partial_damage_enabled() -> bool {
        static ENABLED: OnceLock<bool> = OnceLock::new();
        *ENABLED.get_or_init(|| {
            std::env::var("RUI_PARTIAL_DAMAGE")
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or(false)
        })
    }

    /// Grow shared-font overlay text pipelines so each portal paint band can prepare independently.
    fn ensure_overlay_band_text_renderers(&mut self, band_count: usize) {
        let extra_needed = band_count.saturating_sub(1);
        while self.overlay_band_text_renderers.len() < extra_needed {
            let font_ptr = self.text_renderer.font_system_mut() as *mut glyphon::FontSystem;
            self.overlay_band_text_renderers
                .push(RuiTextRenderer::new_sharing_font_system(
                    &self.device,
                    &self.queue,
                    &self.config,
                    font_ptr,
                ));
        }
    }

    pub(super) fn rebuild_ui(&mut self, root_element: Element) {
        #[cfg(debug_assertions)]
        let mut root_element = {
            let mut r = root_element;
            if !self.hot_reload_overrides.is_empty() {
                hot_reload_inject::apply_templates_to_tree(&mut r, &self.hot_reload_overrides);
            }
            r
        };
        #[cfg(not(debug_assertions))]
        let mut root_element = root_element;

        let initial_mount = Self::is_bootstrap_root(&self.root_element);

        log::debug!(
            "[rebuild_ui] START initial_mount={} new_children={} old_children={}",
            initial_mount,
            root_element.children.len(),
            self.root_element.children.len()
        );

        // First tracked mount replaces the private bootstrap root. Skip reconcile: there is no
        // user state to preserve, and walking an empty shell is pure overhead on every cold start.
        let status = if initial_mount {
            use crate::core::tree::LayoutStatus;
            LayoutStatus::StructureChanged
        } else {
            let mut dirty = Vec::new();
            let status =
                Self::reconcile_elements(&mut root_element, &mut self.root_element, &mut dirty);
            self.pending_layout_dirty.extend(dirty);
            status
        };
        log::debug!("[rebuild_ui] END status={:?}", status);
        self.root_element = root_element;
        recompute_scroll_viewport_flags(&mut self.root_element);
        self.layout_status = status;
        // Route / gallery page swaps must not PartialLoad over a stale retain buffer
        // (progressive unordered patches + scissor from off-screen old bounds).
        if matches!(
            status,
            crate::core::tree::LayoutStatus::StructureChanged
                | crate::core::tree::LayoutStatus::LayoutChanged
        ) {
            self.mark_full_redraw();
        }
    }

    pub(crate) fn reconcile_subtree(
        &mut self,
        anchor_id: &str,
        mut new_fragment: Element,
    ) -> crate::core::tree::LayoutStatus {
        #[cfg(feature = "tracing")]
        let _span = tracing::trace_span!("app.reconcile_subtree", anchor_id).entered();

        use crate::core::tree::LayoutStatus;

        // Fine-grained anchors must resolve deterministically. `hot_reload::template_id_matches` is
        // intentionally fuzzy (path suffix + line) for dev-time patches; using it here can match a
        // *different* node earlier in DFS and replace the wrong subtree — e.g. sibling buttons +
        // `<Show>` vanishing intermittently.
        let want = normalize_anchor_template_id(anchor_id);
        let status = if let Some(old_subtree) =
            find_reactive_subtree_by_anchor_id_mut(&mut self.root_element, &want)
        {
            let mut dirty = Vec::new();
            let status = Self::reconcile_elements(&mut new_fragment, old_subtree, &mut dirty);
            self.pending_layout_dirty.extend(dirty);
            *old_subtree = new_fragment;
            status
        } else {
            LayoutStatus::StructureChanged
        };

        recompute_scroll_viewport_flags(&mut self.root_element);
        self.layout_status = Self::merge_layout_status(self.layout_status, status);
        if matches!(
            status,
            LayoutStatus::StructureChanged | LayoutStatus::LayoutChanged
        ) {
            self.mark_full_redraw();
        }
        status
    }

    fn merge_layout_status(
        base: crate::core::tree::LayoutStatus,
        incoming: crate::core::tree::LayoutStatus,
    ) -> crate::core::tree::LayoutStatus {
        base.merge(incoming)
    }

    fn reconcile_elements(
        new_el: &mut Element,
        old_el: &mut Element,
        dirty: &mut Vec<taffy::prelude::NodeId>,
    ) -> crate::core::tree::LayoutStatus {
        use crate::core::tree::LayoutStatus;

        let id_match = new_el.template_id == old_el.template_id;

        if id_match {
            new_el.layout_node = old_el.layout_node;
            // Keep dispose hooks alive across root rebuilds that recreate the same template_id.
            if new_el.on_unmount.is_none() {
                new_el.on_unmount = old_el.on_unmount.clone();
            }
            let is_reactive_placeholder = new_el.template_hash.as_deref()
                == Some(crate::app::reactive::REACTIVE_PLACEHOLDER_HASH);
            if is_reactive_placeholder {
                // Root rebuilds emit a placeholder shell for reactive anchors; this shell is only a
                // marker and must never overwrite already mounted fine-grained subtree content.
                let old_is_placeholder = old_el.template_hash.as_deref()
                    == Some(crate::app::reactive::REACTIVE_PLACEHOLDER_HASH);
                if old_is_placeholder {
                    // Still waiting for the anchor effect, or another placeholder: merge children only.
                    new_el.children = std::mem::take(&mut old_el.children);
                    new_el.text = old_el.text.clone();
                    new_el.style = old_el.style.clone();
                    new_el.text_style = old_el.text_style.clone();
                } else {
                    // Mounted subtree is the real reactive root (e.g. inner `VStack` with gap/flex).
                    // Moving only `children` would flatten them under this shell and break layout
                    // until the anchor effect runs again — but updates that only rerun the root
                    // effect (e.g. unrelated `use_signal` in `main`) never rerun that anchor.
                    let tid = new_el.template_id.clone();
                    let promoted = std::mem::take(old_el);
                    *new_el = promoted;
                    new_el.template_id = tid;
                }
                return LayoutStatus::Clean;
            }

            let mut status = if new_el.text != old_el.text
                || new_el.style != old_el.style
                || new_el.image_texture != old_el.image_texture
            {
                if let Some(n) = new_el.layout_node {
                    dirty.push(n);
                }
                LayoutStatus::LayoutChanged
            } else if new_el.text_style != old_el.text_style {
                LayoutStatus::StyleChanged
            } else {
                LayoutStatus::Clean
            };

            let new_all_keyed = Self::children_all_keyed(&new_el.children);
            let old_all_keyed = Self::children_all_keyed(&old_el.children);
            if new_all_keyed && old_all_keyed {
                status = Self::merge_layout_status(
                    status,
                    Self::reconcile_keyed_children(new_el, old_el, dirty),
                );
            } else if new_el.children.len() == old_el.children.len() {
                for (n_child, o_child) in new_el.children.iter_mut().zip(old_el.children.iter_mut())
                {
                    let child_status = Self::reconcile_elements(n_child, o_child, dirty);
                    status = Self::merge_layout_status(status, child_status);
                }
            } else {
                log::warn!(
                    "[reconcile] children len mismatch new={} old={} tid={:?} keyed=({},{})",
                    new_el.children.len(),
                    old_el.children.len(),
                    new_el.template_id,
                    new_all_keyed,
                    old_all_keyed
                );
                status = LayoutStatus::StructureChanged;
            }
            status
        } else {
            LayoutStatus::StructureChanged
        }
    }

    #[cfg(test)]
    pub(super) fn reconcile_elements_for_test(
        new_el: &mut Element,
        old_el: &mut Element,
    ) -> crate::core::tree::LayoutStatus {
        let mut dirty = Vec::new();
        Self::reconcile_elements(new_el, old_el, &mut dirty)
    }

    /// Solid-style in-place binding: mutate one mounted node by exact `template_id`.
    ///
    /// Lookup is O(depth) through the cached child-index path; only a cache miss or a stale path
    /// falls back to a full DFS.
    pub(crate) fn update_element_by_template_id<F>(
        &mut self,
        template_id: &str,
        patch: F,
    ) -> crate::core::tree::LayoutStatus
    where
        F: FnOnce(&mut Element) -> crate::core::tree::LayoutStatus,
    {
        let mut path = self.template_paths.remove(template_id).unwrap_or_default();
        if !Self::template_path_matches(&self.root_element, &path, template_id) {
            path.clear();
            if !Self::find_template_path(&self.root_element, template_id, &mut path) {
                return crate::core::tree::LayoutStatus::Clean;
            }
        }

        let mut cursor = &mut self.root_element;
        for &index in &path {
            cursor = &mut cursor.children[index];
        }
        let status = patch(cursor);
        let bounds = cursor.layout_bounds;
        if matches!(status, crate::core::tree::LayoutStatus::LayoutChanged) {
            // Queue for render: mark_dirty + style/measure sync, then compute_layout from app root.
            if let Some(n) = cursor.layout_node {
                self.pending_layout_dirty.push(n);
            }
        }
        self.template_paths.insert(template_id.to_string(), path);
        self.layout_status = Self::merge_layout_status(self.layout_status, status);
        if matches!(status, crate::core::tree::LayoutStatus::StyleChanged) {
            // Paint-only (e.g. bind_text `"10"`→`"11"`): dirty the glyph box, keep Load+scissor
            // (WP-2.2). Do not `mark_full_redraw` — that forces a full Clear every counter tick.
            if bounds[2] > 0.0 && bounds[3] > 0.0 {
                super::damage::accumulate_paint_damage(&mut self.damage_rect, bounds);
            }
        }
        status
    }

    fn template_path_matches(root: &Element, path: &[usize], template_id: &str) -> bool {
        let mut cursor = root;
        for &index in path {
            match cursor.children.get(index) {
                Some(child) => cursor = child,
                None => return false,
            }
        }
        cursor.template_id.as_deref() == Some(template_id)
    }

    fn find_template_path(root: &Element, template_id: &str, out: &mut Vec<usize>) -> bool {
        if root.template_id.as_deref() == Some(template_id) {
            return true;
        }
        for (index, child) in root.children.iter().enumerate() {
            out.push(index);
            if Self::find_template_path(child, template_id, out) {
                return true;
            }
            out.pop();
        }
        false
    }

    fn children_all_keyed(children: &[Element]) -> bool {
        // `Iterator::all` is vacuously true on an empty slice; keyed reconciliation is only defined
        // for non-empty child lists (empty vs empty uses the positional same-length branch).
        !children.is_empty() && children.iter().all(|c| c.template_id.is_some())
    }

    fn reconcile_keyed_children(
        new_el: &mut Element,
        old_el: &mut Element,
        dirty: &mut Vec<taffy::prelude::NodeId>,
    ) -> crate::core::tree::LayoutStatus {
        use crate::core::tree::LayoutStatus;

        let mut status = LayoutStatus::Clean;
        let mut old_by_id: std::collections::HashMap<String, Element> =
            std::collections::HashMap::new();
        for old_child in std::mem::take(&mut old_el.children) {
            if let Some(id) = old_child.template_id.clone() {
                old_by_id.insert(id, old_child);
            }
        }

        let mut next_children = Vec::with_capacity(new_el.children.len());
        for mut new_child in std::mem::take(&mut new_el.children) {
            let Some(id) = new_child.template_id.clone() else {
                status = LayoutStatus::StructureChanged;
                next_children.push(new_child);
                continue;
            };
            if let Some(mut old_child) = old_by_id.remove(&id) {
                let child_status = Self::reconcile_elements(&mut new_child, &mut old_child, dirty);
                status = Self::merge_layout_status(status, child_status);
            } else {
                status = LayoutStatus::StructureChanged;
            }
            next_children.push(new_child);
        }

        if !old_by_id.is_empty() {
            status = LayoutStatus::StructureChanged;
        }
        new_el.children = next_children;
        status
    }

    pub(super) fn update_scroll_offset_in_place(&mut self, target_id: &str, offset: [f32; 2]) {
        fn recurse(el: &mut Element, target: &str, offset: [f32; 2], prefix: &str) {
            if let Some(fid) = el.focus_id.as_deref() {
                if fid.strip_prefix(prefix) == Some(target) {
                    el.scroll_offset = offset;
                }
            }
            for child in &mut el.children {
                recurse(child, target, offset, prefix);
            }
        }
        recurse(
            &mut self.root_element,
            target_id,
            offset,
            SCROLL_VIEWPORT_FOCUS_PREFIX,
        );
    }

    #[cfg(debug_assertions)]
    pub(super) fn inject_hot_reload_partial(
        &mut self,
        new_root: Element,
        templates: &[crate::app::hot_reload::TemplateFragment],
    ) -> bool {
        if templates.is_empty() {
            return false;
        }
        let previous_overrides = self.hot_reload_overrides.clone();
        let mut any_applied = false;
        for tpl in templates {
            if tpl.text_replacements.is_empty() {
                if let Some(h) = self.last_applied_template_hash.get(&tpl.id) {
                    if h == &tpl.hash {
                        continue;
                    }
                }
            }
            any_applied = true;
            let mut replacements = tpl.text_replacements.clone();
            if replacements.is_empty() {
                replacements =
                    hot_reload_inject::derive_replacements_from_previous(&previous_overrides, tpl);
            }
            let mut effective_tpl = tpl.clone();
            effective_tpl.text_replacements = replacements;
            hot_reload_inject::upsert_overrides(
                &mut self.hot_reload_overrides,
                std::slice::from_ref(&effective_tpl),
            );
            self.last_applied_template_hash
                .insert(tpl.id.clone(), tpl.hash.clone());
        }
        if !any_applied {
            // 与上次已应用 hash 相同：不触发 fallback rebuild_ui（避免无意义整树重建）。
            return true;
        }
        self.rebuild_ui(new_root);
        true
    }

    /// 开发态：按路径重载内置 WGSL（当前仅 `rounded_rect.wgsl` 会作用于主渲染管线）。
    #[cfg(debug_assertions)]
    pub(super) fn reload_wgsl_assets(&mut self, paths: &[String]) -> bool {
        let mut reloaded = false;
        for p in paths {
            let Some(resolved) = hot_reload_inject::resolve_dev_asset_path(p) else {
                log::warn!("hot-reload asset: path not found {:?}", p);
                continue;
            };
            let is_rounded_rect = resolved
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.eq_ignore_ascii_case("rounded_rect.wgsl"));
            if !is_rounded_rect {
                continue;
            }
            let Ok(source) = std::fs::read_to_string(&resolved) else {
                log::warn!("hot-reload asset: failed to read {}", resolved.display());
                continue;
            };
            self.pipeline
                .rebuild_wgsl(&self.device, &self.config, &source);
            log::warn!("hot-reload asset: WGSL reloaded {}", resolved.display());
            reloaded = true;
        }
        reloaded
    }

    pub(super) fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width == 0 || new_size.height == 0 {
            return;
        }
        let max_dim = self.device.limits().max_texture_dimension_2d.max(1);
        let width = new_size.width.min(max_dim).max(1);
        let height = new_size.height.min(max_dim).max(1);
        if width != new_size.width || height != new_size.height {
            log::warn!(
                "clamping surface {}x{} to device max_texture_dimension_2d={}",
                new_size.width,
                new_size.height,
                max_dim
            );
        }
        let dims_changed = self.config.width != width || self.config.height != height;
        self.size = PhysicalSize::new(width, height);
        self.config.width = width;
        self.config.height = height;
        // Only reconfigure when the backing size changes. Calling `configure` on every occlusion
        // / redraw causes visible flicker; use `recover_surface` after `SurfaceError::Lost` instead.
        if dims_changed {
            self.surface.configure(&self.device, &self.config);
            self.pipeline
                .update_resolution(&self.queue, self.config.width, self.config.height);
            self.textured_pipeline.update_resolution(
                &self.queue,
                self.config.width,
                self.config.height,
            );
            self.layout_status = crate::core::tree::LayoutStatus::LayoutChanged;
            self.mark_full_redraw();
        }
    }

    /// Recreate the swapchain for the current `config` (same dimensions). Used after `Lost` /
    /// `Outdated` without a size change.
    pub(super) fn recover_surface(&mut self) {
        self.surface.configure(&self.device, &self.config);
        self.mark_full_redraw();
    }

    pub(super) fn render(
        &mut self,
        mouse_pos: [f32; 2],
        scrollbar_update: ScrollbarUpdateMode,
        active_drag_rest: Option<&str>,
        text_force_fast_mode: bool,
        // 无拇指拖拽时：overlay 弹簧或平滑滚会走 Full 预更新，但 flex 树尺寸不变，可跳过 compute_layout，
        // collect 用滚动信号画轨道/拇指（与 ViewportsOnly 同源路径）。
        scrollbar_skip_flex_layout: bool,
    ) -> Result<(), wgpu::SurfaceError> {
        // Scroll/drag fast path for prepare perf. Layout fit skip applies only during live
        // resize — scrolling must still compute/consult fit so bidi labels do not overflow.
        // Settle is deferred one frame after `inner_size` matches the last Resized event so the
        // final drag frame stays fluid; that next frame must run with fast=false + LayoutChanged.
        let settling = self.needs_text_settle && self.last_resized_event.is_none();
        if settling {
            self.needs_text_settle = false;
            self.layout_status = crate::core::tree::LayoutStatus::LayoutChanged;
            // Geometry may match the last fast frame; force prepare so Exact/Bounds apply fit wrap.
            self.last_text_nodes_cache.clear();
        }
        let layout_fast = self.last_resized_event.is_some();
        crate::text::set_layout_text_fast_mode(layout_fast);
        let text_fast = text_force_fast_mode || layout_fast;
        let _text_backend = self.text_backend;
        #[cfg(debug_assertions)]
        let render_begin = Instant::now();
        let inner = self.window.inner_size();
        if inner.width > 0 && inner.height > 0 {
            let target = Self::resolve_surface_size(inner, self.last_resized_event);
            if target.width != self.config.width || target.height != self.config.height {
                self.resize(target);
            }
            // Drop the event once `inner_size` matches (grow or shrink).
            if self
                .last_resized_event
                .is_some_and(|e| inner.width == e.width && inner.height == e.height)
            {
                self.last_resized_event = None;
                // This frame still ran with layout_fast; next frame settles (fit + remasure).
                self.needs_text_settle = true;
                self.window.request_redraw();
            }
        }

        // Chrome sync (hover/focus/input/switch) must not feed into scrollbar prepass or Taffy
        // rebuild gates: merging it into `layout_status` forced `Full` scrollbar passes and
        // `build_and_layout` every time a button shadow or hover border changed, which reset
        // scrollbar styles/springs and broke auto hide + caused flicker.
        let pre_chrome_layout_status = self.layout_status;

        let mut sync_impact = crate::core::tree::LayoutStatus::Clean;
        crate::core::tree::sync_reactive_elements_subtree(
            &mut self.root_element,
            &mut self.layout_tree,
            &mut sync_impact,
        );
        crate::components::button::sync_button_hover_paint_subtree(
            &mut self.root_element,
            &mut self.layout_tree,
            &mut sync_impact,
        );
        crate::components::switch::sync_switch_checked_subtree(
            &mut self.root_element,
            &mut self.layout_tree,
            &mut sync_impact,
        );
        crate::components::checkbox::sync_checkbox_checked_subtree(
            &mut self.root_element,
            &mut self.layout_tree,
            &mut sync_impact,
        );
        crate::components::radio_group::sync_radio_checked_subtree(
            &mut self.root_element,
            &mut self.layout_tree,
            &mut sync_impact,
        );
        crate::components::tabs::sync_tabs_trigger_subtree(
            &mut self.root_element,
            &mut self.layout_tree,
            &mut sync_impact,
        );
        // After reactive/toggle pressed shadows so keyboard ring can override without fighting.
        crate::style::sync_focus_ring_subtree(
            &mut self.root_element,
            &mut self.layout_tree,
            &mut sync_impact,
        );
        {
            let focused =
                crate::core::reactivity::use_signal("__rui_button_focus_id", String::new).peek();
            let dark = crate::style::is_dark_theme();
            let (viewport_w, _) = self.logical_size();
            crate::style::sync_class_variants_subtree(
                &mut self.root_element,
                mouse_pos,
                focused.as_str(),
                dark,
                viewport_w,
                &mut self.layout_tree,
                &mut sync_impact,
            );
        }
        // Input/label caret+selection after class variants so chrome resets cannot wipe
        // `text_style.input_caret` before collect.
        crate::components::internal::text_edit::sync_text_edit_paint_subtree(
            &mut self.root_element,
            &mut self.layout_tree,
            &mut sync_impact,
        );
        crate::components::label::sync_label_selection_paint_subtree(
            &mut self.root_element,
            &mut sync_impact,
        );

        use crate::core::tree::LayoutStatus;
        // Merge local chrome dirty rects (button hover, etc.) before deciding Clear vs Load.
        if let Some(local) = super::damage::take_frame_paint_damage() {
            self.union_damage(local);
        }
        // WP-2.2: paint-only StyleChanged must NOT expand to the full window (that forced
        // FullClear on every hover spring tick / bind_text). Structure / layout geometry
        // changes move pixels → PartialLoad+scissor is unsafe (must FullClear).
        match sync_impact {
            LayoutStatus::Clean | LayoutStatus::StyleChanged => {}
            LayoutStatus::LayoutChanged | LayoutStatus::StructureChanged => {
                self.mark_full_redraw();
            }
        }
        let layout_for_build = Self::merge_layout_status(pre_chrome_layout_status, sync_impact);

        if matches!(
            pre_chrome_layout_status,
            LayoutStatus::LayoutChanged | LayoutStatus::StructureChanged
        ) {
            self.mark_full_redraw();
        }

        let portal_moves = hoist_portal_subtrees_refreshing_scroll_flags(&mut self.root_element);

        // Viewport/content signals can be stale right after a resize+StructureChanged (not every
        // StructureChanged). In that narrow case, skip preupdate to avoid corrupt geometry from
        // previous-frame metrics. For normal StructureChanged frames, keep preupdate enabled so
        // tracks do not fall back to the builder-time 120px defaults for one frame.
        let skip_scroll_preupdate = matches!(
            pre_chrome_layout_status,
            crate::core::tree::LayoutStatus::StructureChanged
        ) && self.last_resized_event.is_some();
        let mut scrollbar_update_resolved = scrollbar_update;
        if !skip_scroll_preupdate {
            // Promote on element-tree StructureChanged from either reconcile (`layout_status`) or
            // same-frame `ReactiveSync` (e.g. VirtualList window remount). Do **not** promote on
            // chrome-only LayoutChanged inside sync_impact (hover/shadow) — that was the old
            // flicker path when chrome was merged into `layout_status`.
            scrollbar_update_resolved = match (pre_chrome_layout_status, sync_impact) {
                (LayoutStatus::StructureChanged | LayoutStatus::LayoutChanged, _)
                | (_, LayoutStatus::StructureChanged) => ScrollbarUpdateMode::Full,
                (LayoutStatus::StyleChanged | LayoutStatus::Clean, _) => scrollbar_update,
            };
            update_scrollbar_elements(
                &mut self.root_element,
                &mut self.scrollbar_base_style,
                &mut self.layout_tree,
                scrollbar_update_resolved,
                active_drag_rest,
            );
            // Content scroll moves every pixel in the viewport. If we PartialLoad with a
            // leftover hover/bind_text scissor, only that rect shows the new scroll offset
            // (gallery: hover then wheel → corruption, then FullClear jump).
            let scroll_fp = Self::scroll_offset_fingerprint(&self.root_element);
            if self.has_presented_frame && scroll_fp != self.last_scroll_offset_fp {
                self.mark_full_redraw();
            }
            self.last_scroll_offset_fp = scroll_fp;
        }
        #[cfg(debug_assertions)]
        let pre_done = Instant::now();
        #[cfg(debug_assertions)]
        {
            self.scroll_prepass_log_frames = self.scroll_prepass_log_frames.saturating_add(1);
            let elapsed = self.scroll_prepass_log_last.elapsed();
            // Time-based logging so FPS drops do not reduce log frequency.
            if elapsed >= std::time::Duration::from_secs(1) {
                let frames = self.scroll_prepass_log_frames.max(1);
                self.scroll_prepass_log_frames = 0;
                self.scroll_prepass_log_last = Instant::now();

                let total = SCROLL_PREPASS_RECURSE_COUNT.swap(0, Ordering::Relaxed);
                let viewports_only =
                    SCROLL_PREPASS_RECURSE_VIEWPORTS_ONLY.swap(0, Ordering::Relaxed);
                let full = SCROLL_PREPASS_RECURSE_FULL.swap(0, Ordering::Relaxed);

                log::warn!(
                    "scroll prepass avg(last ~{:.0}ms): fps≈{:.1}, total={:.2}, viewports_only={:.2}, full={:.2} nodes/frame",
                    elapsed.as_millis() as f32,
                    frames as f32 / elapsed.as_secs_f32().max(0.001),
                    total as f32 / frames as f32,
                    viewports_only as f32 / frames as f32,
                    full as f32 / frames as f32
                );
            }
        }

        let (logical_w, logical_h) = self.logical_size();
        // StructureChanged (and cold start / portal hoist) still need a full Taffy rebuild.
        // Include same-frame `ReactiveSync` StructureChanged (`sync_impact`): VirtualList replaces
        // body children while scroll uses `set_silent`, so `layout_status` stays Clean and the
        // fast path would collect with stale taffy child counts (element 23 vs taffy 17).
        // LayoutChanged: mark_dirty then compute from the **app root**. Do not use
        // `pick_subtree_compute_root` — Taffy treats the compute node as a layout root and
        // zeros its `location`, which collapses absolute positions for that whole branch
        // (gallery `<Show>` / `bind_text` LayoutChanged: card title overlapping buttons).
        // Root compute still reuses clean sibling caches via mark_dirty.
        let use_full_taffy_rebuild = self.layout_tree.root_node.is_none()
            || portal_moves > 0
            || matches!(
                layout_for_build,
                crate::core::tree::LayoutStatus::StructureChanged
            );
        let layout_subtree_root: Option<taffy::prelude::NodeId> = None;
        if !use_full_taffy_rebuild
            && (matches!(
                layout_for_build,
                crate::core::tree::LayoutStatus::LayoutChanged
            ) || !self.pending_layout_dirty.is_empty())
        {
            let dirty: std::collections::HashSet<_> = self.pending_layout_dirty.drain(..).collect();
            if dirty.is_empty() {
                // Resize settle / paint→fit relayout: available space often unchanged, so Taffy
                // will not remasure unless text leaves are marked dirty.
                self.layout_tree
                    .mark_all_text_leaves_dirty(&self.root_element);
            } else {
                self.layout_tree
                    .mark_dirty_nodes_in_tree(&self.root_element, &dirty);
            }
        } else if use_full_taffy_rebuild {
            self.pending_layout_dirty.clear();
        }
        // Guard against stale 120px fallback tracks:
        // if this frame needs full Taffy rebuild but prepass stayed ViewportsOnly,
        // track/thumb styles may remain old while collect uses non-overlay geometry.
        if use_full_taffy_rebuild
            && scrollbar_update_resolved == ScrollbarUpdateMode::ViewportsOnly
            && !skip_scroll_preupdate
        {
            update_scrollbar_elements(
                &mut self.root_element,
                &mut self.scrollbar_base_style,
                &mut self.layout_tree,
                ScrollbarUpdateMode::Full,
                active_drag_rest,
            );
            scrollbar_update_resolved = ScrollbarUpdateMode::Full;
        }
        // Scroll offsets do not affect Taffy sizing; skip full `compute_layout` when only
        // viewports were synced (no scrollbar thumb/style changes this frame).
        // `StyleChanged` from per-frame chrome（颜色/阴影等）不改变 flex 几何时可与 `Clean` 一样走
        // scroll-only；`LayoutChanged`（如边框宽度、下划线开关）仍必须跑 `compute_layout`。
        // Full + overlay 淡入淡出 / 平滑滚：预更新已写滚动条样式，flex 树在 Clean 下与上一帧同构。
        let scroll_only = (scrollbar_update_resolved == ScrollbarUpdateMode::ViewportsOnly
            && matches!(
                layout_for_build,
                crate::core::tree::LayoutStatus::Clean
                    | crate::core::tree::LayoutStatus::StyleChanged
            ))
            || (scrollbar_skip_flex_layout
                && scrollbar_update_resolved == ScrollbarUpdateMode::Full
                && matches!(
                    layout_for_build,
                    crate::core::tree::LayoutStatus::Clean
                        | crate::core::tree::LayoutStatus::StyleChanged
                ));
        // When the scrollbar prepass is `ViewportsOnly`, track/thumb Taffy styles are not updated.
        // Chrome may set `StyleChanged` while scrollbars use overlay collect — collect 用信号几何，
        // 避免仅 hover 颜色每帧全量 Taffy。
        // Keep overlay collect while dragging a thumb: idle frames paint via overlay, and
        // switching to Taffy on press used to jump the thumb by the track border (~1px down-right).
        let scrollbar_overlay_collect = scrollbar_update_resolved
            == ScrollbarUpdateMode::ViewportsOnly
            || (scrollbar_skip_flex_layout
                && scrollbar_update_resolved == ScrollbarUpdateMode::Full
                && scroll_only)
            || active_drag_rest.is_some();
        let mut render_result = if use_full_taffy_rebuild {
            self.layout_tree.build_and_layout(
                &mut self.root_element,
                logical_w,
                logical_h,
                mouse_pos,
            )
        } else {
            self.layout_tree.build_and_layout_fast_ex(
                &mut self.root_element,
                logical_w,
                logical_h,
                mouse_pos,
                scroll_only,
                scrollbar_overlay_collect,
                layout_subtree_root,
            )
        };
        // 浮层：首帧用占位坐标，布局后按触发器/视口纠正。滚动时只改 left/top —
        // 只 dirty 浮层节点 + fast compute，禁止每帧全量重建 Taffy（打开 DatePicker 后整页滚动卡顿）。
        if crate::utils::popup::sync_floating_placement(
            &mut self.root_element,
            logical_w,
            logical_h,
        ) {
            crate::utils::popup::visit_floating_panels(&self.root_element, |el| {
                self.layout_tree.mark_element_layout_dirty(el);
            });
            render_result = self.layout_tree.build_and_layout_fast_ex(
                &mut self.root_element,
                logical_w,
                logical_h,
                mouse_pos,
                false,
                scrollbar_overlay_collect,
                None,
            );
        }
        #[cfg(debug_assertions)]
        let layout_done = Instant::now();

        let overlay_split = render_result.overlay_split;
        let scaled_instances = scale_instances(&render_result.instances, self.ui_scale());
        let scaled_input_chrome =
            scale_instances(&render_result.input_chrome_instances, self.ui_scale());
        self.num_instances = render_result.instances.len() as u32;
        self.text_nodes = render_result.text_nodes;
        let overlay_inst_start = overlay_split.instance_start.min(scaled_instances.len());
        let overlay_text_start = overlay_split.text_start.min(self.text_nodes.len());
        let overlay_tex_start = overlay_split
            .textured_start
            .min(render_result.textured_draws.len());
        let overlay_chrome_start = overlay_split.chrome_start.min(scaled_input_chrome.len());
        let overlay_paint_start = overlay_split
            .paint_op_start
            .min(render_result.paint_ops.len());
        // Portal overlays span arbitrary screen regions; partial-damage scissors leave
        // stale page chrome and clipped panels (esp. GLES). Always clear the full frame.
        if overlay_inst_start < scaled_instances.len()
            || overlay_text_start < self.text_nodes.len()
            || overlay_tex_start < render_result.textured_draws.len()
            || overlay_chrome_start < scaled_input_chrome.len()
        {
            self.mark_full_redraw();
        }
        #[cfg(debug_assertions)]
        {
            let p = &render_result.prof;
            self.render_prof_layout_rebuild_ms += p.rebuild_ms;
            self.render_prof_layout_setup_ms += p.setup_ms;
            self.render_prof_layout_taffy_ms += p.taffy_compute_ms;
            self.render_prof_layout_collect_ms += p.collect_ms;
        }

        // Export collected metrics and hover state for window_events.rs to avoid redundant traversals.
        self.last_render_scroll_metrics = render_result.scroll_metrics;
        self.last_hover_cursor = render_result.hover_cursor;

        if scaled_instances.len() > self.instance_buffer_capacity {
            self.instance_buffer =
                self.device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Instance Buffer"),
                        contents: bytemuck::cast_slice(&scaled_instances),
                        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    });
            self.instance_buffer_capacity = scaled_instances.len();
        } else {
            self.queue.write_buffer(
                &self.instance_buffer,
                0,
                bytemuck::cast_slice(&scaled_instances),
            );
        }
        self.layout_status = crate::core::tree::LayoutStatus::Clean;
        // Persist settle intent across the Clean reset above (next frame remasures + reshapes).
        if self.needs_text_settle {
            self.layout_status = crate::core::tree::LayoutStatus::LayoutChanged;
        }

        // Keep the shader `globals.resolution` aligned with the surface every frame (avoids one-frame
        // mismatch if `configure` / layout / uniform updates reorder across events).
        self.pipeline
            .update_resolution(&self.queue, self.config.width, self.config.height);
        self.textured_pipeline.update_resolution(
            &self.queue,
            self.config.width,
            self.config.height,
        );
        #[cfg(debug_assertions)]
        let upload_done = Instant::now();

        let ui_scale = self.ui_scale();

        for (id, rgba, w, h) in super::pending_images::take_pending() {
            let gpu = super::image_registry::upload_rgba8(
                &self.device,
                &self.queue,
                &self.textured_pipeline,
                &rgba,
                w,
                h,
            );
            self.image_textures.insert(id, gpu);
        }
        let scaled_tex = scale_textured_draws(&render_result.textured_draws, ui_scale);
        let tex_insts: Vec<TexturedInstanceInput> =
            scaled_tex.iter().map(|d| d.to_instance()).collect();
        upload_textured_instance_buffer(
            &self.device,
            &self.queue,
            &mut self.textured_instance_buffer,
            &mut self.textured_instance_capacity,
            &tex_insts,
        );
        let has_dynamic_text = self
            .text_nodes
            .iter()
            .any(|n| n.style.input_caret.is_some());
        let caret_phase = crate::renderer::caret_blink_phase();
        if has_dynamic_text && self.last_caret_blink_phase != caret_phase {
            // Blink toggles only underline instances — force a full present so a stale
            // partial-damage scissor cannot freeze the caret on/off.
            self.mark_full_redraw();
        }
        // Portal overlay text must be prepared/rendered in a second pass (after overlay rects).
        // Skip the single-buffer text cache whenever overlay glyphs exist.
        let has_overlay_text = overlay_text_start < self.text_nodes.len();
        let text_cache_valid = !has_overlay_text
            && self.last_text_ui_scale.to_bits() == ui_scale.to_bits()
            && self.last_text_surface_size == [self.config.width, self.config.height]
            && Self::text_nodes_equal(
                &self.text_nodes[..overlay_text_start],
                &self.last_text_nodes_cache,
            )
            && (!has_dynamic_text || self.last_caret_blink_phase == caret_phase)
            && self.text_renderer.pending_warmup.is_empty();
        let shadow_cached_keys: std::collections::HashSet<u64> =
            self.shadow_tex_cache.keys().copied().collect();
        let (underline_instances, caret_instances, shadow_tex_draws) = if text_cache_valid {
            (
                self.last_underline_instances.clone(),
                self.last_caret_instances.clone(),
                self.last_shadow_tex_draws.clone(),
            )
        } else {
            let text_prepare_begin = Instant::now();
            let prep_out = prepare_text_frame(
                &mut self.text_renderer,
                &self.device,
                &self.queue,
                &self.config,
                &self.text_nodes[..overlay_text_start],
                ui_scale,
                text_fast,
                &shadow_cached_keys,
                &self.shadow_rect_cache,
            );
            // Mid-resize cull race: atlas left intact; do not promote an empty prepare into
            // text_cache_valid or the blank frame sticks until geometry drifts.
            if prep_out.culled_skip_prepare {
                self.window.request_redraw();
                (
                    self.last_underline_instances.clone(),
                    self.last_caret_instances.clone(),
                    self.last_shadow_tex_draws.clone(),
                )
            } else {
                for bm in &prep_out.shadow_new_bitmaps {
                    let gpu = super::image_registry::upload_rgba8(
                        &self.device,
                        &self.queue,
                        &self.textured_pipeline,
                        &bm.rgba,
                        bm.width,
                        bm.height,
                    );
                    self.shadow_tex_cache.insert(bm.cache_key, gpu);
                    self.shadow_rect_cache.insert(bm.cache_key, bm.draw_rect);
                }
                self.last_text_nodes_cache = self.text_nodes[..overlay_text_start].to_vec();
                self.last_text_ui_scale = ui_scale;
                self.last_text_surface_size = [self.config.width, self.config.height];
                self.last_underline_instances = prep_out.underline_instances.clone();
                self.last_caret_instances = prep_out.caret_instances.clone();
                self.last_shadow_tex_draws = prep_out.shadow_tex_draws.clone();
                self.last_caret_blink_phase = caret_phase;
                if self.text_backend == TextRendererBackend::CpuTextHybrid {
                    self.cpu_text_profiler.record(
                        (Instant::now() - text_prepare_begin).as_secs_f64() * 1000.0,
                        overlay_text_start,
                    );
                }
                if prep_out.needs_warmup {
                    self.window.request_redraw();
                }
                // P1: paint spill→fit may need a taller leaf; remeasure next frame from FIT_CACHE.
                if crate::text::take_fit_needs_relayout() {
                    self.layout_status = crate::core::tree::LayoutStatus::LayoutChanged;
                    self.window.request_redraw();
                }
                (
                    prep_out.underline_instances,
                    prep_out.caret_instances,
                    prep_out.shadow_tex_draws,
                )
            }
        };

        // Prepare overlay glyphs per portal paint band (separate atlas/renderer each) so the
        // Clear pass can draw `band rects → band text → next band` and higher panels occlude
        // lower glyphs — still one Clear, no GLES LoadOp::Load.
        let overlay_band_ranges = overlay_split.band_ranges(
            scaled_instances.len(),
            scaled_tex.len(),
            self.text_nodes.len(),
            scaled_input_chrome.len(),
            render_result.paint_ops.len(),
        );
        self.ensure_overlay_band_text_renderers(overlay_band_ranges.len());
        let mut overlay_band_underlines: Vec<Vec<InstanceInput>> =
            Vec::with_capacity(overlay_band_ranges.len());
        let mut overlay_needs_warmup = false;
        for (band_i, band) in overlay_band_ranges.iter().enumerate() {
            let text_lo = band.text.0;
            let text_hi = band.text.1;
            let shadow_cached_keys: std::collections::HashSet<u64> =
                self.shadow_tex_cache.keys().copied().collect();
            let overlay_prep = if band_i == 0 {
                prepare_text_frame(
                    &mut self.overlay_text_renderer,
                    &self.device,
                    &self.queue,
                    &self.config,
                    &self.text_nodes[text_lo..text_hi],
                    ui_scale,
                    text_fast,
                    &shadow_cached_keys,
                    &self.shadow_rect_cache,
                )
            } else {
                prepare_text_frame(
                    &mut self.overlay_band_text_renderers[band_i - 1],
                    &self.device,
                    &self.queue,
                    &self.config,
                    &self.text_nodes[text_lo..text_hi],
                    ui_scale,
                    text_fast,
                    &shadow_cached_keys,
                    &self.shadow_rect_cache,
                )
            };
            for bm in &overlay_prep.shadow_new_bitmaps {
                let gpu = super::image_registry::upload_rgba8(
                    &self.device,
                    &self.queue,
                    &self.textured_pipeline,
                    &bm.rgba,
                    bm.width,
                    bm.height,
                );
                self.shadow_tex_cache.insert(bm.cache_key, gpu);
                self.shadow_rect_cache.insert(bm.cache_key, bm.draw_rect);
            }
            if overlay_prep.needs_warmup {
                overlay_needs_warmup = true;
            }
            overlay_band_underlines.push(overlay_prep.underline_instances);
        }
        if overlay_needs_warmup {
            self.window.request_redraw();
        }
        if crate::text::take_fit_needs_relayout() {
            self.layout_status = crate::core::tree::LayoutStatus::LayoutChanged;
            self.window.request_redraw();
        }
        #[cfg(debug_assertions)]
        let text_done = Instant::now();

        // Skip swapchain acquire/submit/present when this redraw produced the same GPU content
        // as the last presented frame (e.g. scrollbar visibility timer wake with no fade yet).
        {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            let mut hasher = DefaultHasher::new();
            hasher.write(bytemuck::cast_slice(&scaled_instances));
            hasher.write(bytemuck::cast_slice(&scaled_input_chrome));
            for d in &scaled_tex {
                d.texture_key.hash(&mut hasher);
                d.opacity.to_bits().hash(&mut hasher);
                for v in d.rect {
                    v.to_bits().hash(&mut hasher);
                }
            }
            hasher.write(bytemuck::cast_slice(&underline_instances));
            hasher.write(bytemuck::cast_slice(&caret_instances));
            // Text glyphs live in the atlas, not in rect instances — include content so
            // StyleChanged counters (same box, new string) still present.
            self.text_nodes.len().hash(&mut hasher);
            for n in &self.text_nodes {
                n.text.hash(&mut hasher);
                n.shape_suffix.hash(&mut hasher);
                n.template_id.hash(&mut hasher);
                n.x.to_bits().hash(&mut hasher);
                n.y.to_bits().hash(&mut hasher);
                n.width.to_bits().hash(&mut hasher);
                n.height.to_bits().hash(&mut hasher);
                n.style.color.hash(&mut hasher);
                n.style.size.to_bits().hash(&mut hasher);
                n.style.weight.hash(&mut hasher);
                // Caret index / selection must affect present even when underline instance
                // bytes happen to collide (e.g. empty selection vs no caret).
                match &n.style.input_caret {
                    None => 0u8.hash(&mut hasher),
                    Some(c) => {
                        1u8.hash(&mut hasher);
                        c.caret_byte_index.hash(&mut hasher);
                        c.selection.hash(&mut hasher);
                        c.overwrite_highlight.hash(&mut hasher);
                        c.selection_bg.hash(&mut hasher);
                    }
                }
            }
            caret_phase.hash(&mut hasher);
            shadow_tex_draws.len().hash(&mut hasher);
            for d in &shadow_tex_draws {
                d.cache_key.hash(&mut hasher);
                for v in d.rect {
                    v.to_bits().hash(&mut hasher);
                }
            }
            let frame_fp = hasher.finish();
            if self.has_presented_frame
                && !self.force_full_redraw
                && frame_fp == self.last_gpu_frame_fp
            {
                if !self.needs_text_settle {
                    self.layout_status = crate::core::tree::LayoutStatus::Clean;
                }
                return Ok(());
            }
            self.last_gpu_frame_fp = frame_fp;
        }

        // Acquire swapchain image as late as possible so CPU-side text/layout work can overlap
        // with present pacing; this typically reduces blocking time in get_current_texture().
        #[cfg(debug_assertions)]
        let acquire_begin = Instant::now();
        let output = self.surface.get_current_texture()?;
        #[cfg(debug_assertions)]
        let acquire_done = Instant::now();
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

        let (logical_w, logical_h) = self.logical_size();
        let viewport_logical = [0.0, 0.0, logical_w, logical_h];
        // Logical Clear vs Load (damage tracking / Gallery). Solid-aligned: fine-grained
        // *tree* updates (bind_*) then one coherent paint — not a half-updated framebuffer.
        // GPU PartialLoad (retain Load+scissor+full blit) is opt-in via RUI_PARTIAL_DAMAGE=1:
        // without draw-list culling it often costs more than a surface FullClear and can miss
        // Input invalid chrome. Default off = pre-WP-2.2 present path.
        let mut logical_policy = super::damage::frame_clear_policy(
            self.has_presented_frame,
            self.force_full_redraw,
            self.damage_rect,
            viewport_logical,
        );
        let mut scissor_physical = None;
        if matches!(logical_policy, super::damage::FrameClearPolicy::PartialLoad) {
            scissor_physical = self.damage_rect.and_then(|damage| {
                super::damage::damage_to_scissor_physical(
                    damage,
                    Self::DAMAGE_SCISSOR_PAD_LOGICAL,
                    ui_scale,
                    self.config.width,
                    self.config.height,
                )
            });
            if scissor_physical.is_none() {
                logical_policy = super::damage::FrameClearPolicy::FullClear;
            }
        }
        let want_gpu_partial = Self::gpu_partial_damage_enabled()
            && matches!(logical_policy, super::damage::FrameClearPolicy::PartialLoad);
        // Experimental path only (`RUI_PARTIAL_DAMAGE=1`): Load when retain is warm; otherwise
        // one FullClear-into-retain to prime. Default builds never enter this branch.
        let use_partial_damage = want_gpu_partial && self.present_retain.contents_valid();
        let warm_retain = want_gpu_partial && !self.present_retain.contents_valid();
        let draw_to_retain = use_partial_damage || warm_retain;
        super::damage::note_frame_clear_policy(logical_policy);
        if draw_to_retain {
            let _ = self.present_retain.ensure_size(
                &self.device,
                self.config.width,
                self.config.height,
            );
        } else {
            self.present_retain.invalidate_contents();
        }
        let clear_color = wgpu::Color {
            r: 0.95,
            g: 0.95,
            b: 0.95,
            a: 1.0,
        };
        let color_load = if use_partial_damage {
            wgpu::LoadOp::Load
        } else {
            wgpu::LoadOp::Clear(clear_color)
        };
        if !use_partial_damage {
            scissor_physical = None;
        }

        let max_backdrop_blur_logical = overlay_split
            .bands
            .iter()
            .map(|b| b.backdrop_blur)
            .fold(0.0_f32, f32::max);
        let allow_primitives = !(self.text_backend == TextRendererBackend::CpuTextHybrid
            && Self::cpu_hybrid_text_only());
        // Fullscreen modal underlay blur: capture page → blur → composite under overlays.
        let use_backdrop_blur = allow_primitives
            && max_backdrop_blur_logical > 0.5
            && overlay_inst_start > 0
            && !use_partial_damage;
        if use_backdrop_blur {
            self.backdrop_blur.ensure_size(
                &self.device,
                self.config.width,
                self.config.height,
                self.config.format,
            );
            if let Some(scene_view) = self.backdrop_blur.scene_view() {
                {
                    let mut page_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("Backdrop Scene Pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: scene_view,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(clear_color),
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: None,
                        timestamp_writes: None,
                        occlusion_query_set: None,
                    });
                    page_pass.set_pipeline(&self.pipeline.render_pipeline);
                    page_pass.set_bind_group(0, &self.pipeline.bind_group, &[]);
                    page_pass.set_vertex_buffer(0, self.pipeline.vertex_buffer.slice(..));
                    page_pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
                    page_pass.set_index_buffer(
                        self.pipeline.index_buffer.slice(..),
                        wgpu::IndexFormat::Uint16,
                    );
                    if overlay_paint_start > 0 {
                        draw_paint_ops(
                            &mut page_pass,
                            &render_result.paint_ops,
                            0,
                            overlay_paint_start,
                            &self.pipeline,
                            &self.instance_buffer,
                            &self.textured_pipeline,
                            &self.textured_instance_buffer,
                            &self.image_textures,
                            &scaled_tex,
                        );
                    } else {
                        page_pass.draw_indexed(
                            0..self.pipeline.num_indices,
                            0,
                            0..overlay_inst_start as u32,
                        );
                        if overlay_tex_start > 0 {
                            draw_packed_textured_runs(
                                &mut page_pass,
                                &self.textured_pipeline,
                                &self.textured_instance_buffer,
                                &self.image_textures,
                                &scaled_tex,
                                0,
                                overlay_tex_start,
                            );
                        }
                    }
                    if !underline_instances.is_empty() {
                        page_pass.set_pipeline(&self.pipeline.render_pipeline);
                        page_pass.set_bind_group(0, &self.pipeline.bind_group, &[]);
                        page_pass.set_vertex_buffer(0, self.pipeline.vertex_buffer.slice(..));
                        let underline_buffer =
                            self.device
                                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                                    label: Some("Underline Instance Buffer (scene)"),
                                    contents: bytemuck::cast_slice(&underline_instances),
                                    usage: wgpu::BufferUsages::VERTEX,
                                });
                        page_pass.set_vertex_buffer(1, underline_buffer.slice(..));
                        page_pass.set_index_buffer(
                            self.pipeline.index_buffer.slice(..),
                            wgpu::IndexFormat::Uint16,
                        );
                        page_pass.draw_indexed(
                            0..self.pipeline.num_indices,
                            0,
                            0..underline_instances.len() as u32,
                        );
                    }
                    self.text_renderer.render(&mut page_pass);
                    if overlay_chrome_start > 0 {
                        page_pass.set_pipeline(&self.pipeline.render_pipeline);
                        page_pass.set_bind_group(0, &self.pipeline.bind_group, &[]);
                        page_pass.set_vertex_buffer(0, self.pipeline.vertex_buffer.slice(..));
                        let chrome_slice = &scaled_input_chrome[..overlay_chrome_start];
                        let chrome_buffer =
                            self.device
                                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                                    label: Some("Input Chrome Instance Buffer (scene)"),
                                    contents: bytemuck::cast_slice(chrome_slice),
                                    usage: wgpu::BufferUsages::VERTEX,
                                });
                        page_pass.set_vertex_buffer(1, chrome_buffer.slice(..));
                        page_pass.set_index_buffer(
                            self.pipeline.index_buffer.slice(..),
                            wgpu::IndexFormat::Uint16,
                        );
                        page_pass.draw_indexed(
                            0..self.pipeline.num_indices,
                            0,
                            0..chrome_slice.len() as u32,
                        );
                    }
                }
                self.backdrop_blur.blur(
                    &self.device,
                    &self.queue,
                    &mut encoder,
                    max_backdrop_blur_logical * ui_scale,
                );
                // Same UV space as capture — not TexturedQuadPipeline (Y-flipped for images).
                let blur_dst = if draw_to_retain {
                    self.present_retain.color_view()
                } else {
                    Some(&view)
                };
                if let Some(dst) = blur_dst {
                    self.backdrop_blur.blit_to(&self.queue, &mut encoder, dst);
                }
            }
        }

        let color_load = if use_backdrop_blur {
            wgpu::LoadOp::Load
        } else {
            color_load
        };

        {
            let color_view = if draw_to_retain {
                self.present_retain
                    .color_view()
                    .ok_or(wgpu::SurfaceError::Lost)?
            } else {
                &view
            };
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: color_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: color_load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            if let Some((x, y, w, h)) = scissor_physical {
                render_pass.set_scissor_rect(x, y, w, h);
            }

            render_pass.set_pipeline(&self.pipeline.render_pipeline);
            render_pass.set_bind_group(0, &self.pipeline.bind_group, &[]);
            render_pass.set_vertex_buffer(0, self.pipeline.vertex_buffer.slice(..));
            render_pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
            render_pass.set_index_buffer(
                self.pipeline.index_buffer.slice(..),
                wgpu::IndexFormat::Uint16,
            );
            // Page layer: tree-order rects + textures (Solid/CSS stacking with portal overlays).
            // Skipped when backdrop blur already composited the page.
            if allow_primitives && !use_backdrop_blur {
                if overlay_paint_start > 0 {
                    draw_paint_ops(
                        &mut render_pass,
                        &render_result.paint_ops,
                        0,
                        overlay_paint_start,
                        &self.pipeline,
                        &self.instance_buffer,
                        &self.textured_pipeline,
                        &self.textured_instance_buffer,
                        &self.image_textures,
                        &scaled_tex,
                    );
                } else if overlay_inst_start > 0 {
                    render_pass.draw_indexed(
                        0..self.pipeline.num_indices,
                        0,
                        0..overlay_inst_start as u32,
                    );
                    if overlay_tex_start > 0 {
                        draw_packed_textured_runs(
                            &mut render_pass,
                            &self.textured_pipeline,
                            &self.textured_instance_buffer,
                            &self.image_textures,
                            &scaled_tex,
                            0,
                            overlay_tex_start,
                        );
                    }
                }
            }

            if !use_backdrop_blur {
                for sd in &shadow_tex_draws {
                    let Some(gpu_img) = self.shadow_tex_cache.get(&sd.cache_key) else {
                        continue;
                    };
                    let inst = TexturedInstanceInput {
                        rect_pos: [sd.rect[0], sd.rect[1]],
                        rect_size: [sd.rect[2], sd.rect[3]],
                        tint: [1.0, 1.0, 1.0, 1.0],
                        uv_offset: [0.0, 0.0],
                        uv_scale: [1.0, 1.0],
                    };
                    let shadow_inst_buf =
                        self.device
                            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                                label: Some("Shadow instance"),
                                contents: bytemuck::cast_slice(&[inst]),
                                usage: wgpu::BufferUsages::VERTEX,
                            });
                    render_pass.set_pipeline(&self.textured_pipeline.render_pipeline);
                    render_pass.set_bind_group(0, &gpu_img.bind_group, &[]);
                    render_pass
                        .set_vertex_buffer(0, self.textured_pipeline.vertex_buffer.slice(..));
                    render_pass.set_vertex_buffer(1, shadow_inst_buf.slice(..));
                    render_pass.set_index_buffer(
                        self.textured_pipeline.index_buffer.slice(..),
                        wgpu::IndexFormat::Uint16,
                    );
                    render_pass.draw_indexed(0..self.textured_pipeline.num_indices, 0, 0..1);
                }
            }

            if !underline_instances.is_empty() && !use_backdrop_blur {
                render_pass.set_pipeline(&self.pipeline.render_pipeline);
                render_pass.set_bind_group(0, &self.pipeline.bind_group, &[]);
                render_pass.set_vertex_buffer(0, self.pipeline.vertex_buffer.slice(..));
                let underline_buffer =
                    self.device
                        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("Underline Instance Buffer"),
                            contents: bytemuck::cast_slice(&underline_instances),
                            usage: wgpu::BufferUsages::VERTEX,
                        });
                render_pass.set_vertex_buffer(1, underline_buffer.slice(..));
                render_pass.set_index_buffer(
                    self.pipeline.index_buffer.slice(..),
                    wgpu::IndexFormat::Uint16,
                );
                render_pass.draw_indexed(
                    0..self.pipeline.num_indices,
                    0,
                    0..underline_instances.len() as u32,
                );
            }

            if !use_backdrop_blur {
                self.text_renderer.render(&mut render_pass);
            }

            // Page input chrome above page glyphs, below portal overlay.
            if allow_primitives && overlay_chrome_start > 0 && !use_backdrop_blur {
                render_pass.set_pipeline(&self.pipeline.render_pipeline);
                render_pass.set_bind_group(0, &self.pipeline.bind_group, &[]);
                render_pass.set_vertex_buffer(0, self.pipeline.vertex_buffer.slice(..));
                let chrome_slice = &scaled_input_chrome[..overlay_chrome_start];
                let chrome_buffer =
                    self.device
                        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("Input Chrome Instance Buffer"),
                            contents: bytemuck::cast_slice(chrome_slice),
                            usage: wgpu::BufferUsages::VERTEX,
                        });
                render_pass.set_vertex_buffer(1, chrome_buffer.slice(..));
                render_pass.set_index_buffer(
                    self.pipeline.index_buffer.slice(..),
                    wgpu::IndexFormat::Uint16,
                );
                render_pass.draw_indexed(
                    0..self.pipeline.num_indices,
                    0,
                    0..chrome_slice.len() as u32,
                );
            }

            // Legacy prepare_text caret path (kept empty; chrome comes from collect).
            if !caret_instances.is_empty() {
                render_pass.set_pipeline(&self.pipeline.render_pipeline);
                render_pass.set_bind_group(0, &self.pipeline.bind_group, &[]);
                render_pass.set_vertex_buffer(0, self.pipeline.vertex_buffer.slice(..));
                let caret_buffer =
                    self.device
                        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("Caret Instance Buffer"),
                            contents: bytemuck::cast_slice(&caret_instances),
                            usage: wgpu::BufferUsages::VERTEX,
                        });
                render_pass.set_vertex_buffer(1, caret_buffer.slice(..));
                render_pass.set_index_buffer(
                    self.pipeline.index_buffer.slice(..),
                    wgpu::IndexFormat::Uint16,
                );
                render_pass.draw_indexed(
                    0..self.pipeline.num_indices,
                    0,
                    0..caret_instances.len() as u32,
                );
            }

            // Portal overlay: per-band `rects → text` so stacked panels occlude correctly
            // (still one Clear pass — no GLES LoadOp::Load).
            for (band_i, band) in overlay_band_ranges.iter().enumerate() {
                if allow_primitives && band.paint_ops.0 < band.paint_ops.1 {
                    draw_paint_ops(
                        &mut render_pass,
                        &render_result.paint_ops,
                        band.paint_ops.0,
                        band.paint_ops.1,
                        &self.pipeline,
                        &self.instance_buffer,
                        &self.textured_pipeline,
                        &self.textured_instance_buffer,
                        &self.image_textures,
                        &scaled_tex,
                    );
                } else if allow_primitives && band.instance.0 < band.instance.1 {
                    render_pass.set_pipeline(&self.pipeline.render_pipeline);
                    render_pass.set_bind_group(0, &self.pipeline.bind_group, &[]);
                    render_pass.set_vertex_buffer(0, self.pipeline.vertex_buffer.slice(..));
                    render_pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
                    render_pass.set_index_buffer(
                        self.pipeline.index_buffer.slice(..),
                        wgpu::IndexFormat::Uint16,
                    );
                    render_pass.draw_indexed(
                        0..self.pipeline.num_indices,
                        0,
                        band.instance.0 as u32..band.instance.1 as u32,
                    );
                    if band.textured.0 < band.textured.1 {
                        draw_packed_textured_runs(
                            &mut render_pass,
                            &self.textured_pipeline,
                            &self.textured_instance_buffer,
                            &self.image_textures,
                            &scaled_tex,
                            band.textured.0,
                            band.textured.1,
                        );
                    }
                }

                if allow_primitives && band.chrome.0 < band.chrome.1 {
                    render_pass.set_pipeline(&self.pipeline.render_pipeline);
                    render_pass.set_bind_group(0, &self.pipeline.bind_group, &[]);
                    render_pass.set_vertex_buffer(0, self.pipeline.vertex_buffer.slice(..));
                    let chrome_slice = &scaled_input_chrome[band.chrome.0..band.chrome.1];
                    let chrome_buffer =
                        self.device
                            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                                label: Some("Overlay Input Chrome Buffer"),
                                contents: bytemuck::cast_slice(chrome_slice),
                                usage: wgpu::BufferUsages::VERTEX,
                            });
                    render_pass.set_vertex_buffer(1, chrome_buffer.slice(..));
                    render_pass.set_index_buffer(
                        self.pipeline.index_buffer.slice(..),
                        wgpu::IndexFormat::Uint16,
                    );
                    render_pass.draw_indexed(
                        0..self.pipeline.num_indices,
                        0,
                        0..chrome_slice.len() as u32,
                    );
                }

                if let Some(ul) = overlay_band_underlines.get(band_i) {
                    if !ul.is_empty() {
                        render_pass.set_pipeline(&self.pipeline.render_pipeline);
                        render_pass.set_bind_group(0, &self.pipeline.bind_group, &[]);
                        render_pass.set_vertex_buffer(0, self.pipeline.vertex_buffer.slice(..));
                        let underline_buffer =
                            self.device
                                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                                    label: Some("Overlay Underline Buffer"),
                                    contents: bytemuck::cast_slice(ul),
                                    usage: wgpu::BufferUsages::VERTEX,
                                });
                        render_pass.set_vertex_buffer(1, underline_buffer.slice(..));
                        render_pass.set_index_buffer(
                            self.pipeline.index_buffer.slice(..),
                            wgpu::IndexFormat::Uint16,
                        );
                        render_pass.draw_indexed(
                            0..self.pipeline.num_indices,
                            0,
                            0..ul.len() as u32,
                        );
                    }
                }

                if band.text.0 < band.text.1 {
                    if band_i == 0 {
                        self.overlay_text_renderer.render(&mut render_pass);
                    } else {
                        self.overlay_band_text_renderers[band_i - 1].render(&mut render_pass);
                    }
                }
            }
        }
        if draw_to_retain {
            self.present_retain
                .blit_to_surface(&mut encoder, &view, clear_color);
            self.present_retain.mark_contents_valid();
        } else {
            // Surface-direct FullClear: retain no longer matches the presented frame.
            self.present_retain.invalidate_contents();
        }
        #[cfg(debug_assertions)]
        let encode_done = Instant::now();

        #[cfg(debug_assertions)]
        let submit_begin = Instant::now();
        self.queue.submit(std::iter::once(encoder.finish()));
        #[cfg(debug_assertions)]
        let submit_done = Instant::now();
        #[cfg(debug_assertions)]
        let present_begin = Instant::now();
        output.present();
        self.damage_rect = None;
        self.force_full_redraw = false;
        self.has_presented_frame = true;
        #[cfg(debug_assertions)]
        {
            let draw_done = Instant::now();
            self.render_prof_frames = self.render_prof_frames.saturating_add(1);
            self.render_prof_pre_ms += (pre_done - render_begin).as_secs_f32() * 1000.0;
            self.render_prof_layout_ms += (layout_done - pre_done).as_secs_f32() * 1000.0;
            self.render_prof_draw_ms += (draw_done - layout_done).as_secs_f32() * 1000.0;
            self.render_prof_acquire_ms += (acquire_done - acquire_begin).as_secs_f32() * 1000.0;
            self.render_prof_submit_ms += (submit_done - submit_begin).as_secs_f32() * 1000.0;
            self.render_prof_present_ms += (draw_done - present_begin).as_secs_f32() * 1000.0;
            self.render_prof_upload_ms += (upload_done - layout_done).as_secs_f32() * 1000.0;
            // text phase is CPU glyph prep before swapchain acquire.
            self.render_prof_text_ms += (text_done - upload_done).as_secs_f32() * 1000.0;
            // encode phase starts after acquire and includes render-pass encoding.
            self.render_prof_encode_ms += (encode_done - acquire_done).as_secs_f32() * 1000.0;

            let elapsed = self.render_prof_last.elapsed();
            if elapsed >= std::time::Duration::from_secs(1) {
                let f = self.render_prof_frames.max(1) as f32;
                log::warn!(
                    "render profile(last ~{:.0}ms): fps≈{:.1}, pre={:.2}ms, layout={:.2}ms, draw={:.2}ms [layout_tree: rebuild={:.2} setup={:.2} taffy={:.2} collect={:.2}] [upload={:.2}, acquire={:.2}, text={:.2}, encode={:.2}, submit={:.2}, present={:.2}]",
                    elapsed.as_millis() as f32,
                    f / elapsed.as_secs_f32().max(0.001),
                    self.render_prof_pre_ms / f,
                    self.render_prof_layout_ms / f,
                    self.render_prof_draw_ms / f,
                    self.render_prof_layout_rebuild_ms / f,
                    self.render_prof_layout_setup_ms / f,
                    self.render_prof_layout_taffy_ms / f,
                    self.render_prof_layout_collect_ms / f,
                    self.render_prof_upload_ms / f,
                    self.render_prof_acquire_ms / f,
                    self.render_prof_text_ms / f,
                    self.render_prof_encode_ms / f,
                    self.render_prof_submit_ms / f,
                    self.render_prof_present_ms / f
                );
                self.render_prof_last = Instant::now();
                self.render_prof_frames = 0;
                self.render_prof_pre_ms = 0.0;
                self.render_prof_layout_ms = 0.0;
                self.render_prof_layout_rebuild_ms = 0.0;
                self.render_prof_layout_setup_ms = 0.0;
                self.render_prof_layout_taffy_ms = 0.0;
                self.render_prof_layout_collect_ms = 0.0;
                self.render_prof_draw_ms = 0.0;
                self.render_prof_acquire_ms = 0.0;
                self.render_prof_submit_ms = 0.0;
                self.render_prof_present_ms = 0.0;
                self.render_prof_upload_ms = 0.0;
                self.render_prof_text_ms = 0.0;
                self.render_prof_encode_ms = 0.0;
            }
        }

        Ok(())
    }
}

#[derive(Clone, Copy)]
struct ScrollbarBaseStyle {
    track_bg: [f32; 4],
    thumb_bg: [f32; 4],
    track_border: [f32; 4],
    thumb_border: [f32; 4],
    track_border_width: f32,
    thumb_border_width: f32,
}

/// Minimum cross-axis size for an `auto` scrollbar at spring≈0 so track/thumb stay hittable.
const SCROLLBAR_HIT_SLAB_CROSS: f32 = 8.0;
/// Thickness encoded on the track via `max_w` / `max_h` in `scrollbar.rs`; must match default
/// `ScrollbarBuilder::thickness` when ScrollArea does not override.
const SCROLLBAR_DEFAULT_CROSS_PX: f32 = 10.0;

/// Hoist `rui_portal` nodes, then refresh [`Element::contains_scroll_viewport`] flags.
///
/// `rebuild_ui` walks the **unhoisted** tree. After lift, the portal layer would otherwise
/// keep `contains_scroll_viewport = false`, so the scrollbar prepass never updates Dialog
/// chrome: tracks stay at auto-vis alpha 0 (hittable, invisible) until a later reconcile
/// such as opening a nested Select.
pub(crate) fn hoist_portal_subtrees_refreshing_scroll_flags(root: &mut Element) -> usize {
    let n = crate::layout::layout_tree::hoist_portal_subtrees(root);
    recompute_scroll_viewport_flags(root);
    n
}

pub(crate) fn recompute_scroll_viewport_flags(el: &mut Element) -> bool {
    let self_is = el
        .focus_id
        .as_deref()
        .is_some_and(|f| f.starts_with(SCROLL_VIEWPORT_FOCUS_PREFIX));
    let mut any_child = false;
    let mut vp_count = u32::from(self_is);
    for child in &mut el.children {
        if recompute_scroll_viewport_flags(child) {
            any_child = true;
        }
        vp_count = vp_count.saturating_add(child.scroll_viewport_subtree_count);
    }
    el.scroll_viewport_subtree_count = vp_count;
    el.contains_scroll_viewport = self_is || any_child;
    el.contains_scroll_viewport
}

fn normalize_anchor_template_id(id: &str) -> String {
    id.replace('\\', "/").to_ascii_lowercase()
}

/// Used by [`AppState::reconcile_subtree`] only. Must not use hot-reload fuzzy matching.
fn find_reactive_subtree_by_anchor_id_mut<'a>(
    el: &'a mut Element,
    want_normalized: &str,
) -> Option<&'a mut Element> {
    if let Some(tid) = el.template_id.as_deref() {
        if normalize_anchor_template_id(tid) == want_normalized {
            return Some(el);
        }
    }
    for child in &mut el.children {
        if let Some(hit) = find_reactive_subtree_by_anchor_id_mut(child, want_normalized) {
            return Some(hit);
        }
    }
    None
}

fn update_scrollbar_elements(
    el: &mut Element,
    base_style: &mut std::collections::HashMap<String, ScrollbarBaseStyle>,
    layout_tree: &mut crate::layout::layout_tree::LayoutTree,
    mode: ScrollbarUpdateMode,
    active_drag_rest: Option<&str>,
) {
    let inset_len_px = |v: &taffy::style::LengthPercentageAuto| match *v {
        taffy::style::LengthPercentageAuto::Length(px) if px.is_finite() => px.max(0.0),
        _ => 0.0,
    };
    fn subtree_contains_scroll_rest(el: &Element, target_rest: &str) -> bool {
        if let Some(fid) = el.focus_id.as_deref() {
            if fid
                .strip_prefix(SCROLL_VIEWPORT_FOCUS_PREFIX)
                .is_some_and(|rest| rest == target_rest)
            {
                return true;
            }
        }
        el.children
            .iter()
            .any(|child| subtree_contains_scroll_rest(child, target_rest))
    }

    #[cfg(debug_assertions)]
    {
        SCROLL_PREPASS_RECURSE_COUNT.fetch_add(1, Ordering::Relaxed);
        match mode {
            ScrollbarUpdateMode::ViewportsOnly => {
                SCROLL_PREPASS_RECURSE_VIEWPORTS_ONLY.fetch_add(1, Ordering::Relaxed);
            }
            ScrollbarUpdateMode::Full => {
                SCROLL_PREPASS_RECURSE_FULL.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    if mode == ScrollbarUpdateMode::ViewportsOnly {
        if !el.contains_scroll_viewport {
            return;
        }
        // Fast path: on a ScrollArea viewport, sync offset. Skip descending only when there are
        // no nested `scroll:` viewports — otherwise inner tracks stop receiving
        // `update_scrollbar_elements` every frame (parent scroll uses ViewportsOnly), and stale
        // `vw`/corner math makes nested bars look doubly reserved / too short.
        if let Some(fid) = el.focus_id.as_deref() {
            if let Some(rest) = fid.strip_prefix(SCROLL_VIEWPORT_FOCUS_PREFIX) {
                let sx = use_signal(&scroll_offset_x_signal_key(rest), || 0.0_f32).peek();
                let sy = use_signal(&scroll_offset_y_signal_key(rest), || 0.0_f32).peek();
                el.scroll_offset = [sx, sy];
                if el.scroll_viewport_subtree_count > 1 {
                    for child in &mut el.children {
                        update_scrollbar_elements(
                            child,
                            base_style,
                            layout_tree,
                            mode,
                            active_drag_rest,
                        );
                    }
                }
                return;
            }
        }
        for child in &mut el.children {
            update_scrollbar_elements(child, base_style, layout_tree, mode, active_drag_rest);
        }
        return;
    }

    // Drag optimization: when actively dragging one scrollbar thumb, keep Full-mode updates
    // for the target subtree. For unrelated subtrees we only sync viewport offsets.
    // NOTE: do not early-return on an ancestor of `target_rest`, otherwise nested target
    // scrollbar geometry freezes during drag and jumps on release.
    if let Some(target_rest) = active_drag_rest {
        if let Some(fid) = el.focus_id.as_deref() {
            if let Some(rest) = fid.strip_prefix(SCROLL_VIEWPORT_FOCUS_PREFIX) {
                if rest != target_rest && !subtree_contains_scroll_rest(el, target_rest) {
                    let sx = use_signal(&scroll_offset_x_signal_key(rest), || 0.0_f32).peek();
                    let sy = use_signal(&scroll_offset_y_signal_key(rest), || 0.0_f32).peek();
                    el.scroll_offset = [sx, sy];
                    return;
                }
            }
        }
    }

    // Update children first so nested scrollbars also work.
    // Full 模式剪枝：无 ScrollArea 子树且子节点本身也不是 scroll/scrollbar 焦点时，不再下降
    // （避免对纯静态 UI 大子树做 612 次无意义递归；轨道/拇指仍由「本节点」在下方逻辑处理。）
    for child in &mut el.children {
        let descend = child.scroll_viewport_subtree_count > 0
            || child.focus_id.as_deref().is_some_and(|f| {
                f.starts_with(SCROLLBAR_THUMB_FOCUS_PREFIX)
                    || f.starts_with(SCROLLBAR_TRACK_FOCUS_PREFIX)
                    || f.starts_with(SCROLL_VIEWPORT_FOCUS_PREFIX)
            });
        if descend {
            update_scrollbar_elements(child, base_style, layout_tree, mode, active_drag_rest);
        }
    }

    let Some(fid) = el.focus_id.as_deref() else {
        return;
    };

    let (rest, axis, is_thumb) =
        if let Some(prefix) = fid.strip_prefix(SCROLLBAR_THUMB_FOCUS_PREFIX) {
            let (rest, axis) = prefix.rsplit_once(':').unwrap_or((prefix, "y"));
            (rest.to_string(), axis, true)
        } else if let Some(prefix) = fid.strip_prefix(SCROLLBAR_TRACK_FOCUS_PREFIX) {
            let (rest, axis) = prefix.rsplit_once(':').unwrap_or((prefix, "y"));
            (rest.to_string(), axis, false)
        } else if let Some(rest) = fid.strip_prefix(SCROLL_VIEWPORT_FOCUS_PREFIX) {
            // High-performance path: sync viewport scroll offset directly in the render pass.
            // This bypasses the reactive rebuild for pure scroll offset changes.
            let sx = use_signal(&scroll_offset_x_signal_key(rest), || 0.0_f32).peek();
            let sy = use_signal(&scroll_offset_y_signal_key(rest), || 0.0_f32).peek();
            el.scroll_offset = [sx, sy];
            return;
        } else {
            return;
        };

    // Only update track; thumb will be updated via parent track traversal.
    if is_thumb {
        return;
    }

    let vw = use_signal(&scroll_viewport_w_signal_key(&rest), || 0.0_f32).peek();
    let vh = use_signal(&scroll_viewport_h_signal_key(&rest), || 0.0_f32).peek();
    let cw = use_signal(&scroll_content_w_signal_key(&rest), || 0.0_f32).peek();
    let ch = use_signal(&scroll_content_h_signal_key(&rest), || 0.0_f32).peek();
    let sx = use_signal(&scroll_offset_x_signal_key(&rest), || 0.0_f32).peek();
    let sy = use_signal(&scroll_offset_y_signal_key(&rest), || 0.0_f32).peek();

    let overflow_x = use_signal(&scroll_overflow_x_signal_key(&rest), || "auto".to_string()).peek();
    let overflow_y = use_signal(&scroll_overflow_y_signal_key(&rest), || "auto".to_string()).peek();
    let allow_x = !overflow_x.eq_ignore_ascii_case("hidden")
        && !overflow_x.eq_ignore_ascii_case("clip")
        && !overflow_x.eq_ignore_ascii_case("visible");
    let allow_y = !overflow_y.eq_ignore_ascii_case("hidden")
        && !overflow_y.eq_ignore_ascii_case("clip")
        && !overflow_y.eq_ignore_ascii_case("visible");
    // 双轴 inside 布局时，角落预留只应发生在“对向轴真正会占据角落”的场景：
    // - 对向轴可滚动（会显示可交互轨道），或
    // - 对向轴显式 `always`（即使不可滚动也要求保留轨道槽位）。
    // `auto && !can_scroll` 若仍预留，会让部分内层轨道看起来被多扣了一截。
    let vis_x_other = use_signal(&scrollbar_visibility_signal_key(&rest, "x"), || {
        "auto".to_string()
    })
    .peek();
    let vis_y_other = use_signal(&scrollbar_visibility_signal_key(&rest, "y"), || {
        "auto".to_string()
    })
    .peek();
    let vis_x_other_trim = vis_x_other.trim().to_ascii_lowercase();
    let vis_y_other_trim = vis_y_other.trim().to_ascii_lowercase();
    let show_track_x = allow_x && vis_x_other_trim != "never";
    let show_track_y = allow_y && vis_y_other_trim != "never";
    // Use a softer threshold to avoid near-boundary flicker around 1px.
    let can_scroll_x = allow_x && cw > vw + 0.5;
    let can_scroll_y = allow_y && ch > vh + 0.5;
    let reserve_for_x = show_track_x
        && match vis_x_other_trim.as_str() {
            "always" => true,
            "never" => false,
            _ => can_scroll_x,
        };
    let reserve_for_y = show_track_y
        && match vis_y_other_trim.as_str() {
            "always" => true,
            "never" => false,
            _ => can_scroll_y,
        };
    let axis_allows = if axis == "x" { allow_x } else { allow_y };
    let scrollable = if axis == "x" {
        can_scroll_x
    } else {
        can_scroll_y
    };

    // Visibility mode: only Auto uses spring-driven show/hide.
    let vis = use_signal(&scrollbar_visibility_signal_key(&rest, axis), || {
        "auto".to_string()
    })
    .peek();
    let vis = vis.trim().to_ascii_lowercase();
    let v = if vis == "never" {
        0.0
    } else if vis == "always" {
        1.0
    } else {
        crate::style::use_scrollbar_visibility_spring(
            &format!("scrollbar_spring_{rest}:{axis}"),
            0.0,
        )
        .peek()
        .clamp(0.0, 1.0)
    };

    // Recover base thickness from the track's max cross constraint only. Using `layout_bounds`
    // here caused cross-axis jitter whenever unrelated flex reflows nudged the resolved size.
    let base_thickness = match axis {
        "x" => match el.style.max_size.height {
            Dimension::Length(px) if px.is_finite() && px > 1.0 => px,
            _ => SCROLLBAR_DEFAULT_CROSS_PX,
        },
        _ => match el.style.max_size.width {
            Dimension::Length(px) if px.is_finite() && px > 1.0 => px,
            _ => SCROLLBAR_DEFAULT_CROSS_PX,
        },
    };

    // Collapse when:
    // - axis overflow mode disallows scrollbars, or
    // - explicitly `never`, or
    // - `auto` but not scrollable (no hidden hit-slab reservation for non-scrollable axis).
    if !axis_allows || vis == "never" || (vis != "always" && !scrollable) {
        el.style.size.width = Dimension::Length(0.0);
        el.style.size.height = Dimension::Length(0.0);
        el.style.border_width = 0.0;
        if let Some(thumb) = el.children.first_mut() {
            thumb.style.size.width = Dimension::Length(0.0);
            thumb.style.size.height = Dimension::Length(0.0);
            thumb.style.border_width = 0.0;
            layout_tree.update_taffy_node_style(thumb);
        }
        // Keep Taffy node in lockstep with collapsed track style.
        layout_tree.update_taffy_node_style(el);
        return;
    }

    // Dynamic inside-corner reservation:
    // reserve only when the opposite axis is actually scrollable and both are inside.
    let inset_x = use_signal(&scroll_scrollbar_inset_x_signal_key(&rest), || {
        "inside".to_string()
    })
    .peek();
    let inset_y = use_signal(&scroll_scrollbar_inset_y_signal_key(&rest), || {
        "inside".to_string()
    })
    .peek();
    let x_inside = !inset_x.eq_ignore_ascii_case("outside");
    let y_inside = !inset_y.eq_ignore_ascii_case("outside");
    let y_side = use_signal(&scroll_scrollbar_y_side_signal_key(&rest), || {
        "right".to_string()
    })
    .peek();
    let x_side = use_signal(&scroll_scrollbar_x_side_signal_key(&rest), || {
        "bottom".to_string()
    })
    .peek();
    let y_is_left = y_side.eq_ignore_ascii_case("left");
    let x_is_top = x_side.eq_ignore_ascii_case("top");
    // Corner reservation should only remove the opposite scrollbar lane thickness.
    let corner_reserve = base_thickness;

    let mut reserve_left = 0.0_f32;
    let mut reserve_right = 0.0_f32;
    let mut reserve_top = 0.0_f32;
    let mut reserve_bottom = 0.0_f32;
    if axis == "x" && x_inside && y_inside && reserve_for_y {
        if y_is_left {
            reserve_left = corner_reserve;
        } else {
            reserve_right = corner_reserve;
        }
    }
    if axis == "y" && x_inside && y_inside && reserve_for_x {
        if x_is_top {
            reserve_top = corner_reserve;
        } else {
            reserve_bottom = corner_reserve;
        }
    }

    // For section4 consistency, track length should only differ by opposite-lane reservation.
    // Subtracting edge pads here causes inside lanes to be 4px shorter than equivalent outside lanes.
    let edge_pad_main = 0.0_f32;

    // Keep constant cross-size; show/hide is alpha-only (`v` = scrollbar spring current).
    // Do not clamp alpha to 0 while 0 < v < 0.01: during spring fade-in `v` stays tiny for several
    // frames — forcing zero here made `auto` scrollbars look like hover never worked. Hit testing
    // uses `layout_bounds`, not multiplied alpha.
    let inset = 1.0_f32;
    let cross = SCROLLBAR_HIT_SLAB_CROSS.max(base_thickness).max(2.0);
    let thumb_cross = (cross - inset * 2.0).max(1.0);

    let main_available = match axis {
        "x" => {
            let inset_l = inset_len_px(&el.style.inset.left);
            let inset_r = inset_len_px(&el.style.inset.right);
            (vw - inset_l - inset_r).max(1.0)
        }
        _ => {
            let inset_t = inset_len_px(&el.style.inset.top);
            let inset_b = inset_len_px(&el.style.inset.bottom);
            (vh - inset_t - inset_b).max(1.0)
        }
    };

    match axis {
        "x" => {
            el.style.size.width = Dimension::Length(
                (main_available - reserve_left - reserve_right - edge_pad_main).max(1.0),
            );
            el.style.size.height = Dimension::Length(cross);
        }
        _ => {
            el.style.size.width = Dimension::Length(cross);
            el.style.size.height = Dimension::Length(
                (main_available - reserve_top - reserve_bottom - edge_pad_main).max(1.0),
            );
        }
    }
    // The scrollbar builder sets `min_w / min_h` at build time to the initial viewport
    // dimensions. These stale constraints override our runtime `size` — Taffy computes
    // `max(min_size, size)`. Clear them so the explicit size above is authoritative.
    el.style.min_size.width = Dimension::Auto;
    el.style.min_size.height = Dimension::Auto;

    // Update thumb geometry (assumes first child is thumb).
    let Some(thumb) = el.children.first_mut() else {
        return;
    };

    // Cache base style once per scroll-area + axis (we will apply alpha every frame).
    let style_key = format!("{rest}:{axis}");
    if let Entry::Vacant(e) = base_style.entry(style_key.clone()) {
        let mut track_bg = el.style.bg_color;
        let mut thumb_bg = thumb.style.bg_color;
        let mut track_border = el.style.border_color;
        let mut thumb_border = thumb.style.border_color;
        // `auto` may be initially built fully transparent (to avoid StructureChanged-frame flash).
        // Base style must keep normalized alpha, otherwise later `* visible_alpha` stays 0 forever.
        if vis == "auto" {
            if track_bg[3] <= f32::EPSILON {
                track_bg[3] = 1.0;
            }
            if thumb_bg[3] <= f32::EPSILON {
                thumb_bg[3] = 1.0;
            }
            if track_border[3] <= f32::EPSILON {
                track_border[3] = 1.0;
            }
            if thumb_border[3] <= f32::EPSILON {
                thumb_border[3] = 1.0;
            }
        }
        e.insert(ScrollbarBaseStyle {
            track_bg,
            thumb_bg,
            track_border,
            thumb_border,
            track_border_width: el.style.border_width,
            thumb_border_width: thumb.style.border_width,
        });
    }
    if let Some(bs) = base_style.get(&style_key).copied() {
        let mut tc = bs.track_bg;
        let mut th = bs.thumb_bg;
        let mut tbc = bs.track_border;
        let mut thbc = bs.thumb_border;
        let visible_alpha = v.clamp(0.0, 1.0);
        tc[3] *= visible_alpha;
        th[3] *= visible_alpha;
        tbc[3] *= visible_alpha;
        thbc[3] *= visible_alpha;
        el.style.bg_color = tc;
        thumb.style.bg_color = th;
        el.style.border_color = tbc;
        thumb.style.border_color = thbc;
        // 边框宽不参与 alpha：否则每帧改变盒模型，无法在无 Taffy compute 的 overlay-only 帧保持几何稳定。
        el.style.border_width = bs.track_border_width;
        thumb.style.border_width = bs.thumb_border_width;
    }

    {
        match axis {
            "x" => {
                let track =
                    (main_available - reserve_left - reserve_right - edge_pad_main).max(1.0);
                let track_inner = (track - inset * 2.0).max(1.0);
                let (tw, ml) = if scrollable {
                    let ratio = (vw / cw.max(vw)).clamp(0.0, 1.0);
                    let min_thumb = scrollbar_min_thumb_along_track(track_inner);
                    let tw = (track_inner * ratio).clamp(min_thumb, track_inner.max(min_thumb));
                    let max_offset = (cw - vw).max(0.0);
                    let t = if max_offset > 1e-4 {
                        (sx / max_offset).clamp(0.0, 1.0)
                    } else {
                        0.0
                    };
                    let ml = (track_inner - tw).max(0.0) * t;
                    (tw, ml)
                } else {
                    (track_inner, 0.0)
                };
                thumb.style.size.width = Dimension::Length(tw);
                thumb.style.size.height = Dimension::Length(thumb_cross);
                thumb.style.margin.left = taffy::style::LengthPercentageAuto::Length(ml);
                thumb.style.margin.top = taffy::style::LengthPercentageAuto::Length(0.0);
            }
            _ => {
                let track =
                    (main_available - reserve_top - reserve_bottom - edge_pad_main).max(1.0);
                let track_inner = (track - inset * 2.0).max(1.0);
                let (th, mt) = if scrollable {
                    let ratio = (vh / ch.max(vh)).clamp(0.0, 1.0);
                    let min_thumb = scrollbar_min_thumb_along_track(track_inner);
                    let th = (track_inner * ratio).clamp(min_thumb, track_inner.max(min_thumb));
                    let max_offset = (ch - vh).max(0.0);
                    let t = if max_offset > 1e-4 {
                        (sy / max_offset).clamp(0.0, 1.0)
                    } else {
                        0.0
                    };
                    let mt = (track_inner - th).max(0.0) * t;
                    (th, mt)
                } else {
                    (track_inner, 0.0)
                };
                thumb.style.size.width = Dimension::Length(thumb_cross);
                thumb.style.size.height = Dimension::Length(th);
                thumb.style.margin.top = taffy::style::LengthPercentageAuto::Length(mt);
                thumb.style.margin.left = taffy::style::LengthPercentageAuto::Length(0.0);
            }
        }
        layout_tree.update_taffy_node_style(thumb);
    }

    // Sync track style after all geometry mutations to avoid one-frame stale layout,
    // which is especially visible in nested smooth-scroll scenarios.
    layout_tree.update_taffy_node_style(el);
}

#[cfg(debug_assertions)]
mod hot_reload_inject {
    use std::path::{Path, PathBuf};

    use crate::core::tree::Element;

    #[allow(dead_code)]
    pub(super) fn inject_text_literals_by_id(
        root: &mut Element,
        template_id: &str,
        text_literals: &[String],
    ) -> bool {
        if text_literals.is_empty() {
            return false;
        }
        if let Some(target) = find_by_template_id_mut(root, template_id) {
            let total = count_text_nodes(target);
            if total != text_literals.len() {
                // Prevent cross-node corruption when extractor/node order differs.
                return false;
            }
            for (idx, text) in text_literals.iter().enumerate() {
                let _ = replace_text_at_index(target, idx, text);
            }
            return true;
        }
        false
    }

    pub(super) fn inject_style_attrs_by_id(
        root: &mut Element,
        template_id: &str,
        attrs: &[crate::app::hot_reload::StyleAttr],
    ) -> bool {
        if attrs.is_empty() {
            return false;
        }
        let Some(template_root) = find_by_template_id_and_index_mut(root, template_id, usize::MAX)
        else {
            return false;
        };
        let mut changed = false;

        // First, group attributes by element index because we don't want to re-traverse for every single attribute
        let mut by_index: std::collections::HashMap<
            usize,
            Vec<&crate::app::hot_reload::StyleAttr>,
        > = std::collections::HashMap::new();
        for attr in attrs {
            by_index.entry(attr.element_index).or_default().push(attr);
        }

        for (idx, element_attrs) in by_index {
            let mut current_idx = 0usize;
            if let Some(target) = find_element_at_index_inner(template_root, idx, &mut current_idx)
            {
                for attr in element_attrs {
                    let v = attr.value.as_str();
                    match attr.name.as_str() {
                        "bg" => {
                            target.style.bg_color = crate::core::tree::parse_color(v);
                            changed = true;
                        }
                        "color" => {
                            target.text_style.color = crate::core::tree::parse_text_color(v);
                            changed = true;
                        }
                        "text_size" | "font_size" => {
                            if let Some(px) = parse_length_px(v) {
                                target.text_style.size = px;
                                changed = true;
                            }
                        }
                        "font_weight" | "text_weight" => {
                            if let Ok(w) = v.parse::<u16>() {
                                target.text_style.weight = w.clamp(100, 900);
                                changed = true;
                            }
                        }
                        "text_decoration" | "text-decoration" => {
                            let raw = v.to_ascii_lowercase();
                            if raw.contains("none") {
                                target.text_style.underline = false;
                                target.text_style.overline = false;
                                target.text_style.line_through = false;
                            } else {
                                let has_line_token = raw.contains("underline")
                                    || raw.contains("overline")
                                    || raw.contains("line-through")
                                    || raw.contains("linethrough")
                                    || raw.contains("strikethrough");
                                if has_line_token {
                                    target.text_style.underline = raw.contains("underline");
                                    target.text_style.overline = raw.contains("overline");
                                    target.text_style.line_through = raw.contains("line-through")
                                        || raw.contains("linethrough")
                                        || raw.contains("strikethrough");
                                }
                                target.text_style.decoration_style = if raw.contains("double") {
                                    crate::style::TextDecorationStyle::Double
                                } else if raw.contains("dotted") {
                                    crate::style::TextDecorationStyle::Dotted
                                } else if raw.contains("dashed") {
                                    crate::style::TextDecorationStyle::Dashed
                                } else if raw.contains("wavy") {
                                    crate::style::TextDecorationStyle::Wavy
                                } else {
                                    crate::style::TextDecorationStyle::Solid
                                };
                            }
                            changed = true;
                        }
                        "text_decoration_line" | "textDecorationLine" | "text-decoration-line" => {
                            let raw = v.to_ascii_lowercase();
                            if raw.contains("none") {
                                target.text_style.underline = false;
                                target.text_style.overline = false;
                                target.text_style.line_through = false;
                            } else {
                                target.text_style.underline = raw.contains("underline");
                                target.text_style.overline = raw.contains("overline");
                                target.text_style.line_through = raw.contains("line-through")
                                    || raw.contains("linethrough")
                                    || raw.contains("strikethrough");
                            }
                            changed = true;
                        }
                        "text_decoration_style"
                        | "textDecorationStyle"
                        | "text-decoration-style" => {
                            let s = v.to_ascii_lowercase();
                            target.text_style.decoration_style = match s.as_str() {
                                "double" => crate::style::TextDecorationStyle::Double,
                                "dotted" => crate::style::TextDecorationStyle::Dotted,
                                "dashed" => crate::style::TextDecorationStyle::Dashed,
                                "wavy" => crate::style::TextDecorationStyle::Wavy,
                                _ => crate::style::TextDecorationStyle::Solid,
                            };
                            changed = true;
                        }
                        "text_decoration_color"
                        | "textDecorationColor"
                        | "text-decoration-color" => {
                            target.text_style.decoration_color =
                                Some(crate::core::tree::parse_text_color(v));
                            changed = true;
                        }
                        "text_decoration_thickness"
                        | "textDecorationThickness"
                        | "text-decoration-thickness" => {
                            if let Some(px) = parse_length_px(v) {
                                target.text_style.decoration_thickness = px.max(0.0);
                                changed = true;
                            }
                        }
                        "rounded" | "radius" => {
                            if let Some(px) = parse_length_px(v) {
                                target.style.radii = [px, px, px, px];
                                changed = true;
                            }
                        }
                        "p" | "padding" => {
                            if let Some(px) = parse_length_px(v) {
                                let lp = taffy::style::LengthPercentage::Length(px);
                                target.style.padding = taffy::prelude::Rect {
                                    left: lp,
                                    right: lp,
                                    top: lp,
                                    bottom: lp,
                                };
                                changed = true;
                            }
                        }
                        "m" | "margin" => {
                            if let Some(px) = parse_length_px(v) {
                                let lp = taffy::style::LengthPercentageAuto::Length(px);
                                target.style.margin = taffy::prelude::Rect {
                                    left: lp,
                                    right: lp,
                                    top: lp,
                                    bottom: lp,
                                };
                                changed = true;
                            }
                        }
                        "w" | "width" => {
                            if let Some(d) = parse_dimension(v) {
                                target.style.size.width = d;
                                changed = true;
                            }
                        }
                        "h" | "height" => {
                            if let Some(d) = parse_dimension(v) {
                                target.style.size.height = d;
                                changed = true;
                            }
                        }
                        "gap" => {
                            if let Some(px) = parse_length_px(v) {
                                let lp = taffy::style::LengthPercentage::Length(px);
                                target.style.gap = taffy::geometry::Size {
                                    width: lp,
                                    height: lp,
                                };
                                changed = true;
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        changed
    }

    fn find_element_at_index_inner<'a>(
        el: &'a mut Element,
        target_idx: usize,
        current_idx: &mut usize,
    ) -> Option<&'a mut Element> {
        let self_idx = *current_idx;
        if self_idx == target_idx {
            return Some(el);
        }
        *current_idx += 1;

        for child in &mut el.children {
            // Stop at nested template boundaries
            if child.template_id.is_some() {
                continue;
            }
            if let Some(found) = find_element_at_index_inner(child, target_idx, current_idx) {
                return Some(found);
            }
        }
        None
    }

    fn parse_length_px(raw: &str) -> Option<f32> {
        let s = raw.trim();
        if let Some(v) = s.strip_suffix("rem") {
            let n = v.trim().parse::<f32>().ok()?;
            return Some(n * crate::core::tree::CSS_ROOT_FONT_SIZE_PX);
        }
        if let Some(v) = s.strip_suffix("px") {
            return v.trim().parse::<f32>().ok();
        }
        s.parse::<f32>().ok()
    }

    fn parse_dimension(raw: &str) -> Option<taffy::style::Dimension> {
        let s = raw.trim();
        if s.eq_ignore_ascii_case("auto") {
            return Some(taffy::style::Dimension::Auto);
        }
        if let Some(v) = s.strip_suffix('%') {
            return v
                .trim()
                .parse::<f32>()
                .ok()
                .map(|n| taffy::style::Dimension::Percent(n / 100.0));
        }
        parse_length_px(s).map(taffy::style::Dimension::Length)
    }

    pub(super) fn resolve_dev_asset_path(p: &str) -> Option<PathBuf> {
        let path = Path::new(p);
        if path.is_absolute() && path.is_file() {
            return Some(path.to_path_buf());
        }
        let cwd = std::env::current_dir().ok()?;
        let joined = cwd.join(p);
        if joined.is_file() {
            return Some(joined);
        }
        if let Ok(root) = std::env::var("RUI_WORKSPACE_ROOT") {
            let j = Path::new(root.trim()).join(p);
            if j.is_file() {
                return Some(j);
            }
        }
        None
    }

    pub(super) fn find_by_template_id_mut<'a>(
        el: &'a mut Element,
        id: &str,
    ) -> Option<&'a mut Element> {
        if let Some(current_id) = el.template_id.as_deref() {
            if crate::app::hot_reload::template_id_matches(id, current_id) {
                return Some(el);
            }
        }
        for child in &mut el.children {
            if let Some(found) = find_by_template_id_mut(child, id) {
                return Some(found);
            }
        }
        None
    }

    pub(super) fn find_by_template_id_and_index_mut<'a>(
        el: &'a mut Element,
        id: &str,
        index: usize,
    ) -> Option<&'a mut Element> {
        if let Some(current_id) = el.template_id.as_deref() {
            if crate::app::hot_reload::template_id_matches(id, current_id) {
                return Some(el);
            }
            // If line doesn't match but file AND index matches (e.g. line shift)
            if matches_file_and_index(id, current_id, index) {
                return Some(el);
            }
        }
        for child in &mut el.children {
            if let Some(found) = find_by_template_id_and_index_mut(child, id, index) {
                return Some(found);
            }
        }
        None
    }

    fn matches_file_and_index(incoming_id: &str, runtime_id: &str, incoming_index: usize) -> bool {
        // If incoming doesn't have index in its ID string yet (older CLI compatibility)
        // or if we want to be extra safe with the dedicated index field.
        let (ip, il, ii) = split_id_triple(incoming_id);
        let (rp, rl, ri) = split_id_triple(runtime_id);

        if !path_component_match_fuzzy(&ip, &rp) {
            return false;
        }

        // Match by index if available in both, otherwise fall back to fuzzy line matching
        if ii != usize::MAX && ri != usize::MAX {
            return ii == ri;
        }

        // If only incoming has index (from the new CLI field)
        if incoming_index != usize::MAX && ri != usize::MAX {
            return incoming_index == ri;
        }

        // Last resort: if file matches and line is very close (e.g. within 10 lines)
        if il != 0 && rl != 0 {
            let diff = (il as isize - rl as isize).abs();
            if diff < 20 {
                return true;
            }
        }

        false
    }

    fn split_id_triple(id: &str) -> (String, usize, usize) {
        let parts: Vec<&str> = id.split(':').collect();
        if parts.len() >= 3 {
            let p = parts[..parts.len() - 2].join(":");
            let l = parts[parts.len() - 2].parse().unwrap_or(0);
            let i = parts[parts.len() - 1].parse().unwrap_or(usize::MAX);
            (p, l, i)
        } else if parts.len() == 2 {
            (
                parts[0].to_string(),
                parts[1].parse().unwrap_or(0),
                usize::MAX,
            )
        } else {
            (id.to_string(), 0, usize::MAX)
        }
    }

    fn path_component_match_fuzzy(a: &str, b: &str) -> bool {
        let a = a.replace('\\', "/").to_ascii_lowercase();
        let b = b.replace('\\', "/").to_ascii_lowercase();
        if a == b {
            return true;
        }
        a.ends_with(&b) || b.ends_with(&a)
    }

    pub(super) fn inject_text_replacements_by_id(
        root: &mut Element,
        template_id: &str,
        replacements: &[crate::app::hot_reload::TextReplacement],
    ) -> bool {
        if replacements.is_empty() {
            return false;
        }
        let Some(target) = find_by_template_id_and_index_mut(root, template_id, usize::MAX) else {
            return false;
        };
        let mut changed = false;
        for rep in replacements {
            // Prefer exact text-based replacement when unique. This is more stable
            // than index mapping when template extraction and runtime text-node
            // order diverge (e.g. expressions/components).
            if count_text_occurrences(target, &rep.from) == 1
                && replace_first_text(target, &rep.from, &rep.to)
            {
                changed = true;
                continue;
            }
            // Fall back to index only when unique text matching is unavailable.
            if replace_text_at_index_checked(target, rep.index, &rep.from, &rep.to) {
                changed = true;
            }
        }
        changed
    }

    fn replace_first_text(el: &mut Element, from: &str, to: &str) -> bool {
        if let Some(text) = el.text.as_mut() {
            if text == from {
                *text = to.to_string();
                return true;
            }
        }
        for child in &mut el.children {
            // Stop at nested template boundaries
            if child.template_id.is_some() {
                continue;
            }
            if replace_first_text(child, from, to) {
                return true;
            }
        }
        false
    }

    fn replace_text_at_index(el: &mut Element, target_index: usize, to: &str) -> bool {
        let mut idx = 0usize;
        replace_text_at_index_inner(el, target_index, to, &mut idx)
    }

    fn replace_text_at_index_checked(
        el: &mut Element,
        target_index: usize,
        from: &str,
        to: &str,
    ) -> bool {
        let mut idx = 0usize;
        replace_text_at_index_checked_inner(el, target_index, from, to, &mut idx)
    }

    fn replace_text_at_index_inner(
        el: &mut Element,
        target_index: usize,
        to: &str,
        idx: &mut usize,
    ) -> bool {
        if let Some(text) = el.text.as_mut() {
            if *idx == target_index {
                *text = to.to_string();
                return true;
            }
            *idx += 1;
        }
        for child in &mut el.children {
            // Stop at nested template boundaries
            if child.template_id.is_some() {
                continue;
            }
            if replace_text_at_index_inner(child, target_index, to, idx) {
                return true;
            }
        }
        false
    }

    fn replace_text_at_index_checked_inner(
        el: &mut Element,
        target_index: usize,
        from: &str,
        to: &str,
        idx: &mut usize,
    ) -> bool {
        if let Some(text) = el.text.as_mut() {
            if *idx == target_index {
                if text == from {
                    *text = to.to_string();
                    return true;
                }
                return false;
            }
            *idx += 1;
        }
        for child in &mut el.children {
            // Stop at nested template boundaries
            if child.template_id.is_some() {
                continue;
            }
            if replace_text_at_index_checked_inner(child, target_index, from, to, idx) {
                return true;
            }
        }
        false
    }

    fn count_text_occurrences(el: &Element, needle: &str) -> usize {
        let mut n = 0usize;
        if el.text.as_deref() == Some(needle) {
            n += 1;
        }
        for child in &el.children {
            // Stop at nested template boundaries
            if child.template_id.is_some() {
                continue;
            }
            n += count_text_occurrences(child, needle);
        }
        n
    }

    pub(super) fn upsert_overrides(
        store: &mut Vec<crate::app::hot_reload::TemplateFragment>,
        incoming: &[crate::app::hot_reload::TemplateFragment],
    ) {
        for tpl in incoming {
            // Find existing override by exact ID OR by (file + index)
            let (file_name, _line, _idx) = split_id_triple(&tpl.id);
            if let Some(existing) = store.iter_mut().find(|v| {
                if v.id == tpl.id {
                    return true;
                }
                let (ef, _el, _ei) = split_id_triple(&v.id);
                path_component_match_fuzzy(&ef, &file_name) && v.index == tpl.index
            }) {
                *existing = tpl.clone();
            } else {
                store.push(tpl.clone());
            }
        }
    }

    pub(super) fn apply_templates_to_tree(
        root: &mut Element,
        templates: &[crate::app::hot_reload::TemplateFragment],
    ) {
        for tpl in templates {
            if let Some(target) = find_by_template_id_and_index_mut(root, &tpl.id, tpl.index) {
                if target
                    .template_hash
                    .as_deref()
                    .is_some_and(|h| h == tpl.hash.as_str())
                {
                    // 与 CLI 同 hash：树来自刚编译的 ui!，勿再按索引覆盖（易与 `{expr}` 槽位错位）。
                    continue;
                }
            }
            let _ = inject_style_attrs_by_id(root, &tpl.id, &tpl.style_attrs);
            // 从 ui!() 回调得到的新树是编译期字符串；热更的增量 replacement 的 `from` 是「上一版热更」
            // 的 literal，与当前树中的字往往对不上，单靠 replacement 会在 resize/信号重绘时失效。
            // 持久化里带有完整 `text_literals` 快照时，按 DFS 一次性写回（与 inject_hot_reload_partial 一致）。
            let mut text_ok = false;
            if !tpl.text_literals.is_empty() {
                text_ok = inject_text_literals_by_id(root, &tpl.id, &tpl.text_literals);
            }
            if !text_ok {
                let _ = inject_text_replacements_by_id(root, &tpl.id, &tpl.text_replacements);
            }
        }
    }

    pub(super) fn derive_replacements_from_previous(
        previous: &[crate::app::hot_reload::TemplateFragment],
        current: &crate::app::hot_reload::TemplateFragment,
    ) -> Vec<crate::app::hot_reload::TextReplacement> {
        let Some(prev) = previous.iter().find(|v| v.id == current.id) else {
            return Vec::new();
        };
        let n = prev.text_literals.len().min(current.text_literals.len());
        let mut out = Vec::new();
        for i in 0..n {
            if prev.text_literals[i] != current.text_literals[i] {
                out.push(crate::app::hot_reload::TextReplacement {
                    index: i,
                    from: prev.text_literals[i].clone(),
                    to: current.text_literals[i].clone(),
                });
            }
        }
        out
    }

    #[allow(dead_code)]
    fn apply_text_literals_dfs(el: &mut Element, literals: &[String], cursor: &mut usize) {
        if let Some(text) = el.text.as_mut() {
            if *cursor < literals.len() {
                *text = literals[*cursor].clone();
                *cursor += 1;
            }
        }
        for child in &mut el.children {
            // Stop at nested template boundaries
            if child.template_id.is_some() {
                continue;
            }
            apply_text_literals_dfs(child, literals, cursor);
        }
    }

    fn count_text_nodes(el: &Element) -> usize {
        let mut n = if el.text.is_some() { 1 } else { 0 };
        for child in &el.children {
            // Stop at nested template boundaries
            if child.template_id.is_some() {
                continue;
            }
            n += count_text_nodes(child);
        }
        n
    }
} // mod hot_reload_inject

#[cfg(test)]
mod template_path_tests {
    use super::AppState;
    use crate::core::tree::Element;

    fn tree() -> Element {
        Element::new().children(vec![
            Element::new().template_id("a"),
            Element::new().children(vec![Element::new().template_id("b")]),
        ])
    }

    #[test]
    fn finds_child_index_path() {
        let root = tree();
        let mut path = Vec::new();
        assert!(AppState::find_template_path(&root, "b", &mut path));
        assert_eq!(path, vec![1, 0]);

        path.clear();
        assert!(!AppState::find_template_path(&root, "missing", &mut path));
        assert!(path.is_empty());
    }

    #[test]
    fn path_validation_rejects_stale_and_out_of_range() {
        let root = tree();
        assert!(AppState::template_path_matches(&root, &[1, 0], "b"));
        // Same shape, wrong node: the cached path must not be trusted blindly.
        assert!(!AppState::template_path_matches(&root, &[1, 0], "a"));
        // Shrunk subtree.
        assert!(!AppState::template_path_matches(&root, &[9], "a"));
    }
}

#[cfg(test)]
mod surface_resize_tests {
    use super::AppState;
    use winit::dpi::PhysicalSize;

    #[test]
    fn shrink_prefers_resized_event_not_stale_large_inner() {
        let inner = PhysicalSize::new(800, 947);
        let resized = PhysicalSize::new(800, 525);
        let target = AppState::resolve_surface_size(inner, Some(resized));
        assert_eq!(
            (target.width, target.height),
            (800, 525),
            "shrink must adopt Resized height; max(inner,resized) left layout tall and disabled scroll"
        );
    }

    #[test]
    fn grow_prefers_resized_event_when_inner_lags() {
        let inner = PhysicalSize::new(800, 525);
        let resized = PhysicalSize::new(800, 947);
        let target = AppState::resolve_surface_size(inner, Some(resized));
        assert_eq!((target.width, target.height), (800, 947));
    }

    #[test]
    fn without_resized_event_uses_inner() {
        let inner = PhysicalSize::new(640, 480);
        let target = AppState::resolve_surface_size(inner, None);
        assert_eq!((target.width, target.height), (640, 480));
    }
}
