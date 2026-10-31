use windows::Win32::{Foundation, Graphics::Dxgi};

pub(crate) trait HResult<O> {
    fn into_device_result(self, description: &str) -> Result<O, crate::__hal::DeviceError>;
}
impl<T> HResult<T> for windows::core::Result<T> {
    fn into_device_result(self, description: &str) -> Result<T, crate::__hal::DeviceError> {
        #![allow(unreachable_code)]

        self.map_err(|err| {
            log::error!("{description} failed: {err}");

            match err.code() {
                Foundation::E_OUTOFMEMORY => crate::__hal::DeviceError::OutOfMemory,
                Dxgi::DXGI_ERROR_DEVICE_RESET | Dxgi::DXGI_ERROR_DEVICE_REMOVED => {
                    #[cfg(jadix_wgpu_hal_feature_device_lost_panic)]
                    panic!("{description} failed: Device lost ({err})");
                    crate::__hal::DeviceError::Lost
                }
                _ => {
                    #[cfg(jadix_wgpu_hal_feature_internal_error_panic)]
                    panic!("{description} failed: {err}");
                    crate::__hal::DeviceError::Unexpected
                }
            }
        })
    }
}
