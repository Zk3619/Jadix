use alloc::{boxed::Box, sync::Arc};
use core::mem::ManuallyDrop;

use crate::__types::BufferUses;

use crate::__core::device::{Device, DeviceError};
use crate::__core::{hal_label, resource_log};

#[derive(Debug)]
pub struct ScratchBuffer {
    raw: ManuallyDrop<Box<dyn crate::__hal::DynBuffer>>,
    device: Arc<Device>,
}

impl ScratchBuffer {
    pub(crate) fn new(
        device: &Arc<Device>,
        size: crate::__types::BufferSize,
    ) -> Result<Self, DeviceError> {
        let raw = unsafe {
            device
                .raw()
                .create_buffer(&crate::__hal::BufferDescriptor {
                    label: hal_label(Some("(wgpu) scratch buffer"), device.instance_flags),
                    size: size.get(),
                    usage: BufferUses::ACCELERATION_STRUCTURE_SCRATCH,
                    memory_flags: crate::__hal::MemoryFlags::empty(),
                })
                .map_err(DeviceError::from_hal)?
        };
        Ok(Self {
            raw: ManuallyDrop::new(raw),
            device: device.clone(),
        })
    }
    pub(crate) fn raw(&self) -> &dyn crate::__hal::DynBuffer {
        self.raw.as_ref()
    }
}

impl Drop for ScratchBuffer {
    fn drop(&mut self) {
        resource_log!("Destroy raw ScratchBuffer");
        // SAFETY: We are in the Drop impl and we don't use self.raw anymore after this point.
        let raw = unsafe { ManuallyDrop::take(&mut self.raw) };
        unsafe { self.device.raw().destroy_buffer(raw) };
    }
}
