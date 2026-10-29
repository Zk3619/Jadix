use super::*;
use crate::primitives::scene3d::{Scene3dJob, Scene3dLight, Scene3dMaterial};
use crate::renderer::offscreen::OffscreenGpu;

#[test]
#[ignore = "requires a headless GPU adapter"]
fn scene_billboards_match_bgra_and_rgba_render_targets() {
    let gpu = OffscreenGpu::new().expect("headless GPU adapter");
    for format in [
        wgpu::TextureFormat::Bgra8UnormSrgb,
        wgpu::TextureFormat::Rgba8UnormSrgb,
    ] {
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: 96,
            height: 96,
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 1,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            color_space: Default::default(),
            view_formats: vec![],
        };
        let textured = TexturedQuadPipeline::new(&gpu.device, &config);
        let mut scene = Scene3dGpu::try_new(&gpu.device, format).expect("scene pipeline");
        let mut images = HashMap::new();
        images.insert(
            "billboard".into(),
            crate::app::image_registry::upload_rgba8(
                &gpu.device,
                &gpu.queue,
                &textured,
                &[255; 4],
                1,
                1,
            ),
        );
        for size in [64, 96, 64] {
            let validation = gpu.device.push_error_scope(wgpu::ErrorFilter::Validation);
            scene.render_job(
                &gpu.device,
                &gpu.queue,
                &textured,
                &mut images,
                Scene3dJob {
                    key: "scene".into(),
                    width: size,
                    height: size,
                    yaw: 0.3,
                    pitch: 0.2,
                    fov_deg: 45.0,
                    camera_z: 2.35,
                    bg: [0.0, 0.0, 0.0, 1.0],
                    light: Scene3dLight::default(),
                    material: Scene3dMaterial::default(),
                    text_tex_key: Some("billboard".into()),
                },
            );
            let error = crate::utils::block_on::block_on(validation.pop());
            assert!(error.is_none(), "{format:?}, {size}px: {error:?}");
            assert_eq!(images["scene"]._texture.format(), format);
        }
    }
}
