# jadix_live2d

Opt-in Cubism-compatible playback, independent of jadix_native. Its renderer uses the workspace jadix_wgpu device. Neither jadix/live2d nor jadix/scene3d-models is enabled by default.

## Frame loop

1. Load Player::load("model/avatar.model3.json"). It loads the moc3, PNG textures, physics3, pose3, and declared motions and expressions.
2. Select Player::play_motion(group, index, looping) and set_expression(name). Set look direction, blink/breath controls and a host-provided mouth amplitude. Automatic blink/breath provide a base; authored motion curves take precedence, followed by look/mouth, expression, physics/pose and explicit parameter overrides.
3. Call update(delta_seconds). Parameter overrides survive frame updates. The host supplies audio playback and the frame clock.
4. Create render::wgpu::WgpuLive2dRenderer once per output format, and GpuModel::new once per model. After each update, call GpuModel::update and GpuModel::encode into the host command encoder. Submit that encoder normally.

GpuModel retains textures and buffers, updates masks, and supports normal/additive/multiplicative blending and inverted clipping. View::fit creates an aspect-preserving transform and maps physical top-left-origin pointer coordinates back to model space for runtime.hit_test.

The crate re-exports assets, core, moc3, motion, expression, json and runtime for custom controllers or renderers. Direct use with default-features = false provides a CPU-only runtime. Enabling jadix/live2d makes it available as jadix::live2d.

## Compatibility boundary

The backend is independent pure-Rust Mocari 0.4, not the official Live2D Cubism Core. Upstream describes it as experimental. This crate does **not** claim complete Cubism SDK or model-version compatibility. It supports the model data/deformation, motion/expression, pose/physics and drawable features implemented by that runtime. New Cubism rendering features, full SDK behavioral parity and an official Core backend remain unverified. Audio capture, decoding and playback belong to the host.

No model assets or proprietary binaries are bundled. The MIT license for adapted renderer code is in THIRD_PARTY_LICENSE_MOCARI. Applications must obtain their own model/SDK permissions.

## Checks

- cargo check -p jadix_live2d
- cargo check -p jadix_live2d --no-default-features
- cargo check -p jadix --no-default-features --features live2d
- cargo test -p jadix_live2d --features gpu-tests

The crate is excluded from workspace default-members; explicit --workspace builds still include it.

## Licensed models and examples

~~~powershell
cargo run -p jadix_live2d --example inspect -- path/to/avatar.model3.json
cargo run -p jadix_live2d --features native-backends --example render_model -- path/to/avatar.model3.json avatar.ppm 2 Idle 0
$env:JADIX_LIVE2D_MODEL = 'path/to/avatar.model3.json'
cargo test -p jadix_live2d --test real_model -- --ignored --nocapture
~~~

inspect loads the model and simulates 60 frames. render_model writes a 512x512 PPM
with an opaque dark background using a real GPU; seconds, motion group and index
are optional. native-backends enables native wgpu backends explicitly.

The real-model test is explicitly ignored by default. JADIX_LIVE2D_MODEL must point
to a licensed model3 file when running --ignored. It exercises declared motions,
expressions, parameter controls, mesh validity and hit testing; it does not prove
full Cubism compatibility or replace visual validation of a real model.
No licensed-model run has been performed as part of the synthetic GPU tests.
