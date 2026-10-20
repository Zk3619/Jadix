# SM5 compatibility for the experimental D3D11 HAL

Based on naga 30.0.0. In HLSL ShaderModel::V5_0 only, emit fixed SamplerState/SamplerComparisonState registers instead of D3D12 sampler heaps, and declare draw-base constants with SM5 cbuffer syntax instead of ConstantBuffer<T>. Sampler binding arrays are rejected in this mode. SM5.1+ paths retain their original lowering.

The source regression lives in examples/d3d11_probe/src/wgpu_native.rs and exercises both SM5 and SM5.1. The native 96-frame GPU oracle compiles and samples the generated SM5 shader using FXC.
