# Optional Windows ANGLE D3D11 transport

Base: crates.io `wgpu-hal` 30.0.0, MIT OR Apache-2.0. All upstream sources and
licenses are retained. The workspace patches this package; default features do
not enable this transport.

Status: **experimental**. ANGLE window color, resize and sustained presentation
pass the recorded probes. Native D3D11 has an offscreen resource/draw path; a
registered wgpu backend and native UI presentation remain outstanding.

`gles-angle` selects the existing EGL implementation instead of WGL on Windows.
It requires an absolute `WGPU_ANGLE_LIBRARY` path to `libEGL.dll` and loads sibling
dependencies with `LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR`. It explicitly selects
`EGL_PLATFORM_ANGLE_TYPE_D3D11_ANGLE` and hardware devices, and fails if the
extension/platform is unavailable. It never requests ANGLE's default backend,
Vulkan, D3D9 or WARP. Wayland types/calls are excluded from Windows compilation.

wgpu's public backend identity remains `Gl`: this is WGSL -> GLSL -> ANGLE ->
D3D11, **not a native wgpu-hal D3D11 backend**. When the feature is enabled, all
Windows GL instances in that build use EGL/ANGLE. Build without the feature for
the original WGL implementation. DX12 is unchanged.

Jadix opts in with `backend-dx11-angle` and explicitly selects it with
`JADIX_GPU_BACKEND=dx11-angle`. That request disables fallback and checks the
observed renderer. Benchmark GPU identity comes from the selected renderer and
must resolve to one DXGI adapter/LUID; merely loading a D3D11 DLL is insufficient.

The optional `dx11` feature exposes native bring-up in `src/dx11`: exact hardware
adapter/device selection, a serialized immediate context, and validated Naga to
HLSL SM5.0/FXC compilation. Explicit uniform bindings map to constant-buffer
registers; other resource layouts are rejected. The native offscreen path owns
RGBA8 targets, initialized dynamic constant buffers, WRITE_DISCARD updates,
procedural triangle pipelines, scissored draws, region copies and pitch-aware
staging readback. Commands validate device ownership, binding sizes and ranges.
It does not yet implement `crate::Api`, register a wgpu backend, or provide
texture/storage bindings, vertex/index buffers or a DXGI surface. `d3d11_probe
native-hal` now uses these resource/draw APIs throughout the triangle oracle;
the direct `native` diagnostic remains an independent implementation. Native
tests exercise repeated updates, clipping, copies and rejected commands.

ANGLE presentation uses a texture-backed sRGB target and explicitly encodes to
a linear EGL window surface. The tested ANGLE D3D11 default-framebuffer blit
decoded the source without re-encoding despite reporting an sRGB destination.
The presentation shader preserves the queue's VAO/program/texture/sampler state.
GLES only toggles FRAMEBUFFER_SRGB when GL_EXT_sRGB_write_control is available.

`scripts/benchmarks/d3d11-verify.ps1` runs all three probes and rejects different GPUs. The
ANGLE probe includes real SDF drawing with polygon clipping and window color /
resize checks. UI correctness/performance still requires the separate native
window comparison and existing Jadix GPU regressions.

No ANGLE binaries are distributed here. A local test build may use pinned
Chrome for Testing binaries; save their source URL, version and SHA-256 outside
Cargo targets. Do not treat a different ANGLE build as the same experiment.
