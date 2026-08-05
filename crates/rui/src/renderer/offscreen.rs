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
}

impl OffscreenGpu {
    pub fn new() -> Option<Self> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: None,
            force_fallback_adapter: std::env::var("RUI_FORCE_SOFTWARE").is_ok(),
        }))?;
        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("rui-offscreen"),
                required_features: wgpu::Features::empty(),
                required_limits:
                    wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits()),
                memory_hints: Default::default(),
            },
            None,
        ))
        .ok()?;
        Some(Self { device, queue })
    }

    /// Clear an offscreen texture to `clear` and read back RGBA8 bytes.
    pub fn clear_readback(&self, width: u32, height: u32, clear: wgpu::Color) -> Option<RgbaImage> {
        let width = width.max(1);
        let height = height.max(1);
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
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("rui-offscreen-enc"),
            });
        {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("rui-offscreen-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(clear),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
        }

        let bytes_per_row = (width * 4).div_ceil(256) * 256;
        let buffer_size = bytes_per_row as u64 * height as u64;
        let output_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rui-offscreen-readback"),
            size: buffer_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            wgpu::ImageCopyTexture {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::ImageCopyBuffer {
                buffer: &output_buffer,
                layout: wgpu::ImageDataLayout {
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
        self.queue.submit(Some(encoder.finish()));

        let slice = output_buffer.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        self.device.poll(wgpu::Maintain::Wait);
        rx.recv().ok()?.ok()?;

        let data = slice.get_mapped_range();
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

#[cfg(test)]
mod tests {
    use super::*;

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
