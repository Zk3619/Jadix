# jadix_3d

glTF/GLB/VRM loading, scene hierarchy, skeletal/morph animation and offscreen wgpu
rendering. Independent of Jadix UI and native hosts; no jadix_native dependency.
The crate is excluded from workspace default-members and jadix default features.
Explicit --workspace builds still include it. Enable jadix/scene3d-models for UI integration.

## APIs

- Scene::load / from_slice: GLB, glTF buffers and images, named scenes, cameras,
  punctual lights, skinning, morph targets and animation tracks.
- Scene::pose: STEP/LINEAR/CUBICSPLINE sampling. LocalPose and Animator add
  play/pause, seeking, speed/repeat, blending and interruptible crossfades.
- Scene::humanoid_bone, apply_expression and solve_ik: VRM 0/1 humanoid lookup,
  morph expressions and bounded CCD inverse kinematics.
- Scene::bounds / raycast, Camera::ray and OrbitCamera: animated bounds, nearest
  deformed-triangle picking and aspect-aware orbit framing.
- geometry: plane, cuboid and UV sphere meshes for procedural scenes.
- Renderer: host-owned wgpu device/target, depth, alpha mask/blend, double-sided
  and unlit materials, basic GGX lighting, base-color, metallic/roughness, normal,
  occlusion and emissive textures with independent samplers and color spaces.

The Jadix scene3d-models feature connects Renderer to scene3d().model(&SceneView).
Keep SceneView alive for the lifetime of its viewport. See
[the capability guide](../../docs/desktop-capabilities.md) for examples and limitations.

## Validation

~~~powershell
cargo run -p jadix_3d --example inspect -- path/to/model.glb
cargo test -p jadix_3d --features gpu-tests -- --nocapture
cargo run -p jadix --features scene3d-models --example model_scene -- path/to/avatar.vrm
~~~

GPU tests require a native adapter; they read back pixels for skinning, textures,
depth, PBR maps and color spaces. CPU tests cover scene/animation input validation,
controllers, crossfades, expressions, inverse kinematics, bounds and picking.

## Limits

CPU skinning/morph evaluation; at most eight punctual lights. UV0 only, up to four
joint influences per vertex, triangle geometry. No shadows, IBL, mipmap generation,
GPU skinning, MToon, spring-bone physics or retargeting. VRM expression support covers
morph binds, not material/texture-transform binds. CCD under nonuniform scales is
approximate. This is not a complete implementation of the VRM rendering specification.
