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
use crate::renderer::{prepare_text_frame, scale_instances, RuiPipeline, RuiTextRenderer};

/// Factory for a secondary window's root [`Element`].
///
/// Invoked only on the event-loop thread after [`super::RuiUserEvent::OpenWindow`]
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
    pub(super) window: Arc<Window>,
    /// Owned when this window uses a compositor-alpha GPU (Vulkan sidecar).
    _glass_instance: Option<wgpu::Instance>,
    _glass_adapter: Option<wgpu::Adapter>,
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    size: PhysicalSize<u32>,
    pipeline: RuiPipeline,
    instance_buffer: wgpu::Buffer,
    instance_buffer_capacity: usize,
    num_instances: u32,
    pub(super) layout_tree: LayoutTree,
    pub(super) text_renderer: RuiTextRenderer,
    /// Band 0 portal overlay glyphs (Dialog / Select menu / etc.).
    overlay_text_renderer: RuiTextRenderer,
    /// Extra stacked portal bands (band_i >= 1).
    overlay_band_text_renderers: Vec<RuiTextRenderer>,
    pub(super) root_element: Element,
    pub(super) layout_status: LayoutStatus,
    pub(super) template_paths: HashMap<String, Vec<usize>>,
    pub(super) pending_layout_dirty: Vec<taffy::prelude::NodeId>,
    #[allow(dead_code)]
    pub(super) last_hover_cursor: Option<crate::core::tree::CursorValue>,
    pub(super) last_render_scroll_metrics: Vec<(String, [f32; 4], [f32; 2], [f32; 2])>,
    pub(super) scroll_metrics_cache: HashMap<String, ([f32; 4], [f32; 2], [f32; 2])>,
    #[allow(dead_code)]
    pub(super) damage_rect: Option<[f32; 4]>,
    pub(super) ime_allowed_latched: bool,
    /// Latest `WindowEvent::Focused` for this secondary (IME HWND drive / soft-blur).
    pub(super) os_focused: bool,
    /// Soft-blur deadline so TIP candidate chrome can briefly steal winit focus.
    pub(super) ime_os_blur_at: Option<std::time::Instant>,
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
    /// Drops first (declared last) so secondary effects dispose before GPU teardown.
    owner: Option<OwnerHandle>,
}

impl SecondaryWindowState {
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
    }

    pub(super) fn new(
        window: Arc<Window>,
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
        instance: &wgpu::Instance,
        adapter: &wgpu::Adapter,
        primary_config: &wgpu::SurfaceConfiguration,
        mut root_element: Element,
        chrome: super::window_chrome::WindowOptions,
    ) -> Result<Self, String> {
        let compositor_alpha = chrome.compositor_alpha();
        let size = window.inner_size();
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
        super::window_chrome::apply_created_window(&window, &chrome, false);

        let pipeline = RuiPipeline::new(&device, &config);
        let mut text_renderer = if let Some(shared) = crate::text::shared_font_system_ptr() {
            RuiTextRenderer::new_sharing_font_system(&device, &queue, &config, shared)
        } else {
            // Headless / tests without primary shared FS: own a FontSystem.
            RuiTextRenderer::new(&device, &queue, &config)
        };
        let overlay_font = text_renderer.font_system_mut() as *mut glyphon::FontSystem;
        let overlay_text_renderer =
            RuiTextRenderer::new_sharing_font_system(&device, &queue, &config, overlay_font);

        let _ = super::state::hoist_portal_subtrees_refreshing_scroll_flags(&mut root_element);

        let mut layout_tree = LayoutTree::new();
        let scale = window.scale_factor() as f32;
        let logical_w = config.width as f32 / scale.max(1.0);
        let logical_h = config.height as f32 / scale.max(1.0);
        let render_result =
            layout_tree.build_and_layout(&mut root_element, logical_w, logical_h, [0.0, 0.0]);
        let scaled_instances = scale_instances(&render_result.instances, scale);
        let num_instances = render_result.instances.len() as u32;

        let instance_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Secondary Instance Buffer"),
            contents: bytemuck::cast_slice(&scaled_instances),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });

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
            instance_buffer,
            instance_buffer_capacity: scaled_instances.len().max(1),
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
            damage_rect: None,
            ime_allowed_latched: false,
            os_focused: false,
            ime_os_blur_at: None,
            text_nodes: Vec::new(),
            has_input_caret: false,
            compositor_alpha,
            composition_swapchain: false,
            chrome,
            owner: None,
        })
    }

    pub(super) fn from_compositor(
        window: Arc<Window>,
        gpu: super::glass_gpu::CompositorGpu,
        mut root_element: Element,
        chrome: super::window_chrome::WindowOptions,
    ) -> Self {
        let compositor_alpha = true;
        let super::glass_gpu::CompositorGpu {
            instance,
            adapter,
            device,
            queue,
            surface,
            config,
        } = gpu;
        super::window_chrome::apply_created_window(&window, &chrome, true);

        let pipeline = RuiPipeline::new(&device, &config);
        let mut text_renderer = if let Some(shared) = crate::text::shared_font_system_ptr() {
            RuiTextRenderer::new_sharing_font_system(&device, &queue, &config, shared)
        } else {
            RuiTextRenderer::new(&device, &queue, &config)
        };
        let overlay_font = text_renderer.font_system_mut() as *mut glyphon::FontSystem;
        let overlay_text_renderer =
            RuiTextRenderer::new_sharing_font_system(&device, &queue, &config, overlay_font);

        let _ = super::state::hoist_portal_subtrees_refreshing_scroll_flags(&mut root_element);

        let mut layout_tree = LayoutTree::new();
        let scale = window.scale_factor() as f32;
        let logical_w = config.width as f32 / scale.max(1.0);
        let logical_h = config.height as f32 / scale.max(1.0);
        let render_result =
            layout_tree.build_and_layout(&mut root_element, logical_w, logical_h, [0.0, 0.0]);
        let scaled_instances = scale_instances(&render_result.instances, scale);
        let num_instances = render_result.instances.len() as u32;
        let size = PhysicalSize::new(config.width, config.height);

        let instance_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Secondary Instance Buffer"),
            contents: bytemuck::cast_slice(&scaled_instances),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });

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
            instance_buffer,
            instance_buffer_capacity: scaled_instances.len().max(1),
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
            damage_rect: None,
            ime_allowed_latched: false,
            os_focused: false,
            ime_os_blur_at: None,
            text_nodes: Vec::new(),
            has_input_caret: false,
            compositor_alpha,
            composition_swapchain: true,
            chrome,
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
        super::window_chrome::apply_created_window(
            &self.window,
            &self.chrome,
            self.composition_swapchain,
        );
        self.pipeline
            .update_resolution(&self.queue, self.config.width, self.config.height);
        self.layout_status = LayoutStatus::LayoutChanged;
    }

    pub(super) fn recover_surface(&mut self) {
        self.surface.configure(&self.device, &self.config);
        super::window_chrome::apply_created_window(
            &self.window,
            &self.chrome,
            self.composition_swapchain,
        );
    }

    /// Layout + clear + draw primitives/text for this secondary window.
    pub(super) fn render(
        &mut self,
    ) -> Result<(), crate::renderer::wgpu_compat::SurfaceAcquireError> {
        let inner = self.window.inner_size();
        if inner.width > 0
            && inner.height > 0
            && (inner.width != self.config.width || inner.height != self.config.height)
        {
            self.resize(inner);
        }

        let scale = self.window.scale_factor() as f32;
        let logical_w = self.config.width as f32 / scale.max(1.0);
        let logical_h = self.config.height as f32 / scale.max(1.0);

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
            .extend(sync_collect.layout_dirty.drain(..));
        let portal_only_layout = matches!(sync_impact, LayoutStatus::LayoutChanged)
            && sync_collect.portal_layout_changed
            && !sync_collect.page_layout_changed;

        let _ = super::state::hoist_portal_subtrees_refreshing_scroll_flags(&mut self.root_element);

        // Same chrome pass as primary [`AppState::render`]: Input/Textarea ring + caret are
        // peek-synced here (build uses `peek`, so focus does not rebuild the tree).
        crate::style::sync_focus_ring_subtree(
            &mut self.root_element,
            &mut self.layout_tree,
            &mut sync_impact,
        );
        crate::components::internal::text_edit::sync_text_edit_paint_subtree_in_viewport(
            &mut self.root_element,
            &mut self.layout_tree,
            &mut sync_impact,
            logical_w,
            logical_h,
            [0.0, 0.0],
        );
        crate::components::label::sync_label_selection_paint_subtree(
            &mut self.root_element,
            &mut sync_impact,
        );

        let use_full = self.layout_tree.root_node.is_none()
            || matches!(
                self.layout_status,
                LayoutStatus::StructureChanged | LayoutStatus::LayoutChanged
            )
            || (sync_impact != LayoutStatus::Clean && !portal_only_layout);

        if !use_full {
            let dirty: std::collections::HashSet<_> =
                self.pending_layout_dirty.drain(..).collect();
            if !dirty.is_empty() {
                self.layout_tree
                    .mark_dirty_nodes_in_tree(&self.root_element, &dirty);
            }
        } else {
            self.pending_layout_dirty.clear();
        }

        let scroll_only = !portal_only_layout
            && matches!(
                sync_impact,
                LayoutStatus::Clean | LayoutStatus::StyleChanged
            );

        let mut render_result = if use_full {
            self.layout_tree.build_and_layout(
                &mut self.root_element,
                logical_w,
                logical_h,
                [0.0, 0.0],
            )
        } else {
            self.layout_tree.build_and_layout_fast(
                &mut self.root_element,
                logical_w,
                logical_h,
                [0.0, 0.0],
                scroll_only,
                true,
            )
        };

        // Same anchored placement pass as primary [`AppState::render`]: dropdown / popover
        // panels need trigger bounds from the first layout before left/top are written.
        if crate::utils::popup::sync_floating_placement(
            &mut self.root_element,
            logical_w,
            logical_h,
        ) {
            crate::utils::popup::visit_floating_panels(&self.root_element, |el| {
                self.layout_tree.mark_element_layout_dirty(el);
            });
            render_result = self.layout_tree.build_and_layout_fast(
                &mut self.root_element,
                logical_w,
                logical_h,
                [0.0, 0.0],
                false,
                true,
            );
        }

        self.has_input_caret = render_result
            .text_nodes
            .iter()
            .any(|n| n.style.input_caret.is_some());

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

        if scaled_instances.len() > self.instance_buffer_capacity {
            self.instance_buffer =
                self.device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Secondary Instance Buffer"),
                        contents: bytemuck::cast_slice(&scaled_instances),
                        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    });
            self.instance_buffer_capacity = scaled_instances.len().max(1);
        } else {
            self.queue.write_buffer(
                &self.instance_buffer,
                0,
                bytemuck::cast_slice(&scaled_instances),
            );
        }
        self.layout_status = LayoutStatus::Clean;
        self.last_render_scroll_metrics = render_result.scroll_metrics.clone();
        self.scroll_metrics_cache = render_result
            .scroll_metrics
            .iter()
            .map(|(rest, b, max, content)| (rest.clone(), (*b, *max, *content)))
            .collect();

        self.pipeline
            .update_resolution(&self.queue, self.config.width, self.config.height);

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
            0,
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

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Secondary Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(crate::renderer::surface_clear_color(
                            self.compositor_alpha,
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
            render_pass.set_bind_group(0, &self.pipeline.bind_group, &[]);
            render_pass.set_vertex_buffer(0, self.pipeline.vertex_buffer.slice(..));
            render_pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
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

            if !prep.underline_instances.is_empty() {
                let underline_buffer =
                    self.device
                        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("Secondary Underline Buffer"),
                            contents: bytemuck::cast_slice(&prep.underline_instances),
                            usage: wgpu::BufferUsages::VERTEX,
                        });
                render_pass.set_vertex_buffer(1, underline_buffer.slice(..));
                render_pass.draw_indexed(
                    0..self.pipeline.num_indices,
                    0,
                    0..prep.underline_instances.len() as u32,
                );
            }

            // Page input caret / selection chrome (before page glyphs).
            if overlay_chrome_start > 0 {
                render_pass.set_pipeline(&self.pipeline.render_pipeline);
                render_pass.set_bind_group(0, &self.pipeline.bind_group, &[]);
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

            self.text_renderer.render(&mut render_pass);

            // Portal overlay: per-band `rects → text` so panels occlude lower glyphs.
            for (band_i, band) in overlay_band_ranges.iter().enumerate() {
                if band.instance.0 < band.instance.1 {
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
                }

                if band.chrome.0 < band.chrome.1 {
                    render_pass.set_pipeline(&self.pipeline.render_pipeline);
                    render_pass.set_bind_group(0, &self.pipeline.bind_group, &[]);
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

                if let Some(ul) = overlay_band_underlines.get(band_i) {
                    if !ul.is_empty() {
                        render_pass.set_pipeline(&self.pipeline.render_pipeline);
                        render_pass.set_bind_group(0, &self.pipeline.bind_group, &[]);
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
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        self.queue.present(output);

        if prep.culled_skip_prepare || prep.needs_warmup || overlay_needs_warmup {
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
                .push(RuiTextRenderer::new_sharing_font_system(
                    &self.device,
                    &self.queue,
                    &self.config,
                    font_ptr,
                ));
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

    pub(super) fn replace_root(&mut self, root: Element) {
        self.root_element = root;
        self.layout_status = LayoutStatus::StructureChanged;
        self.template_paths.clear();
        self.pending_layout_dirty.clear();
    }

    pub(super) fn reconcile_subtree(
        &mut self,
        anchor_id: &str,
        new_fragment: Element,
    ) -> LayoutStatus {
        let want = anchor_id.replace('\\', "/").to_ascii_lowercase();
        let status = if let Some(old) = find_template_mut(&mut self.root_element, &want) {
            *old = new_fragment;
            LayoutStatus::StructureChanged
        } else {
            drop(new_fragment);
            LayoutStatus::StructureChanged
        };
        self.layout_status = self.layout_status.merge(status);
        status
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
        if let Some(el) = find_template_mut(&mut self.root_element, &want) {
            let status = patch(el);
            self.layout_status = self.layout_status.merge(status);
            status
        } else {
            LayoutStatus::Clean
        }
    }
}

fn find_template_mut<'a>(el: &'a mut Element, want_normalized: &str) -> Option<&'a mut Element> {
    if let Some(tid) = el.template_id.as_deref() {
        if tid.replace('\\', "/").to_ascii_lowercase() == want_normalized {
            return Some(el);
        }
    }
    for child in &mut el.children {
        if let Some(hit) = find_template_mut(child, want_normalized) {
            return Some(hit);
        }
    }
    None
}
