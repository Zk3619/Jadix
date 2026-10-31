#[cfg(jadix_wgpu_webgpu)]
pub mod webgpu;
#[cfg(jadix_wgpu_webgpu)]
pub(crate) use webgpu::{get_browser_gpu_property, ContextWebGpu};

#[cfg(jadix_wgpu_wgpu_core)]
pub mod wgpu_core;

#[cfg(jadix_wgpu_wgpu_core)]
pub(crate) use self::wgpu_core::ContextWgpuCore;

#[cfg(jadix_wgpu_custom)]
pub mod custom;
