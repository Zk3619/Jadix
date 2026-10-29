use super::*;
use crate::media::yuv::{i420_frame, media_frame_to_rgba8};

#[test]
fn wgsl_i420_matches_cpu_gray() {
    let Some(gpu) = crate::renderer::offscreen::OffscreenGpu::new() else {
        eprintln!("skip yuv gpu: no adapter");
        return;
    };
    let convert = YuvConvertGpu::new(&gpu.device);
    let w = 8u32;
    let h = 4u32;
    let y = vec![126u8; (w * h) as usize];
    let u = vec![128u8; 4 * 2];
    let v = vec![128u8; 4 * 2];
    let gpu_rgba = convert
        .convert_i420_readback(&gpu.device, &gpu.queue, w, h, &y, &u, &v)
        .expect("gpu convert");
    let cpu = media_frame_to_rgba8(&i420_frame(w, h, &y, &u, &v)).expect("cpu");
    assert_eq!(gpu_rgba.len(), cpu.len());
    for (a, b) in gpu_rgba.iter().zip(cpu.iter()) {
        assert!(a.abs_diff(*b) <= 3, "gpu {gpu_rgba:?} vs cpu {cpu:?}");
    }
}
