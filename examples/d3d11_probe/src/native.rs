use windows::{
    core::{s, PCSTR},
    Win32::{
        Foundation::{HMODULE, RECT},
        Graphics::{
            Direct3D::{
                Fxc::D3DCompile, ID3DBlob, D3D_DRIVER_TYPE_UNKNOWN, D3D_FEATURE_LEVEL,
                D3D_FEATURE_LEVEL_11_0, D3D_FEATURE_LEVEL_11_1,
                D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST,
            },
            Direct3D11::*,
            Dxgi::{Common::*, CreateDXGIFactory1, IDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE},
        },
    },
};

const HLSL: &str = "
float4 vs_main(uint id : SV_VertexID) : SV_Position {
    float2 p = float2((id << 1) & 2, id & 2);
    return float4(p.x * 2 - 1, 1 - p.y * 2, 0, 1);
}
float4 ps_main() : SV_Target { return float4(0.25, 0.5, 0.75, 1); }
";

#[cfg(all(test, feature = "native-hal"))]
#[test]
fn native_compiler_rejects_missing_entries_and_unimplemented_bindings() {
    let source = "@group(0) @binding(0) var<uniform> value: vec4f;
        @fragment fn main() -> @location(0) vec4f { return value; }";
    let module = wgpu::naga::front::wgsl::parse_str(source).unwrap();
    let info = wgpu::naga::valid::Validator::new(
        wgpu::naga::valid::ValidationFlags::all(),
        wgpu::naga::valid::Capabilities::empty(),
    )
    .validate(&module)
    .unwrap();
    assert!(matches!(
        wgpu::hal::dx11::compile_shader(&module, &info, wgpu::naga::ShaderStage::Fragment, "main"),
        Err(wgpu::hal::dx11::Error::Unsupported(
            "shader resource register layout is not implemented"
        ))
    ));
    let module = wgpu::naga::front::wgsl::parse_str(
        "@fragment fn main() -> @location(0) vec4f { return vec4f(1.); }",
    )
    .unwrap();
    let info = wgpu::naga::valid::Validator::new(
        wgpu::naga::valid::ValidationFlags::all(),
        wgpu::naga::valid::Capabilities::empty(),
    )
    .validate(&module)
    .unwrap();
    assert!(wgpu::hal::dx11::compile_shader(
        &module,
        &info,
        wgpu::naga::ShaderStage::Fragment,
        "missing"
    )
    .is_err());
    assert!(wgpu::hal::dx11::compile_shader(
        &module,
        &info,
        wgpu::naga::ShaderStage::Vertex,
        "main"
    )
    .is_err());
    assert!(!wgpu::hal::dx11::compile_shader(
        &module,
        &info,
        wgpu::naga::ShaderStage::Fragment,
        "main"
    )
    .unwrap()
    .is_empty());
}

unsafe fn compile(entry: PCSTR, target: PCSTR) -> Result<ID3DBlob, Box<dyn std::error::Error>> {
    let (mut code, mut errors) = (None, None);
    if let Err(error) = D3DCompile(
        HLSL.as_ptr().cast(),
        HLSL.len(),
        PCSTR::null(),
        None,
        None,
        entry,
        target,
        0,
        0,
        &mut code,
        Some(&mut errors),
    ) {
        if let Some(errors) = errors {
            eprintln!(
                "{}",
                String::from_utf8_lossy(std::slice::from_raw_parts(
                    errors.GetBufferPointer().cast(),
                    errors.GetBufferSize()
                ))
            );
        }
        return Err(error.into());
    }
    code.ok_or_else(|| "D3DCompile returned no bytecode".into())
}

pub fn run(use_hal: bool) -> Result<(serde_json::Value, Vec<u8>), Box<dyn std::error::Error>> {
    #[cfg(feature = "native-hal")]
    if use_hal {
        let mut failures = Vec::new();
        for adapter in wgpu::hal::dx11::enumerate_adapters()? {
            match adapter.open() {
                Ok(device) => {
                    return native_hal::render(&adapter.description, &device);
                }
                Err(error) => failures.push(error.to_string()),
            }
        }
        return Err(format!("no hardware D3D11 device: {failures:?}").into());
    }
    #[cfg(not(feature = "native-hal"))]
    assert!(!use_hal, "requires native-hal feature");
    unsafe {
        let factory: IDXGIFactory1 = CreateDXGIFactory1()?;
        // Match published GPUI's first usable hardware adapter policy.
        let mut selected = None;
        for index in 0.. {
            let Ok(adapter) = factory.EnumAdapters1(index) else {
                break;
            };
            let desc = adapter.GetDesc1()?;
            if desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 != 0 {
                continue;
            }
            let (mut device, mut context, mut level) = (None, None, D3D_FEATURE_LEVEL::default());
            if D3D11CreateDevice(
                &adapter,
                D3D_DRIVER_TYPE_UNKNOWN,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                Some(&[D3D_FEATURE_LEVEL_11_1, D3D_FEATURE_LEVEL_11_0]),
                D3D11_SDK_VERSION,
                Some(&mut device),
                Some(&mut level),
                Some(&mut context),
            )
            .is_ok()
            {
                selected = Some((desc, device.unwrap(), context.unwrap(), level));
                break;
            }
        }
        let (desc, device, context, level) = selected.ok_or("no hardware D3D11 device")?;
        render(&desc, &device, &context, level)
    }
}

#[cfg(feature = "native-hal")]
#[path = "native_hal.rs"]
mod native_hal;

fn render(
    desc: &windows::Win32::Graphics::Dxgi::DXGI_ADAPTER_DESC1,
    device: &ID3D11Device,
    context: &ID3D11DeviceContext,
    level: D3D_FEATURE_LEVEL,
) -> Result<(serde_json::Value, Vec<u8>), Box<dyn std::error::Error>> {
    unsafe {
        let texture_desc = D3D11_TEXTURE2D_DESC {
            Width: super::SIZE as u32,
            Height: super::SIZE as u32,
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT_R8G8B8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: D3D11_BIND_RENDER_TARGET.0 as u32,
            ..Default::default()
        };
        let mut target = None;
        device.CreateTexture2D(&texture_desc, None, Some(&mut target))?;
        let target = target.unwrap();
        let mut view = None;
        device.CreateRenderTargetView(&target, None, Some(&mut view))?;
        let view = view.unwrap();
        let compile_stage = |entry: PCSTR,
                             profile: PCSTR|
         -> Result<Vec<u8>, Box<dyn std::error::Error>> {
            let blob = compile(entry, profile)?;
            Ok(
                std::slice::from_raw_parts(blob.GetBufferPointer().cast(), blob.GetBufferSize())
                    .to_vec(),
            )
        };
        let vs = compile_stage(s!("vs_main"), s!("vs_5_0"))?;
        let ps = compile_stage(s!("ps_main"), s!("ps_5_0"))?;
        let (mut vertex, mut fragment) = (None, None);
        device.CreateVertexShader(&vs, None, Some(&mut vertex))?;
        device.CreatePixelShader(&ps, None, Some(&mut fragment))?;
        let mut rasterizer = None;
        device.CreateRasterizerState(
            &D3D11_RASTERIZER_DESC {
                FillMode: D3D11_FILL_SOLID,
                CullMode: D3D11_CULL_NONE,
                ScissorEnable: true.into(),
                DepthClipEnable: true.into(),
                ..Default::default()
            },
            Some(&mut rasterizer),
        )?;
        context.RSSetState(rasterizer.as_ref());
        context.RSSetViewports(Some(&[D3D11_VIEWPORT {
            Width: super::SIZE as f32,
            Height: super::SIZE as f32,
            MaxDepth: 1.,
            ..Default::default()
        }]));
        context.RSSetScissorRects(Some(&[RECT {
            left: 0,
            top: 0,
            right: 8,
            bottom: 8,
        }]));
        context.OMSetRenderTargets(Some(&[Some(view.clone())]), None);
        context.ClearRenderTargetView(&view, &[0., 0., 0., 1.]);
        context.VSSetShader(vertex.as_ref(), None);
        context.PSSetShader(fragment.as_ref(), None);
        context.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
        context.Draw(3, 0);
        let mut staging = None;
        device.CreateTexture2D(
            &D3D11_TEXTURE2D_DESC {
                Usage: D3D11_USAGE_STAGING,
                BindFlags: 0,
                CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
                ..texture_desc
            },
            None,
            Some(&mut staging),
        )?;
        let staging = staging.unwrap();
        context.CopyResource(&staging, &target);
        let mut mapping = D3D11_MAPPED_SUBRESOURCE::default();
        context.Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapping))?;
        let mut pixels = Vec::with_capacity(super::SIZE * super::SIZE * 4);
        for y in 0..super::SIZE {
            pixels.extend_from_slice(std::slice::from_raw_parts(
                mapping
                    .pData
                    .cast::<u8>()
                    .add(y * mapping.RowPitch as usize),
                super::SIZE * 4,
            ));
        }
        context.Unmap(&staging, 0);
        context.ClearState();
        context.Flush();
        Ok((
            serde_json::json!({
                "api": "D3D11", "implementation": "native-diagnostic",
                "device_name": String::from_utf16_lossy(&desc.Description).trim_end_matches('\0'),
                "vendor_id": desc.VendorId, "device_id": desc.DeviceId,
            "dxgi": {"adapter_luid": [i64::from(desc.AdapterLuid.HighPart), i64::from(desc.AdapterLuid.LowPart)],
                "vendor_id": desc.VendorId, "device_id": desc.DeviceId},
                "feature_level": level.0, "wgpu_hal_backend": false,
            }),
            pixels,
        ))
    }
}
