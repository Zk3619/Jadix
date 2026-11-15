//! Retained, windowless submission of the production Element display list.
use std::{collections::HashMap, io};

use super::{readback_texture_with_buffer, OffscreenGpu, RgbaImage};

/// Last successful frame. CPU elapsed includes submission and synchronous GPU
/// readback; it is not a GPU timestamp or window input-to-present latency.
#[derive(Clone, Copy, Debug)]
pub struct SceneFrameStats {
    pub frame: u64,
    pub pixels: [u32; 2],
    pub cpu_elapsed_ms: f64,
    pub layout: crate::layout::layout_tree::LayoutProfileMs,
    pub resident: crate::layout::layout_tree::LayoutTreeStats,
    pub rectangles: usize,
    pub textures: usize,
    pub text_nodes: usize,
    pub collect_replayed: u32,
}
use crate::{
    app::{image_registry, pending_images, state},
    core::tree::Element,
    layout::layout_tree::{LayoutTree, OverlayBandRange},
    renderer::{
        buffer_upload::RetainedBuffer,
        path_pipeline::PathGpu,
        prepare_text::{prepare_text_frame_at, PrepareTextOutput},
        scale_clip_polys_in_place, scale_instances_in_place, scale_textured_draws,
        BackdropBlurPipeline, JadixPipeline, JadixTextRenderer, PassKind, TexturePool,
        TexturedQuadPipeline,
    },
};

/// A UI-thread renderer with retained device, pipelines, fonts, texture pool,
/// vertex streams and render target. No winit window or surface is created.
///
/// Supports ordinary 2D Element scenes, glyphs/decorations/shadows, images,
/// paths/Canvas, overflow/rounded/polygon clips and page/portal paint bands.
/// Portal fades and ordered backdrop blur reuse the production pipelines.
/// Effect/capture/3D/shader passes and optical glass return `Unsupported`;
/// unavailable textures return `WouldBlock` rather than a successful blank frame.
/// Output is opaque, display-sRGB RGBA8; `size` is physical and `scale` maps
/// logical layout pixels to it. A frame is limited to 64 MiB and device limits.
pub struct SceneRenderer {
    gpu: OffscreenGpu,
    pipeline: JadixPipeline,
    textured: TexturedQuadPipeline,
    bands: Vec<JadixTextRenderer>,
    backdrops: Vec<Option<BackdropBlurPipeline>>,
    text: JadixTextRenderer,
    paths: Option<PathGpu>,
    textures: TexturePool,
    instances: RetainedBuffer,
    textured_instances: RetainedBuffer,
    tree: LayoutTree,
    config: wgpu::SurfaceConfiguration,
    target: Option<wgpu::Texture>,
    readback: Option<wgpu::Buffer>,
    frames: u64,
    last_frame: Option<SceneFrameStats>,
    reuse_collect: bool,
    last_scale: f32,
    known_paint_paths: bool,
}

impl SceneRenderer {
    pub fn new(gpu: OffscreenGpu) -> Self {
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            width: 1,
            height: 1,
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 1,
            alpha_mode: wgpu::CompositeAlphaMode::Opaque,
            color_space: Default::default(),
            view_formats: vec![],
        };
        let device = &gpu.device;
        Self {
            pipeline: JadixPipeline::from_format(device, config.format, 1, 1),
            textured: TexturedQuadPipeline::new(device, &config),
            text: JadixTextRenderer::new(device, &gpu.queue, &config),
            bands: Vec::new(),
            backdrops: Vec::new(),
            paths: PathGpu::try_new(device, 1),
            textures: TexturePool::new(),
            instances: RetainedBuffer::new(
                device,
                "headless rects",
                wgpu::BufferUsages::VERTEX,
                &[],
            ),
            textured_instances: RetainedBuffer::new(
                device,
                "headless textures",
                wgpu::BufferUsages::VERTEX,
                &[],
            ),
            tree: LayoutTree::new(),
            target: None,
            readback: None,
            config,
            gpu,
            frames: 0,
            last_frame: None,
            reuse_collect: false,
            last_scale: 0.0,
            known_paint_paths: false,
        }
    }

    pub fn adapter_info(&self) -> &wgpu::AdapterInfo {
        &self.gpu.adapter_info
    }
    pub fn rendered_frames(&self) -> u64 {
        self.frames
    }

    pub fn last_frame_stats(&self) -> Option<SceneFrameStats> {
        self.last_frame
    }

    /// Request retained layout and fingerprint/geometry-validated display-list
    /// replay for the next frame only. The caller must keep structure unchanged
    /// and apply leaf changes through `replace_root` before this call. Live paint,
    /// changed geometry and fingerprints miss replay. A first/failed frame or
    /// viewport/scale change uses the full path. A later replacement cancels this.
    pub fn reuse_collect_next_frame(&mut self) {
        self.reuse_collect = true;
    }
    /// Replace an addressed plain leaf and invalidate its paint hash ancestors.
    /// Paths refer to this renderer's retained root. Geometry is recomputed;
    /// only unchanged subtree fingerprints are reused on the next valid frame.
    pub fn replace_leaf(&mut self, root: &mut Element, path: &[usize], next: Element) -> bool {
        let mut leaf = &mut *root;
        for index in path {
            let Some(child) = leaf.children.get_mut(*index) else {
                return false;
            };
            leaf = child;
        }
        if !leaf.children.is_empty() || !next.children.is_empty() {
            return false;
        }
        let stamp = crate::layout::collect_cache::leaf_paint_stamp(leaf);
        self.replace_root(leaf, next);
        crate::layout::collect_cache::refresh_leaf_paint_path(root, path, stamp);
        self.known_paint_paths = true;
        true
    }

    /// Replace a tree using the same keyed reconciliation as the window App.
    /// Call before `render`, with a root previously rendered by this renderer.
    /// Matched nodes retain layout identity and hooks; removed nodes unmount.
    /// Projection, reconciliation, layout synchronization and collection still
    /// scan trees. This API does not imply O(k) rendering or transfer owners.
    pub fn replace_root(
        &mut self,
        root: &mut Element,
        mut next: Element,
    ) -> crate::core::tree::LayoutStatus {
        self.reuse_collect = false;
        self.known_paint_paths = false;
        let mut dirty = Vec::new();
        let status = state::reconcile_scene_tree(&mut next, root, &mut dirty);
        self.tree
            .mark_dirty_nodes_in_tree(&next, &dirty.into_iter().collect());
        *root = next;
        status
    }

    /// Resolve a terminal/remote pointer against the same font database and
    /// logical text geometry used by this scene, without synthesizing OS keys.
    pub fn hit_text(&mut self, leaf: &Element, pointer: [f32; 2]) -> usize {
        crate::renderer::hit_test_input_text_index(
            self.text.font_system_mut(),
            leaf.text.as_deref().unwrap_or(""),
            pointer[0],
            pointer[1],
            leaf.viewport_layout_bounds(),
            &leaf.text_style,
            1.0,
        )
    }
    /// Same caret phase/deadline as the windowed renderer. Hosts schedule it
    /// only while an editable caret is visible and the terminal has focus.
    pub fn caret_wakeup_after(&self) -> std::time::Duration {
        crate::renderer::prepare_text::caret_blink_next_toggle_at()
            .saturating_duration_since(web_time::Instant::now())
    }
    pub fn restart_caret(&self) {
        crate::renderer::prepare_text::restart_caret_blink();
    }

    pub fn render(
        &mut self,
        root: &mut Element,
        size: [u32; 2],
        scale: f32,
        pointer: [f32; 2],
        clear: wgpu::Color,
    ) -> io::Result<RgbaImage> {
        let reuse = std::mem::take(&mut self.reuse_collect)
            && self.last_frame.is_some_and(|f| f.pixels == size)
            && self.last_scale == scale;
        let known_paths = std::mem::take(&mut self.known_paint_paths) && reuse;
        self.last_frame = None;
        let started = web_time::Instant::now();
        let [width, height] = size;
        let limit = self.gpu.device.limits().max_texture_dimension_2d;
        if width == 0
            || height == 0
            || width > limit
            || height > limit
            || u64::from(width) * u64::from(height) * 4 > 64 * 1024 * 1024
            || !scale.is_finite()
            || scale <= 0.0
            || !(width as f32 / scale).is_finite()
            || !(height as f32 / scale).is_finite()
            || [clear.r, clear.g, clear.b, clear.a]
                .iter()
                .any(|v| !v.is_finite())
            || clear.a != 1.0
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid opaque headless viewport",
            ));
        }

        self.tree.set_collect_skip_allowed(reuse);
        if known_paths {
            self.tree.reuse_known_paint_paths();
        }
        let mut scene =
            crate::text::measure::with_shared_font_system(self.text.font_system_mut(), || {
                if reuse {
                    self.tree.build_and_layout_fast(
                        root,
                        width as f32 / scale,
                        height as f32 / scale,
                        pointer,
                        false,
                        false,
                    )
                } else {
                    self.tree.build_and_layout(
                        root,
                        width as f32 / scale,
                        height as f32 / scale,
                        pointer,
                    )
                }
            });
        if root.subtree_offscreen_unsupported
            || scene.render_graph.nodes.iter().any(|n| {
                matches!(
                    n.kind,
                    PassKind::SubtreeCapture
                        | PassKind::Effect
                        | PassKind::Scene3d
                        | PassKind::Shader
                        | PassKind::Compute
                )
            })
        {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "headless scene requires an unsupported effect/capture/3D/shader pass",
            ));
        }

        let mut ranges = scene.overlay_split.page_stack_band_ranges();
        if ranges.is_empty() {
            ranges.push(OverlayBandRange {
                instance: (0, scene.overlay_split.instance_start),
                textured: (0, scene.overlay_split.textured_start),
                text: (0, scene.overlay_split.text_start),
                chrome: (0, scene.overlay_split.chrome_start),
                paint_ops: (0, scene.overlay_split.paint_op_start),
                backdrop_blur: 0.0,
                underlay_opacity: 1.0,
                stamp_key: 0,
            });
        }
        ranges.extend(scene.overlay_split.band_ranges(
            scene.instances.len(),
            scene.textured_draws.len(),
            scene.text_nodes.len(),
            scene.input_chrome_instances.len(),
            scene.paint_ops.len(),
        ));
        let blur_radii: Vec<f32> = ranges
            .iter()
            .map(|band| {
                scene.instances[band.instance.0..band.instance.1]
                    .iter()
                    .map(|instance| instance.backdrop_blur)
                    .fold(band.backdrop_blur, f32::max)
                    * scale
            })
            .collect();
        let blur_count = blur_radii.iter().filter(|r| **r > 0.5).count() as u64;
        // Each blur owns independent uniforms and at most four full-sized targets.
        // Bound the conservative live allocation, including the output target.
        if u64::from(width) * u64::from(height) * 4 * (1 + 4 * blur_count) > 256 * 1024 * 1024 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "headless backdrop targets exceed 256 MiB",
            ));
        }
        scale_instances_in_place(&mut scene.instances, scale);
        scale_instances_in_place(&mut scene.input_chrome_instances, scale);
        scale_clip_polys_in_place(&mut scene.clip_polys, scale);
        let scaled = scale_textured_draws(&scene.textured_draws, scale);
        self.textures.begin_frame();
        for d in &scaled {
            self.textures.touch(&d.texture_key);
            if self.textures.get(&d.texture_key).is_none() {
                pending_images::mark_src_needed(&d.texture_key);
            }
        }
        crate::style::flush_gradient_textures(|key| self.textures.contains_key(key));
        crate::utils::icon_texture::flush_icon_textures(|key| self.textures.contains_key(key));
        crate::primitives::path::flush_path_textures(|key| self.textures.contains_key(key));
        #[cfg(feature = "svg")]
        crate::media::svg_raster::flush_svg_textures(|key| self.textures.contains_key(key));
        pending_images::flush_src_textures(|key| self.textures.contains_key(key));
        crate::media::texture::flush_retained_textures(
            scaled.iter().map(|d| d.texture_key.as_str()),
            |key| self.textures.contains_key(key),
        );
        crate::utils::text_texture::flush_text_xform_textures(|key| {
            self.textures.contains_key(key)
        });
        let device = &self.gpu.device;
        let queue = &self.gpu.queue;
        // Consume only this scene's pending keys; do not steal another host's uploads.
        for d in &scaled {
            if let Some((rgba, w, h)) = pending_images::take_pending_key(&d.texture_key) {
                let linear = crate::utils::text_texture::is_text_xform_texture_key(&d.texture_key);
                let format = if linear {
                    wgpu::TextureFormat::Rgba8Unorm
                } else {
                    self.config.format
                };
                if !self
                    .textures
                    .try_overwrite_rgba8(&d.texture_key, queue, &rgba, w, h, format)
                {
                    let upload = if linear {
                        image_registry::upload_rgba8_linear
                    } else {
                        image_registry::upload_rgba8
                    };
                    let texture = upload(device, queue, &self.textured, &rgba, w, h);
                    self.textures
                        .insert_with_size(d.texture_key.clone(), texture, w, h);
                }
            }
        }
        if let Some(paths) = &mut self.paths {
            paths.flush_matching(
                device,
                queue,
                &self.textured,
                self.textures.as_map_mut(),
                |key| scaled.iter().any(|d| d.texture_key == key),
            );
        }
        // Path target metadata and pooled textures share the scene lifetime.
        if let Some(d) = scaled
            .iter()
            .find(|d| !self.textures.contains_key(&d.texture_key))
        {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                format!("texture not ready: {}", d.texture_key),
            ));
        }
        self.textures
            .retain_keys(|key| scaled.iter().any(|d| d.texture_key == key));
        if let Some(paths) = &mut self.paths {
            paths.retain_targets(|key| self.textures.contains_key(key));
        }
        self.pipeline.update_resolution(queue, width, height);
        self.textured.update_resolution(queue, width, height);
        self.pipeline
            .ensure_clip_polys(device, queue, &scene.clip_polys);
        self.instances
            .upload(device, queue, bytemuck::cast_slice(&scene.instances));
        let textured_instances: Vec<_> = scaled.iter().map(|d| d.to_instance()).collect();
        self.textured_instances
            .upload(device, queue, bytemuck::cast_slice(&textured_instances));
        self.config.width = width;
        self.config.height = height;
        while self.bands.len() < ranges.len() {
            self.bands.push(JadixTextRenderer::new_sharing_resources(
                device,
                queue,
                &self.config,
                &self.text,
            ));
        }
        self.bands.truncate(ranges.len());
        self.backdrops.resize_with(ranges.len(), || None);
        for (slot, radius) in self.backdrops.iter_mut().zip(&blur_radii) {
            if *radius > 0.5 {
                let backdrop = slot
                    .get_or_insert_with(|| BackdropBlurPipeline::new(device, self.config.format));
                backdrop.ensure_size(device, width, height, self.config.format);
            } else {
                *slot = None;
            }
        }
        let mut prepared: Vec<PrepareTextOutput> = Vec::new();
        let mut shadows = HashMap::new();
        for (band, text) in ranges.iter().zip(&mut self.bands) {
            let output = prepare_text_frame_at(
                text,
                device,
                queue,
                width,
                height,
                &scene.text_nodes[band.text.0..band.text.1],
                scale,
                false,
                &Default::default(),
                &Default::default(),
            );
            for bitmap in &output.shadow_new_bitmaps {
                shadows.insert(
                    bitmap.cache_key,
                    image_registry::upload_rgba8(
                        device,
                        queue,
                        &self.textured,
                        &bitmap.rgba,
                        bitmap.width,
                        bitmap.height,
                    ),
                );
            }
            prepared.push(output);
        }
        if self
            .target
            .as_ref()
            .is_none_or(|t| t.width() != width || t.height() != height)
        {
            self.target = Some(device.create_texture(&wgpu::TextureDescriptor {
                label: Some("jadix retained headless target"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: self.config.format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            }));
        }
        let target = self.target.as_ref().unwrap();
        let view = target.create_view(&Default::default());
        let mut encoder = device.create_command_encoder(&Default::default());
        for (index, (((band, text), output), radius)) in ranges
            .iter()
            .zip(&self.bands)
            .zip(&prepared)
            .zip(&blur_radii)
            .enumerate()
        {
            // Capture only the already-painted prefix. Each layer's blur resources
            // and uniform ring remain independent until this encoder is submitted.
            let backdrop_binding = if let Some(backdrop) = &mut self.backdrops[index] {
                if index == 0 {
                    let _clear = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("headless empty backdrop"),
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
                }
                if !backdrop.capture_from(&mut encoder, target) {
                    return Err(io::Error::other("incompatible headless backdrop capture"));
                }
                if band.backdrop_blur > 0.5 {
                    // The shared collector deliberately omits viewport-underlay
                    // blur from SDF instances: these use a fullscreen, opacity-
                    // weighted blit, exactly like the production window path.
                    backdrop.blur_modal(device, queue, &mut encoder, *radius, *radius, None);
                    backdrop.blit_to(queue, &mut encoder, &view, band.underlay_opacity, clear);
                    None
                } else {
                    backdrop.blur(device, queue, &mut encoder, *radius, None);
                    Some(self.pipeline.make_optical_backdrop_bind_group(
                        device,
                        backdrop.blurred_view().unwrap(),
                        backdrop.blurred_view().unwrap(),
                        backdrop.sampler(),
                    ))
                }
            } else {
                None
            };
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("jadix headless scene"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: if index == 0 && self.backdrops[index].is_none() {
                            wgpu::LoadOp::Clear(clear)
                        } else {
                            wgpu::LoadOp::Load
                        },
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            let dummy = backdrop_binding
                .as_ref()
                .unwrap_or_else(|| self.pipeline.dummy_backdrop());
            state::draw_paint_ops(
                &mut pass,
                &scene.paint_ops,
                band.paint_ops.0,
                band.paint_ops.1,
                &self.pipeline,
                &self.instances.buffer,
                &self.textured,
                &self.textured_instances.buffer,
                self.textures.as_map(),
                &scaled,
                dummy,
                &scene.instances,
                None,
            );
            state::draw_shadow_tex_draws(
                &mut pass,
                &output.shadow_tex_draws,
                &shadows,
                &self.textured,
                device,
                None,
            );
            state::draw_cpu_sdf_instances(
                &mut pass,
                &self.pipeline,
                device,
                dummy,
                &output.underline_instances,
            );
            text.render(&mut pass);
            state::draw_cpu_sdf_instances(
                &mut pass,
                &self.pipeline,
                device,
                dummy,
                &output.caret_instances,
            );
            state::draw_cpu_sdf_instances(
                &mut pass,
                &self.pipeline,
                device,
                dummy,
                &scene.input_chrome_instances[band.chrome.0..band.chrome.1],
            );
        }
        let readback_bytes = u64::from((width * 4).div_ceil(256) * 256) * u64::from(height);
        if self
            .readback
            .as_ref()
            .is_none_or(|b| b.size() != readback_bytes)
        {
            self.readback = Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("jadix retained headless readback"),
                size: readback_bytes,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            }));
        }
        let image = readback_texture_with_buffer(
            device,
            queue,
            target,
            width,
            height,
            encoder,
            self.readback.as_ref().unwrap(),
        )
        .ok_or_else(|| io::Error::other("headless GPU readback failed"))?;
        for text in &mut self.bands {
            text.trim_glyph_atlas();
        }
        self.frames = self.frames.saturating_add(1);
        self.last_scale = scale;
        self.last_frame = Some(SceneFrameStats {
            frame: self.frames,
            pixels: size,
            cpu_elapsed_ms: started.elapsed().as_secs_f64() * 1000.0,
            layout: scene.prof,
            resident: self.tree.stats(),
            rectangles: scene.instances.len(),
            textures: scene.textured_draws.len(),
            text_nodes: scene.text_nodes.len(),
            collect_replayed: self.tree.last_collect_skip_counts().0,
        });
        Ok(image)
    }
}
