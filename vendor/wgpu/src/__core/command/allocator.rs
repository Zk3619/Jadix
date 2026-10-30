use alloc::{boxed::Box, vec::Vec};

use crate::__core::lock::{rank, Mutex};

/// A pool of free [`wgpu::hal::CommandEncoder`]s, owned by a `Device`.
///
/// Each encoder in this list is in the "closed" state.
///
/// Since a raw [`CommandEncoder`][ce] is itself a pool for allocating
/// raw [`CommandBuffer`][cb]s, this is a pool of pools.
///
/// [`wgpu::hal::CommandEncoder`]: hal::CommandEncoder
/// [ce]: hal::CommandEncoder
/// [cb]: hal::Api::CommandBuffer
pub(crate) struct CommandAllocator {
    free_encoders: Mutex<Vec<Box<dyn crate::__hal::DynCommandEncoder>>>,
}

impl CommandAllocator {
    pub(crate) fn new() -> Self {
        Self {
            free_encoders: Mutex::new(rank::COMMAND_ALLOCATOR_FREE_ENCODERS, Vec::new()),
        }
    }

    /// Return a fresh [`wgpu::hal::CommandEncoder`] in the "closed" state.
    ///
    /// If we have free encoders in the pool, take one of those. Otherwise,
    /// create a new one on `device`.
    ///
    /// [`wgpu::hal::CommandEncoder`]: hal::CommandEncoder
    pub(crate) fn acquire_encoder(
        &self,
        device: &dyn crate::__hal::DynDevice,
        queue: &dyn crate::__hal::DynQueue,
    ) -> Result<Box<dyn crate::__hal::DynCommandEncoder>, crate::__hal::DeviceError> {
        let mut free_encoders = self.free_encoders.lock();
        match free_encoders.pop() {
            Some(encoder) => Ok(encoder),
            None => unsafe {
                let hal_desc = crate::__hal::CommandEncoderDescriptor { label: None, queue };
                device.create_command_encoder(&hal_desc)
            },
        }
    }

    /// Add `encoder` back to the free pool.
    pub(crate) fn release_encoder(&self, encoder: Box<dyn crate::__hal::DynCommandEncoder>) {
        let mut free_encoders = self.free_encoders.lock();
        free_encoders.push(encoder);
    }
}
