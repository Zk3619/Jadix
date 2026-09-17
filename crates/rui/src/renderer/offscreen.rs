//! Offscreen / headless GPU helpers for golden (pixel snapshot) tests.
//!
//! These do **not** replace the windowed `App` path; they allocate a device and an
//! offscreen color texture so CI can render a tiny scene without a display.
//!
//! Enable with env `RUI_FORCE_SOFTWARE=1` for WARP/llvmpipe when available.

/// RGBA8 image buffer from an offscreen render.
#[derive(Clone, Debug)]
pub struct RgbaImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl RgbaImage {
    pub fn pixel(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let i = ((y * self.width + x) * 4) as usize;
        Some([
            self.pixels[i],
            self.pixels[i + 1],
            self.pixels[i + 2],
            self.pixels[i + 3],
        ])
    }
}

/// Minimal GPU context for offscreen renders (no window / surface).
pub struct OffscreenGpu {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub adapter_info: wgpu::AdapterInfo,
}

impl OffscreenGpu {
    pub fn new() -> Option<Self> {
        Self::with_backends(wgpu::Backends::all())
    }

    pub fn with_backends(backends: wgpu::Backends) -> Option<Self> {
        let instance = super::wgpu_compat::instance(backends);
        let adapter = crate::utils::block_on::block_on(instance.request_adapter(
            &wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::LowPower,
                compatible_surface: None,
                force_fallback_adapter: std::env::var("RUI_FORCE_SOFTWARE").is_ok(),
                ..Default::default()
            },
        ))
        .ok()?;
        let required_limits = super::limits_for(&adapter);
        let (device, queue) = crate::utils::block_on::block_on(adapter.request_device(
            &super::wgpu_compat::device_desc(
                Some("rui-offscreen"),
                required_limits,
                Default::default(),
            ),
        ))
        .ok()?;
        super::wgpu_compat::install_device_error_handlers(&device);
        Some(Self {
            device,
            queue,
            adapter_info: adapter.get_info(),
        })
    }

    /// Clear an offscreen texture to `clear` and read back RGBA8 bytes.
    pub fn clear_readback(&self, width: u32, height: u32, clear: wgpu::Color) -> Option<RgbaImage> {
        self.render_instances_readback(width, height, clear, &[])
    }

    /// FullClear + draw rounded-rect [`InstanceInput`]s, then read back RGBA8.
    ///
    /// Used by WP-2.6 component goldens. `scale` is fixed at 1.0 (logical == physical).
    /// Text glyphs are **not** drawn here (chrome / clip geometry only) so CI stays
    /// cross-OS stable without embedded Inter; layout still measures text leaves.
    pub fn render_instances_readback(
        &self,
        width: u32,
        height: u32,
        clear: wgpu::Color,
        instances: &[super::InstanceInput],
    ) -> Option<RgbaImage> {
        use wgpu::util::DeviceExt;

        let width = width.max(1);
        let height = height.max(1);
        let format = wgpu::TextureFormat::Rgba8UnormSrgb;
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("rui-offscreen-tex"),
            size: wgpu::Extent3d {
                width,
                height,
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
        let pipeline = super::RuiPipeline::from_format(&self.device, format, width, height);
        pipeline.update_resolution(&self.queue, width, height);

        let instance_buffer = if instances.is_empty() {
            None
        } else {
            Some(
                self.device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("rui-offscreen-instances"),
                        contents: bytemuck::cast_slice(instances),
                        usage: wgpu::BufferUsages::VERTEX,
                    }),
            )
        };

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("rui-offscreen-enc"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("rui-offscreen-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(clear),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            if let Some(ref ib) = instance_buffer {
                pass.set_pipeline(&pipeline.render_pipeline);
                pass.set_bind_group(0, &pipeline.bind_group, &[]);
                pass.set_bind_group(1, pipeline.dummy_backdrop(), &[]);
                pass.set_vertex_buffer(0, pipeline.vertex_buffer.slice(..));
                pass.set_vertex_buffer(1, ib.slice(..));
                pass.set_index_buffer(pipeline.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
                pass.draw_indexed(0..pipeline.num_indices, 0, 0..instances.len() as u32);
            }
        }

        readback_texture(&self.device, &self.queue, &texture, width, height, encoder)
    }
}

pub(crate) fn readback_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    width: u32,
    height: u32,
    mut encoder: wgpu::CommandEncoder,
) -> Option<RgbaImage> {
    let bytes_per_row = (width * 4).div_ceil(256) * 256;
    let buffer_size = bytes_per_row as u64 * height as u64;
    let output_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("rui-offscreen-readback"),
        size: buffer_size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &output_buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(bytes_per_row),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    queue.submit(Some(encoder.finish()));

    let slice = output_buffer.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    let _ = device.poll(wgpu::PollType::wait_indefinitely());
    rx.recv().ok()?.ok()?;

    let data = slice.get_mapped_range().ok()?;
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for row in 0..height {
        let start = (row * bytes_per_row) as usize;
        let end = start + (width * 4) as usize;
        pixels.extend_from_slice(&data[start..end]);
    }
    drop(data);
    output_buffer.unmap();
    Some(RgbaImage {
        width,
        height,
        pixels,
    })
}

/// Layout `root` at fixed `width`×`height` (scale 1) and render chrome instances to RGBA.
///
/// Returns `None` when no GPU adapter is available (skip in that environment).
pub fn render_element_scene(
    root: &mut crate::core::tree::Element,
    width: u32,
    height: u32,
    clear: wgpu::Color,
) -> Option<RgbaImage> {
    let gpu = OffscreenGpu::new()?;
    let mut tree = crate::layout::layout_tree::LayoutTree::new();
    let result = tree.build_and_layout(root, width as f32, height as f32, [-1.0, -1.0]);
    let mut instances = result.instances;
    instances.extend(result.input_chrome_instances);
    // Portal overlay instances are already appended after overlay_split in collect order.
    gpu.render_instances_readback(width, height, clear, &instances)
}

/// Max channel difference allowed when comparing goldens (font/AA variance).
pub fn images_match(a: &RgbaImage, b: &RgbaImage, tolerance: u8) -> bool {
    if a.width != b.width || a.height != b.height || a.pixels.len() != b.pixels.len() {
        return false;
    }
    a.pixels
        .iter()
        .zip(b.pixels.iter())
        .all(|(x, y)| x.abs_diff(*y) <= tolerance)
}

const GOLDEN_MAGIC: &[u8; 4] = b"RUIG";

/// Serialize a golden as `magic + u32 width + u32 height + RGBA8 rows`.
///
/// Deliberately not PNG: goldens are compared byte-exactly after decode, and a raw format keeps
/// the test path free of an image-codec dependency.
pub fn encode_rgba_golden(image: &RgbaImage) -> Vec<u8> {
    let mut out = Vec::with_capacity(12 + image.pixels.len());
    out.extend_from_slice(GOLDEN_MAGIC);
    out.extend_from_slice(&image.width.to_le_bytes());
    out.extend_from_slice(&image.height.to_le_bytes());
    out.extend_from_slice(&image.pixels);
    out
}

/// Inverse of [`encode_rgba_golden`]. `None` on a bad magic, truncated header, or size mismatch.
pub fn decode_rgba_golden(bytes: &[u8]) -> Option<RgbaImage> {
    if bytes.len() < 12 || &bytes[0..4] != GOLDEN_MAGIC {
        return None;
    }
    let width = u32::from_le_bytes(bytes[4..8].try_into().ok()?);
    let height = u32::from_le_bytes(bytes[8..12].try_into().ok()?);
    let expected = (width as usize)
        .checked_mul(height as usize)?
        .checked_mul(4)?;
    let pixels = bytes.get(12..12 + expected)?.to_vec();
    Some(RgbaImage {
        width,
        height,
        pixels,
    })
}

/// Path of the golden named `name` under `crates/rui/tests/golden/`.
pub fn golden_path(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
        .join(format!("{name}.rgba"))
}

/// Compare `image` against the stored golden `name`, panicking on mismatch.
///
/// Set `RUI_UPDATE_GOLDEN=1` to (re)write the golden instead of asserting; a missing golden is
/// always written so the first run of a new case bootstraps itself.
pub fn assert_matches_golden(name: &str, image: &RgbaImage, tolerance: u8) {
    let path = golden_path(name);
    let update = std::env::var("RUI_UPDATE_GOLDEN").is_ok();

    if update || !path.exists() {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).expect("create golden dir");
        }
        std::fs::write(&path, encode_rgba_golden(image)).expect("write golden");
        return;
    }

    let bytes = std::fs::read(&path).expect("read golden");
    let expected = decode_rgba_golden(&bytes).unwrap_or_else(|| {
        panic!("golden `{name}` is corrupt; re-record with RUI_UPDATE_GOLDEN=1")
    });
    assert!(
        images_match(&expected, image, tolerance),
        "golden `{name}` mismatch ({}x{} vs {}x{}); re-record with RUI_UPDATE_GOLDEN=1",
        expected.width,
        expected.height,
        image.width,
        image.height
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn golden_encode_decode_roundtrip() {
        let img = RgbaImage {
            width: 2,
            height: 2,
            pixels: vec![
                255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
            ],
        };
        let decoded = decode_rgba_golden(&encode_rgba_golden(&img)).expect("decode");
        assert_eq!(decoded.width, 2);
        assert_eq!(decoded.height, 2);
        assert!(images_match(&img, &decoded, 0));
    }

    /// Pure-CPU synthetic 8×8 pattern golden (no GPU) for Stage 8 CI smoke.
    #[test]
    fn stage8_pattern_golden_cpu() {
        let mut pixels = Vec::with_capacity(8 * 8 * 4);
        for y in 0..8u32 {
            for x in 0..8u32 {
                let on = ((x ^ y) & 1) == 0;
                if on {
                    pixels.extend_from_slice(&[0x22, 0xC5, 0x5E, 0xFF]); // green
                } else {
                    pixels.extend_from_slice(&[0x0F, 0x17, 0x2A, 0xFF]); // slate
                }
            }
        }
        let img = RgbaImage {
            width: 8,
            height: 8,
            pixels,
        };
        // Roundtrip encode first, then assert against stored golden.
        let decoded = decode_rgba_golden(&encode_rgba_golden(&img)).expect("roundtrip");
        assert!(images_match(&img, &decoded, 0));
        assert_matches_golden("stage8_pattern", &img, 0);
    }

    #[test]
    fn golden_decode_rejects_foreign_bytes() {
        assert!(decode_rgba_golden(b"PNG\x00short").is_none());
    }

    #[test]
    fn offscreen_clear_readback_golden() {
        let Some(gpu) = OffscreenGpu::new() else {
            eprintln!("skip: no wgpu adapter");
            return;
        };
        let img = gpu
            .clear_readback(
                8,
                8,
                wgpu::Color {
                    r: 0.0,
                    g: 0.0,
                    b: 1.0,
                    a: 1.0,
                },
            )
            .expect("readback");
        // Software rasterizers differ slightly in sRGB encode; compare with slack.
        assert_matches_golden("offscreen_clear_blue_8x8", &img, 8);
    }

    #[test]
    fn offscreen_clear_readback_smoke() {
        let Some(gpu) = OffscreenGpu::new() else {
            eprintln!("skip: no wgpu adapter");
            return;
        };
        let img = gpu
            .clear_readback(
                8,
                8,
                wgpu::Color {
                    r: 1.0,
                    g: 0.0,
                    b: 0.0,
                    a: 1.0,
                },
            )
            .expect("readback");
        assert_eq!(img.width, 8);
        let p = img.pixel(0, 0).unwrap();
        // sRGB encode may not be exact 255; allow slack
        assert!(p[0] > 200, "expected red-ish, got {p:?}");
        assert!(p[1] < 40 && p[2] < 40, "expected red-ish, got {p:?}");
    }
}
