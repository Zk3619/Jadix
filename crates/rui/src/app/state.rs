#[cfg(debug_assertions)]
use std::collections::HashMap;
use std::sync::Arc;

use wgpu::util::DeviceExt;
use winit::dpi::PhysicalSize;
use winit::window::Window;

use crate::core::tree::{Element, LayoutTree};
use crate::renderer::{
    prepare_text_frame, scale_instances, scale_textured_draws, RuiPipeline, RuiTextRenderer,
    TexturedInstanceInput, TexturedQuadPipeline,
};
use crate::{
    components::{
        scroll_area::{
            scroll_content_h_signal_key, scroll_content_w_signal_key, scroll_offset_x_signal_key,
            scroll_offset_y_signal_key, scroll_overflow_x_signal_key, scroll_overflow_y_signal_key,
            scroll_scrollbar_inset_x_signal_key, scroll_scrollbar_inset_y_signal_key,
            scroll_scrollbar_x_side_signal_key, scroll_scrollbar_y_side_signal_key,
            scroll_viewport_h_signal_key, scroll_viewport_w_signal_key, SCROLL_AREA_FOCUS_PREFIX,
        },
        scrollbar::scrollbar_visibility_signal_key,
        scrollbar::{SCROLLBAR_THUMB_FOCUS_PREFIX, SCROLLBAR_TRACK_FOCUS_PREFIX},
    },
    core::reactivity::use_signal,
};
use taffy::prelude::Dimension;
use std::collections::hash_map::Entry;
#[cfg(debug_assertions)]
use std::sync::atomic::{AtomicUsize, Ordering};
#[cfg(debug_assertions)]
use std::time::Instant;

#[cfg(debug_assertions)]
static SCROLL_PREPASS_RECURSE_COUNT: AtomicUsize = AtomicUsize::new(0);
#[cfg(debug_assertions)]
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

pub(super) struct AppState {
    pub(super) window: Arc<Window>,
    pub(super) surface: wgpu::Surface<'static>,
    pub(super) device: wgpu::Device,
    pub(super) queue: wgpu::Queue,
    pub(super) config: wgpu::SurfaceConfiguration,
    pub(super) size: winit::dpi::PhysicalSize<u32>,
    pub(super) pipeline: RuiPipeline,
    textured_pipeline: TexturedQuadPipeline,
    image_textures: std::collections::HashMap<String, super::image_registry::GpuImageTexture>,
    textured_instance_buffer: wgpu::Buffer,
    textured_instance_capacity: usize,
    pub(super) instance_buffer: wgpu::Buffer,
    instance_buffer_capacity: usize,
    pub(super) layout_tree: LayoutTree,
    pub(super) text_renderer: RuiTextRenderer,
    pub(super) root_element: Element,
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
    /// Latest `WindowEvent::Resized` size. `inner_size()` can briefly lag during maximize; merging
    /// avoids a one-frame swapchain smaller than the DWM client area (black letterboxing at edges).
    last_resized_event: Option<PhysicalSize<u32>>,
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
    last_caret_blink_phase: u128,
}

impl AppState {
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
        // Tolerate tiny layout jitter so text-frame cache is stable across frames.
        const TEXT_GEOM_EPS: f32 = 0.25;
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
        crate::accesskit_bridge::tree_update_from_elements(&self.root_element, fid)
    }

    fn logical_size(&self) -> (f32, f32) {
        let scale = self.ui_scale().max(1.0);
        (self.config.width as f32 / scale, self.config.height as f32 / scale)
    }

    pub(super) async fn new(window: Arc<Window>, mut root_element: Element) -> Self {
        let size = window.inner_size();
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..Default::default()
        });

        let surface = instance.create_surface(window.clone()).unwrap();

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .unwrap();

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    label: None,
                    memory_hints: wgpu::MemoryHints::Performance,
                },
                None,
            )
            .await
            .unwrap();

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(surface_caps.formats[0]);

        let present_mode = surface_caps
            .present_modes
            .iter()
            .copied()
            .find(|m| matches!(m, wgpu::PresentMode::Mailbox))
            .or_else(|| {
                surface_caps
                    .present_modes
                    .iter()
                    .copied()
                    .find(|m| matches!(m, wgpu::PresentMode::Fifo))
            })
            .unwrap_or(surface_caps.present_modes[0]);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 1,
        };
        surface.configure(&device, &config);

        let pipeline = RuiPipeline::new(&device, &config);
        let textured_pipeline = TexturedQuadPipeline::new(&device, &config);
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
        recompute_scroll_viewport_flags(&mut root_element);
        let _portal = crate::layout::layout_tree::hoist_portal_subtrees(&mut root_element);
        let render_result = layout_tree.build_and_layout(&mut root_element, logical_w, logical_h, [0.0, 0.0]);
        let scaled_instances = scale_instances(&render_result.instances, scale);
        let num_instances = render_result.instances.len() as u32;
        let last_render_scroll_metrics = render_result.scroll_metrics;
        let last_hover_cursor = render_result.hover_cursor;

        let instance_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Instance Buffer"),
            contents: bytemuck::cast_slice(&scaled_instances),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });

        let text_renderer = RuiTextRenderer::new(&device, &queue, &config);

        Self {
            window,
            surface,
            device,
            queue,
            config,
            size,
            pipeline,
            textured_pipeline,
            image_textures: std::collections::HashMap::new(),
            textured_instance_buffer,
            textured_instance_capacity: 256,
            instance_buffer,
            instance_buffer_capacity: scaled_instances.len(),
            layout_tree,
            text_renderer,
            root_element,
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
            last_resized_event: None,
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
            last_caret_blink_phase: 0,
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

        log::warn!("[rebuild_ui] START new_children={} old_children={}", root_element.children.len(), self.root_element.children.len());
        let status = Self::reconcile_elements(&mut root_element, &mut self.root_element);
        log::warn!("[rebuild_ui] END status={:?}", status);
        self.root_element = root_element;
        recompute_scroll_viewport_flags(&mut self.root_element);
        self.layout_status = status;
    }

    pub(crate) fn reconcile_subtree(
        &mut self,
        anchor_id: &str,
        mut new_fragment: Element,
    ) -> crate::core::tree::LayoutStatus {
        use crate::core::tree::LayoutStatus;

        // Fine-grained anchors must resolve deterministically. `hot_reload::template_id_matches` is
        // intentionally fuzzy (path suffix + line) for dev-time patches; using it here can match a
        // *different* node earlier in DFS and replace the wrong subtree — e.g. sibling buttons +
        // `<Show>` vanishing intermittently.
        let want = normalize_anchor_template_id(anchor_id);
        let status = if let Some(old_subtree) =
            find_reactive_subtree_by_anchor_id_mut(&mut self.root_element, &want)
        {
            let status = Self::reconcile_elements(&mut new_fragment, old_subtree);
            *old_subtree = new_fragment;
            status
        } else {
            LayoutStatus::StructureChanged
        };

        recompute_scroll_viewport_flags(&mut self.root_element);
        self.layout_status = Self::merge_layout_status(self.layout_status, status);
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
    ) -> crate::core::tree::LayoutStatus {
        use crate::core::tree::LayoutStatus;

        let id_match = new_el.template_id == old_el.template_id;

        if id_match {
            new_el.layout_node = old_el.layout_node;
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
                    let promoted = std::mem::replace(old_el, Element::new());
                    *new_el = promoted;
                    new_el.template_id = tid;
                }
                return LayoutStatus::Clean;
            }
            
            let mut status = if new_el.text != old_el.text {
                LayoutStatus::LayoutChanged
            } else if new_el.style != old_el.style {
                LayoutStatus::LayoutChanged
            } else if new_el.text_style != old_el.text_style {
                LayoutStatus::StyleChanged
            } else if new_el.image_texture != old_el.image_texture {
                LayoutStatus::LayoutChanged
            } else {
                LayoutStatus::Clean
            };

            let new_all_keyed = Self::children_all_keyed(&new_el.children);
            let old_all_keyed = Self::children_all_keyed(&old_el.children);
            if new_all_keyed && old_all_keyed {
                status = Self::merge_layout_status(status, Self::reconcile_keyed_children(new_el, old_el));
            } else if new_el.children.len() == old_el.children.len() {
                for (n_child, o_child) in new_el.children.iter_mut().zip(old_el.children.iter_mut()) {
                    let child_status = Self::reconcile_elements(n_child, o_child);
                    status = Self::merge_layout_status(status, child_status);
                }
            } else {
                log::warn!(
                    "[reconcile] children len mismatch new={} old={} tid={:?} keyed=({},{})",
                    new_el.children.len(), old_el.children.len(),
                    new_el.template_id, new_all_keyed, old_all_keyed
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
        Self::reconcile_elements(new_el, old_el)
    }

    fn children_all_keyed(children: &[Element]) -> bool {
        // `Iterator::all` is vacuously true on an empty slice; keyed reconciliation is only defined
        // for non-empty child lists (empty vs empty uses the positional same-length branch).
        !children.is_empty() && children.iter().all(|c| c.template_id.is_some())
    }

    fn reconcile_keyed_children(new_el: &mut Element, old_el: &mut Element) -> crate::core::tree::LayoutStatus {
        use crate::core::tree::LayoutStatus;

        let mut status = LayoutStatus::Clean;
        let mut old_by_id: std::collections::HashMap<String, Element> = std::collections::HashMap::new();
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
                let child_status = Self::reconcile_elements(&mut new_child, &mut old_child);
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
        recurse(&mut self.root_element, target_id, offset, SCROLL_AREA_FOCUS_PREFIX);
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
                replacements = hot_reload_inject::derive_replacements_from_previous(&previous_overrides, tpl);
            }
            let mut effective_tpl = tpl.clone();
            effective_tpl.text_replacements = replacements;
            hot_reload_inject::upsert_overrides(&mut self.hot_reload_overrides, std::slice::from_ref(&effective_tpl));
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
        let dims_changed =
            self.config.width != new_size.width || self.config.height != new_size.height;
        self.size = new_size;
        self.config.width = new_size.width;
        self.config.height = new_size.height;
        // Only reconfigure when the backing size changes. Calling `configure` on every occlusion
        // / redraw causes visible flicker; use `recover_surface` after `SurfaceError::Lost` instead.
        if dims_changed {
            self.surface.configure(&self.device, &self.config);
            self.pipeline
                .update_resolution(&self.queue, self.config.width, self.config.height);
            self.textured_pipeline
                .update_resolution(&self.queue, self.config.width, self.config.height);
            self.layout_status = crate::core::tree::LayoutStatus::StructureChanged;
        }
    }

    /// Recreate the swapchain for the current `config` (same dimensions). Used after `Lost` /
    /// `Outdated` without a size change.
    pub(super) fn recover_surface(&mut self) {
        self.surface.configure(&self.device, &self.config);
    }

    pub(super) fn render(
        &mut self,
        mouse_pos: [f32; 2],
        scrollbar_update: ScrollbarUpdateMode,
        active_drag_rest: Option<&str>,
        text_force_fast_mode: bool,
    ) -> Result<(), wgpu::SurfaceError> {
        #[cfg(debug_assertions)]
        let render_begin = Instant::now();
        let inner = self.window.inner_size();
        if inner.width > 0 && inner.height > 0 {
            let mut w = inner.width;
            let mut h = inner.height;
            if let Some(e) = self.last_resized_event {
                w = w.max(e.width);
                h = h.max(e.height);
            }
            let target = PhysicalSize::new(w, h);
            if target.width != self.config.width || target.height != self.config.height {
                self.resize(target);
            }
            if self.last_resized_event.is_some_and(|e| {
                inner.width >= e.width && inner.height >= e.height
            }) {
                self.last_resized_event = None;
            }
        }

        // Chrome sync (hover/focus/input/switch) must not feed into scrollbar prepass or Taffy
        // rebuild gates: merging it into `layout_status` forced `Full` scrollbar passes and
        // `build_and_layout` every time a button shadow or hover border changed, which reset
        // scrollbar styles/springs and broke auto hide + caused flicker.
        let pre_chrome_layout_status = self.layout_status;

        let mut sync_impact = crate::core::tree::LayoutStatus::Clean;
        crate::components::button::sync_button_hover_paint_subtree(
            &mut self.root_element,
            &mut self.layout_tree,
            &mut sync_impact,
        );
        crate::components::input::sync_input_paint_subtree(
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

        let layout_for_build = Self::merge_layout_status(pre_chrome_layout_status, sync_impact);

        let portal_moves = crate::layout::layout_tree::hoist_portal_subtrees(&mut self.root_element);

        // On `StructureChanged`, viewport signals from the *previous* frame are stale (e.g. first
        // frame after maximize). Feeding them into `update_scrollbar_elements` corrupts Taffy
        // styles before layout and produces a small “island” of UI on a large surface.
        let skip_scroll_preupdate = matches!(
            pre_chrome_layout_status,
            crate::core::tree::LayoutStatus::StructureChanged
        );
        let mut scrollbar_update_resolved = scrollbar_update;
        if !skip_scroll_preupdate {
            // Only `LayoutChanged` / `StructureChanged` from the *element tree* (reconcile/rebuild)
            // promote to a full scrollbar prepass — not per-frame chrome sync.
            scrollbar_update_resolved = match pre_chrome_layout_status {
                crate::core::tree::LayoutStatus::StructureChanged
                | crate::core::tree::LayoutStatus::LayoutChanged => ScrollbarUpdateMode::Full,
                crate::core::tree::LayoutStatus::StyleChanged
                | crate::core::tree::LayoutStatus::Clean => scrollbar_update,
            };
            update_scrollbar_elements(
                &mut self.root_element,
                &mut self.scrollbar_base_style,
                &mut self.layout_tree,
                scrollbar_update_resolved,
                active_drag_rest,
            );
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
        let use_full_taffy_rebuild = self.layout_tree.root_node.is_none()
            || portal_moves > 0
            || matches!(
                pre_chrome_layout_status,
                crate::core::tree::LayoutStatus::StructureChanged
                    | crate::core::tree::LayoutStatus::LayoutChanged
            );
        let render_result = if use_full_taffy_rebuild {
            self.layout_tree.build_and_layout(&mut self.root_element, logical_w, logical_h, mouse_pos)
        } else {
            // Scroll offsets do not affect Taffy sizing; skip full `compute_layout` when only
            // viewports were synced (no scrollbar thumb/style changes this frame).
            let scroll_only = matches!(
                layout_for_build,
                crate::core::tree::LayoutStatus::Clean
            ) && scrollbar_update_resolved == ScrollbarUpdateMode::ViewportsOnly;
            // When the scrollbar prepass is `ViewportsOnly`, track/thumb Taffy styles are not updated.
            // Chrome may still set `layout_for_build` to `StyleChanged`, forcing `!scroll_only` layout.
            // Collect must keep using signal-derived scrollbar geometry in that case — otherwise
            // draw uses stale Taffy sizes (thin/wrong tracks) until the next `Clean` frame.
            let scrollbar_overlay_collect =
                scrollbar_update_resolved == ScrollbarUpdateMode::ViewportsOnly;
            self.layout_tree.build_and_layout_fast(
                &mut self.root_element,
                logical_w,
                logical_h,
                mouse_pos,
                scroll_only,
                scrollbar_overlay_collect,
            )
        };
        #[cfg(debug_assertions)]
        let layout_done = Instant::now();
        
        let scaled_instances = scale_instances(&render_result.instances, self.ui_scale());
        self.num_instances = render_result.instances.len() as u32;
        self.text_nodes = render_result.text_nodes;
        
        // Export collected metrics and hover state for window_events.rs to avoid redundant traversals.
        self.last_render_scroll_metrics = render_result.scroll_metrics;
        self.last_hover_cursor = render_result.hover_cursor;

        if scaled_instances.len() > self.instance_buffer_capacity {
            self.instance_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Instance Buffer"),
                contents: bytemuck::cast_slice(&scaled_instances),
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            });
            self.instance_buffer_capacity = scaled_instances.len();
        } else {
            self.queue.write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(&scaled_instances));
        }
        self.layout_status = crate::core::tree::LayoutStatus::Clean;

        // Keep the shader `globals.resolution` aligned with the surface every frame (avoids one-frame
        // mismatch if `configure` / layout / uniform updates reorder across events).
        self.pipeline
            .update_resolution(&self.queue, self.config.width, self.config.height);
        self.textured_pipeline
            .update_resolution(&self.queue, self.config.width, self.config.height);
        #[cfg(debug_assertions)]
        let upload_done = Instant::now();

        let ui_scale = self.ui_scale();

        for (id, rgba, w, h) in super::pending_images::take_pending() {
            if self.image_textures.contains_key(&id) {
                continue;
            }
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
        let has_dynamic_text = self
            .text_nodes
            .iter()
            .any(|n| n.style.input_caret.is_some());
        let caret_phase = crate::renderer::caret_blink_phase();
        let text_cache_valid = self.last_text_ui_scale.to_bits() == ui_scale.to_bits()
            && self.last_text_surface_size == [self.config.width, self.config.height]
            && Self::text_nodes_equal(&self.text_nodes, &self.last_text_nodes_cache)
            && (!has_dynamic_text || self.last_caret_blink_phase == caret_phase);
        let underline_instances = if text_cache_valid {
            self.last_underline_instances.clone()
        } else {
            let new_underlines = prepare_text_frame(
                &mut self.text_renderer,
                &self.device,
                &self.queue,
                &self.config,
                &self.text_nodes,
                ui_scale,
                text_force_fast_mode,
            );
            self.last_text_nodes_cache = self.text_nodes.clone();
            self.last_text_ui_scale = ui_scale;
            self.last_text_surface_size = [self.config.width, self.config.height];
            self.last_underline_instances = new_underlines.clone();
            self.last_caret_blink_phase = caret_phase;
            new_underlines
        };
        #[cfg(debug_assertions)]
        let text_done = Instant::now();

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

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.95,
                            g: 0.95,
                            b: 0.95,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_pipeline(&self.pipeline.render_pipeline);
            render_pass.set_bind_group(0, &self.pipeline.bind_group, &[]);
            render_pass.set_vertex_buffer(0, self.pipeline.vertex_buffer.slice(..));
            render_pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
            render_pass
                .set_index_buffer(self.pipeline.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
            if self.num_instances > 0 {
                render_pass.draw_indexed(0..self.pipeline.num_indices, 0, 0..self.num_instances);
            }

            if !scaled_tex.is_empty() {
                let tex_stride = std::mem::size_of::<TexturedInstanceInput>();
                let mut ti = 0;
                while ti < scaled_tex.len() {
                    let key = scaled_tex[ti].texture_key.clone();
                    let Some(gpu_img) = self.image_textures.get(&key) else {
                        ti += 1;
                        continue;
                    };
                    let run = scaled_tex[ti..]
                        .iter()
                        .take_while(|d| d.texture_key == key)
                        .count();
                    let insts: Vec<TexturedInstanceInput> = scaled_tex[ti..ti + run]
                        .iter()
                        .map(|d| TexturedInstanceInput {
                            rect_pos: [d.rect[0], d.rect[1]],
                            rect_size: [d.rect[2], d.rect[3]],
                            tint: [1.0, 1.0, 1.0, 1.0],
                        })
                        .collect();
                    if !insts.is_empty() {
                        if insts.len() > self.textured_instance_capacity {
                            let new_cap = insts.len().max(256);
                            self.textured_instance_buffer =
                                self.device.create_buffer(&wgpu::BufferDescriptor {
                                    label: Some("Textured instance buffer"),
                                    size: (tex_stride * new_cap) as u64,
                                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                                    mapped_at_creation: false,
                                });
                            self.textured_instance_capacity = new_cap;
                        }
                        self.queue.write_buffer(
                            &self.textured_instance_buffer,
                            0,
                            bytemuck::cast_slice(&insts),
                        );
                        render_pass.set_pipeline(&self.textured_pipeline.render_pipeline);
                        render_pass.set_bind_group(0, &gpu_img.bind_group, &[]);
                        render_pass.set_vertex_buffer(0, self.textured_pipeline.vertex_buffer.slice(..));
                        render_pass.set_vertex_buffer(1, self.textured_instance_buffer.slice(..));
                        render_pass.set_index_buffer(
                            self.textured_pipeline.index_buffer.slice(..),
                            wgpu::IndexFormat::Uint16,
                        );
                        render_pass.draw_indexed(
                            0..self.textured_pipeline.num_indices,
                            0,
                            0..insts.len() as u32,
                        );
                    }
                    ti += run;
                }
            }

            if !underline_instances.is_empty() {
                let underline_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Underline Instance Buffer"),
                    contents: bytemuck::cast_slice(&underline_instances),
                    usage: wgpu::BufferUsages::VERTEX,
                });
                render_pass.set_vertex_buffer(1, underline_buffer.slice(..));
                render_pass.draw_indexed(
                    0..self.pipeline.num_indices,
                    0,
                    0..underline_instances.len() as u32,
                );
            }

            self.text_renderer.render(&mut render_pass);
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
                    "render profile(last ~{:.0}ms): fps≈{:.1}, pre={:.2}ms, layout={:.2}ms, draw={:.2}ms [upload={:.2}, acquire={:.2}, text={:.2}, encode={:.2}, submit={:.2}, present={:.2}]",
                    elapsed.as_millis() as f32,
                    f / elapsed.as_secs_f32().max(0.001),
                    self.render_prof_pre_ms / f,
                    self.render_prof_layout_ms / f,
                    self.render_prof_draw_ms / f,
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
/// Scrollbar edge padding must match `components::scroll_area::SCROLLBAR_EDGE_PAD`.
const SCROLLBAR_EDGE_PAD_RUNTIME: f32 = 2.0;

/// Thickness encoded on the track via `max_w` / `max_h` in `scrollbar.rs`; must match default
/// `ScrollbarBuilder::thickness` when ScrollArea does not override.
const SCROLLBAR_DEFAULT_CROSS_PX: f32 = 10.0;

fn recompute_scroll_viewport_flags(el: &mut Element) -> bool {
    let self_is = el
        .focus_id
        .as_deref()
        .is_some_and(|f| f.starts_with(SCROLL_AREA_FOCUS_PREFIX));
    let mut any_child = false;
    for child in &mut el.children {
        if recompute_scroll_viewport_flags(child) {
            any_child = true;
        }
    }
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
    fn subtree_contains_scroll_rest(el: &Element, target_rest: &str) -> bool {
        if let Some(fid) = el.focus_id.as_deref() {
            if fid
                .strip_prefix(SCROLL_AREA_FOCUS_PREFIX)
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
        // Fast path: once we hit a ScrollArea viewport node, sync only its offset and stop
        // descending into that subtree. This avoids walking large sibling lists every frame.
        if let Some(fid) = el.focus_id.as_deref() {
            if let Some(rest) = fid.strip_prefix(SCROLL_AREA_FOCUS_PREFIX) {
                let sx = use_signal(&scroll_offset_x_signal_key(rest), || 0.0_f32).peek();
                let sy = use_signal(&scroll_offset_y_signal_key(rest), || 0.0_f32).peek();
                el.scroll_offset = [sx, sy];
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
            if let Some(rest) = fid.strip_prefix(SCROLL_AREA_FOCUS_PREFIX) {
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
    for child in &mut el.children {
        update_scrollbar_elements(child, base_style, layout_tree, mode, active_drag_rest);
    }

    let Some(fid) = el.focus_id.as_deref() else {
        return;
    };

    let (rest, axis, is_thumb) = if let Some(prefix) = fid.strip_prefix(SCROLLBAR_THUMB_FOCUS_PREFIX)
    {
        let (rest, axis) = prefix.rsplit_once(':').unwrap_or((prefix, "y"));
        (rest.to_string(), axis, true)
    } else if let Some(prefix) = fid.strip_prefix(SCROLLBAR_TRACK_FOCUS_PREFIX) {
        let (rest, axis) = prefix.rsplit_once(':').unwrap_or((prefix, "y"));
        (rest.to_string(), axis, false)
    } else if let Some(rest) = fid.strip_prefix(SCROLL_AREA_FOCUS_PREFIX) {
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
    // Use a softer threshold to avoid near-boundary flicker around 1px.
    let can_scroll_x = allow_x && cw > vw + 0.5;
    let can_scroll_y = allow_y && ch > vh + 0.5;
    let axis_allows = if axis == "x" { allow_x } else { allow_y };
    let scrollable = if axis == "x" { can_scroll_x } else { can_scroll_y };

    // Visibility mode: only Auto uses spring-driven show/hide.
    let vis = use_signal(
        &scrollbar_visibility_signal_key(&rest, axis),
        || "auto".to_string(),
    )
    .peek();
    let vis = vis.trim().to_ascii_lowercase();
    let v = if vis == "never" {
        0.0
    } else if vis == "always" {
        1.0
    } else {
        crate::style::use_spring(&format!("scrollbar_spring_{rest}:{axis}"), 0.0)
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

    // Collapse only when this axis is disallowed or explicitly hidden. For `auto`/`always`,
    // keep a non-zero hit slab while `!scrollable` so hover can expand the bar; thumb fills track.
    if !axis_allows || vis == "never" {
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
    let inset_x = use_signal(&scroll_scrollbar_inset_x_signal_key(&rest), || "inside".to_string()).peek();
    let inset_y = use_signal(&scroll_scrollbar_inset_y_signal_key(&rest), || "inside".to_string()).peek();
    let x_inside = !inset_x.eq_ignore_ascii_case("outside");
    let y_inside = !inset_y.eq_ignore_ascii_case("outside");
    let y_side = use_signal(&scroll_scrollbar_y_side_signal_key(&rest), || "right".to_string()).peek();
    let x_side = use_signal(&scroll_scrollbar_x_side_signal_key(&rest), || "bottom".to_string()).peek();
    let y_is_left = y_side.eq_ignore_ascii_case("left");
    let x_is_top = x_side.eq_ignore_ascii_case("top");
    let corner_reserve = base_thickness + SCROLLBAR_EDGE_PAD_RUNTIME;

    let mut reserve_left = 0.0_f32;
    let mut reserve_right = 0.0_f32;
    let mut reserve_top = 0.0_f32;
    let mut reserve_bottom = 0.0_f32;
    if axis == "x" && x_inside && y_inside && can_scroll_y {
        if y_is_left {
            reserve_left = corner_reserve;
        } else {
            reserve_right = corner_reserve;
        }
    }
    if axis == "y" && x_inside && y_inside && can_scroll_x {
        if x_is_top {
            reserve_top = corner_reserve;
        } else {
            reserve_bottom = corner_reserve;
        }
    }

    // Inside (absolute) scrollbar tracks have `top/bottom` (or `left/right`) insets set at
    // build time in `scroll_area.rs`. Once we set an explicit `size`, Taffy ignores the far-side
    // inset — so we must subtract the edge pads ourselves. Outside (relative) tracks have no
    // absolute insets, so no subtraction is needed.
    let this_inside = match axis {
        "x" => x_inside,
        _ => y_inside,
    };
    let edge_pad_main = if this_inside { 2.0 * SCROLLBAR_EDGE_PAD_RUNTIME } else { 0.0 };

    // Keep constant cross-size; show/hide is alpha-only (`v` = scrollbar spring current).
    // Do not clamp alpha to 0 while 0 < v < 0.01: during spring fade-in `v` stays tiny for several
    // frames — forcing zero here made `auto` scrollbars look like hover never worked. Hit testing
    // uses `layout_bounds`, not multiplied alpha.
    let inset = 1.0_f32;
    let cross = SCROLLBAR_HIT_SLAB_CROSS.max(base_thickness).max(2.0);
    let thumb_cross = (cross - inset * 2.0).max(1.0);

    match axis {
        "x" => {
            el.style.size.width =
                Dimension::Length((vw - reserve_left - reserve_right - edge_pad_main).max(1.0));
            el.style.size.height = Dimension::Length(cross);
        }
        _ => {
            el.style.size.width = Dimension::Length(cross);
            el.style.size.height =
                Dimension::Length((vh - reserve_top - reserve_bottom - edge_pad_main).max(1.0));
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
        // Fade border width as well to avoid “outline pops”.
        el.style.border_width = bs.track_border_width * visible_alpha;
        thumb.style.border_width = bs.thumb_border_width * visible_alpha;
    }

    {
        match axis {
            "x" => {
                let track = (vw - reserve_left - reserve_right - edge_pad_main).max(1.0);
                let track_inner = (track - inset * 2.0).max(1.0);
                let (tw, ml) = if scrollable {
                    let ratio = (vw / cw.max(vw)).clamp(0.0, 1.0);
                    let min_thumb = track_inner.min(24.0);
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
                let track = (vh - reserve_top - reserve_bottom - edge_pad_main).max(1.0);
                let track_inner = (track - inset * 2.0).max(1.0);
                let (th, mt) = if scrollable {
                    let ratio = (vh / ch.max(vh)).clamp(0.0, 1.0);
                    let min_thumb = track_inner.min(24.0);
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
pub(super) fn inject_text_literals_by_id(root: &mut Element, template_id: &str, text_literals: &[String]) -> bool {
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
    let Some(template_root) = find_by_template_id_and_index_mut(root, template_id, usize::MAX) else {
        return false;
    };
    let mut changed = false;

    // First, group attributes by element index because we don't want to re-traverse for every single attribute
    let mut by_index: std::collections::HashMap<usize, Vec<&crate::app::hot_reload::StyleAttr>> =
        std::collections::HashMap::new();
    for attr in attrs {
        by_index.entry(attr.element_index).or_default().push(attr);
    }

    for (idx, element_attrs) in by_index {
        let mut current_idx = 0usize;
        if let Some(target) = find_element_at_index_inner(template_root, idx, &mut current_idx) {
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

pub(super) fn find_by_template_id_mut<'a>(el: &'a mut Element, id: &str) -> Option<&'a mut Element> {
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
        let p = parts[..parts.len()-2].join(":");
        let l = parts[parts.len()-2].parse().unwrap_or(0);
        let i = parts[parts.len()-1].parse().unwrap_or(usize::MAX);
        (p, l, i)
    } else if parts.len() == 2 {
        (parts[0].to_string(), parts[1].parse().unwrap_or(0), usize::MAX)
    } else {
        (id.to_string(), 0, usize::MAX)
    }
}

fn path_component_match_fuzzy(a: &str, b: &str) -> bool {
    let a = a.replace('\\', "/").to_ascii_lowercase();
    let b = b.replace('\\', "/").to_ascii_lowercase();
    if a == b { return true; }
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

fn replace_text_at_index_checked(el: &mut Element, target_index: usize, from: &str, to: &str) -> bool {
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

