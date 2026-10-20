//! Resolve the renderer-reported GPU name to one DXGI adapter in this boot.
//! A system inventory alone is never evidence of the device actually selected.
#[cfg(windows)]
pub fn dxgi_identity(renderer_name: &str) -> serde_json::Value {
    use windows::Win32::Graphics::Dxgi::{
        CreateDXGIFactory1, IDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE,
    };
    let factory: IDXGIFactory1 = unsafe { CreateDXGIFactory1() }.expect("DXGI factory");
    let Some((_, suffix)) = renderer_name.split_once(" [dxgi-luid=") else {
        // An untagged name cannot distinguish duplicate DXGI adapters.
        return serde_json::Value::Null;
    };
    let luid = suffix.trim_end_matches(']');
    let mut matches = Vec::new();
    for index in 0.. {
        let Ok(adapter) = (unsafe { factory.EnumAdapters1(index) }) else {
            break;
        };
        let desc = unsafe { adapter.GetDesc1() }.expect("DXGI adapter description");
        let name = String::from_utf16_lossy(&desc.Description)
            .trim_end_matches('\0')
            .to_owned();
        let actual_luid = format!("{}:{}", desc.AdapterLuid.HighPart, desc.AdapterLuid.LowPart);
        if !name.is_empty() && renderer_name.contains(&name) && luid == actual_luid {
            assert_eq!(
                desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32,
                0,
                "software renderer"
            );
            matches.push(serde_json::json!({"name": name, "vendor_id": desc.VendorId,
                "device_id": desc.DeviceId,
                "adapter_luid": [i64::from(desc.AdapterLuid.HighPart), i64::from(desc.AdapterLuid.LowPart)]}));
        }
    }
    assert_eq!(
        matches.len(),
        1,
        "renderer name must identify one DXGI device: {renderer_name}"
    );
    matches.pop().unwrap()
}

#[cfg(not(windows))]
pub fn dxgi_identity(_: &str) -> serde_json::Value {
    serde_json::Value::Null
}
