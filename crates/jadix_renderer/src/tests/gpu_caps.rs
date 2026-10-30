use super::*;

#[test]
fn gl_never_enables_compute() {
    let caps = probe_from(wgpu::Backend::Gl, &wgpu::Limits::default());
    assert!(!caps.compute);
    assert_eq!(compute_yes_no(caps), "no");
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
        max_storage_textures_per_shader_stage: 1,
        ..Default::default()
    };
    assert!(!probe_from(wgpu::Backend::Vulkan, &l).compute);
}

#[test]
fn gl_limits_for_does_not_copy_adapter_compute() {
    let adapter = wgpu::Limits::default();
    let l = limits_for_backend(wgpu::Backend::Gl, &adapter);
    let down = wgpu::Limits::downlevel_webgl2_defaults();
    assert_eq!(
        l.max_compute_workgroup_size_x,
        down.max_compute_workgroup_size_x
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
    assert!(l.max_storage_textures_per_shader_stage >= 2);
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
        down.max_compute_workgroup_size_x
    );
    assert_eq!(l.max_texture_dimension_2d, adapter.max_texture_dimension_2d);
}
