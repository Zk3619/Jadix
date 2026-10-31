//! Secondary window GPU + UI tree (MVP).
//!
//! Shares the primary [`super::state::AppState`]'s `Device` / `Queue` / `Instance`,
//! and hosts an independent surface, layout tree, and root element.

use std::collections::HashMap;
use std::sync::Arc;

use wgpu::util::DeviceExt;
use winit::dpi::PhysicalSize;
use winit::window::Window;

use crate::core::reactivity::OwnerHandle;
use crate::core::tree::{Element, LayoutStatus, LayoutTree};
use crate::renderer::{
    prepare_text_frame, scale_clip_polys_in_place, scale_instances, scale_textured_draws,
    JadixPipeline, JadixTextRenderer, ScaledTexturedDraw, TexturePool, TexturedQuadPipeline,
};

/// Factory for a secondary window's root [`Element`].
///
/// Invoked only on the event-loop thread after [`super::JadixUserEvent::OpenWindow`]
/// is delivered. Marked `Send` so it can hop through [`winit::event_loop::EventLoopProxy`];
/// prefer calling [`super::multi_window::open_secondary_window`] from the UI thread
/// (or via [`super::ui_bridge::run_on_ui_thread`]).
pub struct SecondaryUiBuilder(pub Box<dyn FnMut() -> Element>);

impl std::fmt::Debug for SecondaryUiBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SecondaryUiBuilder(..)")
    }
}

// SAFETY: builders run only on the UI / event-loop thread after proxy delivery.
unsafe impl Send for SecondaryUiBuilder {}

/// Per-secondary-window render state (independent UI tree + swapchain).
pub(super) struct SecondaryWindowState {
    // Drop order: reverse of declaration — surface before device / window.
    pub(super) window: Arc<dyn Window>,
    /// Owned when this window uses a compositor-alpha GPU (Vulkan sidecar).
    _glass_instance: Option<wgpu::Instance>,
    _glass_adapter: Option<wgpu::Adapter>,
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    size: PhysicalSize<u32>,
    pipeline: JadixPipeline,
    pub(super) present_retain: crate::renderer::PresentRetainPipeline,
    pub(super) theme_transition: crate::renderer::theme_transition::ThemeTransition,
    /// Lucide caption icons / Image / SVG collect as `textured_draws`. The primary
    /// window uploads these; secondaries (especially glass sidecars) must too.
    textured_pipeline: TexturedQuadPipeline,
    image_textures: TexturePool,
    textured_instance_buffer: crate::renderer::buffer_upload::RetainedBuffer,
    instance_buffer: crate::renderer::buffer_upload::RetainedBuffer,
    num_instances: u32,
    pub(super) layout_tree: LayoutTree,
    pub(super) text_renderer: JadixTextRenderer,
    /// Band 0 portal overlay glyphs (Dialog / Select menu / etc.).
    overlay_text_renderer: JadixTextRenderer,
    /// Extra stacked portal bands (band_i >= 1).
    overlay_band_text_renderers: Vec<JadixTextRenderer>,
    pub(super) root_element: Element,
    pub(super) layout_status: LayoutStatus,
    pub(super) template_paths: HashMap<String, Vec<usize>>,
    pub(super) pending_layout_dirty: Vec<taffy::prelude::NodeId>,
    #[allow(dead_code)]
    pub(super) last_hover_cursor: Option<crate::core::tree::CursorValue>,
    pub(super) last_render_scroll_metrics: Vec<(String, [f32; 4], [f32; 2], [f32; 2])>,
    pub(super) scroll_metrics_cache: HashMap<String, ([f32; 4], [f32; 2], [f32; 2])>,
    scrollbar_base_style: HashMap<String, super::state::ScrollbarBaseStyle>,
    scrollbar_activity: HashMap<String, ([f32; 2], [web_time::Instant; 2])>,
    scroll_motion: HashMap<String, [super::scroll_support::SmoothScrollAxis; 2]>,
    last_frame_at: web_time::Instant,
    #[allow(dead_code)]
    pub(super) damage_rect: Option<[f32; 4]>,
    pub(super) ime_allowed_latched: bool,
    /// Latest `WindowEvent::Focused` for this secondary (IME HWND drive / soft-blur).
    pub(super) os_focused: bool,
    /// Soft-blur deadline so TIP candidate chrome can briefly steal winit focus.
    pub(super) ime_os_blur_at: Option<web_time::Instant>,
    /// Last layout `text_nodes` (IME painted caret bounds).
    pub(super) text_nodes: Vec<crate::core::tree::TextLayoutInfo>,
    /// Last frame painted an Input/Textarea caret — drives blink wake in [`super::window_events`].
    pub(super) has_input_caret: bool,
    /// Swapchain uses compositor alpha (transparent / blur).
    compositor_alpha: bool,
    /// True when presenting a premultiplied composition swapchain (not shared GL).
    composition_swapchain: bool,
    /// Creation chrome; re-applied after surface configure (DWM punch-through).
    chrome: super::window_chrome::WindowOptions,
    pending_window_chrome: bool,
    /// Drops first (declared last) so secondary effects dispose before GPU teardown.
    owner: Option<OwnerHandle>,
}

impl SecondaryWindowState {
    pub(super) fn scroll_animation_active(&self) -> bool {
        !self.scroll_motion.is_empty()
    }

    pub(super) fn bust_text_caches(&mut self) {
        self.image_textures.invalidate_text_textures();
        self.text_renderer
            .bust_caches(&self.device, &self.queue, &self.config);
        self.overlay_text_renderer
            .bust_caches(&self.device, &self.queue, &self.config);
        for renderer in &mut self.overlay_band_text_renderers {
            renderer.bust_caches(&self.device, &self.queue, &self.config);
        }
        self.layout_tree
            .invalidate_resource_caches(&mut self.root_element);
        self.layout_status = self.layout_status.merge(LayoutStatus::LayoutChanged);
    }

    /// Best-effort memory trim before dropping the secondary window state.
    ///
    /// This helps RSS fall back faster after repeatedly opening/closing windows:
    /// clear large CPU-side caches and vectors before GPU/owner teardown.
    pub(super) fn trim_before_drop(&mut self) {
        self.root_element.children.clear();
        self.template_paths.clear();
        self.pending_layout_dirty.clear();
        self.last_render_scroll_metrics.clear();
        self.scroll_metrics_cache.clear();
        self.scroll_motion.clear();
        self.text_nodes.clear();

        self.text_renderer.shaped_cache.clear();
        self.text_renderer.emoji_cache = Default::default();
        self.text_renderer.pending_warmup.clear();

        self.overlay_text_renderer.shaped_cache.clear();
        self.overlay_text_renderer.emoji_cache = Default::default();
        self.overlay_text_renderer.pending_warmup.clear();

        for r in &mut self.overlay_band_text_renderers {
            r.shaped_cache.clear();
            r.emoji_cache = Default::default();
            r.pending_warmup.clear();
        }
        self.overlay_band_text_renderers.clear();
        self.image_textures.as_map_mut().clear();
    }

    pub(super) fn new(
        window: Arc<dyn Window>,
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
        instance: &wgpu::Instance,
        adapter: &wgpu::Adapter,
        primary_config: &wgpu::SurfaceConfiguration,
        mut root_element: Element,
        chrome: super::window_chrome::WindowOptions,
    ) -> Result<Self, String> {
        #[cfg(debug_assertions)]
        super::hot_reload::replay_committed(&mut root_element);
        let compositor_alpha = chrome.compositor_alpha();
        let size = window.surface_size();
        let surface = instance
            .create_surface(window.clone())
            .map_err(|e| format!("secondary window: create_surface failed: {e}"))?;

        let surface_caps = surface.get_capabilities(adapter);
        let alpha_mode =
            crate::renderer::pick_composite_alpha(&surface_caps.alpha_modes, compositor_alpha);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: primary_config.format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: primary_config.present_mode,
            alpha_mode,
            color_space: wgpu::SurfaceColorSpace::Auto,
            view_formats: vec![],
            desired_maximum_frame_latency: 1,
        };
        surface.configure(&device, &config);
        super::window_chrome::apply_created_window(window.as_ref(), &chrome, false);

        let mut pipeline = JadixPipeline::new(&device, &config);
        let present_retain =
            crate::renderer::PresentRetainPipeline::new_for_surface(&device, &config);
        let theme_transition =
            crate::renderer::theme_transition::ThemeTransition::new_for_surface(&device, &config);
        let (textured_pipeline, textured_instance_buffer) =
            init_secondary_textured(&device, &config);
        let mut text_renderer = if let Some(shared) = crate::text::shared_font_system_ptr() {
            JadixTextRenderer::new_sharing_font_system(&device, &queue, &config, shared)
        } else {
            // Headless / tests without primary shared FS: own a FontSystem.
            JadixTextRenderer::new(&device, &queue, &config)
        };
        let overlay_font = text_renderer.font_system_mut() as *mut glyphon::FontSystem;
        let overlay_text_renderer =
            JadixTextRenderer::new_sharing_font_system(&device, &queue, &config, overlay_font);

        let _ = super::state::hoist_portal_subtrees_refreshing_scroll_flags(&mut root_element);

        let mut layout_tree = LayoutTree::new();
        let scale = window.scale_factor() as f32;
        let logical_w = config.width as f32 / scale.max(1.0);
        let logical_h = config.height as f32 / scale.max(1.0);
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

        let instance_buffer = crate::renderer::buffer_upload::RetainedBuffer::new(
            &device,
            "Secondary Instance Buffer",
            wgpu::BufferUsages::VERTEX,
            bytemuck::cast_slice(&scaled_instances),
        );

        Ok(Self {
            window,
            _glass_instance: None,
            _glass_adapter: None,
            device,
            queue,
            surface,
            config,
            size,
            pipeline,
            present_retain,
            theme_transition,
            textured_pipeline,
            image_textures: TexturePool::new(),
            textured_instance_buffer,
            instance_buffer,
            num_instances,
            layout_tree,
            text_renderer,
            overlay_text_renderer,
            overlay_band_text_renderers: Vec::new(),
            root_element,
            layout_status: LayoutStatus::StructureChanged,
            template_paths: HashMap::new(),
            pending_layout_dirty: Vec::new(),
            last_hover_cursor: None,
            last_render_scroll_metrics: Vec::new(),
            scroll_metrics_cache: HashMap::new(),
            scrollbar_base_style: HashMap::new(),
            scrollbar_activity: HashMap::new(),
            scroll_motion: HashMap::new(),
            last_frame_at: web_time::Instant::now(),
            damage_rect: None,
            ime_allowed_latched: false,
            os_focused: false,
            ime_os_blur_at: None,
            text_nodes: Vec::new(),
            has_input_caret: false,
            compositor_alpha,
            composition_swapchain: false,
            chrome,
            pending_window_chrome: false,
            owner: None,
        })
    }

    pub(super) fn from_compositor(
        window: Arc<dyn Window>,
        gpu: super::glass_gpu::CompositorGpu,
        mut root_element: Element,
        chrome: super::window_chrome::WindowOptions,
        composition_swapchain: bool,
    ) -> Self {
        #[cfg(debug_assertions)]
        super::hot_reload::replay_committed(&mut root_element);
        let compositor_alpha = true;
        let super::glass_gpu::CompositorGpu {
            instance,
            adapter,
            device,
            queue,
            surface,
            config,
        } = gpu;
        super::window_chrome::apply_created_window(window.as_ref(), &chrome, composition_swapchain);

        let mut pipeline = JadixPipeline::new(&device, &config);
        let present_retain =
            crate::renderer::PresentRetainPipeline::new_for_surface(&device, &config);
        let theme_transition =
            crate::renderer::theme_transition::ThemeTransition::new_for_surface(&device, &config);
        let (textured_pipeline, textured_instance_buffer) =
            init_secondary_textured(&device, &config);
        let mut text_renderer = if let Some(shared) = crate::text::shared_font_system_ptr() {
            JadixTextRenderer::new_sharing_font_system(&device, &queue, &config, shared)
        } else {
            JadixTextRenderer::new(&device, &queue, &config)
        };
        let overlay_font = text_renderer.font_system_mut() as *mut glyphon::FontSystem;
        let overlay_text_renderer =
            JadixTextRenderer::new_sharing_font_system(&device, &queue, &config, overlay_font);

        let _ = super::state::hoist_portal_subtrees_refreshing_scroll_flags(&mut root_element);

        let mut layout_tree = LayoutTree::new();
        let scale = window.scale_factor() as f32;
        let logical_w = config.width as f32 / scale.max(1.0);
        let logical_h = config.height as f32 / scale.max(1.0);
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
        let size = PhysicalSize::new(config.width, config.height);

        let instance_buffer = crate::renderer::buffer_upload::RetainedBuffer::new(
            &device,
            "Secondary Instance Buffer",
            wgpu::BufferUsages::VERTEX,
            bytemuck::cast_slice(&scaled_instances),
        );

        Self {
            window,
            _glass_instance: Some(instance),
            _glass_adapter: Some(adapter),
            device,
            queue,
            surface,
            config,
            size,
            pipeline,
            present_retain,
            theme_transition,
            textured_pipeline,
            image_textures: TexturePool::new(),
            textured_instance_buffer,
            instance_buffer,
            num_instances,
            layout_tree,
            text_renderer,
            overlay_text_renderer,
            overlay_band_text_renderers: Vec::new(),
            root_element,
            layout_status: LayoutStatus::StructureChanged,
            template_paths: HashMap::new(),
            pending_layout_dirty: Vec::new(),
            last_hover_cursor: None,
            last_render_scroll_metrics: Vec::new(),
            scroll_metrics_cache: HashMap::new(),
            scrollbar_base_style: HashMap::new(),
            scrollbar_activity: HashMap::new(),
            scroll_motion: HashMap::new(),
            last_frame_at: web_time::Instant::now(),
            damage_rect: None,
            ime_allowed_latched: false,
            os_focused: false,
            ime_os_blur_at: None,
            text_nodes: Vec::new(),
            has_input_caret: false,
            compositor_alpha,
            composition_swapchain,
            chrome,
            pending_window_chrome: false,
            owner: None,
        }
    }

    pub(super) fn resize(&mut self, new_size: PhysicalSize<u32>) {
        if new_size.width == 0 || new_size.height == 0 {
            return;
        }
        let max_dim = self.device.limits().max_texture_dimension_2d.max(1);
        let width = new_size.width.min(max_dim).max(1);
        let height = new_size.height.min(max_dim).max(1);
        if self.config.width == width && self.config.height == height {
            return;
        }
        self.size = PhysicalSize::new(width, height);
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
        if !self.pending_window_chrome {
            super::window_chrome::apply_created_window(
                self.window.as_ref(),
                &self.chrome,
                self.composition_swapchain,
            );
        }
        self.pipeline
            .update_resolution(&self.queue, self.config.width, self.config.height);
        self.textured_pipeline.update_resolution(
            &self.queue,
            self.config.width,
            self.config.height,
        );
        self.layout_status = LayoutStatus::LayoutChanged;
    }

    pub(super) fn recover_surface(&mut self) {
        self.surface.configure(&self.device, &self.config);
        if !self.pending_window_chrome {
            super::window_chrome::apply_created_window(
                self.window.as_ref(),
                &self.chrome,
                self.composition_swapchain,
            );
        }
    }

    pub(super) fn sync_window_theme(&mut self) {
        self.window
            .set_theme(super::window_chrome::native_theme_preference());
        self.scrollbar_base_style.clear();
        self.pending_window_chrome = true;
        if self.chrome.themed().blur {
            super::window_chrome::apply_created_window(
                self.window.as_ref(),
                &self.chrome,
                self.composition_swapchain,
            );
            crate::platform::glass_sync_jadix_theme(self.window.as_ref());
            self.pending_window_chrome = false;
        }
        // Theme changes may alter text metrics/raster mode without changing the
        // string list. Rebuild glyph caches before the first themed frame.
        // Secondary windows do not retain a page cache, so invalidate the
        // renderer/layout resources directly.
        self.bust_text_caches();
        self.window.request_redraw();
    }

    /// Layout + clear + draw primitives/text for this secondary window.
    ///
    /// `mouse_pos` is logical client coords when the pointer is inside this window,
    /// otherwise `[-1, -1]` so CSS `:hover` (ghost caption `hover:bg-accent`) leaves.
    pub(super) fn render(
        &mut self,
        mouse_pos: [f32; 2],
    ) -> Result<(), crate::renderer::wgpu_compat::SurfaceAcquireError> {
        #[cfg(feature = "performance")]
        let render_begin = web_time::Instant::now();
        self.layout_tree.begin_presentation_frame();
        super::frame_stats::begin_scene_uploads();
        super::frame_stats::begin_scene_batches();
        let inner = self.window.surface_size();
        if inner.width > 0
            && inner.height > 0
            && (inner.width != self.config.width || inner.height != self.config.height)
        {
            self.resize(inner);
        }

        let scale = self.window.scale_factor() as f32;
        let logical_w = self.config.width as f32 / scale.max(1.0);
        let logical_h = self.config.height as f32 / scale.max(1.0);

        let now = web_time::Instant::now();
        let frame_dt = now
            .duration_since(self.last_frame_at)
            .as_secs_f32()
            .clamp(1.0 / 240.0, 0.05);
        crate::style::utility::set_frame_dt(frame_dt);
        if advance_secondary_scroll(
            &self.last_render_scroll_metrics,
            &mut self.scroll_motion,
            frame_dt,
            scale,
        ) {
            self.window.request_redraw();
        }
        sync_secondary_scrollbars(
            &self.last_render_scroll_metrics,
            mouse_pos,
            &mut self.scrollbar_activity,
        );
        if self
            .scrollbar_activity
            .values()
            .any(|(_, deadlines)| deadlines.iter().any(|until| *until > now))
        {
            self.window.request_redraw();
        }
        crate::style::tick_animations(frame_dt);
        self.last_frame_at = now;
        crate::utils::icon_provider::sync_icon_hover(&mut self.root_element);
        if crate::authoring::icon::tick_tree(&mut self.root_element, frame_dt) {
            self.window.request_redraw();
        }

        crate::utils::drag::runtime::sync_tree(&mut self.root_element);
        let mut sync_impact = LayoutStatus::Clean;
        let mut sync_collect = crate::core::tree::ReactiveSyncCollect::default();
        crate::core::tree::sync_reactive_elements_subtree_ex(
            &mut self.root_element,
            &mut self.layout_tree,
            &mut sync_impact,
            logical_w,
            logical_h,
            [0.0, 0.0],
            false,
            &mut sync_collect,
        );
        self.pending_layout_dirty
            .append(&mut sync_collect.layout_dirty);
        // Same mouse as CSS `:hover` below (`[-1,-1]` when the pointer is off this window).
        crate::app::tree_hit::apply_pointer_hover_reconcile(
            &self.root_element,
            mouse_pos[0],
            mouse_pos[1],
            mouse_pos[0] < 0.0 && mouse_pos[1] < 0.0,
        );

        sync_impact = sync_impact.merge(crate::authoring::presentation::sync_tree(
            &mut self.root_element,
            &mut self.layout_tree,
            crate::authoring::presentation::Context {
                phase: crate::authoring::presentation::Phase::Hover,
                viewport: [logical_w, logical_h],
                width_budget: None,
            },
        ));
        let _ = super::state::hoist_portal_subtrees_refreshing_scroll_flags(&mut self.root_element);

        sync_impact = sync_impact.merge(crate::authoring::presentation::sync_tree(
            &mut self.root_element,
            &mut self.layout_tree,
            crate::authoring::presentation::Context {
                phase: crate::authoring::presentation::Phase::State,
                viewport: [logical_w, logical_h],
                width_budget: None,
            },
        ));

        // Same chrome pass as primary [`AppState::render`]: Input/Textarea ring + caret are
        // peek-synced here (build uses `peek`, so focus does not rebuild the tree).
        crate::style::sync_focus_ring_subtree(
            &mut self.root_element,
            &mut self.layout_tree,
            &mut sync_impact,
        );
        {
            let focused =
                crate::core::reactivity::use_signal("__jadix_button_focus_id", String::new).peek();
            let dark = crate::style::is_dark_theme();
            crate::style::sync_class_variants_subtree(
                &mut self.root_element,
                mouse_pos,
                focused.as_str(),
                dark,
                logical_w,
                logical_h,
                &mut self.layout_tree,
                &mut sync_impact,
            );
        }
        crate::utils::text_edit::sync_text_edit_paint_subtree_in_viewport(
            &mut self.root_element,
            &mut self.layout_tree,
            &mut sync_impact,
            logical_w,
            logical_h,
            [0.0, 0.0],
        );
        crate::utils::text_selection::sync_label_selection_paint_subtree(
            &mut self.root_element,
            &mut sync_impact,
        );

        super::state::update_scrollbar_elements(
            &mut self.root_element,
            &mut self.scrollbar_base_style,
            &mut self.layout_tree,
            super::state::ScrollbarUpdateMode::Full,
            None,
        );

        sync_impact = sync_impact.merge(crate::authoring::presentation::sync_tree(
            &mut self.root_element,
            &mut self.layout_tree,
            crate::authoring::presentation::Context::new(
                crate::authoring::presentation::Phase::BeforeLayout,
                [logical_w, logical_h],
            ),
        ));
        let use_full =
            self.layout_tree.root_node.is_none() || self.root_element.layout_node.is_none();
        let impact = self.layout_status.merge(sync_impact);
        let geometry_changed = matches!(
            impact,
            LayoutStatus::StructureChanged | LayoutStatus::LayoutChanged
        );
        let geometry_dirty = self
            .layout_tree
            .root_node
            .is_some_and(|node| self.layout_tree.taffy.dirty(node).unwrap_or(false));
        let paint_only = impact == LayoutStatus::Clean
            && self.pending_layout_dirty.is_empty()
            && !geometry_dirty;

        if !use_full {
            let dirty: std::collections::HashSet<_> = self.pending_layout_dirty.drain(..).collect();
            if !dirty.is_empty() {
                self.layout_tree
                    .mark_dirty_nodes_in_tree(&self.root_element, &dirty);
            } else if geometry_changed {
                // Resize or non-binding synchronizers may have no targeted ids.
                // Keep the resident tree, but refresh all affected layout inputs.
                self.layout_tree
                    .mark_subtree_layout_dirty(&self.root_element);
            }
        } else {
            self.pending_layout_dirty.clear();
        }

        let geometry_motion = self.layout_tree.take_geometry_motion();
        let text_settle = self.layout_tree.take_text_settle();
        crate::text::set_layout_text_fast_mode(false);
        crate::authoring::input::set_geometry_changing(geometry_motion);
        if geometry_motion || text_settle {
            self.layout_tree
                .mark_all_text_leaves_dirty(&self.root_element);
            self.window.request_redraw();
        }
        self.layout_tree
            .set_collect_skip_allowed(!use_full && !geometry_changed && !text_settle);

        #[cfg(feature = "performance")]
        let layout_begin = web_time::Instant::now();
        let mut render_result = if use_full {
            self.layout_tree.build_and_layout(
                &mut self.root_element,
                logical_w,
                logical_h,
                mouse_pos,
            )
        } else {
            self.layout_tree.build_and_layout_fast(
                &mut self.root_element,
                logical_w,
                logical_h,
                mouse_pos,
                paint_only,
                true,
            )
        };

        let overflow_impact = crate::authoring::presentation::sync_tree(
            &mut self.root_element,
            &mut self.layout_tree,
            crate::authoring::presentation::Context::new(
                crate::authoring::presentation::Phase::AfterGeometry,
                [logical_w, logical_h],
            ),
        );
        if matches!(
            overflow_impact,
            LayoutStatus::StructureChanged | LayoutStatus::LayoutChanged
        ) {
            render_result = self.layout_tree.build_and_layout_fast(
                &mut self.root_element,
                logical_w,
                logical_h,
                mouse_pos,
                false,
                false,
            );
        }

        if crate::utils::collapse_panel::seed_measured_collapse_panels(
            &mut self.root_element,
            &mut self.layout_tree,
        ) {
            render_result = self.layout_tree.build_and_layout_fast(
                &mut self.root_element,
                logical_w,
                logical_h,
                mouse_pos,
                false,
                false,
            );
        }
        let pointer_impact = crate::app::tree_hit::sync_pointer_component_chrome_after_layout(
            &mut self.root_element,
            &mut self.layout_tree,
            mouse_pos,
            logical_w,
            logical_h,
        );
        if pointer_impact != LayoutStatus::Clean {
            render_result =
                self.layout_tree
                    .collect_animation_frame(&mut self.root_element, mouse_pos, false);
        }
        let floating_needs_frame =
            crate::utils::floating_motion::flush_floating_motion_subtree(&mut self.root_element);
        let floating_css_ticking =
            crate::utils::floating_motion::tick_hoisted_floating_css(&mut self.root_element);
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
                false,
                false,
            );
        } else if floating_css_ticking {
            render_result =
                self.layout_tree
                    .collect_animation_frame(&mut self.root_element, mouse_pos, false);
        }
        if floating_needs_frame || floating_css_ticking {
            self.window.request_redraw();
        }
        #[cfg(feature = "performance")]
        let layout_ms = layout_begin.elapsed().as_secs_f64() * 1000.0;
        if sync_secondary_scroll_metrics(&self.root_element, &render_result.scroll_metrics) {
            crate::authoring::input::note_scroll();
            self.window.request_redraw();
        }
        if crate::style::any_ui_animation_active() {
            self.window.request_redraw();
        }
        let painted_caret = self.layout_tree.take_caret_blink_request();
        self.has_input_caret = render_result
            .text_nodes
            .iter()
            .any(|n| n.style.input_caret.is_some())
            || painted_caret;

        let scaled_instances = scale_instances(&render_result.instances, scale);
        self.num_instances = render_result.instances.len() as u32;
        let overlay_inst_start = render_result
            .overlay_split
            .instance_start
            .min(scaled_instances.len());
        let scaled_input_chrome = scale_instances(&render_result.input_chrome_instances, scale);
        let overlay_chrome_start = render_result
            .overlay_split
            .chrome_start
            .min(scaled_input_chrome.len());
        let scaled_tex = scale_textured_draws(&render_result.textured_draws, scale);
        let overlay_tex_start = render_result
            .overlay_split
            .textured_start
            .min(scaled_tex.len());
        crate::utils::drag::routing::queue_frame(&self.root_element, mouse_pos[0], mouse_pos[1]);
        let textured_insts: Vec<_> = scaled_tex.iter().map(|d| d.to_instance()).collect();
        self.textured_instance_buffer.upload(
            &self.device,
            &self.queue,
            bytemuck::cast_slice(&textured_insts),
        );
        let mut clip_polys = render_result.clip_polys;
        scale_clip_polys_in_place(&mut clip_polys, scale);
        while clip_polys.len() < scaled_instances.len() {
            clip_polys.push(crate::style::GpuClipPoly::NONE);
        }

        self.instance_buffer.upload(
            &self.device,
            &self.queue,
            bytemuck::cast_slice(&scaled_instances),
        );
        self.pipeline
            .ensure_clip_polys(&self.device, &self.queue, &clip_polys);
        self.layout_status = LayoutStatus::Clean;
        self.last_render_scroll_metrics = render_result.scroll_metrics.clone();
        super::scroll_support::merge_scroll_metrics_cache(
            &mut self.scroll_metrics_cache,
            render_result.scroll_metrics.iter().cloned(),
        );

        self.pipeline
            .update_resolution(&self.queue, self.config.width, self.config.height);
        self.textured_pipeline.update_resolution(
            &self.queue,
            self.config.width,
            self.config.height,
        );
        self.upload_pending_textures(&scaled_tex);

        #[cfg(feature = "performance")]
        let text_begin = web_time::Instant::now();
        let empty_keys = std::collections::HashSet::new();
        let empty_rects = std::collections::HashMap::new();
        let overlay_text_start = render_result
            .overlay_split
            .text_start
            .min(render_result.text_nodes.len());
        let page_text = &render_result.text_nodes[..overlay_text_start];
        let prep = prepare_text_frame(
            &mut self.text_renderer,
            &self.device,
            &self.queue,
            &self.config,
            page_text,
            scale,
            false,
            &empty_keys,
            &empty_rects,
        );

        // Portal overlay glyphs (Dialog / Select menu) — same band model as AppState.
        let overlay_band_ranges = render_result.overlay_split.band_ranges(
            scaled_instances.len(),
            scaled_tex.len(),
            render_result.text_nodes.len(),
            scaled_input_chrome.len(),
            0,
        );
        self.ensure_overlay_band_text_renderers(overlay_band_ranges.len());
        let mut overlay_band_underlines: Vec<Vec<_>> =
            Vec::with_capacity(overlay_band_ranges.len());
        let mut overlay_needs_warmup = false;
        for (band_i, band) in overlay_band_ranges.iter().enumerate() {
            let text_slice = &render_result.text_nodes[band.text.0..band.text.1];
            let overlay_prep = if band_i == 0 {
                prepare_text_frame(
                    &mut self.overlay_text_renderer,
                    &self.device,
                    &self.queue,
                    &self.config,
                    text_slice,
                    scale,
                    false,
                    &empty_keys,
                    &empty_rects,
                )
            } else {
                prepare_text_frame(
                    &mut self.overlay_band_text_renderers[band_i - 1],
                    &self.device,
                    &self.queue,
                    &self.config,
                    text_slice,
                    scale,
                    false,
                    &empty_keys,
                    &empty_rects,
                )
            };
            if overlay_prep.needs_warmup {
                overlay_needs_warmup = true;
            }
            overlay_band_underlines.push(overlay_prep.underline_instances);
        }

        #[cfg(feature = "performance")]
        let text_done = web_time::Instant::now();
        self.text_renderer.trim_glyph_atlas();
        self.overlay_text_renderer.trim_glyph_atlas();
        for tr in &mut self.overlay_band_text_renderers {
            tr.trim_glyph_atlas();
        }

        let output = crate::renderer::wgpu_compat::acquire_surface(&self.surface)?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Secondary Render Encoder"),
            });

        let draw_to_retain = self.present_retain.needs_compositor_encoding()
            || crate::style::theme_transition_config().enabled
            || self.theme_transition.needs_frame();
        if draw_to_retain {
            self.present_retain
                .ensure_size(&self.device, self.config.width, self.config.height);
            self.theme_transition
                .prepare(&self.device, &mut encoder, &self.present_retain);
        } else {
            self.present_retain.invalidate_contents();
        }
        {
            let color_view = if draw_to_retain {
                self.present_retain.color_view().unwrap()
            } else {
                &view
            };
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Secondary Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: color_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(crate::renderer::surface_clear_color(
                            self.compositor_alpha
                                && (self.chrome.transparent || self.chrome.themed().blur),
                        )),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            render_pass.set_pipeline(&self.pipeline.render_pipeline);
            render_pass.set_bind_group(0, &self.pipeline.bind_group_clip_poly, &[]);
            render_pass.set_bind_group(1, self.pipeline.dummy_backdrop(), &[]);
            render_pass.set_vertex_buffer(0, self.pipeline.vertex_buffer.slice(..));
            render_pass.set_vertex_buffer(1, self.instance_buffer.buffer.slice(..));
            render_pass.set_index_buffer(
                self.pipeline.index_buffer.slice(..),
                wgpu::IndexFormat::Uint16,
            );
            // Page chrome first.
            if overlay_inst_start > 0 {
                render_pass.draw_indexed(
                    0..self.pipeline.num_indices,
                    0,
                    0..overlay_inst_start as u32,
                );
            }

            // Named Lucide caption icons (min/max/close) collect as textured draws.
            if overlay_tex_start > 0 {
                super::state::draw_packed_textured_runs(
                    &mut render_pass,
                    &self.textured_pipeline,
                    &self.textured_instance_buffer.buffer,
                    &self.image_textures,
                    &scaled_tex,
                    0,
                    overlay_tex_start,
                    None,
                );
            }

            if !prep.underline_instances.is_empty() {
                render_pass.set_pipeline(&self.pipeline.render_pipeline);
                render_pass.set_bind_group(0, &self.pipeline.bind_group, &[]);
                render_pass.set_bind_group(1, self.pipeline.dummy_backdrop(), &[]);
                render_pass.set_vertex_buffer(0, self.pipeline.vertex_buffer.slice(..));
                let underline_buffer =
                    self.device
                        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("Secondary Underline Buffer"),
                            contents: bytemuck::cast_slice(&prep.underline_instances),
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
                    0..prep.underline_instances.len() as u32,
                );
            }

            self.text_renderer.render(&mut render_pass);

            // Page I-beam above page glyphs (swapchain overlay; not baked under text).
            if overlay_chrome_start > 0 {
                render_pass.set_pipeline(&self.pipeline.render_pipeline);
                render_pass.set_bind_group(0, &self.pipeline.bind_group, &[]);
                render_pass.set_bind_group(1, self.pipeline.dummy_backdrop(), &[]);
                render_pass.set_vertex_buffer(0, self.pipeline.vertex_buffer.slice(..));
                let chrome_slice = &scaled_input_chrome[..overlay_chrome_start];
                let chrome_buffer =
                    self.device
                        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("Secondary Input Chrome Buffer"),
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

            if !prep.caret_instances.is_empty() {
                render_pass.set_pipeline(&self.pipeline.render_pipeline);
                render_pass.set_bind_group(0, &self.pipeline.bind_group, &[]);
                render_pass.set_bind_group(1, self.pipeline.dummy_backdrop(), &[]);
                render_pass.set_vertex_buffer(0, self.pipeline.vertex_buffer.slice(..));
                let caret_buffer =
                    self.device
                        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("Secondary Caret Buffer"),
                            contents: bytemuck::cast_slice(&prep.caret_instances),
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
                    0..prep.caret_instances.len() as u32,
                );
            }

            // Portal overlay: per-band `rects → text` so panels occlude lower glyphs.
            for (band_i, band) in overlay_band_ranges.iter().enumerate() {
                if band.instance.0 < band.instance.1 {
                    render_pass.set_pipeline(&self.pipeline.render_pipeline);
                    render_pass.set_bind_group(0, &self.pipeline.bind_group_clip_poly, &[]);
                    render_pass.set_bind_group(1, self.pipeline.dummy_backdrop(), &[]);
                    render_pass.set_vertex_buffer(0, self.pipeline.vertex_buffer.slice(..));
                    render_pass.set_vertex_buffer(1, self.instance_buffer.buffer.slice(..));
                    render_pass.set_index_buffer(
                        self.pipeline.index_buffer.slice(..),
                        wgpu::IndexFormat::Uint16,
                    );
                    render_pass.draw_indexed(
                        0..self.pipeline.num_indices,
                        0,
                        band.instance.0 as u32..band.instance.1 as u32,
                    );
                }

                if band.textured.0 < band.textured.1 {
                    super::state::draw_packed_textured_runs(
                        &mut render_pass,
                        &self.textured_pipeline,
                        &self.textured_instance_buffer.buffer,
                        &self.image_textures,
                        &scaled_tex,
                        band.textured.0,
                        band.textured.1,
                        None,
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
                                    label: Some("Secondary Overlay Underline Buffer"),
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

                if band.chrome.0 < band.chrome.1 {
                    render_pass.set_pipeline(&self.pipeline.render_pipeline);
                    render_pass.set_bind_group(0, &self.pipeline.bind_group, &[]);
                    render_pass.set_bind_group(1, self.pipeline.dummy_backdrop(), &[]);
                    render_pass.set_vertex_buffer(0, self.pipeline.vertex_buffer.slice(..));
                    let chrome_slice = &scaled_input_chrome[band.chrome.0..band.chrome.1];
                    let chrome_buffer =
                        self.device
                            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                                label: Some("Secondary Overlay Input Chrome Buffer"),
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

        if draw_to_retain {
            if self.theme_transition.present(
                &self.queue,
                &mut encoder,
                &self.present_retain,
                &view,
                crate::renderer::surface_clear_color(
                    self.compositor_alpha && (self.chrome.transparent || self.chrome.themed().blur),
                ),
                |_| {},
            ) {
                self.window.request_redraw();
            }
            self.present_retain.mark_contents_valid();
        }
        self.queue.submit(std::iter::once(encoder.finish()));
        self.queue.present(output);
        if self.pending_window_chrome && !self.theme_transition.needs_frame() {
            super::window_chrome::apply_created_window(
                self.window.as_ref(),
                &self.chrome,
                self.composition_swapchain,
            );
            self.pending_window_chrome = false;
            crate::platform::glass_sync_jadix_theme(self.window.as_ref());
        }
        super::frame_stats::note_presented_frame(self.window.id());
        #[cfg(feature = "performance")]
        super::frame_stats::note_frame_timings(
            self.window.id(),
            render_begin.elapsed().as_secs_f64() * 1000.0,
            layout_ms,
            (text_done - text_begin).as_secs_f64() * 1000.0,
        );
        super::frame_stats::log_scene_uploads(true);
        // This window redraws its complete surface; consume only its own damage.
        super::damage::clear_frame_paint_nodes();

        if prep.culled_skip_prepare || prep.needs_warmup || overlay_needs_warmup {
            self.window.request_redraw();
        }
        if scaled_tex
            .iter()
            .any(|d| self.image_textures.get(&d.texture_key).is_none())
        {
            self.window.request_redraw();
        }
        if crate::text::take_fit_needs_relayout() {
            self.window.request_redraw();
        }
        self.text_nodes = render_result.text_nodes;
        Ok(())
    }

    fn ensure_overlay_band_text_renderers(&mut self, band_count: usize) {
        let extra_needed = band_count.saturating_sub(1);
        while self.overlay_band_text_renderers.len() < extra_needed {
            let font_ptr = self.text_renderer.font_system_mut() as *mut glyphon::FontSystem;
            self.overlay_band_text_renderers
                .push(JadixTextRenderer::new_sharing_font_system(
                    &self.device,
                    &self.queue,
                    &self.config,
                    font_ptr,
                ));
        }
    }

    /// Rehydrate CPU caches into this window's device, then upload `take_pending`.
    ///
    /// Glass sidecars cannot share primary `GpuImageTexture`s; each collect marks
    /// its Lucide icons needed so missing slots are uploaded to this window's pool.
    fn upload_pending_textures(&mut self, scaled_tex: &[ScaledTexturedDraw]) {
        self.image_textures.begin_frame();
        for draw in scaled_tex {
            self.image_textures.touch(&draw.texture_key);
        }
        let _ = crate::style::flush_gradient_textures(|key| self.image_textures.get(key).is_some());
        let _ = crate::utils::icon_texture::flush_icon_textures(|key| {
            self.image_textures.get(key).is_some()
        });
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
        crate::media::texture::flush_retained_textures(
            scaled_tex.iter().map(|draw| draw.texture_key.as_str()),
            |key| self.image_textures.get(key).is_some(),
        );
        let _ = crate::utils::text_texture::flush_text_xform_textures(|key| {
            self.image_textures.get(key).is_some()
        });

        for (id, rgba, w, h) in super::pending_images::take_pending() {
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
    }

    pub(super) fn attach_owner(&mut self, owner: OwnerHandle) {
        self.owner = Some(owner);
    }

    /// Drop the reactive owner before GPU teardown (avoids effects re-entering `secondaries`).
    pub(super) fn take_owner(&mut self) -> Option<OwnerHandle> {
        self.owner.take()
    }

    pub(super) fn hide_window(&self) {
        self.window.set_visible(false);
    }

    #[cfg(all(debug_assertions, feature = "hot-reload"))]
    pub(super) fn invalidate_hot_reload_image(&mut self, key: &str) {
        self.image_textures.invalidate(key);
    }

    #[cfg(all(debug_assertions, feature = "hot-reload"))]
    pub(super) fn invalidate_hot_reload(
        &mut self,
        templates: &[super::hot_reload::TemplateFragment],
    ) {
        super::state::hot_reload_inject::sync_template_layout(
            &self.root_element,
            templates,
            &mut self.layout_tree,
        );
        self.layout_status = LayoutStatus::StructureChanged;
        self.template_paths.clear();
        self.window.request_redraw();
    }

    pub(super) fn replace_root(&mut self, mut root: Element) {
        #[cfg(debug_assertions)]
        super::hot_reload::replay_committed(&mut root);
        let status = super::state::AppState::reconcile_elements(
            &mut root,
            &mut self.root_element,
            &mut self.pending_layout_dirty,
        );
        self.root_element = root;
        super::state::hoist_portal_subtrees_refreshing_scroll_flags(&mut self.root_element);
        self.layout_status = self.layout_status.merge(status);
        self.template_paths.clear();
    }

    pub(super) fn reconcile_subtree(
        &mut self,
        anchor_id: &str,
        mut new_fragment: Element,
    ) -> Option<LayoutStatus> {
        #[cfg(debug_assertions)]
        super::hot_reload::replay_committed(&mut new_fragment);
        let want = anchor_id.replace('\\', "/").to_ascii_lowercase();
        let old = find_template_mut(&mut self.root_element, &want)?;
        let needs_portal_hoist = element_contains_jadix_portal(&new_fragment);
        let mut status = super::state::AppState::reconcile_elements(
            &mut new_fragment,
            old,
            &mut self.pending_layout_dirty,
        );
        *old = new_fragment;
        if needs_portal_hoist {
            let portal_moves = crate::app::state::hoist_portal_subtrees_refreshing_scroll_flags(
                &mut self.root_element,
            );
            if portal_moves > 0 {
                status = status.merge(LayoutStatus::StructureChanged);
            }
        }
        self.layout_status = self.layout_status.merge(status);
        Some(status)
    }

    pub(super) fn update_element_by_template_id<F>(
        &mut self,
        template_id: &str,
        patch: F,
    ) -> LayoutStatus
    where
        F: FnOnce(&mut Element) -> LayoutStatus,
    {
        let want = template_id.replace('\\', "/").to_ascii_lowercase();
        let Some(status) = find_template_mut(&mut self.root_element, &want).map(patch) else {
            crate::app::reactive::note_bind_miss(template_id);
            return LayoutStatus::Clean;
        };
        self.layout_status = self.layout_status.merge(status);
        if status != LayoutStatus::Clean {
            crate::layout::collect_cache::invalidate_collect_paint_hash_for_template(
                &mut self.root_element,
                &want,
            );
        }
        status
    }
}

fn advance_secondary_scroll(
    metrics: &[(String, [f32; 4], [f32; 2], [f32; 2])],
    motion: &mut HashMap<String, [super::scroll_support::SmoothScrollAxis; 2]>,
    dt: f32,
    scale: f32,
) -> bool {
    use crate::core::reactivity::use_signal;
    use crate::core::scroll_viewport::{
        scroll_behavior_is_smooth, ScrollViewportSignalKeys, DEFAULT_SCROLL_BEHAVIOR,
    };
    motion.retain(|rest, _| metrics.iter().any(|(id, _, _, _)| id == rest));
    let mut active = false;
    for (rest, _, maximum, _) in metrics {
        let keys = ScrollViewportSignalKeys::new(rest);
        let smooth = scroll_behavior_is_smooth(
            &use_signal(&keys.behavior, || DEFAULT_SCROLL_BEHAVIOR.to_string()).peek(),
        );
        let offsets = [
            use_signal(&keys.offset_x, || 0.0_f32),
            use_signal(&keys.offset_y, || 0.0_f32),
        ];
        let targets = [
            use_signal(&keys.target_x, || 0.0_f32),
            use_signal(&keys.target_y, || 0.0_f32),
        ];
        let axes = motion.entry(rest.clone()).or_insert_with(|| {
            offsets.map(|offset| super::scroll_support::SmoothScrollAxis::new(offset.peek()))
        });
        let mut moving = false;
        for axis in 0..2 {
            let current = offsets[axis].peek().clamp(0.0, maximum[axis].max(0.0));
            let target = targets[axis].peek().clamp(0.0, maximum[axis].max(0.0));
            targets[axis].set_silent(target);
            let (next, chasing) = if smooth {
                axes[axis].step(current, target, dt, scale)
            } else {
                (target, false)
            };
            offsets[axis].set_silent(next.clamp(0.0, maximum[axis].max(0.0)));
            moving |= chasing;
        }
        if !moving {
            motion.remove(rest);
        }
        active |= moving;
    }
    active
}

fn sync_secondary_scroll_metrics(
    root: &Element,
    metrics: &[(String, [f32; 4], [f32; 2], [f32; 2])],
) -> bool {
    use crate::core::reactivity::use_signal;
    use crate::core::scroll_viewport::ScrollViewportSignalKeys;

    let pending = super::scroll_support::resolve_pending_descendant_bounds(root);
    let mut changed = false;
    for (rest, bounds, max_scroll, content) in metrics {
        let keys = ScrollViewportSignalKeys::new(rest);
        for (key, value) in [
            (&keys.viewport_w, bounds[2]),
            (&keys.viewport_h, bounds[3]),
            (&keys.content_w, content[0]),
            (&keys.content_h, content[1]),
        ] {
            let signal = use_signal(key, || 0.0_f32);
            if (signal.peek() - value).abs() > 0.1 {
                signal.set_silent(value.max(0.0));
                changed = true;
            }
        }
        for (key, maximum) in [
            (&keys.offset_x, max_scroll[0]),
            (&keys.offset_y, max_scroll[1]),
        ] {
            let signal = use_signal(key, || 0.0_f32);
            let value = signal.peek().clamp(0.0, maximum.max(0.0));
            if (signal.peek() - value).abs() > 0.1 {
                signal.set_silent(value);
                changed = true;
            }
        }
        changed |= super::scroll_support::apply_pending_scroll_align(
            rest,
            *bounds,
            content[0],
            content[1],
            pending.get(rest).copied(),
        );
    }
    changed
}

fn sync_secondary_scrollbars(
    metrics: &[(String, [f32; 4], [f32; 2], [f32; 2])],
    mouse_pos: [f32; 2],
    activity: &mut HashMap<String, ([f32; 2], [web_time::Instant; 2])>,
) {
    use crate::core::reactivity::use_signal;
    use crate::core::scroll_viewport::ScrollViewportSignalKeys;

    let now = web_time::Instant::now();
    for (rest, bounds, maximum, _) in metrics {
        let keys = ScrollViewportSignalKeys::new(rest);
        let inside = mouse_pos[0] >= bounds[0]
            && mouse_pos[0] < bounds[0] + bounds[2]
            && mouse_pos[1] >= bounds[1]
            && mouse_pos[1] < bounds[1] + bounds[3];
        let offsets = [
            use_signal(&keys.offset_x, || 0.0_f32).peek(),
            use_signal(&keys.offset_y, || 0.0_f32).peek(),
        ];
        let (previous, deadlines) = activity.entry(rest.clone()).or_insert((offsets, [now; 2]));
        for (axis, (visibility, spring, axis_hover, extent)) in [
            (
                &keys.scrollbar_visibility_x,
                &keys.spring_x,
                &keys.hover_axis_x,
                maximum[0],
            ),
            (
                &keys.scrollbar_visibility_y,
                &keys.spring_y,
                &keys.hover_axis_y,
                maximum[1],
            ),
        ]
        .into_iter()
        .enumerate()
        {
            if (previous[axis] - offsets[axis]).abs() > 0.1 {
                deadlines[axis] = now + std::time::Duration::from_millis(700);
            }
            let hovered = inside
                && if axis == 0 {
                    mouse_pos[1] >= bounds[1] + bounds[3] - 12.0
                } else {
                    mouse_pos[0] >= bounds[0] + bounds[2] - 12.0
                };
            let mode = use_signal(visibility, || "auto".to_string()).peek();
            let dragging = use_signal(axis_hover, || false).peek();
            let shown = extent > 0.5
                && (mode == "always"
                    || (mode != "never" && (hovered || dragging || deadlines[axis] > now)));
            crate::style::use_ui_transition_cubic(spring, 0.0).set_at_frame_start(if shown {
                1.0
            } else {
                0.0
            });
        }
        *previous = offsets;
    }
}

fn init_secondary_textured(
    device: &wgpu::Device,
    config: &wgpu::SurfaceConfiguration,
) -> (
    TexturedQuadPipeline,
    crate::renderer::buffer_upload::RetainedBuffer,
) {
    let pipeline = TexturedQuadPipeline::new(device, config);
    let buffer = crate::renderer::buffer_upload::RetainedBuffer::new(
        device,
        "Secondary Textured instance buffer",
        wgpu::BufferUsages::VERTEX,
        &[],
    );
    (pipeline, buffer)
}

fn find_template_mut<'a>(el: &'a mut Element, want_normalized: &str) -> Option<&'a mut Element> {
    crate::core::tree::find_by_template_id_mut(el, want_normalized)
}

fn element_contains_jadix_portal(el: &Element) -> bool {
    crate::core::tree::find_by_attr(el, "jadix_portal", "1").is_some()
}

#[cfg(test)]
#[path = "tests/scroll.rs"]
mod scroll_tests;
