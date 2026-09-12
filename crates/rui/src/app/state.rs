use std::borrow::Cow;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use wgpu::util::DeviceExt;
use winit::dpi::PhysicalSize;
use winit::window::Window;

use crate::core::tree::{Element, LayoutProfileMs, LayoutTree};
use crate::renderer::{
    cpu_text::CpuTextProfiler, prepare_text_frame, scale_clip_polys_in_place, scale_instances,
    scale_instances_in_place, scale_textured_draws, BackdropBlurPipeline, InstanceInput,
    PresentRetainPipeline, RuiPipeline, RuiTextRenderer, ScaledTexturedDraw, TexturedInstanceInput,
    TexturedQuadPipeline,
};
use crate::{
    components::{
        scrollbar::scrollbar_visibility_signal_key,
        scrollbar::{
            apply_scrollbar_visible_alpha, normalize_scrollbar_base_for_auto,
            scrollbar_min_thumb_along_track, ATTR_SCROLLBAR_TRACK_FADES,
            SCROLLBAR_THUMB_FOCUS_PREFIX, SCROLLBAR_TRACK_FOCUS_PREFIX,
        },
    },
    core::reactivity::use_signal,
    core::scroll_viewport::{
        scroll_content_h_signal_key, scroll_content_w_signal_key, scroll_offset_x_signal_key,
        scroll_offset_y_signal_key, scroll_overflow_x_signal_key, scroll_overflow_y_signal_key,
        scroll_scrollbar_inset_x_signal_key, scroll_scrollbar_inset_y_signal_key,
        scroll_viewport_h_signal_key, scroll_viewport_w_signal_key, SCROLL_VIEWPORT_FOCUS_PREFIX,
    },
};
use std::collections::hash_map::Entry;
#[cfg(debug_assertions)]
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::OnceLock;
use std::time::Instant;
use taffy::prelude::Dimension;

fn render_profile_log_enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| cfg!(debug_assertions) || std::env::var_os("RUI_RENDER_PROFILE").is_some())
}

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
    pub(super) adapter: wgpu::Adapter,
    pub(super) device: Arc<wgpu::Device>,
    pub(super) queue: Arc<wgpu::Queue>,
    pub(super) surface: wgpu::Surface<'static>,
    pub(super) config: wgpu::SurfaceConfiguration,
    pub(super) size: winit::dpi::PhysicalSize<u32>,
    pub(super) pipeline: RuiPipeline,
    textured_pipeline: TexturedQuadPipeline,
    #[cfg(feature = "scene3d")]
    scene3d: Option<crate::renderer::scene3d::Scene3dGpu>,
    backdrop_blur: BackdropBlurPipeline,
    effect_pass: crate::renderer::EffectPassGpu,
    css_filter_pass: crate::renderer::CssFilterPassGpu,
    /// Offscreen color target so PartialLoad+scissor does not hit a non-retaining swapchain.
    present_retain: PresentRetainPipeline,
    image_textures: crate::renderer::TexturePool,
    textured_instance_buffer: wgpu::Buffer,
    textured_instance_capacity: usize,
    pub(super) instance_buffer: wgpu::Buffer,
    instance_buffer_capacity: usize,
    pub(super) layout_tree: LayoutTree,
    pub(super) text_renderer: RuiTextRenderer,
    /// Dedicated glyphon pipeline for subtree-capture text (xform / fx). Must not share
    /// prepare state with [`Self::text_renderer`] — capture runs before page prepare and
    /// would otherwise clobber atlas/viewport when `text_cache_valid` skips the main prepare
    /// (gallery 3b: flipped/clipped glyphs + spin stutter).
    capture_text_renderer: RuiTextRenderer,
    /// Separate glyphon pipeline for portal overlay glyphs so page + overlay text can be
    /// prepared together and drawn in one Clear pass (page → overlay rects → overlay text)
    /// without a GLES-fragile `LoadOp::Load` second pass that erased sidebar chrome.
    ///
    /// Band 0 of stacked portals. Additional bands use [`Self::overlay_band_text_renderers`]
    /// so each portal child can paint `rects → text` before the next child (correct occlusion).
    overlay_text_renderer: RuiTextRenderer,
    /// Extra glyphon pipelines for portal paint bands beyond the first (share FontSystem).
    overlay_band_text_renderers: Vec<RuiTextRenderer>,
    /// Extra glyphon pipelines for in-page stacking bands beyond the first (CarouselStack).
    /// Band 0 reuses [`Self::text_renderer`]; further bands share its FontSystem.
    page_stack_band_text_renderers: Vec<RuiTextRenderer>,
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
    /// This prevents the CLI from confusing the layout with secondary injection when it continuously sends "same content, no diff" patches to the same file.
    #[cfg(debug_assertions)]
    last_applied_template_hash: HashMap<String, String>,
    pub(super) last_render_scroll_metrics: Vec<(String, [f32; 4], [f32; 2], [f32; 2])>,
    pub(super) last_hover_cursor: Option<crate::core::tree::CursorValue>,
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
    render_prof_last: Instant,
    /// End of the previous `render()`; slow-frame logs use this as `idle=` (ms since last paint).
    last_render_at: Instant,
    render_prof_frames: u32,
    render_prof_pre_ms: f32,
    render_prof_layout_ms: f32,
    /// `RenderResult.prof`: `build_and_layout` build/prun Taffy (complementary read with total `layout`).
    render_prof_layout_rebuild_ms: f32,
    render_prof_layout_setup_ms: f32,
    render_prof_layout_taffy_ms: f32,
    render_prof_layout_collect_ms: f32,
    render_prof_draw_ms: f32,
    render_prof_acquire_ms: f32,
    render_prof_submit_ms: f32,
    render_prof_present_ms: f32,
    render_prof_upload_ms: f32,
    render_prof_text_ms: f32,
    render_prof_encode_ms: f32,
    last_text_nodes_cache: Vec<crate::core::tree::TextLayoutInfo>,
    last_text_ui_scale: f32,
    last_text_surface_size: [u32; 2],
    last_underline_instances: Vec<crate::renderer::InstanceInput>,
    /// Flattened underlines last submitted to GPU (survives page-stack `last_underline_instances.clear()`).
    last_presented_underlines: Vec<crate::renderer::InstanceInput>,
    last_caret_instances: Vec<crate::renderer::InstanceInput>,
    last_caret_blink_phase: u128,
    cpu_text_profiler: CpuTextProfiler,
    shadow_tex_cache: std::collections::HashMap<u64, super::image_registry::GpuImageTexture>,
    shadow_rect_cache: std::collections::HashMap<u64, [f32; 4]>,
    last_shadow_tex_draws: Vec<crate::renderer::prepare_text::ShadowTexDraw>,
    /// Accumulated dirty rect in logical px for **scroll viewport layer** only.
    pub(super) damage_rect: Option<[f32; 4]>,
    /// Paint-only dirty **nodes** (never a cross-node union AABB).
    damage_nodes: Vec<super::damage::PaintDirty>,
    pub(super) force_full_redraw: bool,
    /// Portal layer membership changed (popover / tooltip / menu hoist or close).
    /// Skip window FullClear; scissor to overlay bounds after layout.
    pub(super) portal_membership_dirty: bool,
    /// Non-portal island reconcile changed page geometry this frame.
    page_geom_dirty: bool,
    /// Last presented hoisted overlay boxes (logical px), one per overlay node.
    last_overlay_boxes: Vec<[f32; 4]>,
    /// Union of overlay boxes while the popover has been open (expand/exit
    /// keyframes shrink the last box; close must restore this max extent).
    overlay_occupied: Option<[f32; 4]>,
    /// Last presented union of hoisted overlay boxes (logical px). Close/move uses this as damage.
    last_portal_layer_damage: Option<[f32; 4]>,
    /// After the first successful present, partial `LoadOp::Load` is allowed.
    has_presented_frame: bool,
    /// Fingerprint of the last submitted GPU frame (instances + text chrome + caret).
    /// Used to skip acquire/submit/present when a redraw was requested but nothing visual changed.
    last_gpu_frame_fp: u64,
    /// Hash of all `scroll:` viewport offsets after prepass.
    last_scroll_offset_fp: u64,
    last_scroll_offsets: HashMap<String, [u32; 2]>,
    /// Scroll moved: PartialLoad the viewport layer (Clear scissor, then redraw).
    scroll_layer_clear: bool,
    /// Unpadded union of viewports whose offset changed this frame (text clip).
    scroll_text_clip: Option<[f32; 4]>,
    /// Overlay closed or shrank: Clear the leftover scissor then redraw page
    /// (retain still holds composited overlay pixels).
    overlay_restore_clear: bool,
    /// Swapchain uses compositor alpha (transparent / blur windows).
    compositor_alpha: bool,
    /// True when the swapchain uses Pre/Post-multiplied compositor alpha.
    pub(super) composition_swapchain: bool,
    /// Creation chrome; re-applied after surface configure (Windows DWM punch-through).
    chrome: super::window_chrome::WindowOptions,
}

/// `queue.write_buffer` is executed before render pass during submit; the same buffer is written to offset 0 multiple times.
/// Only the last time will be left (as soon as the pie chart enters the viewport, the polyline instance is covered into a pie chart rectangle). It must be packaged once and drawn according to instance range.
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
    cull: Option<[f32; 4]>,
) {
    let end = end.min(scaled_tex.len());
    let mut ti = start.min(end);
    while ti < end {
        if let Some(damage) = cull {
            if !super::draw_cull::textured_hits_damage(&scaled_tex[ti], damage) {
                ti += 1;
                continue;
            }
        }
        let key = scaled_tex[ti].texture_key.as_str();
        let Some(gpu_img) = image_textures.get(key) else {
            ti += 1;
            continue;
        };
        let run = if cull.is_some() {
            1
        } else {
            scaled_tex[ti..end]
                .iter()
                .take_while(|d| d.texture_key == key)
                .count()
                .max(1)
        };
        pass.set_pipeline(&pipeline.render_pipeline);
        pass.set_bind_group(0, &gpu_img.bind_group, &[]);
        pass.set_vertex_buffer(0, pipeline.vertex_buffer.slice(..));
        pass.set_vertex_buffer(1, instance_buffer.slice(..));
        pass.set_index_buffer(pipeline.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
        pass.draw_indexed(0..pipeline.num_indices, 0, ti as u32..(ti + run) as u32);
        super::frame_stats::note_scene_batch(
            super::frame_stats::SceneKind::Surface,
            run as u32,
        );
        ti += run;
    }
}

/// Hash underline rects that this frame will actually draw.
///
/// Page-stack / overlay prepare returns empty `underline_instances`; the rects live in the
/// band vecs. Omitting those from `last_gpu_frame_fp` made Link `hover:underline` leave
/// match the previous fingerprint (same button rects, same text color/weight) so present
/// was skipped and the decoration stayed until an unrelated instance change.
fn hash_present_underline_layers(
    hasher: &mut impl Hasher,
    underline_instances: &[InstanceInput],
    page_stack_band_underlines: &[Vec<InstanceInput>],
    overlay_band_underlines: &[Vec<InstanceInput>],
) {
    hasher.write(bytemuck::cast_slice(underline_instances));
    for ul in page_stack_band_underlines {
        hasher.write(bytemuck::cast_slice(ul));
    }
    for ul in overlay_band_underlines {
        hasher.write(bytemuck::cast_slice(ul));
    }
}

/// Underlines actually on screen after this frame's prepare.
///
/// Page-stack prepare stores bands and **clears** `last_underline_instances`.
/// Reuse frames also push empty band vecs. Node present must not treat that
/// as "no decorations" or Link leave cannot erase, and the next hover stacks.
fn current_underline_layer(
    has_page_stack_bands: bool,
    reuse_page_stack: bool,
    page_stack_flat: &[InstanceInput],
    page_underlines: &[InstanceInput],
    last_presented: &[InstanceInput],
) -> Vec<InstanceInput> {
    if has_page_stack_bands {
        if page_stack_flat.is_empty() && reuse_page_stack {
            last_presented.to_vec()
        } else {
            page_stack_flat.to_vec()
        }
    } else {
        page_underlines.to_vec()
    }
}

/// Draw CSS `text-shadow` bitmaps. Must run **after** the band's opaque fills and
/// **before** glyphon glyphs — otherwise page-stack card backgrounds cover the quads
/// (gallery「Text-shadow」vanishes whenever `page_stack_bands` is non-empty).
fn draw_shadow_tex_draws<'pass>(
    pass: &mut wgpu::RenderPass<'pass>,
    draws: &[crate::renderer::prepare_text::ShadowTexDraw],
    shadow_tex_cache: &'pass HashMap<u64, super::image_registry::GpuImageTexture>,
    textured_pipeline: &'pass TexturedQuadPipeline,
    device: &wgpu::Device,
    cull: Option<[f32; 4]>,
) {
    for sd in draws {
        if let Some(damage) = cull {
            if crate::renderer::aabbs_disjoint(sd.rect, damage) {
                continue;
            }
        }
        let Some(gpu_img) = shadow_tex_cache.get(&sd.cache_key) else {
            continue;
        };
        let (clip_pos, clip_size) =
            crate::renderer::prepare_text::shadow_tex_instance_clip(sd.clip);
        let inst = TexturedInstanceInput {
            rect_pos: [sd.rect[0], sd.rect[1]],
            rect_size: [sd.rect[2], sd.rect[3]],
            tint: [1.0, 1.0, 1.0, 1.0],
            uv_offset: [0.0, 0.0],
            uv_scale: [1.0, 1.0],
            transform: crate::style::transform::MAT2D_IDENTITY,
            flags: 0.0,
            clip_pos,
            clip_size,
            warp_c01: TexturedInstanceInput::WARP_C01_OFF,
            warp_c23: TexturedInstanceInput::WARP_C23_OFF,
            warp_w: TexturedInstanceInput::WARP_W_OFF,
        };
        let shadow_inst_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Shadow instance"),
            contents: bytemuck::cast_slice(&[inst]),
            usage: wgpu::BufferUsages::VERTEX,
        });
        pass.set_pipeline(&textured_pipeline.render_pipeline);
        pass.set_bind_group(0, &gpu_img.bind_group, &[]);
        pass.set_vertex_buffer(0, textured_pipeline.vertex_buffer.slice(..));
        pass.set_vertex_buffer(1, shadow_inst_buf.slice(..));
        pass.set_index_buffer(
            textured_pipeline.index_buffer.slice(..),
            wgpu::IndexFormat::Uint16,
        );
        pass.draw_indexed(0..textured_pipeline.num_indices, 0, 0..1);
        super::frame_stats::note_scene_batch(super::frame_stats::SceneKind::ShadowTex, 1);
    }
}

/// Where textured `text-shadow` quads must be submitted relative to page fills.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // exercised by unit tests + documents draw-site contract
enum TextShadowDrawSlot {
    /// No page-stack bands: after all page rects/textures, before glyphon.
    AfterPageFills,
    /// CarouselStack / z-index bands: inside each band after fills, before that band's text.
    PerBandAfterRects,
}

struct GpuPresentPlan {
    logical_policy: super::damage::FrameClearPolicy,
    scissor_physical: Option<(u32, u32, u32, u32)>,
    use_partial_damage: bool,
    #[allow(dead_code)]
    warm_retain: bool,
    /// Composite target. When GPU partial damage is on, FullClear still draws
    /// into retain so the next overlay/hover PartialLoad can Load page pixels.
    draw_to_retain: bool,
    cull_phys: Option<[f32; 4]>,
    #[allow(dead_code)]
    cull_logical: Option<[f32; 4]>,
    /// Scroll viewport layer: Load retain then Clear the scissor before redrawing.
    clear_damage: bool,
    /// Paint-only: per-node scissor, no cross-node AABB cull.
    use_node_present: bool,
}

fn text_nodes_for_prepare(
    nodes: &[crate::core::tree::TextLayoutInfo],
    lo: usize,
    hi: usize,
    cull: Option<[f32; 4]>,
) -> Cow<'_, [crate::core::tree::TextLayoutInfo]> {
    let lo = lo.min(nodes.len());
    let hi = hi.min(nodes.len()).max(lo);
    let slice = &nodes[lo..hi];
    match cull {
        Some(d) => Cow::Owned(super::draw_cull::clip_text_nodes_to_rect(slice, d)),
        None => Cow::Borrowed(slice),
    }
}

fn instances_for_upload<'a>(
    slice: &'a [InstanceInput],
    cull: Option<[f32; 4]>,
) -> Cow<'a, [InstanceInput]> {
    match cull {
        Some(d) => Cow::Owned(super::draw_cull::filter_instances(slice, d)),
        None => Cow::Borrowed(slice),
    }
}

fn draw_instance_range(
    pass: &mut wgpu::RenderPass<'_>,
    pipeline: &RuiPipeline,
    instances: &[InstanceInput],
    start: u32,
    count: u32,
    cull: Option<[f32; 4]>,
) {
    if count == 0 {
        return;
    }
    // Capture / scene passes may pass an empty CPU slice while the GPU buffer is populated.
    // Fail open: never skip draws when we cannot test bounds.
    if let Some(damage) = cull {
        if !instances.is_empty() {
            for (s, c) in super::draw_cull::visible_instance_runs(instances, start, count, damage) {
                if c > 0 {
                    super::frame_stats::note_scene_batch(super::frame_stats::SceneKind::Quad, c);
                    pass.draw_indexed(0..pipeline.num_indices, 0, s..(s + c));
                }
            }
            return;
        }
    }
    super::frame_stats::note_scene_batch(super::frame_stats::SceneKind::Quad, count);
    pass.draw_indexed(0..pipeline.num_indices, 0, start..start + count);
}

fn draw_cpu_sdf_instances<'a>(
    pass: &mut wgpu::RenderPass<'a>,
    pipeline: &'a RuiPipeline,
    device: &wgpu::Device,
    dummy: &'a wgpu::BindGroup,
    instances: &[InstanceInput],
) {
    if instances.is_empty() {
        return;
    }
    pipeline.bind_sdf(pass, false, dummy);
    pass.set_vertex_buffer(0, pipeline.vertex_buffer.slice(..));
    let buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Node present CPU SDF"),
        contents: bytemuck::cast_slice(instances),
        usage: wgpu::BufferUsages::VERTEX,
    });
    pass.set_vertex_buffer(1, buf.slice(..));
    pass.set_index_buffer(pipeline.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
    pass.draw_indexed(
        0..pipeline.num_indices,
        0,
        0..instances.len() as u32,
    );
    super::frame_stats::note_scene_batch(
        super::frame_stats::SceneKind::Underline,
        instances.len() as u32,
    );
}

fn draw_scissor_clear<'a>(
    pass: &mut wgpu::RenderPass<'a>,
    pipeline: &'a RuiPipeline,
    device: &wgpu::Device,
    rect: [f32; 4],
    clear: wgpu::Color,
) {
    let inst = InstanceInput {
        rect_pos: [rect[0], rect[1]],
        rect_size: [rect[2], rect[3]],
        // Src-over with a=0 (compositor-alpha TRANSPARENT) is a no-op and leaves
        // overlay keyframes in retain. Always replace the hole, then restore page.
        color: [
            clear.r as f32,
            clear.g as f32,
            clear.b as f32,
            1.0,
        ],
        border_color: [0.0; 4],
        radii: [0.0; 4],
        border_width: 0.0,
        shadow_color: [0.0; 4],
        shadow_offset: [0.0, 0.0],
        shadow_blur: 0.0,
        shadow_spread: 0.0,
        shadow_inset: 0.0,
        transform: crate::style::transform::MAT2D_IDENTITY,
        clip_pos: [0.0, 0.0],
        clip_size: InstanceInput::NO_CLIP_SIZE,
        border_extra: 0.0,
        backdrop_blur: 0.0,
    };
    let buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Scroll layer clear"),
        contents: bytemuck::cast_slice(&[inst]),
        usage: wgpu::BufferUsages::VERTEX,
    });
    pipeline.bind_sdf(pass, false, pipeline.dummy_backdrop());
    pass.set_vertex_buffer(0, pipeline.vertex_buffer.slice(..));
    pass.set_vertex_buffer(1, buf.slice(..));
    pass.set_index_buffer(pipeline.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
    pass.draw_indexed(0..pipeline.num_indices, 0, 0..1);
}

#[inline]
#[allow(dead_code)]
fn text_shadow_draw_slot(has_page_stack_bands: bool) -> TextShadowDrawSlot {
    if has_page_stack_bands {
        TextShadowDrawSlot::PerBandAfterRects
    } else {
        TextShadowDrawSlot::AfterPageFills
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
    backdrop: &'pass wgpu::BindGroup,
    instances: &[InstanceInput],
    cull: Option<[f32; 4]>,
) {
    use crate::layout::layout_tree::PaintOp;
    let hi = hi.min(ops.len());
    let lo = lo.min(hi);
    for op in &ops[lo..hi] {
        match *op {
            PaintOp::Rects { start, count } if count > 0 => {
                pipeline.bind_sdf(pass, true, backdrop);
                pass.set_vertex_buffer(0, pipeline.vertex_buffer.slice(..));
                pass.set_vertex_buffer(1, instance_buffer.slice(..));
                pass.set_index_buffer(pipeline.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
                draw_instance_range(pass, pipeline, instances, start, count, cull);
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
                    cull,
                );
            }
            _ => {}
        }
    }
}

fn draw_page_stack_band_to_pass<'pass>(
    pass: &mut wgpu::RenderPass<'pass>,
    band_i: usize,
    band: &crate::layout::layout_tree::OverlayBandRange,
    paint_ops: &[crate::layout::layout_tree::PaintOp],
    pipeline: &'pass RuiPipeline,
    instance_buffer: &'pass wgpu::Buffer,
    textured_pipeline: &'pass TexturedQuadPipeline,
    textured_instance_buffer: &'pass wgpu::Buffer,
    image_textures: &'pass HashMap<String, super::image_registry::GpuImageTexture>,
    scaled_tex: &'pass [ScaledTexturedDraw],
    scaled_input_chrome: &'pass [InstanceInput],
    device: &'pass wgpu::Device,
    page_stack_band_underlines: &[Vec<InstanceInput>],
    page_stack_band_shadows: &[Vec<crate::renderer::prepare_text::ShadowTexDraw>],
    shadow_tex_cache: &'pass HashMap<u64, super::image_registry::GpuImageTexture>,
    backdrop: &'pass wgpu::BindGroup,
    instances: &[InstanceInput],
    cull: Option<[f32; 4]>,
) {
    if band.paint_ops.0 < band.paint_ops.1 {
        draw_paint_ops(
            pass,
            paint_ops,
            band.paint_ops.0,
            band.paint_ops.1,
            pipeline,
            instance_buffer,
            textured_pipeline,
            textured_instance_buffer,
            image_textures,
            scaled_tex,
            backdrop,
            instances,
            cull,
        );
    } else if band.instance.0 < band.instance.1 {
        pipeline.bind_sdf(pass, true, backdrop);
        pass.set_vertex_buffer(0, pipeline.vertex_buffer.slice(..));
        pass.set_vertex_buffer(1, instance_buffer.slice(..));
        pass.set_index_buffer(pipeline.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
        let start = band.instance.0 as u32;
        let count = (band.instance.1 - band.instance.0) as u32;
        draw_instance_range(pass, pipeline, instances, start, count, cull);
        if band.textured.0 < band.textured.1 {
            draw_packed_textured_runs(
                pass,
                textured_pipeline,
                textured_instance_buffer,
                image_textures,
                scaled_tex,
                band.textured.0,
                band.textured.1,
                cull,
            );
        }
    }

    if band.chrome.0 < band.chrome.1 {
        let chrome_slice =
            instances_for_upload(&scaled_input_chrome[band.chrome.0..band.chrome.1], cull);
        if !chrome_slice.is_empty() {
            pipeline.bind_sdf(pass, false, backdrop);
            pass.set_vertex_buffer(0, pipeline.vertex_buffer.slice(..));
            let chrome_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Page Stack Chrome Buffer"),
                contents: bytemuck::cast_slice(chrome_slice.as_ref()),
                usage: wgpu::BufferUsages::VERTEX,
            });
            pass.set_vertex_buffer(1, chrome_buffer.slice(..));
            pass.set_index_buffer(pipeline.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
            pass.draw_indexed(
                0..pipeline.num_indices,
                0,
                0..chrome_slice.len() as u32,
            );
            super::frame_stats::note_scene_batch(super::frame_stats::SceneKind::Quad, 1);
        }
    }

    if let Some(ul) = page_stack_band_underlines.get(band_i) {
        let ul = instances_for_upload(ul, cull);
        if !ul.is_empty() {
            pipeline.bind_sdf(pass, false, backdrop);
            pass.set_vertex_buffer(0, pipeline.vertex_buffer.slice(..));
            let underline_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Page Stack Underline Buffer"),
                contents: bytemuck::cast_slice(ul.as_ref()),
                usage: wgpu::BufferUsages::VERTEX,
            });
            pass.set_vertex_buffer(1, underline_buffer.slice(..));
            pass.set_index_buffer(pipeline.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
            pass.draw_indexed(0..pipeline.num_indices, 0, 0..ul.len() as u32);
            super::frame_stats::note_scene_batch(super::frame_stats::SceneKind::Underline, 1);
        }
    }

    if let Some(sds) = page_stack_band_shadows.get(band_i) {
        draw_shadow_tex_draws(pass, sds, shadow_tex_cache, textured_pipeline, device, cull);
    }
}

fn capture_page_stack_local_backdrop(
    backdrop: &mut BackdropBlurPipeline,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    surface_w: u32,
    surface_h: u32,
    surface_format: wgpu::TextureFormat,
    band: &crate::layout::layout_tree::OverlayBandRange,
    page_stack_band_ranges: &[crate::layout::layout_tree::OverlayBandRange],
    band_i: usize,
    radius_px: f32,
    clear_color: wgpu::Color,
    paint_ops: &[crate::layout::layout_tree::PaintOp],
    pipeline: &RuiPipeline,
    instance_buffer: &wgpu::Buffer,
    textured_pipeline: &TexturedQuadPipeline,
    textured_instance_buffer: &wgpu::Buffer,
    image_textures: &HashMap<String, super::image_registry::GpuImageTexture>,
    scaled_tex: &[ScaledTexturedDraw],
    scaled_input_chrome: &[InstanceInput],
    page_stack_band_underlines: &[Vec<InstanceInput>],
    page_stack_band_shadows: &[Vec<crate::renderer::prepare_text::ShadowTexDraw>],
    shadow_tex_cache: &HashMap<u64, super::image_registry::GpuImageTexture>,
    text_renderer: &crate::renderer::RuiTextRenderer,
    page_stack_band_text_renderers: &[crate::renderer::RuiTextRenderer],
) {
    let dummy = pipeline.dummy_backdrop();
    backdrop.ensure_size(device, surface_w, surface_h, surface_format);

    // Own encoder + submit, same as `effect_pass::render_capture`. Prefix glyphon and the
    // later surface pass share vertex buffers / atlas; recording both into the frame encoder
    // left the scene RT with SDF fills (host blue) and no glyphs on GLES.
    let mut cap_encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("Local Backdrop Capture Encoder"),
    });
    {
        let Some(scene_view) = backdrop.scene_view() else {
            return;
        };
        let mut scene_pass = cap_encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Local Backdrop Scene Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: scene_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(clear_color),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });

        // `band_i == 0`: only intra-band SDF prefix exists (no prior paint band).
        if band_i == 0 {
            draw_page_stack_prefix_to_pass(
                &mut scene_pass,
                band,
                paint_ops,
                pipeline,
                instance_buffer,
                textured_pipeline,
                textured_instance_buffer,
                image_textures,
                scaled_tex,
                scaled_input_chrome,
                device,
                dummy,
            );
        }
        for (j, prev) in page_stack_band_ranges.iter().enumerate().take(band_i) {
            draw_page_stack_band_to_pass(
                &mut scene_pass,
                j,
                prev,
                paint_ops,
                pipeline,
                instance_buffer,
                textured_pipeline,
                textured_instance_buffer,
                image_textures,
                scaled_tex,
                scaled_input_chrome,
                device,
                page_stack_band_underlines,
                page_stack_band_shadows,
                shadow_tex_cache,
                dummy,
                &[],
                None,
            );
            if prev.text.0 < prev.text.1 {
                if j == 0 {
                    text_renderer.render(&mut scene_pass);
                } else {
                    page_stack_band_text_renderers[j - 1].render(&mut scene_pass);
                }
            }
        }
    }

    backdrop.blur(device, queue, &mut cap_encoder, radius_px);
    queue.submit(std::iter::once(cap_encoder.finish()));
}

fn draw_page_stack_prefix_to_pass<'pass>(
    pass: &mut wgpu::RenderPass<'pass>,
    band: &crate::layout::layout_tree::OverlayBandRange,
    paint_ops: &[crate::layout::layout_tree::PaintOp],
    pipeline: &'pass RuiPipeline,
    instance_buffer: &'pass wgpu::Buffer,
    textured_pipeline: &'pass TexturedQuadPipeline,
    textured_instance_buffer: &'pass wgpu::Buffer,
    image_textures: &'pass HashMap<String, super::image_registry::GpuImageTexture>,
    scaled_tex: &'pass [ScaledTexturedDraw],
    scaled_input_chrome: &'pass [InstanceInput],
    device: &'pass wgpu::Device,
    backdrop: &'pass wgpu::BindGroup,
) {
    if band.paint_ops.0 > 0 {
        draw_paint_ops(
            pass,
            paint_ops,
            0,
            band.paint_ops.0,
            pipeline,
            instance_buffer,
            textured_pipeline,
            textured_instance_buffer,
            image_textures,
            scaled_tex,
            backdrop,
            &[],
            None,
        );
    } else if band.instance.0 > 0 {
        pipeline.bind_sdf(pass, true, backdrop);
        pass.set_vertex_buffer(0, pipeline.vertex_buffer.slice(..));
        pass.set_vertex_buffer(1, instance_buffer.slice(..));
        pass.set_index_buffer(pipeline.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
        pass.draw_indexed(0..pipeline.num_indices, 0, 0..band.instance.0 as u32);
        if band.textured.0 > 0 {
            draw_packed_textured_runs(
                pass,
                textured_pipeline,
                textured_instance_buffer,
                image_textures,
                scaled_tex,
                0,
                band.textured.0,
                None,
            );
        }
    }
    if band.chrome.0 > 0 {
        pipeline.bind_sdf(pass, false, backdrop);
        pass.set_vertex_buffer(0, pipeline.vertex_buffer.slice(..));
        let chrome_slice = &scaled_input_chrome[0..band.chrome.0];
        let chrome_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Page Stack Prefix Chrome Buffer"),
            contents: bytemuck::cast_slice(chrome_slice),
            usage: wgpu::BufferUsages::VERTEX,
        });
        pass.set_vertex_buffer(1, chrome_buffer.slice(..));
        pass.set_index_buffer(pipeline.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
        pass.draw_indexed(0..pipeline.num_indices, 0, 0..chrome_slice.len() as u32);
    }
}

fn band_instance_backdrop_blur(
    instances: &[InstanceInput],
    band: &crate::layout::layout_tree::OverlayBandRange,
) -> f32 {
    let lo = band.instance.0.min(instances.len());
    let hi = band.instance.1.min(instances.len());
    if lo >= hi {
        return 0.0;
    }
    instances[lo..hi]
        .iter()
        .map(|i| i.backdrop_blur)
        .fold(0.0_f32, f32::max)
}

/// Physical scissor for local `backdrop-filter` capture: the host overflow clip of
/// blur instances in `band`. `instances` must already be in framebuffer pixels
/// (the layout buffer is `take`n into `scaled_instances` before capture).
/// Kept for regression tests; the capture pass itself uses a full-surface scissor
/// so glyphon prefix glyphs are not dropped.
#[cfg(test)]
fn local_backdrop_host_scissor_physical(
    instances: &[InstanceInput],
    band: &crate::layout::layout_tree::OverlayBandRange,
    surface_w: u32,
    surface_h: u32,
) -> Option<(u32, u32, u32, u32)> {
    let lo = band.instance.0.min(instances.len());
    let hi = band.instance.1.min(instances.len());
    if lo >= hi {
        return None;
    }
    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    let mut max_x = f32::MIN;
    let mut max_y = f32::MIN;
    let mut found = false;
    for inst in &instances[lo..hi] {
        if inst.backdrop_blur <= 0.5 {
            continue;
        }
        let (x, y, w, h) = if inst.clip_size[0] > 0.5 && inst.clip_size[1] > 0.5 {
            (
                inst.clip_pos[0],
                inst.clip_pos[1],
                inst.clip_size[0],
                inst.clip_size[1],
            )
        } else {
            (
                inst.rect_pos[0],
                inst.rect_pos[1],
                inst.rect_size[0],
                inst.rect_size[1],
            )
        };
        if w <= 0.0 || h <= 0.0 {
            continue;
        }
        found = true;
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x + w);
        max_y = max_y.max(y + h);
    }
    if !found {
        return None;
    }
    let x0 = min_x.floor().max(0.0) as u32;
    let y0 = min_y.floor().max(0.0) as u32;
    let x1 = max_x.ceil().min(surface_w as f32).max(0.0) as u32;
    let y1 = max_y.ceil().min(surface_h as f32).max(0.0) as u32;
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    Some((x0, y0, x1.saturating_sub(x0), y1.saturating_sub(y0)))
}

#[cfg(test)]
mod local_backdrop_capture_tests {
    use super::*;
    use crate::layout::layout_tree::OverlayBandRange;
    use bytemuck::Zeroable;

    fn band_instances(lo: usize, hi: usize) -> OverlayBandRange {
        OverlayBandRange {
            instance: (lo, hi),
            textured: (0, 0),
            text: (0, 0),
            chrome: (0, 0),
            paint_ops: (0, 0),
            backdrop_blur: 0.0,
            underlay_opacity: 1.0,
        }
    }

    #[test]
    fn local_blur_radius_is_zero_when_instance_buffer_was_taken() {
        // Regression: `mem::take(render_result.instances)` left an empty vec; looking
        // there for backdrop_blur skipped capture and the scrim sampled the dummy.
        let band = band_instances(0, 4);
        assert_eq!(band_instance_backdrop_blur(&[], &band), 0.0);
    }

    #[test]
    fn local_blur_radius_reads_scaled_instance_buffer() {
        let mut inst = InstanceInput::zeroed();
        inst.backdrop_blur = 16.0;
        inst.rect_size = [100.0, 80.0];
        inst.clip_size = InstanceInput::NO_CLIP_SIZE;
        let band = band_instances(0, 1);
        assert!((band_instance_backdrop_blur(&[inst], &band) - 16.0).abs() < 0.01);
    }

    #[test]
    fn host_scissor_uses_physical_clip_without_rescaling() {
        let mut inst = InstanceInput::zeroed();
        inst.backdrop_blur = 16.0;
        inst.clip_pos = [100.0, 200.0];
        inst.clip_size = [220.0, 140.0];
        let band = band_instances(0, 1);
        let sc = local_backdrop_host_scissor_physical(&[inst], &band, 800, 600).expect("scissor");
        assert_eq!(sc, (100, 200, 220, 140));
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
        // Size: ignore flex/scrollbar subpixel so idle frames do not reshape.
        const SIZE_EPS: f32 = 0.5;
        // Position must stay tight even after smooth-scroll settles. A 0.5px slack
        // parked glyphs at the last catch-up sample while rects snapped to target
        // (gallery avatar labels sitting beside their circles).
        const POS_EPS: f32 = 0.02;
        if a.len() != b.len() {
            return false;
        }
        a.iter().zip(b.iter()).all(|(x, y)| {
            x.text == y.text
                && x.shape_suffix == y.shape_suffix
                && x.template_id == y.template_id
                && x.style == y.style
                && Self::approx_clip_eq(x.clip_bounds, y.clip_bounds, POS_EPS)
                && Self::approx_eq(x.x, y.x, POS_EPS)
                && Self::approx_eq(x.y, y.y, POS_EPS)
                && Self::approx_eq(x.width, y.width, SIZE_EPS)
                && Self::approx_eq(x.height, y.height, SIZE_EPS)
                && Self::approx_eq(x.paint_scale, y.paint_scale, 0.002)
                && x.paint_transform
                    .iter()
                    .zip(y.paint_transform.iter())
                    .all(|(a, b)| Self::approx_eq(*a, *b, 0.002))
                && Self::approx_eq(x.paint_origin[0], y.paint_origin[0], POS_EPS)
                && Self::approx_eq(x.paint_origin[1], y.paint_origin[1], POS_EPS)
        })
    }

    pub(super) fn note_winit_resized(&mut self, physical_size: PhysicalSize<u32>) {
        // Minimize reports 0×0. Pinning that (or keeping a stale maximized size)
        // makes `layout_fast` + `scroll_only` stick, and restore then never computes.
        if physical_size.width == 0 || physical_size.height == 0 {
            self.last_resized_event = None;
            return;
        }
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
        let focused = use_signal("__rui_button_focus_id", String::new).peek();
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

    /// Create the instance / surface / adapter triple from the first backend that works.
    async fn acquire_adapter(
        window: &Arc<Window>,
        force_software: bool,
        compositor_alpha: bool,
    ) -> (wgpu::Instance, wgpu::Surface<'static>, wgpu::Adapter) {
        let candidates = crate::platform::gpu_backend_candidates(compositor_alpha);
        if !force_software {
            for (backends, name) in &candidates {
                let instance = crate::renderer::wgpu_compat::instance(*backends);
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
                        ..Default::default()
                    })
                    .await;
                if let Ok(adapter) = adapter {
                    log::info!("gpu backend: {name} ({})", adapter.get_info().name);
                    return (instance, surface, adapter);
                }
                log::debug!("backend {name}: no adapter");
            }
        }

        // WARP / llvmpipe last: only DX12 and Vulkan expose a fallback adapter.
        for backends in [wgpu::Backends::DX12, wgpu::Backends::VULKAN] {
            let instance = crate::renderer::wgpu_compat::instance(backends);
            let Ok(surface) = instance.create_surface(window.clone()) else {
                continue;
            };
            let adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::LowPower,
                    compatible_surface: Some(&surface),
                    force_fallback_adapter: true,
                    ..Default::default()
                })
                .await;
            if let Ok(adapter) = adapter {
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

    pub(super) async fn new_bootstrap(
        window: Arc<Window>,
        chrome: super::window_chrome::WindowOptions,
    ) -> Self {
        let compositor_alpha = chrome.compositor_alpha();
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
        let (instance, surface, adapter) =
            Self::acquire_adapter(&window, force_software, compositor_alpha).await;
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
            .request_device(&crate::renderer::wgpu_compat::device_desc(
                None,
                required_limits,
                wgpu::MemoryHints::MemoryUsage,
            ))
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
        let alpha_mode =
            crate::renderer::pick_composite_alpha(&surface_caps.alpha_modes, compositor_alpha);
        let composition_swapchain = matches!(
            alpha_mode,
            wgpu::CompositeAlphaMode::PreMultiplied | wgpu::CompositeAlphaMode::PostMultiplied
        );
        log::info!("surface present_mode: {present_mode:?} alpha_mode: {alpha_mode:?}");

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode,
            alpha_mode,
            color_space: wgpu::SurfaceColorSpace::Auto,
            view_formats: vec![],
            desired_maximum_frame_latency: 1,
        };
        surface.configure(&device, &config);
        super::window_chrome::apply_created_window(&window, &chrome, composition_swapchain);

        let mut pipeline = RuiPipeline::new(&device, &config);
        let textured_pipeline = TexturedQuadPipeline::new(&device, &config);
        let backdrop_blur = BackdropBlurPipeline::new(&device, config.format);
        let effect_pass = crate::renderer::EffectPassGpu::new(&device, config.format);
        let css_filter_pass = crate::renderer::CssFilterPassGpu::new(&device, config.format);
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
        let mut clip_polys = render_result.clip_polys;
        scale_clip_polys_in_place(&mut clip_polys, scale);
        while clip_polys.len() < scaled_instances.len() {
            clip_polys.push(crate::style::GpuClipPoly::NONE);
        }
        pipeline.ensure_clip_polys(&device, &queue, &clip_polys);
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
        let capture_text_renderer =
            RuiTextRenderer::new_sharing_font_system(&device, &queue, &config, font_ptr);
        let overlay_text_renderer =
            RuiTextRenderer::new_sharing_font_system(&device, &queue, &config, font_ptr);
        log::debug!("new_bootstrap: after text_renderer");

        Self {
            window,
            instance,
            adapter,
            device,
            queue,
            surface,
            config,
            size,
            pipeline,
            textured_pipeline,
            #[cfg(feature = "scene3d")]
            scene3d: None,
            backdrop_blur,
            effect_pass,
            css_filter_pass,
            present_retain,
            image_textures: crate::renderer::TexturePool::new(),
            textured_instance_buffer,
            textured_instance_capacity: 256,
            instance_buffer,
            instance_buffer_capacity: scaled_instances.len(),
            layout_tree,
            text_renderer,
            capture_text_renderer,
            overlay_text_renderer,
            overlay_band_text_renderers: Vec::new(),
            page_stack_band_text_renderers: Vec::new(),
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
            render_prof_last: Instant::now(),
            last_render_at: Instant::now(),
            render_prof_frames: 0,
            render_prof_pre_ms: 0.0,
            render_prof_layout_ms: 0.0,
            render_prof_layout_rebuild_ms: 0.0,
            render_prof_layout_setup_ms: 0.0,
            render_prof_layout_taffy_ms: 0.0,
            render_prof_layout_collect_ms: 0.0,
            render_prof_draw_ms: 0.0,
            render_prof_acquire_ms: 0.0,
            render_prof_submit_ms: 0.0,
            render_prof_present_ms: 0.0,
            render_prof_upload_ms: 0.0,
            render_prof_text_ms: 0.0,
            render_prof_encode_ms: 0.0,
            last_text_nodes_cache: Vec::new(),
            last_text_ui_scale: 0.0,
            last_text_surface_size: [0, 0],
            last_underline_instances: Vec::new(),
            last_presented_underlines: Vec::new(),
            last_caret_instances: Vec::new(),
            last_caret_blink_phase: 0,
            cpu_text_profiler: CpuTextProfiler::new(),
            shadow_tex_cache: std::collections::HashMap::new(),
            shadow_rect_cache: std::collections::HashMap::new(),
            last_shadow_tex_draws: Vec::new(),
            damage_rect: None,
            damage_nodes: Vec::new(),
            force_full_redraw: true,
            portal_membership_dirty: false,
            page_geom_dirty: false,
            last_overlay_boxes: Vec::new(),
            overlay_occupied: None,
            last_portal_layer_damage: None,
            has_presented_frame: false,
            last_gpu_frame_fp: 0,
            last_scroll_offset_fp: 0,
            last_scroll_offsets: HashMap::new(),
            scroll_layer_clear: false,
            scroll_text_clip: None,
            overlay_restore_clear: false,
            compositor_alpha,
            composition_swapchain,
            chrome,
        }
    }

    /// Fingerprint scroll viewport offsets (logical). Used to force FullClear when content moves.
    fn scroll_offset_fingerprint(root: &Element) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        let mut hasher = DefaultHasher::new();
        crate::core::tree::walk_elements(root, |el| {
            if let Some(fid) = el.focus_id.as_deref() {
                if let Some(rest) = fid.strip_prefix(SCROLL_VIEWPORT_FOCUS_PREFIX) {
                    rest.hash(&mut hasher);
                    el.scroll_offset[0].to_bits().hash(&mut hasher);
                    el.scroll_offset[1].to_bits().hash(&mut hasher);
                }
            }
        });
        hasher.finish()
    }

    fn scroll_offsets_map(root: &Element) -> HashMap<String, [u32; 2]> {
        let mut map = HashMap::new();
        crate::core::tree::walk_elements(root, |el| {
            if let Some(fid) = el.focus_id.as_deref() {
                if let Some(rest) = fid.strip_prefix(SCROLL_VIEWPORT_FOCUS_PREFIX) {
                    map.insert(
                        rest.to_string(),
                        [
                            el.scroll_offset[0].to_bits(),
                            el.scroll_offset[1].to_bits(),
                        ],
                    );
                }
            }
        });
        map
    }

    /// Union previous-frame clip bounds of viewports whose offset changed.
    /// `false` → caller must FullClear (missing metrics).
    fn union_scrolled_viewport_damage(
        &mut self,
        prev: &HashMap<String, [u32; 2]>,
        cur: &HashMap<String, [u32; 2]>,
    ) -> bool {
        let mut any = false;
        for (id, off) in cur {
            if prev.get(id) == Some(off) {
                continue;
            }
            let Some(bounds) = self
                .last_render_scroll_metrics
                .iter()
                .find(|(rest, _, _, _)| rest == id)
                .map(|(_, b, _, _)| *b)
            else {
                return false;
            };
            self.union_damage(bounds);
            self.scroll_text_clip = Some(match self.scroll_text_clip {
                Some(prev) => super::damage::union_rects(prev, bounds),
                None => bounds,
            });
            any = true;
        }
        for id in prev.keys() {
            if cur.contains_key(id) {
                continue;
            }
            if let Some(bounds) = self
                .last_render_scroll_metrics
                .iter()
                .find(|(rest, _, _, _)| rest == id)
                .map(|(_, b, _, _)| *b)
            {
                self.union_damage(bounds);
                self.scroll_text_clip = Some(match self.scroll_text_clip {
                    Some(prev) => super::damage::union_rects(prev, bounds),
                    None => bounds,
                });
                any = true;
            }
        }
        any
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
        self.damage_nodes.clear();
        self.scroll_text_clip = None;
        super::damage::clear_frame_paint_nodes();
    }

    /// Portal layer gained/lost a surface. Overlay-local damage, not gallery FullClear.
    pub(crate) fn mark_portal_membership_dirty(&mut self) {
        self.portal_membership_dirty = true;
    }

    /// After layout: overlay open/close dirties **overlay nodes** only.
    /// Page hover stays on its own nodes — never unioned into one AABB with the popover.
    /// Painted overlay with no layout box → FullClear (cannot scissor).
    fn apply_portal_overlay_damage(
        &mut self,
        overlay_present: bool,
        overlay_visual: Option<[f32; 4]>,
    ) {
        let cur_boxes = crate::layout::layout_tree::hoisted_portal_boxes(&self.root_element);
        let cur = crate::layout::layout_tree::hoisted_portal_damage(&self.root_element);
        let prev = self.last_portal_layer_damage;
        let membership = self.portal_membership_dirty;
        if let Some(c) = cur {
            self.overlay_occupied = Some(match self.overlay_occupied {
                Some(o) => super::damage::union_rects(o, c),
                None => c,
            });
        }
        // Layout boxes miss expand-from-chip scale, shadows, and wrap overflow.
        // Occupancy must track what GPU actually stamped into present_retain.
        if let Some(visual) = overlay_visual {
            self.overlay_occupied =
                super::damage::union_overlay_visual_bounds(self.overlay_occupied, visual);
        }
        // Painted footprint this frame (clip/max-width may shrink while Taffy
        // wrapper bounds stay large). Restore against this, not the wrapper.
        let current_paint = overlay_visual.or(cur);
        let needs_restore =
            super::damage::overlay_restore_needed(self.overlay_occupied, current_paint);
        if overlay_present
            && super::damage::overlay_pixels_need_full_clear(overlay_present, cur)
            && !self.force_full_redraw
        {
            self.mark_full_redraw();
            self.overlay_restore_clear = false;
        } else if needs_restore {
            self.overlay_restore_clear = self.overlay_occupied.is_some();
            if !self.force_full_redraw && !self.scroll_layer_clear {
                if let Some(occ) = self.overlay_occupied {
                    self.damage_nodes.push(super::damage::PaintDirty {
                        key: 0xE0E1_0000,
                        bounds: super::draw_cull::padded_xywh(
                            occ,
                            super::damage::OVERLAY_RESTORE_PAD_LOGICAL,
                        ),
                        kind: super::damage::PaintDirtyKind::OverlayRestore,
                    });
                }
                if overlay_present || !cur_boxes.is_empty() {
                    if cur_boxes.is_empty() {
                        if let Some(b) = current_paint.or(self.overlay_occupied) {
                            self.damage_nodes.push(super::damage::PaintDirty {
                                key: 0xE0E0_0000,
                                bounds: b,
                                kind: super::damage::PaintDirtyKind::Overlay,
                            });
                        }
                    } else {
                        for (i, b) in cur_boxes.iter().copied().enumerate() {
                            let bounds = match overlay_visual {
                                Some(v) => super::damage::union_rects(b, v),
                                None => b,
                            };
                            self.damage_nodes.push(super::damage::PaintDirty {
                                key: 0xE0E0_0000 | i as u64,
                                bounds,
                                kind: super::damage::PaintDirtyKind::Overlay,
                            });
                        }
                    }
                }
            }
            if cur_boxes.is_empty() && !overlay_present {
                self.overlay_occupied.take();
            }
        } else {
            self.overlay_restore_clear = false;
            if !self.force_full_redraw && !self.scroll_layer_clear && (membership || prev != cur)
            {
                for (i, b) in cur_boxes.iter().copied().enumerate() {
                    let bounds = match overlay_visual {
                        Some(v) => super::damage::union_rects(b, v),
                        None => b,
                    };
                    self.damage_nodes.push(super::damage::PaintDirty {
                        key: 0xE0E0_0000 | i as u64,
                        bounds,
                        kind: super::damage::PaintDirtyKind::Overlay,
                    });
                }
            }
        }
        self.last_overlay_boxes = cur_boxes;
        self.last_portal_layer_damage = cur;
        self.portal_membership_dirty = false;
        self.page_geom_dirty = false;
    }

    /// Overlay instance/text AABB in logical px (physical collect coords ÷ `ui_scale`).
    fn overlay_visual_bounds_logical(
        instances: &[InstanceInput],
        textured: &[ScaledTexturedDraw],
        texts: &[crate::layout::layout_tree::TextLayoutInfo],
        chrome: &[InstanceInput],
        ui_scale: f32,
    ) -> Option<[f32; 4]> {
        let inv = 1.0 / ui_scale.max(1e-6);
        let mut acc = None;
        let absorb_logical = |acc: &mut Option<[f32; 4]>, b: [f32; 4]| {
            *acc = super::damage::union_overlay_visual_bounds(*acc, b);
        };
        let absorb_physical = |acc: &mut Option<[f32; 4]>, b: [f32; 4]| {
            absorb_logical(acc, super::draw_cull::scale_xywh(b, inv));
        };
        for i in instances {
            absorb_physical(&mut acc, super::draw_cull::instance_paint_bounds(i));
        }
        for i in chrome {
            absorb_physical(&mut acc, super::draw_cull::instance_paint_bounds(i));
        }
        for d in textured {
            absorb_physical(&mut acc, super::draw_cull::textured_paint_bounds(d));
        }
        for n in texts {
            absorb_logical(&mut acc, super::draw_cull::text_paint_bounds(n));
        }
        acc
    }

    /// Logical padding around paint damage for shadows / AA lives in
    /// [`super::draw_cull::DAMAGE_CULL_PAD_LOGICAL`] (keep scissor and cull pads equal).

    /// GPU PartialLoad: UI draws into `present_retain`; swapchain only gets a full blit.
    /// Default **on** (draw-list cull + scissor). `RUI_PARTIAL_DAMAGE=0` forces surface FullClear.
    /// Overlay portal membership scissors to overlay bounds. Scroll uses viewport-layer Clear inside the scissor.
    fn gpu_partial_damage_enabled() -> bool {
        static ENABLED: OnceLock<bool> = OnceLock::new();
        *ENABLED.get_or_init(|| match std::env::var("RUI_PARTIAL_DAMAGE") {
            Ok(v) => {
                let v = v.trim();
                !(v == "0" || v.eq_ignore_ascii_case("false") || v.eq_ignore_ascii_case("off"))
            }
            Err(_) => true,
        })
    }

    fn plan_gpu_present(&self, ui_scale: f32) -> GpuPresentPlan {
        let (logical_w, logical_h) = self.logical_size();
        let viewport_logical = [0.0, 0.0, logical_w, logical_h];
        let node_present = !self.scroll_layer_clear && !self.damage_nodes.is_empty();
        let mut logical_policy = if node_present {
            super::damage::frame_clear_policy_for_nodes(
                self.has_presented_frame,
                self.force_full_redraw,
                &self.damage_nodes,
                viewport_logical,
            )
        } else {
            super::damage::frame_clear_policy(
                self.has_presented_frame,
                self.force_full_redraw,
                self.damage_rect,
                viewport_logical,
            )
        };
        let mut scissor_physical = None;
        if !node_present
            && matches!(logical_policy, super::damage::FrameClearPolicy::PartialLoad)
        {
            scissor_physical = self.damage_rect.and_then(|damage| {
                super::damage::damage_to_scissor_physical(
                    damage,
                    super::draw_cull::DAMAGE_CULL_PAD_LOGICAL,
                    ui_scale,
                    self.config.width,
                    self.config.height,
                )
            });
            if scissor_physical.is_none() {
                logical_policy = super::damage::FrameClearPolicy::FullClear;
            }
        }
        let keep_retain = Self::gpu_partial_damage_enabled();
        let want_gpu_partial = keep_retain
            && matches!(logical_policy, super::damage::FrameClearPolicy::PartialLoad);
        let use_partial_damage = want_gpu_partial && self.present_retain.contents_valid();
        let warm_retain = keep_retain && !self.present_retain.contents_valid();
        let draw_to_retain = keep_retain;
        let use_node_present = node_present && use_partial_damage;
        let (cull_phys, cull_logical) = if use_partial_damage && !use_node_present {
            if let Some(damage) = self.damage_rect {
                (
                    Some(super::draw_cull::damage_cull_rect_physical(
                        damage,
                        super::draw_cull::DAMAGE_CULL_PAD_LOGICAL,
                        ui_scale,
                    )),
                    Some(super::draw_cull::padded_xywh(
                        damage,
                        super::draw_cull::DAMAGE_CULL_PAD_LOGICAL,
                    )),
                )
            } else {
                (None, None)
            }
        } else {
            (None, None)
        };
        GpuPresentPlan {
            logical_policy,
            scissor_physical,
            use_partial_damage,
            warm_retain,
            draw_to_retain,
            cull_phys,
            cull_logical,
            clear_damage: use_partial_damage && self.scroll_layer_clear,
            use_node_present,
        }
    }

    fn encode_node_present<'pass>(
        &'pass self,
        render_pass: &mut wgpu::RenderPass<'pass>,
        paint_ops: &[crate::layout::layout_tree::PaintOp],
        node_paints: &[crate::layout::layout_tree::NodePaint],
        scaled_instances: &[InstanceInput],
        scaled_tex: &'pass [crate::renderer::ScaledTexturedDraw],
        scaled_input_chrome: &[InstanceInput],
        overlay_inst_start: usize,
        overlay_tex_start: usize,
        overlay_paint_start: usize,
        allow_primitives: bool,
        clear_color: wgpu::Color,
        ui_scale: f32,
        overlay_band_ranges: &[crate::layout::layout_tree::OverlayBandRange],
        capture_text_ready: bool,
        underline_instances: &[InstanceInput],
        prev_underline_instances: &[InstanceInput],
    ) {
        use super::damage::PaintDirtyKind;
        let dummy = self.pipeline.dummy_backdrop();
        let mut pending_page_glyphs = capture_text_ready;
        for dirty in &self.damage_nodes {
            // Overlay draws after page dest-reset. Flush the clipped capture once
            // so neighbor captions are not stamped under each node's scissor.
            if dirty.kind == PaintDirtyKind::Overlay && pending_page_glyphs {
                self.flush_node_present_page_glyphs(render_pass, ui_scale);
                pending_page_glyphs = false;
            }
            let scissor_pad = match dirty.kind {
                PaintDirtyKind::Overlay => 16.0,
                // `dirty.bounds` already includes OVERLAY_RESTORE_PAD_LOGICAL.
                PaintDirtyKind::OverlayRestore => 2.0,
                PaintDirtyKind::Node => 8.0,
            };
            let Some((x, y, w, h)) = super::damage::damage_to_scissor_physical(
                dirty.bounds,
                scissor_pad,
                ui_scale,
                self.config.width,
                self.config.height,
            ) else {
                continue;
            };
            render_pass.set_scissor_rect(x, y, w, h);
            let cull_phys = Some(super::draw_cull::damage_cull_rect_physical(
                dirty.bounds,
                scissor_pad,
                ui_scale,
            ));
            match dirty.kind {
                PaintDirtyKind::OverlayRestore => {
                    let rect = super::draw_cull::damage_cull_rect_physical(
                        dirty.bounds,
                        scissor_pad,
                        ui_scale,
                    );
                    draw_scissor_clear(
                        render_pass,
                        &self.pipeline,
                        &self.device,
                        rect,
                        clear_color,
                    );
                    if allow_primitives {
                        if overlay_paint_start > 0 {
                            draw_paint_ops(
                                render_pass,
                                paint_ops,
                                0,
                                overlay_paint_start,
                                &self.pipeline,
                                &self.instance_buffer,
                                &self.textured_pipeline,
                                &self.textured_instance_buffer,
                                &self.image_textures,
                                scaled_tex,
                                dummy,
                                scaled_instances,
                                cull_phys,
                            );
                        }
                        self.draw_matching_node_paints(
                            render_pass,
                            node_paints,
                            scaled_instances,
                            scaled_tex,
                            scaled_input_chrome,
                            dummy,
                            |p| {
                                !p.overlay
                                    && super::damage::restore_slot_in_hole(p.bounds, dirty.bounds)
                            },
                        );
                    }
                    if allow_primitives {
                        self.draw_node_present_underlines(
                            render_pass,
                            dummy,
                            underline_instances,
                            &[],
                            scaled_instances,
                            overlay_inst_start,
                            cull_phys,
                        );
                    }
                }
                PaintDirtyKind::Overlay => {
                    if allow_primitives {
                        for band in overlay_band_ranges {
                            let op_lo = band.paint_ops.0.max(overlay_paint_start);
                            let op_hi = band.paint_ops.1.max(op_lo);
                            let inst_lo = band.instance.0.max(overlay_inst_start);
                            let inst_hi = band.instance.1.max(inst_lo);
                            let tex_lo = band.textured.0.max(overlay_tex_start);
                            let tex_hi = band.textured.1.max(tex_lo);
                            if op_lo < op_hi {
                                draw_paint_ops(
                                    render_pass,
                                    paint_ops,
                                    op_lo,
                                    op_hi,
                                    &self.pipeline,
                                    &self.instance_buffer,
                                    &self.textured_pipeline,
                                    &self.textured_instance_buffer,
                                    &self.image_textures,
                                    scaled_tex,
                                    dummy,
                                    scaled_instances,
                                    None,
                                );
                            } else if inst_lo < inst_hi {
                                // Previous dirty node may have left glyphon's strip
                                // pipeline bound; indexed SDF draws must rebind first.
                                self.pipeline.bind_sdf(render_pass, true, dummy);
                                render_pass.set_vertex_buffer(
                                    0,
                                    self.pipeline.vertex_buffer.slice(..),
                                );
                                render_pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
                                render_pass.set_index_buffer(
                                    self.pipeline.index_buffer.slice(..),
                                    wgpu::IndexFormat::Uint16,
                                );
                                draw_instance_range(
                                    render_pass,
                                    &self.pipeline,
                                    scaled_instances,
                                    inst_lo as u32,
                                    (inst_hi - inst_lo) as u32,
                                    None,
                                );
                                if tex_lo < tex_hi {
                                    draw_packed_textured_runs(
                                        render_pass,
                                        &self.textured_pipeline,
                                        &self.textured_instance_buffer,
                                        &self.image_textures,
                                        scaled_tex,
                                        tex_lo,
                                        tex_hi,
                                        None,
                                    );
                                }
                            }
                        }
                    }
                    if overlay_band_ranges.iter().any(|b| b.text.0 < b.text.1) {
                        self.overlay_text_renderer.render(render_pass);
                        for r in &self.overlay_band_text_renderers {
                            r.render(render_pass);
                        }
                        super::frame_stats::note_scene_batch(super::frame_stats::SceneKind::Text, 1);
                    }
                }
                PaintDirtyKind::Node => {
                    if allow_primitives {
                        let page = &scaled_instances
                            [..overlay_inst_start.min(scaled_instances.len())];
                        // Load + src-over of the same rounded SDF stacks AA (Default
                        // looks smaller / jittery). Ghost leave cannot erase an opaque
                        // hover fill. Wipe the widget AABB with the card behind it —
                        // never the widget's own fill, never window Clear / parent slot.
                        if let Some((rect, color)) = super::draw_cull::node_present_dest_reset(
                            dirty.key,
                            dirty.bounds,
                            node_paints,
                            page,
                        ) {
                            draw_scissor_clear(
                                render_pass,
                                &self.pipeline,
                                &self.device,
                                rect,
                                wgpu::Color {
                                    r: color[0] as f64,
                                    g: color[1] as f64,
                                    b: color[2] as f64,
                                    a: 1.0,
                                },
                            );
                        }
                        self.draw_matching_node_paints(
                            render_pass,
                            node_paints,
                            scaled_instances,
                            scaled_tex,
                            scaled_input_chrome,
                            dummy,
                            |p| {
                                !p.overlay
                                    && super::damage::paint_dirty_matches_slot(
                                        dirty.key,
                                        dirty.bounds,
                                        p.key,
                                        p.bounds,
                                    )
                            },
                        );
                        self.draw_node_present_underlines(
                            render_pass,
                            dummy,
                            underline_instances,
                            prev_underline_instances,
                            scaled_instances,
                            overlay_inst_start,
                            cull_phys,
                        );
                    }
                }
            }
        }
        if pending_page_glyphs {
            self.flush_node_present_page_glyphs(render_pass, ui_scale);
        }
    }

    /// Draw the prepared capture buffer once, scissored to the page-dirty union.
    ///
    /// Glyphs are already clipped to their owning dirty box in prepare. Drawing the
    /// same buffer under every node scissor would src-over neighbor captions into retain.
    fn flush_node_present_page_glyphs<'pass>(
        &'pass self,
        render_pass: &mut wgpu::RenderPass<'pass>,
        ui_scale: f32,
    ) {
        use super::damage::PaintDirtyKind;
        let mut union: Option<[f32; 4]> = None;
        for dirty in &self.damage_nodes {
            let pad = match dirty.kind {
                PaintDirtyKind::Overlay => continue,
                PaintDirtyKind::OverlayRestore => 2.0,
                // Owner clip already limits glyphs. Do not use the 8px node scissor —
                // that strip overlaps `gap-2` siblings and the sidebar rail.
                PaintDirtyKind::Node => super::draw_cull::NODE_PRESENT_GLYPH_CLIP_PAD,
            };
            let padded = super::draw_cull::padded_xywh(dirty.bounds, pad);
            union = Some(match union {
                Some(prev) => super::damage::union_rects(prev, padded),
                None => padded,
            });
        }
        if let Some(bounds) = union {
            if let Some((x, y, w, h)) = super::damage::damage_to_scissor_physical(
                bounds,
                0.0,
                ui_scale,
                self.config.width,
                self.config.height,
            ) {
                render_pass.set_scissor_rect(x, y, w, h);
            }
        }
        self.capture_text_renderer.render(render_pass);
        super::frame_stats::note_scene_batch(super::frame_stats::SceneKind::Text, 1);
    }

    /// CSS underlines live in `prepare_text_frame`, not `NodePaint`. Full present
    /// draws them before glyphon; node present must too or Link `hover:underline`
    /// never appears. Stale strips are painted with the page fill behind them so
    /// leave does not keep a ghost line (transparent link chrome cannot erase).
    fn draw_node_present_underlines<'pass>(
        &'pass self,
        render_pass: &mut wgpu::RenderPass<'pass>,
        dummy: &'pass wgpu::BindGroup,
        current: &[InstanceInput],
        prev: &[InstanceInput],
        scaled_instances: &[InstanceInput],
        overlay_inst_start: usize,
        cull_phys: Option<[f32; 4]>,
    ) {
        let page = &scaled_instances[..overlay_inst_start.min(scaled_instances.len())];
        let erase_color = cull_phys.and_then(|cull| {
            super::draw_cull::opaque_fill_color_at(
                page,
                cull[0] + cull[2] * 0.5,
                cull[1] + cull[3] * 0.5,
            )
        });
        let owned = |list: &[InstanceInput]| -> Vec<InstanceInput> {
            match cull_phys {
                Some(cull) => list
                    .iter()
                    .copied()
                    .filter(|ul| {
                        let cx = ul.rect_pos[0] + ul.rect_size[0] * 0.5;
                        let cy = ul.rect_pos[1] + ul.rect_size[1] * 0.5;
                        cx >= cull[0]
                            && cy >= cull[1]
                            && cx < cull[0] + cull[2]
                            && cy < cull[1] + cull[3]
                    })
                    .collect(),
                None => list.to_vec(),
            }
        };
        if !prev.is_empty() {
            let erased: Vec<InstanceInput> = owned(prev)
                .into_iter()
                .filter_map(|ul| {
                    let cx = ul.rect_pos[0] + ul.rect_size[0] * 0.5;
                    let cy = ul.rect_pos[1] + ul.rect_size[1] * 0.5;
                    let color = erase_color
                        .or_else(|| super::draw_cull::opaque_fill_color_at(page, cx, cy))?;
                    let mut out = ul;
                    out.rect_pos[0] -= 1.0;
                    out.rect_pos[1] -= 1.0;
                    out.rect_size[0] += 2.0;
                    out.rect_size[1] += 2.0;
                    out.color = color;
                    out.border_width = 0.0;
                    out.border_color = [0.0; 4];
                    out.shadow_blur = 0.0;
                    out.shadow_color = [0.0; 4];
                    Some(out)
                })
                .collect();
            draw_cpu_sdf_instances(
                render_pass,
                &self.pipeline,
                &self.device,
                dummy,
                &erased,
            );
        }
        let now = owned(current);
        draw_cpu_sdf_instances(
            render_pass,
            &self.pipeline,
            &self.device,
            dummy,
            &now,
        );
    }

    fn draw_matching_node_paints<'pass>(
        &'pass self,
        render_pass: &mut wgpu::RenderPass<'pass>,
        node_paints: &[crate::layout::layout_tree::NodePaint],
        scaled_instances: &[InstanceInput],
        scaled_tex: &'pass [crate::renderer::ScaledTexturedDraw],
        scaled_input_chrome: &[InstanceInput],
        dummy: &'pass wgpu::BindGroup,
        pred: impl Fn(&crate::layout::layout_tree::NodePaint) -> bool,
    ) {
        for p in node_paints.iter().filter(|p| pred(p)) {
            // `PaintOp` runs coalesce across siblings/ancestors. Drawing a slot's
            // paint_ops range would submit neighbor quads into this scissor.
            if p.instance.0 < p.instance.1 {
                // Prior slot may have been textured, or the previous dirty node
                // ended on glyphon (TriangleStrip, strip_index_format None).
                // Indexed SDF draws with the leftover Uint16 index buffer panic:
                // "strip index format None must match index buffer format Uint16".
                // Clip-poly SSBO is indexed by `instance_index`. A ranged draw on
                // GLES can feed a relative index and clip the fill to another
                // widget's inset round-clip (Default looks 1px smaller on hover).
                self.pipeline.bind_sdf(render_pass, false, dummy);
                render_pass.set_vertex_buffer(0, self.pipeline.vertex_buffer.slice(..));
                render_pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
                render_pass.set_index_buffer(
                    self.pipeline.index_buffer.slice(..),
                    wgpu::IndexFormat::Uint16,
                );
                draw_instance_range(
                    render_pass,
                    &self.pipeline,
                    scaled_instances,
                    p.instance.0 as u32,
                    (p.instance.1 - p.instance.0) as u32,
                    None,
                );
            }
            if p.textured.0 < p.textured.1 {
                draw_packed_textured_runs(
                    render_pass,
                    &self.textured_pipeline,
                    &self.textured_instance_buffer,
                    &self.image_textures,
                    scaled_tex,
                    p.textured.0,
                    p.textured.1,
                    None,
                );
            }
            if p.chrome.0 < p.chrome.1 && p.chrome.1 <= scaled_input_chrome.len() {
                let chrome_slice = &scaled_input_chrome[p.chrome.0..p.chrome.1];
                if !chrome_slice.is_empty() {
                    render_pass.set_pipeline(&self.pipeline.render_pipeline);
                    render_pass.set_bind_group(0, &self.pipeline.bind_group, &[]);
                    render_pass.set_bind_group(1, dummy, &[]);
                    render_pass.set_vertex_buffer(0, self.pipeline.vertex_buffer.slice(..));
                    let chrome_buffer =
                        self.device
                            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                                label: Some("Node chrome"),
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
            }
        }
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

    /// Drop GPU glyph vertices for a page-stack band. Culled bands used to keep last
    /// frame's glyphon buffer; on scroll-up those stale glyphs sit in the local mask.
    fn clear_page_stack_band_glyphon(&mut self, band_i: usize) {
        if band_i == 0 {
            self.text_renderer
                .prepare(&self.device, &self.queue, &self.config, std::iter::empty());
        } else if let Some(tr) = self.page_stack_band_text_renderers.get_mut(band_i - 1) {
            tr.prepare(&self.device, &self.queue, &self.config, std::iter::empty());
        }
    }

    /// Grow shared-font page stacking pipelines (band 0 = [`Self::text_renderer`]).
    fn ensure_page_stack_band_text_renderers(&mut self, band_count: usize) {
        let extra_needed = band_count.saturating_sub(1);
        while self.page_stack_band_text_renderers.len() < extra_needed {
            let font_ptr = self.text_renderer.font_system_mut() as *mut glyphon::FontSystem;
            self.page_stack_band_text_renderers
                .push(RuiTextRenderer::new_sharing_font_system(
                    &self.device,
                    &self.queue,
                    &self.config,
                    font_ptr,
                ));
        }
    }

    fn trim_all_glyph_atlases(&mut self) {
        self.text_renderer.trim_glyph_atlas();
        self.overlay_text_renderer.trim_glyph_atlas();
        for tr in &mut self.overlay_band_text_renderers {
            tr.trim_glyph_atlas();
        }
        for tr in &mut self.page_stack_band_text_renderers {
            tr.trim_glyph_atlas();
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
        #[cfg(feature = "devtools")]
        {
            let _ = crate::app::devtools::reapply_inspector_overrides(
                &mut self.root_element,
                &mut self.layout_tree,
            );
        }
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
    ) -> Option<crate::core::tree::LayoutStatus> {
        #[cfg(feature = "tracing")]
        let _span = tracing::trace_span!("app.reconcile_subtree", anchor_id).entered();

        use crate::core::tree::LayoutStatus;

        // Fine-grained anchors must resolve deterministically. `hot_reload::template_id_matches` is
        // intentionally fuzzy (path suffix + line) for dev-time patches; using it here can match a
        // *different* node earlier in DFS and replace the wrong subtree — e.g. sibling buttons +
        // `<Show>` vanishing intermittently.
        let want = normalize_anchor_template_id(anchor_id);
        let island_ptr = crate::core::tree::find_in_tree(&self.root_element, |n| {
            n.template_id
                .as_deref()
                .is_some_and(|tid| normalize_anchor_template_id(tid) == want)
        })
        .map(|el| el as *const Element);
        let clip_bounds = island_ptr
            .and_then(|p| crate::core::tree::overflow_clip_bounds_for(&self.root_element, p));
        let Some(old_subtree) =
            find_reactive_subtree_by_anchor_id_mut(&mut self.root_element, &want)
        else {
            // Do **not** report StructureChanged here: callers treat `Some(_)` as "applied".
            // Returning success while dropping `new_fragment` left overlays (menu subs) stuck
            // open-with-no-panel until a signal re-notified the island.
            log::debug!(
                target: "rui::reconcile",
                "reconcile_subtree: anchor not found, fragment dropped ({anchor_id})"
            );
            return None;
        };

        let mut dirty = Vec::new();
        let portal_island = crate::layout::layout_tree::subtree_declares_portal(old_subtree)
            || crate::layout::layout_tree::subtree_declares_portal(&new_fragment);
        let status = Self::reconcile_elements(&mut new_fragment, old_subtree, &mut dirty);
        self.pending_layout_dirty.extend(dirty);
        // Nested portals (Dropdown/Menubar SubContent) are born under already-hoisted
        // parents that use overflow:hidden scroll. Promote them in this same call so the
        // panel escapes clipping before the next paint — waiting for a later layout frame
        // left gallery Share submenu stuck with highlight and no right-hand panel.
        let needs_portal_hoist = element_contains_rui_portal(&new_fragment);
        *old_subtree = new_fragment;

        recompute_scroll_viewport_flags(&mut self.root_element);
        let mut status = status;
        let mut portal_moves = 0usize;
        if needs_portal_hoist {
            portal_moves =
                hoist_portal_subtrees_refreshing_scroll_flags(&mut self.root_element);
            if portal_moves > 0 {
                status = Self::merge_layout_status(status, LayoutStatus::StructureChanged);
            }
        }
        self.layout_status = Self::merge_layout_status(self.layout_status, status);
        if portal_island || portal_moves > 0 {
            // Bubble +N expand / popover hoist: Taffy still rebuilds the portal layer,
            // but GPU FullClear of the gallery is the fullscreen flash.
            self.portal_membership_dirty = true;
        } else if matches!(
            status,
            LayoutStatus::StructureChanged | LayoutStatus::LayoutChanged
        ) {
            // MessageScroller Send / nested For: dirty the clip box only. FullClear
            // of the gallery is what kept Footer on the Send present path.
            if let Some(clip) = clip_bounds {
                super::damage::push_frame_paint_damage(clip);
            } else {
                self.page_geom_dirty = true;
                self.mark_full_redraw();
            }
        }
        Some(status)
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

        let mut status = LayoutStatus::Clean;
        let mut stack: Vec<(*mut Element, *mut Element)> =
            vec![(new_el as *mut Element, old_el as *mut Element)];
        // Keyed / identity pairing moves old children out of the live tree; Boxes keep
        // their addresses stable while the worklist still points at them.
        let mut old_keepalive: Vec<Box<Element>> = Vec::new();

        while let Some((new_ptr, old_ptr)) = stack.pop() {
            // SAFETY: `new_ptr` is in the caller's new tree (or a child we just
            // assigned). `old_ptr` is in the caller's old tree or `old_keepalive`.
            let new_el = unsafe { &mut *new_ptr };
            let old_el = unsafe { &mut *old_ptr };
            status = Self::merge_layout_status(
                status,
                Self::reconcile_pair(new_el, old_el, dirty, &mut stack, &mut old_keepalive),
            );
        }
        status
    }

    fn reconcile_pair(
        new_el: &mut Element,
        old_el: &mut Element,
        dirty: &mut Vec<taffy::prelude::NodeId>,
        stack: &mut Vec<(*mut Element, *mut Element)>,
        old_keepalive: &mut Vec<Box<Element>>,
    ) -> crate::core::tree::LayoutStatus {
        use crate::core::tree::LayoutStatus;

        if new_el.template_id != old_el.template_id {
            return LayoutStatus::StructureChanged;
        }

        new_el.layout_node = old_el.layout_node;
        // Keep dispose hooks alive across root rebuilds that recreate the same template_id.
        if new_el.on_unmount.is_none() {
            new_el.on_unmount = old_el.on_unmount.clone();
        }
        // Keyed overlay：同 template_id 保留 transition mid（见 utils/keyed_overlay）。
        let _ = crate::utils::keyed_overlay::preserve_keyed_motion_on_reconcile(new_el, old_el);
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
                Self::reconcile_keyed_children(new_el, old_el, stack, old_keepalive, dirty),
            );
        } else if new_el.children.len() == old_el.children.len() {
            let n = new_el.children.len();
            for i in (0..n).rev() {
                stack.push((
                    std::ptr::addr_of_mut!(new_el.children[i]),
                    std::ptr::addr_of_mut!(old_el.children[i]),
                ));
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
            // Do not drop the whole child list. Gallery's live root is
            // `[chrome, toast_host, __rui_portal_layer]` while `app_builder()`
            // only emits `[chrome, toast_host]`. Wiping here replaces the
            // filled `switch_reactive` island with an empty placeholder.
            status = Self::merge_layout_status(
                status,
                Self::reconcile_children_by_identity(new_el, old_el, stack, old_keepalive),
            );
        }
        status
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
            // Paint-only (e.g. bind_text `"10"`→`"11"`): this node's box only.
            // Do not `mark_full_redraw` — that forces a full Clear every counter tick.
            if bounds[2] > 0.0 && bounds[3] > 0.0 {
                super::damage::push_frame_paint_damage(bounds);
            }
        }
        status
    }

    /// Inspector live style edit (B1 P1): patch one node by child-index path.
    #[cfg(feature = "devtools")]
    pub(super) fn patch_devtools_element_at_path(
        &mut self,
        path: &[usize],
        patch: crate::app::devtools::StylePatch,
    ) -> bool {
        use crate::app::devtools::{apply_style_patch, find_element_by_path_mut};
        let Some(el) = find_element_by_path_mut(&mut self.root_element, path) else {
            return false;
        };
        let bounds = el.layout_bounds;
        let status = apply_style_patch(el, &patch);
        if let Some(n) = el.layout_node {
            self.pending_layout_dirty.push(n);
            self.layout_tree.update_taffy_node_style(el);
        }
        self.layout_status = Self::merge_layout_status(self.layout_status, status);
        if matches!(status, crate::core::tree::LayoutStatus::StyleChanged)
            && bounds[2] > 0.0
            && bounds[3] > 0.0
        {
            super::damage::push_frame_paint_damage(bounds);
        } else if matches!(status, crate::core::tree::LayoutStatus::LayoutChanged) {
            self.mark_full_redraw();
        }
        true
    }

    /// Inspector CSSOM text apply (B1 P2). Returns canonical dump after apply.
    #[cfg(feature = "devtools")]
    pub(super) fn patch_devtools_cssom_at_path(
        &mut self,
        path: &[usize],
        css: &str,
    ) -> Option<String> {
        use crate::app::devtools::{
            apply_cssom_text, cssom_text_from_element, find_element_by_path_mut,
        };
        let Some(el) = find_element_by_path_mut(&mut self.root_element, path) else {
            return None;
        };
        let bounds = el.layout_bounds;
        let status = apply_cssom_text(el, css);
        let dump = cssom_text_from_element(el);
        if let Some(n) = el.layout_node {
            self.pending_layout_dirty.push(n);
            self.layout_tree.update_taffy_node_style(el);
        }
        self.layout_status = Self::merge_layout_status(self.layout_status, status);
        if matches!(status, crate::core::tree::LayoutStatus::StyleChanged)
            && bounds[2] > 0.0
            && bounds[3] > 0.0
        {
            super::damage::push_frame_paint_damage(bounds);
        } else if matches!(status, crate::core::tree::LayoutStatus::LayoutChanged) {
            self.mark_full_redraw();
        }
        Some(dump)
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
        match crate::core::tree::find_path_in_tree(root, |el| {
            el.template_id.as_deref() == Some(template_id)
        }) {
            Some(path) => {
                *out = path;
                true
            }
            None => false,
        }
    }

    fn children_all_keyed(children: &[Element]) -> bool {
        // `Iterator::all` is vacuously true on an empty slice; keyed reconciliation is only defined
        // for non-empty child lists (empty vs empty uses the positional same-length branch).
        !children.is_empty() && children.iter().all(|c| c.template_id.is_some())
    }

    fn reconcile_keyed_children(
        new_el: &mut Element,
        old_el: &mut Element,
        stack: &mut Vec<(*mut Element, *mut Element)>,
        old_keepalive: &mut Vec<Box<Element>>,
        dirty: &mut Vec<taffy::prelude::NodeId>,
    ) -> crate::core::tree::LayoutStatus {
        use crate::core::tree::LayoutStatus;

        let old_ids: Vec<String> = old_el
            .children
            .iter()
            .filter_map(|c| c.template_id.clone())
            .collect();
        let new_ids: Vec<String> = new_el
            .children
            .iter()
            .filter_map(|c| c.template_id.clone())
            .collect();

        let mut status = LayoutStatus::Clean;
        let mut old_by_id: std::collections::HashMap<String, Element> =
            std::collections::HashMap::new();
        for old_child in std::mem::take(&mut old_el.children) {
            if let Some(id) = old_child.template_id.clone() {
                old_by_id.insert(id, old_child);
            }
        }

        let mut next_children = Vec::with_capacity(new_el.children.len());
        let mut matched_old: Vec<Option<Element>> = Vec::with_capacity(new_el.children.len());
        for new_child in std::mem::take(&mut new_el.children) {
            let Some(id) = new_child.template_id.clone() else {
                status = LayoutStatus::StructureChanged;
                next_children.push(new_child);
                matched_old.push(None);
                continue;
            };
            if let Some(old_child) = old_by_id.remove(&id) {
                matched_old.push(Some(old_child));
            } else {
                status = LayoutStatus::StructureChanged;
                matched_old.push(None);
            }
            next_children.push(new_child);
        }

        if !old_by_id.is_empty() {
            status = LayoutStatus::StructureChanged;
        } else if status == LayoutStatus::Clean && old_ids != new_ids {
            // Same keys, new order: keep the Taffy nodes (not StructureChanged) but the
            // flex pack must follow `element.children`. Clean would skip `set_children`
            // on the fast layout path and paint the old rows (Gallery Inbox: 松手后
            // Alpha/Beta 仍按旧 layout_bounds).
            status = LayoutStatus::LayoutChanged;
            if let Some(n) = new_el.layout_node {
                dirty.push(n);
            }
        }
        new_el.children = next_children;
        for i in (0..new_el.children.len()).rev() {
            if let Some(old_child) = matched_old[i].take() {
                old_keepalive.push(Box::new(old_child));
                let old_ptr = old_keepalive.last_mut().unwrap().as_mut() as *mut Element;
                stack.push((std::ptr::addr_of_mut!(new_el.children[i]), old_ptr));
            }
        }
        status
    }

    /// Length-mismatch fallback: pair children that share `template_id` (so a
    /// `reactive_view` placeholder can still promote a mounted island) and keep
    /// unmatched new nodes. Unkeyed children are not positionally paired — that
    /// would attach the wrong subtree when a list grows or shrinks.
    fn reconcile_children_by_identity(
        new_el: &mut Element,
        old_el: &mut Element,
        stack: &mut Vec<(*mut Element, *mut Element)>,
        old_keepalive: &mut Vec<Box<Element>>,
    ) -> crate::core::tree::LayoutStatus {
        use crate::core::tree::LayoutStatus;

        let status = LayoutStatus::StructureChanged;
        let mut old_by_id: std::collections::HashMap<String, Element> =
            std::collections::HashMap::new();
        for old_child in std::mem::take(&mut old_el.children) {
            if let Some(id) = old_child.template_id.clone() {
                old_by_id.insert(id, old_child);
            }
        }

        let mut next_children = Vec::with_capacity(new_el.children.len());
        let mut matched_old: Vec<Option<Element>> = Vec::with_capacity(new_el.children.len());
        for new_child in std::mem::take(&mut new_el.children) {
            let matched = new_child
                .template_id
                .as_ref()
                .and_then(|id| old_by_id.remove(id));
            matched_old.push(matched);
            next_children.push(new_child);
        }
        new_el.children = next_children;
        for i in (0..new_el.children.len()).rev() {
            if let Some(old_child) = matched_old[i].take() {
                old_keepalive.push(Box::new(old_child));
                let old_ptr = old_keepalive.last_mut().unwrap().as_mut() as *mut Element;
                stack.push((std::ptr::addr_of_mut!(new_el.children[i]), old_ptr));
            }
        }
        status
    }

    pub(super) fn update_scroll_offset_in_place(&mut self, target_id: &str, offset: [f32; 2]) {
        crate::core::tree::walk_elements_mut(&mut self.root_element, |el| {
            if let Some(fid) = el.focus_id.as_deref() {
                if fid.strip_prefix(SCROLL_VIEWPORT_FOCUS_PREFIX) == Some(target_id) {
                    el.scroll_offset = offset;
                }
            }
        });
    }

    #[cfg(debug_assertions)]
    pub(super) fn inject_hot_reload_partial(
        &mut self,
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
            return true;
        }
        // Always persist overrides, even when the patched `ui!` is not mounted
        // (Gallery `switch_reactive` only builds the current section). Entering
        // that page later replays via `apply_hot_reload_overrides_to_fragment`.
        let incoming_ids: Vec<String> = templates.iter().map(|t| t.id.clone()).collect();
        let live = crate::app::hot_reload::has_any_template_id(&self.root_element, &incoming_ids);
        hot_reload_inject::apply_templates_to_tree(
            &mut self.root_element,
            &self.hot_reload_overrides,
        );
        if live {
            self.layout_status = Self::merge_layout_status(
                self.layout_status,
                crate::core::tree::LayoutStatus::LayoutChanged,
            );
            self.mark_full_redraw();
        }
        true
    }

    #[cfg(debug_assertions)]
    pub(super) fn apply_hot_reload_overrides_to_fragment(&self, fragment: &mut Element) {
        if self.hot_reload_overrides.is_empty() {
            return;
        }
        hot_reload_inject::apply_templates_to_tree(fragment, &self.hot_reload_overrides);
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
        // / redraw causes visible flicker; use `recover_surface` after a Lost acquire instead.
        if dims_changed {
            self.surface.configure(&self.device, &self.config);
            super::window_chrome::apply_created_window(
                &self.window,
                &self.chrome,
                self.composition_swapchain,
            );
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
        super::window_chrome::apply_created_window(
            &self.window,
            &self.chrome,
            self.composition_swapchain,
        );
        self.mark_full_redraw();
    }

    pub(super) fn render(
        &mut self,
        mouse_pos: [f32; 2],
        scrollbar_update: ScrollbarUpdateMode,
        active_drag_rest: Option<&str>,
        text_force_fast_mode: bool,
        // 无拇指拖拽时：overlay 淡入淡出会走 Full 预更新，但 flex 树尺寸不变，可跳过 compute_layout，
        // collect draws the track/thumb with the scroll signal (same path as ViewportsOnly).
        scrollbar_skip_flex_layout: bool,
        // Smooth-scroll / overlay-fade: skip chrome walks that cannot change (checked, focus ring,
        // caret paint). Class `:hover` **enter** is frozen here — content sliding onto a still
        // cursor must not light up every `transition-transform` tile (gallery layout WP jank +
        // sidebar hover flash). Already-hovered nodes still **leave** when the pointer is no
        // longer on their paint AABB. Active CSS tweens still advance via `tick_active_css_transitions`.
        scroll_paint_only: bool,
    ) -> Result<(), crate::renderer::wgpu_compat::SurfaceAcquireError> {
        // Collect / Taffy / GPU do not map tree depth onto the OS stack.
        // Construction (`app_builder`, `flush_pending` islands) grows at those sites.
        self.render_inner(
            mouse_pos,
            scrollbar_update,
            active_drag_rest,
            text_force_fast_mode,
            scrollbar_skip_flex_layout,
            scroll_paint_only,
        )
    }

    #[inline(never)]
    fn render_inner(
        &mut self,
        mouse_pos: [f32; 2],
        scrollbar_update: ScrollbarUpdateMode,
        active_drag_rest: Option<&str>,
        text_force_fast_mode: bool,
        scrollbar_skip_flex_layout: bool,
        scroll_paint_only: bool,
    ) -> Result<(), crate::renderer::wgpu_compat::SurfaceAcquireError> {
        // Scroll/drag fast path for prepare perf. Layout fit skip applies during live
        // window resize and sidebar rail-width tweens — scrolling must still compute
        // fit so bidi labels do not overflow.
        // Settle is deferred one frame after `inner_size` matches the last Resized event so the
        // final drag frame stays fluid; that next frame must run with fast=false + LayoutChanged.
        if crate::components::sidebar::take_sidebar_rail_text_settle() {
            self.needs_text_settle = true;
        }
        let rail_anim = crate::components::sidebar::sidebar_rail_width_anim_active();
        let settling = self.needs_text_settle && self.last_resized_event.is_none() && !rail_anim;
        if settling {
            self.needs_text_settle = false;
            self.layout_status = crate::core::tree::LayoutStatus::LayoutChanged;
            // Geometry may match the last fast frame; force prepare so Exact/Bounds apply fit wrap.
            self.last_text_nodes_cache.clear();
        }
        // Sidebar width tweens reflow the app shell every frame — same wrap churn as live
        // window resize. Fast-path FIT_CACHE for the 200ms tween, then settle once.
        let layout_fast = self.last_resized_event.is_some() || rail_anim;
        crate::text::set_layout_text_fast_mode(layout_fast);
        let text_fast = text_force_fast_mode || layout_fast;
        let _text_backend = self.text_backend;
        let render_begin = Instant::now();
        let idle_ms = self.last_render_at.elapsed().as_secs_f32() * 1000.0;
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

        // Pull live scroll offsets **before** class-variant / button hover sync so `:hover` and
        // hit-adjacent chrome see the same `Element.scroll_offset` as this frame's paint. Doing
        // this only in the later scrollbar prepass left chrome one frame behind smooth scroll —
        // content slid under a still cursor with stale offset, thrashing hover + sidebar flash.
        if self.root_element.contains_scroll_viewport {
            sync_viewport_scroll_offsets_from_signals(&mut self.root_element);
        }

        #[cfg(feature = "devtools")]
        {
            // Apply queued CSSOM edits before chrome sync; committed map is reasserted after.
            for edit in crate::app::devtools::take_pending_inspector_edits() {
                crate::app::devtools::commit_inspector_cssom(&edit.path, edit.css.clone());
                if self
                    .patch_devtools_cssom_at_path(&edit.path, &edit.css)
                    .is_some()
                {
                    crate::app::devtools::sync_inspector_applied_cssom(&edit.path, &edit.css);
                }
            }
        }

        let mut sync_impact = crate::core::tree::LayoutStatus::Clean;
        let mut sync_collect = crate::core::tree::ReactiveSyncCollect::default();
        let (viewport_w, viewport_h) = self.logical_size();
        crate::utils::modal_motion::set_modal_viewport_size(viewport_w, viewport_h);
        crate::components::carousel::begin_carousel_paint_frame();
        crate::core::tree::sync_reactive_elements_subtree_ex(
            &mut self.root_element,
            &mut self.layout_tree,
            &mut sync_impact,
            viewport_w,
            viewport_h,
            [0.0, 0.0],
            false,
            &mut sync_collect,
        );
        self.pending_layout_dirty
            .extend(sync_collect.layout_dirty.drain(..));
        // Snapshot before chrome merges into `sync_impact` (button/slider/… may also LayoutChanged).
        let portal_only_reactive_layout =
            matches!(sync_impact, crate::core::tree::LayoutStatus::LayoutChanged)
                && sync_collect.portal_layout_changed
                && !sync_collect.page_layout_changed;
        // Button hover chrome: always sync — scroll frames update layout before this pass,
        // and pointer hover may have left on the prior post-layout reconcile while
        // `scroll_paint_only` was true (signal already false; skipping sync stuck hover
        // until scroll settled).
        crate::components::button::sync_button_hover_paint_subtree(
            &mut self.root_element,
            &mut self.layout_tree,
            &mut sync_impact,
            viewport_w,
            viewport_h,
            [0.0, 0.0],
        );
        if !scroll_paint_only {
            crate::components::command::sync_command_item_paint_subtree(
                &mut self.root_element,
                &mut self.layout_tree,
                &mut sync_impact,
            );
            crate::components::context_menu::sync_context_menu_item_paint_subtree(
                &mut self.root_element,
                &mut self.layout_tree,
                &mut sync_impact,
            );
            crate::components::dropdown_menu::sync_dropdown_menu_item_paint_subtree(
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
            crate::components::sidebar::sync_sidebar_rail_subtree(
                &mut self.root_element,
                &mut self.layout_tree,
                &mut sync_impact,
            );
            crate::components::sidebar::sync_sidebar_menu_button_paint_subtree(
                &mut self.root_element,
                &mut self.layout_tree,
                &mut sync_impact,
            );
            crate::components::bubble::sync_bubble_reactions_subtree(
                &mut self.root_element,
                &mut self.layout_tree,
                &mut sync_impact,
                viewport_w,
            );
            crate::components::avatar::sync_avatar_group_subtree(
                &mut self.root_element,
                &mut self.layout_tree,
                &mut sync_impact,
                viewport_w,
            );
            // After reactive/toggle pressed shadows so keyboard ring can override without fighting.
            crate::style::sync_focus_ring_subtree(
                &mut self.root_element,
                &mut self.layout_tree,
                &mut sync_impact,
            );
        }
        {
            let focused =
                crate::core::reactivity::use_signal("__rui_button_focus_id", String::new).peek();
            let dark = crate::style::is_dark_theme();
            // Class-variant `:hover` uses last-frame layout_bounds. After a **page**
            // StructureChanged (or while the Inspector dock is about to insert/remove),
            // those bounds still cover the pre-rebuild geometry — dozens of nodes falsely
            // report hovered and flash together.
            // Overlay-only StructureChanged (bubble +N / popover hoist) must NOT do this:
            // a sentinel mouse forces every card to leave, unions paint damage to the
            // viewport, and FullClears even after skip_full_clear_for_portal_structure.
            // Hoist runs after this pass — mark membership now so +N Show-mount is not
            // treated as a page rebuild.
            if matches!(
                pre_chrome_layout_status,
                crate::core::tree::LayoutStatus::StructureChanged
            ) && (self.portal_membership_dirty
                || element_contains_rui_portal(&self.root_element))
            {
                self.portal_membership_dirty = true;
            }
            let variant_mouse = {
                let structure_dirty = super::damage::page_structure_rebuild(
                    matches!(
                        pre_chrome_layout_status,
                        crate::core::tree::LayoutStatus::StructureChanged
                    ),
                    self.portal_membership_dirty,
                    self.page_geom_dirty,
                );
                // Only suppress class-hover while the dock is open *and* waiting for sync.
                // `is_enabled()` alone (feature on, panel closed) must not pin the mouse to a
                // sentinel — that interacted badly with layout-page CSS transitions.
                #[cfg(feature = "devtools")]
                let panel_pending = crate::app::devtools::DevToolsState::is_enabled()
                    && crate::app::devtools::DevToolsState::is_open()
                    && crate::app::devtools::DevToolsState::generation()
                        != crate::app::devtools::DevToolsState::last_sync_gen();
                #[cfg(not(feature = "devtools"))]
                let panel_pending = false;
                if structure_dirty || panel_pending {
                    [-1.0e9_f32, -1.0e9_f32]
                } else {
                    mouse_pos
                }
            };
            if scroll_paint_only {
                crate::style::tick_active_css_transitions_subtree(
                    &mut self.root_element,
                    variant_mouse,
                    focused.as_str(),
                    dark,
                    viewport_w,
                    viewport_h,
                    [0.0, 0.0],
                    &mut self.layout_tree,
                    &mut sync_impact,
                );
            } else {
                crate::style::sync_class_variants_subtree(
                    &mut self.root_element,
                    variant_mouse,
                    focused.as_str(),
                    dark,
                    viewport_w,
                    viewport_h,
                    &mut self.layout_tree,
                    &mut sync_impact,
                );
            }
        }
        // Input/label caret+selection after class variants so chrome resets cannot wipe
        // `text_style.input_caret` before collect. Offset-only scroll frames keep last paint.
        if !scroll_paint_only {
            crate::components::internal::text_edit::sync_text_edit_paint_subtree_in_viewport(
                &mut self.root_element,
                &mut self.layout_tree,
                &mut sync_impact,
                viewport_w,
                viewport_h,
                [0.0, 0.0],
            );
            crate::components::label::sync_label_selection_paint_subtree(
                &mut self.root_element,
                &mut sync_impact,
            );
        }

        #[cfg(feature = "devtools")]
        {
            // Chrome sync (button hover / class variants) restores authored snapshots and
            // would wipe live Inspector Set/Apply edits — reassert committed overrides.
            let ovr = crate::app::devtools::reapply_inspector_overrides(
                &mut self.root_element,
                &mut self.layout_tree,
            );
            sync_impact = Self::merge_layout_status(sync_impact, ovr);
        }

        use crate::core::tree::LayoutStatus;
        // Merge local chrome dirty rects (button hover, etc.) before deciding Clear vs Load.
        self.damage_nodes.extend(super::damage::take_frame_paint_nodes());
        // Hoist before skip_full_clear: bubble +N Show-mounts the portal in-tree, then this
        // pass lifts it. Membership must be set before StructureChanged decides FullClear,
        // otherwise overlay-only frames still wipe the gallery.
        let portal_moves = hoist_portal_subtrees_refreshing_scroll_flags(&mut self.root_element);
        if portal_moves > 0 {
            self.portal_membership_dirty = true;
        }
        // WP-2.2: paint-only StyleChanged must NOT expand to the full window (that forced
        // FullClear on every hover spring tick / bind_text). Structure / *page* layout geometry
        // changes move pixels under the main tree → PartialLoad+scissor is unsafe (FullClear).
        // Portal-local LayoutChanged (toast pile height) and overlay portal membership
        // (bubble +N expand, popover) only dirty overlay nodes — do not Clear the whole gallery.
        let page_geom = self.page_geom_dirty || sync_collect.page_layout_changed;
        let portal_skip = portal_only_reactive_layout
            || super::damage::skip_full_clear_for_portal_structure(
                self.portal_membership_dirty,
                page_geom,
            );
        // One gate for sync_impact **and** pre_chrome. A later pre_chrome
        // `mark_full_redraw` used to undo named-node PartialLoad (Input + spin).
        if super::damage::force_full_redraw_after_chrome(
            pre_chrome_layout_status,
            sync_impact,
            page_geom,
            portal_skip,
            !self.damage_nodes.is_empty(),
        ) {
            self.mark_full_redraw();
        }
        let layout_for_build_base =
            Self::merge_layout_status(pre_chrome_layout_status, sync_impact);
        // `layout_for_build` will only be merged once for `devtools`; no mut is required for non-devtools builds.
        #[cfg(feature = "devtools")]
        let mut layout_for_build = layout_for_build_base;
        #[cfg(not(feature = "devtools"))]
        let layout_for_build = layout_for_build_base;

        crate::components::drawer::sync_drawer_background_scale(&mut self.root_element);

        // Modal overlay/panel live on the hoisted portal tree. Flush seeds enter/exit
        // keyframes; CSS then ticks **once** on those nodes. Pre-hoist class-variant
        // tick skips them via `modal_shell_defer_css_tick` so we do not double-step
        // (or step a stale un-hoisted copy).
        let modal_needs_frame =
            crate::utils::modal_motion::flush_modal_shell_motion_subtree(
                &mut self.root_element,
            );
        let modal_css_ticking = crate::utils::modal_motion::tick_hoisted_modal_css(
            &mut self.root_element,
        );
        // Floating Expand 依赖 layout_bounds 测高；Fade 也可在 layout 后 seed。
        // Do not flush immediately after hoist (the floating layer height is still 0 at this time, and reactive reconstruction will flush out mid).
        if modal_needs_frame || modal_css_ticking {
            self.window.request_redraw();
        }

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
                (LayoutStatus::StructureChanged, _) | (_, LayoutStatus::StructureChanged)
                    if !(self.page_geom_dirty || sync_collect.page_layout_changed) =>
                {
                    // Portal hoist *or* nested For (Send): do not Full-sync every
                    // scrollbar. Touching the sidebar track busts Footer collect-skip.
                    scrollbar_update
                }
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
            let scroll_changed =
                self.has_presented_frame && scroll_fp != self.last_scroll_offset_fp;
            let cur_offsets = Self::scroll_offsets_map(&self.root_element);
            self.scroll_layer_clear = false;
            self.scroll_text_clip = None;
            if scroll_changed && !self.portal_membership_dirty {
                // Pixels move inside the scroller: Clear that viewport layer, keep the rest
                // of the retain buffer (sidebar / chrome). Whole-window FullClear is wrong here.
                let prev_offsets = self.last_scroll_offsets.clone();
                if !self.union_scrolled_viewport_damage(&prev_offsets, &cur_offsets) {
                    self.mark_full_redraw();
                } else {
                    self.scroll_layer_clear = true;
                }
            }
            self.last_scroll_offset_fp = scroll_fp;
            self.last_scroll_offsets = cur_offsets;
            // Highlight geometry is restamped during layout collect from paint_visual_bounds.
            // Do **not** refresh_highlight here: it raced collect (stale abs−scroll vs paint
            // rect) and flickered the overlay during / after smooth scroll + pick hover.
        }
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

        #[cfg(feature = "devtools")]
        {
            // App tree StructureChanged (route / reactive_view) does not bump Inspector GEN by
            // itself — without this the Elements panel keeps a stale snapshot after page switches.
            // Skip during scroll_paint_only so VirtualList window moves do not rebuild the dock
            // every frame.
            if !scroll_paint_only
                && crate::app::devtools::DevToolsState::is_open()
                && super::damage::page_structure_rebuild(
                    matches!(
                        pre_chrome_layout_status,
                        crate::core::tree::LayoutStatus::StructureChanged
                    ),
                    self.portal_membership_dirty,
                    self.page_geom_dirty,
                )
            {
                crate::app::devtools::DevToolsState::bump();
                // Stale pick/tree hover paths refer to the previous page.
                crate::app::devtools::DevToolsState::clear_hover_highlight();
            }
            if crate::app::devtools::sync_overlay(&mut self.root_element) {
                layout_for_build = Self::merge_layout_status(
                    layout_for_build,
                    crate::core::tree::LayoutStatus::StructureChanged,
                );
                self.mark_full_redraw();
                // Panel rebuild replaces every `on_hover` Rc. Soft-invalidate so the next
                // post-render hover pass re-asserts by focus_id without leave→spring(0) thrash.
                crate::app::tree_hit::invalidate_hover_handlers();
            }
            let _ = crate::app::devtools::sync_box_model_visuals(&mut self.root_element);
            let _ = crate::app::devtools::sync_tree_row_hovers(&mut self.root_element);
        }

        let (logical_w, logical_h) = self.logical_size();
        // LayoutChanged: mark_dirty then compute from the **app root**. Do not use
        // `pick_subtree_compute_root` — Taffy treats the compute node as a layout root and
        // zeros its `location`, which collapses absolute positions for that whole branch
        // (gallery `<Show>` / `bind_text` LayoutChanged: card title overlapping buttons).
        // Root compute still reuses clean sibling caches via mark_dirty.
        // Cold start / portal hoist still need a full Taffy rebuild (NodeId recycle;
        // hoist rewrites layer.children — skipping rebuild desyncs overlay_split and
        // drops page glyphs: gallery bubbles keep chrome, lose text).
        // Local `<Show>` structure changes attach unbuilt nodes on the fast path.
        let use_full_taffy_rebuild = self.layout_tree.root_node.is_none() || portal_moves > 0;
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
                // Do **not** hit this when `reactive_sync` returned LayoutChanged — those nodes
                // are queued into `pending_layout_dirty` (portal toast pile, slider, …).
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
        // `StyleChanged` from per-frame chrome (color/shading, etc.) can be used the same as `Clean` without changing the flex geometry
        // scroll-only; `LayoutChanged` (such as border width, underline switch) must still run `compute_layout`.
        // Full + overlay 淡入淡出 / 平滑滚：预更新已写滚动条样式，flex 树在 Clean 下与上一帧同构。
        //
        // Mid-scroll exception: after entering a huge page (gallery layout WP), glyphon_fit spill
        // keeps promoting `LayoutChanged` for several frames. Forcing full `compute_layout` on
        // every smooth-scroll frame janks the whole window (sidebar + scrollbar flash together).
        // Defer that remeasure until scroll settles (`take_fit_needs_relayout` re-arms below).
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
                ))
            || (scroll_paint_only
                && !matches!(
                    layout_for_build,
                    crate::core::tree::LayoutStatus::StructureChanged
                )
                && !matches!(
                    pre_chrome_layout_status,
                    crate::core::tree::LayoutStatus::StructureChanged
                ));
        // When the scrollbar prepass is `ViewportsOnly`, track/thumb Taffy styles are not updated.
        // Chrome may set `StyleChanged` while scrollbars use overlay collect — collect with signal geometry,
        // Avoid just hovering the full color Taffy every frame.
        // Keep overlay collect while dragging a thumb: idle frames paint via overlay, and
        // switching to Taffy on press used to jump the thumb by the track border (~1px down-right).
        let scrollbar_overlay_collect = scrollbar_update_resolved
            == ScrollbarUpdateMode::ViewportsOnly
            || (scrollbar_skip_flex_layout
                && scrollbar_update_resolved == ScrollbarUpdateMode::Full
                && scroll_only)
            || active_drag_rest.is_some();
        crate::style::set_gradient_raster_dpi(self.ui_scale());
        let allow_collect_skip = !scroll_only && !use_full_taffy_rebuild;
        self.layout_tree
            .set_collect_skip_allowed(allow_collect_skip);
        let mut layout_passes = 0u32;
        let mut layout_prof_sum = LayoutProfileMs::default();
        let mut acc_layout_prof = |p: &LayoutProfileMs| {
            layout_passes += 1;
            layout_prof_sum.rebuild_ms += p.rebuild_ms;
            layout_prof_sum.setup_ms += p.setup_ms;
            layout_prof_sum.taffy_compute_ms += p.taffy_compute_ms;
            layout_prof_sum.collect_ms += p.collect_ms;
        };
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
        acc_layout_prof(&render_result.prof);
        if !scroll_paint_only {
            let mut overflow_impact = crate::core::tree::LayoutStatus::Clean;
            crate::components::bubble::sync_bubble_reactions_subtree(
                &mut self.root_element,
                &mut self.layout_tree,
                &mut overflow_impact,
                logical_w,
            );
            crate::components::avatar::sync_avatar_group_subtree(
                &mut self.root_element,
                &mut self.layout_tree,
                &mut overflow_impact,
                logical_w,
            );
            if matches!(
                overflow_impact,
                crate::core::tree::LayoutStatus::StructureChanged
                    | crate::core::tree::LayoutStatus::LayoutChanged
            ) {
                render_result = self.layout_tree.build_and_layout_fast_ex(
                    &mut self.root_element,
                    logical_w,
                    logical_h,
                    mouse_pos,
                    false,
                    scrollbar_overlay_collect,
                    None,
                );
                acc_layout_prof(&render_result.prof);
            }
        }
        // 浮层：首帧用占位坐标，布局后按触发器/视口纠正。滚动时只改 left/top —
        // Only dirty floating layer nodes + fast compute, prohibiting full reconstruction of Taffy every frame (the whole page scrolls and freezes after opening DatePicker).
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
            acc_layout_prof(&render_result.prof);
        }
        // Floating motion: Expand can read height measurement only after seed/tick of layout (and placement sync).
        let floating_needs_frame =
            crate::utils::floating_motion::flush_floating_motion_subtree(
                &mut self.root_element,
            );
        let floating_css_ticking =
            crate::utils::floating_motion::tick_hoisted_floating_css(
                &mut self.root_element,
            );
        if floating_needs_frame || floating_css_ticking {
            // Expand 写入的 max-height keyframe 需再跑一趟 Taffy，本帧才能裁剪高度。
            crate::utils::floating_motion::mark_floating_panels_layout_dirty(
                &self.root_element,
                &mut self.layout_tree,
            );
            render_result = self.layout_tree.build_and_layout_fast_ex(
                &mut self.root_element,
                logical_w,
                logical_h,
                mouse_pos,
                false,
                scrollbar_overlay_collect,
                None,
            );
            acc_layout_prof(&render_result.prof);
            self.window.request_redraw();
        }
        drop(acc_layout_prof);
        let layout_done = Instant::now();

        let overlay_split = render_result.overlay_split;
        let ui_scale_now = self.ui_scale();
        scale_instances_in_place(&mut render_result.instances, ui_scale_now);
        scale_instances_in_place(&mut render_result.input_chrome_instances, ui_scale_now);
        scale_clip_polys_in_place(&mut render_result.clip_polys, ui_scale_now);
        while render_result.clip_polys.len() < render_result.instances.len() {
            render_result
                .clip_polys
                .push(crate::style::GpuClipPoly::NONE);
        }
        let scaled_instances = std::mem::take(&mut render_result.instances);
        let scaled_input_chrome = std::mem::take(&mut render_result.input_chrome_instances);
        self.num_instances = scaled_instances.len() as u32;
        self.text_nodes = render_result.text_nodes;
        if self
            .text_nodes
            .iter()
            .any(|n| n.glyph_fx.as_deref().is_some_and(|s| !s.is_empty()))
        {
            self.window.request_redraw();
        }
        let overlay_inst_start = overlay_split.instance_start.min(scaled_instances.len());
        let overlay_text_start = overlay_split.text_start.min(self.text_nodes.len());
        let overlay_tex_start = overlay_split
            .textured_start
            .min(render_result.textured_draws.len());
        let overlay_chrome_start = overlay_split.chrome_start.min(scaled_input_chrome.len());
        let overlay_paint_start = overlay_split
            .paint_op_start
            .min(render_result.paint_ops.len());
        // Overlay suffix in the draw list is not a page rebuild. FullClear here flashed
        // the gallery on bubble +N expand and every hover:scale tick while the panel is
        // open. `apply_portal_overlay_damage` scissors to the hoisted box instead.
        {
            self.render_prof_layout_rebuild_ms += layout_prof_sum.rebuild_ms;
            self.render_prof_layout_setup_ms += layout_prof_sum.setup_ms;
            self.render_prof_layout_taffy_ms += layout_prof_sum.taffy_compute_ms;
            self.render_prof_layout_collect_ms += layout_prof_sum.collect_ms;
        }

        // Export collected metrics and hover state for window_events.rs to avoid redundant traversals.
        self.last_render_scroll_metrics = render_result.scroll_metrics.clone();
        self.last_hover_cursor = render_result.hover_cursor.clone();

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
        self.pipeline
            .ensure_clip_polys(&self.device, &self.queue, &render_result.clip_polys);
        self.layout_status = crate::core::tree::LayoutStatus::Clean;
        if crate::core::tree_stats::tree_stats_enabled()
            && !Self::is_bootstrap_root(&self.root_element)
        {
            static TREE_STATS_ONCE: std::sync::Once = std::sync::Once::new();
            TREE_STATS_ONCE.call_once(|| {
                crate::core::tree_stats::emit_tree_stats(&self.root_element, &self.layout_tree);
            });
        }
        if crate::core::registry_stats::registry_stats_enabled()
            && !Self::is_bootstrap_root(&self.root_element)
        {
            static REGISTRY_STATS_ONCE: std::sync::Once = std::sync::Once::new();
            REGISTRY_STATS_ONCE.call_once(|| {
                crate::core::registry_stats::emit_registry_stats();
            });
        }
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
        let upload_done = Instant::now();

        let ui_scale = self.ui_scale();
        self.image_textures.begin_frame();

        // Pin every texture referenced by this frame's draw list before uploads/eviction.
        for draw in &render_result.textured_draws {
            self.image_textures.touch(&draw.texture_key);
        }

        // Rehydrate CPU-cached textures after TexturePool eviction / first paint.
        // Uploads happen immediately in take_pending below (same frame).
        let _ = crate::style::flush_gradient_textures(|key| self.image_textures.get(key).is_some());
        let _ = crate::primitives::path::flush_path_textures(|key| {
            self.image_textures.get(key).is_some()
        });
        let _ = crate::media::svg_raster::flush_svg_textures(|key| {
            self.image_textures.get(key).is_some()
        });
        let _ =
            super::pending_images::flush_src_textures(|key| self.image_textures.get(key).is_some());
        let _ = crate::components::color_picker::flush_color_picker_textures(|key| {
            self.image_textures.get(key).is_some()
        });
        let _ = crate::utils::text_texture::flush_text_xform_textures(|key| {
            self.image_textures.get(key).is_some()
        });

        for (id, rgba, w, h) in super::pending_images::take_pending() {
            // Bake/MSDF text: Linear filter without mip chain. Mips + minify washed out
            // supersample AA and blurred sticky 0° / rotate samples.
            let is_text_xform = crate::utils::text_texture::is_text_xform_texture_key(&id);
            let format = if is_text_xform {
                wgpu::TextureFormat::Rgba8Unorm
            } else {
                wgpu::TextureFormat::Rgba8UnormSrgb
            };
            if self
                .image_textures
                .try_overwrite_rgba8(&id, &self.queue, &rgba, w, h, format)
            {
                continue;
            }
            let gpu = if is_text_xform {
                super::image_registry::upload_rgba8_linear(
                    &self.device,
                    &self.queue,
                    &self.textured_pipeline,
                    &rgba,
                    w,
                    h,
                )
            } else {
                super::image_registry::upload_rgba8(
                    &self.device,
                    &self.queue,
                    &self.textured_pipeline,
                    &rgba,
                    w,
                    h,
                )
            };
            self.image_textures.insert_with_size(id, gpu, w, h);
        }
        #[cfg(feature = "scene3d")]
        {
            let gpu = self
                .scene3d
                .get_or_insert_with(|| crate::renderer::scene3d::Scene3dGpu::new(&self.device));
            if gpu.flush(
                &self.device,
                &self.queue,
                &self.textured_pipeline,
                &mut self.image_textures,
            ) {
                // Collect already ran under `&mut self`; do not go through `runtime::request_redraw`.
                self.window.request_redraw();
            }
        }
        let captures = crate::renderer::take_capture_jobs();
        let effects = crate::renderer::take_effect_jobs();
        let filters = crate::renderer::take_filter_jobs();
        let capture_n = captures.len();
        let fx_begin = Instant::now();
        let mut fx_dirty = false;
        if self.effect_pass.flush(
            &self.device,
            &self.queue,
            &self.textured_pipeline,
            &mut self.image_textures,
            &captures,
            &effects,
            crate::style::global_time_s(),
            mouse_pos,
            ui_scale,
            &self.pipeline,
            Some(&mut self.capture_text_renderer),
        ) {
            fx_dirty = true;
        }
        // Captures already live in `image_textures`; filter pass only runs graphs.
        if self.css_filter_pass.flush_filter_jobs(
            &self.device,
            &self.queue,
            &self.textured_pipeline,
            &mut self.image_textures,
            &filters,
            ui_scale,
        ) {
            fx_dirty = true;
        }
        if fx_dirty {
            self.window.request_redraw();
        }
        let fx_done = Instant::now();
        // Capture/effect passes rewrite pipeline globals.resolution to RT size.
        self.pipeline
            .update_resolution(&self.queue, self.config.width, self.config.height);
        self.textured_pipeline.update_resolution(
            &self.queue,
            self.config.width,
            self.config.height,
        );
        for draw in &render_result.textured_draws {
            self.image_textures.touch(&draw.texture_key);
        }
        if crate::renderer::render_graph_dump_enabled() {
            let dump = format!(
                "{}{}",
                render_result.render_graph.dump(),
                self.image_textures.dump()
            );
            crate::renderer::store_render_graph_dump(dump);
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
            // I-beam on/off is the focused field's chrome. FullClear here wiped every
            // other node dirty (Marker spin) and contradicted Solid-style present.
            crate::components::internal::text_edit::push_focused_text_edit_caret_damage(
                &self.root_element,
            );
        }
        // Size the retain RT before Load vs Clear so a recreate cannot cull into a discarded buffer.
        if Self::gpu_partial_damage_enabled() {
            let _ = self.present_retain.ensure_size(
                &self.device,
                self.config.width,
                self.config.height,
            );
        }
        let overlay_present = overlay_inst_start < scaled_instances.len()
            || overlay_text_start < self.text_nodes.len()
            || overlay_tex_start < render_result.textured_draws.len()
            || overlay_chrome_start < scaled_input_chrome.len()
            || overlay_paint_start < render_result.paint_ops.len();
        // Overlay membership scissors to the hoisted box (WP-2.2). Do not FullClear
        // the gallery, and do not cancel a real scroll-layer Clear — that viewport
        // still has to repaint page glyphs after pixels moved.
        let overlay_tex_slice_start = overlay_tex_start.min(scaled_tex.len());
        let overlay_visual = Self::overlay_visual_bounds_logical(
            &scaled_instances[overlay_inst_start..],
            &scaled_tex[overlay_tex_slice_start..],
            &self.text_nodes[overlay_text_start..],
            &scaled_input_chrome[overlay_chrome_start..],
            ui_scale,
        );
        self.damage_nodes
            .extend(super::damage::take_frame_paint_nodes());
        self.apply_portal_overlay_damage(overlay_present, overlay_visual);
        let gpu_present = self.plan_gpu_present(ui_scale);
        let cull_phys = gpu_present.cull_phys;
        let page_stack_band_ranges = overlay_split.page_stack_band_ranges();
        let has_page_stack_bands = !page_stack_band_ranges.is_empty();
        // Overlay glyphs use `overlay_text_renderer`. Page/page-stack glyphon stays
        // complete unless this is a scroll-layer Clear (pixels in the scissor were wiped).
        // Overlay PartialLoad used to filter page_stack bands to the chip box, then
        // `clear_page_stack_band_glyphon` → gallery bubbles: gray shells, no captions.
        let page_text_unchanged = self.last_text_ui_scale.to_bits() == ui_scale.to_bits()
            && self.last_text_surface_size == [self.config.width, self.config.height]
            && Self::text_nodes_equal(
                &self.text_nodes[..overlay_text_start],
                &self.last_text_nodes_cache,
            )
            && (!has_dynamic_text || self.last_caret_blink_phase == caret_phase)
            && self.text_renderer.pending_warmup.is_empty();
        let text_cache_valid = !has_page_stack_bands
            && page_text_unchanged
            && gpu_present.use_partial_damage;
        // Never cull page glyphon except a real scroll-layer Clear. Never reuse a
        // page-stack buffer on FullClear — that drew empty captions over a wiped RT.
        // Glyph clip uses the unpadded scrolled viewport (1px AA), not the 16px SDF pad.
        let page_text_cull = if self.scroll_layer_clear && gpu_present.use_partial_damage {
            self.scroll_text_clip
                .or(self.damage_rect)
                .map(|d| super::draw_cull::padded_xywh(d, super::draw_cull::SCROLL_LAYER_TEXT_PAD_LOGICAL))
        } else {
            None
        };
        let reuse_page_stack = has_page_stack_bands
            && page_text_unchanged
            && gpu_present.use_partial_damage
            && !self.scroll_layer_clear
            && !self.overlay_restore_clear;
        // Load retain + overlay scissor: keep page pixels. Never pair this with a
        // scissor Clear or a culled page-glyphon prepare (empty gallery shells).
        let retain_page_pixels = super::damage::overlay_partial_load_retains_page(
            gpu_present.use_partial_damage,
            self.scroll_layer_clear || self.overlay_restore_clear,
            overlay_present,
            page_text_unchanged,
        );
        let shadow_cached_keys: std::collections::HashSet<u64> =
            self.shadow_tex_cache.keys().copied().collect();
        let mut page_stack_band_underlines: Vec<Vec<InstanceInput>> = Vec::new();
        let mut page_stack_band_shadows: Vec<Vec<crate::renderer::prepare_text::ShadowTexDraw>> =
            Vec::new();
        let mut page_stack_skip_glyphon: Vec<bool> = Vec::new();
        let mut skip_page_glyphon = false;
        let (underline_instances, caret_instances, shadow_tex_draws) = if has_page_stack_bands {
            self.ensure_page_stack_band_text_renderers(page_stack_band_ranges.len());
            let mut underlines = Vec::with_capacity(page_stack_band_ranges.len());
            let mut shadow_bands = Vec::with_capacity(page_stack_band_ranges.len());
            page_stack_skip_glyphon = vec![false; page_stack_band_ranges.len()];
            if gpu_present.use_node_present {
                // Node present draws capture glyphs only. Preparing page-stack here
                // replaced the last complete mesh; a later Load `render()` of that
                // mixed buffer stamped sidebar captions into retain after Send.
                page_stack_skip_glyphon.fill(true);
                self.last_text_nodes_cache = self.text_nodes[..overlay_text_start].to_vec();
                self.last_text_ui_scale = ui_scale;
                self.last_text_surface_size = [self.config.width, self.config.height];
                (
                    Vec::new(),
                    self.last_caret_instances.clone(),
                    self.last_shadow_tex_draws.clone(),
                )
            } else {
            let mut caret_all = Vec::new();
            let mut needs_warmup = false;
            let mut culled_any = false;
            let text_prepare_begin = Instant::now();
            for (band_i, band) in page_stack_band_ranges.iter().enumerate() {
                let text_lo = band.text.0.min(overlay_text_start);
                let text_hi = band.text.1.min(overlay_text_start);
                if text_lo >= text_hi {
                    page_stack_skip_glyphon[band_i] = true;
                    underlines.push(Vec::new());
                    shadow_bands.push(Vec::new());
                    continue;
                }
                let Some(band_text) = super::draw_cull::page_stack_band_submit_text(
                    &self.last_text_nodes_cache,
                    &self.text_nodes,
                    text_lo,
                    text_hi,
                    page_text_cull,
                    gpu_present.use_partial_damage,
                ) else {
                    // Unchanged runs (sidebar after Send) or outside the scrolled viewport.
                    // Keep last glyphon GPU buffer; do not submit it on Load retain.
                    page_stack_skip_glyphon[band_i] = true;
                    underlines.push(Vec::new());
                    shadow_bands.push(Vec::new());
                    continue;
                };
                let prep_out = if band_i == 0 {
                    prepare_text_frame(
                        &mut self.text_renderer,
                        &self.device,
                        &self.queue,
                        &self.config,
                        &band_text,
                        ui_scale,
                        text_fast,
                        &shadow_cached_keys,
                        &self.shadow_rect_cache,
                    )
                } else {
                    prepare_text_frame(
                        &mut self.page_stack_band_text_renderers[band_i - 1],
                        &self.device,
                        &self.queue,
                        &self.config,
                        &band_text,
                        ui_scale,
                        text_fast,
                        &shadow_cached_keys,
                        &self.shadow_rect_cache,
                    )
                };
                // A band whose glyphs are all clip-culled must not abort the rest of the page:
                // instances for other bands were already uploaded at the new scroll offset.
                // Breaking here leaves glyphon vertices on the previous frame (background
                // scrolls, text does not follow). Clear this band's GPU vertices instead of
                // keeping them: stale glyphs land in the local mask for one frame on scroll-up.
                // Overlay/hover PartialLoad must NOT clear: that wiped gallery bubble captions.
                if prep_out.culled_skip_prepare {
                    culled_any = true;
                    page_stack_skip_glyphon[band_i] = true;
                    if self.scroll_layer_clear {
                        self.clear_page_stack_band_glyphon(band_i);
                    }
                    underlines.push(Vec::new());
                    shadow_bands.push(Vec::new());
                    continue;
                }
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
                if prep_out.needs_warmup {
                    needs_warmup = true;
                }
                underlines.push(prep_out.underline_instances);
                caret_all.extend(prep_out.caret_instances);
                shadow_bands.push(prep_out.shadow_tex_draws);
            }
            if culled_any {
                self.window.request_redraw();
            }
            {
                let shadow_all: Vec<_> = shadow_bands.iter().flatten().cloned().collect();
                // Always keep the CPU cache aligned so the next Send can skip
                // unchanged runs even when this frame submitted a later bubble.
                self.last_text_nodes_cache = self.text_nodes[..overlay_text_start].to_vec();
                self.last_text_ui_scale = ui_scale;
                self.last_text_surface_size = [self.config.width, self.config.height];
                self.last_underline_instances.clear();
                self.last_caret_instances = caret_all.clone();
                self.last_shadow_tex_draws = shadow_all.clone();
                self.last_caret_blink_phase = caret_phase;
                if self.text_backend == TextRendererBackend::CpuTextHybrid {
                    self.cpu_text_profiler.record(
                        (Instant::now() - text_prepare_begin).as_secs_f64() * 1000.0,
                        overlay_text_start,
                    );
                }
                if needs_warmup {
                    self.window.request_redraw();
                }
                if crate::text::take_fit_needs_relayout() {
                    if scroll_paint_only {
                        crate::text::note_fit_needs_relayout();
                    } else {
                        self.layout_status = crate::core::tree::LayoutStatus::LayoutChanged;
                        self.window.request_redraw();
                    }
                }
                page_stack_band_underlines = underlines;
                page_stack_band_shadows = shadow_bands;
                (Vec::new(), caret_all, shadow_all)
            }
            }
        } else if text_cache_valid {
            skip_page_glyphon = true;
            (
                self.last_underline_instances.clone(),
                self.last_caret_instances.clone(),
                self.last_shadow_tex_draws.clone(),
            )
        } else {
            let text_prepare_begin = Instant::now();
            match super::draw_cull::page_stack_band_submit_text(
                &self.last_text_nodes_cache,
                &self.text_nodes,
                0,
                overlay_text_start,
                page_text_cull,
                gpu_present.use_partial_damage,
            ) {
                None => {
                    skip_page_glyphon = true;
                    (
                        self.last_underline_instances.clone(),
                        self.last_caret_instances.clone(),
                        self.last_shadow_tex_draws.clone(),
                    )
                }
                Some(page_text) => {
                    let prep_out = prepare_text_frame(
                        &mut self.text_renderer,
                        &self.device,
                        &self.queue,
                        &self.config,
                        &page_text,
                        ui_scale,
                        text_fast,
                        &shadow_cached_keys,
                        &self.shadow_rect_cache,
                    );
                    if prep_out.culled_skip_prepare {
                        skip_page_glyphon = true;
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
                        if page_text_cull.is_none() {
                            // A scroll-layer clip must not be cached as the complete page
                            // buffer — the next overlay/hover PartialLoad would reuse it and
                            // drop captions outside the scroller.
                            self.last_text_nodes_cache =
                                self.text_nodes[..overlay_text_start].to_vec();
                            self.last_text_ui_scale = ui_scale;
                            self.last_text_surface_size = [self.config.width, self.config.height];
                            self.last_underline_instances = prep_out.underline_instances.clone();
                            self.last_caret_instances = prep_out.caret_instances.clone();
                            self.last_shadow_tex_draws = prep_out.shadow_tex_draws.clone();
                            self.last_caret_blink_phase = caret_phase;
                        }
                        if self.text_backend == TextRendererBackend::CpuTextHybrid {
                            self.cpu_text_profiler.record(
                                (Instant::now() - text_prepare_begin).as_secs_f64() * 1000.0,
                                overlay_text_start,
                            );
                        }
                        if prep_out.needs_warmup {
                            self.window.request_redraw();
                        }
                        if crate::text::take_fit_needs_relayout() {
                            if scroll_paint_only {
                                crate::text::note_fit_needs_relayout();
                            } else {
                                self.layout_status = crate::core::tree::LayoutStatus::LayoutChanged;
                                self.window.request_redraw();
                            }
                        }
                        (
                            prep_out.underline_instances,
                            prep_out.caret_instances,
                            prep_out.shadow_tex_draws,
                        )
                    }
                }
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
        let mut overlay_band_shadows: Vec<Vec<crate::renderer::prepare_text::ShadowTexDraw>> =
            Vec::with_capacity(overlay_band_ranges.len());
        let mut overlay_needs_warmup = false;
        for (band_i, band) in overlay_band_ranges.iter().enumerate() {
            let text_lo = band.text.0;
            let text_hi = band.text.1;
            if text_lo >= text_hi {
                overlay_band_underlines.push(Vec::new());
                overlay_band_shadows.push(Vec::new());
                continue;
            }
            let shadow_cached_keys: std::collections::HashSet<u64> =
                self.shadow_tex_cache.keys().copied().collect();
            let overlay_text =
                text_nodes_for_prepare(&self.text_nodes, text_lo, text_hi, None);
            let overlay_prep = if band_i == 0 {
                prepare_text_frame(
                    &mut self.overlay_text_renderer,
                    &self.device,
                    &self.queue,
                    &self.config,
                    &overlay_text,
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
                    &overlay_text,
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
            overlay_band_shadows.push(overlay_prep.shadow_tex_draws);
        }
        if overlay_needs_warmup {
            self.window.request_redraw();
        }
        if crate::text::take_fit_needs_relayout() {
            if scroll_paint_only {
                crate::text::note_fit_needs_relayout();
            } else {
                self.layout_status = crate::core::tree::LayoutStatus::LayoutChanged;
                self.window.request_redraw();
            }
        }
        let capture_text_ready = if gpu_present.use_node_present
            && self.damage_nodes.iter().any(|d| {
                match d.kind {
                    super::damage::PaintDirtyKind::OverlayRestore => true,
                    super::damage::PaintDirtyKind::Node => {
                        super::draw_cull::node_present_needs_glyphs(
                            d.key,
                            d.bounds,
                            &render_result.node_paints,
                            &scaled_instances
                                [..overlay_inst_start.min(scaled_instances.len())],
                        )
                    }
                    super::damage::PaintDirtyKind::Overlay => false,
                }
            }) {
            let page_text = &self.text_nodes[..overlay_text_start];
            let page_inst = &scaled_instances[..overlay_inst_start.min(scaled_instances.len())];
            let glyph_dirties: Vec<_> = self
                .damage_nodes
                .iter()
                .copied()
                .filter(|d| match d.kind {
                    super::damage::PaintDirtyKind::OverlayRestore => true,
                    super::damage::PaintDirtyKind::Node => {
                        super::draw_cull::node_present_needs_glyphs(
                            d.key,
                            d.bounds,
                            &render_result.node_paints,
                            page_inst,
                        )
                    }
                    super::damage::PaintDirtyKind::Overlay => false,
                })
                .collect();
            let hole_text = super::draw_cull::node_present_clipped_capture_text(
                &glyph_dirties,
                page_text,
                &render_result.node_paints,
            );
            if hole_text.is_empty() {
                false
            } else {
                let shadow_keys: std::collections::HashSet<u64> =
                    self.shadow_tex_cache.keys().copied().collect();
                prepare_text_frame(
                    &mut self.capture_text_renderer,
                    &self.device,
                    &self.queue,
                    &self.config,
                    &hole_text,
                    ui_scale,
                    text_fast,
                    &shadow_keys,
                    &self.shadow_rect_cache,
                );
                true
            }
        } else {
            false
        };
        if crate::core::texture_stats::texture_stats_enabled()
            && !Self::is_bootstrap_root(&self.root_element)
        {
            static TEXTURE_STATS_ONCE: std::sync::Once = std::sync::Once::new();
            TEXTURE_STATS_ONCE.call_once(|| {
                crate::core::texture_stats::emit_texture_stats(
                    &self.image_textures,
                    self.shadow_tex_cache.len(),
                    &self.text_renderer,
                    &self.overlay_text_renderer,
                    &self.overlay_band_text_renderers,
                );
            });
        }
        let text_done = Instant::now();
        let prev_presented_underlines = self.last_presented_underlines.clone();
        let page_stack_underline_flat: Vec<InstanceInput> = page_stack_band_underlines
            .iter()
            .flatten()
            .copied()
            .collect();
        let presented_underlines = current_underline_layer(
            has_page_stack_bands,
            reuse_page_stack,
            &page_stack_underline_flat,
            &underline_instances,
            &prev_presented_underlines,
        );

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
            hash_present_underline_layers(
                &mut hasher,
                &underline_instances,
                &page_stack_band_underlines,
                &overlay_band_underlines,
            );
            hasher.write(bytemuck::cast_slice(&caret_instances));
            // Canvas / scene3d reuse a stable texture_key while pixels change every frame.
            // Without this, last_gpu_frame_fp matches and present is skipped (frozen cube / laggy ink).
            super::pending_images::dynamic_texture_epoch().hash(&mut hasher);
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
                n.style.color_resolved().hash(&mut hasher);
                n.style.size_resolved().to_bits().hash(&mut hasher);
                n.style.weight_resolved().hash(&mut hasher);
                n.style.underline_resolved().hash(&mut hasher);
                n.style.overline_resolved().hash(&mut hasher);
                n.style.line_through_resolved().hash(&mut hasher);
                n.style.decoration_color.hash(&mut hasher);
                n.style
                    .underline_offset_resolved()
                    .to_bits()
                    .hash(&mut hasher);
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
            for band in &overlay_band_shadows {
                band.len().hash(&mut hasher);
                for d in band {
                    d.cache_key.hash(&mut hasher);
                    for v in d.rect {
                        v.to_bits().hash(&mut hasher);
                    }
                }
            }
            let frame_fp = hasher.finish();
            if self.has_presented_frame
                && !self.force_full_redraw
                && self.damage_nodes.is_empty()
                && !self.scroll_layer_clear
                && !self.overlay_restore_clear
                && frame_fp == self.last_gpu_frame_fp
            {
                let total_ms = render_begin.elapsed().as_secs_f32() * 1000.0;
                if total_ms >= 200.0 {
                    eprintln!(
                        "slow render {total_ms:.0}ms skip_present=true idle={idle_ms:.0} pre={:.1} layout={:.1} rebuild={:.1} taffy={:.1} collect={:.1} setup={:.1} passes={layout_passes} full={} portal={portal_moves} upload={:.1} fx={:.1} text={:.1} captures={capture_n} modal_css={}",
                        (pre_done - render_begin).as_secs_f32() * 1000.0,
                        (layout_done - pre_done).as_secs_f32() * 1000.0,
                        layout_prof_sum.rebuild_ms,
                        layout_prof_sum.taffy_compute_ms,
                        layout_prof_sum.collect_ms,
                        layout_prof_sum.setup_ms,
                        use_full_taffy_rebuild,
                        (upload_done - layout_done).as_secs_f32() * 1000.0,
                        (fx_done - fx_begin).as_secs_f32() * 1000.0,
                        (text_done - fx_done).as_secs_f32() * 1000.0,
                        crate::utils::modal_motion::any_modal_css_needs_frame(),
                    );
                }
                if !self.needs_text_settle {
                    self.layout_status = crate::core::tree::LayoutStatus::Clean;
                }
                self.layout_tree.recycle_collect_buffers(
                    scaled_instances,
                    std::mem::take(&mut render_result.textured_draws),
                    scaled_input_chrome,
                );
                self.last_render_at = Instant::now();
                return Ok(());
            }
            self.last_gpu_frame_fp = frame_fp;
            self.last_presented_underlines = presented_underlines.clone();
        }

        // Acquire swapchain image as late as possible so CPU-side text/layout work can overlap
        // with present pacing; this typically reduces blocking time in get_current_texture().
        let acquire_begin = Instant::now();
        let output = crate::renderer::wgpu_compat::acquire_surface(&self.surface)?;
        let acquire_done = Instant::now();
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

        let mut scissor_physical = gpu_present.scissor_physical;
        let logical_policy = gpu_present.logical_policy;
        let use_partial_damage = gpu_present.use_partial_damage;
        let mut draw_to_retain = gpu_present.draw_to_retain;
        super::damage::note_frame_clear_policy(logical_policy);

        let max_backdrop_blur_logical = overlay_split
            .bands
            .iter()
            .map(|b| b.backdrop_blur)
            .fold(0.0_f32, f32::max);
        // Only underlays that actually blur — the panel band is opacity=1 for the
        // whole slide and would pin σ at full strength (instant blur, then pop off).
        let modal_underlay_opacity = overlay_split
            .bands
            .iter()
            .filter(|b| b.backdrop_blur > 0.5)
            .map(|b| b.underlay_opacity)
            .fold(0.0_f32, f32::max);
        let allow_primitives = !(self.text_backend == TextRendererBackend::CpuTextHybrid
            && Self::cpu_hybrid_text_only());
        // Fullscreen modal underlay blur: capture page → blur → composite under overlays.
        let use_backdrop_blur = allow_primitives
            && max_backdrop_blur_logical > 0.5
            && modal_underlay_opacity > 0.001
            && overlay_inst_start > 0
            && !use_partial_damage;
        // Never `Load` a FLIP_DISCARD / GLES swapchain. Modal blur used to blit then Load
        // the surface; composite into retain and blit the finished frame instead.
        if use_backdrop_blur {
            draw_to_retain = true;
        }
        if draw_to_retain {
            let _ = self.present_retain.ensure_size(
                &self.device,
                self.config.width,
                self.config.height,
            );
        } else {
            self.present_retain.invalidate_contents();
        }
        let clear_color = crate::renderer::surface_clear_color(self.compositor_alpha);
        let color_load = if use_partial_damage {
            wgpu::LoadOp::Load
        } else {
            wgpu::LoadOp::Clear(clear_color)
        };
        if !use_partial_damage {
            scissor_physical = None;
        }
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
                            depth_slice: None,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(clear_color),
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: None,
                        timestamp_writes: None,
                        occlusion_query_set: None,
                        multiview_mask: None,
                    });
                    page_pass.set_pipeline(&self.pipeline.render_pipeline);
                    page_pass.set_bind_group(0, &self.pipeline.bind_group_clip_poly, &[]);
                    page_pass.set_bind_group(1, self.pipeline.dummy_backdrop(), &[]);
                    page_pass.set_vertex_buffer(0, self.pipeline.vertex_buffer.slice(..));
                    page_pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
                    page_pass.set_index_buffer(
                        self.pipeline.index_buffer.slice(..),
                        wgpu::IndexFormat::Uint16,
                    );
                    if has_page_stack_bands {
                        for (band_i, band) in page_stack_band_ranges.iter().enumerate() {
                            if band.paint_ops.0 < band.paint_ops.1 {
                                draw_paint_ops(
                                    &mut page_pass,
                                    &render_result.paint_ops,
                                    band.paint_ops.0,
                                    band.paint_ops.1,
                                    &self.pipeline,
                                    &self.instance_buffer,
                                    &self.textured_pipeline,
                                    &self.textured_instance_buffer,
                                    &self.image_textures,
                                    &scaled_tex,
                                    self.pipeline.dummy_backdrop(),
                                    &[],
                                    None,
                                );
                            } else if band.instance.0 < band.instance.1 {
                                page_pass.set_pipeline(&self.pipeline.render_pipeline);
                                page_pass.set_bind_group(
                                    0,
                                    &self.pipeline.bind_group_clip_poly,
                                    &[],
                                );
                                page_pass.set_bind_group(1, self.pipeline.dummy_backdrop(), &[]);
                                page_pass
                                    .set_vertex_buffer(0, self.pipeline.vertex_buffer.slice(..));
                                page_pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
                                page_pass.set_index_buffer(
                                    self.pipeline.index_buffer.slice(..),
                                    wgpu::IndexFormat::Uint16,
                                );
                                page_pass.draw_indexed(
                                    0..self.pipeline.num_indices,
                                    0,
                                    band.instance.0 as u32..band.instance.1 as u32,
                                );
                                if band.textured.0 < band.textured.1 {
                                    draw_packed_textured_runs(
                                        &mut page_pass,
                                        &self.textured_pipeline,
                                        &self.textured_instance_buffer,
                                        &self.image_textures,
                                        &scaled_tex,
                                        band.textured.0,
                                        band.textured.1,
                                        None,
                                    );
                                }
                            }
                            if band.chrome.0 < band.chrome.1 {
                                page_pass.set_pipeline(&self.pipeline.render_pipeline);
                                page_pass.set_bind_group(0, &self.pipeline.bind_group, &[]);
                                page_pass.set_bind_group(1, self.pipeline.dummy_backdrop(), &[]);
                                page_pass
                                    .set_vertex_buffer(0, self.pipeline.vertex_buffer.slice(..));
                                let chrome_slice =
                                    &scaled_input_chrome[band.chrome.0..band.chrome.1];
                                let chrome_buffer = self.device.create_buffer_init(
                                    &wgpu::util::BufferInitDescriptor {
                                        label: Some("Page Stack Chrome Buffer (scene)"),
                                        contents: bytemuck::cast_slice(chrome_slice),
                                        usage: wgpu::BufferUsages::VERTEX,
                                    },
                                );
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
                            if let Some(ul) = page_stack_band_underlines.get(band_i) {
                                if !ul.is_empty() {
                                    page_pass.set_pipeline(&self.pipeline.render_pipeline);
                                    page_pass.set_bind_group(0, &self.pipeline.bind_group, &[]);
                                    page_pass.set_bind_group(
                                        1,
                                        self.pipeline.dummy_backdrop(),
                                        &[],
                                    );
                                    page_pass.set_vertex_buffer(
                                        0,
                                        self.pipeline.vertex_buffer.slice(..),
                                    );
                                    let underline_buffer = self.device.create_buffer_init(
                                        &wgpu::util::BufferInitDescriptor {
                                            label: Some("Page Stack Underline Buffer (scene)"),
                                            contents: bytemuck::cast_slice(ul),
                                            usage: wgpu::BufferUsages::VERTEX,
                                        },
                                    );
                                    page_pass.set_vertex_buffer(1, underline_buffer.slice(..));
                                    page_pass.set_index_buffer(
                                        self.pipeline.index_buffer.slice(..),
                                        wgpu::IndexFormat::Uint16,
                                    );
                                    page_pass.draw_indexed(
                                        0..self.pipeline.num_indices,
                                        0,
                                        0..ul.len() as u32,
                                    );
                                }
                            }
                            if let Some(sds) = page_stack_band_shadows.get(band_i) {
                                draw_shadow_tex_draws(
                                    &mut page_pass,
                                    sds,
                                    &self.shadow_tex_cache,
                                    &self.textured_pipeline,
                                    &self.device,
                                    None,
                                );
                            }
                            if band.text.0 < band.text.1
                                && !page_stack_skip_glyphon.get(band_i).copied().unwrap_or(false)
                            {
                                if band_i == 0 {
                                    self.text_renderer.render(&mut page_pass);
                                } else {
                                    self.page_stack_band_text_renderers[band_i - 1]
                                        .render(&mut page_pass);
                                }
                            }
                        }
                    } else if overlay_paint_start > 0 {
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
                            self.pipeline.dummy_backdrop(),
                            &[],
                            None,
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
                                None,
                            );
                        }
                    }
                    if !has_page_stack_bands {
                        if !underline_instances.is_empty() {
                            page_pass.set_pipeline(&self.pipeline.render_pipeline);
                            page_pass.set_bind_group(0, &self.pipeline.bind_group, &[]);
                            page_pass.set_bind_group(1, self.pipeline.dummy_backdrop(), &[]);
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
                        draw_shadow_tex_draws(
                            &mut page_pass,
                            &shadow_tex_draws,
                            &self.shadow_tex_cache,
                            &self.textured_pipeline,
                            &self.device,
                            None,
                        );
                        self.text_renderer.render(&mut page_pass);
                    }
                    if overlay_chrome_start > 0 {
                        page_pass.set_pipeline(&self.pipeline.render_pipeline);
                        page_pass.set_bind_group(0, &self.pipeline.bind_group, &[]);
                        page_pass.set_bind_group(1, self.pipeline.dummy_backdrop(), &[]);
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
                // Ramp blur σ with overlay opacity (CSS `backdrop-filter` under a fading
                // element). Keep the blit itself opaque — dimming is the underlay rect.
                self.backdrop_blur.blur(
                    &self.device,
                    &self.queue,
                    &mut encoder,
                    max_backdrop_blur_logical * modal_underlay_opacity * ui_scale,
                );
                // Same UV space as capture / TexturedQuadPipeline (rect top → uv.y = 0).
                // Destination is retain (draw_to_retain forced above); never Load the swapchain.
                let blur_dst = self.present_retain.color_view();
                if let Some(dst) = blur_dst {
                    self.backdrop_blur
                        .blit_to(&self.queue, &mut encoder, dst, 1.0, clear_color);
                }
            }
        }

        let color_load = if use_backdrop_blur {
            // Retain already holds the blurred underlay (Clear blit). Load that, not the surface.
            wgpu::LoadOp::Load
        } else {
            color_load
        };

        let mut live_backdrops: Vec<wgpu::BindGroup> = Vec::new();
        let mut band_backdrop_idx: Vec<Option<usize>> = vec![None; page_stack_band_ranges.len()];
        // Capture submits its own encoder (like subtree `render_capture`) so prefix
        // glyphon is flushed to the scene RT before this frame encoder reuses the
        // same vertex buffers on the swapchain.
        if allow_primitives && !use_backdrop_blur && has_page_stack_bands {
            for (band_i, band) in page_stack_band_ranges.iter().enumerate() {
                // `render_result.instances` was `take`n into `scaled_instances` (physical).
                // Sampling the empty buffer made local_blur always 0 — dummy backdrop, no frost.
                let local_blur = band_instance_backdrop_blur(&scaled_instances, band);
                if local_blur <= 0.5 {
                    continue;
                }
                capture_page_stack_local_backdrop(
                    &mut self.backdrop_blur,
                    &self.device,
                    &self.queue,
                    self.config.width,
                    self.config.height,
                    self.config.format,
                    band,
                    &page_stack_band_ranges,
                    band_i,
                    local_blur * ui_scale,
                    clear_color,
                    &render_result.paint_ops,
                    &self.pipeline,
                    &self.instance_buffer,
                    &self.textured_pipeline,
                    &self.textured_instance_buffer,
                    &self.image_textures,
                    &scaled_tex,
                    &scaled_input_chrome,
                    &page_stack_band_underlines,
                    &page_stack_band_shadows,
                    &self.shadow_tex_cache,
                    &self.text_renderer,
                    &self.page_stack_band_text_renderers,
                );
                if let Some(view) = self.backdrop_blur.blurred_view() {
                    let idx = live_backdrops.len();
                    live_backdrops.push(self.pipeline.make_backdrop_bind_group(
                        &self.device,
                        view,
                        self.backdrop_blur.sampler(),
                    ));
                    band_backdrop_idx[band_i] = Some(idx);
                }
            }
        }

        {
            debug_assert!(
                !matches!(color_load, wgpu::LoadOp::Load) || draw_to_retain,
                "LoadOp::Load is only valid on present_retain, never the swapchain"
            );
            let color_view = if draw_to_retain {
                self.present_retain
                    .color_view()
                    .ok_or(crate::renderer::wgpu_compat::SurfaceAcquireError::Lost)?
            } else {
                &view
            };
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: color_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: color_load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            if let Some((x, y, w, h)) = scissor_physical {
                render_pass.set_scissor_rect(x, y, w, h);
            }
            super::frame_stats::begin_scene_batches();
            if gpu_present.clear_damage {
                if let Some(rect) = cull_phys {
                    draw_scissor_clear(
                        &mut render_pass,
                        &self.pipeline,
                        &self.device,
                        rect,
                        clear_color,
                    );
                }
            }

            render_pass.set_pipeline(&self.pipeline.render_pipeline);
            self.pipeline
                .bind_sdf(&mut render_pass, true, self.pipeline.dummy_backdrop());
            render_pass.set_vertex_buffer(0, self.pipeline.vertex_buffer.slice(..));
            render_pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
            render_pass.set_index_buffer(
                self.pipeline.index_buffer.slice(..),
                wgpu::IndexFormat::Uint16,
            );
            // Page layer: tree-order rects + textures (Solid/CSS stacking with portal overlays).
            // With in-page stacking bands (CarouselStack): each band is `rects → text` so
            // higher-z card fills occlude lower cards' glyphs.
            // Skipped when backdrop blur already composited the page.
            // Overlay PartialLoad: Load keeps page pixels; drawing fills here without
            // matching glyphon is the empty-shell bug.
            if gpu_present.use_node_present {
                self.encode_node_present(
                    &mut render_pass,
                    &render_result.paint_ops,
                    &render_result.node_paints,
                    &scaled_instances,
                    &scaled_tex,
                    &scaled_input_chrome,
                    overlay_inst_start,
                    overlay_tex_start,
                    overlay_paint_start,
                    allow_primitives,
                    clear_color,
                    ui_scale,
                    &overlay_band_ranges,
                    capture_text_ready,
                    &presented_underlines,
                    &prev_presented_underlines,
                );
            } else if allow_primitives && !use_backdrop_blur && !retain_page_pixels {
                if has_page_stack_bands {
                    for (band_i, band) in page_stack_band_ranges.iter().enumerate() {
                        let band_backdrop = band_backdrop_idx
                            .get(band_i)
                            .and_then(|idx| *idx)
                            .and_then(|idx| live_backdrops.get(idx))
                            .unwrap_or_else(|| self.pipeline.dummy_backdrop());
                        draw_page_stack_band_to_pass(
                            &mut render_pass,
                            band_i,
                            band,
                            &render_result.paint_ops,
                            &self.pipeline,
                            &self.instance_buffer,
                            &self.textured_pipeline,
                            &self.textured_instance_buffer,
                            &self.image_textures,
                            &scaled_tex,
                            &scaled_input_chrome,
                            &self.device,
                            &page_stack_band_underlines,
                            &page_stack_band_shadows,
                            &self.shadow_tex_cache,
                            band_backdrop,
                            &scaled_instances,
                            cull_phys,
                        );
                        if band.text.0 < band.text.1
                            && !page_stack_skip_glyphon.get(band_i).copied().unwrap_or(false)
                        {
                            if band_i == 0 {
                                self.text_renderer.render(&mut render_pass);
                            } else {
                                self.page_stack_band_text_renderers[band_i - 1]
                                    .render(&mut render_pass);
                            }
                            super::frame_stats::note_scene_batch(
                                super::frame_stats::SceneKind::Text,
                                1,
                            );
                        }
                    }
                } else if overlay_paint_start > 0 {
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
                        self.pipeline.dummy_backdrop(),
                        &scaled_instances,
                        cull_phys,
                    );
                } else if overlay_inst_start > 0 {
                    draw_instance_range(
                        &mut render_pass,
                        &self.pipeline,
                        &scaled_instances,
                        0,
                        overlay_inst_start as u32,
                        cull_phys,
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
                            cull_phys,
                        );
                    }
                }
            }

            if !use_backdrop_blur && !has_page_stack_bands && !retain_page_pixels && !gpu_present.use_node_present {
                draw_shadow_tex_draws(
                    &mut render_pass,
                    &shadow_tex_draws,
                    &self.shadow_tex_cache,
                    &self.textured_pipeline,
                    &self.device,
                    cull_phys,
                );
            }

            if !has_page_stack_bands && !underline_instances.is_empty() && !use_backdrop_blur && !retain_page_pixels && !gpu_present.use_node_present {
                let underline_instances = instances_for_upload(&underline_instances, cull_phys);
                if !underline_instances.is_empty() {
                    render_pass.set_pipeline(&self.pipeline.render_pipeline);
                    render_pass.set_bind_group(0, &self.pipeline.bind_group, &[]);
                    render_pass.set_bind_group(1, self.pipeline.dummy_backdrop(), &[]);
                    render_pass.set_vertex_buffer(0, self.pipeline.vertex_buffer.slice(..));
                    let underline_buffer =
                        self.device
                            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                                label: Some("Underline Instance Buffer"),
                                contents: bytemuck::cast_slice(underline_instances.as_ref()),
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
            }

            if !has_page_stack_bands && !use_backdrop_blur && !retain_page_pixels && !gpu_present.use_node_present && !skip_page_glyphon {
                self.text_renderer.render(&mut render_pass);
                super::frame_stats::note_scene_batch(super::frame_stats::SceneKind::Text, 1);
            }

            // Page input chrome above page glyphs, below portal overlay.
            // (Stacking bands already drew per-band chrome inside the band loop.)
            if allow_primitives
                && overlay_chrome_start > 0
                && !use_backdrop_blur
                && !has_page_stack_bands
                && !retain_page_pixels
                && !gpu_present.use_node_present
            {
                let chrome_slice =
                    instances_for_upload(&scaled_input_chrome[..overlay_chrome_start], cull_phys);
                if !chrome_slice.is_empty() {
                    render_pass.set_pipeline(&self.pipeline.render_pipeline);
                    render_pass.set_bind_group(0, &self.pipeline.bind_group, &[]);
                    render_pass.set_bind_group(1, self.pipeline.dummy_backdrop(), &[]);
                    render_pass.set_vertex_buffer(0, self.pipeline.vertex_buffer.slice(..));
                    let chrome_buffer =
                        self.device
                            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                                label: Some("Input Chrome Instance Buffer"),
                                contents: bytemuck::cast_slice(chrome_slice.as_ref()),
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
            }

            // Legacy prepare_text caret path (kept empty; chrome comes from collect).
            if !caret_instances.is_empty() && !retain_page_pixels && !gpu_present.use_node_present {
                let caret_instances = instances_for_upload(&caret_instances, cull_phys);
                if !caret_instances.is_empty() {
                    render_pass.set_pipeline(&self.pipeline.render_pipeline);
                    render_pass.set_bind_group(0, &self.pipeline.bind_group, &[]);
                    render_pass.set_bind_group(1, self.pipeline.dummy_backdrop(), &[]);
                    render_pass.set_vertex_buffer(0, self.pipeline.vertex_buffer.slice(..));
                    let caret_buffer =
                        self.device
                            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                                label: Some("Caret Instance Buffer"),
                                contents: bytemuck::cast_slice(caret_instances.as_ref()),
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
            }

            // Portal overlay: per-band `rects → text` so stacked panels occlude correctly
            // (still one Clear pass — no GLES LoadOp::Load).
            if !gpu_present.use_node_present {
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
                        self.pipeline.dummy_backdrop(),
                        &scaled_instances,
                        None,
                    );
                } else if allow_primitives && band.instance.0 < band.instance.1 {
                    render_pass.set_pipeline(&self.pipeline.render_pipeline);
                    render_pass.set_bind_group(0, &self.pipeline.bind_group_clip_poly, &[]);
                    render_pass.set_bind_group(1, self.pipeline.dummy_backdrop(), &[]);
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
                            None,
                        );
                    }
                }

                if allow_primitives && band.chrome.0 < band.chrome.1 {
                    render_pass.set_pipeline(&self.pipeline.render_pipeline);
                    render_pass.set_bind_group(0, &self.pipeline.bind_group, &[]);
                    render_pass.set_bind_group(1, self.pipeline.dummy_backdrop(), &[]);
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
                        render_pass.set_bind_group(1, self.pipeline.dummy_backdrop(), &[]);
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

                if let Some(sds) = overlay_band_shadows.get(band_i) {
                    draw_shadow_tex_draws(
                        &mut render_pass,
                        sds,
                        &self.shadow_tex_cache,
                        &self.textured_pipeline,
                        &self.device,
                        None,
                    );
                }

                if band.text.0 < band.text.1 {
                    if band_i == 0 {
                        self.overlay_text_renderer.render(&mut render_pass);
                    } else {
                        self.overlay_band_text_renderers[band_i - 1].render(&mut render_pass);
                    }
                    super::frame_stats::note_scene_batch(super::frame_stats::SceneKind::Text, 1);
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
        let encode_done = Instant::now();

        // Glyphon hello-world: prepare → render → trim. Trimming before capture/surface
        // `render` emptied `glyphs_in_use` so a later prepare could LRU-evict atlas
        // slots the already-recorded passes still sample.
        self.trim_all_glyph_atlases();

        let submit_begin = Instant::now();
        self.queue.submit(std::iter::once(encoder.finish()));
        let submit_done = Instant::now();
        let present_begin = Instant::now();
        self.queue.present(output);
        self.damage_rect = None;
        self.damage_nodes.clear();
        self.force_full_redraw = false;
        self.has_presented_frame = true;
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
            self.render_prof_text_ms += (text_done - upload_done).as_secs_f32() * 1000.0;
            self.render_prof_encode_ms += (encode_done - acquire_done).as_secs_f32() * 1000.0;

            let total_ms = (draw_done - render_begin).as_secs_f32() * 1000.0;
            if total_ms >= 200.0 {
                eprintln!(
                    "slow render {total_ms:.0}ms skip_present=false idle={idle_ms:.0} pre={:.1} layout={:.1} rebuild={:.1} taffy={:.1} collect={:.1} setup={:.1} passes={layout_passes} full={} portal={portal_moves} upload={:.1} fx={:.1} text={:.1} acquire={:.1} encode={:.1} submit={:.1} present={:.1} captures={capture_n} modal_css={}",
                    (pre_done - render_begin).as_secs_f32() * 1000.0,
                    (layout_done - pre_done).as_secs_f32() * 1000.0,
                    layout_prof_sum.rebuild_ms,
                    layout_prof_sum.taffy_compute_ms,
                    layout_prof_sum.collect_ms,
                    layout_prof_sum.setup_ms,
                    use_full_taffy_rebuild,
                    (upload_done - layout_done).as_secs_f32() * 1000.0,
                    (fx_done - fx_begin).as_secs_f32() * 1000.0,
                    (text_done - fx_done).as_secs_f32() * 1000.0,
                    (acquire_done - acquire_begin).as_secs_f32() * 1000.0,
                    (encode_done - acquire_done).as_secs_f32() * 1000.0,
                    (submit_done - submit_begin).as_secs_f32() * 1000.0,
                    (draw_done - present_begin).as_secs_f32() * 1000.0,
                    crate::utils::modal_motion::any_modal_css_needs_frame(),
                );
            }

            let elapsed = self.render_prof_last.elapsed();
            if elapsed >= std::time::Duration::from_secs(1) && render_profile_log_enabled() {
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

        self.layout_tree.recycle_collect_buffers(
            scaled_instances,
            std::mem::take(&mut render_result.textured_draws),
            scaled_input_chrome,
        );
        self.last_render_at = Instant::now();
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
    track_fades: bool,
    thumb_border_fades: bool,
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
    crate::core::tree::walk_elements_post_order_mut(el, |el| {
        let self_is = el
            .focus_id
            .as_deref()
            .is_some_and(|f| f.starts_with(SCROLL_VIEWPORT_FOCUS_PREFIX));
        let mut any_child = false;
        let mut vp_count = u32::from(self_is);
        for child in &el.children {
            if child.contains_scroll_viewport {
                any_child = true;
            }
            vp_count = vp_count.saturating_add(child.scroll_viewport_subtree_count);
        }
        el.scroll_viewport_subtree_count = vp_count;
        el.contains_scroll_viewport = self_is || any_child;
    });
    el.contains_scroll_viewport
}

fn normalize_anchor_template_id(id: &str) -> String {
    id.replace('\\', "/").to_ascii_lowercase()
}

fn element_contains_rui_portal(el: &Element) -> bool {
    crate::core::tree::find_by_attr(el, "rui_portal", "1").is_some()
}

/// Used by [`AppState::reconcile_subtree`] only. Must not use hot-reload fuzzy matching.
fn find_reactive_subtree_by_anchor_id_mut<'a>(
    el: &'a mut Element,
    want_normalized: &str,
) -> Option<&'a mut Element> {
    crate::core::tree::find_in_tree_mut(el, |n| {
        n.template_id
            .as_deref()
            .is_some_and(|tid| normalize_anchor_template_id(tid) == want_normalized)
    })
}

/// Copy scroll-offset signals onto every `scroll:` viewport **before** chrome sync.
///
/// Cheap walk (flags prune non-scroll subtrees). The later scrollbar prepass still runs for
/// track/thumb styling; this only keeps `Element.scroll_offset` current for `:hover` / paint.
fn sync_viewport_scroll_offsets_from_signals(el: &mut Element) {
    let mut stack: Vec<*mut Element> = vec![el as *mut Element];
    while let Some(ptr) = stack.pop() {
        let el = unsafe { &mut *ptr };
        if !el.contains_scroll_viewport {
            continue;
        }
        if let Some(fid) = el.focus_id.as_deref() {
            if let Some(rest) = fid.strip_prefix(SCROLL_VIEWPORT_FOCUS_PREFIX) {
                let sx = use_signal(&scroll_offset_x_signal_key(rest), || 0.0_f32).peek();
                let sy = use_signal(&scroll_offset_y_signal_key(rest), || 0.0_f32).peek();
                el.scroll_offset = [sx, sy];
                if el.scroll_viewport_subtree_count <= 1 {
                    continue;
                }
            }
        }
        let n = el.children.len();
        for i in (0..n).rev() {
            stack.push(std::ptr::addr_of_mut!(el.children[i]));
        }
    }
}

fn update_scrollbar_element_self(
    el: &mut Element,
    base_style: &mut std::collections::HashMap<String, ScrollbarBaseStyle>,
    layout_tree: &mut crate::layout::layout_tree::LayoutTree,
) {
    let inset_len_px = |v: &taffy::style::LengthPercentageAuto| {
        crate::layout::lpa_length_px(*v)
            .filter(|px| px.is_finite())
            .unwrap_or(0.0)
            .max(0.0)
    };
    let Some(fid) = el.focus_id.clone() else {
        return;
    };
    let fid = fid.as_str();

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
    let can_scroll_x = allow_x && cw > vw + 0.5;
    let can_scroll_y = allow_y && ch > vh + 0.5;
    let axis_allows = if axis == "x" { allow_x } else { allow_y };
    let scrollable = if axis == "x" {
        can_scroll_x
    } else {
        can_scroll_y
    };

    // Visibility mode: only Auto uses cubic fade show/hide (CSS opacity on track).
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
        crate::style::use_ui_transition_cubic(&format!("scrollbar_spring_{rest}:{axis}"), 0.0)
            .peek()
            .clamp(0.0, 1.0)
    };

    // Recover base thickness from the track's max cross constraint only. Using `layout_bounds`
    // here caused cross-axis jitter whenever unrelated flex reflows nudged the resolved size.
    let base_thickness = match axis {
        "x" => crate::layout::dim_length_px(el.style.max_size.height)
            .filter(|px| px.is_finite() && *px > 1.0)
            .unwrap_or(SCROLLBAR_DEFAULT_CROSS_PX),
        _ => crate::layout::dim_length_px(el.style.max_size.width)
            .filter(|px| px.is_finite() && *px > 1.0)
            .unwrap_or(SCROLLBAR_DEFAULT_CROSS_PX),
    };

    // Collapse when:
    // - axis overflow mode disallows scrollbars, or
    // - explicitly `never`, or
    // - `auto` but not scrollable (no hidden hit-slab reservation for non-scrollable axis).
    if !axis_allows || vis == "never" || (vis != "always" && !scrollable) {
        el.style.size.width = Dimension::length(0.0);
        el.style.size.height = Dimension::length(0.0);
        el.style.border_width = 0.0;
        if let Some(thumb) = el.children.first_mut() {
            thumb.style.size.width = Dimension::length(0.0);
            thumb.style.size.height = Dimension::length(0.0);
            thumb.style.border_width = 0.0;
            layout_tree.update_taffy_node_style(thumb);
        }
        // Keep Taffy node in lockstep with collapsed track style.
        layout_tree.update_taffy_node_style(el);
        return;
    }

    // Overlay tracks keep the length Taffy already resolved from absolute insets.
    // Outside flex lanes stretch to 100% of their slot so the corner is an L-join
    // (the vertical lane owns the leftover square) instead of a dead gutter.
    let inset_x = use_signal(&scroll_scrollbar_inset_x_signal_key(&rest), || {
        "inside".to_string()
    })
    .peek();
    let inset_y = use_signal(&scroll_scrollbar_inset_y_signal_key(&rest), || {
        "inside".to_string()
    })
    .peek();
    let this_inset_outside = if axis == "x" {
        inset_x.eq_ignore_ascii_case("outside")
    } else {
        inset_y.eq_ignore_ascii_case("outside")
    };

    // Keep constant cross-size; show/hide is alpha-only (`v` = visibility cubic current).
    // Do not clamp alpha to 0 while 0 < v < 0.01: during fade-in `v` stays tiny for several
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

    let track_main = {
        let laid = if axis == "x" {
            el.layout_bounds[2]
        } else {
            el.layout_bounds[3]
        };
        if laid > 1.0 {
            laid
        } else {
            main_available
        }
    };

    match axis {
        "x" => {
            el.style.size.width = if this_inset_outside {
                Dimension::percent(1.0)
            } else {
                Dimension::auto()
            };
            el.style.size.height = Dimension::length(cross);
        }
        _ => {
            el.style.size.width = Dimension::length(cross);
            el.style.size.height = if this_inset_outside {
                Dimension::percent(1.0)
            } else {
                Dimension::auto()
            };
        }
    }
    // The scrollbar builder sets `min_w / min_h` at build time to the initial viewport
    // dimensions. These stale constraints override our runtime `size` — Taffy computes
    // `max(min_size, size)`. Clear them so the explicit size above is authoritative.
    el.style.min_size.width = Dimension::auto();
    el.style.min_size.height = Dimension::auto();

    // Update thumb geometry (assumes first child is thumb).
    {
        let inner = &mut *el.inner;
        let Some(thumb) = inner.children.first_mut() else {
            return;
        };

        // Cache base style once per scroll-area + axis (we will apply alpha every frame).
        let style_key = format!("{rest}:{axis}");
        if let Entry::Vacant(e) = base_style.entry(style_key.clone()) {
            let track_fades = inner
                .attrs
                .get(ATTR_SCROLLBAR_TRACK_FADES)
                .is_none_or(|s| s != "0");
            let mut track_bg = inner.style.bg_color;
            let mut thumb_bg = thumb.style.bg_color;
            let mut track_border = inner.style.border_color;
            let mut thumb_border = thumb.style.border_color;
            let track_border_width = inner.style.border_width;
            let thumb_border_width = thumb.style.border_width;
            normalize_scrollbar_base_for_auto(
                vis.as_str(),
                track_fades,
                track_border_width,
                thumb_border_width,
                &mut track_bg,
                &mut track_border,
                &mut thumb_bg,
                &mut thumb_border,
            );
            e.insert(ScrollbarBaseStyle {
                track_bg,
                thumb_bg,
                track_border,
                thumb_border,
                track_border_width,
                thumb_border_width,
                track_fades,
                thumb_border_fades: thumb_border_width > 0.0,
            });
        }
        if let Some(bs) = base_style.get(&style_key).copied() {
            let mut tc = bs.track_bg;
            let mut th = bs.thumb_bg;
            let mut tbc = bs.track_border;
            let mut thbc = bs.thumb_border;
            let visible_alpha = v.clamp(0.0, 1.0);
            apply_scrollbar_visible_alpha(
                visible_alpha,
                bs.track_fades,
                bs.thumb_border_fades,
                &mut tc,
                &mut th,
                &mut tbc,
                &mut thbc,
            );
            inner.style.bg_color = tc;
            thumb.style.bg_color = th;
            inner.style.border_color = tbc;
            thumb.style.border_color = thbc;
            // 边框宽不参与 alpha：否则每帧改变盒模型，无法在无 Taffy compute 的 overlay-only 帧保持几何稳定。
            inner.style.border_width = bs.track_border_width;
            thumb.style.border_width = bs.thumb_border_width;
        }

        {
            match axis {
                "x" => {
                    let track = track_main.max(1.0);
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
                    thumb.style.size.width = Dimension::length(tw);
                    thumb.style.size.height = Dimension::length(thumb_cross);
                    thumb.style.margin.left = taffy::style::LengthPercentageAuto::length(ml);
                    thumb.style.margin.top = taffy::style::LengthPercentageAuto::length(inset);
                }
                _ => {
                    let track = track_main.max(1.0);
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
                    thumb.style.size.width = Dimension::length(thumb_cross);
                    thumb.style.size.height = Dimension::length(th);
                    thumb.style.margin.top = taffy::style::LengthPercentageAuto::length(mt);
                    thumb.style.margin.left = taffy::style::LengthPercentageAuto::length(inset);
                }
            }
            layout_tree.update_taffy_node_style(thumb);
        }
    }

    // Sync track style after all geometry mutations to avoid one-frame stale layout,
    // which is especially visible in nested smooth-scroll scenarios.
    layout_tree.update_taffy_node_style(el);
}

fn update_scrollbar_elements(
    el: &mut Element,
    base_style: &mut std::collections::HashMap<String, ScrollbarBaseStyle>,
    layout_tree: &mut crate::layout::layout_tree::LayoutTree,
    mode: ScrollbarUpdateMode,
    active_drag_rest: Option<&str>,
) {
    enum Phase {
        Enter,
        Leave,
    }
    fn subtree_contains_scroll_rest(el: &Element, target_rest: &str) -> bool {
        crate::core::tree::find_in_tree(el, |n| {
            n.focus_id
                .as_deref()
                .and_then(|fid| fid.strip_prefix(SCROLL_VIEWPORT_FOCUS_PREFIX))
                .is_some_and(|rest| rest == target_rest)
        })
        .is_some()
    }
    let mut stack: Vec<(*mut Element, Phase)> = vec![(el as *mut Element, Phase::Enter)];
    while let Some((ptr, phase)) = stack.pop() {
        // SAFETY: exclusive walk of the tree passed to this function.
        let el = unsafe { &mut *ptr };
        match phase {
            Phase::Enter => {
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
                        continue;
                    }
                    if let Some(fid) = el.focus_id.as_deref() {
                        if let Some(rest) = fid.strip_prefix(SCROLL_VIEWPORT_FOCUS_PREFIX) {
                            let sx =
                                use_signal(&scroll_offset_x_signal_key(rest), || 0.0_f32).peek();
                            let sy =
                                use_signal(&scroll_offset_y_signal_key(rest), || 0.0_f32).peek();
                            el.scroll_offset = [sx, sy];
                            if el.scroll_viewport_subtree_count > 1 {
                                let n = el.children.len();
                                for i in (0..n).rev() {
                                    stack.push((
                                        std::ptr::addr_of_mut!(el.children[i]),
                                        Phase::Enter,
                                    ));
                                }
                            }
                            continue;
                        }
                    }
                    let n = el.children.len();
                    for i in (0..n).rev() {
                        stack.push((std::ptr::addr_of_mut!(el.children[i]), Phase::Enter));
                    }
                    continue;
                }

                if let Some(target_rest) = active_drag_rest {
                    if let Some(fid) = el.focus_id.as_deref() {
                        if let Some(rest) = fid.strip_prefix(SCROLL_VIEWPORT_FOCUS_PREFIX) {
                            if rest != target_rest && !subtree_contains_scroll_rest(el, target_rest)
                            {
                                let sx = use_signal(&scroll_offset_x_signal_key(rest), || 0.0_f32)
                                    .peek();
                                let sy = use_signal(&scroll_offset_y_signal_key(rest), || 0.0_f32)
                                    .peek();
                                el.scroll_offset = [sx, sy];
                                continue;
                            }
                        }
                    }
                }

                stack.push((ptr, Phase::Leave));
                let n = el.children.len();
                for i in (0..n).rev() {
                    let child = &el.children[i];
                    let descend = child.scroll_viewport_subtree_count > 0
                        || child.focus_id.as_deref().is_some_and(|f| {
                            f.starts_with(SCROLLBAR_THUMB_FOCUS_PREFIX)
                                || f.starts_with(SCROLLBAR_TRACK_FOCUS_PREFIX)
                                || f.starts_with(SCROLL_VIEWPORT_FOCUS_PREFIX)
                        });
                    if descend {
                        stack.push((std::ptr::addr_of_mut!(el.children[i]), Phase::Enter));
                    }
                }
            }
            Phase::Leave => {
                update_scrollbar_element_self(el, base_style, layout_tree);
            }
        }
    }
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
        let Some(target) = find_template_target_mut(root, template_id) else {
            return false;
        };
        inject_text_literals(target, text_literals)
    }

    fn inject_text_literals(target: &mut Element, text_literals: &[String]) -> bool {
        if text_literals.is_empty() {
            return false;
        }
        let total = count_text_nodes(target);
        if total != text_literals.len() {
            // Prevent cross-node corruption when extractor/node order differs.
            return false;
        }
        for (idx, text) in text_literals.iter().enumerate() {
            let _ = replace_text_at_index(target, idx, text);
        }
        true
    }

    #[allow(dead_code)]
    pub(super) fn inject_style_attrs_by_id(
        root: &mut Element,
        template_id: &str,
        attrs: &[crate::app::hot_reload::StyleAttr],
    ) -> bool {
        let Some(template_root) = find_template_target_mut(root, template_id) else {
            return false;
        };
        inject_style_attrs(template_root, attrs)
    }

    fn inject_style_attrs(
        template_root: &mut Element,
        attrs: &[crate::app::hot_reload::StyleAttr],
    ) -> bool {
        if attrs.is_empty() {
            return false;
        }
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
                            target.text_style.color = Some(crate::core::tree::parse_text_color(v));
                            changed = true;
                        }
                        "text_size" | "font_size" => {
                            if let Some(px) = parse_length_px(v) {
                                target.text_style.size = Some(px);
                                changed = true;
                            }
                        }
                        "font_weight" | "text_weight" => {
                            if let Ok(w) = v.parse::<u16>() {
                                target.text_style.weight = Some(w.clamp(100, 900));
                                changed = true;
                            }
                        }
                        "text_decoration" | "text-decoration" => {
                            let raw = v.to_ascii_lowercase();
                            if raw.contains("none") {
                                target.text_style.underline = Some(false);
                                target.text_style.overline = Some(false);
                                target.text_style.line_through = Some(false);
                            } else {
                                let has_line_token = raw.contains("underline")
                                    || raw.contains("overline")
                                    || raw.contains("line-through")
                                    || raw.contains("linethrough")
                                    || raw.contains("strikethrough");
                                if has_line_token {
                                    target.text_style.underline = Some(raw.contains("underline"));
                                    target.text_style.overline = Some(raw.contains("overline"));
                                    target.text_style.line_through = Some(
                                        raw.contains("line-through")
                                            || raw.contains("linethrough")
                                            || raw.contains("strikethrough"),
                                    );
                                }
                                target.text_style.decoration_style =
                                    Some(if raw.contains("double") {
                                        crate::style::TextDecorationStyle::Double
                                    } else if raw.contains("dotted") {
                                        crate::style::TextDecorationStyle::Dotted
                                    } else if raw.contains("dashed") {
                                        crate::style::TextDecorationStyle::Dashed
                                    } else if raw.contains("wavy") {
                                        crate::style::TextDecorationStyle::Wavy
                                    } else {
                                        crate::style::TextDecorationStyle::Solid
                                    });
                            }
                            changed = true;
                        }
                        "text_decoration_line" | "textDecorationLine" | "text-decoration-line" => {
                            let raw = v.to_ascii_lowercase();
                            if raw.contains("none") {
                                target.text_style.underline = Some(false);
                                target.text_style.overline = Some(false);
                                target.text_style.line_through = Some(false);
                            } else {
                                target.text_style.underline = Some(raw.contains("underline"));
                                target.text_style.overline = Some(raw.contains("overline"));
                                target.text_style.line_through = Some(
                                    raw.contains("line-through")
                                        || raw.contains("linethrough")
                                        || raw.contains("strikethrough"),
                                );
                            }
                            changed = true;
                        }
                        "text_decoration_style"
                        | "textDecorationStyle"
                        | "text-decoration-style" => {
                            let s = v.to_ascii_lowercase();
                            target.text_style.decoration_style = Some(match s.as_str() {
                                "double" => crate::style::TextDecorationStyle::Double,
                                "dotted" => crate::style::TextDecorationStyle::Dotted,
                                "dashed" => crate::style::TextDecorationStyle::Dashed,
                                "wavy" => crate::style::TextDecorationStyle::Wavy,
                                _ => crate::style::TextDecorationStyle::Solid,
                            });
                            changed = true;
                        }
                        "text_decoration_color"
                        | "textDecorationColor"
                        | "text-decoration-color" => {
                            target.text_style.decoration_color =
                                Some(crate::core::tree::parse_text_color(v));
                            target.text_style.decoration_color_specified = true;
                            changed = true;
                        }
                        "text_decoration_thickness"
                        | "textDecorationThickness"
                        | "text-decoration-thickness" => {
                            if let Some(px) = parse_length_px(v) {
                                target.text_style.decoration_thickness = Some(px.max(0.0));
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
                                let lp = taffy::style::LengthPercentage::length(px);
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
                                let lp = taffy::style::LengthPercentageAuto::length(px);
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
                                let lp = taffy::style::LengthPercentage::length(px);
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

    /// Runtime wrappers the CLI RSX DFS never counts (`<ScrollArea>` body is just
    /// `<VStack>…`, but `From<ScrollAreaBuilder>` inserts a `scroll:` viewport and
    /// `__rui_*_scrollbar_*` chrome). Walking those as real elements shifted
    /// `element_index` so `text_size` landed on VStack instead of Label.
    fn hot_reload_style_omit(el: &Element) -> bool {
        if let Some(fid) = el.focus_id.as_deref() {
            if fid.starts_with(crate::components::scrollbar::SCROLLBAR_THUMB_FOCUS_PREFIX)
                || fid.starts_with(crate::components::scrollbar::SCROLLBAR_TRACK_FOCUS_PREFIX)
            {
                return true;
            }
        }
        el.template_id.as_deref().is_some_and(|id| {
            id.starts_with("__rui_v_scrollbar_") || id.starts_with("__rui_h_scrollbar_")
        })
    }

    fn hot_reload_style_transparent(el: &Element) -> bool {
        el.focus_id.as_deref().is_some_and(|f| {
            f.starts_with(crate::core::scroll_viewport::SCROLL_VIEWPORT_FOCUS_PREFIX)
        })
    }

    fn find_element_at_index_inner<'a>(
        el: &'a mut Element,
        target_idx: usize,
        current_idx: &mut usize,
    ) -> Option<&'a mut Element> {
        enum Kind {
            Full,
            CountLeaf,
        }
        let mut stack: Vec<(*mut Element, Kind)> = vec![(el as *mut Element, Kind::Full)];
        let mut found: Option<*mut Element> = None;
        while let Some((ptr, kind)) = stack.pop() {
            // SAFETY: exclusive subtree walk of `el`.
            let node = unsafe { &mut *ptr };
            let expand = match kind {
                Kind::Full => {
                    if hot_reload_style_omit(node) {
                        continue;
                    }
                    if hot_reload_style_transparent(node) {
                        let n = node.children.len();
                        for i in (0..n).rev() {
                            stack.push((std::ptr::addr_of_mut!(node.children[i]), Kind::Full));
                        }
                        continue;
                    }
                    true
                }
                Kind::CountLeaf => false,
            };
            if *current_idx == target_idx {
                found = Some(ptr);
                break;
            }
            *current_idx += 1;
            if !expand {
                continue;
            }
            let n = node.children.len();
            for i in (0..n).rev() {
                if hot_reload_style_omit(&node.children[i]) {
                    continue;
                }
                if hot_reload_style_transparent(&node.children[i]) {
                    stack.push((std::ptr::addr_of_mut!(node.children[i]), Kind::Full));
                    continue;
                }
                if node.children[i].template_id.is_some() {
                    stack.push((std::ptr::addr_of_mut!(node.children[i]), Kind::CountLeaf));
                    continue;
                }
                stack.push((std::ptr::addr_of_mut!(node.children[i]), Kind::Full));
            }
        }
        found.map(|ptr| unsafe { &mut *ptr })
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
            return Some(taffy::style::Dimension::auto());
        }
        if let Some(v) = s.strip_suffix('%') {
            return v
                .trim()
                .parse::<f32>()
                .ok()
                .map(|n| taffy::style::Dimension::percent(n / 100.0));
        }
        parse_length_px(s).map(taffy::style::Dimension::length)
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

    #[allow(dead_code)]
    pub(super) fn find_by_template_id_mut<'a>(
        el: &'a mut Element,
        id: &str,
    ) -> Option<&'a mut Element> {
        find_template_target_mut(el, id)
    }

    #[allow(dead_code)]
    pub(super) fn find_by_template_id_and_index_mut<'a>(
        el: &'a mut Element,
        id: &str,
        _index: usize,
    ) -> Option<&'a mut Element> {
        find_template_target_mut(el, id)
    }

    fn find_template_target_mut<'a>(el: &'a mut Element, id: &str) -> Option<&'a mut Element> {
        let path = crate::app::hot_reload::find_best_template_path(el, id)?;
        crate::app::hot_reload::follow_template_path_mut(el, &path)
    }

    fn path_component_match_fuzzy(a: &str, b: &str) -> bool {
        let a = crate::app::hot_reload::parse_template_id(a);
        let b = crate::app::hot_reload::parse_template_id(b);
        let ap = a.path;
        let bp = b.path;
        if ap == bp {
            return true;
        }
        ap.ends_with(&bp) || bp.ends_with(&ap)
    }

    #[allow(dead_code)]
    pub(super) fn inject_text_replacements_by_id(
        root: &mut Element,
        template_id: &str,
        replacements: &[crate::app::hot_reload::TextReplacement],
    ) -> bool {
        let Some(target) = find_template_target_mut(root, template_id) else {
            return false;
        };
        inject_text_replacements(target, replacements)
    }

    fn inject_text_replacements(
        target: &mut Element,
        replacements: &[crate::app::hot_reload::TextReplacement],
    ) -> bool {
        if replacements.is_empty() {
            return false;
        }
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
        crate::core::tree::walk_within_template_mut(el, |node| {
            if let Some(text) = node.text.as_mut() {
                if text == from {
                    *text = to.to_string();
                    return true;
                }
            }
            false
        })
    }

    fn replace_text_at_index(el: &mut Element, target_index: usize, to: &str) -> bool {
        let mut idx = 0usize;
        crate::core::tree::walk_within_template_mut(el, |node| {
            if let Some(text) = node.text.as_mut() {
                if idx == target_index {
                    *text = to.to_string();
                    return true;
                }
                idx += 1;
            }
            false
        })
    }

    fn replace_text_at_index_checked(
        el: &mut Element,
        target_index: usize,
        from: &str,
        to: &str,
    ) -> bool {
        let mut idx = 0usize;
        let mut ok = false;
        crate::core::tree::walk_within_template_mut(el, |node| {
            if let Some(text) = node.text.as_mut() {
                if idx == target_index {
                    if text == from {
                        *text = to.to_string();
                        ok = true;
                    }
                    return true;
                }
                idx += 1;
            }
            false
        });
        ok
    }

    fn count_text_occurrences(el: &Element, needle: &str) -> usize {
        let mut n = 0usize;
        crate::core::tree::walk_within_template(el, |node| {
            if node.text.as_deref() == Some(needle) {
                n += 1;
            }
        });
        n
    }

    pub(super) fn upsert_overrides(
        store: &mut Vec<crate::app::hot_reload::TemplateFragment>,
        incoming: &[crate::app::hot_reload::TemplateFragment],
    ) {
        for tpl in incoming {
            if let Some(existing) = store.iter_mut().find(|v| {
                if v.id == tpl.id || crate::app::hot_reload::template_id_matches(&v.id, &tpl.id) {
                    return true;
                }
                path_component_match_fuzzy(&v.id, &tpl.id) && v.index == tpl.index
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
            let Some(target) = find_template_target_mut(root, &tpl.id) else {
                continue;
            };
            if target
                .template_hash
                .as_deref()
                .is_some_and(|h| h == tpl.hash.as_str())
            {
                //Same hash as CLI: the tree comes from the newly compiled ui!, do not overwrite by index (easy to misalign with `{expr}` slot).
                continue;
            }
            let _ = inject_style_attrs(target, &tpl.style_attrs);
            // 从 ui!() 回调得到的新树是编译期字符串；热更的增量 replacement 的 `from` 是「上一版热更」
            // 的 literal，与当前树中的字往往对不上，单靠 replacement 会在 resize/信号重绘时失效。
            // When persistence contains a complete `text_literals` snapshot, DFS writes it back in one go (consistent with inject_hot_reload_partial).
            let mut text_ok = false;
            if !tpl.text_literals.is_empty() {
                text_ok = inject_text_literals(target, &tpl.text_literals);
            }
            if !text_ok {
                let _ = inject_text_replacements(target, &tpl.text_replacements);
            }
        }
    }

    pub(super) fn derive_replacements_from_previous(
        previous: &[crate::app::hot_reload::TemplateFragment],
        current: &crate::app::hot_reload::TemplateFragment,
    ) -> Vec<crate::app::hot_reload::TextReplacement> {
        let Some(prev) = previous.iter().find(|v| {
            v.id == current.id
                || crate::app::hot_reload::template_id_matches(&v.id, &current.id)
                || (v.index == current.index && path_component_match_fuzzy(&v.id, &current.id))
        }) else {
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
        crate::core::tree::walk_within_template_mut(el, |node| {
            if let Some(text) = node.text.as_mut() {
                if *cursor < literals.len() {
                    *text = literals[*cursor].clone();
                    *cursor += 1;
                }
            }
            false
        });
    }

    fn count_text_nodes(el: &Element) -> usize {
        let mut n = 0usize;
        crate::core::tree::walk_within_template(el, |node| {
            if node.text.is_some() {
                n += 1;
            }
        });
        n
    }

    #[cfg(test)]
    mod inject_match_tests {
        use super::*;
        use crate::core::tree::Element;

        fn tpl(
            id: &str,
            hash: &str,
            literals: &[&str],
        ) -> crate::app::hot_reload::TemplateFragment {
            crate::app::hot_reload::TemplateFragment {
                id: id.to_string(),
                hash: hash.to_string(),
                len: 1,
                index: 0,
                text_literals: literals.iter().map(|s| (*s).to_string()).collect(),
                text_replacements: vec![],
                style_attrs: vec![],
            }
        }

        #[test]
        fn apply_hits_matching_line_not_first_same_column() {
            let mut root = Element::new().children(vec![
                Element::new()
                    .template_id("examples/gallery/src/main.rs:188:9")
                    .template_hash("old-a")
                    .text("first"),
                Element::new()
                    .template_id("examples/gallery/src/main.rs:400:9")
                    .template_hash("old-b")
                    .text("second"),
            ]);
            apply_templates_to_tree(
                &mut root,
                &[tpl(
                    "d:/projects/rui/examples/gallery/src/main.rs:400:9",
                    "new-b",
                    &["second-new"],
                )],
            );
            assert_eq!(root.children[0].text.as_deref(), Some("first"));
            assert_eq!(root.children[1].text.as_deref(), Some("second-new"));
        }

        #[test]
        fn apply_follows_small_line_shift_to_same_column_block() {
            let mut root = Element::new().children(vec![
                Element::new()
                    .template_id("examples/gallery/src/main.rs:188:9")
                    .template_hash("old-a")
                    .text("first"),
                Element::new()
                    .template_id("examples/gallery/src/main.rs:400:9")
                    .template_hash("old-b")
                    .text("second"),
            ]);
            apply_templates_to_tree(
                &mut root,
                &[tpl(
                    "d:/projects/rui/examples/gallery/src/main.rs:402:9",
                    "new-b",
                    &["second-new"],
                )],
            );
            assert_eq!(root.children[0].text.as_deref(), Some("first"));
            assert_eq!(root.children[1].text.as_deref(), Some("second-new"));
        }

        #[test]
        fn apply_updates_label_inside_switch_reactive_island() {
            // Gallery overwrites the page `ui!` template_id with `App:section_body`.
            // Hot reload must still find the island via the preserved source id.
            let mut island = Element::new()
                .template_id("examples/gallery/App:section_body")
                .template_hash("old-nav")
                .children(vec![
                    Element::new().children(vec![Element::new().text("导航")])
                ]);
            island.set_attr(
                crate::app::reactive::ATTR_SOURCE_TEMPLATE_ID,
                "examples/gallery/src/main.rs:4334:21",
            );
            let mut root = Element::new().children(vec![
                Element::new()
                    .template_id("examples/gallery/src/main.rs:100:9")
                    .text("chrome"),
                island,
            ]);
            apply_templates_to_tree(
                &mut root,
                &[tpl(
                    "d:/projects/rui/examples/gallery/src/main.rs:4334:21",
                    "new-nav",
                    &["导航改"],
                )],
            );
            assert_eq!(root.children[0].text.as_deref(), Some("chrome"));
            assert_eq!(
                root.children[1].children[0].children[0].text.as_deref(),
                Some("导航改")
            );
            assert_eq!(
                root.children[1].template_id.as_deref(),
                Some("examples/gallery/App:section_body"),
                "island identity must stay stable after inject"
            );
        }

        #[test]
        fn apply_twice_keeps_island_children_and_updates_text() {
            let mut island = Element::new()
                .template_id("examples/gallery/App:section_body")
                .template_hash("old-nav")
                .children(vec![
                    Element::new().children(vec![Element::new().text("导航")])
                ]);
            island.set_attr(
                crate::app::reactive::ATTR_SOURCE_TEMPLATE_ID,
                "examples/gallery/src/main.rs:4334:21",
            );
            let nested = Element::new()
                .template_id("examples/gallery/src/main.rs:4179:5")
                .text("navbar");
            island.children[0].children.push(nested);
            let mut root = Element::new().children(vec![island]);

            apply_templates_to_tree(
                &mut root,
                &[tpl(
                    "examples/gallery/src/main.rs:4334:21",
                    "nav-1",
                    &["导航改"],
                )],
            );
            apply_templates_to_tree(
                &mut root,
                &[tpl(
                    "examples/gallery/src/main.rs:4334:21",
                    "nav-2",
                    &["导航再改"],
                )],
            );
            assert_eq!(
                root.children[0].children[0].children[0].text.as_deref(),
                Some("导航再改")
            );
            assert_eq!(
                root.children[0].children[0].children.len(),
                2,
                "second inject must not drop nested showcase children"
            );
            assert_eq!(
                root.children[0].children[0].children[1].text.as_deref(),
                Some("navbar")
            );
        }

        #[test]
        fn apply_style_skips_scroll_viewport_and_scrollbar_chrome() {
            // CLI RSX DFS: 0 ScrollArea, 1 VStack, 2 Label (text_size).
            // Runtime `From<ScrollAreaBuilder>` wraps the body in a `scroll:` viewport
            // and appends scrollbar siblings — those must not consume element_index.
            let label = Element::new()
                .text("导航")
                .text_size(1.25 * crate::core::tree::CSS_ROOT_FONT_SIZE_PX);
            let vstack = Element::new().children(vec![label]);
            let viewport = Element::new()
                .focus_id("scroll:scroll-navigation")
                .children(vec![vstack]);
            let vbar = Element::new()
                .template_id("__rui_v_scrollbar_scroll-navigation")
                .focus_id("scrollbar-track:scroll-navigation:y");
            let hbar = Element::new()
                .template_id("__rui_h_scrollbar_scroll-navigation")
                .focus_id("scrollbar-track:scroll-navigation:x");
            let mut island = Element::new()
                .template_id("examples/gallery/App:section_body")
                .template_hash("old-nav")
                .children(vec![viewport, vbar, hbar]);
            island.set_attr(
                crate::app::reactive::ATTR_SOURCE_TEMPLATE_ID,
                "examples/gallery/src/main.rs:4334:21",
            );
            let mut root = Element::new().children(vec![island]);

            let mut patch = tpl(
                "examples/gallery/src/main.rs:4334:21",
                "new-nav-size",
                &["导航"],
            );
            patch.style_attrs = vec![crate::app::hot_reload::StyleAttr {
                element_index: 2,
                name: "text_size".into(),
                value: "2rem".into(),
            }];
            apply_templates_to_tree(&mut root, &[patch]);

            let vstack = &root.children[0].children[0].children[0];
            let label = &vstack.children[0];
            let expected = 2.0 * crate::core::tree::CSS_ROOT_FONT_SIZE_PX;
            assert_eq!(
                label.text_style.size_resolved(),
                expected,
                "text_size must land on Label (CLI index 2), not the viewport/VStack wrapper"
            );
            assert_eq!(
                vstack.text_style.size, None,
                "VStack must not receive the Label style patch"
            );
            assert_eq!(
                label.text_style.size,
                Some(expected),
                "explicit size must survive cascade and not be overwritten"
            );
        }

        #[test]
        fn deferred_override_applies_when_island_later_mounts() {
            // CLI only sends the changed `ui!`. If that page is not mounted, the
            // live tree has no match — the patch must still apply when the
            // `switch_reactive` island is created later from compile-time source.
            let patch = tpl(
                "d:/projects/rui/examples/gallery/src/main.rs:4334:21",
                "hmr-nav-new",
                &["导航改"],
            );

            let mut live = Element::new().children(vec![Element::new()
                .template_id("examples/gallery/src/main.rs:100:9")
                .template_hash("chrome")
                .text("chrome")]);
            apply_templates_to_tree(&mut live, std::slice::from_ref(&patch));
            assert_eq!(
                live.children[0].text.as_deref(),
                Some("chrome"),
                "unrelated mounted template must stay untouched"
            );

            let mut island = Element::new()
                .template_id("examples/gallery/App:section_body")
                .template_hash("compile-time-old")
                .children(vec![Element::new()
                    .focus_id("scroll:scroll-navigation")
                    .children(vec![
                        Element::new().children(vec![Element::new().text("导航")])
                    ])]);
            island.set_attr(
                crate::app::reactive::ATTR_SOURCE_TEMPLATE_ID,
                "examples/gallery/src/main.rs:4334:21",
            );
            apply_templates_to_tree(&mut island, std::slice::from_ref(&patch));
            assert_eq!(
                island.children[0].children[0].children[0].text.as_deref(),
                Some("导航改"),
                "entering the page must replay the stored hot-reload literals"
            );
        }
    }
} // mod hot_reload_inject

#[cfg(test)]
mod text_shadow_draw_slot_tests {
    use super::{text_shadow_draw_slot, TextShadowDrawSlot};

    #[test]
    fn page_stack_bands_require_per_band_shadow_slot() {
        // Regression: drawing all text-shadows before page_stack band fills makes them
        // invisible under opaque card backgrounds (gallery Text-shadow section).
        assert_eq!(
            text_shadow_draw_slot(true),
            TextShadowDrawSlot::PerBandAfterRects
        );
        assert_eq!(
            text_shadow_draw_slot(false),
            TextShadowDrawSlot::AfterPageFills
        );
    }
}

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
mod text_scroll_cache_tests {
    use super::AppState;
    use crate::core::tree::TextLayoutInfo;
    use crate::renderer::GlyphRasterMode;
    use crate::style::TextStyle;

    fn node_at_y(y: f32) -> TextLayoutInfo {
        TextLayoutInfo {
            text: "hello".into(),
            shape_suffix: None,
            spans: None,
            template_id: None,
            x: 8.0,
            y,
            width: 40.0,
            height: 16.0,
            clip_bounds: Some([0.0, 0.0, 400.0, 300.0]),
            style: TextStyle::default(),
            prepared: None,
            glyph_fx: None,
            shimmer: None,
            paint_scale: 1.0,
            paint_transform: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
            paint_origin: [8.0, y],
            glyph_raster: GlyphRasterMode::Coverage,
        }
    }

    #[test]
    fn idle_cache_tolerates_tiny_pos_noise() {
        let a = [node_at_y(100.0)];
        let b = [node_at_y(100.01)];
        assert!(AppState::text_nodes_equal(&a, &b));
    }

    #[test]
    fn position_drift_quarter_pixel_busts_cache() {
        let a = [node_at_y(100.0)];
        let b = [node_at_y(100.25)];
        assert!(
            !AppState::text_nodes_equal(&a, &b),
            "0.25px scroll must not reuse glyphs parked behind rects (avatar label offset)"
        );
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

    #[test]
    fn minimize_zero_size_does_not_pin_layout() {
        let inner = PhysicalSize::new(800, 600);
        let target = AppState::resolve_surface_size(inner, Some(PhysicalSize::new(0, 0)));
        assert_eq!(
            (target.width, target.height),
            (800, 600),
            "0×0 minimize must fall back to inner so restore can rebuild"
        );
    }
}

#[cfg(test)]
mod present_fingerprint_tests {
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
        let reused = super::current_underline_layer(true, true, &[], &[], &last);
        assert_eq!(reused.len(), 1);
        let left = super::current_underline_layer(true, false, &[], &[], &last);
        assert!(
            left.is_empty(),
            "leave (not reuse) must drop decorations so node present can erase"
        );
        let hovered = super::current_underline_layer(true, false, &[line], &[], &[]);
        assert_eq!(hovered.len(), 1);
    }
}
