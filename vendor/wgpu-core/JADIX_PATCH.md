# Native D3D11 dispatch compatibility

Based on wgpu-core 30.0.0. Handle the additional Backend::Dx11 exhaustively in the shader passthrough selector by rejecting unsupported precompiled input. Validated WGSL/Naga enters the graphics HAL through the existing generic Instance::from_hal and Dyn* dispatch. No backend is aliased to Dx12 or Gl.
