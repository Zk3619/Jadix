# Experimental native D3D11 graphics HAL

Based on wgpu-hal 30.0.0, alongside the prior ANGLE modifications. Opt in with Jadix feature backend-dx11 and JADIX_GPU_BACKEND=dx11. The facade constructs a wgpu instance with Instance::from_hal::<dx11::Api>; generic wgpu-core dynamic dispatch owns validation, resources, command buffers and surface lifetimes. It is identified as Backend::Dx11, never Gl/Dx12. Ordinary wgpu::Instance::new does not automatically enumerate it.

Implemented: hardware DXGI adapter identity, D3D11 immediate-context serialization, stable host staging, default GPU buffers with partial nonconstant uploads, dynamic uniform offsets through Context1, read-only raw storage SRVs, 2D/array texture views, samplers, vertex/index/instance input, Naga/FXC SM5 graphics pipelines, command retention/replay, event-query fences, flip-discard opaque Win32 surfaces, sRGB views, resize, copies and pitched staging readback. A reserved b13 holds nonzero draw bases for shaders using vertex/instance builtins. Buffer writes maintain a CPU shadow because shader buffer writes are not supported.

This is a graphics subset. Compute, shader-writable storage, depth/stencil, MSAA, compressed/3D/cube textures, indirect draws, timestamps, ray tracing, binding arrays, external textures, pipeline caches and transparent composition are not implemented. Optional Jadix GPU path pipelines use the existing CPU fallback. Unsupported operations fail rather than returning fake GPU work. Do not treat this as a complete WebGPU implementation or change the default backend until broader compatibility is covered.

Regression commands from the repository root:

- cargo test --locked -p d3d11_probe --features native-wgpu sm5_register_tests
- cargo test --locked -p d3d11_probe --features native-wgpu native_wgpu_resource_and_window_oracle -- --ignored --nocapture
- Set JADIX_UPLOAD_BACKEND=dx11 and run the Jadix scene_submission tests explicitly with backend-dx11 enabled.

The hardware oracle covers four composited window colors, swapchain resize, the real Jadix SDF polygon clip, exact triangle pixels, and 96 frames combining dynamic uniforms, partial writes, sampled textures, storage, indexed instancing, nonzero instance base, 19x13 and 23x17 pitched readback. Native HWND GDI backing pixels do not represent flip-model output; the visible oracle samples the composited screen.

Uniform buffers use dynamic WRITE_DISCARD uploads from the full CPU shadow so partial queue writes retain untouched bytes. Pipeline clones share the last draw-base values; repeated values skip mapping, and each pipeline bind restores its reserved b13 binding. The 96-frame oracle includes changed bases followed by repeated draws.
Binding maps, bind-group entries and vertex strides share immutable allocations during command recording. Resource binding honors each layout entry's declared vertex/fragment visibility.
