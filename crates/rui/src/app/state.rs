use std::borrow::Cow;
#[cfg(test)]
#[path = "state_present_tests.rs"]
mod present_pixel_tests;
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
            apply_scrollbar_visible_alpha, horizontal_lane_side_inset,
            normalize_scrollbar_base_for_auto, scrollbar_min_thumb_along_track,
            vertical_scrollbar_occupies_lane, ATTR_H_LANE_INSET, ATTR_H_LANE_V_CROSS,
            ATTR_SCROLLBAR_TRACK_FADES, SCROLLBAR_EDGE_PAD, SCROLLBAR_THUMB_FOCUS_PREFIX,
            SCROLLBAR_TRACK_FOCUS_PREFIX,
        },
    },
    core::reactivity::use_signal,
    core::scroll_viewport::{
        scroll_content_h_signal_key, scroll_content_w_signal_key, scroll_offset_x_signal_key,
        scroll_offset_y_signal_key, scroll_overflow_x_signal_key, scroll_overflow_y_signal_key,
        scroll_scrollbar_inset_x_signal_key, scroll_scrollbar_inset_y_signal_key,
        scroll_scrollbar_y_side_signal_key, scroll_viewport_h_signal_key,
        scroll_viewport_w_signal_key, SCROLL_VIEWPORT_FOCUS_PREFIX,
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
#[cfg(debug_assertions)]
static PRESENT_HOLE_CHROME: AtomicUsize = AtomicUsize::new(0);
#[cfg(debug_assertions)]
static PRESENT_HOLE_BAND: AtomicUsize = AtomicUsize::new(0);
#[cfg(debug_assertions)]
static PRESENT_SDF_BG: AtomicUsize = AtomicUsize::new(0);

fn present_hole_counts() -> (usize, usize, usize) {
    #[cfg(debug_assertions)]
    {
        (
            PRESENT_HOLE_CHROME.load(Ordering::Relaxed),
            PRESENT_HOLE_BAND.load(Ordering::Relaxed),
            PRESENT_SDF_BG.load(Ordering::Relaxed),
        )
    }
    #[cfg(not(debug_assertions))]
    {
        (0, 0, 0)
    }
}

fn reset_present_hole_counts() {
    #[cfg(debug_assertions)]
    {
        PRESENT_HOLE_CHROME.store(0, Ordering::Relaxed);
        PRESENT_HOLE_BAND.store(0, Ordering::Relaxed);
        PRESENT_SDF_BG.store(0, Ordering::Relaxed);
    }
}

fn note_present_hole_chrome() {
    #[cfg(debug_assertions)]
    PRESENT_HOLE_CHROME.fetch_add(1, Ordering::Relaxed);
}

fn note_present_hole_band() {
    #[cfg(debug_assertions)]
    PRESENT_HOLE_BAND.fetch_add(1, Ordering::Relaxed);
}

fn note_present_sdf_bg() {
    #[cfg(debug_assertions)]
    PRESENT_SDF_BG.fetch_add(1, Ordering::Relaxed);
}

/// TextBounds clips prepared runs; the pass scissor also has to restrict them
/// to pixels whose underlay was repaired. Retained coverage must stay intact.
fn draw_captured_page_glyphs<'pass>(
    renderer: &'pass RuiTextRenderer,
    pass: &mut wgpu::RenderPass<'pass>,
    width: u32,
    height: u32,
    cleared: &[[f32; 4]],
    portals: &[[f32; 4]],
) {
    for rect in super::draw_cull::capture_glyph_scissors_excluding_portals_physical(
        cleared, portals, width, height,
    ) {
        let (x, y, w, h) = super::damage::physical_xywh_to_scissor(rect, width, height)
            .expect("capture scissors are clamped integer rectangles");
        pass.set_scissor_rect(x, y, w, h);
        renderer.render(pass);
        super::frame_stats::note_scene_batch(super::frame_stats::SceneKind::Text, 1);
    }
}

/// How much scroll-related work to do before layout each frame.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScrollbarUpdateMode {
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
    pub(super) window: Arc<dyn Window>,
    /// Retained so secondary windows can `create_surface` on the same GPU backend.
    pub(super) instance: wgpu::Instance,
    pub(super) adapter: wgpu::Adapter,
    pub(super) gpu_caps: crate::renderer::GpuCaps,
    pub(super) device: Arc<wgpu::Device>,
    pub(super) queue: Arc<wgpu::Queue>,
    pub(super) surface: wgpu::Surface<'static>,
    pub(super) config: wgpu::SurfaceConfiguration,
    pub(super) size: winit::dpi::PhysicalSize<u32>,
    pub(super) pipeline: RuiPipeline,
    textured_pipeline: TexturedQuadPipeline,
    #[cfg(feature = "scene3d")]
    scene3d: Option<crate::renderer::scene3d::Scene3dGpu>,
    shader: Option<crate::renderer::shader_leaf::ShaderGpu>,
    path2d: Option<crate::renderer::path_pipeline::PathGpu>,
    backdrop_blur: BackdropBlurPipeline,
    effect_pass: crate::renderer::EffectPassGpu,
    css_filter_pass: crate::renderer::CssFilterPassGpu,
    compute: crate::renderer::ComputePipelines,
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
    /// Layered jump-chrome dest-reset restores wrapping underlay glyphs *under* the
    /// overlay slot. Sharing [`Self::capture_text_renderer`] would flush them after
    /// Latest and stamp the thread on top of the pill.
    capture_underlay_text_renderer: RuiTextRenderer,
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
    /// Taffy node ids that need `mark_dirty` + style/measure sync on the next LayoutChanged /
    /// in-place StructureChanged frame (reconcile / bind paths). Cleared after apply.
    /// Full Taffy rebuild is only cold start / portal hoist.
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
    render_prof_layout_stamp_ms: f32,
    render_prof_skip_n: f32,
    render_prof_patch_n: f32,
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
    /// Last presented hoisted overlay stamps (`rui_portal_key` hash, layout box).
    last_overlay_stamps: Vec<(u64, [f32; 4])>,
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
    /// This `render()` submitted a swapchain frame. Skip-present still replaces
    /// `text_nodes` / element `scroll_offset`; IME must not follow that unpresented
    /// geometry (gallery forms: page frozen, candidate HWND walks up).
    pub(super) gpu_presented_this_frame: bool,
    /// Hash of all `scroll:` viewport offsets after prepass.
    last_scroll_offset_fp: u64,
    last_scroll_offsets: HashMap<String, [u32; 2]>,
    /// Scroll moved: PartialLoad the viewport layer (Clear scissor, then redraw).
    scroll_layer_clear: bool,
    /// Last presented page-node boxes. Node-present dest-resets `now` only —
    /// if a named dirty moved, Load keeps the previous pixels.
    last_present_node_bounds: Vec<(u64, [f32; 4])>,
    /// Unpadded union of viewports whose offset changed this frame (text clip).
    scroll_text_clip: Option<[f32; 4]>,
    /// Dual-axis blit: second L strip (logical). `None` for single-axis / full viewport Clear.
    scroll_text_clip_second: Option<[f32; 4]>,
    /// Scroll: copy overlap in retain; Clear only the newly exposed strip(s).
    scroll_strip_blit: Vec<super::damage::ScrollStripBlitJob>,
    /// Extra Clear+redraw holes on a blit frame (sticky / overlay / named / nested layer).
    scroll_blit_pins: Vec<[f32; 4]>,
    /// Overlay closed or shrank: Clear the leftover scissor then redraw page
    /// (retain still holds composited overlay pixels).
    overlay_restore_clear: bool,
    /// Swapchain uses compositor alpha (transparent / blur windows).
    compositor_alpha: bool,
    /// True when the swapchain uses Pre/Post-multiplied compositor alpha.
    pub(super) composition_swapchain: bool,
    /// Creation chrome; re-applied after surface configure (Windows DWM punch-through).
    chrome: super::window_chrome::WindowOptions,
    /// `backdrop_blur.full` still holds last local frost. Skip recapture when
    /// PartialLoad damage misses the host (gallery hover on another card).
    local_backdrop_valid: bool,
}

/// `queue.write_buffer` is executed before render pass during submit; the same buffer is written to offset 0 multiple times.
/// Only the last time will be left (as soon as the pie chart enters the viewport, the polyline instance is covered into a pie chart rectangle). It must be packaged once and drawn according to instance range.
pub(super) fn upload_textured_instance_buffer(
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

pub(super) fn draw_packed_textured_runs<'pass>(
    pass: &mut wgpu::RenderPass<'pass>,
    pipeline: &'pass TexturedQuadPipeline,
    instance_buffer: &'pass wgpu::Buffer,
    image_textures: &'pass HashMap<String, super::image_registry::GpuImageTexture>,
    scaled_tex: &[ScaledTexturedDraw],
    start: usize,
    end: usize,
    cull: Option<super::draw_cull::CullRects>,
) {
    let end = end.min(scaled_tex.len());
    let mut ti = start.min(end);
    while ti < end {
        if let Some(damage) = cull {
            if !damage.hits_textured(&scaled_tex[ti]) {
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
        super::frame_stats::note_scene_batch(super::frame_stats::SceneKind::Surface, run as u32);
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
    node_capture: Option<(&[InstanceInput], &[[f32; 4]])>,
) -> Vec<InstanceInput> {
    if let Some((captured, refreshed)) = node_capture {
        let mut out: Vec<_> = last_presented
            .iter()
            .copied()
            .filter(|line| {
                let cx = line.rect_pos[0] + line.rect_size[0] * 0.5;
                let cy = line.rect_pos[1] + line.rect_size[1] * 0.5;
                !refreshed
                    .iter()
                    .any(|r| cx >= r[0] && cx < r[0] + r[2] && cy >= r[1] && cy < r[1] + r[3])
            })
            .collect();
        out.extend_from_slice(captured);
        return out;
    }
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

/// Include decorations outside the glyph box when replacing a captured caption.
fn captured_text_decoration_bounds(
    n: &crate::layout::layout_tree::TextLayoutInfo,
    ui_scale: f32,
) -> [f32; 4] {
    let paint_scale = if n.paint_scale.is_finite() && n.paint_scale > 0.0 {
        n.paint_scale
    } else {
        1.0
    };
    let scale = ui_scale * paint_scale;
    let thickness = n
        .style
        .decoration_thickness
        .filter(|v| *v > 0.0)
        .unwrap_or_else(|| (n.style.size_resolved() * ui_scale * 0.07).max(1.0))
        * paint_scale;
    let offset = n.style.underline_offset_resolved() * scale;
    [
        n.x * ui_scale,
        n.y * ui_scale - thickness - (-offset).max(0.0),
        n.width * scale,
        n.height * scale + offset.abs() + thickness * 3.0,
    ]
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
    cull: Option<super::draw_cull::CullRects>,
) {
    for sd in draws {
        if let Some(damage) = cull {
            if !damage.hits_aabb(sd.rect) {
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
    cull_phys: Option<super::draw_cull::CullRects>,
    /// Scroll viewport layer: Load retain then Clear each exposed strip (not the L AABB).
    clear_damage: bool,
    /// Paint-only: per-node scissor, no cross-node AABB cull.
    use_node_present: bool,
}

/// Overlay glyphon ignores the occupancy wgpu scissor. Clip each band to its
/// portal stamp so accordion-up cannot leave HEX/RGB ink after chrome shrinks.
fn overlay_band_text_for_prepare<'a>(
    nodes: &'a [crate::core::tree::TextLayoutInfo],
    lo: usize,
    hi: usize,
    stamp_key: u64,
    stamps: &[(u64, [f32; 4])],
) -> Cow<'a, [crate::core::tree::TextLayoutInfo]> {
    let lo = lo.min(nodes.len());
    let hi = hi.min(nodes.len()).max(lo);
    let slice = &nodes[lo..hi];
    if stamp_key == 0 {
        return Cow::Borrowed(slice);
    }
    match super::damage::overlay_stamp_box(stamps, stamp_key) {
        Some(stamp) => Cow::Owned(super::draw_cull::clip_text_nodes_to_rect(slice, stamp)),
        None => Cow::Borrowed(slice),
    }
}

fn instances_for_upload<'a>(
    slice: &'a [InstanceInput],
    cull: Option<super::draw_cull::CullRects>,
) -> Cow<'a, [InstanceInput]> {
    match cull {
        Some(d) => Cow::Owned(super::draw_cull::filter_instances(slice, d)),
        None => Cow::Borrowed(slice),
    }
}

fn instance_range_hits(
    instances: &[InstanceInput],
    start: u32,
    count: u32,
    cull: super::draw_cull::CullRects,
) -> bool {
    let start = start as usize;
    let end = (start + count as usize).min(instances.len());
    let start = start.min(end);
    instances[start..end]
        .iter()
        .any(|inst| cull.hits_instance(inst))
}

fn draw_instance_range(
    pass: &mut wgpu::RenderPass<'_>,
    pipeline: &RuiPipeline,
    instances: &[InstanceInput],
    start: u32,
    count: u32,
    cull: Option<super::draw_cull::CullRects>,
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

/// Node-present ranged SDF: copy the instance range and its clip_polys so
/// `instance_index` 0 matches `clip_polys[0]` (GLES firstInstance is relative).
fn draw_sdf_range_with_clip<'a>(
    pass: &mut wgpu::RenderPass<'a>,
    pipeline: &'a RuiPipeline,
    device: &wgpu::Device,
    dummy: &'a wgpu::BindGroup,
    instances: &[InstanceInput],
    clip_polys: &[crate::style::GpuClipPoly],
    start: u32,
    count: u32,
    cull: Option<super::draw_cull::CullRects>,
) {
    if count == 0 {
        return;
    }
    let lo = start as usize;
    let hi = (lo + count as usize).min(instances.len());
    if lo >= hi {
        return;
    }
    if let Some(cull) = cull {
        // Parent collect ranges include every descendant quad. Underlay restore
        // of an absolute track used to upload each of those ops (forms page:
        // 808 bind groups, ~170ms encode) and only then test the scissor.
        let mut hit_inst = Vec::new();
        let mut hit_poly = Vec::new();
        for i in lo..hi {
            if !cull.hits_instance(&instances[i]) {
                continue;
            }
            hit_inst.push(instances[i]);
            hit_poly.push(
                clip_polys
                    .get(i)
                    .copied()
                    .unwrap_or(crate::style::GpuClipPoly::NONE),
            );
        }
        if hit_inst.is_empty() {
            return;
        }
        draw_uploaded_sdf_with_clip(pass, pipeline, device, dummy, &hit_inst, &hit_poly, None);
        return;
    }
    let inst_slice = &instances[lo..hi];
    let mut polys: Vec<crate::style::GpuClipPoly> = if lo < clip_polys.len() {
        clip_polys[lo..hi.min(clip_polys.len())].to_vec()
    } else {
        Vec::new()
    };
    while polys.len() < inst_slice.len() {
        polys.push(crate::style::GpuClipPoly::NONE);
    }
    draw_uploaded_sdf_with_clip(pass, pipeline, device, dummy, inst_slice, &polys, None);
}

/// One clip-poly bind group for an already-gathered instance list.
///
/// Per-slot uploads in a thumb hole created a GL buffer and bind group for every
/// ancestor whose box contains the thumb (inline hue/alpha). Painter order is
/// preserved by the caller flushing before a texture that actually hits.
fn draw_uploaded_sdf_with_clip<'a>(
    pass: &mut wgpu::RenderPass<'a>,
    pipeline: &'a RuiPipeline,
    device: &wgpu::Device,
    dummy: &'a wgpu::BindGroup,
    instances: &[InstanceInput],
    polys: &[crate::style::GpuClipPoly],
    cull: Option<super::draw_cull::CullRects>,
) {
    if instances.is_empty() {
        return;
    }
    let polys_owned = if polys.len() >= instances.len() {
        polys[..instances.len()].to_vec()
    } else {
        let mut padded = polys.to_vec();
        while padded.len() < instances.len() {
            padded.push(crate::style::GpuClipPoly::NONE);
        }
        padded
    };
    note_present_sdf_bg();
    let inst_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Node present SDF range"),
        contents: bytemuck::cast_slice(instances),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let (_clip_buf, _clip_bg) =
        pipeline.bind_sdf_with_clip_polys(pass, device, dummy, &polys_owned);
    pass.set_vertex_buffer(0, pipeline.vertex_buffer.slice(..));
    pass.set_vertex_buffer(1, inst_buf.slice(..));
    pass.set_index_buffer(pipeline.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
    draw_instance_range(pass, pipeline, instances, 0, instances.len() as u32, cull);
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
    pass.draw_indexed(0..pipeline.num_indices, 0, 0..instances.len() as u32);
    super::frame_stats::note_scene_batch(
        super::frame_stats::SceneKind::Underline,
        instances.len() as u32,
    );
}

/// Primitive repair must cover every pixel in the rounded GPU scissor.
fn node_present_repair_cull_rect(scissor: (u32, u32, u32, u32)) -> [f32; 4] {
    let (x, y, w, h) = scissor;
    [x as f32, y as f32, w as f32, h as f32]
}

/// Dest-reset that must fill `rect` opaquely. The SDF shader expands +2px AA;
/// drawing a quad the size of the scissor leaves a translucent fringe that
/// cuts page captions as a white frame. Inflate the instance so the fringe
/// sits outside the scissor.
fn draw_scissor_fill<'a>(
    pass: &mut wgpu::RenderPass<'a>,
    pipeline: &'a RuiPipeline,
    device: &wgpu::Device,
    rect: [f32; 4],
    clear: wgpu::Color,
) {
    draw_scissor_clear_ex(pass, pipeline, device, rect, clear, 8.0);
}

/// Attachment clears use linear RGB; SDF instances use display sRGB. Convert
/// before drawing a repair so it matches the pixels produced by FullClear.
fn draw_scissor_surface_clear<'a>(
    pass: &mut wgpu::RenderPass<'a>,
    pipeline: &'a RuiPipeline,
    device: &wgpu::Device,
    rect: [f32; 4],
    clear: wgpu::Color,
) {
    let rgb = crate::style::encode_linear_srgb_to_display_srgb([
        clear.r as f32,
        clear.g as f32,
        clear.b as f32,
    ]);
    draw_scissor_fill(
        pass,
        pipeline,
        device,
        rect,
        wgpu::Color {
            r: rgb[0] as f64,
            g: rgb[1] as f64,
            b: rgb[2] as f64,
            a: clear.a,
        },
    );
}

/// I-beam / extra carets. Drawn on the swapchain after retain blit so blink
/// cannot dest-reset Search (竖条 / 白线贯穿).
fn draw_input_caret_overlay(
    pass: &mut wgpu::RenderPass<'_>,
    pipeline: &RuiPipeline,
    device: &wgpu::Device,
    dummy: &wgpu::BindGroup,
    chrome: &[InstanceInput],
) {
    if chrome.is_empty() {
        return;
    }
    // Do not call `bind_sdf` here: it ties `&pipeline` to the pass lifetime, which
    // `present_retain.blit_to_surface` cannot name.
    pass.set_pipeline(&pipeline.render_pipeline);
    pass.set_bind_group(0, &pipeline.bind_group, &[]);
    pass.set_bind_group(1, dummy, &[]);
    pass.set_vertex_buffer(0, pipeline.vertex_buffer.slice(..));
    let buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Input Caret Overlay"),
        contents: bytemuck::cast_slice(chrome),
        usage: wgpu::BufferUsages::VERTEX,
    });
    pass.set_vertex_buffer(1, buf.slice(..));
    pass.set_index_buffer(pipeline.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
    pass.draw_indexed(0..pipeline.num_indices, 0, 0..chrome.len() as u32);
    super::frame_stats::note_scene_batch(super::frame_stats::SceneKind::Quad, 1);
}

fn draw_scissor_clear_ex<'a>(
    pass: &mut wgpu::RenderPass<'a>,
    pipeline: &'a RuiPipeline,
    device: &wgpu::Device,
    rect: [f32; 4],
    clear: wgpu::Color,
    inflate: f32,
) {
    let rect = if inflate > 0.0 {
        super::draw_cull::padded_xywh(rect, inflate)
    } else {
        rect
    };
    let inst = InstanceInput {
        rect_pos: [rect[0], rect[1]],
        rect_size: [rect[2], rect[3]],
        // Src-over with a=0 (compositor-alpha TRANSPARENT) is a no-op and leaves
        // overlay keyframes in retain. Always replace the hole, then restore page.
        color: [clear.r as f32, clear.g as f32, clear.b as f32, 1.0],
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
    cull: Option<super::draw_cull::CullRects>,
) {
    use crate::layout::layout_tree::PaintOp;
    let hi = hi.min(ops.len());
    let lo = lo.min(hi);
    for op in &ops[lo..hi] {
        match *op {
            PaintOp::Rects { start, count } if count > 0 => {
                // Bind is a GL pipeline switch. A thumb hole used to do it for every
                // page op, then cull to zero draws (inline hue/alpha encode).
                if let Some(damage) = cull {
                    if !instances.is_empty()
                        && !instance_range_hits(instances, start, count, damage)
                    {
                        continue;
                    }
                }
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
    cull: Option<super::draw_cull::CullRects>,
    paint_caret_chrome: bool,
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

    if paint_caret_chrome && band.chrome.0 < band.chrome.1 {
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
            pass.draw_indexed(0..pipeline.num_indices, 0, 0..chrome_slice.len() as u32);
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
    compute: Option<&mut crate::renderer::ComputePipelines>,
    instances: &[InstanceInput],
    capture_region: Option<(u32, u32, u32, u32)>,
) {
    let dummy = pipeline.dummy_backdrop();
    backdrop.ensure_size(device, surface_w, surface_h, surface_format);
    let capture_cull = capture_region.map(|sc| {
        super::draw_cull::CullRects::one([sc.0 as f32, sc.1 as f32, sc.2 as f32, sc.3 as f32])
    });

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
        if let Some((x, y, w, h)) = capture_region {
            scene_pass.set_scissor_rect(x, y, w, h);
        }

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
                instances,
                capture_cull,
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
                instances,
                capture_cull,
                false,
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

    backdrop.blur_with_region(
        device,
        queue,
        &mut cap_encoder,
        radius_px,
        compute,
        capture_region,
    );
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
    instances: &[InstanceInput],
    cull: Option<super::draw_cull::CullRects>,
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
            instances,
            cull,
        );
    } else if band.instance.0 > 0 {
        pipeline.bind_sdf(pass, true, backdrop);
        pass.set_vertex_buffer(0, pipeline.vertex_buffer.slice(..));
        pass.set_vertex_buffer(1, instance_buffer.slice(..));
        pass.set_index_buffer(pipeline.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
        draw_instance_range(pass, pipeline, instances, 0, band.instance.0 as u32, cull);
        if band.textured.0 > 0 {
            draw_packed_textured_runs(
                pass,
                textured_pipeline,
                textured_instance_buffer,
                image_textures,
                scaled_tex,
                0,
                band.textured.0,
                cull,
            );
        }
    }
    let _ = (scaled_input_chrome, device);
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

/// Local `mount=` frost is independent of fullscreen modal frost.
///
/// Nested CSS `backdrop-filter` stacks: the modal samples the already-frosted
/// page, it does not replace the local scrim. Skipping this when a ViewportModal
/// is open was the "弹窗冲掉局部模糊" bug.
fn should_capture_page_stack_local_backdrops(
    allow_primitives: bool,
    has_page_stack_bands: bool,
) -> bool {
    allow_primitives && has_page_stack_bands
}

fn page_stack_band_backdrop<'a>(
    band_i: usize,
    band_backdrop_idx: &[Option<usize>],
    live_backdrops: &'a [wgpu::BindGroup],
    dummy: &'a wgpu::BindGroup,
) -> &'a wgpu::BindGroup {
    band_backdrop_idx
        .get(band_i)
        .copied()
        .flatten()
        .and_then(|idx| live_backdrops.get(idx))
        .unwrap_or(dummy)
}

/// Physical scissor for local `backdrop-filter` capture: the host overflow clip of
/// blur instances in `band`. `instances` must already be in framebuffer pixels
/// (the layout buffer is `take`n into `scaled_instances` before capture).
/// Expanded by the blur kernel so edge taps do not sample empty texels.
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

fn local_backdrop_kernel_pad_px(radius_px: f32) -> u32 {
    (radius_px * 3.0).ceil() as u32 + 2
}

fn expand_physical_scissor(
    sc: (u32, u32, u32, u32),
    pad: u32,
    surface_w: u32,
    surface_h: u32,
) -> (u32, u32, u32, u32) {
    let x0 = sc.0.saturating_sub(pad);
    let y0 = sc.1.saturating_sub(pad);
    let x1 = (sc.0.saturating_add(sc.2).saturating_add(pad)).min(surface_w);
    let y1 = (sc.1.saturating_add(sc.3).saturating_add(pad)).min(surface_h);
    if x1 <= x0 || y1 <= y0 {
        sc
    } else {
        (x0, y0, x1 - x0, y1 - y0)
    }
}

fn physical_scissor_aabb(sc: (u32, u32, u32, u32)) -> [f32; 4] {
    [sc.0 as f32, sc.1 as f32, sc.2 as f32, sc.3 as f32]
}

fn logical_rect_hits_physical_scissor(
    damage_logical: [f32; 4],
    ui_scale: f32,
    host: (u32, u32, u32, u32),
) -> bool {
    let s = ui_scale.max(0.0);
    let phys = [
        damage_logical[0] * s,
        damage_logical[1] * s,
        damage_logical[2] * s,
        damage_logical[3] * s,
    ];
    !crate::renderer::aabbs_disjoint(phys, physical_scissor_aabb(host))
}

/// Recapture when the pixels *behind* the local scrim may have changed.
/// Hover / overlay ticks elsewhere keep last frame's frost.
fn local_backdrop_must_recapture(
    has_valid_blur: bool,
    use_partial_damage: bool,
    scroll_source_moved: bool,
    overlay_restore_clear: bool,
    host: Option<(u32, u32, u32, u32)>,
    dirty_logical: &[[f32; 4]],
    ui_scale: f32,
) -> bool {
    if !has_valid_blur || !use_partial_damage || scroll_source_moved || overlay_restore_clear {
        return true;
    }
    let Some(host) = host else {
        return true;
    };
    dirty_logical
        .iter()
        .any(|d| logical_rect_hits_physical_scissor(*d, ui_scale, host))
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
            stamp_key: 0,
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
    fn local_frost_still_captures_when_modal_underlay_will_blur() {
        assert!(
            should_capture_page_stack_local_backdrops(true, true),
            "page-stack frost must run even if a ViewportModal underlay will also blur"
        );
        assert!(!should_capture_page_stack_local_backdrops(false, true));
        assert!(!should_capture_page_stack_local_backdrops(true, false));
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

    #[test]
    fn kernel_pad_covers_three_sigma() {
        assert_eq!(local_backdrop_kernel_pad_px(2.0), 8);
        assert_eq!(local_backdrop_kernel_pad_px(12.0), 38);
    }

    #[test]
    fn expand_scissor_clamps_to_surface() {
        let expanded = expand_physical_scissor((10, 10, 20, 20), 8, 800, 600);
        assert_eq!(expanded, (2, 2, 36, 36));
        let edge = expand_physical_scissor((0, 0, 10, 10), 8, 800, 600);
        assert_eq!(edge, (0, 0, 18, 18));
    }

    #[test]
    fn skip_recapture_when_partial_damage_misses_host() {
        let host = Some((100, 200, 220, 140));
        assert!(
            !local_backdrop_must_recapture(
                true,
                true,
                false,
                false,
                host,
                &[[10.0, 10.0, 40.0, 20.0]],
                1.0,
            ),
            "hover elsewhere must keep last frost"
        );
        assert!(
            local_backdrop_must_recapture(
                true,
                true,
                false,
                false,
                host,
                &[[120.0, 220.0, 10.0, 10.0]],
                1.0,
            ),
            "damage on the host must recapture"
        );
        assert!(
            local_backdrop_must_recapture(true, true, true, false, host, &[], 1.0),
            "scroll blit moves pixels under the scrim"
        );
        assert!(
            local_backdrop_must_recapture(false, true, false, false, host, &[], 1.0),
            "first frame has no cached frost"
        );
        assert!(
            !local_backdrop_must_recapture(true, true, false, false, host, &[], 1.0),
            "overlay-only PartialLoad with empty dirty keeps frost"
        );
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

    /// Maximize/restore plants `needs_text_settle` + `request_redraw` inside
    /// `RedrawRequested`. Windows coalesces that redraw; without Poll the settle
    /// frame never runs (fast-layout ghosts) and CSS clocks stop until the next wheel.
    pub(super) fn needs_resize_followup(&self) -> bool {
        self.last_resized_event.is_some() || self.needs_text_settle
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
        // Cached DLs keep absolute glyph Y. Maximize can otherwise replay the
        // pre-grow list (shimmer label ghosting onto the next section).
        self.layout_tree.invalidate_collect_skip();
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
    #[cfg(feature = "a11y")]
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
    async fn acquire_gpu(
        window: &Arc<dyn Window>,
        force_software: bool,
        compositor_alpha: bool,
    ) -> (
        wgpu::Instance,
        wgpu::Surface<'static>,
        wgpu::Adapter,
        wgpu::Device,
        wgpu::Queue,
    ) {
        async fn try_backend(
            window: &Arc<dyn Window>,
            backends: wgpu::Backends,
            name: &str,
            force_fallback: bool,
        ) -> Option<(
            wgpu::Instance,
            wgpu::Surface<'static>,
            wgpu::Adapter,
            wgpu::Device,
            wgpu::Queue,
        )> {
            let instance = crate::renderer::wgpu_compat::instance(backends);
            let Ok(surface) = instance.create_surface(window.clone()) else {
                log::debug!("backend {name}: no surface");
                return None;
            };
            let adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::LowPower,
                    compatible_surface: Some(&surface),
                    force_fallback_adapter: force_fallback,
                    ..Default::default()
                })
                .await
                .ok()?;
            let required_limits = crate::renderer::limits_for(&adapter);
            match adapter
                .request_device(&crate::renderer::wgpu_compat::device_desc(
                    None,
                    required_limits,
                    wgpu::MemoryHints::MemoryUsage,
                ))
                .await
            {
                Ok((device, queue)) => {
                    if force_fallback {
                        log::warn!(
                            "falling back to software adapter: {}",
                            adapter.get_info().name
                        );
                    } else {
                        let caps = crate::renderer::probe_gpu_caps(&adapter);
                        log::info!(
                            "gpu backend: {name} ({}) compute: {}",
                            adapter.get_info().name,
                            crate::renderer::compute_yes_no(caps)
                        );
                    }
                    Some((instance, surface, adapter, device, queue))
                }
                Err(e) => {
                    log::warn!("backend {name}: device failed: {e}");
                    None
                }
            }
        }

        let candidates = crate::platform::gpu_backend_candidates(compositor_alpha);
        if !force_software {
            for (backends, name) in &candidates {
                if let Some(gpu) = try_backend(window, *backends, name, false).await {
                    return gpu;
                }
            }
        }

        // WARP / llvmpipe last: only DX12 and Vulkan expose a fallback adapter.
        for (backends, name) in [
            (wgpu::Backends::DX12, "dx12-software"),
            (wgpu::Backends::VULKAN, "vulkan-software"),
        ] {
            if let Some(gpu) = try_backend(window, backends, name, true).await {
                return gpu;
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
        window: Arc<dyn Window>,
        chrome: super::window_chrome::WindowOptions,
    ) -> Self {
        let compositor_alpha = chrome.compositor_alpha();
        let mut root_element = Element::new().w("100%").h("100%");
        root_element.set_attr(Self::BOOTSTRAP_ROOT_ATTR, "1");
        let size = window.surface_size();
        let force_software = std::env::var("RUI_FORCE_SOFTWARE")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);

        // Backend choice dominates resident memory: on Windows the D3D12 and Vulkan
        // user-mode drivers commit ~80 MB of private heaps at device creation, while
        // the OpenGL ICD commits ~5 MB for the same clear-screen workload. 2D UI does
        // not need what DX12 buys, so GL leads and the others stay as fallbacks.
        let (instance, surface, adapter, device, queue) = if compositor_alpha {
            if let Some(gpu) = super::glass_gpu::try_acquire(&window, &chrome) {
                (
                    gpu.instance,
                    gpu.surface,
                    gpu.adapter,
                    gpu.device,
                    gpu.queue,
                )
            } else {
                let (i, s, a, d, q) =
                    Self::acquire_gpu(&window, force_software, compositor_alpha).await;
                (i, s, a, Arc::new(d), Arc::new(q))
            }
        } else {
            let (i, s, a, d, q) =
                Self::acquire_gpu(&window, force_software, compositor_alpha).await;
            (i, s, a, Arc::new(d), Arc::new(q))
        };
        crate::renderer::wgpu_compat::install_device_error_handlers(&device);
        let text_backend = if adapter.get_info().device_type == wgpu::DeviceType::Cpu {
            TextRendererBackend::CpuTextHybrid
        } else {
            TextRendererBackend::Gpu
        };
        log::warn!("text renderer backend: {:?}", text_backend);

        let mut gpu_caps = crate::renderer::probe_gpu_caps(&adapter);
        let compute = crate::renderer::ComputePipelines::new(&device, &mut gpu_caps);

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
        super::window_chrome::apply_created_window(window.as_ref(), &chrome, composition_swapchain);

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
        let capture_text_renderer =
            RuiTextRenderer::new_sharing_resources(&device, &queue, &config, &text_renderer);
        let capture_underlay_text_renderer =
            RuiTextRenderer::new_sharing_resources(&device, &queue, &config, &text_renderer);
        let overlay_text_renderer =
            RuiTextRenderer::new_sharing_resources(&device, &queue, &config, &text_renderer);
        log::debug!("new_bootstrap: after text_renderer");

        Self {
            window,
            instance,
            adapter,
            gpu_caps,
            device,
            queue,
            surface,
            config,
            size,
            pipeline,
            textured_pipeline,
            #[cfg(feature = "scene3d")]
            scene3d: None,
            shader: None,
            path2d: None,
            backdrop_blur,
            effect_pass,
            css_filter_pass,
            compute,
            present_retain,
            image_textures: crate::renderer::TexturePool::new(),
            textured_instance_buffer,
            textured_instance_capacity: 256,
            instance_buffer,
            instance_buffer_capacity: scaled_instances.len(),
            layout_tree,
            text_renderer,
            capture_text_renderer,
            capture_underlay_text_renderer,
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
            render_prof_layout_stamp_ms: 0.0,
            render_prof_skip_n: 0.0,
            render_prof_patch_n: 0.0,
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
            last_overlay_stamps: Vec::new(),
            last_overlay_boxes: Vec::new(),
            overlay_occupied: None,
            last_portal_layer_damage: None,
            has_presented_frame: false,
            last_gpu_frame_fp: 0,
            gpu_presented_this_frame: true,
            last_scroll_offset_fp: 0,
            last_scroll_offsets: HashMap::new(),
            last_present_node_bounds: Vec::new(),
            scroll_layer_clear: false,
            scroll_text_clip: None,
            scroll_text_clip_second: None,
            scroll_strip_blit: Vec::new(),
            scroll_blit_pins: Vec::new(),
            overlay_restore_clear: false,
            compositor_alpha,
            composition_swapchain,
            chrome,
            local_backdrop_valid: false,
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
                        [el.scroll_offset[0].to_bits(), el.scroll_offset[1].to_bits()],
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
                // Spawn: no prev clip to pin. Do not abort siblings that do have metrics.
                if !prev.contains_key(id) {
                    continue;
                }
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

    /// Fall back to a full viewport-layer Clear (overlap copy not safe this frame).
    fn abort_scroll_strip_blit(&mut self) {
        let jobs = std::mem::take(&mut self.scroll_strip_blit);
        self.scroll_blit_pins.clear();
        if jobs.is_empty() {
            return;
        }
        for job in jobs {
            self.union_damage(job.plan.viewport);
            self.scroll_text_clip = Some(match self.scroll_text_clip {
                Some(prev) => super::damage::union_rects(prev, job.plan.viewport),
                None => job.plan.viewport,
            });
        }
        self.scroll_text_clip_second = None;
    }

    fn find_viewport_element(&self, viewport_id: &str) -> Option<&Element> {
        let mut found: Option<&Element> = None;
        crate::core::tree::walk_elements(&self.root_element, |el| {
            if found.is_some() {
                return;
            }
            if let Some(fid) = el.focus_id.as_deref() {
                if let Some(rest) = fid.strip_prefix(SCROLL_VIEWPORT_FOCUS_PREFIX) {
                    if rest == viewport_id {
                        found = Some(el);
                    }
                }
            }
        });
        found
    }

    /// One or more viewports. Overlay / sticky / named paint become pin holes
    /// later (`collect_scroll_blit_pins`); nested inner viewports skip blit and
    /// return their bounds for a layer Clear pin (outer still blits). Overlapping
    /// siblings blit both; occupancy of the intersection is a pin. Despawned
    /// viewports pin last clip bounds.
    fn try_plan_scroll_strip_blits(
        &self,
        prev: &HashMap<String, [u32; 2]>,
        cur: &HashMap<String, [u32; 2]>,
    ) -> (Vec<super::damage::ScrollStripBlitJob>, Vec<[f32; 4]>) {
        let policy = super::damage::ScrollStripBlitPolicy {
            enabled: super::damage::scroll_strip_blit_enabled(),
            gpu_partial: Self::gpu_partial_damage_enabled(),
            retain_valid: self.present_retain.contents_valid(),
            force_full_redraw: self.force_full_redraw,
            named_paint_dirty: !self.damage_nodes.is_empty(),
            overlay: self.overlay_occupied.is_some() || !self.last_overlay_boxes.is_empty(),
        };
        if !policy.allows() {
            return (Vec::new(), Vec::new());
        }
        let Some(deltas) = super::damage::changed_viewport_scroll_deltas(prev, cur) else {
            return (Vec::new(), Vec::new());
        };
        let mut candidates = Vec::new();
        let mut layer_clear = Vec::new();
        for (id, delta) in deltas {
            let Some(bounds) = self
                .last_render_scroll_metrics
                .iter()
                .find(|(rest, _, _, _)| rest == &id)
                .map(|(_, b, _, _)| *b)
            else {
                continue;
            };
            let Some(plan) = super::damage::plan_scroll_strip_blit(bounds, delta) else {
                layer_clear.push(bounds);
                continue;
            };
            if super::damage::scroll_strip_copy_physical(
                &plan,
                self.ui_scale(),
                self.config.width,
                self.config.height,
            )
            .is_none()
            {
                layer_clear.push(bounds);
                continue;
            }
            match self.find_viewport_element(&id) {
                None => {
                    layer_clear.push(bounds);
                    continue;
                }
                Some(el) if !super::damage::scroll_strip_blit_viewport_axis_aligned(el) => {
                    layer_clear.push(bounds);
                    continue;
                }
                Some(_) => {}
            }
            candidates.push(super::damage::ScrollStripBlitJob {
                viewport_id: id,
                plan,
            });
        }
        let (kept, nested) = super::damage::partition_disjoint_blit_jobs(candidates);
        for job in nested {
            layer_clear.push(job.plan.viewport);
        }
        for pin in super::damage::overlapping_sibling_occupancy_pins(&kept) {
            layer_clear.push(pin);
        }
        for id in super::damage::despawned_viewport_ids(prev, cur) {
            if let Some(bounds) = self
                .last_render_scroll_metrics
                .iter()
                .find(|(rest, _, _, _)| rest == &id)
                .map(|(_, b, _, _)| *b)
            {
                layer_clear.push(bounds);
            }
        }
        if kept.is_empty() {
            (Vec::new(), Vec::new())
        } else {
            (kept, layer_clear)
        }
    }

    fn collect_scroll_blit_pins(
        &mut self,
        overlay_visual: Option<[f32; 4]>,
        node_paints: &[crate::layout::layout_tree::NodePaint],
    ) {
        if self.scroll_strip_blit.is_empty() {
            self.scroll_blit_pins.clear();
            return;
        }
        let viewports: Vec<[f32; 4]> = self
            .scroll_strip_blit
            .iter()
            .map(|j| j.plan.viewport)
            .collect();
        let ids: Vec<String> = self
            .scroll_strip_blit
            .iter()
            .map(|j| j.viewport_id.clone())
            .collect();
        let nested = std::mem::take(&mut self.scroll_blit_pins);
        let mut sticky = Vec::new();
        for id in &ids {
            if let Some(vp) = self.find_viewport_element(id) {
                sticky.extend(super::damage::sticky_layout_bounds(vp));
            }
        }
        sticky.extend(super::damage::overlay_scrollbar_scroll_blit_pins(
            &self.root_element,
            &ids,
            &viewports,
        ));
        let oof = super::damage::out_of_flow_scroll_blit_pins(&self.root_element, &ids, &viewports);
        let deltas: Vec<[f32; 2]> = self
            .scroll_strip_blit
            .iter()
            .map(|j| j.plan.delta)
            .collect();
        sticky.extend(oof.iter().copied());
        sticky.extend(super::damage::smear_out_of_flow_after_strip_blit(
            &oof, &viewports, &deltas,
        ));
        sticky.extend(super::damage::focused_input_caret_scroll_blit_pins(
            &self.root_element,
            &viewports,
        ));
        sticky.extend(super::damage::clip_culled_scroll_blit_pins(
            &self.last_present_node_bounds,
            self.layout_tree.last_clip_culled_paint_keys(),
            &viewports,
            &deltas,
        ));
        let named = super::draw_cull::named_scroll_blit_pins(
            &self.damage_nodes,
            &self.last_present_node_bounds,
            node_paints,
        );
        let overlay = if self.overlay_restore_clear {
            self.overlay_occupied.or(overlay_visual)
        } else {
            match (overlay_visual, self.overlay_occupied) {
                (Some(v), Some(o)) => Some(super::damage::union_rects(v, o)),
                (Some(v), None) => Some(v),
                (None, o) => o,
            }
        };
        self.scroll_blit_pins = super::damage::merge_scroll_blit_pins(
            overlay,
            &self.last_overlay_boxes,
            self.overlay_restore_clear,
            &named,
            &sticky,
            &nested,
            &viewports,
        );
        if self.overlay_restore_clear && self.last_overlay_boxes.is_empty() {
            self.overlay_occupied.take();
        }
        // Count actual disjoint coverage, not nested dirty-node records.
        if super::draw_cull::scroll_repair_rects_physical(
            &self.scroll_strip_blit,
            &self.scroll_blit_pins,
            self.ui_scale(),
        )
        .is_none()
        {
            self.abort_scroll_strip_blit();
        }
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
        self.scroll_text_clip_second = None;
        self.scroll_strip_blit.clear();
        self.scroll_blit_pins.clear();
        super::damage::clear_frame_paint_nodes();
        // Wave 52: Window tier must not fingerprint-replay the previous page DL.
        self.layout_tree.invalidate_collect_skip();
    }

    fn note_fit_relayout_followup(&mut self, page_geometry: bool, scroll_paint_only: bool) {
        if !crate::text::take_fit_needs_relayout() {
            return;
        }
        if scroll_paint_only {
            crate::text::note_fit_needs_relayout();
            return;
        }
        self.layout_status = crate::core::tree::LayoutStatus::LayoutChanged;
        if page_geometry {
            self.page_geom_dirty = true;
        }
        self.window.request_redraw();
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
        let cur_stamps = crate::layout::layout_tree::hoisted_portal_stamps(&self.root_element);
        // Nested menu changes can resize an idle parent portal. Its new footprint
        // needs its own restore even when the child already queued overlay damage.
        for (key, bounds) in
            super::damage::overlay_membership_paint_stamps(&self.last_overlay_stamps, &cur_stamps)
        {
            self.damage_nodes.push(super::damage::PaintDirty {
                key,
                bounds,
                kind: super::damage::PaintDirtyKind::Overlay,
            });
        }
        let collect_overlay_dirty = self
            .damage_nodes
            .iter()
            .any(|d| matches!(d.kind, super::damage::PaintDirtyKind::Overlay));
        let prev_occ = self.overlay_occupied;
        let prev_stamps = self.last_overlay_stamps.clone();
        let had_occupancy = prev_occ.is_some();
        let cur_boxes: Vec<[f32; 4]> = cur_stamps.iter().map(|(_, b)| *b).collect();
        let cur = crate::layout::layout_tree::hoisted_portal_damage(&self.root_element);
        let prev = self.last_portal_layer_damage;
        let membership = self.portal_membership_dirty;
        let measuring = crate::layout::layout_tree::hoisted_expand_measuring(&self.root_element);
        // Opacity-0 expand measure still emits instances at unconstrained content
        // height — that AABB is not occupancy. Stamps omit the measuring portal
        // (`hoisted_portal_paint_box`); always record the remaining keys.
        let current_paint = if measuring {
            None
        } else {
            overlay_visual.filter(|b| b[2] > 0.0 && b[3] > 0.0)
        };
        let needs_restore = super::damage::overlay_restore_needed(prev_occ, current_paint);
        let same_footprint = super::damage::overlay_same_footprint(prev_occ, current_paint, 2.0);
        let overlay_dirty = collect_overlay_dirty
            || membership
            || prev != cur
            || (needs_restore && overlay_present);
        let fade_replace = super::damage::overlay_fade_replace_needed(
            had_occupancy,
            overlay_present,
            overlay_dirty,
            needs_restore,
            same_footprint,
        );
        // A nested portal is composited over an already retained parent panel.
        // During its enter/exit animation the child shadow and the parent's
        // hover chrome share pixels, so a local retain replay can leave a stale
        // fringe or draw the trigger caption twice. Rebuild the frame while
        // that nested portal is changing; the next settled frame returns to
        // the normal partial-damage path.
        let nested_overlay_transition = (prev_stamps.len() > 1 || cur_stamps.len() > 1)
            && (membership || prev != cur || collect_overlay_dirty);
        if nested_overlay_transition
            && !self.force_full_redraw
            && !self.scroll_layer_clear
        {
            self.mark_full_redraw();
            self.overlay_restore_clear = false;
            self.overlay_occupied = current_paint;
        }
        // The last fade frame can leave the anchor's border under the portal
        // halo one shade darker than a fresh composite. Once the portal has no
        // remaining visual occupancy, rebuild the page and trigger together so
        // the close frame cannot retain that stale fringe.
        if needs_restore
            && cur_stamps.is_empty()
            && !overlay_present
            && !self.force_full_redraw
            && !self.scroll_layer_clear
        {
            self.mark_full_redraw();
        }
        // The first visible portal frame also paints its shadow over the
        // trigger halo. Rebuild the page once at that boundary so the shadow
        // is composited over a fresh trigger instead of the retained baseline.
        if overlay_present && !had_occupancy && !self.force_full_redraw && !self.scroll_layer_clear
        {
            self.mark_full_redraw();
        }
        if overlay_present
            && super::damage::overlay_requires_full_clear(overlay_present, cur, had_occupancy)
            && !measuring
            && !self.force_full_redraw
        {
            self.mark_full_redraw();
            self.overlay_restore_clear = false;
            self.overlay_occupied = current_paint;
        } else if fade_replace && !self.force_full_redraw && !self.scroll_layer_clear {
            self.overlay_restore_clear = false;
            let overlay_dirties: Vec<_> = self
                .damage_nodes
                .iter()
                .filter(|d| matches!(d.kind, super::damage::PaintDirtyKind::Overlay))
                .copied()
                .collect();
            if overlay_dirties.is_empty() {
                for (key, bounds) in
                    super::damage::overlay_membership_paint_stamps(&prev_stamps, &cur_stamps)
                {
                    for b in super::damage::overlay_mounted_replace_holes(
                        &prev_stamps,
                        &cur_stamps,
                        key,
                        Some(bounds),
                    ) {
                        self.push_overlay_restore_visual(key, b);
                    }
                    self.damage_nodes.push(super::damage::PaintDirty {
                        key,
                        bounds,
                        kind: super::damage::PaintDirtyKind::Overlay,
                    });
                }
            } else {
                for d in overlay_dirties {
                    for b in super::damage::overlay_mounted_replace_holes(
                        &prev_stamps,
                        &cur_stamps,
                        d.key,
                        Some(d.bounds),
                    ) {
                        self.push_overlay_restore_visual(d.key, b);
                    }
                }
            }
            self.push_overlay_restore_leftovers(&prev_stamps, &cur_stamps);
            if cur_stamps.is_empty() && !overlay_present {
                self.overlay_occupied.take();
            } else {
                self.overlay_occupied = current_paint.or(prev_occ.filter(|_| overlay_present));
            }
        } else if needs_restore {
            if !self.force_full_redraw && !self.scroll_layer_clear {
                self.push_overlay_restore_leftovers(&prev_stamps, &cur_stamps);
                if !collect_overlay_dirty && (overlay_present || !cur_stamps.is_empty()) {
                    for (key, bounds) in
                        super::damage::overlay_membership_paint_stamps(&prev_stamps, &cur_stamps)
                    {
                        self.damage_nodes.push(super::damage::PaintDirty {
                            key,
                            bounds,
                            kind: super::damage::PaintDirtyKind::Overlay,
                        });
                    }
                }
            }
            self.overlay_restore_clear = self
                .damage_nodes
                .iter()
                .any(|d| matches!(d.kind, super::damage::PaintDirtyKind::OverlayRestore));
            if cur_stamps.is_empty() && !overlay_present && !self.scroll_layer_clear {
                self.overlay_occupied.take();
            } else {
                self.overlay_occupied = current_paint.or(prev_occ.filter(|_| overlay_present));
            }
        } else {
            self.overlay_restore_clear = false;
            self.overlay_occupied = current_paint.or(prev_occ.filter(|_| overlay_present));
            if !self.force_full_redraw && !self.scroll_layer_clear && (membership || prev != cur) {
                for (key, bounds) in
                    super::damage::overlay_membership_paint_stamps(&prev_stamps, &cur_stamps)
                {
                    self.damage_nodes.push(super::damage::PaintDirty {
                        key,
                        bounds,
                        kind: super::damage::PaintDirtyKind::Overlay,
                    });
                }
            }
        }
        if !self.force_full_redraw && !self.scroll_layer_clear {
            let replay_dirties: Vec<_> = self
                .damage_nodes
                .iter()
                .filter(|d| d.kind == super::damage::PaintDirtyKind::Overlay)
                .map(|d| (d.key, super::damage::overlay_suffix_encode_bounds(d.bounds)))
                .collect();
            for (keys, dirty) in
                super::damage::merge_overlay_replay_dirties(&replay_dirties, &cur_stamps)
            {
                let key = keys.first().copied().unwrap_or(0);
                if let Some(bounds) = super::damage::overlay_replay_restore_bounds(
                    key,
                    dirty,
                    &cur_stamps,
                    self.overlay_occupied,
                ) {
                    self.push_overlay_restore_visual(key, bounds);
                }
            }
        }
        self.last_overlay_stamps = cur_stamps;
        self.last_overlay_boxes = cur_boxes;
        self.last_portal_layer_damage = cur;
        self.portal_membership_dirty = false;
        super::damage::sort_paint_dirty_nodes(&mut self.damage_nodes);
    }

    fn push_overlay_restore_leftovers(
        &mut self,
        prev_stamps: &[(u64, [f32; 4])],
        cur_stamps: &[(u64, [f32; 4])],
    ) {
        for (key, bounds) in super::damage::overlay_restore_leftover_coverage(
            prev_stamps,
            cur_stamps,
            self.overlay_occupied,
        ) {
            self.push_overlay_restore_visual(key, bounds);
        }
    }

    fn push_overlay_restore_visual(&mut self, key: u64, bounds: [f32; 4]) {
        if bounds[2] <= 0.0 || bounds[3] <= 0.0 {
            return;
        }
        self.damage_nodes.push(super::damage::PaintDirty {
            key,
            bounds,
            kind: super::damage::PaintDirtyKind::OverlayRestore,
        });
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
        let contributes_visual_ink = |i: &InstanceInput| {
            // The portal shell is often a full-window transparent layout box. It
            // must not become visual occupancy, while backdrop blur still is ink.
            super::draw_cull::instance_has_dest_ink(i) || i.color[3] > 0.0 || i.backdrop_blur > 0.0
        };
        for i in instances {
            if !contributes_visual_ink(i) {
                continue;
            }
            absorb_physical(&mut acc, super::draw_cull::instance_paint_bounds(i));
        }
        for i in chrome {
            if !contributes_visual_ink(i) {
                continue;
            }
            absorb_physical(&mut acc, super::draw_cull::instance_paint_bounds(i));
        }
        for d in textured {
            if d.opacity <= 0.0 {
                continue;
            }
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
    /// Overlay portal membership scissors to overlay bounds. Scroll uses viewport-layer
    /// Clear inside the scissor; a valid retain may strip-blit the overlap first,
    /// with pin holes for sticky / overlay / named paint.
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
        if !node_present && matches!(logical_policy, super::damage::FrameClearPolicy::PartialLoad) {
            scissor_physical = self.damage_rect.and_then(|damage| {
                super::damage::damage_to_scissor_physical(
                    damage,
                    if self.scroll_layer_clear {
                        0.0
                    } else {
                        super::draw_cull::DAMAGE_CULL_PAD_LOGICAL
                    },
                    ui_scale,
                    self.config.width,
                    self.config.height,
                )
            });
            if scissor_physical.is_none() {
                logical_policy = super::damage::FrameClearPolicy::FullClear;
            }
        }
        // Dual-axis / multi-viewport / pin AABB may equal the window.
        // Using it as damage_rect would `damage_covers_full` → FullClear and wipe
        // the copied overlap. Keep PartialLoad. A single wgpu scissor is only
        // safe for one exposed strip; pins/L-strips are per-rect (`cull_phys`).
        if !self.scroll_strip_blit.is_empty()
            && self.has_presented_frame
            && !self.force_full_redraw
            && !node_present
        {
            logical_policy = super::damage::FrameClearPolicy::PartialLoad;
            let jobs: Vec<_> = self.scroll_strip_blit.iter().map(|j| j.plan).collect();
            let scissor_src =
                super::damage::scroll_blit_pass_scissor_logical(&jobs, &self.scroll_blit_pins);
            scissor_physical = scissor_src.and_then(|damage| {
                super::damage::damage_to_scissor_physical(
                    damage,
                    0.0,
                    ui_scale,
                    self.config.width,
                    self.config.height,
                )
            });
            // `None` scissor is the multi-hole path (encode loops `cull_phys`),
            // not a FullClear — that would wipe the copy.
        }
        let keep_retain = Self::gpu_partial_damage_enabled();
        let want_gpu_partial =
            keep_retain && matches!(logical_policy, super::damage::FrameClearPolicy::PartialLoad);
        let use_partial_damage = want_gpu_partial && self.present_retain.contents_valid();
        let warm_retain = keep_retain && !self.present_retain.contents_valid();
        let draw_to_retain = keep_retain;
        let use_node_present = node_present && use_partial_damage;
        let cull_phys = if use_partial_damage && !use_node_present {
            if !self.scroll_strip_blit.is_empty() {
                super::draw_cull::scroll_repair_rects_physical(
                    &self.scroll_strip_blit,
                    &self.scroll_blit_pins,
                    ui_scale,
                )
            } else {
                // The viewport-clear fallback uses the same integer coverage
                // for clear, primitives and glyphs, including fractional DPI.
                scissor_physical.map(|(x, y, w, h)| {
                    super::draw_cull::CullRects::one([x as f32, y as f32, w as f32, h as f32])
                })
            }
        } else {
            None
        };
        GpuPresentPlan {
            logical_policy,
            scissor_physical,
            use_partial_damage,
            warm_retain,
            draw_to_retain,
            cull_phys,
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
        clip_polys: &[crate::style::GpuClipPoly],
        scaled_tex: &'pass [crate::renderer::ScaledTexturedDraw],
        scaled_input_chrome: &'pass [InstanceInput],
        overlay_inst_start: usize,
        overlay_tex_start: usize,
        overlay_paint_start: usize,
        allow_primitives: bool,
        clear_color: wgpu::Color,
        ui_scale: f32,
        overlay_band_ranges: &[crate::layout::layout_tree::OverlayBandRange],
        page_bands: &[crate::layout::layout_tree::OverlayBandRange],
        page_band_skip_text: &[bool],
        skip_page_text: bool,
        page_band_underlines: &[Vec<InstanceInput>],
        page_band_shadows: &[Vec<crate::renderer::prepare_text::ShadowTexDraw>],
        band_backdrop_idx: &[Option<usize>],
        live_backdrops: &'pass [wgpu::BindGroup],
        capture_text_ready: bool,
        capture_underlay_ready: bool,
        underline_instances: &[InstanceInput],
        prev_underline_instances: &[InstanceInput],
        caption_glyph_holes: &[[f32; 4]],
    ) {
        use super::damage::PaintDirtyKind;
        reset_present_hole_counts();
        let dummy = self.pipeline.dummy_backdrop();
        let pending_page_glyphs = capture_text_ready;
        let pending_underlay_glyphs = capture_underlay_ready;
        let mut overlay_dirties: Vec<(u64, [f32; 4])> = Vec::new();
        // Physical integer rectangles: decomposing logical rectangles first
        // would round neighboring fractional edges onto the same device pixel.
        let mut page_punch_overlaps = Vec::new();
        // Physical scissors actually repaired for captured page glyphs. Track
        // Region repairs as well as Node repairs, and deduplicate after rounding.
        let mut node_glyph_holes: Vec<[f32; 4]> = Vec::new();
        let overlay_replay_coverage = super::damage::overlay_replay_coverage(
            &self.last_overlay_stamps,
            self.overlay_occupied,
        );
        for dirty in &self.damage_nodes {
            if super::damage::paint_dirty_covered_by_overlay_restore(
                dirty,
                &self.damage_nodes,
                &self.last_overlay_stamps,
            ) {
                // OverlayRestore already replayed page instances in tree order.
                // Per-face dest-reset of overlapping AvatarGroup chips would
                // CLEAR later faces for one frame. Triggers above a popover sit
                // in the Restore pad but outside the panel stamp — those still
                // dest-reset so hover clears the shadow fringe once.
                continue;
            }
            let scissor_pad = super::draw_cull::node_present_scissor_pad(dirty.kind);
            // Pad is inside the helper; overflow is re-intersected after pad so the
            // 8px Node scissor cannot leak into a title above ScrollArea.
            let scissor_logical = super::draw_cull::node_present_scissor_logical(
                dirty.kind,
                dirty.key,
                dirty.bounds,
                &self.last_present_node_bounds,
                node_paints,
                scissor_pad,
            );
            if dirty.kind == PaintDirtyKind::Overlay {
                // Raw Overlay bounds only. `scissor_logical` already pads 16px;
                // encode pads occupancy again and the extra ring src-overs
                // AlertDialog card chrome when footer Cancel/OK hover.
                overlay_dirties.push((
                    dirty.key,
                    super::damage::overlay_suffix_encode_bounds(dirty.bounds),
                ));
                continue;
            }
            // A rejected ancestor clip is final. A reset derived from an old
            // animation pose must never replace it with an on-screen rectangle.
            if scissor_logical[2] <= 0.0 || scissor_logical[3] <= 0.0 {
                continue;
            }
            let page_inst = &scaled_instances[..overlay_inst_start.min(scaled_instances.len())];
            if super::draw_cull::paint_dirty_is_caret_overlay(dirty, node_paints, page_inst) {
                // I-beam is a swapchain overlay after retain blit. Dest-reset
                // here CLEARs Search and recaptures a hairline slice (竖条).
                continue;
            }
            let mut scissor_logical = scissor_logical;
            // Own outset shadow dest-reset sits outside overflow:hidden. GPU scissor
            // must cover that wipe or PartialLoad src-overs the halo darker every tick.
            let layered =
                super::draw_cull::layered_overlay_slot_bounds(dirty.key, dirty.bounds, node_paints)
                    .is_some();
            let page = &scaled_instances[..overlay_inst_start.min(scaled_instances.len())];
            let skip_covered_nested = super::draw_cull::skip_covered_nested_node_present(
                dirty,
                &self.damage_nodes,
                layered,
                node_paints,
                page,
                ui_scale,
            );
            if super::draw_cull::skip_in_flow_under_layered_overlay(
                dirty,
                &self.damage_nodes,
                node_paints,
                page,
            ) {
                // Wrapping under Latest: dest-reset would punch the pill and
                // hole_text wrap-box would flush after the overlay slot.
                continue;
            }
            let node_dest_reset =
                if dirty.kind == PaintDirtyKind::Node && allow_primitives && !skip_covered_nested {
                    let prev_slot = super::draw_cull::last_present_prev_for_dirty(
                        dirty.kind,
                        dirty.key,
                        &self.last_present_node_bounds,
                    );
                    super::draw_cull::node_present_dest_reset_full(
                        dirty.key,
                        dirty.bounds,
                        node_paints,
                        page,
                        scaled_tex,
                        scaled_input_chrome,
                        ui_scale,
                        prev_slot,
                    )
                } else {
                    None
                };
            if let Some(reset) = node_dest_reset.as_ref() {
                if ui_scale > 0.0 {
                    let logical = [
                        reset.rect[0] / ui_scale,
                        reset.rect[1] / ui_scale,
                        reset.rect[2] / ui_scale,
                        reset.rect[3] / ui_scale,
                    ];
                    // Present hole is the dest-reset wipe. Unioning `dirty+8`
                    // (widget-halo pad) onto an I-beam dirty opened an 18px
                    // scissor over neighboring glyphs after CLEAR of 2px.
                    scissor_logical = logical;
                }
            };
            // Keep the shadow fringe in the hole. Occupancy includes that
            // halo; cutting it clips the trigger hover fill and the later
            // portal replay src-overs the uncleared fringe so the shadow
            // stacks darker (AnchoredOverlay / Select under the trigger).
            // Subtract only portal **layout** boxes — Node hover and Region
            // share this path so button dest-reset clears the fringe once.
            let mut scissors = if dirty.kind == PaintDirtyKind::Node
                || dirty.kind == PaintDirtyKind::Region
            {
                let pieces = super::damage::page_scissors_excluding_portal_boxes(
                    scissor_logical,
                    &self.last_overlay_stamps,
                );
                if dirty.kind == PaintDirtyKind::Region && !self.last_overlay_stamps.is_empty() {
                    // A Region can subsume a changed trigger Node. Preserve
                    // idle controls while repairing that Node's current fill.
                    super::draw_cull::region_scissors_keeping_stable_page_captions(
                        pieces,
                        dirty,
                        &self.damage_nodes,
                        node_paints,
                        &self.last_overlay_stamps,
                        self.overlay_occupied,
                    )
                } else {
                    pieces
                }
            } else if dirty.kind == PaintDirtyKind::OverlayRestore {
                // The padded halo contains the trigger. Clearing that caption
                // every fade tick and letting the shadow land on it flickers
                // the Latin suffix, which does not move between 打开 and 关闭.
                super::draw_cull::scissors_keeping_stable_page_captions(
                    vec![scissor_logical],
                    node_paints,
                    &self.last_overlay_stamps,
                    self.overlay_occupied.or(Some(dirty.bounds)),
                )
            } else {
                vec![scissor_logical]
            };
            if dirty.kind == PaintDirtyKind::Node {
                // Drop the gap halo ring from the CLEAR (keep portal shadow in
                // retain). Still CLEAR the widget face — including where the
                // halo AABB overlaps the trigger — so transition-colors can
                // update. Do **not** Overlay-punch that face: every ~150ms
                // transition tick would rebuild the popover (卡顿) and src-over
                // the shadow onto freshly flushed glyphs (文字叠粗中间态).
                scissors = super::damage::page_node_present_scissors_preserving_portal_halo(
                    scissors,
                    dirty.bounds,
                    overlay_replay_coverage,
                    &self.last_overlay_stamps,
                );
                // The shadow edge splits this clear through the caption. The fill
                // still has to cover both scripts; capture draws that line once
                // after the shadow. Carving it out froze the fill, and a later
                // page-color patch flashed every glyph.
                scissors = super::draw_cull::node_scissors_keeping_unchanged_captions(
                    scissors,
                    node_paints,
                    &self.damage_nodes,
                    &self.last_overlay_stamps,
                );
                // Dest-reset above replaced the 8px pad with the border box.
                // `text-underline-offset` paints outside that box; without this
                // union, hover clips the line until scroll FullClear.
                let deco_key = dirty.key;
                let deco_bounds = dirty.bounds;
                scissors = scissors
                    .into_iter()
                    .map(|piece| {
                        if super::damage::intersect_rects(piece, deco_bounds).is_none() {
                            return piece;
                        }
                        super::draw_cull::expand_scissor_for_text_decoration(
                            piece,
                            deco_key,
                            deco_bounds,
                            node_paints,
                            &self.text_nodes,
                            prev_underline_instances,
                            ui_scale,
                        )
                    })
                    .collect();
            }
            if super::damage::page_dirty_punches_overlay_after_clear(dirty.kind) {
                for piece in &scissors {
                    if let Some(overlap) =
                        super::damage::overlay_page_punch_overlap(overlay_replay_coverage, *piece)
                    {
                        if let Some((x, y, w, h)) = super::damage::damage_to_scissor_physical(
                            overlap,
                            0.0,
                            ui_scale,
                            self.config.width,
                            self.config.height,
                        ) {
                            super::damage::push_disjoint_rect(
                                &mut page_punch_overlaps,
                                [x as f32, y as f32, w as f32, h as f32],
                            );
                        }
                    }
                }
            }
            for scissor_logical in scissors {
                let Some((x, y, w, h)) = super::damage::damage_to_scissor_physical(
                    scissor_logical,
                    0.0,
                    ui_scale,
                    self.config.width,
                    self.config.height,
                ) else {
                    continue;
                };
                render_pass.set_scissor_rect(x, y, w, h);
                // CLEAR touches the outward-rounded GPU scissor. Cull against
                // those same pixels: the fractional logical hole can exclude a
                // clipped page primitive covering its last row, leaving white
                // residue after an accordion menu closes.
                let cull_rect = node_present_repair_cull_rect((x, y, w, h));
                let cull_phys = Some(super::draw_cull::CullRects::one(cull_rect));
                match dirty.kind {
                    PaintDirtyKind::OverlayRestore | PaintDirtyKind::Region => {
                        // Textured-track thumb (inline hue/alpha): dest-reset finds no
                        // opaque fill and escalates to Region. Page-band replay uploads
                        // every descendant quad and runs glyphon. Own chrome + the ramp
                        // is the hole. Floating sliders stay Overlay; solid Slider
                        // dest-resets as Node.
                        if dirty.kind == PaintDirtyKind::Region
                            && super::draw_cull::region_replays_own_chrome_only(
                                dirty.key,
                                dirty.bounds,
                                node_paints,
                            )
                        {
                            note_present_hole_chrome();
                            node_glyph_holes.push(cull_rect);
                            self.draw_own_chrome_region(
                                render_pass,
                                node_paints,
                                scaled_instances,
                                clip_polys,
                                scaled_tex,
                                dummy,
                                scissor_logical,
                                ui_scale,
                                cull_phys,
                            );
                            continue;
                        }
                        note_present_hole_band();
                        // Reconstruct once from the same clear color and ordered
                        // page bands as FullClear, rather than sampling a top fill
                        // and submitting descendants a second time (alpha buildup).
                        draw_scissor_surface_clear(
                            render_pass,
                            &self.pipeline,
                            &self.device,
                            cull_rect,
                            clear_color,
                        );
                        if dirty.kind == PaintDirtyKind::Region {
                            node_glyph_holes.push(cull_rect);
                        }
                        if allow_primitives {
                            if !page_bands.is_empty() {
                                for (band_i, band) in page_bands.iter().enumerate() {
                                    render_pass.set_scissor_rect(x, y, w, h);
                                    draw_page_stack_band_to_pass(
                                        render_pass,
                                        band_i,
                                        band,
                                        paint_ops,
                                        &self.pipeline,
                                        &self.instance_buffer,
                                        &self.textured_pipeline,
                                        &self.textured_instance_buffer,
                                        &self.image_textures,
                                        scaled_tex,
                                        scaled_input_chrome,
                                        &self.device,
                                        page_band_underlines,
                                        page_band_shadows,
                                        &self.shadow_tex_cache,
                                        page_stack_band_backdrop(
                                            band_i,
                                            band_backdrop_idx,
                                            live_backdrops,
                                            dummy,
                                        ),
                                        scaled_instances,
                                        cull_phys,
                                        false,
                                    );
                                    if dirty.kind != PaintDirtyKind::Region
                                        && band.text.0 < band.text.1
                                        && !page_band_skip_text.get(band_i).copied().unwrap_or(true)
                                    {
                                        if band_i == 0 {
                                            self.text_renderer.render(render_pass);
                                        } else {
                                            self.page_stack_band_text_renderers[band_i - 1]
                                                .render(render_pass);
                                        }
                                    }
                                }
                                // Page `TextRenderer` holds every glyph on the
                                // forms page. Recording it for a thumb hole is
                                // the 150ms encode (prepare stays ~4ms; the
                                // scissor does not shrink the command buffer).
                                // Hole glyphs are in the capture renderer and
                                // flushed once after chrome — drawing them here
                                // as well src-overs the caption darker.
                                continue;
                            }
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
                            } else {
                                let inst_end = overlay_inst_start.min(scaled_instances.len());
                                if inst_end > 0 {
                                    draw_instance_range(
                                        render_pass,
                                        &self.pipeline,
                                        scaled_instances,
                                        0,
                                        inst_end as u32,
                                        cull_phys,
                                    );
                                }
                                if overlay_tex_start > 0 {
                                    draw_packed_textured_runs(
                                        render_pass,
                                        &self.textured_pipeline,
                                        &self.textured_instance_buffer,
                                        &self.image_textures,
                                        scaled_tex,
                                        0,
                                        overlay_tex_start,
                                        cull_phys,
                                    );
                                }
                            }
                        }
                        if allow_primitives {
                            self.draw_node_present_underlines(
                                render_pass,
                                dummy,
                                underline_instances,
                                &[],
                                scaled_instances,
                                overlay_inst_start,
                                Some(cull_rect),
                            );
                        }
                        if dirty.kind != PaintDirtyKind::Region && !skip_page_text {
                            render_pass.set_scissor_rect(x, y, w, h);
                            self.text_renderer.render(render_pass);
                        }
                    }
                    PaintDirtyKind::Overlay => {
                        // Recorded into overlay_scissor above; suffix is drawn once after
                        // Restore/Node so fade CSS + membership cannot stack the halo.
                    }
                    PaintDirtyKind::Node => {
                        if skip_covered_nested {
                            // Ancestor present already submitted this slot's ink.
                            continue;
                        }
                        if allow_primitives {
                            node_glyph_holes.push(cull_rect);
                            if let Some(reset) = node_dest_reset.as_ref() {
                                // Keep this visible piece's scissor through CLEAR and
                                // replay. Widening to reset.rect punches the portal.
                                let replay = reset.replay.1 > reset.replay.0;
                                // Layered jump chrome (Latest) is a rounded pill. A
                                // scissor-sized radii=0 CLEAR lets SDF +2px AA leak a
                                // translucent white frame — hover flashes sharp corners.
                                // Inflate the fill so AA sits outside the scissor.
                                let reset_color = wgpu::Color {
                                    r: reset.clear[0] as f64,
                                    g: reset.clear[1] as f64,
                                    b: reset.clear[2] as f64,
                                    a: 1.0,
                                };
                                // Hairline I-beam wipe is 2px — a scissor-sized CLEAR is
                                // entirely SDF vertex AA (translucent), so blink OFF left
                                // serif caps and src-overed Search「S」. Inflate so the
                                // fringe sits outside the wipe scissor (opaque hole).
                                draw_scissor_fill(
                                    render_pass,
                                    &self.pipeline,
                                    &self.device,
                                    reset.rect,
                                    reset_color,
                                );
                                if replay {
                                    let wipe_cull =
                                        Some(super::draw_cull::CullRects::one(reset.rect));
                                    draw_sdf_range_with_clip(
                                        render_pass,
                                        &self.pipeline,
                                        &self.device,
                                        dummy,
                                        scaled_instances,
                                        clip_polys,
                                        reset.replay.0 as u32,
                                        (reset.replay.1 - reset.replay.0) as u32,
                                        wipe_cull,
                                    );
                                    render_pass.set_scissor_rect(x, y, w, h);
                                    // The underlay SDF and its ramp are one slot (slider
                                    // track). CLEAR drops the texture; replaying only the
                                    // SDF leaves a solid bite, and the thumb shadow then
                                    // never covers the gradient on the equator.
                                    if let Some(owner) = node_paints
                                        .iter()
                                        .filter(|p| {
                                            !p.overlay
                                                && p.instance.0 <= reset.replay.0
                                                && reset.replay.0 < p.instance.1
                                        })
                                        .min_by_key(|p| p.instance.1.saturating_sub(p.instance.0))
                                    {
                                        let t0 = owner.textured.0;
                                        let t1 = owner.textured.1.min(overlay_tex_start);
                                        if t0 < t1 {
                                            draw_packed_textured_runs(
                                                render_pass,
                                                &self.textured_pipeline,
                                                &self.textured_instance_buffer,
                                                &self.image_textures,
                                                scaled_tex,
                                                t0,
                                                t1,
                                                wipe_cull,
                                            );
                                            render_pass.set_scissor_rect(x, y, w, h);
                                        }
                                    }
                                }
                            }
                            let has_keyed = super::damage::paint_dirty_has_keyed_slot(
                                dirty.key,
                                dirty.bounds,
                                node_paints
                                    .iter()
                                    .filter(|p| !p.overlay)
                                    .map(|p| (p.key, p.bounds)),
                            );
                            let repair_logical = [
                                cull_rect[0] / ui_scale.max(1e-6),
                                cull_rect[1] / ui_scale.max(1e-6),
                                cull_rect[2] / ui_scale.max(1e-6),
                                cull_rect[3] / ui_scale.max(1e-6),
                            ];
                            let replay_peers = node_dest_reset.is_some()
                                && super::draw_cull::node_present_replays_peer_chrome(
                                    dirty.key,
                                    dirty.bounds,
                                    repair_logical,
                                    node_paints,
                                    scaled_instances,
                                );
                            if !replay_peers
                                && super::draw_cull::node_present_restore_underlay(
                                    dirty.key,
                                    dirty.bounds,
                                    node_paints,
                                )
                            {
                                // Stacking overlay dest-reset punched in-flow fills
                                // (rounded bubble under a jump pill). Rebuild them
                                // in the CLEAR hole only — the 8px node scissor
                                // hangs below Latest onto wrapping thread / composer.
                                if let Some(reset) = node_dest_reset.as_ref() {
                                    let scale = ui_scale.max(1e-6);
                                    let reset_logical = [
                                        reset.rect[0] / scale,
                                        reset.rect[1] / scale,
                                        reset.rect[2] / scale,
                                        reset.rect[3] / scale,
                                    ];
                                    let restore_cull =
                                        Some(super::draw_cull::CullRects::one(reset.rect));
                                    self.draw_matching_node_paints(
                                        render_pass,
                                        paint_ops,
                                        node_paints,
                                        scaled_instances,
                                        clip_polys,
                                        scaled_tex,
                                        scaled_input_chrome,
                                        dummy,
                                        |p| {
                                            !p.overlay
                                                && !super::damage::paint_dirty_matches_present_slot(
                                                    dirty.key,
                                                    dirty.bounds,
                                                    p.key,
                                                    p.bounds,
                                                    has_keyed,
                                                )
                                                && super::draw_cull::node_present_underlay_paint(
                                                    dirty.key,
                                                    dirty.bounds,
                                                    p,
                                                    reset_logical,
                                                    node_paints,
                                                    scaled_instances,
                                                )
                                        },
                                        restore_cull,
                                        Some(overlay_inst_start),
                                        Some(overlay_tex_start),
                                        true,
                                        false,
                                    );
                                    if pending_underlay_glyphs
                                    && super::draw_cull::node_present_flush_layered_underlay_glyphs(
                                        dirty.key,
                                        dirty.bounds,
                                        node_paints,
                                    )
                                {
                                    self.capture_underlay_text_renderer.render(render_pass);
                                    super::frame_stats::note_scene_batch(
                                        super::frame_stats::SceneKind::Text,
                                        1,
                                    );
                                    // glyphon preserves the GPU scissor. Each disjoint
                                    // visible piece needs its own underlay submission.
                                }
                                    // Keep the wipe scissor for Latest. Restoring the
                                    // 8px node scissor let SDF halo paint outside
                                    // CLEAR — PartialLoad stacked the enter ring.
                                } else {
                                    // No CLEAR: wrapping underlay is still in retain.
                                    // Do not drop pending — a later Latest dest-reset
                                    // still needs recapture in its wipe (hide hole).
                                    render_pass.set_scissor_rect(x, y, w, h);
                                }
                            } else if pending_underlay_glyphs
                                && super::draw_cull::node_present_flush_layered_underlay_glyphs(
                                    dirty.key,
                                    dirty.bounds,
                                    node_paints,
                                )
                            {
                                if node_dest_reset.is_some() {
                                    // CLEAR already ran. Recapture wrapping even when
                                    // restore_underlay is false (hide-settle Latest
                                    // often has no dest-ink / keyed layered slot).
                                    self.capture_underlay_text_renderer.render(render_pass);
                                    super::frame_stats::note_scene_batch(
                                        super::frame_stats::SceneKind::Text,
                                        1,
                                    );
                                }
                            }
                            let overlay_cull = node_dest_reset
                                .as_ref()
                                .filter(|_| layered || replay_peers)
                                .map(|r| {
                                    super::draw_cull::CullRects::one(if replay_peers {
                                        cull_rect
                                    } else {
                                        r.rect
                                    })
                                });
                            // Glyphon may set its own scissor while restoring text.
                            render_pass.set_scissor_rect(x, y, w, h);
                            self.draw_matching_node_paints(
                                render_pass,
                                paint_ops,
                                node_paints,
                                scaled_instances,
                                clip_polys,
                                scaled_tex,
                                scaled_input_chrome,
                                dummy,
                                |p| {
                                    !p.overlay
                                        && (super::damage::paint_dirty_matches_present_slot(
                                            dirty.key,
                                            dirty.bounds,
                                            p.key,
                                            p.bounds,
                                            has_keyed,
                                        ) || (replay_peers
                                            && super::draw_cull::node_present_underlay_paint(
                                                dirty.key,
                                                dirty.bounds,
                                                p,
                                                repair_logical,
                                                node_paints,
                                                scaled_instances,
                                            )))
                                },
                                overlay_cull,
                                None,
                                None,
                                true,
                                false,
                            );
                            // Device-pixel ceil of this hole eats the 1px seam on
                            // the shared edge. The fill was just drawn over it;
                            // put the strip back on top.
                            if node_dest_reset.is_some() {
                                render_pass.set_scissor_rect(x, y, w, h);
                                self.draw_matching_node_paints(
                                    render_pass,
                                    paint_ops,
                                    node_paints,
                                    scaled_instances,
                                    clip_polys,
                                    scaled_tex,
                                    scaled_input_chrome,
                                    dummy,
                                    |p| {
                                        super::draw_cull::node_present_replays_shared_seam(
                                            dirty.key,
                                            dirty.bounds,
                                            p,
                                            repair_logical,
                                            node_paints,
                                        )
                                    },
                                    Some(super::draw_cull::CullRects::one(cull_rect)),
                                    None,
                                    None,
                                    true,
                                    false,
                                );
                            }
                            // I-beam is drawn after retain→swapchain blit, not into retain.
                            self.draw_node_present_underlines(
                                render_pass,
                                dummy,
                                underline_instances,
                                prev_underline_instances,
                                scaled_instances,
                                overlay_inst_start,
                                Some(cull_rect),
                            );
                        }
                    }
                }
            }
        }
        // A live overlay is composited after the page repair. Replay grazing
        // captions before that overlay so fractional/antialiased portal edges
        // blend over the retained page text instead of painting the text on top
        // of the panel. Closed restores have no live portal and stay in the
        // existing late flush below, after the page bands are rebuilt.
        let caption_replayed_before_overlay =
            !caption_glyph_holes.is_empty() && !self.last_overlay_stamps.is_empty();
        let caption_physical: Vec<[f32; 4]> = if caption_replayed_before_overlay {
            caption_glyph_holes
                .iter()
                .filter_map(|logical| {
                    super::damage::damage_to_scissor_physical(
                        *logical,
                        0.0,
                        ui_scale,
                        self.config.width,
                        self.config.height,
                    )
                    .map(|(x, y, w, h)| [x as f32, y as f32, w as f32, h as f32])
                })
                .collect()
        } else {
            Vec::new()
        };
        if caption_replayed_before_overlay {
            draw_captured_page_glyphs(
                &self.capture_text_renderer,
                render_pass,
                self.config.width,
                self.config.height,
                &caption_physical,
                &[],
            );
        }
        // Overlay suffix after Restore/Node dest-reset. Same portal key encodes
        // once (fade CSS + membership). Different keys keep separate scissors so
        // a nested accordion cannot restack the parent outset shadow.
        //
        // Do not flush leftover wrapping after Latest. Glyphon ignores the GPU
        // scissor; a second underlay pass after capture_text stamps the thread
        // onto the label ("ek ls the" over "Latest"). Wrapping without CLEAR is
        // already in retain — dest-reset holes flush underlay before the pill.
        let remaining_keys: Vec<u64> = self.last_overlay_stamps.iter().map(|(k, _)| *k).collect();
        let dest_reset_holes: Vec<[f32; 4]> = self
            .damage_nodes
            .iter()
            .filter(|d| d.kind == PaintDirtyKind::OverlayRestore)
            .map(|d| d.bounds)
            .collect();
        let restore_keys: Vec<u64> = self
            .damage_nodes
            .iter()
            .filter(|d| d.kind == PaintDirtyKind::OverlayRestore)
            .map(|d| d.key)
            .collect();
        let closed_restore =
            super::damage::overlay_restore_is_closed_portal(&restore_keys, &remaining_keys);
        // Dest-reset CLEARs first, then overlay suffix / punches, then page
        // glyph recapture. Flushing glyphs before a Region punch lets translucent
        // portal shadow src-over the caption (叠粗). Capture already skips
        // stamps-occluded nodes, so late flush cannot stamp under the panel.
        let merged = super::damage::merge_overlay_replay_dirties(
            &overlay_dirties,
            &self.last_overlay_stamps,
        );
        for (dirty_keys, dirty_bounds) in merged {
            let occupancy_key = dirty_keys.first().copied().unwrap_or(0);
            let occupancy = super::damage::overlay_encode_occupancy(
                occupancy_key,
                Some(dirty_bounds),
                &self.last_overlay_stamps,
                if self.last_overlay_stamps.len() <= 1 {
                    self.overlay_occupied
                } else {
                    None
                },
            );
            let Some(scissor_logical) =
                super::damage::overlay_encode_scissor(Some(dirty_bounds), occupancy, None)
            else {
                continue;
            };
            // Same caption hole as OverlayRestore. Drawing the shadow through
            // the stable suffix src-overs it once per opacity tick.
            let suffix_logical = super::draw_cull::scissors_keeping_stable_page_captions(
                vec![scissor_logical],
                node_paints,
                &self.last_overlay_stamps,
                occupancy,
            );
            // Holes need *all* overlay bands, including idle siblings above
            // this dirty portal. Reserve them for the complete replay below.
            let mut pieces = Vec::new();
            for logical in suffix_logical {
                let Some((x, y, w, h)) = super::damage::damage_to_scissor_physical(
                    logical,
                    0.0,
                    ui_scale,
                    self.config.width,
                    self.config.height,
                ) else {
                    continue;
                };
                pieces.push([x as f32, y as f32, w as f32, h as f32]);
            }
            for hole in &page_punch_overlaps {
                pieces = pieces
                    .into_iter()
                    .flat_map(|r| super::damage::subtract_rect(r, *hole))
                    .collect();
            }
            for piece in pieces {
                let Some((x, y, w, h)) = super::damage::physical_xywh_to_scissor(
                    piece,
                    self.config.width,
                    self.config.height,
                ) else {
                    continue;
                };
                render_pass.set_scissor_rect(x, y, w, h);
                self.draw_node_present_overlay(
                    render_pass,
                    paint_ops,
                    scaled_instances,
                    scaled_tex,
                    overlay_inst_start,
                    overlay_tex_start,
                    overlay_paint_start,
                    overlay_band_ranges,
                    dummy,
                    allow_primitives,
                    &dirty_keys,
                    closed_restore,
                    &remaining_keys,
                    &dest_reset_holes,
                    ui_scale,
                    None,
                );
            }
        }
        // Replay only pixels whose page underlay was reconstructed. Unioning
        // separate holes, or redrawing a whole idle portal, accumulates alpha
        // in untouched shadows and gaps between the holes. Node hover never
        // contributes holes (halo-preserve above); these are Region/Restore.
        for hole in page_punch_overlaps {
            let Some((x, y, w, h)) = super::damage::physical_xywh_to_scissor(
                hole,
                self.config.width,
                self.config.height,
            ) else {
                continue;
            };
            render_pass.set_scissor_rect(x, y, w, h);
            let hole_cull = Some(super::draw_cull::CullRects::one(hole));
            self.draw_node_present_overlay(
                render_pass,
                paint_ops,
                scaled_instances,
                scaled_tex,
                overlay_inst_start,
                overlay_tex_start,
                overlay_paint_start,
                overlay_band_ranges,
                dummy,
                allow_primitives,
                &remaining_keys,
                false,
                &remaining_keys,
                &[],
                ui_scale,
                hole_cull,
            );
        }
        if pending_page_glyphs
            || (!caption_replayed_before_overlay && !caption_glyph_holes.is_empty())
        {
            let mut holes = node_glyph_holes;
            // The shared capture mesh was already flushed before the portal
            // shadow. An overlapping late repair must not blend it again.
            holes = super::draw_cull::capture_glyph_scissors_after_replay_physical(
                &holes,
                &caption_physical,
                self.config.width,
                self.config.height,
            );
            if !caption_replayed_before_overlay {
                for logical in caption_glyph_holes {
                    let Some((x, y, w, h)) = super::damage::damage_to_scissor_physical(
                        *logical,
                        0.0,
                        ui_scale,
                        self.config.width,
                        self.config.height,
                    ) else {
                        continue;
                    };
                    holes.push([x as f32, y as f32, w as f32, h as f32]);
                }
            }
            if !holes.is_empty() {
                self.flush_node_present_page_glyphs(render_pass, &holes, ui_scale);
            }
        }
    }

    fn draw_node_present_overlay<'pass>(
        &'pass self,
        render_pass: &mut wgpu::RenderPass<'pass>,
        paint_ops: &[crate::layout::layout_tree::PaintOp],
        scaled_instances: &[InstanceInput],
        scaled_tex: &'pass [crate::renderer::ScaledTexturedDraw],
        overlay_inst_start: usize,
        overlay_tex_start: usize,
        overlay_paint_start: usize,
        overlay_band_ranges: &[crate::layout::layout_tree::OverlayBandRange],
        dummy: &'pass wgpu::BindGroup,
        allow_primitives: bool,
        overlay_dirty_keys: &[u64],
        closed_restore: bool,
        remaining_stamp_keys: &[u64],
        dest_reset_holes: &[[f32; 4]],
        ui_scale: f32,
        cull: Option<super::draw_cull::CullRects>,
    ) {
        // FullClear draws `band rects → band text → next band` so a higher menu
        // occludes lower glyphs. Flushing every overlay fill then every glyphon
        // renderer replays DatePicker calendar digits on top of the month menu
        // (looks like a missing background) and src-overs the parent halo again.
        let overlay_band_keys: Vec<u64> = overlay_band_ranges.iter().map(|b| b.stamp_key).collect();
        let mut overlay_band_encode = super::damage::overlay_band_encode_flags(
            &overlay_band_keys,
            overlay_dirty_keys,
            closed_restore,
            remaining_stamp_keys,
        );
        super::damage::overlay_band_promote_dest_reset_siblings(
            &mut overlay_band_encode,
            &overlay_band_keys,
            overlay_dirty_keys,
            &self.last_overlay_stamps,
            dest_reset_holes,
        );
        for (band_i, band) in overlay_band_ranges.iter().enumerate() {
            let op_lo = band.paint_ops.0.max(overlay_paint_start);
            let op_hi = band.paint_ops.1.max(op_lo);
            let inst_lo = band.instance.0.max(overlay_inst_start);
            let inst_hi = band.instance.1.max(inst_lo);
            let tex_lo = band.textured.0.max(overlay_tex_start);
            let tex_hi = band.textured.1.max(tex_lo);
            if !overlay_band_encode.get(band_i).copied().unwrap_or(false) {
                continue;
            }
            if allow_primitives {
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
                        cull,
                    );
                } else if inst_lo < inst_hi {
                    self.pipeline.bind_sdf(render_pass, true, dummy);
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
                        inst_lo as u32,
                        (inst_hi - inst_lo) as u32,
                        cull,
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
                            cull,
                        );
                    }
                }
            }
            if band.text.0 < band.text.1 {
                // Punch-path holes are physical; stamps are logical. Panel glyphs
                // never sit in the trigger-face strip — skip glyphon there.
                let skip_text = cull.is_some_and(|c| {
                    self.last_overlay_stamps
                        .iter()
                        .find(|(k, _)| *k == band.stamp_key)
                        .is_some_and(|(_, stamp)| {
                            let phys = super::draw_cull::scale_xywh(*stamp, ui_scale);
                            !c.hits_aabb(phys)
                        })
                });
                if !skip_text {
                    if band_i == 0 {
                        self.overlay_text_renderer.render(render_pass);
                    } else if let Some(r) = self.overlay_band_text_renderers.get(band_i - 1) {
                        r.render(render_pass);
                    }
                    super::frame_stats::note_scene_batch(super::frame_stats::SceneKind::Text, 1);
                }
            }
        }
    }

    /// Draw captured glyphs inside the disjoint physical repair scissors.
    fn flush_node_present_page_glyphs<'pass>(
        &'pass self,
        render_pass: &mut wgpu::RenderPass<'pass>,
        node_cleared: &[[f32; 4]],
        ui_scale: f32,
    ) {
        let portals: Vec<_> = self
            .last_overlay_stamps
            .iter()
            .map(|(_, stamp)| super::draw_cull::scale_xywh(*stamp, ui_scale))
            .collect();
        draw_captured_page_glyphs(
            &self.capture_text_renderer,
            render_pass,
            self.config.width,
            self.config.height,
            node_cleared,
            &portals,
        );
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
            draw_cpu_sdf_instances(render_pass, &self.pipeline, &self.device, dummy, &erased);
        }
        let now = owned(current);
        draw_cpu_sdf_instances(render_pass, &self.pipeline, &self.device, dummy, &now);
    }

    /// Intersecting slots contribute their own fill and textures, not the
    /// descendant instance range collect stores on the parent.
    fn draw_own_chrome_region<'pass>(
        &'pass self,
        render_pass: &mut wgpu::RenderPass<'pass>,
        node_paints: &[crate::layout::layout_tree::NodePaint],
        scaled_instances: &[InstanceInput],
        clip_polys: &[crate::style::GpuClipPoly],
        scaled_tex: &'pass [crate::renderer::ScaledTexturedDraw],
        dummy: &'pass wgpu::BindGroup,
        scissor_logical: [f32; 4],
        ui_scale: f32,
        cull: Option<super::draw_cull::CullRects>,
    ) {
        let mut batch_inst: Vec<InstanceInput> = Vec::new();
        let mut batch_poly: Vec<crate::style::GpuClipPoly> = Vec::new();
        fn flush<'a>(
            pass: &mut wgpu::RenderPass<'a>,
            pipeline: &'a RuiPipeline,
            device: &'a wgpu::Device,
            dummy: &'a wgpu::BindGroup,
            insts: &mut Vec<InstanceInput>,
            polys: &mut Vec<crate::style::GpuClipPoly>,
        ) {
            if insts.is_empty() {
                return;
            }
            draw_uploaded_sdf_with_clip(pass, pipeline, device, dummy, insts, polys, None);
            insts.clear();
            polys.clear();
        }
        for p in node_paints {
            if p.overlay || super::damage::intersect_rects(p.bounds, scissor_logical).is_none() {
                continue;
            }
            let (lo, hi) = super::draw_cull::slot_own_chrome_range(p, scaled_instances, ui_scale);
            for i in lo..hi {
                if let Some(c) = cull {
                    if !c.hits_instance(&scaled_instances[i]) {
                        continue;
                    }
                }
                batch_inst.push(scaled_instances[i]);
                batch_poly.push(
                    clip_polys
                        .get(i)
                        .copied()
                        .unwrap_or(crate::style::GpuClipPoly::NONE),
                );
            }
            super::draw_cull::for_each_own_textured_run(
                p,
                node_paints,
                scaled_tex.len(),
                |start, end| {
                    let hits = match cull {
                        Some(c) => scaled_tex[start..end].iter().any(|d| c.hits_textured(d)),
                        None => start < end,
                    };
                    if !hits {
                        return;
                    }
                    flush(
                        render_pass,
                        &self.pipeline,
                        &self.device,
                        dummy,
                        &mut batch_inst,
                        &mut batch_poly,
                    );
                    draw_packed_textured_runs(
                        render_pass,
                        &self.textured_pipeline,
                        &self.textured_instance_buffer,
                        &self.image_textures,
                        scaled_tex,
                        start,
                        end,
                        cull,
                    );
                },
            );
        }
        flush(
            render_pass,
            &self.pipeline,
            &self.device,
            dummy,
            &mut batch_inst,
            &mut batch_poly,
        );
    }

    fn draw_matching_node_paints<'pass>(
        &'pass self,
        render_pass: &mut wgpu::RenderPass<'pass>,
        paint_ops: &[crate::layout::layout_tree::PaintOp],
        node_paints: &[crate::layout::layout_tree::NodePaint],
        scaled_instances: &[InstanceInput],
        clip_polys: &[crate::style::GpuClipPoly],
        scaled_tex: &'pass [crate::renderer::ScaledTexturedDraw],
        scaled_input_chrome: &[InstanceInput],
        dummy: &'pass wgpu::BindGroup,
        pred: impl Fn(&crate::layout::layout_tree::NodePaint) -> bool,
        cull: Option<super::draw_cull::CullRects>,
        inst_cap: Option<usize>,
        tex_cap: Option<usize>,
        include_fill: bool,
        include_chrome: bool,
    ) {
        for p in node_paints.iter().filter(|p| pred(p)) {
            // Clip coalesced runs to this slot, preserving texture/rect order.
            let inst_lo = p.instance.0;
            let inst_hi = inst_cap.unwrap_or(p.instance.1).min(p.instance.1);
            let tex_lo = p.textured.0;
            let tex_hi = tex_cap.unwrap_or(p.textured.1).min(p.textured.1);
            if include_fill {
                for op in super::draw_cull::slot_paint_ops(
                    paint_ops,
                    (inst_lo, inst_hi),
                    (tex_lo, tex_hi),
                ) {
                    match op {
                        crate::layout::layout_tree::PaintOp::Rects { start, count } => {
                            draw_sdf_range_with_clip(
                                render_pass,
                                &self.pipeline,
                                &self.device,
                                dummy,
                                scaled_instances,
                                clip_polys,
                                start,
                                count,
                                cull,
                            );
                        }
                        crate::layout::layout_tree::PaintOp::Textured { start, count } => {
                            draw_packed_textured_runs(
                                render_pass,
                                &self.textured_pipeline,
                                &self.textured_instance_buffer,
                                &self.image_textures,
                                scaled_tex,
                                start as usize,
                                (start + count) as usize,
                                cull,
                            );
                        }
                    }
                }
            }
            if include_chrome && p.chrome.0 < p.chrome.1 && p.chrome.1 <= scaled_input_chrome.len()
            {
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
            self.overlay_band_text_renderers
                .push(RuiTextRenderer::new_sharing_resources(
                    &self.device,
                    &self.queue,
                    &self.config,
                    &self.text_renderer,
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

    /// Accordion-up can clip every overlay label while collect still lists them.
    /// Skipping prepare left last frame's HEX/RGB mesh in the overlay renderer.
    fn clear_overlay_band_glyphon(&mut self, band_i: usize) {
        if band_i == 0 {
            self.overlay_text_renderer.prepare(
                &self.device,
                &self.queue,
                &self.config,
                std::iter::empty(),
            );
        } else if let Some(tr) = self.overlay_band_text_renderers.get_mut(band_i - 1) {
            tr.prepare(&self.device, &self.queue, &self.config, std::iter::empty());
        }
    }

    /// Grow shared-font page stacking pipelines (band 0 = [`Self::text_renderer`]).
    fn ensure_page_stack_band_text_renderers(&mut self, band_count: usize) {
        let extra_needed = band_count.saturating_sub(1);
        while self.page_stack_band_text_renderers.len() < extra_needed {
            self.page_stack_band_text_renderers
                .push(RuiTextRenderer::new_sharing_resources(
                    &self.device,
                    &self.queue,
                    &self.config,
                    &self.text_renderer,
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
        let mut button_caption_only = false;
        let mut portal_only_structure = false;
        let status = if initial_mount {
            use crate::core::tree::LayoutStatus;
            LayoutStatus::StructureChanged
        } else {
            let mut dirty = Vec::new();
            let status =
                Self::reconcile_elements(&mut root_element, &mut self.root_element, &mut dirty);
            button_caption_only =
                Self::button_caption_only_change(&root_element, &self.root_element);
            portal_only_structure =
                Self::portal_only_structure_change(&root_element, &self.root_element);
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
        ) && !(status == crate::core::tree::LayoutStatus::LayoutChanged && button_caption_only)
            && !portal_only_structure
        {
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
        // A newly mounted reactive anchor has no collect stamp or geometry yet.
        // Determine its portal owner from the live ancestry, before replacing it;
        // otherwise a nested alpha/format slot is mistaken for a page remount.
        let mut portal_owner = None;
        let mut in_portal = false;
        if let Some(path) = crate::core::tree::find_path_in_tree(&self.root_element, |n| {
            n.template_id
                .as_deref()
                .is_some_and(|tid| normalize_anchor_template_id(tid) == want)
        }) {
            let mut node = &self.root_element;
            for index in std::iter::once(None).chain(path.into_iter().map(Some)) {
                if let Some(index) = index {
                    node = &node.children[index];
                }
                let is_surface = node.attrs.get("rui_portal").map(String::as_str) == Some("1")
                    || node
                        .attrs
                        .get(crate::layout::layout_tree::ATTR_HOISTED_PORTAL)
                        .map(String::as_str)
                        == Some("1");
                in_portal |= is_surface
                    || node.template_id.as_deref()
                        == Some(crate::layout::layout_tree::PORTAL_LAYER_ID);
                if is_surface {
                    let key = crate::layout::layout_tree::overlay_portal_stamp_key(node);
                    let bounds = self
                        .last_overlay_stamps
                        .iter()
                        .find(|(stamp, _)| *stamp == key)
                        .map(|(_, bounds)| *bounds)
                        .unwrap_or_else(|| {
                            crate::core::tree::visual_paint_aabb(node, node.paint_scroll_acc)
                        });
                    portal_owner = Some((key, bounds));
                }
            }
        }
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
        // Snapshot before reconcile: Tabs hide still has the last panel box; using the
        // ancestor ScrollArea clip instead punched Gallery 8b ToggleGroup glyphs.
        let island_key = super::damage::paint_element_key(old_subtree);
        let island_box =
            crate::core::tree::visual_paint_aabb(old_subtree, old_subtree.paint_scroll_acc);
        let button_caption_only = Self::button_caption_only_change(&new_fragment, old_subtree);
        let has_button_caption_change = Self::has_button_caption_change(&new_fragment, old_subtree);
        let portal_only_change = Self::portal_only_structure_change(&new_fragment, old_subtree);
        let portal_button_damage = portal_only_change
            .then(|| Self::portal_button_caption_damage(&new_fragment, old_subtree))
            .flatten();

        let mut dirty = Vec::new();
        let declares_portal = crate::layout::layout_tree::subtree_declares_portal(old_subtree)
            || crate::layout::layout_tree::subtree_declares_portal(&new_fragment);
        let portal_island = declares_portal
            && crate::layout::layout_tree::subtree_is_portal_only(old_subtree)
            && crate::layout::layout_tree::subtree_is_portal_only(&new_fragment);
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
            portal_moves = hoist_portal_subtrees_refreshing_scroll_flags(&mut self.root_element);
            if portal_moves > 0 {
                status = Self::merge_layout_status(status, LayoutStatus::StructureChanged);
            }
        }
        self.layout_status = Self::merge_layout_status(self.layout_status, status);
        if !matches!(status, LayoutStatus::Clean) {
            // Bind StyleChanged already zeros this spine. Island replace does not —
            // ViewportsOnly `reuse_idle` would skip-replay the previous title/desc
            // (and an idle sidebar) on top of the new Match body.
            if let Some(path) = crate::core::tree::find_path_in_tree(&self.root_element, |n| {
                n.template_id
                    .as_deref()
                    .is_some_and(|tid| normalize_anchor_template_id(tid) == want)
            }) {
                crate::layout::collect_cache::invalidate_collect_paint_hash_path(
                    &mut self.root_element,
                    &path,
                );
            }
        }
        if declares_portal || portal_moves > 0 || (in_portal && status != LayoutStatus::Clean) {
            // Bubble +N expand / popover hoist: Taffy still rebuilds the portal layer,
            // but GPU FullClear of the gallery is the fullscreen flash.
            self.portal_membership_dirty = true;
        }
        if status != LayoutStatus::Clean {
            if let Some((key, bounds)) = portal_owner {
                super::damage::push_frame_overlay_damage(key, bounds);
            }
        }
        if let Some((button_key, button_bounds)) = portal_button_damage {
            // The trigger is the page-facing part of a mixed portal island. Its caption may
            // change at the same time as the portal host is inserted/removed, but that must not
            // promote the whole island to a Region clear. Portal membership damage owns the
            // floating panel; this key owns only the trigger's retained pixels.
            super::damage::push_frame_paint_key(button_key, button_bounds);
        }
        if !in_portal
            && !portal_island
            && !portal_only_change
            && matches!(
                status,
                LayoutStatus::StructureChanged | LayoutStatus::LayoutChanged
            )
        {
            // Nested island (Tabs panel / MessageScroller Send): Region Clear
            // clip ∩ (prev ∪ now) — not a Node dest-reset, not the clip itself.
            // Gallery Forms Tabs share the page ScrollArea with 8b ToggleGroup;
            // dirtying that clip wiped sibling glyphs. Route-sized Match bodies
            // (App:section_body) cover most of the viewport → Window FullClear.
            let now_box = crate::core::tree::find_in_tree(&self.root_element, |n| {
                n.template_id
                    .as_deref()
                    .is_some_and(|tid| normalize_anchor_template_id(tid) == want)
            })
            .map(|el| crate::core::tree::visual_paint_aabb(el, el.paint_scroll_acc));
            if let Some(d) =
                super::damage::region_island_structure_damage(clip_bounds, island_box, now_box)
            {
                let (lw, lh) = self.logical_size();
                let viewport = [0.0, 0.0, lw, lh];
                if super::damage::region_is_page_route_tier(d, viewport)
                    && !button_caption_only
                    && !has_button_caption_change
                {
                    self.page_geom_dirty = true;
                    self.mark_full_redraw();
                } else {
                    super::damage::push_frame_region_paint(island_key, d);
                }
            } else if clip_bounds.is_none() && !button_caption_only && !has_button_caption_change {
                self.page_geom_dirty = true;
                self.mark_full_redraw();
            } else if button_caption_only || has_button_caption_change {
                let now_box = crate::core::tree::find_in_tree(&self.root_element, |n| {
                    n.template_id
                        .as_deref()
                        .is_some_and(|tid| normalize_anchor_template_id(tid) == want)
                })
                .map(|el| crate::core::tree::visual_paint_aabb(el, el.paint_scroll_acc))
                .unwrap_or(island_box);
                let damage = super::damage::union_rects(island_box, now_box);
                super::damage::push_frame_paint_key(island_key, damage);
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

    /// A button's plain caption can change its intrinsic text width while remaining inside the
    /// same mounted subtree. That change needs the button's layout node revisited, but it does
    /// not invalidate the retained page surface. Keep this classification structural: any style,
    /// image, child-count, or identity change escapes the local path.
    fn button_caption_only_change(new_root: &Element, old_root: &Element) -> bool {
        fn walk(new_el: &Element, old_el: &Element, in_button: bool, changed: &mut bool) -> bool {
            if new_el.template_id != old_el.template_id
                || new_el.style != old_el.style
                || new_el.text_style != old_el.text_style
                || new_el.image_texture != old_el.image_texture
                || new_el.children.len() != old_el.children.len()
            {
                return false;
            }
            let in_button = in_button || new_el.button_hover_paint.is_some();
            if new_el.text != old_el.text {
                if !in_button {
                    return false;
                }
                *changed = true;
            }
            new_el
                .children
                .iter()
                .zip(&old_el.children)
                .all(|(new_child, old_child)| walk(new_child, old_child, in_button, changed))
        }
        let mut changed = false;
        walk(new_root, old_root, false, &mut changed) && changed
    }

    fn has_button_caption_change(new_root: &Element, old_root: &Element) -> bool {
        fn walk(new_el: &Element, old_el: &Element, in_button: bool) -> bool {
            let in_button = in_button || new_el.button_hover_paint.is_some();
            if in_button && new_el.text != old_el.text {
                return true;
            }
            new_el
                .children
                .iter()
                .zip(&old_el.children)
                .any(|(new_child, old_child)| walk(new_child, old_child, in_button))
        }
        walk(new_root, old_root, false)
    }

    fn portal_button_caption_damage(
        new_root: &Element,
        old_root: &Element,
    ) -> Option<(u64, [f32; 4])> {
        fn walk(
            new_el: &Element,
            old_el: &Element,
            in_button: bool,
        ) -> (bool, Option<(u64, [f32; 4])>) {
            let is_button = in_button || new_el.button_hover_paint.is_some();
            let mut changed = is_button && new_el.text != old_el.text;
            let mut descendant_damage = None;
            for (new_child, old_child) in
                new_el
                    .children
                    .iter()
                    .filter(|child| !crate::layout::layout_tree::subtree_declares_portal(child))
                    .zip(old_el.children.iter().filter(|child| {
                        !crate::layout::layout_tree::subtree_declares_portal(child)
                    }))
            {
                let (child_changed, child_damage) = walk(new_child, old_child, is_button);
                changed |= child_changed;
                descendant_damage = child_damage.or(descendant_damage);
            }
            if changed && new_el.button_hover_paint.is_some() {
                return (
                    true,
                    Some((
                        super::damage::paint_element_key(new_el),
                        super::damage::union_rects(
                            crate::core::tree::visual_paint_aabb(new_el, new_el.paint_scroll_acc),
                            crate::core::tree::visual_paint_aabb(old_el, old_el.paint_scroll_acc),
                        ),
                    )),
                );
            }
            (changed, descendant_damage)
        }
        walk(new_root, old_root, false).1
    }

    /// Root rebuilds can change a portal's mounted children while the page tree stays intact.
    /// Reconcile reports that as StructureChanged, but scheduling a window clear here makes the
    /// later portal-only damage path ineffective. Compare the non-portal projection so portal
    /// insertion/removal is handled by portal membership damage instead.
    fn portal_only_structure_change(new_root: &Element, old_root: &Element) -> bool {
        fn walk(new_el: &Element, old_el: &Element, in_button: bool) -> bool {
            let in_button = in_button || new_el.button_hover_paint.is_some();
            if new_el.template_id != old_el.template_id
                || new_el.style != old_el.style
                || new_el.text_style != old_el.text_style
                || new_el.image_texture != old_el.image_texture
                || (!in_button && new_el.text != old_el.text)
            {
                return false;
            }
            let new_non_portal: Vec<&Element> = new_el
                .children
                .iter()
                .filter(|child| !crate::layout::layout_tree::subtree_declares_portal(child))
                .collect();
            let old_non_portal: Vec<&Element> = old_el
                .children
                .iter()
                .filter(|child| !crate::layout::layout_tree::subtree_declares_portal(child))
                .collect();
            new_non_portal.len() == old_non_portal.len()
                && new_non_portal
                    .iter()
                    .zip(old_non_portal)
                    .all(|(new_child, old_child)| walk(new_child, old_child, in_button))
        }
        if !crate::layout::layout_tree::subtree_declares_portal(new_root)
            && !crate::layout::layout_tree::subtree_declares_portal(old_root)
        {
            return false;
        }
        walk(new_root, old_root, false)
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
        let Some(path) =
            Self::resolve_template_path(&self.root_element, &mut self.template_paths, template_id)
        else {
            return crate::core::tree::LayoutStatus::Clean;
        };
        let button_damage = Self::button_ancestor_damage(&self.root_element, &path);
        let button_caption = button_damage.is_some();
        if let Some((key, bounds)) = button_damage {
            // Capture the old button footprint before the caption changes. The new footprint is
            // collected after layout; the two records union so shrinking captions erase stale
            // outline/background pixels as well as the glyphs.
            super::damage::push_frame_paint_key(key, bounds);
        }
        let (status, frame_status) = {
            let mut cursor = &mut self.root_element;
            for &index in &path {
                cursor = &mut cursor.children[index];
            }
            let status = patch(cursor);
            // Bind writes the specified target into `style`. Commit the standing
            // CSS tween before dest-reset / collect so the used pose (and dirty
            // AABB) stay at the current frame — not a one-frame flash at `to`.
            if status != crate::core::tree::LayoutStatus::Clean {
                crate::style::commit_style_transitions_after_specified_write(cursor);
            }
            // LayoutChanged: patched node only. StructureChanged: whole subtree —
            // bind apply may flip child `display` / flex (Marker separator hairlines)
            // and the fast path no longer rebuilds Taffy on StructureChanged.
            //
            // A fixed ancestor does not fix descendant text metrics. Recompute
            // the leaf so longer captions do not wrap into their previous width.
            let frame_status = status;
            crate::core::tree::collect_bind_layout_dirty(
                cursor,
                status,
                &mut self.pending_layout_dirty,
            );
            if matches!(frame_status, crate::core::tree::LayoutStatus::StyleChanged) {
                // Paint-only (e.g. bind_text `"10"`→`"11"`): this node's collect slot.
                // Key by Taffy id so node-present dest-reset / glyph capture match
                // even when the label has no SDF fill (gallery MouseEvent log).
                super::damage::push_frame_paint_element(cursor, cursor.paint_scroll_acc);
            }
            if matches!(
                frame_status,
                crate::core::tree::LayoutStatus::LayoutChanged
                    | crate::core::tree::LayoutStatus::StructureChanged
            ) {
                if super::damage::paint_kind_for_element(cursor)
                    == super::damage::PaintDirtyKind::Overlay
                {
                    super::damage::push_frame_paint_element(cursor, cursor.paint_scroll_acc);
                } else if button_caption {
                    // A button caption may change its intrinsic width, but the damage is still
                    // local to the button. Escalating this leaf to page geometry makes every
                    // rapid click clear the retained page and drops the live hover target for a
                    // frame, which is the visible caption flicker.
                    super::damage::push_frame_paint_element(cursor, cursor.paint_scroll_acc);
                } else {
                    self.page_geom_dirty = true;
                }
            }
            (status, frame_status)
        };
        self.layout_status = Self::merge_layout_status(self.layout_status, frame_status);
        if frame_status != crate::core::tree::LayoutStatus::Clean {
            // ViewportsOnly `reuse_idle` would skip-replay idle ancestors (gallery
            // Item variant bind while the clicked button is still hovered).
            crate::layout::collect_cache::invalidate_collect_paint_hash_path(
                &mut self.root_element,
                &path,
            );
        }
        self.template_paths.insert(template_id.to_string(), path);
        status
    }

    fn path_has_button_ancestor(root: &Element, path: &[usize]) -> bool {
        let mut node = root;
        for &index in path {
            if node.button_hover_paint.is_some() {
                return true;
            }
            let Some(next) = node.children.get(index) else {
                return false;
            };
            node = next;
        }
        node.button_hover_paint.is_some()
    }

    fn button_ancestor_damage(root: &Element, path: &[usize]) -> Option<(u64, [f32; 4])> {
        let mut node = root;
        let mut found = None;
        for &index in path {
            if node.button_hover_paint.is_some() {
                found = Some((
                    crate::app::damage::paint_element_key(node),
                    crate::core::tree::visual_paint_aabb(node, node.paint_scroll_acc),
                ));
            }
            node = node.children.get(index)?;
        }
        if node.button_hover_paint.is_some() {
            found = Some((
                crate::app::damage::paint_element_key(node),
                crate::core::tree::visual_paint_aabb(node, node.paint_scroll_acc),
            ));
        }
        found
    }

    /// Resolve a cached or DFS `template_id` path. Miss records [`super::reactive::note_bind_miss`].
    pub(super) fn resolve_template_path(
        root: &Element,
        template_paths: &mut HashMap<String, Vec<usize>>,
        template_id: &str,
    ) -> Option<Vec<usize>> {
        let mut path = template_paths.remove(template_id).unwrap_or_default();
        if !Self::template_path_matches(root, &path, template_id) {
            path.clear();
            if !Self::find_template_path(root, template_id, &mut path) {
                crate::app::reactive::note_bind_miss(template_id);
                return None;
            }
        }
        Some(path)
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
            if self
                .pipeline
                .rebuild_wgsl(&self.device, &self.config, &source)
            {
                log::warn!("hot-reload asset: WGSL reloaded {}", resolved.display());
                reloaded = true;
            } else {
                log::warn!(
                    "hot-reload asset: WGSL compile failed, keeping previous pipeline ({})",
                    resolved.display()
                );
            }
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
                self.window.as_ref(),
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
            self.page_geom_dirty = true;
            self.local_backdrop_valid = false;
            self.mark_full_redraw();
        }
    }

    /// Recreate the swapchain for the current `config` (same dimensions). Used after `Lost` /
    /// `Outdated` without a size change.
    pub(super) fn recover_surface(&mut self) {
        self.surface.configure(&self.device, &self.config);
        super::window_chrome::apply_created_window(
            self.window.as_ref(),
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
        // caret paint). Class `hover:scale/rotate/skew` **enter** is frozen here — content
        // sliding onto a still cursor must not light up every `transition-transform` tile
        // (gallery layout WP jank + sidebar hover flash). `hover:bg/text` **does** enter
        // (CSS `:hover` is geometric, same as Button). Already-hovered nodes still **leave**
        // when the pointer is no longer on their paint AABB. Active CSS tweens still advance
        // via `tick_active_css_transitions`.
        scroll_paint_only: bool,
    ) -> Result<(), crate::renderer::wgpu_compat::SurfaceAcquireError> {
        self.gpu_presented_this_frame = false;
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
        // Snapshot before `inner_size` may drop `last_resized_event` this frame.
        let viewport_resizing = self.last_resized_event.is_some();
        let settling = self.needs_text_settle && self.last_resized_event.is_none() && !rail_anim;
        if settling {
            self.needs_text_settle = false;
            self.layout_status = crate::core::tree::LayoutStatus::LayoutChanged;
            // Geometry may match the last fast frame; force prepare so Exact/Bounds apply fit wrap.
            self.last_text_nodes_cache.clear();
            // Fit remasure reflows the page. Named spin dirties must not keep PartialLoad.
            self.page_geom_dirty = true;
        }
        // Sidebar width tweens reflow the app shell every frame — same wrap churn as live
        // window resize. Fast-path FIT_CACHE for the 200ms tween, then settle once.
        let layout_fast = self.last_resized_event.is_some() || rail_anim;
        // `take_fit_needs_relayout` runs after `page_geom_dirty` is cleared this frame.
        // Flag the next chrome so named spin cannot PartialLoad a page-wide Y shift.
        let page_fit_relayout =
            super::damage::fit_relayout_is_page_geometry(layout_fast, viewport_resizing, settling);
        crate::text::set_layout_text_fast_mode(layout_fast);
        let text_fast = text_force_fast_mode || layout_fast;
        let _text_backend = self.text_backend;
        let render_begin = Instant::now();
        let idle_ms = self.last_render_at.elapsed().as_secs_f32() * 1000.0;
        let inner = self.window.surface_size();
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
        ) && (self.portal_membership_dirty || element_contains_rui_portal(&self.root_element))
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
        // `on_hover` / `{id}_hover` must leave (and enter) with CSS `:hover` on this
        // chrome frame — same `variant_mouse`, before Button / class paint. Post-render
        // reconcile is then an idempotent Rc-refresh after layout.
        crate::app::tree_hit::apply_pointer_hover_reconcile(
            &self.root_element,
            variant_mouse[0],
            variant_mouse[1],
            scroll_paint_only || variant_mouse[0] < 0.0,
        );
        // Button / SidebarMenuButton hover chrome: always sync — `{id}_hover` was just
        // reconciled, including during `scroll_paint_only` (main-pane smooth scroll under
        // a still pointer on the rail). Gating sidebar here left enter/leave signals live
        // but never wrote `sidebar-accent`, so nav items looked dead until scroll settled.
        crate::components::button::sync_button_hover_paint_subtree(
            &mut self.root_element,
            &mut self.layout_tree,
            &mut sync_impact,
            viewport_w,
            viewport_h,
            [0.0, 0.0],
        );
        crate::components::sidebar::sync_sidebar_menu_button_paint_subtree(
            &mut self.root_element,
            &mut self.layout_tree,
            &mut sync_impact,
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
        // `text_style.input_caret` before collect. Same as button hover: do **not** skip
        // this on `scroll_paint_only`. Tick-active hover-leave restores `base_text_style`
        // (caret-less); skipping reinstall drops `input_caret` from `text_nodes`,
        // `caret_wake_at` dies, and Windows coalesces the settle redraw with Wait.
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
        self.damage_nodes
            .extend(super::damage::take_frame_paint_nodes());
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
        let has_region = self
            .damage_nodes
            .iter()
            .any(|d| matches!(d.kind, super::damage::PaintDirtyKind::Region))
            || !sync_collect.display_reveal_keys.is_empty();
        let has_named = !self.damage_nodes.is_empty();
        // One gate for sync_impact **and** pre_chrome. A later pre_chrome
        // `mark_full_redraw` used to undo named-node PartialLoad (Input + spin).
        if matches!(
            super::damage::classify_present_tier(
                pre_chrome_layout_status,
                sync_impact,
                page_geom,
                portal_skip,
                has_named,
                has_region,
            ),
            super::damage::PresentTier::Window
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

        // Seed overlay/zoom closed pose *before* layout so `__modal_zoom` capture
        // sees opacity 0 / scale 0.90. CSS time starts *after* layout — first-open
        // AlertDialog hitch (variant chrome + zoom capture) must not eat the 200ms
        // fade, and the animation must play in full once that hitch is over.
        let modal_needs_frame =
            crate::utils::modal_motion::flush_modal_shell_motion_subtree(&mut self.root_element);
        if modal_needs_frame {
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
        }
        // Content scroll moves every pixel in the viewport. Named spin/shimmer must
        // not stay node-present PartialLoad (dest-reset of *now* leaves stacked
        // labels). `scroll_layer_clear` is that gate — not window FullClear.
        // Resize+StructureChanged may skip scrollbar geometry above; it must not
        // skip this check.
        let scroll_fp = Self::scroll_offset_fingerprint(&self.root_element);
        let scroll_changed = self.has_presented_frame && scroll_fp != self.last_scroll_offset_fp;
        let cur_offsets = Self::scroll_offsets_map(&self.root_element);
        self.scroll_layer_clear = false;
        self.scroll_text_clip = None;
        self.scroll_text_clip_second = None;
        self.scroll_strip_blit.clear();
        self.scroll_blit_pins.clear();
        if scroll_changed {
            // Pixels move inside the scroller: Clear that viewport layer, keep the rest
            // of the retain buffer (sidebar / chrome). Whole-window FullClear is wrong here.
            // Portal join must not cancel this — overlay membership scissors the panel;
            // the page viewport still translated. Fingerprint-only spawn of an overlay
            // ScrollArea is not page scroll (`existing_scroll_offset_changed`).
            let prev_offsets = self.last_scroll_offsets.clone();
            if super::damage::existing_scroll_offset_changed(&prev_offsets, &cur_offsets) {
                if !self.union_scrolled_viewport_damage(&prev_offsets, &cur_offsets) {
                    self.mark_full_redraw();
                } else {
                    self.scroll_layer_clear = true;
                    let (jobs, nested_pins) =
                        self.try_plan_scroll_strip_blits(&prev_offsets, &cur_offsets);
                    if !jobs.is_empty() {
                        self.damage_rect = Some(jobs[0].plan.exposed);
                        self.scroll_text_clip = Some(jobs[0].plan.exposed);
                        self.scroll_text_clip_second = jobs[0].plan.exposed_second;
                        self.scroll_strip_blit = jobs;
                        self.scroll_blit_pins = nested_pins;
                    }
                }
            }
        }
        // Do **not** commit `last_scroll_offset_fp` here. skip_present still ran
        // `update_scroll_offset_in_place`; committing would hide the delta from the
        // next presented frame (page frozen, leftover target dumps on unfreeze).
        // Highlight geometry is restamped during layout collect from paint_visual_bounds.
        // Do **not** refresh_highlight here: it raced collect (stale abs−scroll vs paint
        // rect) and flickered the overlay during / after smooth scroll + pick hover.
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
        // Cold start still needs a full Taffy rebuild. Portal membership (+N hoist)
        // attaches unbuilt overlay nodes and syncs the portal layer's children —
        // recycling the whole gallery dropped frames and is not required.
        // Local `<Show>` structure changes attach unbuilt nodes on the fast path.
        // Portal membership (+N hoist) used to force `build_and_layout` of the
        // gallery every scroll frame the overlay opened — that is the hitch.
        let portal_only_structure =
            (self.portal_membership_dirty || portal_moves > 0) && !page_geom;
        // Cold `rebuild_ui` swaps an unbound tree onto a LayoutTree that already
        // laid out the bootstrap shell. Portal membership still uses the fast path.
        let use_full_taffy_rebuild =
            self.layout_tree.root_node.is_none() || self.root_element.layout_node.is_none();
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
        let geom_followup = viewport_resizing || settling || self.needs_text_settle;
        let scroll_only = !geom_followup
            && ((scrollbar_update_resolved == ScrollbarUpdateMode::ViewportsOnly
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
                    && (portal_only_structure
                        || !matches!(
                            layout_for_build,
                            crate::core::tree::LayoutStatus::StructureChanged
                        ))
                    && (portal_only_structure
                        || !matches!(
                            pre_chrome_layout_status,
                            crate::core::tree::LayoutStatus::StructureChanged
                        ))));
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
        let skip_layout_status = if portal_only_structure
            && matches!(
                layout_for_build,
                crate::core::tree::LayoutStatus::StructureChanged
            ) {
            crate::core::tree::LayoutStatus::StyleChanged
        } else {
            layout_for_build
        };
        let allow_collect_skip = crate::layout::collect_cache::collect_skip_allowed_for_frame(
            skip_layout_status,
            scroll_paint_only,
            use_full_taffy_rebuild,
            viewport_resizing,
            settling || self.needs_text_settle,
        );
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
            layout_prof_sum.collect_stamp_ms += p.collect_stamp_ms;
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
        // Placement is applied inside layout (after Taffy, before collect) so a
        // scroll frame with +N open does not collect the gallery a second time.
        // Floating motion: Expand can read height measurement only after seed/tick of layout (and placement sync).
        // DialogFade CSS: same — tick only after overlay/zoom have bounds so the
        // 200ms enter/exit starts after first-open layout hitch, not during it.
        let modal_css_ticking =
            crate::utils::modal_motion::tick_hoisted_modal_css(&mut self.root_element);
        let floating_needs_frame =
            crate::utils::floating_motion::flush_floating_motion_subtree(&mut self.root_element);
        let floating_css_ticking =
            crate::utils::floating_motion::tick_hoisted_floating_css(&mut self.root_element);
        // Expand writes max-height keyframes that need a same-frame Taffy pass.
        // Fade is opacity-only: a second full-tree collect here dropped frames while
        // a Popover / ColorPicker was open (gallery scroll hitch).
        if crate::utils::floating_motion::floating_motion_needs_relayout() {
            crate::utils::floating_motion::mark_floating_panels_layout_dirty(
                &self.root_element,
                &mut self.layout_tree,
            );
            render_result = self.layout_tree.relayout_overlay_follow_and_collect(
                &mut self.root_element,
                logical_w,
                logical_h,
                mouse_pos,
                scroll_only,
                scrollbar_overlay_collect,
            );
            acc_layout_prof(&render_result.prof);
        } else if modal_css_ticking || floating_css_ticking {
            // The animation clock advanced after layout/collect. Submit its
            // current pose, not last frame's meshes paired with current damage
            // (which clears the scrim before drawing the old panel).
            render_result = self.layout_tree.collect_animation_frame(
                &mut self.root_element,
                mouse_pos,
                scrollbar_overlay_collect,
            );
            acc_layout_prof(&render_result.prof);
        }
        if modal_css_ticking || floating_needs_frame || floating_css_ticking {
            self.window.request_redraw();
        }
        if crate::components::color_picker::is_color_dragging() {
            let live = crate::components::color_picker::snapshot_color_drag_bounds(
                &self.root_element,
                mouse_pos[0],
                mouse_pos[1],
            );
            crate::components::color_picker::queue_color_drag_apply(
                mouse_pos[0],
                mouse_pos[1],
                live,
            );
        }
        drop(acc_layout_prof);
        let layout_done = Instant::now();

        let overlay_split = render_result.overlay_split;
        let ui_scale_now = self.ui_scale();
        // Logical visual slots *before* DPI scale + `take`. Dest-reset multiplies prev by
        // `ui_scale`; recording after scale would double-count, and after take the instance
        // / chrome outset (outline / extra-shadow / caret) is gone.
        let next_last_present = super::draw_cull::last_present_visual_slots(
            &render_result.node_paints,
            &render_result.instances,
            &scale_textured_draws(&render_result.textured_draws, 1.0),
            &render_result.text_nodes,
            &render_result.input_chrome_instances,
        );
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
            self.render_prof_layout_stamp_ms += layout_prof_sum.collect_stamp_ms;
            let (skip, patch) = self.layout_tree.last_collect_skip_counts();
            self.render_prof_skip_n += skip as f32;
            self.render_prof_patch_n += patch as f32;
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
        #[cfg(feature = "svg")]
        {
            let _ = crate::media::svg_raster::flush_svg_textures(|key| {
                self.image_textures.get(key).is_some()
            });
        }
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
            if !crate::renderer::scene3d::scene3d_gpu_enabled() {
                let _ = crate::primitives::scene3d::take_scene3d_jobs();
            } else if crate::primitives::scene3d::has_scene3d_jobs() || self.scene3d.is_some() {
                if self.scene3d.is_none() {
                    match crate::renderer::scene3d::Scene3dGpu::try_new(&self.device) {
                        Some(gpu) => self.scene3d = Some(gpu),
                        None => {
                            crate::renderer::scene3d::disable_scene3d_gpu_runtime();
                            let _ = crate::primitives::scene3d::take_scene3d_jobs();
                        }
                    }
                }
                if let Some(gpu) = self.scene3d.as_mut() {
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
            }
        }
        {
            if crate::primitives::path::has_path_jobs() || self.path2d.is_some() {
                if self.path2d.is_none() {
                    let samples =
                        crate::renderer::path_pipeline::path_msaa_sample_count(&self.adapter);
                    match crate::renderer::path_pipeline::PathGpu::try_new(&self.device, samples) {
                        Some(gpu) => self.path2d = Some(gpu),
                        None => {
                            crate::renderer::geometry2d::disable_gpu2d_runtime();
                            let _ = crate::primitives::path::take_path_jobs();
                        }
                    }
                }
                if let Some(gpu) = self.path2d.as_mut() {
                    if gpu.flush(
                        &self.device,
                        &self.queue,
                        &self.textured_pipeline,
                        &mut self.image_textures,
                    ) {
                        self.window.request_redraw();
                    }
                }
            }
        }
        {
            if !crate::renderer::shader_leaf::shader_gpu_enabled() {
                let _ = crate::primitives::shader::take_shader_jobs();
            } else if crate::primitives::shader::has_shader_jobs() || self.shader.is_some() {
                if self.shader.is_none() {
                    match crate::renderer::shader_leaf::ShaderGpu::try_new(&self.device) {
                        Some(gpu) => self.shader = Some(gpu),
                        None => {
                            crate::renderer::shader_leaf::disable_shader_gpu_runtime();
                            let _ = crate::primitives::shader::take_shader_jobs();
                        }
                    }
                }
                if let Some(gpu) = self.shader.as_mut() {
                    if gpu.flush(
                        &self.device,
                        &self.queue,
                        &self.textured_pipeline,
                        &mut self.image_textures,
                        crate::style::global_time_s(),
                        mouse_pos,
                    ) {
                        self.window.request_redraw();
                    }
                }
            }
        }
        let captures = crate::renderer::take_capture_jobs();
        let effects = crate::renderer::take_effect_jobs();
        let filters = crate::renderer::take_filter_jobs();
        let capture_n = captures.len();
        let fx_begin = Instant::now();
        let mut fx_dirty = false;
        let use_compute = self.gpu_caps.compute;
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
            if use_compute {
                Some(&mut self.compute)
            } else {
                None
            },
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
            if use_compute {
                self.compute.blur.as_mut()
            } else {
                None
            },
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
        // Expand's unconstrained measure frame still emits opacity-0 instances at
        // content height. That AABB is not occupancy (`hoisted_portal_paint_box`)
        // and must not become a scroll blit pin / dest-reset strip down the page.
        let overlay_visual =
            if crate::layout::layout_tree::hoisted_expand_measuring(&self.root_element) {
                None
            } else {
                Self::overlay_visual_bounds_logical(
                    &scaled_instances[overlay_inst_start..],
                    &scaled_tex[overlay_tex_slice_start..],
                    &self.text_nodes[overlay_text_start..],
                    &scaled_input_chrome[overlay_chrome_start..],
                    ui_scale,
                )
            };
        crate::core::tree::push_display_reveal_regions(
            &self.root_element,
            &sync_collect.display_reveal_keys,
        );
        self.damage_nodes
            .extend(super::damage::take_frame_paint_nodes());
        self.apply_portal_overlay_damage(overlay_present, overlay_visual);
        super::draw_cull::restore_culled_dirty_clips(
            &self.root_element,
            &mut render_result.node_paints,
            &self.damage_nodes,
        );
        self.collect_scroll_blit_pins(overlay_visual, &render_result.node_paints);
        // Consumed by `force_full_redraw_after_chrome` / scrollbar gates this frame.
        // Clear here (not inside overlay occupancy) so `note_fit_relayout_followup`
        // can still arm the *next* chrome after text prepare.
        self.page_geom_dirty = false;
        // Same space as `last_present_node_bounds` (visual own-chrome / halo,
        // recorded before DPI scale + `take`). Layout `p.bounds` vs that visual
        // prev makes every outset-shadow slot look translated — gallery inset
        // dest-resets and PartialLoad src-overs the Md halo on idle CSS ticks.
        // Visual slots skip nested foreign descendant ink so a child's
        // hover:scale Bake cannot dest-reset an overflow:visible parent pill.
        if !self.scroll_layer_clear {
            super::damage::expand_paint_dirties_for_translated_slots(
                &mut self.damage_nodes,
                &next_last_present,
                &self.last_present_node_bounds,
            );
            super::damage::expand_paint_dirties_for_clip_culled_slots(
                &mut self.damage_nodes,
                &next_last_present,
                &self.last_present_node_bounds,
                self.layout_tree.last_clip_culled_paint_keys(),
            );
        }
        if super::damage::node_present_moved_forces_full_clear(
            super::damage::unnamed_translated_page_slots(
                &next_last_present,
                &self.last_present_node_bounds,
                &self.damage_nodes,
            ),
            self.scroll_layer_clear,
        ) {
            self.mark_full_redraw();
        }
        // Tier changes must precede both policy selection and glyph capture.
        super::draw_cull::restore_culled_dirty_clips(
            &self.root_element,
            &mut render_result.node_paints,
            &self.damage_nodes,
        );
        super::draw_cull::prepare_node_present_dirties_ex(
            &mut self.damage_nodes,
            &render_result.node_paints,
            &self.last_present_node_bounds,
            &scaled_instances[..overlay_inst_start.min(scaled_instances.len())],
            &scaled_tex[..overlay_tex_start.min(scaled_tex.len())],
            &scaled_input_chrome[..overlay_chrome_start.min(scaled_input_chrome.len())],
            ui_scale,
            self.overlay_occupied,
            &self.last_overlay_stamps,
        );
        let region_present = self.damage_nodes.iter().any(|d| {
            matches!(
                d.kind,
                super::damage::PaintDirtyKind::Region
                    | super::damage::PaintDirtyKind::OverlayRestore
            )
        });
        // A source-over clear quad cannot replace transparent destination alpha.
        // Preserve compositor transparency with a real attachment Clear.
        if region_present && self.compositor_alpha {
            self.force_full_redraw = true;
        }
        let gpu_present = self.plan_gpu_present(ui_scale);
        let cull_phys = gpu_present.cull_phys;
        let page_stack_band_ranges = overlay_split.page_stack_band_ranges();
        let has_page_stack_bands = !page_stack_band_ranges.is_empty();
        // Overlay glyphs use `overlay_text_renderer`. Full and Region presents prepare the
        // page/page-stack glyphon mesh here. Node Present is different: it clears only local
        // holes, so its page captions are restored by `capture_text_renderer` after primitive
        // replay; preparing the page mesh as well would submit overlapping glyphs twice.
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
            && gpu_present.use_partial_damage
            && !region_present;
        // Every cleared pixel needs both primitives and glyphs from this frame.
        // FullClear prepares complete bands; partial repair uses the same holes
        // for text and primitive submission.
        let page_text_cull = if self.scroll_layer_clear && gpu_present.use_partial_damage {
            // Prepare exactly the pixels reset below, including viewport-clear
            // fallback and pins outside the scrolled viewport.
            cull_phys.and_then(|c| {
                super::draw_cull::CullRects::from_rects(c.strips().map(|r| {
                    let s = ui_scale.max(1e-6);
                    [r[0] / s, r[1] / s, r[2] / s, r[3] / s]
                }))
            })
        } else if region_present && gpu_present.use_node_present {
            Some(super::draw_cull::region_repair_text_cull(
                &self.damage_nodes,
                &self.last_present_node_bounds,
                &render_result.node_paints,
                &self.last_overlay_stamps,
                self.overlay_occupied,
                ui_scale,
                self.config.width,
                self.config.height,
            ))
        } else {
            None
        };
        let reuse_page_stack = has_page_stack_bands
            && !region_present
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
        let local_node_text = super::damage::node_present_skips_page_stack_glyphon(
            gpu_present.use_node_present,
            region_present,
        );
        // Node capture prepares the changed captions instead of the page bands.
        // Keep their decoration output too, including the old extent on leave.
        let previous_decoration_bounds: Vec<_> = if local_node_text {
            self.last_text_nodes_cache
                .iter()
                .filter(|n| {
                    n.style.underline_resolved()
                        || n.style.overline_resolved()
                        || n.style.line_through_resolved()
                })
                .map(|n| (n.x, n.y, captured_text_decoration_bounds(n, ui_scale)))
                .collect()
        } else {
            Vec::new()
        };
        let mut captured_underlines = Vec::new();
        let mut captured_decoration_bounds = Vec::new();
        let mut record_capture_decorations =
            |text: &[crate::layout::layout_tree::TextLayoutInfo], lines: Vec<InstanceInput>| {
                if !local_node_text {
                    return;
                }
                captured_underlines.extend(lines);
                for n in text {
                    captured_decoration_bounds.push(captured_text_decoration_bounds(n, ui_scale));
                    for &(x, y, bounds) in &previous_decoration_bounds {
                        if (n.x - x).abs() < 0.02 && (n.y - y).abs() < 0.02 {
                            captured_decoration_bounds.push(bounds);
                        }
                    }
                }
            };
        let shadow_cached_keys: std::collections::HashSet<u64> =
            self.shadow_tex_cache.keys().copied().collect();
        let mut page_stack_band_underlines: Vec<Vec<InstanceInput>> = Vec::new();
        let mut page_stack_band_shadows: Vec<Vec<crate::renderer::prepare_text::ShadowTexDraw>> =
            Vec::new();
        let mut page_stack_skip_glyphon: Vec<bool> = Vec::new();
        let mut skip_page_glyphon = false;
        // Only node present executes the caption repair pass. Full draws must
        // keep complete page runs instead of cutting holes for an absent pass.
        let (caption_replay_runs, caption_glyph_holes) = if gpu_present.use_node_present {
            super::draw_cull::overlay_grazing_caption_replay(
                &self.damage_nodes,
                &render_result.node_paints,
                &self.last_overlay_stamps,
                self.overlay_occupied,
                &self.text_nodes[..overlay_text_start],
                ui_scale,
            )
        } else {
            (Vec::new(), Vec::new())
        };
        let (underline_instances, caret_instances, shadow_tex_draws) = if has_page_stack_bands {
            self.ensure_page_stack_band_text_renderers(page_stack_band_ranges.len());
            let mut underlines = Vec::with_capacity(page_stack_band_ranges.len());
            let mut shadow_bands = Vec::with_capacity(page_stack_band_ranges.len());
            page_stack_skip_glyphon = vec![false; page_stack_band_ranges.len()];
            if local_node_text {
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
                        // Empty later band: drop the last GPU mesh. Skip-render
                        // keeps those vertices off this pass, but the next
                        // prepare/render of a stale buffer stamps leftover glyphs.
                        if band_i > 0 {
                            self.clear_page_stack_band_glyphon(band_i);
                        }
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
                        gpu_present.use_partial_damage && !region_present,
                    )
                    .map(|runs| {
                        super::draw_cull::page_submit_excluding_live_node_capture(
                            runs.into_owned(),
                            &self.text_nodes[..overlay_text_start],
                            &self.damage_nodes,
                            &render_result.node_paints,
                            &self.last_overlay_stamps,
                            self.overlay_occupied,
                            gpu_present.use_node_present,
                        )
                    })
                    .map(|runs| {
                        super::draw_cull::page_runs_excluding_caption_replay(
                            runs,
                            &caption_replay_runs,
                        )
                    })
                    .filter(|runs| !runs.is_empty()) else {
                        // Unchanged runs (sidebar after Send) or outside the scrolled viewport.
                        // Keep last glyphon GPU buffer; do not submit it on Load retain.
                        // A caption owned by node capture is dropped here so the Latin
                        // suffix is not src-overed a second time on a trigger toggle.
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
                    self.note_fit_relayout_followup(page_fit_relayout, scroll_paint_only);
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
                gpu_present.use_partial_damage && !region_present,
            )
            .map(|runs| {
                super::draw_cull::page_submit_excluding_live_node_capture(
                    runs.into_owned(),
                    &self.text_nodes[..overlay_text_start],
                    &self.damage_nodes,
                    &render_result.node_paints,
                    &self.last_overlay_stamps,
                    self.overlay_occupied,
                    gpu_present.use_node_present,
                )
            })
            .map(|runs| {
                super::draw_cull::page_runs_excluding_caption_replay(runs, &caption_replay_runs)
            })
            .filter(|runs| !runs.is_empty())
            {
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
                        self.note_fit_relayout_followup(page_fit_relayout, scroll_paint_only);
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
        let overlay_kinds: Vec<super::damage::PaintDirtyKind> =
            self.damage_nodes.iter().map(|d| d.kind).collect();
        let prepare_overlay_glyphs = super::damage::overlay_glyphon_prepare_needed(
            gpu_present.use_node_present,
            &overlay_kinds,
        );
        if !prepare_overlay_glyphs {
            // Reuse the last portal mesh. Reshaping it on every trigger
            // transition tick is the hover-switch hitch; the bands are not
            // encoded on a page-node present.
            for _ in &overlay_band_ranges {
                overlay_band_underlines.push(Vec::new());
                overlay_band_shadows.push(Vec::new());
            }
        }
        for (band_i, band) in overlay_band_ranges.iter().enumerate() {
            if !prepare_overlay_glyphs {
                continue;
            }
            let text_lo = band.text.0;
            let text_hi = band.text.1;
            if text_lo >= text_hi {
                overlay_band_underlines.push(Vec::new());
                overlay_band_shadows.push(Vec::new());
                continue;
            }
            let shadow_cached_keys: std::collections::HashSet<u64> =
                self.shadow_tex_cache.keys().copied().collect();
            let overlay_text = overlay_band_text_for_prepare(
                &self.text_nodes,
                text_lo,
                text_hi,
                band.stamp_key,
                &self.last_overlay_stamps,
            );
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
            if overlay_prep.culled_skip_prepare {
                self.clear_overlay_band_glyphon(band_i);
            }
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
        self.note_fit_relayout_followup(page_fit_relayout, scroll_paint_only);
        let page_inst = &scaled_instances[..overlay_inst_start.min(scaled_instances.len())];
        let page_tex = &scaled_tex[..overlay_tex_start.min(scaled_tex.len())];
        let page_chrome =
            &scaled_input_chrome[..overlay_chrome_start.min(scaled_input_chrome.len())];
        let prev_slot_for = |key: u64| {
            self.last_present_node_bounds
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, b)| *b)
        };
        let node_needs_glyphs = |d: &super::damage::PaintDirty| {
            super::draw_cull::node_present_needs_glyphs_scaled(
                d.key,
                d.bounds,
                &render_result.node_paints,
                page_inst,
                page_tex,
                page_chrome,
                ui_scale,
                prev_slot_for(d.key),
            )
        };
        let page_node_occluded = |d: &super::damage::PaintDirty| {
            super::damage::page_node_present_occluded_by_overlay(
                d.bounds,
                &self.last_overlay_stamps,
            )
        };
        let skip_covered_nested = |d: &super::damage::PaintDirty| {
            let layered = super::draw_cull::layered_overlay_slot_bounds(
                d.key,
                d.bounds,
                &render_result.node_paints,
            )
            .is_some();
            super::draw_cull::skip_covered_nested_node_present(
                d,
                &self.damage_nodes,
                layered,
                &render_result.node_paints,
                page_inst,
                ui_scale,
            )
        };
        let page_text_for_glyphs = &self.text_nodes[..overlay_text_start];
        let node_in_glyph_dirties = |d: &super::damage::PaintDirty| match d.kind {
            super::damage::PaintDirtyKind::OverlayRestore => false,
            // Own-chrome regions skip the page glyphon run. Child captions in
            // the hole (select trigger value) are covered by the fill unless
            // this capture puts them back. Flush draws that capture once.
            super::damage::PaintDirtyKind::Region => super::draw_cull::region_needs_glyph_capture(
                d,
                &render_result.node_paints,
                page_text_for_glyphs,
            ),
            super::damage::PaintDirtyKind::Node => {
                !super::damage::paint_dirty_covered_by_overlay_restore(
                    d,
                    &self.damage_nodes,
                    &self.last_overlay_stamps,
                ) && !skip_covered_nested(d)
                    && !super::draw_cull::skip_in_flow_under_layered_overlay(
                        d,
                        &self.damage_nodes,
                        &render_result.node_paints,
                        page_inst,
                    )
                    && node_needs_glyphs(d)
                    && !page_node_occluded(d)
                    && !super::draw_cull::paint_dirty_is_caret_overlay(
                        d,
                        &render_result.node_paints,
                        page_inst,
                    )
            }
            super::damage::PaintDirtyKind::Overlay => false,
        };
        let mut capture_underlay_ready = false;
        // Fade ticks must not redraw a caption the Restore scissor left in
        // retain. Flushing it after the shadow blends coverage against the
        // animating shadow, so the unchanged Latin suffix flickers.
        let capture_text_ready = if gpu_present.use_node_present
            && self.damage_nodes.iter().any(node_in_glyph_dirties)
        {
            let page_text = &self.text_nodes[..overlay_text_start];
            let glyph_dirties: Vec<_> = self
                .damage_nodes
                .iter()
                .copied()
                .filter(node_in_glyph_dirties)
                .filter(|d| {
                    let s = super::draw_cull::node_present_scissor_logical(
                        d.kind,
                        d.key,
                        d.bounds,
                        &self.last_present_node_bounds,
                        &render_result.node_paints,
                        super::draw_cull::node_present_scissor_pad(d.kind),
                    );
                    s[2] > 0.5 && s[3] > 0.5
                })
                .collect();
            let scale = ui_scale.max(1e-6);
            let wipe_clips: Vec<(u64, [f32; 4])> = glyph_dirties
                .iter()
                .filter(|d| d.kind == super::damage::PaintDirtyKind::Node)
                .filter_map(|d| {
                    let wipe = super::draw_cull::node_present_dest_reset_full(
                        d.key,
                        d.bounds,
                        &render_result.node_paints,
                        page_inst,
                        page_tex,
                        page_chrome,
                        ui_scale,
                        prev_slot_for(d.key),
                    )?
                    .rect;
                    let logical = [
                        wipe[0] / scale,
                        wipe[1] / scale,
                        wipe[2] / scale,
                        wipe[3] / scale,
                    ];
                    Some((
                        d.key,
                        super::draw_cull::node_present_glyph_clip_logical(
                            d,
                            Some(logical),
                            ui_scale,
                        ),
                    ))
                })
                .collect();
            let underlay_text = if glyph_dirties.iter().any(|d| {
                super::draw_cull::node_present_flush_layered_underlay_glyphs(
                    d.key,
                    d.bounds,
                    &render_result.node_paints,
                )
            }) {
                super::draw_cull::node_present_layered_underlay_capture_text_wipes(
                    &glyph_dirties,
                    page_text,
                    &render_result.node_paints,
                    &wipe_clips,
                    page_inst,
                    ui_scale,
                )
            } else {
                Vec::new()
            };
            if !underlay_text.is_empty() {
                let shadow_keys: std::collections::HashSet<u64> =
                    self.shadow_tex_cache.keys().copied().collect();
                let prep = prepare_text_frame(
                    &mut self.capture_underlay_text_renderer,
                    &self.device,
                    &self.queue,
                    &self.config,
                    &underlay_text,
                    ui_scale,
                    text_fast,
                    &shadow_keys,
                    &self.shadow_rect_cache,
                );
                capture_underlay_ready = !prep.culled_skip_prepare;
                if capture_underlay_ready {
                    record_capture_decorations(&underlay_text, prep.underline_instances);
                }
            }
            let mut hole_text = if glyph_dirties.is_empty() {
                Vec::new()
            } else {
                super::draw_cull::node_present_clipped_capture_text_wipes_excluding_overlay(
                    &glyph_dirties,
                    page_text,
                    &render_result.node_paints,
                    &wipe_clips,
                    &self.last_overlay_stamps,
                    page_inst,
                    ui_scale,
                )
            };
            // A trigger/ancestor capture can own this same line. Grazing replay
            // is its single late submission source for this frame.
            hole_text = super::draw_cull::page_runs_excluding_caption_replay(
                hole_text,
                &caption_replay_runs,
            );
            hole_text.extend(caption_replay_runs.iter().cloned());
            if hole_text.is_empty() {
                false
            } else {
                let shadow_keys: std::collections::HashSet<u64> =
                    self.shadow_tex_cache.keys().copied().collect();
                let prep = prepare_text_frame(
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
                if !prep.culled_skip_prepare {
                    record_capture_decorations(&hole_text, prep.underline_instances);
                    true
                } else {
                    false
                }
            }
        } else {
            false
        };
        let caption_flush: Vec<[f32; 4]> =
            if caption_replay_runs.is_empty() || !gpu_present.use_node_present {
                Vec::new()
            } else if capture_text_ready {
                caption_glyph_holes.clone()
            } else {
                let shadow_keys: std::collections::HashSet<u64> =
                    self.shadow_tex_cache.keys().copied().collect();
                let prep = prepare_text_frame(
                    &mut self.capture_text_renderer,
                    &self.device,
                    &self.queue,
                    &self.config,
                    &caption_replay_runs,
                    ui_scale,
                    text_fast,
                    &shadow_keys,
                    &self.shadow_rect_cache,
                );
                if prep.culled_skip_prepare {
                    Vec::new()
                } else {
                    record_capture_decorations(&caption_replay_runs, prep.underline_instances);
                    caption_glyph_holes.clone()
                }
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
            local_node_text.then_some((&captured_underlines, &captured_decoration_bounds)),
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
                // Dest AABB is often the rest pose; CSS scale/rotate lives here.
                for v in d.transform {
                    v.to_bits().hash(&mut hasher);
                }
            }
            hash_present_underline_layers(
                &mut hasher,
                &presented_underlines,
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
                n.paint_scale.to_bits().hash(&mut hasher);
                for v in n.paint_transform {
                    v.to_bits().hash(&mut hasher);
                }
                n.paint_origin[0].to_bits().hash(&mut hasher);
                n.paint_origin[1].to_bits().hash(&mut hasher);
                // Caret index / selection must affect present even when underline instance
                // bytes happen to collide (e.g. empty selection vs no caret).
                match &n.style.input_caret {
                    None => 0u8.hash(&mut hasher),
                    Some(c) => {
                        1u8.hash(&mut hasher);
                        c.caret_byte_index.hash(&mut hasher);
                        c.selection.hash(&mut hasher);
                        c.extra_carets.hash(&mut hasher);
                        c.extra_selections.hash(&mut hasher);
                        c.overwrite_highlight.hash(&mut hasher);
                        c.selection_bg.hash(&mut hasher);
                        c.preedit_ul.hash(&mut hasher);
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
                    log::warn!(
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
                self.gpu_presented_this_frame = false;
                return Ok(());
            }
            self.last_gpu_frame_fp = frame_fp;
            self.last_presented_underlines = presented_underlines.clone();
        }

        // Acquire swapchain image as late as possible so CPU-side text/layout work can overlap
        // with present pacing; this typically reduces blocking time in get_current_texture().
        let acquire_begin = Instant::now();
        let output = crate::renderer::wgpu_compat::acquire_surface(&self.surface)?;
        self.last_scroll_offset_fp = scroll_fp;
        self.last_scroll_offsets = cur_offsets;
        self.gpu_presented_this_frame = true;
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
        let mut strip_copy_committed = self.scroll_strip_blit.is_empty();
        if use_partial_damage && self.present_retain.contents_valid() {
            if !self.scroll_strip_blit.is_empty() {
                let mut copied = 0usize;
                for job in &self.scroll_strip_blit {
                    let Some(copy) = super::damage::scroll_strip_copy_physical(
                        &job.plan,
                        ui_scale,
                        self.config.width,
                        self.config.height,
                    ) else {
                        break;
                    };
                    if !self.present_retain.copy_scroll_strip(
                        &mut encoder,
                        copy.src_x,
                        copy.src_y,
                        copy.dst_x,
                        copy.dst_y,
                        copy.width,
                        copy.height,
                    ) {
                        break;
                    }
                    super::damage::note_scroll_strip_blit();
                    copied += 1;
                }
                strip_copy_committed = copied == self.scroll_strip_blit.len();
                if !strip_copy_committed {
                    // Load + strip-only redraw over an unmoved retain is empty
                    // cards / doubled glyphs. Keep this frame's pixels; FullClear next.
                    self.force_full_redraw = true;
                    self.window.request_redraw();
                }
            }
        }
        let retain_unmoved_scroll = super::damage::should_retain_unmoved_scroll(
            use_partial_damage,
            !self.scroll_strip_blit.is_empty(),
            strip_copy_committed,
        );
        let clear_color = crate::renderer::surface_clear_color(self.compositor_alpha);
        let color_load = if use_partial_damage {
            wgpu::LoadOp::Load
        } else {
            wgpu::LoadOp::Clear(clear_color)
        };
        if !use_partial_damage {
            scissor_physical = None;
        }

        let mut live_backdrops: Vec<wgpu::BindGroup> = Vec::new();
        let mut band_backdrop_idx: Vec<Option<usize>> = vec![None; page_stack_band_ranges.len()];
        // Capture submits its own encoder (like subtree `render_capture`) so prefix
        // glyphon is flushed to the scene RT before this frame encoder reuses the
        // same vertex buffers. Must run **before** modal frost: the modal scene pass
        // samples these bind groups so local + modal blurs stack.
        if should_capture_page_stack_local_backdrops(allow_primitives, has_page_stack_bands) {
            let scroll_source_moved = self.scroll_layer_clear || !self.scroll_strip_blit.is_empty();
            let mut dirty_logical: Vec<[f32; 4]> = Vec::new();
            if let Some(r) = self.damage_rect {
                dirty_logical.push(r);
            }
            for n in &self.damage_nodes {
                dirty_logical.push(n.bounds);
            }
            for (band_i, band) in page_stack_band_ranges.iter().enumerate() {
                // `render_result.instances` was `take`n into `scaled_instances` (physical).
                // Sampling the empty buffer made local_blur always 0 — dummy backdrop, no frost.
                let local_blur = band_instance_backdrop_blur(&scaled_instances, band);
                if local_blur <= 0.5 {
                    continue;
                }
                let radius_px = local_blur * ui_scale;
                let host = local_backdrop_host_scissor_physical(
                    &scaled_instances,
                    band,
                    self.config.width,
                    self.config.height,
                );
                let capture_region = host.map(|sc| {
                    expand_physical_scissor(
                        sc,
                        local_backdrop_kernel_pad_px(radius_px),
                        self.config.width,
                        self.config.height,
                    )
                });
                let recapture = local_backdrop_must_recapture(
                    self.local_backdrop_valid && self.backdrop_blur.blurred_view().is_some(),
                    use_partial_damage,
                    scroll_source_moved,
                    self.overlay_restore_clear,
                    capture_region,
                    &dirty_logical,
                    ui_scale,
                );
                if recapture {
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
                        radius_px,
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
                        if self.gpu_caps.compute {
                            Some(&mut self.compute)
                        } else {
                            None
                        },
                        &scaled_instances,
                        capture_region,
                    );
                    self.local_backdrop_valid = self.backdrop_blur.blurred_view().is_some();
                }
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
                        let dummy = self.pipeline.dummy_backdrop();
                        for (band_i, band) in page_stack_band_ranges.iter().enumerate() {
                            let band_backdrop = page_stack_band_backdrop(
                                band_i,
                                &band_backdrop_idx,
                                &live_backdrops,
                                dummy,
                            );
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
                                    band_backdrop,
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
                                page_pass.set_bind_group(1, band_backdrop, &[]);
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
                            // I-beam is a swapchain overlay after retain blit.
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
                                && !page_stack_skip_glyphon
                                    .get(band_i)
                                    .copied()
                                    .unwrap_or(false)
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
                    // I-beam is a swapchain overlay after retain blit.
                }
                // Ramp blur σ with overlay opacity (CSS `backdrop-filter` under a fading
                // element). Keep the blit itself opaque — dimming is the underlay rect.
                self.backdrop_blur.blur(
                    &self.device,
                    &self.queue,
                    &mut encoder,
                    max_backdrop_blur_logical * modal_underlay_opacity * ui_scale,
                    if self.gpu_caps.compute {
                        Some(&mut self.compute)
                    } else {
                        None
                    },
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

            let strip_draw_scissors: Vec<(u32, u32, u32, u32)> = if scissor_physical.is_none()
                && !self.scroll_strip_blit.is_empty()
                && use_partial_damage
                && !retain_unmoved_scroll
            {
                match cull_phys {
                    Some(cull) => cull
                        .strips()
                        .filter_map(|r| {
                            super::damage::physical_xywh_to_scissor(
                                r,
                                self.config.width,
                                self.config.height,
                            )
                        })
                        .collect(),
                    None => Vec::new(),
                }
            } else {
                Vec::new()
            };
            if let Some((x, y, w, h)) = scissor_physical {
                render_pass.set_scissor_rect(x, y, w, h);
            }
            super::frame_stats::begin_scene_batches();
            if gpu_present.clear_damage && strip_copy_committed {
                if let Some(cull) = cull_phys {
                    for rect in cull.strips() {
                        let Some((x, y, w, h)) = super::damage::physical_xywh_to_scissor(
                            rect,
                            self.config.width,
                            self.config.height,
                        ) else {
                            continue;
                        };
                        render_pass.set_scissor_rect(x, y, w, h);
                        draw_scissor_surface_clear(
                            &mut render_pass,
                            &self.pipeline,
                            &self.device,
                            rect,
                            clear_color,
                        );
                    }
                    if let Some((x, y, w, h)) = scissor_physical {
                        render_pass.set_scissor_rect(x, y, w, h);
                    }
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
            let page_scissor_passes: Vec<Option<(u32, u32, u32, u32)>> =
                if strip_draw_scissors.is_empty() {
                    vec![None]
                } else {
                    strip_draw_scissors.iter().copied().map(Some).collect()
                };
            for page_sc in page_scissor_passes {
                if let Some((x, y, w, h)) = page_sc {
                    render_pass.set_scissor_rect(x, y, w, h);
                }
                if retain_unmoved_scroll {
                    continue;
                }
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
                        &render_result.clip_polys,
                        &scaled_tex,
                        &scaled_input_chrome,
                        overlay_inst_start,
                        overlay_tex_start,
                        overlay_paint_start,
                        allow_primitives,
                        clear_color,
                        ui_scale,
                        &overlay_band_ranges,
                        &page_stack_band_ranges,
                        &page_stack_skip_glyphon,
                        skip_page_glyphon,
                        &page_stack_band_underlines,
                        &page_stack_band_shadows,
                        &band_backdrop_idx,
                        &live_backdrops,
                        capture_text_ready,
                        capture_underlay_ready,
                        &presented_underlines,
                        &prev_presented_underlines,
                        &caption_flush,
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
                                !draw_to_retain,
                            );
                            if band.text.0 < band.text.1
                                && !page_stack_skip_glyphon
                                    .get(band_i)
                                    .copied()
                                    .unwrap_or(false)
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

                if !use_backdrop_blur
                    && !has_page_stack_bands
                    && !retain_page_pixels
                    && !gpu_present.use_node_present
                {
                    draw_shadow_tex_draws(
                        &mut render_pass,
                        &shadow_tex_draws,
                        &self.shadow_tex_cache,
                        &self.textured_pipeline,
                        &self.device,
                        cull_phys,
                    );
                }

                if !has_page_stack_bands
                    && !underline_instances.is_empty()
                    && !use_backdrop_blur
                    && !retain_page_pixels
                    && !gpu_present.use_node_present
                {
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

                if !has_page_stack_bands
                    && !use_backdrop_blur
                    && !retain_page_pixels
                    && !gpu_present.use_node_present
                    && !skip_page_glyphon
                {
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
                    && !draw_to_retain
                {
                    let chrome_slice = instances_for_upload(
                        &scaled_input_chrome[..overlay_chrome_start],
                        cull_phys,
                    );
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
                if !caret_instances.is_empty()
                    && !retain_page_pixels
                    && !gpu_present.use_node_present
                    && !draw_to_retain
                {
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
            }
            if !strip_draw_scissors.is_empty() {
                render_pass.set_scissor_rect(0, 0, self.config.width, self.config.height);
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
                        super::frame_stats::note_scene_batch(
                            super::frame_stats::SceneKind::Text,
                            1,
                        );
                    }

                    if allow_primitives && !draw_to_retain && band.chrome.0 < band.chrome.1 {
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
                }
            }
        }
        if draw_to_retain {
            let pipeline = &self.pipeline;
            let device = &self.device;
            let dummy = pipeline.dummy_backdrop();
            let chrome = scaled_input_chrome.as_slice();
            self.present_retain
                .blit_to_surface(&mut encoder, &view, clear_color, |pass| {
                    draw_input_caret_overlay(pass, pipeline, device, dummy, chrome);
                });
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
        self.force_full_redraw = retain_unmoved_scroll;
        self.last_present_node_bounds = next_last_present;
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
                let (hole_chrome, hole_band, sdf_bg) = present_hole_counts();
                log::warn!(
                    "slow render {total_ms:.0}ms skip_present=false idle={idle_ms:.0} pre={:.1} layout={:.1} rebuild={:.1} taffy={:.1} collect={:.1} setup={:.1} passes={layout_passes} full={} portal={portal_moves} upload={:.1} fx={:.1} text={:.1} acquire={:.1} encode={:.1} submit={:.1} present={:.1} captures={capture_n} modal_css={} hole_chrome={hole_chrome} hole_band={hole_band} sdf_bg={sdf_bg}",
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
                    "render profile(last ~{:.0}ms): fps≈{:.1}, pre={:.2}ms, layout={:.2}ms, draw={:.2}ms [layout_tree: rebuild={:.2} setup={:.2} taffy={:.2} collect={:.2} stamp={:.2} skip={:.0} patch={:.0}] [upload={:.2}, acquire={:.2}, text={:.2}, encode={:.2}, submit={:.2}, present={:.2}]",
                    elapsed.as_millis() as f32,
                    f / elapsed.as_secs_f32().max(0.001),
                    self.render_prof_pre_ms / f,
                    self.render_prof_layout_ms / f,
                    self.render_prof_draw_ms / f,
                    self.render_prof_layout_rebuild_ms / f,
                    self.render_prof_layout_setup_ms / f,
                    self.render_prof_layout_taffy_ms / f,
                    self.render_prof_layout_collect_ms / f,
                    self.render_prof_layout_stamp_ms / f,
                    self.render_prof_skip_n / f,
                    self.render_prof_patch_n / f,
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
                self.render_prof_layout_stamp_ms = 0.0;
                self.render_prof_skip_n = 0.0;
                self.render_prof_patch_n = 0.0;
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
    // `auto` cubic is paint-only (collect × live peek). Baking it here made
    // ViewportsOnly hover/hide frames paint stale alpha; Full+skip_flex then
    // rewrote thumb Taffy size (crushed-top) and the 8px hit slab.
    let v: f32 = if vis == "never" { 0.0 } else { 1.0 };

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
    //
    // Do **not** treat unset viewport signals as "not scrollable". Match page
    // switches run Full prepass before post-render publishes vw/vh/cw/ch; collapsing
    // here zeros the overlay track, and subsequent ViewportsOnly frames never
    // restore hit geometry — later pages then have no bar on wheel or hover.
    let metrics_ready = vw > 0.5 && vh > 0.5;
    let collapse_auto = vis != "always" && !scrollable && metrics_ready;
    if !axis_allows || vis == "never" || collapse_auto {
        // In-flow bars must drop both axes or the flex lane stays (horizontal
        // track stays short of the vertical thickness). Overlay bars are
        // absolute: zeroing the cross size moves the inset anchor onto the far
        // edge. Collect then draws the bar outward from that point, and the
        // fade dest-reset only covers the later in-box rect — the outside
        // strip stays (wrap=off textarea, gray band under the field).
        let overlay = el.attrs.get("rui_scrollbar_overlay").map(String::as_str) == Some("1");
        if overlay && axis == "x" {
            el.style.size.width = Dimension::length(0.0);
            el.style.min_size.width = Dimension::length(0.0);
        } else if overlay {
            el.style.size.height = Dimension::length(0.0);
            el.style.min_size.height = Dimension::length(0.0);
        } else {
            el.style.size.width = Dimension::length(0.0);
            el.style.size.height = Dimension::length(0.0);
            el.style.min_size.width = Dimension::length(0.0);
            el.style.min_size.height = Dimension::length(0.0);
        }
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
    let y_vis = use_signal(&scrollbar_visibility_signal_key(&rest, "y"), || {
        "auto".to_string()
    })
    .peek();
    let y_lane = vertical_scrollbar_occupies_lane(allow_y, y_vis.trim(), vh, ch);
    // Only the side facing the vertical bar. Replacing both insets with the
    // shared edge pad moved textarea bars (left 4 → 2) on the first frame, and
    // a collect-time stretch then painted outside the box the fade clears.
    if axis == "x" && !this_inset_outside {
        let y_side = use_signal(&scroll_scrollbar_y_side_signal_key(&rest), || {
            "right".to_string()
        })
        .peek();
        let on_left = y_side.trim().eq_ignore_ascii_case("left");
        let current = inset_len_px(if on_left {
            &el.style.inset.left
        } else {
            &el.style.inset.right
        });
        let v_cross = el
            .attrs
            .get(ATTR_H_LANE_V_CROSS)
            .and_then(|s| s.parse::<f32>().ok())
            .filter(|px| px.is_finite() && *px > 0.0)
            .unwrap_or(base_thickness);
        let edge = el
            .attrs
            .get(ATTR_H_LANE_INSET)
            .and_then(|s| s.parse::<f32>().ok())
            .unwrap_or_else(|| {
                // Legacy builds baked v_cross into the inset; recover the edge pad.
                let edge = if current > SCROLLBAR_EDGE_PAD + 0.5 {
                    (current - v_cross).max(SCROLLBAR_EDGE_PAD)
                } else {
                    current.max(0.0)
                };
                el.set_attr(ATTR_H_LANE_INSET, edge.to_string());
                edge
            });
        let next = horizontal_lane_side_inset(edge, v_cross, y_lane);
        if (current - next).abs() > 0.5 {
            let v = taffy::style::LengthPercentageAuto::length(next);
            if on_left {
                el.style.inset.left = v;
            } else {
                el.style.inset.right = v;
            }
        }
    }

    // Keep constant cross-size; show/hide is paint-only cubic (collect), not size.
    // Hit testing uses `layout_bounds`, not multiplied alpha.
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
        // Last frame still included the vertical lane. Size the thumb to the
        // released length now; Taffy catches the track box up on this layout.
        if axis == "x" && !y_lane && main_available > laid + 0.5 {
            main_available
        } else if laid > 1.0 {
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

/// Test hook: same scrollbar prepass as `render` (`Full` / `ViewportsOnly`).
#[cfg(test)]
pub(crate) fn update_scrollbar_elements_for_test(
    el: &mut Element,
    layout_tree: &mut crate::layout::layout_tree::LayoutTree,
    mode: ScrollbarUpdateMode,
) {
    let mut base = std::collections::HashMap::new();
    update_scrollbar_elements(el, &mut base, layout_tree, mode, None);
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
            return Some(n * crate::layout::dimension::css_root_font_size_px());
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
mod button_caption_damage_tests {
    use super::AppState;
    use crate::components::button::button;
    use crate::core::tree::Element;

    #[test]
    fn caption_layout_change_stays_local_to_button() {
        let root = Element::new().child(
            Element::from(button("trigger").text("Open"))
                .child(Element::new().template_id("trigger-caption")),
        );
        assert!(AppState::path_has_button_ancestor(&root, &[0, 0]));
        assert!(!AppState::path_has_button_ancestor(&root, &[]));
    }

    #[test]
    fn conditional_button_caption_change_is_local_layout_only() {
        let old: Element = button("trigger").text("打开 RSX Trigger").into();
        let new: Element = button("trigger").text("关闭 RSX Trigger").into();
        assert!(AppState::button_caption_only_change(&new, &old));
        let old_page = Element::new().child(old);
        let new_page = Element::new().child(new);
        assert!(AppState::button_caption_only_change(&new_page, &old_page));

        let old_other = Element::new().child(Element::new().text("Open"));
        let new_other = Element::new().child(Element::new().text("Close"));
        assert!(!AppState::button_caption_only_change(
            &new_other, &old_other
        ));

        let old_structural: Element = button("trigger").text("Open").into();
        let mut new_structural: Element = button("trigger").text("Close").into();
        new_structural.children.push(Element::new());
        assert!(!AppState::button_caption_only_change(
            &new_structural,
            &old_structural
        ));
        assert!(AppState::has_button_caption_change(
            &new_structural,
            &old_structural
        ));
    }

    #[test]
    fn portal_only_structure_change_does_not_schedule_window_clear() {
        let page_old: Element = crate::components::button::button("trigger")
            .text("Open")
            .into();
        let page_new: Element = crate::components::button::button("trigger")
            .text("Close")
            .into();
        let mut old_portal = Element::new().template_id("panel").text("old");
        old_portal.set_attr("rui_portal", "1");
        let mut new_portal = Element::new().template_id("panel").text("new");
        new_portal.set_attr("rui_portal", "1");
        let old = Element::new().child(page_old).child(old_portal);
        let new = Element::new().child(page_new).child(new_portal);
        assert!(AppState::portal_only_structure_change(&new, &old));

        let changed_page = Element::new()
            .child(Element::new().template_id("page").text("changed"))
            .child({
                let mut panel = Element::new().template_id("panel").text("new");
                panel.set_attr("rui_portal", "1");
                panel
            });
        assert!(!AppState::portal_only_structure_change(&changed_page, &old));
    }

    #[test]
    fn mixed_portal_caption_change_targets_trigger_only() {
        let mut old_button: Element = crate::components::button::button("trigger")
            .text("Open")
            .into();
        let mut new_button: Element = crate::components::button::button("trigger")
            .text("Close")
            .into();
        old_button.layout_bounds = [10.0, 20.0, 80.0, 24.0];
        new_button.layout_bounds = [10.0, 20.0, 92.0, 24.0];
        let mut old_panel = Element::new().template_id("panel").text("old");
        old_panel.set_attr("rui_portal", "1");
        let mut new_panel = Element::new().template_id("panel").text("new");
        new_panel.set_attr("rui_portal", "1");
        let old = Element::new().child(old_button).child(old_panel);
        let new = Element::new().child(new_button).child(new_panel);

        let damage = AppState::portal_button_caption_damage(&new, &old)
            .expect("caption change must produce trigger damage");
        assert_ne!(damage.0, 0);
        assert!(damage.1[2] > 0.0 && damage.1[3] > 0.0);
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
    fn breadcrumb_node_capture_draws_hover_underline_and_clears_on_leave() {
        use crate::app::damage::{clear_frame_paint_nodes, take_frame_paint_nodes};
        use crate::app::draw_cull::node_present_clipped_capture_text;
        use crate::app::tree_hit::{apply_pointer_hover_reconcile, clear_hover_tracking};
        use crate::components::breadcrumb::breadcrumb;
        use crate::components::button::sync_button_hover_paint_subtree;
        use crate::core::reactivity::create_root;
        use crate::core::tree::{Element, LayoutStatus};
        use crate::layout::layout_tree::LayoutTree;
        use crate::renderer::{offscreen::OffscreenGpu, prepare_text_frame, RuiTextRenderer};
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
            let mut renderer = RuiTextRenderer::new(&gpu.device, &gpu.queue, &config);
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
                let captured = node_present_clipped_capture_text(
                    &dirty,
                    &frame.text_nodes,
                    &frame.node_paints,
                );
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
}

#[cfg(test)]
mod overlay_visual_bounds_tests {
    use super::AppState;
    use crate::renderer::InstanceInput;
    use bytemuck::Zeroable;

    fn rect(x: f32, y: f32, w: f32, h: f32, alpha: f32) -> InstanceInput {
        let mut i = InstanceInput::zeroed();
        i.rect_pos = [x, y];
        i.rect_size = [w, h];
        i.color[3] = alpha;
        i.transform = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
        i.clip_size = InstanceInput::NO_CLIP_SIZE;
        i
    }

    #[test]
    fn transparent_fullscreen_portal_shell_does_not_expand_avatar_bounds() {
        let shell = rect(0.0, 0.0, 1280.0, 720.0, 0.0);
        let avatar_panel = rect(120.0, 80.0, 160.0, 48.0, 1.0);

        let bounds =
            AppState::overlay_visual_bounds_logical(&[shell, avatar_panel], &[], &[], &[], 1.0);

        assert_eq!(bounds, Some([118.0, 78.0, 164.0, 52.0]));
    }

    #[test]
    fn transparent_shell_does_not_hide_backdrop_visual_occupancy() {
        let mut shell = rect(0.0, 0.0, 1280.0, 720.0, 0.0);
        shell.backdrop_blur = 12.0;

        let bounds = AppState::overlay_visual_bounds_logical(&[shell], &[], &[], &[], 1.0);

        assert_eq!(bounds, Some([-2.0, -2.0, 1284.0, 724.0]));
    }
}
