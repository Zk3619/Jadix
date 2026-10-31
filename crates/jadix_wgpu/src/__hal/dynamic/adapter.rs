use alloc::boxed::Box;

use crate::__hal::{
    Adapter, Api, DeviceError, OpenDevice, SurfaceCapabilities, TextureFormatCapabilities,
};

use super::{DynDevice, DynQueue, DynResource, DynResourceExt, DynSurface};

#[expect(missing_debug_implementations, reason = "dyn")]
pub struct DynOpenDevice {
    pub device: Box<dyn DynDevice>,
    pub queue: Box<dyn DynQueue>,
}

impl<A: Api> From<OpenDevice<A>> for DynOpenDevice {
    fn from(open_device: OpenDevice<A>) -> Self {
        Self {
            device: Box::new(open_device.device),
            queue: Box::new(open_device.queue),
        }
    }
}

pub trait DynAdapter: DynResource {
    unsafe fn open(
        &self,
        features: crate::__types::Features,
        limits: &crate::__types::Limits,
        memory_hints: &crate::__types::MemoryHints,
    ) -> Result<DynOpenDevice, DeviceError>;

    unsafe fn texture_format_capabilities(
        &self,
        format: crate::__types::TextureFormat,
    ) -> TextureFormatCapabilities;

    unsafe fn surface_capabilities(&self, surface: &dyn DynSurface) -> Option<SurfaceCapabilities>;

    unsafe fn surface_display_hdr_info(
        &self,
        surface: &dyn DynSurface,
    ) -> Option<crate::__types::DisplayHdrInfo>;

    unsafe fn get_presentation_timestamp(&self) -> crate::__types::PresentationTimestamp;

    fn get_ordered_buffer_usages(&self) -> crate::__types::BufferUses;

    fn get_ordered_texture_usages(&self) -> crate::__types::TextureUses;
}

impl<A: Adapter + DynResource> DynAdapter for A {
    unsafe fn open(
        &self,
        features: crate::__types::Features,
        limits: &crate::__types::Limits,
        memory_hints: &crate::__types::MemoryHints,
    ) -> Result<DynOpenDevice, DeviceError> {
        unsafe { A::open(self, features, limits, memory_hints) }.map(|open_device| DynOpenDevice {
            device: Box::new(open_device.device),
            queue: Box::new(open_device.queue),
        })
    }

    unsafe fn texture_format_capabilities(
        &self,
        format: crate::__types::TextureFormat,
    ) -> TextureFormatCapabilities {
        unsafe { A::texture_format_capabilities(self, format) }
    }

    unsafe fn surface_capabilities(&self, surface: &dyn DynSurface) -> Option<SurfaceCapabilities> {
        let surface = surface.expect_downcast_ref();
        unsafe { A::surface_capabilities(self, surface) }
    }

    unsafe fn surface_display_hdr_info(
        &self,
        surface: &dyn DynSurface,
    ) -> Option<crate::__types::DisplayHdrInfo> {
        let surface = surface.expect_downcast_ref();
        unsafe { A::surface_display_hdr_info(self, surface) }
    }

    unsafe fn get_presentation_timestamp(&self) -> crate::__types::PresentationTimestamp {
        unsafe { A::get_presentation_timestamp(self) }
    }

    fn get_ordered_buffer_usages(&self) -> crate::__types::BufferUses {
        A::get_ordered_buffer_usages(self)
    }

    fn get_ordered_texture_usages(&self) -> crate::__types::TextureUses {
        A::get_ordered_texture_usages(self)
    }
}
