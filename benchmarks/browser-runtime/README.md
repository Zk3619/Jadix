# Actual owned browser GPU readback

This isolated workspace produces separate WASM modules for Jadix and upstream
Vello to avoid duplicated web-sys/forked wgpu bindings. The owned local HTTP
server launches a headless Edge process with a new profile, collects actual
readback JSON, then terminates only that owned process tree. Existing browser
sessions are not used. Runtime evidence is in round17/browser-runtime.json.

Prerequisites: wasm32-unknown-unknown Rust target, installed Edge, Python,
and wasm-bindgen-cli 0.2.114 at
`tools/wasm-bindgen-0.2.114-x86_64-pc-windows-msvc/wasm-bindgen.exe`.
Obtain that exact official release from
https://github.com/wasm-bindgen/wasm-bindgen/releases/tag/0.2.114 .
The tool directory, generated pkg-* bindings and target/ are build artifacts.

```powershell
python benchmarks/browser-runtime/run.py
```

Jadix's WebGPU and WebGL tests render an RGBA16F HDR brush, tagged P3 float image
and Gaussian isolated group, imports the produced texture through a second
GPU image pipeline and checks actual half-float pixels. CSS context evaluation
also compares 33 computed-color/Canvas results against Edge. WebGL uses
a COPY_SRC-only producer texture with GPU staging; WebGPU imports a directly
sampleable producer texture. WebGL uses an OffscreenCanvas-compatible adapter
and bounded event-loop polling until map completion. Vello's independent WebGPU
module tests one opaque fill.
Each operation has a finite timeout; failure is reported as failure.
These are finite runtime probes, not the native full matrix, all APIs,
all browser vendors, HDR monitor presentation, Metal or mobile validation.

Round 17 executes Jadix Area AA and opt-in automatic tight layer domains in both
GPU backends; results retain those flags. Vello only tests its own Area fill.
