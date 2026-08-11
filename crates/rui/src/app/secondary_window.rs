//! Secondary window GPU + UI tree (MVP).
//!
//! Shares the primary [`super::state::AppState`]'s `Device` / `Queue` / `Instance`,
//! and hosts an independent surface, layout tree, and root element.

use std::sync::Arc;

use wgpu::util::DeviceExt;
use winit::dpi::PhysicalSize;
use winit::window::Window;

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
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    size: PhysicalSize<u32>,
    pipeline: RuiPipeline,
    instance_buffer: wgpu::Buffer,
    instance_buffer_capacity: usize,
    num_instances: u32,
    layout_tree: LayoutTree,
    text_renderer: RuiTextRenderer,
    root_element: Element,
    pub(super) layout_status: LayoutStatus,
}

impl SecondaryWindowState {
    pub(super) fn new(
        window: Arc<Window>,
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
        instance: &wgpu::Instance,
        primary_config: &wgpu::SurfaceConfiguration,
        mut root_element: Element,
    ) -> Result<Self, String> {
        let size = window.inner_size();
        let surface = instance
            .create_surface(window.clone())
            .map_err(|e| format!("secondary window: create_surface failed: {e}"))?;

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: primary_config.format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: primary_config.present_mode,
            alpha_mode: primary_config.alpha_mode,
            view_formats: vec![],
            desired_maximum_frame_latency: 1,
        };
        surface.configure(&device, &config);

        let pipeline = RuiPipeline::new(&device, &config);
        let text_renderer = if let Some(shared) = crate::text::shared_font_system_ptr() {
            RuiTextRenderer::new_sharing_font_system(&device, &queue, &config, shared)
        } else {
            // Headless / tests without primary shared FS: own a FontSystem.
            RuiTextRenderer::new(&device, &queue, &config)
        };

        super::state::recompute_scroll_viewport_flags(&mut root_element);
        let _ = crate::layout::layout_tree::hoist_portal_subtrees(&mut root_element);

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
            root_element,
            layout_status: LayoutStatus::StructureChanged,
        })
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
        self.pipeline
            .update_resolution(&self.queue, self.config.width, self.config.height);
        self.layout_status = LayoutStatus::LayoutChanged;
    }

    pub(super) fn recover_surface(&mut self) {
        self.surface.configure(&self.device, &self.config);
    }

    /// Layout + clear + draw primitives/text for this secondary window.
    pub(super) fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
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
        crate::core::tree::sync_reactive_elements_subtree(
            &mut self.root_element,
            &mut self.layout_tree,
            &mut sync_impact,
        );

        let _ = crate::layout::layout_tree::hoist_portal_subtrees(&mut self.root_element);

        let use_full = self.layout_tree.root_node.is_none()
            || matches!(
                self.layout_status,
                LayoutStatus::StructureChanged | LayoutStatus::LayoutChanged
            )
            || sync_impact != LayoutStatus::Clean;

        let render_result = if use_full {
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
                true,
                true,
            )
        };

        let scaled_instances = scale_instances(&render_result.instances, scale);
        self.num_instances = render_result.instances.len() as u32;

        if scaled_instances.len() > self.instance_buffer_capacity {
            self.instance_buffer =
                self.device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Secondary Instance Buffer"),
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
        self.layout_status = LayoutStatus::Clean;

        self.pipeline
            .update_resolution(&self.queue, self.config.width, self.config.height);

        let empty_keys = std::collections::HashSet::new();
        let empty_rects = std::collections::HashMap::new();
        let prep = prepare_text_frame(
            &mut self.text_renderer,
            &self.device,
            &self.queue,
            &self.config,
            &render_result.text_nodes,
            scale,
            false,
            &empty_keys,
            &empty_rects,
        );

        let output = self.surface.get_current_texture()?;
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
            render_pass.set_index_buffer(
                self.pipeline.index_buffer.slice(..),
                wgpu::IndexFormat::Uint16,
            );
            if self.num_instances > 0 {
                render_pass.draw_indexed(0..self.pipeline.num_indices, 0, 0..self.num_instances);
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

            self.text_renderer.render(&mut render_pass);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        if prep.culled_skip_prepare || prep.needs_warmup {
            self.window.request_redraw();
        }
        if crate::text::take_fit_needs_relayout() {
            self.window.request_redraw();
        }
        Ok(())
    }
}
