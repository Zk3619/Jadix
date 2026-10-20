# Benchmark GPU identity only

Base: crates.io `gpui-pre-windows` 0.3.9, Apache-2.0. Optional feature
`benchmark-gpu-identity` appends `[dxgi-luid=high:low]` to `GpuSpecs.device_name`,
using the descriptor of this renderer's actual `devices.adapter`.

The standalone comparison enables it to distinguish same-name GPUs. No draw,
atlas, shader, scheduling, swapchain or Fast caching behavior changes. The
feature is off by default; sources under `Reference` are unchanged.
