//! Read the actual D3D11 device owned by the selected EGLDisplay.
pub fn selected_device(adapter: &wgpu::Adapter) -> serde_json::Value {
    use std::ffi::c_void;
    use windows::{
        core::Interface,
        Win32::Graphics::{Direct3D11::ID3D11Device, Dxgi::IDXGIDevice},
    };
    type Query = unsafe extern "system" fn(*mut c_void, i32, *mut usize) -> u32;
    let hal = unsafe { adapter.as_hal::<wgpu::hal::gles::Api>() }.expect("ANGLE GLES adapter");
    let context = hal.adapter_context();
    let egl = context.egl_instance().expect("owned EGL instance");
    let display = context.raw_display().expect("owned EGL display");
    // Khronos EGL_EXT_device_query + EGL_ANGLE_device_d3d. Values and signatures
    // are from EGL-Registry/api/EGL/eglext.h; neither query transfers ownership.
    let query_display: Query = unsafe {
        std::mem::transmute(
            egl.get_proc_address("eglQueryDisplayAttribEXT")
                .expect("EGL_EXT_device_query"),
        )
    };
    let query_device: Query = unsafe {
        std::mem::transmute(
            egl.get_proc_address("eglQueryDeviceAttribEXT")
                .expect("EGL_ANGLE_device_d3d"),
        )
    };
    let (mut egl_device, mut d3d_device) = (0, 0);
    assert_ne!(
        unsafe { query_display(display.as_ptr(), 0x322C, &mut egl_device) },
        0,
        "query EGL device"
    );
    assert_ne!(
        unsafe { query_device(egl_device as *mut c_void, 0x33A1, &mut d3d_device) },
        0,
        "query D3D11 device"
    );
    let raw_device = d3d_device as *mut c_void;
    let device =
        unsafe { ID3D11Device::from_raw_borrowed(&raw_device) }.expect("D3D11 device pointer");
    let dxgi: IDXGIDevice = device.cast().expect("D3D11 DXGI device");
    let desc = unsafe { dxgi.GetAdapter().and_then(|a| a.GetDesc()) }.expect("actual DXGI adapter");
    serde_json::json!({"name": String::from_utf16_lossy(&desc.Description).trim_end_matches('\0'),
        "vendor_id": desc.VendorId, "device_id": desc.DeviceId,
        "adapter_luid": [i64::from(desc.AdapterLuid.HighPart), i64::from(desc.AdapterLuid.LowPart)]})
}
