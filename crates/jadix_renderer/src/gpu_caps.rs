//! Adapter probe + device limit bands for optional wgpu compute (S7).
//!
//! GLES / WebGL stay fragment/CPU. Vulkan / DX12 / Metal may enable compute when
//! adapter limits clear the floor. Never request more than the adapter reports.
//! Does not change ADR-3 backend order.

/// GPU capability snapshot (one window / one device).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GpuCaps {
    pub backend: wgpu::Backend,
    pub compute: bool,
}

impl GpuCaps {
    pub fn none() -> Self {
        Self {
            backend: wgpu::Backend::Gl,
            compute: false,
        }
    }
}

pub const fn feature_enabled() -> bool {
    cfg!(feature = "compute")
}

pub fn native_compute_backend(backend: wgpu::Backend) -> bool {
    matches!(
        backend,
        wgpu::Backend::Vulkan | wgpu::Backend::Dx12 | wgpu::Backend::Metal
    )
}

pub fn probe(adapter: &wgpu::Adapter) -> GpuCaps {
    probe_from(adapter.get_info().backend, &adapter.limits())
}

pub fn probe_from(backend: wgpu::Backend, l: &wgpu::Limits) -> GpuCaps {
    if !feature_enabled() {
        return GpuCaps {
            backend,
            compute: false,
        };
    }
    let compute = native_compute_backend(backend)
        && l.max_compute_workgroup_size_x >= 8
        && l.max_compute_workgroup_size_y >= 8
        && l.max_compute_invocations_per_workgroup >= 64
        && l.max_compute_workgroups_per_dimension >= 65535
        && l.max_storage_buffers_per_shader_stage >= 4
        && l.max_storage_textures_per_shader_stage >= 2;
    GpuCaps { backend, compute }
}

pub fn limits_for(adapter: &wgpu::Adapter) -> wgpu::Limits {
    limits_for_backend(adapter.get_info().backend, &adapter.limits())
}

/// Shared by the window device, glass compositor, and offscreen goldens.
pub fn limits_for_backend(backend: wgpu::Backend, adapter: &wgpu::Limits) -> wgpu::Limits {
    let mut l = if backend == wgpu::Backend::Gl {
        wgpu::Limits::downlevel_webgl2_defaults()
    } else {
        wgpu::Limits::downlevel_defaults()
    };
    l.max_texture_dimension_1d = adapter.max_texture_dimension_1d;
    l.max_texture_dimension_2d = adapter.max_texture_dimension_2d;
    l.max_texture_dimension_3d = adapter
        .max_texture_dimension_3d
        .min(l.max_texture_dimension_3d.max(2048));
    l.max_inter_stage_shader_variables = adapter.max_inter_stage_shader_variables;
    if backend == wgpu::Backend::Dx11 {
        l.max_compute_workgroup_size_x = 0;
        l.max_compute_workgroup_size_y = 0;
        l.max_compute_workgroup_size_z = 0;
        l.max_compute_invocations_per_workgroup = 0;
        l.max_compute_workgroups_per_dimension = 0;
    }
    if feature_enabled() && native_compute_backend(backend) {
        l.max_compute_workgroup_size_x = adapter.max_compute_workgroup_size_x;
        l.max_compute_workgroup_size_y = adapter.max_compute_workgroup_size_y;
        l.max_compute_workgroup_size_z = adapter.max_compute_workgroup_size_z;
        l.max_compute_invocations_per_workgroup = adapter.max_compute_invocations_per_workgroup;
        l.max_compute_workgroups_per_dimension = adapter.max_compute_workgroups_per_dimension;
        l.max_compute_workgroup_storage_size = adapter.max_compute_workgroup_storage_size;
        l.max_storage_buffers_per_shader_stage = adapter.max_storage_buffers_per_shader_stage;
        l.max_storage_textures_per_shader_stage = adapter.max_storage_textures_per_shader_stage;
        l.max_storage_buffer_binding_size = adapter.max_storage_buffer_binding_size;
        l.max_dynamic_storage_buffers_per_pipeline_layout =
            adapter.max_dynamic_storage_buffers_per_pipeline_layout;
    }
    l
}

pub fn compute_yes_no(caps: GpuCaps) -> &'static str {
    if caps.compute {
        "yes"
    } else {
        "no"
    }
}

#[cfg(test)]
#[path = "tests/gpu_caps.rs"]
mod tests;
