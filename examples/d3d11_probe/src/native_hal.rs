use wgpu_hal::dx11::Device;

const SHADER: &str = "
@group(2) @binding(7) var<uniform> color: vec4f;
@vertex fn vs_main(@builtin(vertex_index) id: u32) -> @builtin(position) vec4f {
    let p = vec2f(f32((id << 1u) & 2u), f32(id & 2u));
    return vec4f(p.x * 2. - 1., 1. - p.y * 2., 0., 1.);
}
@fragment fn ps_main() -> @location(0) vec4f { return color; }
";

fn color_bytes(color: [f32; 4]) -> Vec<u8> {
    color.into_iter().flat_map(f32::to_ne_bytes).collect()
}

fn shader() -> (naga::Module, naga::valid::ModuleInfo) {
    let module = naga::front::wgsl::parse_str(SHADER).unwrap();
    let info = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::empty(),
    )
    .validate(&module)
    .unwrap();
    (module, info)
}

const BINDING: naga::ResourceBinding = naga::ResourceBinding {
    group: 2,
    binding: 7,
};

pub fn render(
    desc: &windows::Win32::Graphics::Dxgi::DXGI_ADAPTER_DESC1,
    device: &Device,
) -> Result<(serde_json::Value, Vec<u8>), Box<dyn std::error::Error>> {
    let (module, info) = shader();
    let pipeline =
        device.create_render_pipeline(&module, &info, "vs_main", "ps_main", &[(BINDING, 3)])?;
    let uniform = device.create_uniform_buffer(&color_bytes([0., 1., 0., 1.]))?;
    let target = device.create_render_target([super::super::SIZE as u32; 2])?;
    // A real WRITE_DISCARD update must replace the initial green before drawing.
    device.write_uniform_buffer(&uniform, &color_bytes([0.25, 0.5, 0.75, 1.]))?;
    device.draw(
        &target,
        &pipeline,
        &[(3, &uniform)],
        [0, 0, 8, 8],
        0..3,
        Some([0., 0., 0., 1.]),
    )?;
    let pixels = device.read_render_target(&target)?;
    Ok((
        serde_json::json!({
            "api": "D3D11", "implementation": "native-hal-offscreen",
            "device_name": String::from_utf16_lossy(&desc.Description).trim_end_matches('\0'),
            "vendor_id": desc.VendorId, "device_id": desc.DeviceId,
            "dxgi": {"adapter_luid": [i64::from(desc.AdapterLuid.HighPart), i64::from(desc.AdapterLuid.LowPart)],
                "vendor_id": desc.VendorId, "device_id": desc.DeviceId},
            "feature_level": device.feature_level.0, "wgpu_hal_backend": false,
            "hal_resources_and_draw": true, "uniform_write_discard": true,
        }),
        pixels,
    ))
}

#[test]
fn native_resources_update_clip_and_reject_invalid_commands() {
    let adapter = wgpu_hal::dx11::enumerate_adapters().unwrap().remove(0);
    let device = adapter.open().unwrap();
    let (module, info) = shader();
    let pipeline = device
        .create_render_pipeline(&module, &info, "vs_main", "ps_main", &[(BINDING, 3)])
        .unwrap();
    let uniform = device.create_uniform_buffer(&color_bytes([1.; 4])).unwrap();
    assert!(device.create_uniform_buffer(&[0; 15]).is_err());
    assert!(device.write_uniform_buffer(&uniform, &[0; 32]).is_err());
    assert!(device.create_render_target([0, 16]).is_err());
    assert!(device
        .create_render_pipeline(&module, &info, "vs_main", "ps_main", &[(BINDING, 14)])
        .is_err());
    assert!(device
        .create_render_pipeline(
            &module,
            &info,
            "vs_main",
            "ps_main",
            &[(BINDING, 3), (BINDING, 4)]
        )
        .is_err());
    let other = adapter.open().unwrap();
    assert!(other
        .write_uniform_buffer(&uniform, &color_bytes([0.; 4]))
        .is_err());
    let extra_output = SHADER.replace(
        "@fragment fn ps_main() -> @location(0) vec4f { return color; }",
        "struct Output { @location(0) color: vec4f, @builtin(frag_depth) depth: f32 };\n\
         @fragment fn ps_main() -> Output { return Output(color, 0.5); }",
    );
    let extra_module = naga::front::wgsl::parse_str(&extra_output).unwrap();
    let extra_info = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::empty(),
    )
    .validate(&extra_module)
    .unwrap();
    assert!(device
        .create_render_pipeline(
            &extra_module,
            &extra_info,
            "vs_main",
            "ps_main",
            &[(BINDING, 3)]
        )
        .is_err());
    for size in [[16, 16], [19, 13]] {
        let target = device.create_render_target(size).unwrap();
        assert!(device
            .read_render_target(&target)
            .unwrap()
            .iter()
            .all(|&n| n == 0));
        assert!(other.read_render_target(&target).is_err());
        for color in [[1., 0., 0., 1.], [0., 0.5, 1., 1.], [0.25, 0.5, 0.75, 1.]] {
            device
                .write_uniform_buffer(&uniform, &color_bytes(color))
                .unwrap();
            assert!(device
                .draw(&target, &pipeline, &[], [0, 0, 8, 8], 0..3, None)
                .is_err());
            assert!(device
                .draw(
                    &target,
                    &pipeline,
                    &[(3, &uniform)],
                    [u32::MAX, 0, 8, 8],
                    0..3,
                    None
                )
                .is_err());
            assert!(device
                .draw(
                    &target,
                    &pipeline,
                    &[(3, &uniform)],
                    [0, 0, 8, 8],
                    0..3,
                    Some([f32::NAN; 4])
                )
                .is_err());
            device
                .draw(
                    &target,
                    &pipeline,
                    &[(3, &uniform)],
                    [2, 3, 8, 6],
                    0..3,
                    Some([0., 0., 0., 1.]),
                )
                .unwrap();
            let pixels = device.read_render_target(&target).unwrap();
            for (i, pixel) in pixels.chunks_exact(4).enumerate() {
                let (x, y) = (i as u32 % size[0], i as u32 / size[0]);
                let expected = if (2..10).contains(&x) && (3..9).contains(&y) {
                    color.map(|n| (n * 255.).round() as u8)
                } else {
                    [0, 0, 0, 255]
                };
                assert!(
                    pixel.iter().zip(expected).all(|(&a, b)| a.abs_diff(b) <= 1),
                    "{size:?} pixel ({x},{y}): {pixel:?} != {expected:?}"
                );
            }
            let copy = device.create_render_target(size).unwrap();
            assert!(device
                .copy_render_target_region(&target, &target, [0, 0], [8, 6], [1, 1])
                .is_err());
            assert!(device
                .copy_render_target_region(&target, &copy, [u32::MAX, 0], [8, 6], [0, 0])
                .is_err());
            device
                .copy_render_target_region(&target, &copy, [2, 3], [8, 6], [4, 5])
                .unwrap();
            let copied = device.read_render_target(&copy).unwrap();
            for (i, pixel) in copied.chunks_exact(4).enumerate() {
                let (x, y) = (i as u32 % size[0], i as u32 / size[0]);
                let expected = if (4..12).contains(&x) && (5..11).contains(&y) {
                    color.map(|n| (n * 255.).round() as u8)
                } else {
                    [0; 4]
                };
                assert!(pixel.iter().zip(expected).all(|(&a, b)| a.abs_diff(b) <= 1));
            }
        }
    }
}
