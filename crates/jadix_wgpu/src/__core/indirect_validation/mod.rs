use crate::__core::{
    device::DeviceError,
    pipeline::{CreateComputePipelineError, CreateShaderModuleError},
};
use alloc::boxed::Box;
use thiserror::Error;

mod dispatch;
mod draw;
mod utils;

pub(crate) use dispatch::Dispatch;
pub(crate) use draw::{Draw, DrawBatcher, DrawResources};

#[derive(Clone, Debug, Error)]
#[non_exhaustive]
enum CreateIndirectValidationPipelineError {
    #[error(transparent)]
    DeviceError(#[from] DeviceError),
    #[error(transparent)]
    ShaderModule(#[from] CreateShaderModuleError),
    #[error(transparent)]
    ComputePipeline(#[from] CreateComputePipelineError),
}

pub(crate) struct IndirectValidation {
    pub(crate) dispatch: Dispatch,
    pub(crate) draw: Draw,
}

impl IndirectValidation {
    pub(crate) fn new(
        device: &dyn crate::__hal::DynDevice,
        required_limits: &crate::__types::Limits,
        required_features: &crate::__types::Features,
        instance_flags: crate::__types::InstanceFlags,
        backend: crate::__types::Backend,
    ) -> Result<Self, DeviceError> {
        let dispatch = match Dispatch::new(device, instance_flags, required_limits) {
            Ok(dispatch) => dispatch,
            Err(e) => {
                log::error!("indirect-validation error: {e:?}");
                return Err(DeviceError::Lost);
            }
        };
        let draw = match Draw::new(
            device,
            required_features,
            instance_flags,
            backend,
            required_limits,
        ) {
            Ok(draw) => draw,
            Err(e) => {
                log::error!("indirect-draw-validation error: {e:?}");
                return Err(DeviceError::Lost);
            }
        };
        Ok(Self { dispatch, draw })
    }

    pub(crate) fn dispose(self, device: &dyn crate::__hal::DynDevice) {
        let Self { dispatch, draw } = self;

        dispatch.dispose(device);
        draw.dispose(device);
    }
}

#[derive(Debug)]
pub(crate) struct BindGroups {
    pub(crate) dispatch: Box<dyn crate::__hal::DynBindGroup>,
    draw: Box<dyn crate::__hal::DynBindGroup>,
}

impl BindGroups {
    /// `Ok(None)` will only be returned if `buffer_size` is `0`.
    pub(crate) fn new(
        indirect_validation: &IndirectValidation,
        device: &crate::__core::device::Device,
        buffer_size: u64,
        buffer: &dyn crate::__hal::DynBuffer,
    ) -> Result<Option<Self>, DeviceError> {
        let dispatch = indirect_validation.dispatch.create_src_bind_group(
            device.raw(),
            &device.limits,
            buffer_size,
            buffer,
            device.instance_flags,
        )?;
        let draw = indirect_validation.draw.create_src_bind_group(
            device.raw(),
            &device.adapter.limits(),
            buffer_size,
            buffer,
            device.instance_flags,
        )?;

        match (dispatch, draw) {
            (None, None) => Ok(None),
            (None, Some(_)) => unreachable!(),
            (Some(_), None) => unreachable!(),
            (Some(dispatch), Some(draw)) => Ok(Some(Self { dispatch, draw })),
        }
    }

    pub(crate) fn dispose(self, device: &dyn crate::__hal::DynDevice) {
        let Self { dispatch, draw } = self;

        unsafe {
            device.destroy_bind_group(dispatch);
            device.destroy_bind_group(draw);
        }
    }
}
