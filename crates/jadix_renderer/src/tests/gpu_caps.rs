use super::*;

#[test]
fn backend_and_feature_both_gate_compute() {
    let limits = wgpu::Limits::default();
    assert_eq!(
        probe_from(wgpu::Backend::BrowserWebGpu, &limits).compute,
        feature_enabled()
    );
    assert_eq!(
        probe_from(wgpu::Backend::Gl, &limits).compute,
        feature_enabled() && !cfg!(target_arch = "wasm32")
    );
    assert!(
        !probe_from(
            wgpu::Backend::Gl,
            &wgpu::Limits::downlevel_webgl2_defaults()
        )
        .compute
    );
    assert!(!probe_from(wgpu::Backend::Dx11, &limits).compute);
}

#[test]
#[cfg(feature = "compute")]
fn vulkan_with_webgpu_limits_enables_compute() {
    let caps = probe_from(wgpu::Backend::Vulkan, &wgpu::Limits::default());
    assert!(caps.compute);
    assert_eq!(compute_yes_no(caps), "yes");
}

#[test]
#[cfg(not(feature = "compute"))]
fn feature_off_disables_even_on_vulkan() {
    let caps = probe_from(wgpu::Backend::Vulkan, &wgpu::Limits::default());
    assert!(!caps.compute);
    assert_eq!(compute_yes_no(caps), "no");
    assert!(!feature_enabled());
}

#[test]
#[cfg(feature = "compute")]
fn metal_and_dx12_follow_native_compute() {
    assert!(probe_from(wgpu::Backend::Metal, &wgpu::Limits::default()).compute);
    assert!(probe_from(wgpu::Backend::Dx12, &wgpu::Limits::default()).compute);
}

#[test]
fn low_storage_textures_disables_compute() {
    let l = wgpu::Limits {
        max_storage_textures_per_shader_stage: 0,
        ..Default::default()
    };
    assert!(!probe_from(wgpu::Backend::Vulkan, &l).compute);
}

#[test]
fn gl_limits_copy_compute_only_when_supported_and_enabled() {
    let adapter = wgpu::Limits::default();
    let l = limits_for_backend(wgpu::Backend::Gl, &adapter);
    let down = wgpu::Limits::downlevel_webgl2_defaults();
    assert_eq!(
        l.max_compute_workgroup_size_x,
        if feature_enabled() && !cfg!(target_arch = "wasm32") {
            adapter.max_compute_workgroup_size_x
        } else {
            down.max_compute_workgroup_size_x
        }
    );
    assert_eq!(l.max_texture_dimension_2d, adapter.max_texture_dimension_2d);
}

#[test]
fn varying_request_does_not_exceed_reported_limit() {
    let adapter = wgpu::Limits {
        max_inter_stage_shader_variables: 8,
        ..Default::default()
    };
    let limits = limits_for_backend(wgpu::Backend::Gl, &adapter);
    assert!(limits.check_limits(&adapter));
    assert_eq!(limits.max_inter_stage_shader_variables, 8);
}

#[test]
fn gles_without_compute_accepts_requested_limits() {
    let adapter = wgpu::Limits::downlevel_webgl2_defaults();
    let limits = limits_for_backend(wgpu::Backend::Gl, &adapter);
    assert!(limits.check_limits(&adapter));
    assert_eq!(limits.max_compute_workgroups_per_dimension, 0);
    assert_eq!(limits.max_storage_buffers_per_shader_stage, 0);
}

#[test]
#[cfg(feature = "compute")]
fn vulkan_limits_for_raises_compute_from_adapter() {
    let adapter = wgpu::Limits::default();
    let l = limits_for_backend(wgpu::Backend::Vulkan, &adapter);
    assert!(l.max_compute_workgroup_size_x >= 8);
    assert!(l.max_storage_textures_per_shader_stage >= 1);
    assert_eq!(
        l.max_storage_textures_per_shader_stage,
        adapter.max_storage_textures_per_shader_stage
    );
}

#[test]
#[cfg(not(feature = "compute"))]
fn feature_off_vulkan_limits_keep_downlevel_compute() {
    let adapter = wgpu::Limits::default();
    let l = limits_for_backend(wgpu::Backend::Vulkan, &adapter);
    let down = wgpu::Limits::downlevel_defaults();
    assert_eq!(
        l.max_compute_workgroup_size_x,
        if feature_enabled() && !cfg!(target_arch = "wasm32") {
            adapter.max_compute_workgroup_size_x
        } else {
            down.max_compute_workgroup_size_x
        }
    );
    assert_eq!(l.max_texture_dimension_2d, adapter.max_texture_dimension_2d);
}
