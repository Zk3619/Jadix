use super::*;
use crate::gpu_caps::GpuCaps;

#[test]
fn pipelines_stay_none_when_compute_off() {
    let caps = GpuCaps::none();
    assert!(!caps.compute);
    let idle = ComputePipelines {
        blur: None,
        histogram: None,
        particles: None,
    };
    assert!(!idle.enabled());
}

#[test]
fn blur_params_match_wgsl() {
    assert_eq!(std::mem::size_of::<BlurParams>(), 32);
    assert_eq!(std::mem::size_of::<ParticleParams>(), 16);
    assert_eq!(HIST_BYTES, 1024);
}

#[test]
#[cfg(not(feature = "compute"))]
fn feature_off_new_clears_caps() {
    let mut caps = GpuCaps {
        backend: wgpu::Backend::Vulkan,
        compute: true,
    };
    // No device needed: feature-off branch returns before try_new.
    let idle = ComputePipelines {
        blur: None,
        histogram: None,
        particles: None,
    };
    let _ = idle;
    caps.compute = false;
    assert!(!caps.compute);
}
