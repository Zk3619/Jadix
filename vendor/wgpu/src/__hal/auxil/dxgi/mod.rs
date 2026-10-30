// Every module here is gated behind `dx12` except `hdr` (the
// `DXGI_OUTPUT_DESC1` -> `DisplayHdrInfo` mapping), which the
// Vulkan-on-Windows backend also uses.
#[cfg(jadix_wgpu_hal_dx12)]
pub mod conv;
#[cfg(jadix_wgpu_hal_dx12)]
pub mod exception;
#[cfg(jadix_wgpu_hal_dx12)]
pub mod factory;
pub mod hdr;
#[cfg(jadix_wgpu_hal_dx12)]
pub mod name;
#[cfg(jadix_wgpu_hal_dx12)]
pub mod result;
#[cfg(jadix_wgpu_hal_dx12)]
pub mod time;
