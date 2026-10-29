use super::*;
use glyphon::RasterizeCustomGlyphRequest;

#[test]
#[ignore = "native GPU glyph cache pixel comparison; run explicitly"]
fn shared_cpu_glyph_rasters_preserve_independent_meshes() {
    use super::super::offscreen::{readback_texture, OffscreenGpu};
    let gpu = OffscreenGpu::with_backends(wgpu::Backends::GL).expect("native GL GPU");
    let config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        width: 512,
        height: 128,
        present_mode: wgpu::PresentMode::Fifo,
        desired_maximum_frame_latency: 1,
        alpha_mode: wgpu::CompositeAlphaMode::Auto,
        color_space: Default::default(),
        view_formats: vec![],
    };
    let mut primary = JadixTextRenderer::new(&gpu.device, &gpu.queue, &config);
    let mut shared =
        JadixTextRenderer::new_sharing_resources(&gpu.device, &gpu.queue, &config, &primary);
    let mut independent = JadixTextRenderer::new_sharing_font_system(
        &gpu.device,
        &gpu.queue,
        &config,
        primary.font_system,
    );
    assert!(Rc::ptr_eq(&primary.swash_cache, &shared.swash_cache));
    assert!(
        primary.swash_cache.borrow().image_cache.is_empty(),
        "construction must not rasterize glyphs"
    );
    let mut buffer =
        glyphon::Buffer::new(primary.font_system_mut(), glyphon::Metrics::new(22.0, 30.0));
    buffer.set_size(Some(500.0), Some(120.0));
    buffer.set_text(
        "Gallery 字形缓存 ABC xyz",
        &glyphon::Attrs::new(),
        glyphon::Shaping::Advanced,
        None,
    );
    buffer.shape_until_scroll(primary.font_system_mut(), false);
    let area = |top| glyphon::TextArea {
        buffer: &buffer,
        left: 3.25,
        top,
        scale: 1.0,
        bounds: glyphon::TextBounds {
            left: 0,
            top: 0,
            right: 512,
            bottom: 128,
        },
        default_color: glyphon::Color::rgb(180, 220, 255),
        custom_glyphs: &[],
    };
    primary.prepare(&gpu.device, &gpu.queue, &config, std::iter::once(area(0.0)));
    let entries = primary.swash_cache.borrow().image_cache.len();
    assert!(entries > 0);
    shared.prepare(
        &gpu.device,
        &gpu.queue,
        &config,
        std::iter::once(area(45.0)),
    );
    assert_eq!(
        primary.swash_cache.borrow().image_cache.len(),
        entries,
        "integer translation must reuse CPU rasters"
    );
    independent.prepare(
        &gpu.device,
        &gpu.queue,
        &config,
        std::iter::once(area(45.0)),
    );
    let draw = |other: &JadixTextRenderer| {
        let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("shared glyph raster pixels"),
            size: wgpu::Extent3d {
                width: 512,
                height: 128,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: config.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        let mut encoder = gpu.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            primary.render(&mut pass);
            other.render(&mut pass);
        }
        readback_texture(&gpu.device, &gpu.queue, &texture, 512, 128, encoder).expect("readback")
    };
    let actual = draw(&shared);
    let expected = draw(&independent);
    assert_eq!(actual.pixels, expected.pixels);
    assert!(
        actual.pixels.chunks_exact(4).any(|p| p[3] != 0),
        "comparison must contain visible glyphs"
    );
    if let Some(dir) = std::env::var_os("JADIX_GLYPH_TEST_OUTPUT") {
        std::fs::create_dir_all(&dir).unwrap();
        image::save_buffer(
            std::path::Path::new(&dir).join("shared-glyph-rasters.png"),
            &actual.pixels,
            512,
            128,
            image::ColorType::Rgba8,
        )
        .unwrap();
    }
}

fn sample_raster(w: u16, h: u16) -> EmojiRaster {
    EmojiRaster::new(w, h, vec![10; (w as usize) * (h as usize) * 4])
}

#[test]
fn glyphs_for_falls_back_to_wrap_sibling() {
    std::env::set_var("JADIX_EMOJI_NODE_CACHE_MAX", "8");
    let mut cache = EmojiGlyphCache::default();
    let key_a = EmojiCacheKey {
        sequence_hash: 0xE111,
        pixel_size: 16,
        scale_bits: 1.0f32.to_bits(),
        color_mode: 1,
        wrap_width_q: 10,
    };
    let mut rasters = HashMap::new();
    rasters.insert(3, sample_raster(16, 16));
    cache.insert_emoji_node(
        key_a.clone(),
        EmojiGlyphEntry {
            glyphs: vec![glyphon::CustomGlyph {
                id: 3,
                left: 0.0,
                top: 0.0,
                width: 16.0,
                height: 16.0,
                color: None,
                snap_to_physical_pixel: true,
                metadata: 0,
            }],
            rasters,
        },
    );
    let key_b = EmojiCacheKey {
        sequence_hash: 0xE111,
        pixel_size: 16,
        scale_bits: 1.0f32.to_bits(),
        color_mode: 1,
        wrap_width_q: 11, // different wrap bucket
    };
    assert!(
        !cache.entries.contains_key(&key_b),
        "exact wrap miss expected"
    );
    assert_eq!(
        cache.glyphs_for(&key_b).len(),
        1,
        "paint must reuse wrap sibling glyphs (else emoji blanks on soft-width jitter)"
    );
}

#[test]
fn rasterize_survives_node_lru_eviction() {
    let mut cache = EmojiGlyphCache::default();
    let key_a = EmojiCacheKey {
        sequence_hash: 0xA,
        pixel_size: 16,
        scale_bits: 1.0f32.to_bits(),
        color_mode: 1,
        wrap_width_q: 10,
    };
    let mut rasters = HashMap::new();
    rasters.insert(7, sample_raster(16, 16));
    cache.insert_emoji_node(
        key_a.clone(),
        EmojiGlyphEntry {
            glyphs: vec![],
            rasters,
        },
    );
    // Simulate node LRU eviction: entry map drops but id_rasters keeps stable ids.
    cache.entries.remove(&key_a);
    let out = cache.rasterize(RasterizeCustomGlyphRequest {
        id: 7,
        width: 16,
        height: 16,
        x_bin: glyphon::SubpixelBin::Zero,
        y_bin: glyphon::SubpixelBin::Zero,
        scale: 1.0,
    });
    assert!(
        out.is_some(),
        "glyphon requires Some after Some — id_rasters must outlive node LRU"
    );
    assert_eq!(out.unwrap().data.len(), 16 * 16 * 4);
}

#[test]
fn rasterize_resizes_when_request_size_differs_from_cache() {
    let mut cache = EmojiGlyphCache::default();
    let mut rgba = vec![0u8; 16 * 16 * 4];
    // Mark center pixel opaque red so resize must preserve ink (not all-zero placeholder).
    rgba[(8 * 16 + 8) * 4] = 255;
    rgba[(8 * 16 + 8) * 4 + 3] = 255;
    let key = EmojiCacheKey {
        sequence_hash: 0xE1,
        pixel_size: 16,
        scale_bits: 1.0f32.to_bits(),
        color_mode: 1,
        wrap_width_q: 10,
    };
    cache.insert_emoji_node(
        key,
        EmojiGlyphEntry {
            glyphs: vec![glyphon::CustomGlyph {
                id: 3,
                left: 0.0,
                top: 0.0,
                width: 14.0, // paint quad smaller than raster (old 1.15 bug shape)
                height: 14.0,
                color: None,
                snap_to_physical_pixel: true,
                metadata: 0,
            }],
            rasters: {
                let mut m = HashMap::new();
                m.insert(3, EmojiRaster::new(16, 16, rgba));
                m
            },
        },
    );
    let out = cache
        .rasterize(RasterizeCustomGlyphRequest {
            id: 3,
            width: 14,
            height: 14,
            x_bin: glyphon::SubpixelBin::Zero,
            y_bin: glyphon::SubpixelBin::Zero,
            scale: 1.0,
        })
        .expect("must Some");
    assert_eq!(out.data.len(), 14 * 14 * 4);
    assert!(
        out.data.iter().any(|&b| b != 0),
        "size mismatch must NN-resize cached ink, not return transparent placeholder"
    );
}

#[test]
fn rasterize_never_returns_none() {
    let cache = EmojiGlyphCache::default();
    let out = cache.rasterize(RasterizeCustomGlyphRequest {
        id: 99,
        width: 8,
        height: 8,
        x_bin: glyphon::SubpixelBin::Zero,
        y_bin: glyphon::SubpixelBin::Zero,
        scale: 1.0,
    });
    assert!(out.is_some());
    assert_eq!(out.unwrap().data.len(), 8 * 8 * 4);
}
