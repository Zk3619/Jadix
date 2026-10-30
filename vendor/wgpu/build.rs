fn has(name: &str, expected: &str) -> bool {
    std::env::var(name).is_ok_and(|value| value.split(',').any(|item| item == expected))
}

fn main() {
    println!("cargo::rustc-check-cfg=cfg(web_sys_unstable_apis)");
    let target_panic_unwind = has("CARGO_CFG_PANIC", "unwind");
    let target_target_arch_aarch64 = has("CARGO_CFG_TARGET_ARCH", "aarch64");
    let target_target_arch_wasm32 = has("CARGO_CFG_TARGET_ARCH", "wasm32");
    let target_target_env_msvc = has("CARGO_CFG_TARGET_ENV", "msvc");
    let target_target_feature_atomics = has("CARGO_CFG_TARGET_FEATURE", "atomics");
    let target_target_has_atomic_64 = has("CARGO_CFG_TARGET_HAS_ATOMIC", "64");
    let target_target_has_atomic_ptr = has("CARGO_CFG_TARGET_HAS_ATOMIC", "ptr");
    let target_target_os_android = has("CARGO_CFG_TARGET_OS", "android");
    let target_target_os_emscripten = has("CARGO_CFG_TARGET_OS", "emscripten");
    let target_target_os_freebsd = has("CARGO_CFG_TARGET_OS", "freebsd");
    let target_target_os_linux = has("CARGO_CFG_TARGET_OS", "linux");
    let target_target_os_netbsd = has("CARGO_CFG_TARGET_OS", "netbsd");
    let target_target_os_openbsd = has("CARGO_CFG_TARGET_OS", "openbsd");
    let target_target_os_unknown = has("CARGO_CFG_TARGET_OS", "unknown");
    let target_target_os_windows = has("CARGO_CFG_TARGET_OS", "windows");
    let target_target_vendor_apple = has("CARGO_CFG_TARGET_VENDOR", "apple");
    let target_target_vendor_unknown = has("CARGO_CFG_TARGET_VENDOR", "unknown");
    let requested_test = std::env::var_os("CARGO_CFG_TEST").is_some();
    let requested_windows = std::env::var_os("CARGO_CFG_WINDOWS").is_some();
    let requested_angle = std::env::var_os("CARGO_FEATURE_ANGLE").is_some();
    let requested_counters = std::env::var_os("CARGO_FEATURE_COUNTERS").is_some();
    let requested_custom = std::env::var_os("CARGO_FEATURE_CUSTOM").is_some();
    let requested_drm = std::env::var_os("CARGO_FEATURE_DRM").is_some();
    let requested_dx11 = std::env::var_os("CARGO_FEATURE_DX11").is_some();
    let requested_dx12 = std::env::var_os("CARGO_FEATURE_DX12").is_some();
    let requested_exhaust = std::env::var_os("CARGO_FEATURE_EXHAUST").is_some();
    let requested_fragile_send_sync_non_atomic_wasm =
        std::env::var_os("CARGO_FEATURE_FRAGILE_SEND_SYNC_NON_ATOMIC_WASM").is_some();
    let requested_gles = std::env::var_os("CARGO_FEATURE_GLES").is_some();
    let requested_gles_angle = std::env::var_os("CARGO_FEATURE_GLES_ANGLE").is_some();
    let requested_glsl = std::env::var_os("CARGO_FEATURE_GLSL").is_some();
    let requested_metal = std::env::var_os("CARGO_FEATURE_METAL").is_some();
    let requested_naga_ir = std::env::var_os("CARGO_FEATURE_NAGA_IR").is_some();
    let requested_noop = std::env::var_os("CARGO_FEATURE_NOOP").is_some();
    let requested_parking_lot = std::env::var_os("CARGO_FEATURE_PARKING_LOT").is_some();
    let requested_serde = std::env::var_os("CARGO_FEATURE_SERDE").is_some();
    let requested_spirv = std::env::var_os("CARGO_FEATURE_SPIRV").is_some();
    let requested_static_dxc = std::env::var_os("CARGO_FEATURE_STATIC_DXC").is_some();
    let requested_std = std::env::var_os("CARGO_FEATURE_STD").is_some();
    let requested_strict_asserts = std::env::var_os("CARGO_FEATURE_STRICT_ASSERTS").is_some();
    let requested_trace = std::env::var_os("CARGO_FEATURE_TRACE").is_some();
    let requested_vulkan = std::env::var_os("CARGO_FEATURE_VULKAN").is_some();
    let requested_vulkan_portability =
        std::env::var_os("CARGO_FEATURE_VULKAN_PORTABILITY").is_some();
    let requested_web = std::env::var_os("CARGO_FEATURE_WEB").is_some();
    let requested_webgl = std::env::var_os("CARGO_FEATURE_WEBGL").is_some();
    let requested_webgpu = std::env::var_os("CARGO_FEATURE_WEBGPU").is_some();
    let requested_wgsl = std::env::var_os("CARGO_FEATURE_WGSL").is_some();
    println!("cargo:rustc-check-cfg=cfg(wgpu_validate_locks)");
    let jadix_wgpu_enabled = true;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_enabled)");
    if jadix_wgpu_enabled {
        println!("cargo:rustc-cfg=jadix_wgpu_enabled");
    }
    let jadix_wgpu_feature_angle = requested_angle;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_feature_angle)");
    if jadix_wgpu_feature_angle {
        println!("cargo:rustc-cfg=jadix_wgpu_feature_angle");
    }
    let jadix_wgpu_feature_counters = requested_counters;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_feature_counters)");
    if jadix_wgpu_feature_counters {
        println!("cargo:rustc-cfg=jadix_wgpu_feature_counters");
    }
    let jadix_wgpu_feature_custom = requested_custom;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_feature_custom)");
    if jadix_wgpu_feature_custom {
        println!("cargo:rustc-cfg=jadix_wgpu_feature_custom");
    }
    let jadix_wgpu_feature_default = false;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_feature_default)");
    if jadix_wgpu_feature_default {
        println!("cargo:rustc-cfg=jadix_wgpu_feature_default");
    }
    let jadix_wgpu_feature_drm = requested_drm;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_feature_drm)");
    if jadix_wgpu_feature_drm {
        println!("cargo:rustc-cfg=jadix_wgpu_feature_drm");
    }
    let jadix_wgpu_feature_dx12 = requested_dx12;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_feature_dx12)");
    if jadix_wgpu_feature_dx12 {
        println!("cargo:rustc-cfg=jadix_wgpu_feature_dx12");
    }
    let jadix_wgpu_feature_exhaust = requested_exhaust;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_feature_exhaust)");
    if jadix_wgpu_feature_exhaust {
        println!("cargo:rustc-cfg=jadix_wgpu_feature_exhaust");
    }
    let jadix_wgpu_feature_fragile_send_sync_non_atomic_wasm =
        requested_fragile_send_sync_non_atomic_wasm;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_feature_fragile_send_sync_non_atomic_wasm)");
    if jadix_wgpu_feature_fragile_send_sync_non_atomic_wasm {
        println!("cargo:rustc-cfg=jadix_wgpu_feature_fragile_send_sync_non_atomic_wasm");
    }
    let jadix_wgpu_feature_gles = requested_gles;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_feature_gles)");
    if jadix_wgpu_feature_gles {
        println!("cargo:rustc-cfg=jadix_wgpu_feature_gles");
    }
    let jadix_wgpu_feature_glsl = requested_glsl;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_feature_glsl)");
    if jadix_wgpu_feature_glsl {
        println!("cargo:rustc-cfg=jadix_wgpu_feature_glsl");
    }
    let jadix_wgpu_feature_js_sys = jadix_wgpu_feature_default;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_feature_js_sys)");
    if jadix_wgpu_feature_js_sys {
        println!("cargo:rustc-cfg=jadix_wgpu_feature_js_sys");
    }
    let jadix_wgpu_feature_metal = requested_metal;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_feature_metal)");
    if jadix_wgpu_feature_metal {
        println!("cargo:rustc-cfg=jadix_wgpu_feature_metal");
    }
    let jadix_wgpu_feature_naga = jadix_wgpu_feature_default;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_feature_naga)");
    if jadix_wgpu_feature_naga {
        println!("cargo:rustc-cfg=jadix_wgpu_feature_naga");
    }
    let jadix_wgpu_feature_naga_ir = requested_naga_ir;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_feature_naga_ir)");
    if jadix_wgpu_feature_naga_ir {
        println!("cargo:rustc-cfg=jadix_wgpu_feature_naga_ir");
    }
    let jadix_wgpu_feature_noop = requested_noop;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_feature_noop)");
    if jadix_wgpu_feature_noop {
        println!("cargo:rustc-cfg=jadix_wgpu_feature_noop");
    }
    let jadix_wgpu_feature_parking_lot = requested_parking_lot;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_feature_parking_lot)");
    if jadix_wgpu_feature_parking_lot {
        println!("cargo:rustc-cfg=jadix_wgpu_feature_parking_lot");
    }
    let jadix_wgpu_feature_serde = (requested_serde) || (requested_trace);
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_feature_serde)");
    if jadix_wgpu_feature_serde {
        println!("cargo:rustc-cfg=jadix_wgpu_feature_serde");
    }
    let jadix_wgpu_feature_smallvec = jadix_wgpu_feature_default;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_feature_smallvec)");
    if jadix_wgpu_feature_smallvec {
        println!("cargo:rustc-cfg=jadix_wgpu_feature_smallvec");
    }
    let jadix_wgpu_feature_spirv = requested_spirv;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_feature_spirv)");
    if jadix_wgpu_feature_spirv {
        println!("cargo:rustc-cfg=jadix_wgpu_feature_spirv");
    }
    let jadix_wgpu_feature_static_dxc = requested_static_dxc;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_feature_static_dxc)");
    if jadix_wgpu_feature_static_dxc {
        println!("cargo:rustc-cfg=jadix_wgpu_feature_static_dxc");
    }
    let jadix_wgpu_feature_std = requested_std;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_feature_std)");
    if jadix_wgpu_feature_std {
        println!("cargo:rustc-cfg=jadix_wgpu_feature_std");
    }
    let jadix_wgpu_feature_strict_asserts = requested_strict_asserts;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_feature_strict_asserts)");
    if jadix_wgpu_feature_strict_asserts {
        println!("cargo:rustc-cfg=jadix_wgpu_feature_strict_asserts");
    }
    let jadix_wgpu_feature_trace = requested_trace;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_feature_trace)");
    if jadix_wgpu_feature_trace {
        println!("cargo:rustc-cfg=jadix_wgpu_feature_trace");
    }
    let jadix_wgpu_feature_vulkan = requested_vulkan;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_feature_vulkan)");
    if jadix_wgpu_feature_vulkan {
        println!("cargo:rustc-cfg=jadix_wgpu_feature_vulkan");
    }
    let jadix_wgpu_feature_vulkan_portability = requested_vulkan_portability;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_feature_vulkan_portability)");
    if jadix_wgpu_feature_vulkan_portability {
        println!("cargo:rustc-cfg=jadix_wgpu_feature_vulkan_portability");
    }
    let jadix_wgpu_feature_wasm_bindgen = jadix_wgpu_feature_default;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_feature_wasm_bindgen)");
    if jadix_wgpu_feature_wasm_bindgen {
        println!("cargo:rustc-cfg=jadix_wgpu_feature_wasm_bindgen");
    }
    let jadix_wgpu_feature_wasm_bindgen_futures = jadix_wgpu_feature_default;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_feature_wasm_bindgen_futures)");
    if jadix_wgpu_feature_wasm_bindgen_futures {
        println!("cargo:rustc-cfg=jadix_wgpu_feature_wasm_bindgen_futures");
    }
    let jadix_wgpu_feature_web = (requested_web) || (requested_webgl) || (requested_webgpu);
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_feature_web)");
    if jadix_wgpu_feature_web {
        println!("cargo:rustc-cfg=jadix_wgpu_feature_web");
    }
    let jadix_wgpu_feature_web_sys = jadix_wgpu_feature_default;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_feature_web_sys)");
    if jadix_wgpu_feature_web_sys {
        println!("cargo:rustc-cfg=jadix_wgpu_feature_web_sys");
    }
    let jadix_wgpu_feature_webgl = requested_webgl;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_feature_webgl)");
    if jadix_wgpu_feature_webgl {
        println!("cargo:rustc-cfg=jadix_wgpu_feature_webgl");
    }
    let jadix_wgpu_feature_webgpu = requested_webgpu;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_feature_webgpu)");
    if jadix_wgpu_feature_webgpu {
        println!("cargo:rustc-cfg=jadix_wgpu_feature_webgpu");
    }
    let jadix_wgpu_feature_wgpu_core = jadix_wgpu_feature_default;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_feature_wgpu_core)");
    if jadix_wgpu_feature_wgpu_core {
        println!("cargo:rustc-cfg=jadix_wgpu_feature_wgpu_core");
    }
    let jadix_wgpu_feature_wgpu_hal = jadix_wgpu_feature_default;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_feature_wgpu_hal)");
    if jadix_wgpu_feature_wgpu_hal {
        println!("cargo:rustc-cfg=jadix_wgpu_feature_wgpu_hal");
    }
    let jadix_wgpu_feature_wgsl = requested_wgsl;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_feature_wgsl)");
    if jadix_wgpu_feature_wgsl {
        println!("cargo:rustc-cfg=jadix_wgpu_feature_wgsl");
    }
    let jadix_wgpu_native = !target_target_arch_wasm32;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_native)");
    if jadix_wgpu_native {
        println!("cargo:rustc-cfg=jadix_wgpu_native");
    }
    let jadix_wgpu_emscripten = target_target_arch_wasm32 && target_target_os_emscripten;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_emscripten)");
    if jadix_wgpu_emscripten {
        println!("cargo:rustc-cfg=jadix_wgpu_emscripten");
    }
    let jadix_wgpu_supports_64bit_atomics = target_target_has_atomic_64;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_supports_64bit_atomics)");
    if jadix_wgpu_supports_64bit_atomics {
        println!("cargo:rustc-cfg=jadix_wgpu_supports_64bit_atomics");
    }
    let jadix_wgpu_core_enabled = (!target_target_arch_wasm32)
        || (target_target_os_emscripten)
        || (requested_noop)
        || (requested_webgl);
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_core_enabled)");
    if jadix_wgpu_core_enabled {
        println!("cargo:rustc-cfg=jadix_wgpu_core_enabled");
    }
    let jadix_wgpu_core_feature_angle = requested_angle
        && ((!target_target_arch_wasm32)
            || (target_target_os_emscripten)
            || (requested_noop)
            || (requested_webgl));
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_core_feature_angle)");
    if jadix_wgpu_core_feature_angle {
        println!("cargo:rustc-cfg=jadix_wgpu_core_feature_angle");
    }
    let jadix_wgpu_core_feature_api_log_info = jadix_wgpu_feature_default;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_core_feature_api_log_info)");
    if jadix_wgpu_core_feature_api_log_info {
        println!("cargo:rustc-cfg=jadix_wgpu_core_feature_api_log_info");
    }
    let jadix_wgpu_core_feature_counters = requested_counters
        && ((!target_target_arch_wasm32)
            || (target_target_os_emscripten)
            || (requested_noop)
            || (requested_webgl));
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_core_feature_counters)");
    if jadix_wgpu_core_feature_counters {
        println!("cargo:rustc-cfg=jadix_wgpu_core_feature_counters");
    }
    let jadix_wgpu_core_feature_default = (!target_target_arch_wasm32)
        || (target_target_os_emscripten)
        || (requested_noop)
        || (requested_webgl);
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_core_feature_default)");
    if jadix_wgpu_core_feature_default {
        println!("cargo:rustc-cfg=jadix_wgpu_core_feature_default");
    }
    let jadix_wgpu_core_feature_drm = requested_drm
        && ((!target_target_arch_wasm32)
            || (target_target_os_emscripten)
            || (requested_noop)
            || (requested_webgl));
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_core_feature_drm)");
    if jadix_wgpu_core_feature_drm {
        println!("cargo:rustc-cfg=jadix_wgpu_core_feature_drm");
    }
    let jadix_wgpu_core_feature_dx12 = requested_dx12
        && ((!target_target_arch_wasm32)
            || (target_target_os_emscripten)
            || (requested_noop)
            || (requested_webgl));
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_core_feature_dx12)");
    if jadix_wgpu_core_feature_dx12 {
        println!("cargo:rustc-cfg=jadix_wgpu_core_feature_dx12");
    }
    let jadix_wgpu_core_feature_fragile_send_sync_non_atomic_wasm =
        requested_fragile_send_sync_non_atomic_wasm
            && ((!target_target_arch_wasm32)
                || (target_target_os_emscripten)
                || (requested_noop)
                || (requested_webgl));
    println!(
        "cargo:rustc-check-cfg=cfg(jadix_wgpu_core_feature_fragile_send_sync_non_atomic_wasm)"
    );
    if jadix_wgpu_core_feature_fragile_send_sync_non_atomic_wasm {
        println!("cargo:rustc-cfg=jadix_wgpu_core_feature_fragile_send_sync_non_atomic_wasm");
    }
    let jadix_wgpu_core_feature_gles = requested_gles
        && ((!target_target_arch_wasm32)
            || (target_target_os_emscripten)
            || (requested_noop)
            || (requested_webgl));
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_core_feature_gles)");
    if jadix_wgpu_core_feature_gles {
        println!("cargo:rustc-cfg=jadix_wgpu_core_feature_gles");
    }
    let jadix_wgpu_core_feature_glsl = requested_glsl
        && ((!target_target_arch_wasm32)
            || (target_target_os_emscripten)
            || (requested_noop)
            || (requested_webgl));
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_core_feature_glsl)");
    if jadix_wgpu_core_feature_glsl {
        println!("cargo:rustc-cfg=jadix_wgpu_core_feature_glsl");
    }
    let jadix_wgpu_core_feature_macro_rules_attribute = (!target_target_arch_wasm32
        && requested_serde)
        || (!target_target_arch_wasm32 && requested_trace)
        || (target_target_os_emscripten && requested_serde)
        || (target_target_os_emscripten && requested_trace)
        || (requested_noop && requested_serde)
        || (requested_noop && requested_trace)
        || (requested_serde && requested_webgl)
        || (requested_trace && requested_webgl);
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_core_feature_macro_rules_attribute)");
    if jadix_wgpu_core_feature_macro_rules_attribute {
        println!("cargo:rustc-cfg=jadix_wgpu_core_feature_macro_rules_attribute");
    }
    let jadix_wgpu_core_feature_metal = requested_metal
        && ((!target_target_arch_wasm32)
            || (target_target_os_emscripten)
            || (requested_noop)
            || (requested_webgl));
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_core_feature_metal)");
    if jadix_wgpu_core_feature_metal {
        println!("cargo:rustc-cfg=jadix_wgpu_core_feature_metal");
    }
    let jadix_wgpu_core_feature_noop = jadix_wgpu_feature_noop;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_core_feature_noop)");
    if jadix_wgpu_core_feature_noop {
        println!("cargo:rustc-cfg=jadix_wgpu_core_feature_noop");
    }
    let jadix_wgpu_core_feature_observe_locks = jadix_wgpu_feature_default;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_core_feature_observe_locks)");
    if jadix_wgpu_core_feature_observe_locks {
        println!("cargo:rustc-cfg=jadix_wgpu_core_feature_observe_locks");
    }
    let jadix_wgpu_core_feature_portable_atomic = jadix_wgpu_native;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_core_feature_portable_atomic)");
    if jadix_wgpu_core_feature_portable_atomic {
        println!("cargo:rustc-cfg=jadix_wgpu_core_feature_portable_atomic");
    }
    let jadix_wgpu_core_feature_renderdoc = jadix_wgpu_native;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_core_feature_renderdoc)");
    if jadix_wgpu_core_feature_renderdoc {
        println!("cargo:rustc-cfg=jadix_wgpu_core_feature_renderdoc");
    }
    let jadix_wgpu_core_feature_replay = jadix_wgpu_feature_default;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_core_feature_replay)");
    if jadix_wgpu_core_feature_replay {
        println!("cargo:rustc-cfg=jadix_wgpu_core_feature_replay");
    }
    let jadix_wgpu_core_feature_resource_log_info = jadix_wgpu_feature_default;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_core_feature_resource_log_info)");
    if jadix_wgpu_core_feature_resource_log_info {
        println!("cargo:rustc-cfg=jadix_wgpu_core_feature_resource_log_info");
    }
    let jadix_wgpu_core_feature_ron = jadix_wgpu_feature_default;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_core_feature_ron)");
    if jadix_wgpu_core_feature_ron {
        println!("cargo:rustc-cfg=jadix_wgpu_core_feature_ron");
    }
    let jadix_wgpu_core_feature_serde = (!target_target_arch_wasm32 && requested_serde)
        || (!target_target_arch_wasm32 && requested_trace)
        || (target_target_os_emscripten && requested_serde)
        || (target_target_os_emscripten && requested_trace)
        || (requested_noop && requested_serde)
        || (requested_noop && requested_trace)
        || (requested_serde && requested_webgl)
        || (requested_trace && requested_webgl);
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_core_feature_serde)");
    if jadix_wgpu_core_feature_serde {
        println!("cargo:rustc-cfg=jadix_wgpu_core_feature_serde");
    }
    let jadix_wgpu_core_feature_spirv = requested_spirv
        && ((!target_target_arch_wasm32)
            || (target_target_os_emscripten)
            || (requested_noop)
            || (requested_webgl));
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_core_feature_spirv)");
    if jadix_wgpu_core_feature_spirv {
        println!("cargo:rustc-cfg=jadix_wgpu_core_feature_spirv");
    }
    let jadix_wgpu_core_feature_static_dxc = requested_static_dxc
        && ((!target_target_arch_wasm32)
            || (target_target_os_emscripten)
            || (requested_noop)
            || (requested_webgl));
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_core_feature_static_dxc)");
    if jadix_wgpu_core_feature_static_dxc {
        println!("cargo:rustc-cfg=jadix_wgpu_core_feature_static_dxc");
    }
    let jadix_wgpu_core_feature_std = (!target_target_arch_wasm32)
        || (target_target_os_emscripten)
        || (requested_noop)
        || (requested_webgl);
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_core_feature_std)");
    if jadix_wgpu_core_feature_std {
        println!("cargo:rustc-cfg=jadix_wgpu_core_feature_std");
    }
    let jadix_wgpu_core_feature_strict_asserts = std::env::var_os("CARGO_FEATURE_STRICT_ASSERTS")
        .is_some()
        && ((!target_target_arch_wasm32)
            || (target_target_os_emscripten)
            || (requested_noop)
            || (requested_webgl));
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_core_feature_strict_asserts)");
    if jadix_wgpu_core_feature_strict_asserts {
        println!("cargo:rustc-cfg=jadix_wgpu_core_feature_strict_asserts");
    }
    let jadix_wgpu_core_feature_trace = requested_trace
        && ((!target_target_arch_wasm32)
            || (target_target_os_emscripten)
            || (requested_noop)
            || (requested_webgl));
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_core_feature_trace)");
    if jadix_wgpu_core_feature_trace {
        println!("cargo:rustc-cfg=jadix_wgpu_core_feature_trace");
    }
    let jadix_wgpu_core_feature_vulkan = requested_vulkan
        && ((!target_target_arch_wasm32)
            || (target_target_os_emscripten)
            || (requested_noop)
            || (requested_webgl));
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_core_feature_vulkan)");
    if jadix_wgpu_core_feature_vulkan {
        println!("cargo:rustc-cfg=jadix_wgpu_core_feature_vulkan");
    }
    let jadix_wgpu_core_feature_vulkan_portability = requested_vulkan_portability
        && ((!target_target_arch_wasm32)
            || (target_target_os_emscripten)
            || (requested_noop)
            || (requested_webgl));
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_core_feature_vulkan_portability)");
    if jadix_wgpu_core_feature_vulkan_portability {
        println!("cargo:rustc-cfg=jadix_wgpu_core_feature_vulkan_portability");
    }
    let jadix_wgpu_core_feature_webgl = jadix_wgpu_feature_webgl;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_core_feature_webgl)");
    if jadix_wgpu_core_feature_webgl {
        println!("cargo:rustc-cfg=jadix_wgpu_core_feature_webgl");
    }
    let jadix_wgpu_core_feature_wgpu_core_deps_apple = jadix_wgpu_feature_default;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_core_feature_wgpu_core_deps_apple)");
    if jadix_wgpu_core_feature_wgpu_core_deps_apple {
        println!("cargo:rustc-cfg=jadix_wgpu_core_feature_wgpu_core_deps_apple");
    }
    let jadix_wgpu_core_feature_wgpu_core_deps_emscripten = jadix_wgpu_feature_default;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_core_feature_wgpu_core_deps_emscripten)");
    if jadix_wgpu_core_feature_wgpu_core_deps_emscripten {
        println!("cargo:rustc-cfg=jadix_wgpu_core_feature_wgpu_core_deps_emscripten");
    }
    let jadix_wgpu_core_feature_wgpu_core_deps_wasm = jadix_wgpu_feature_default;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_core_feature_wgpu_core_deps_wasm)");
    if jadix_wgpu_core_feature_wgpu_core_deps_wasm {
        println!("cargo:rustc-cfg=jadix_wgpu_core_feature_wgpu_core_deps_wasm");
    }
    let jadix_wgpu_core_feature_wgpu_core_deps_windows_linux_android = jadix_wgpu_feature_default;
    println!(
        "cargo:rustc-check-cfg=cfg(jadix_wgpu_core_feature_wgpu_core_deps_windows_linux_android)"
    );
    if jadix_wgpu_core_feature_wgpu_core_deps_windows_linux_android {
        println!("cargo:rustc-cfg=jadix_wgpu_core_feature_wgpu_core_deps_windows_linux_android");
    }
    let jadix_wgpu_core_feature_wgsl = (!target_target_arch_wasm32)
        || (target_target_os_emscripten && requested_wgsl)
        || (requested_noop && requested_wgsl)
        || (requested_webgl && requested_wgsl);
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_core_feature_wgsl)");
    if jadix_wgpu_core_feature_wgsl {
        println!("cargo:rustc-cfg=jadix_wgpu_core_feature_wgsl");
    }
    let jadix_wgpu_core_windows_linux_android = (target_target_os_android)
        || (target_target_os_freebsd)
        || (target_target_os_linux)
        || (requested_windows);
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_core_windows_linux_android)");
    if jadix_wgpu_core_windows_linux_android {
        println!("cargo:rustc-cfg=jadix_wgpu_core_windows_linux_android");
    }
    let jadix_wgpu_core_supports_64bit_atomics = jadix_wgpu_supports_64bit_atomics;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_core_supports_64bit_atomics)");
    if jadix_wgpu_core_supports_64bit_atomics {
        println!("cargo:rustc-cfg=jadix_wgpu_core_supports_64bit_atomics");
    }
    let jadix_wgpu_hal_enabled = (!target_target_arch_wasm32)
        || (target_target_os_emscripten)
        || (requested_noop)
        || (requested_webgl);
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_hal_enabled)");
    if jadix_wgpu_hal_enabled {
        println!("cargo:rustc-cfg=jadix_wgpu_hal_enabled");
    }
    let jadix_wgpu_hal_feature_android_system_properties = jadix_wgpu_feature_default;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_hal_feature_android_system_properties)");
    if jadix_wgpu_hal_feature_android_system_properties {
        println!("cargo:rustc-cfg=jadix_wgpu_hal_feature_android_system_properties");
    }
    let jadix_wgpu_hal_feature_arrayvec = jadix_wgpu_feature_default;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_hal_feature_arrayvec)");
    if jadix_wgpu_hal_feature_arrayvec {
        println!("cargo:rustc-cfg=jadix_wgpu_hal_feature_arrayvec");
    }
    let jadix_wgpu_hal_feature_ash = jadix_wgpu_feature_default;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_hal_feature_ash)");
    if jadix_wgpu_hal_feature_ash {
        println!("cargo:rustc-cfg=jadix_wgpu_hal_feature_ash");
    }
    let jadix_wgpu_hal_feature_bit_set = jadix_wgpu_feature_default;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_hal_feature_bit_set)");
    if jadix_wgpu_hal_feature_bit_set {
        println!("cargo:rustc-cfg=jadix_wgpu_hal_feature_bit_set");
    }
    let jadix_wgpu_hal_feature_block2 = jadix_wgpu_feature_default;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_hal_feature_block2)");
    if jadix_wgpu_hal_feature_block2 {
        println!("cargo:rustc-cfg=jadix_wgpu_hal_feature_block2");
    }
    let jadix_wgpu_hal_feature_bytemuck = jadix_wgpu_feature_default;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_hal_feature_bytemuck)");
    if jadix_wgpu_hal_feature_bytemuck {
        println!("cargo:rustc-cfg=jadix_wgpu_hal_feature_bytemuck");
    }
    let jadix_wgpu_hal_feature_device_lost_panic = jadix_wgpu_feature_default;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_hal_feature_device_lost_panic)");
    if jadix_wgpu_hal_feature_device_lost_panic {
        println!("cargo:rustc-cfg=jadix_wgpu_hal_feature_device_lost_panic");
    }
    let jadix_wgpu_hal_feature_drm = (requested_windows
        || target_target_os_linux
        || target_target_os_android
        || target_target_os_freebsd
        || target_target_os_netbsd)
        && (requested_windows
            || target_target_os_linux
            || target_target_os_android
            || target_target_os_freebsd)
        && requested_drm
        && ((!target_target_arch_wasm32)
            || (target_target_os_emscripten)
            || (requested_noop)
            || (requested_webgl));
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_hal_feature_drm)");
    if jadix_wgpu_hal_feature_drm {
        println!("cargo:rustc-cfg=jadix_wgpu_hal_feature_drm");
    }
    let jadix_wgpu_hal_feature_dx11 = requested_dx11;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_hal_feature_dx11)");
    if jadix_wgpu_hal_feature_dx11 {
        println!("cargo:rustc-cfg=jadix_wgpu_hal_feature_dx11");
    }
    let jadix_wgpu_hal_feature_dx12 = (requested_windows
        || target_target_os_linux
        || target_target_os_android
        || target_target_os_freebsd
        || target_target_os_netbsd)
        && (requested_windows
            || target_target_os_linux
            || target_target_os_android
            || target_target_os_freebsd)
        && requested_dx12
        && ((!target_target_arch_wasm32)
            || (target_target_os_emscripten)
            || (requested_noop)
            || (requested_webgl));
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_hal_feature_dx12)");
    if jadix_wgpu_hal_feature_dx12 {
        println!("cargo:rustc-cfg=jadix_wgpu_hal_feature_dx12");
    }
    let jadix_wgpu_hal_feature_fragile_send_sync_non_atomic_wasm =
        requested_fragile_send_sync_non_atomic_wasm
            && ((!target_target_arch_wasm32)
                || (target_target_os_emscripten)
                || (requested_noop)
                || (requested_webgl));
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_hal_feature_fragile_send_sync_non_atomic_wasm)");
    if jadix_wgpu_hal_feature_fragile_send_sync_non_atomic_wasm {
        println!("cargo:rustc-cfg=jadix_wgpu_hal_feature_fragile_send_sync_non_atomic_wasm");
    }
    let jadix_wgpu_hal_feature_gles = (!target_target_arch_wasm32
        && (requested_windows
            || target_target_os_linux
            || target_target_os_android
            || target_target_os_freebsd
            || target_target_os_netbsd)
        && (requested_windows
            || target_target_os_linux
            || target_target_os_android
            || target_target_os_freebsd)
        && requested_gles)
        || (!target_target_arch_wasm32 && target_target_vendor_apple && requested_angle)
        || ((target_target_arch_wasm32 && !target_target_os_emscripten) && requested_webgl)
        || ((requested_windows
            || target_target_os_linux
            || target_target_os_android
            || target_target_os_freebsd
            || target_target_os_netbsd)
            && (requested_windows
                || target_target_os_linux
                || target_target_os_android
                || target_target_os_freebsd)
            && requested_gles
            && requested_noop)
        || ((requested_windows
            || target_target_os_linux
            || target_target_os_android
            || target_target_os_freebsd
            || target_target_os_netbsd)
            && (requested_windows
                || target_target_os_linux
                || target_target_os_android
                || target_target_os_freebsd)
            && requested_gles
            && requested_webgl)
        || (target_target_os_emscripten && target_target_vendor_apple && requested_angle)
        || (target_target_os_emscripten && requested_gles)
        || (target_target_vendor_apple && requested_angle && requested_noop)
        || (target_target_vendor_apple && requested_angle && requested_webgl)
        || (requested_gles_angle);
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_hal_feature_gles)");
    if jadix_wgpu_hal_feature_gles {
        println!("cargo:rustc-cfg=jadix_wgpu_hal_feature_gles");
    }
    let jadix_wgpu_hal_feature_gles_angle = requested_gles_angle;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_hal_feature_gles_angle)");
    if jadix_wgpu_hal_feature_gles_angle {
        println!("cargo:rustc-cfg=jadix_wgpu_hal_feature_gles_angle");
    }
    let jadix_wgpu_hal_feature_glow = jadix_wgpu_feature_default;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_hal_feature_glow)");
    if jadix_wgpu_hal_feature_glow {
        println!("cargo:rustc-cfg=jadix_wgpu_hal_feature_glow");
    }
    let jadix_wgpu_hal_feature_glutin_wgl_sys = jadix_wgpu_feature_default;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_hal_feature_glutin_wgl_sys)");
    if jadix_wgpu_hal_feature_glutin_wgl_sys {
        println!("cargo:rustc-cfg=jadix_wgpu_hal_feature_glutin_wgl_sys");
    }
    let jadix_wgpu_hal_feature_gpu_allocator = jadix_wgpu_feature_default;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_hal_feature_gpu_allocator)");
    if jadix_wgpu_hal_feature_gpu_allocator {
        println!("cargo:rustc-cfg=jadix_wgpu_hal_feature_gpu_allocator");
    }
    let jadix_wgpu_hal_feature_internal_error_panic = jadix_wgpu_feature_default;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_hal_feature_internal_error_panic)");
    if jadix_wgpu_hal_feature_internal_error_panic {
        println!("cargo:rustc-cfg=jadix_wgpu_hal_feature_internal_error_panic");
    }
    let jadix_wgpu_hal_feature_js_sys = jadix_wgpu_feature_default;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_hal_feature_js_sys)");
    if jadix_wgpu_hal_feature_js_sys {
        println!("cargo:rustc-cfg=jadix_wgpu_hal_feature_js_sys");
    }
    let jadix_wgpu_hal_feature_khronos_egl = jadix_wgpu_feature_default;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_hal_feature_khronos_egl)");
    if jadix_wgpu_hal_feature_khronos_egl {
        println!("cargo:rustc-cfg=jadix_wgpu_hal_feature_khronos_egl");
    }
    let jadix_wgpu_hal_feature_libc = jadix_wgpu_feature_default;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_hal_feature_libc)");
    if jadix_wgpu_hal_feature_libc {
        println!("cargo:rustc-cfg=jadix_wgpu_hal_feature_libc");
    }
    let jadix_wgpu_hal_feature_libloading = jadix_wgpu_feature_default;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_hal_feature_libloading)");
    if jadix_wgpu_hal_feature_libloading {
        println!("cargo:rustc-cfg=jadix_wgpu_hal_feature_libloading");
    }
    let jadix_wgpu_hal_feature_mach_dxcompiler_rs = jadix_wgpu_feature_default;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_hal_feature_mach_dxcompiler_rs)");
    if jadix_wgpu_hal_feature_mach_dxcompiler_rs {
        println!("cargo:rustc-cfg=jadix_wgpu_hal_feature_mach_dxcompiler_rs");
    }
    let jadix_wgpu_hal_feature_metal = target_target_vendor_apple
        && requested_metal
        && ((!target_target_arch_wasm32)
            || (target_target_os_emscripten)
            || (requested_noop)
            || (requested_webgl));
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_hal_feature_metal)");
    if jadix_wgpu_hal_feature_metal {
        println!("cargo:rustc-cfg=jadix_wgpu_hal_feature_metal");
    }
    let jadix_wgpu_hal_feature_ndk_sys = jadix_wgpu_feature_default;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_hal_feature_ndk_sys)");
    if jadix_wgpu_hal_feature_ndk_sys {
        println!("cargo:rustc-cfg=jadix_wgpu_hal_feature_ndk_sys");
    }
    let jadix_wgpu_hal_feature_objc2 = jadix_wgpu_feature_default;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_hal_feature_objc2)");
    if jadix_wgpu_hal_feature_objc2 {
        println!("cargo:rustc-cfg=jadix_wgpu_hal_feature_objc2");
    }
    let jadix_wgpu_hal_feature_objc2_core_foundation = jadix_wgpu_feature_default;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_hal_feature_objc2_core_foundation)");
    if jadix_wgpu_hal_feature_objc2_core_foundation {
        println!("cargo:rustc-cfg=jadix_wgpu_hal_feature_objc2_core_foundation");
    }
    let jadix_wgpu_hal_feature_objc2_core_graphics = jadix_wgpu_feature_default;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_hal_feature_objc2_core_graphics)");
    if jadix_wgpu_hal_feature_objc2_core_graphics {
        println!("cargo:rustc-cfg=jadix_wgpu_hal_feature_objc2_core_graphics");
    }
    let jadix_wgpu_hal_feature_objc2_foundation = jadix_wgpu_feature_default;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_hal_feature_objc2_foundation)");
    if jadix_wgpu_hal_feature_objc2_foundation {
        println!("cargo:rustc-cfg=jadix_wgpu_hal_feature_objc2_foundation");
    }
    let jadix_wgpu_hal_feature_objc2_metal = jadix_wgpu_feature_default;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_hal_feature_objc2_metal)");
    if jadix_wgpu_hal_feature_objc2_metal {
        println!("cargo:rustc-cfg=jadix_wgpu_hal_feature_objc2_metal");
    }
    let jadix_wgpu_hal_feature_objc2_quartz_core = jadix_wgpu_feature_default;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_hal_feature_objc2_quartz_core)");
    if jadix_wgpu_hal_feature_objc2_quartz_core {
        println!("cargo:rustc-cfg=jadix_wgpu_hal_feature_objc2_quartz_core");
    }
    let jadix_wgpu_hal_feature_once_cell = jadix_wgpu_feature_default;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_hal_feature_once_cell)");
    if jadix_wgpu_hal_feature_once_cell {
        println!("cargo:rustc-cfg=jadix_wgpu_hal_feature_once_cell");
    }
    let jadix_wgpu_hal_feature_ordered_float = jadix_wgpu_feature_default;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_hal_feature_ordered_float)");
    if jadix_wgpu_hal_feature_ordered_float {
        println!("cargo:rustc-cfg=jadix_wgpu_hal_feature_ordered_float");
    }
    let jadix_wgpu_hal_feature_parking_lot = jadix_wgpu_feature_default;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_hal_feature_parking_lot)");
    if jadix_wgpu_hal_feature_parking_lot {
        println!("cargo:rustc-cfg=jadix_wgpu_hal_feature_parking_lot");
    }
    let jadix_wgpu_hal_feature_portable_atomic = jadix_wgpu_native;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_hal_feature_portable_atomic)");
    if jadix_wgpu_hal_feature_portable_atomic {
        println!("cargo:rustc-cfg=jadix_wgpu_hal_feature_portable_atomic");
    }
    let jadix_wgpu_hal_feature_portable_atomic_util = jadix_wgpu_feature_default;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_hal_feature_portable_atomic_util)");
    if jadix_wgpu_hal_feature_portable_atomic_util {
        println!("cargo:rustc-cfg=jadix_wgpu_hal_feature_portable_atomic_util");
    }
    let jadix_wgpu_hal_feature_profiling = jadix_wgpu_feature_default;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_hal_feature_profiling)");
    if jadix_wgpu_hal_feature_profiling {
        println!("cargo:rustc-cfg=jadix_wgpu_hal_feature_profiling");
    }
    let jadix_wgpu_hal_feature_range_alloc = jadix_wgpu_feature_default;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_hal_feature_range_alloc)");
    if jadix_wgpu_hal_feature_range_alloc {
        println!("cargo:rustc-cfg=jadix_wgpu_hal_feature_range_alloc");
    }
    let jadix_wgpu_hal_feature_raw_window_metal = jadix_wgpu_feature_default;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_hal_feature_raw_window_metal)");
    if jadix_wgpu_hal_feature_raw_window_metal {
        println!("cargo:rustc-cfg=jadix_wgpu_hal_feature_raw_window_metal");
    }
    let jadix_wgpu_hal_feature_renderdoc = (!target_target_arch_wasm32
        && (requested_windows
            || target_target_os_linux
            || target_target_os_android
            || target_target_os_freebsd
            || target_target_os_netbsd)
        && (requested_windows
            || target_target_os_linux
            || target_target_os_android
            || target_target_os_freebsd))
        || (!target_target_arch_wasm32 && target_target_vendor_apple && requested_angle)
        || (!target_target_arch_wasm32
            && target_target_vendor_apple
            && requested_vulkan_portability)
        || (target_target_os_emscripten && target_target_vendor_apple && requested_angle)
        || (target_target_os_emscripten
            && target_target_vendor_apple
            && requested_vulkan_portability)
        || (target_target_vendor_apple && requested_angle && requested_noop)
        || (target_target_vendor_apple && requested_angle && requested_webgl)
        || (target_target_vendor_apple && requested_noop && requested_vulkan_portability)
        || (target_target_vendor_apple && requested_vulkan_portability && requested_webgl);
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_hal_feature_renderdoc)");
    if jadix_wgpu_hal_feature_renderdoc {
        println!("cargo:rustc-cfg=jadix_wgpu_hal_feature_renderdoc");
    }
    let jadix_wgpu_hal_feature_renderdoc_sys = jadix_wgpu_feature_default;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_hal_feature_renderdoc_sys)");
    if jadix_wgpu_hal_feature_renderdoc_sys {
        println!("cargo:rustc-cfg=jadix_wgpu_hal_feature_renderdoc_sys");
    }
    let jadix_wgpu_hal_feature_smallvec = jadix_wgpu_feature_default;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_hal_feature_smallvec)");
    if jadix_wgpu_hal_feature_smallvec {
        println!("cargo:rustc-cfg=jadix_wgpu_hal_feature_smallvec");
    }
    let jadix_wgpu_hal_feature_static_dxc = requested_static_dxc
        && ((!target_target_arch_wasm32)
            || (target_target_os_emscripten)
            || (requested_noop)
            || (requested_webgl));
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_hal_feature_static_dxc)");
    if jadix_wgpu_hal_feature_static_dxc {
        println!("cargo:rustc-cfg=jadix_wgpu_hal_feature_static_dxc");
    }
    let jadix_wgpu_hal_feature_validation_canary = jadix_wgpu_feature_default;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_hal_feature_validation_canary)");
    if jadix_wgpu_hal_feature_validation_canary {
        println!("cargo:rustc-cfg=jadix_wgpu_hal_feature_validation_canary");
    }
    let jadix_wgpu_hal_feature_vulkan = (!target_target_arch_wasm32
        && (requested_windows
            || target_target_os_linux
            || target_target_os_android
            || target_target_os_freebsd
            || target_target_os_netbsd)
        && (requested_windows
            || target_target_os_linux
            || target_target_os_android
            || target_target_os_freebsd)
        && requested_vulkan)
        || (!target_target_arch_wasm32
            && target_target_vendor_apple
            && requested_vulkan_portability)
        || ((requested_windows
            || target_target_os_linux
            || target_target_os_android
            || target_target_os_freebsd
            || target_target_os_netbsd)
            && (requested_windows
                || target_target_os_linux
                || target_target_os_android
                || target_target_os_freebsd)
            && target_target_os_emscripten
            && requested_vulkan)
        || ((requested_windows
            || target_target_os_linux
            || target_target_os_android
            || target_target_os_freebsd
            || target_target_os_netbsd)
            && (requested_windows
                || target_target_os_linux
                || target_target_os_android
                || target_target_os_freebsd)
            && requested_noop
            && requested_vulkan)
        || ((requested_windows
            || target_target_os_linux
            || target_target_os_android
            || target_target_os_freebsd
            || target_target_os_netbsd)
            && (requested_windows
                || target_target_os_linux
                || target_target_os_android
                || target_target_os_freebsd)
            && requested_vulkan
            && requested_webgl)
        || (target_target_os_emscripten
            && target_target_vendor_apple
            && requested_vulkan_portability)
        || (target_target_vendor_apple && requested_noop && requested_vulkan_portability)
        || (target_target_vendor_apple && requested_vulkan_portability && requested_webgl);
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_hal_feature_vulkan)");
    if jadix_wgpu_hal_feature_vulkan {
        println!("cargo:rustc-cfg=jadix_wgpu_hal_feature_vulkan");
    }
    let jadix_wgpu_hal_feature_wasm_bindgen = jadix_wgpu_feature_default;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_hal_feature_wasm_bindgen)");
    if jadix_wgpu_hal_feature_wasm_bindgen {
        println!("cargo:rustc-cfg=jadix_wgpu_hal_feature_wasm_bindgen");
    }
    let jadix_wgpu_hal_feature_wayland_sys = jadix_wgpu_feature_default;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_hal_feature_wayland_sys)");
    if jadix_wgpu_hal_feature_wayland_sys {
        println!("cargo:rustc-cfg=jadix_wgpu_hal_feature_wayland_sys");
    }
    let jadix_wgpu_hal_feature_web_sys = jadix_wgpu_feature_default;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_hal_feature_web_sys)");
    if jadix_wgpu_hal_feature_web_sys {
        println!("cargo:rustc-cfg=jadix_wgpu_hal_feature_web_sys");
    }
    let jadix_wgpu_hal_feature_windows = jadix_wgpu_feature_default;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_hal_feature_windows)");
    if jadix_wgpu_hal_feature_windows {
        println!("cargo:rustc-cfg=jadix_wgpu_hal_feature_windows");
    }
    let jadix_wgpu_hal_feature_windows_core = jadix_wgpu_feature_default;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_hal_feature_windows_core)");
    if jadix_wgpu_hal_feature_windows_core {
        println!("cargo:rustc-cfg=jadix_wgpu_hal_feature_windows_core");
    }
    let jadix_wgpu_hal_feature_windows_result = jadix_wgpu_feature_default;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_hal_feature_windows_result)");
    if jadix_wgpu_hal_feature_windows_result {
        println!("cargo:rustc-cfg=jadix_wgpu_hal_feature_windows_result");
    }
    let jadix_wgpu_hal_native = jadix_wgpu_native;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_hal_native)");
    if jadix_wgpu_hal_native {
        println!("cargo:rustc-cfg=jadix_wgpu_hal_native");
    }
    let jadix_wgpu_hal_supports_64bit_atomics = jadix_wgpu_supports_64bit_atomics;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_hal_supports_64bit_atomics)");
    if jadix_wgpu_hal_supports_64bit_atomics {
        println!("cargo:rustc-cfg=jadix_wgpu_hal_supports_64bit_atomics");
    }
    let jadix_wgpu_hal_supports_ptr_atomics = target_target_has_atomic_ptr;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_hal_supports_ptr_atomics)");
    if jadix_wgpu_hal_supports_ptr_atomics {
        println!("cargo:rustc-cfg=jadix_wgpu_hal_supports_ptr_atomics");
    }
    let jadix_wgpu_types_enabled = jadix_wgpu_enabled;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_types_enabled)");
    if jadix_wgpu_types_enabled {
        println!("cargo:rustc-cfg=jadix_wgpu_types_enabled");
    }
    let jadix_wgpu_types_feature_counters = requested_counters
        && ((!target_target_arch_wasm32)
            || (target_target_os_emscripten)
            || (requested_noop)
            || (requested_webgl));
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_types_feature_counters)");
    if jadix_wgpu_types_feature_counters {
        println!("cargo:rustc-cfg=jadix_wgpu_types_feature_counters");
    }
    let jadix_wgpu_types_feature_default = jadix_wgpu_feature_default;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_types_feature_default)");
    if jadix_wgpu_types_feature_default {
        println!("cargo:rustc-cfg=jadix_wgpu_types_feature_default");
    }
    let jadix_wgpu_types_feature_exhaust = jadix_wgpu_feature_exhaust;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_types_feature_exhaust)");
    if jadix_wgpu_types_feature_exhaust {
        println!("cargo:rustc-cfg=jadix_wgpu_types_feature_exhaust");
    }
    let jadix_wgpu_types_feature_fragile_send_sync_non_atomic_wasm =
        requested_fragile_send_sync_non_atomic_wasm;
    println!(
        "cargo:rustc-check-cfg=cfg(jadix_wgpu_types_feature_fragile_send_sync_non_atomic_wasm)"
    );
    if jadix_wgpu_types_feature_fragile_send_sync_non_atomic_wasm {
        println!("cargo:rustc-cfg=jadix_wgpu_types_feature_fragile_send_sync_non_atomic_wasm");
    }
    let jadix_wgpu_types_feature_js_sys = jadix_wgpu_feature_default;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_types_feature_js_sys)");
    if jadix_wgpu_types_feature_js_sys {
        println!("cargo:rustc-cfg=jadix_wgpu_types_feature_js_sys");
    }
    let jadix_wgpu_types_feature_serde = (requested_serde) || (requested_trace);
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_types_feature_serde)");
    if jadix_wgpu_types_feature_serde {
        println!("cargo:rustc-cfg=jadix_wgpu_types_feature_serde");
    }
    let jadix_wgpu_types_feature_std = (requested_std) || (requested_trace);
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_types_feature_std)");
    if jadix_wgpu_types_feature_std {
        println!("cargo:rustc-cfg=jadix_wgpu_types_feature_std");
    }
    let jadix_wgpu_types_feature_strict_asserts = requested_strict_asserts;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_types_feature_strict_asserts)");
    if jadix_wgpu_types_feature_strict_asserts {
        println!("cargo:rustc-cfg=jadix_wgpu_types_feature_strict_asserts");
    }
    let jadix_wgpu_types_feature_trace = jadix_wgpu_feature_trace;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_types_feature_trace)");
    if jadix_wgpu_types_feature_trace {
        println!("cargo:rustc-cfg=jadix_wgpu_types_feature_trace");
    }
    let jadix_wgpu_types_feature_web = (!target_target_arch_wasm32
        && (requested_windows
            || target_target_os_linux
            || target_target_os_android
            || target_target_os_freebsd
            || target_target_os_netbsd)
        && (requested_windows
            || target_target_os_linux
            || target_target_os_android
            || target_target_os_freebsd)
        && requested_gles)
        || (!target_target_arch_wasm32 && target_target_vendor_apple && requested_angle)
        || (!target_target_arch_wasm32 && requested_gles_angle)
        || ((requested_windows
            || target_target_os_linux
            || target_target_os_android
            || target_target_os_freebsd
            || target_target_os_netbsd)
            && (requested_windows
                || target_target_os_linux
                || target_target_os_android
                || target_target_os_freebsd)
            && requested_gles
            && requested_noop)
        || (target_target_os_emscripten && target_target_vendor_apple && requested_angle)
        || (target_target_os_emscripten && requested_gles)
        || (target_target_os_emscripten && requested_gles_angle)
        || (target_target_vendor_apple && requested_angle && requested_noop)
        || (requested_gles_angle && requested_noop)
        || (requested_web)
        || (requested_webgl)
        || (requested_webgpu);
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_types_feature_web)");
    if jadix_wgpu_types_feature_web {
        println!("cargo:rustc-cfg=jadix_wgpu_types_feature_web");
    }
    let jadix_wgpu_types_feature_web_sys = jadix_wgpu_feature_default;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_types_feature_web_sys)");
    if jadix_wgpu_types_feature_web_sys {
        println!("cargo:rustc-cfg=jadix_wgpu_types_feature_web_sys");
    }
    let jadix_wgpu_naga_bridge_enabled = (!target_target_arch_wasm32)
        || (target_target_os_emscripten)
        || (requested_noop)
        || (requested_webgl);
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_naga_bridge_enabled)");
    if jadix_wgpu_naga_bridge_enabled {
        println!("cargo:rustc-cfg=jadix_wgpu_naga_bridge_enabled");
    }
    let jadix_naga_enabled = (!target_target_arch_wasm32)
        || (target_target_os_emscripten)
        || (requested_glsl)
        || (requested_naga_ir)
        || (requested_noop)
        || (requested_spirv)
        || (requested_webgl);
    println!("cargo:rustc-check-cfg=cfg(jadix_naga_enabled)");
    if jadix_naga_enabled {
        println!("cargo:rustc-cfg=jadix_naga_enabled");
    }
    let jadix_naga_feature_arbitrary = jadix_wgpu_feature_default;
    println!("cargo:rustc-check-cfg=cfg(jadix_naga_feature_arbitrary)");
    if jadix_naga_feature_arbitrary {
        println!("cargo:rustc-cfg=jadix_naga_feature_arbitrary");
    }
    let jadix_naga_feature_default = (!target_target_arch_wasm32)
        || (target_target_os_emscripten)
        || (requested_glsl)
        || (requested_naga_ir)
        || (requested_noop)
        || (requested_spirv)
        || (requested_webgl);
    println!("cargo:rustc-check-cfg=cfg(jadix_naga_feature_default)");
    if jadix_naga_feature_default {
        println!("cargo:rustc-cfg=jadix_naga_feature_default");
    }
    let jadix_naga_feature_deserialize = jadix_wgpu_feature_default;
    println!("cargo:rustc-check-cfg=cfg(jadix_naga_feature_deserialize)");
    if jadix_naga_feature_deserialize {
        println!("cargo:rustc-cfg=jadix_naga_feature_deserialize");
    }
    let jadix_naga_feature_dot_out = jadix_wgpu_feature_default;
    println!("cargo:rustc-check-cfg=cfg(jadix_naga_feature_dot_out)");
    if jadix_naga_feature_dot_out {
        println!("cargo:rustc-cfg=jadix_naga_feature_dot_out");
    }
    let jadix_naga_feature_fs = jadix_wgpu_feature_default;
    println!("cargo:rustc-check-cfg=cfg(jadix_naga_feature_fs)");
    if jadix_naga_feature_fs {
        println!("cargo:rustc-cfg=jadix_naga_feature_fs");
    }
    let jadix_naga_feature_glsl_in = jadix_wgpu_feature_glsl;
    println!("cargo:rustc-check-cfg=cfg(jadix_naga_feature_glsl_in)");
    if jadix_naga_feature_glsl_in {
        println!("cargo:rustc-cfg=jadix_naga_feature_glsl_in");
    }
    let jadix_naga_feature_glsl_out = (!target_target_arch_wasm32
        && (requested_windows
            || target_target_os_linux
            || target_target_os_android
            || target_target_os_freebsd
            || target_target_os_netbsd)
        && (requested_windows
            || target_target_os_linux
            || target_target_os_android
            || target_target_os_freebsd)
        && requested_gles)
        || (!target_target_arch_wasm32 && target_target_vendor_apple && requested_angle)
        || (!target_target_arch_wasm32 && requested_gles_angle)
        || ((target_target_arch_wasm32 && !target_target_os_emscripten) && requested_webgl)
        || ((requested_windows
            || target_target_os_linux
            || target_target_os_android
            || target_target_os_freebsd
            || target_target_os_netbsd)
            && (requested_windows
                || target_target_os_linux
                || target_target_os_android
                || target_target_os_freebsd)
            && requested_gles
            && requested_noop)
        || ((requested_windows
            || target_target_os_linux
            || target_target_os_android
            || target_target_os_freebsd
            || target_target_os_netbsd)
            && (requested_windows
                || target_target_os_linux
                || target_target_os_android
                || target_target_os_freebsd)
            && requested_gles
            && requested_webgl)
        || (target_target_os_emscripten && target_target_vendor_apple && requested_angle)
        || (target_target_os_emscripten && requested_gles)
        || (target_target_os_emscripten && requested_gles_angle)
        || (target_target_vendor_apple && requested_angle && requested_noop)
        || (target_target_vendor_apple && requested_angle && requested_webgl)
        || (requested_gles_angle && requested_noop)
        || (requested_gles_angle && requested_webgl);
    println!("cargo:rustc-check-cfg=cfg(jadix_naga_feature_glsl_out)");
    if jadix_naga_feature_glsl_out {
        println!("cargo:rustc-cfg=jadix_naga_feature_glsl_out");
    }
    let jadix_naga_feature_hlsl_out = (!target_target_arch_wasm32
        && (requested_windows
            || target_target_os_linux
            || target_target_os_android
            || target_target_os_freebsd
            || target_target_os_netbsd)
        && (requested_windows
            || target_target_os_linux
            || target_target_os_android
            || target_target_os_freebsd)
        && requested_dx12)
        || (!target_target_arch_wasm32 && requested_dx11)
        || ((requested_windows
            || target_target_os_linux
            || target_target_os_android
            || target_target_os_freebsd
            || target_target_os_netbsd)
            && (requested_windows
                || target_target_os_linux
                || target_target_os_android
                || target_target_os_freebsd)
            && target_target_os_emscripten
            && requested_dx12)
        || ((requested_windows
            || target_target_os_linux
            || target_target_os_android
            || target_target_os_freebsd
            || target_target_os_netbsd)
            && (requested_windows
                || target_target_os_linux
                || target_target_os_android
                || target_target_os_freebsd)
            && requested_dx12
            && requested_noop)
        || ((requested_windows
            || target_target_os_linux
            || target_target_os_android
            || target_target_os_freebsd
            || target_target_os_netbsd)
            && (requested_windows
                || target_target_os_linux
                || target_target_os_android
                || target_target_os_freebsd)
            && requested_dx12
            && requested_webgl)
        || (target_target_os_emscripten && requested_dx11)
        || (requested_dx11 && requested_noop)
        || (requested_dx11 && requested_webgl);
    println!("cargo:rustc-check-cfg=cfg(jadix_naga_feature_hlsl_out)");
    if jadix_naga_feature_hlsl_out {
        println!("cargo:rustc-cfg=jadix_naga_feature_hlsl_out");
    }
    let jadix_naga_feature_hlsl_out_if_target_windows = jadix_wgpu_feature_default;
    println!("cargo:rustc-check-cfg=cfg(jadix_naga_feature_hlsl_out_if_target_windows)");
    if jadix_naga_feature_hlsl_out_if_target_windows {
        println!("cargo:rustc-cfg=jadix_naga_feature_hlsl_out_if_target_windows");
    }
    let jadix_naga_feature_msl_out = target_target_vendor_apple
        && requested_metal
        && ((!target_target_arch_wasm32)
            || (target_target_os_emscripten)
            || (requested_noop)
            || (requested_webgl));
    println!("cargo:rustc-check-cfg=cfg(jadix_naga_feature_msl_out)");
    if jadix_naga_feature_msl_out {
        println!("cargo:rustc-cfg=jadix_naga_feature_msl_out");
    }
    let jadix_naga_feature_msl_out_if_target_apple = jadix_wgpu_feature_default;
    println!("cargo:rustc-check-cfg=cfg(jadix_naga_feature_msl_out_if_target_apple)");
    if jadix_naga_feature_msl_out_if_target_apple {
        println!("cargo:rustc-cfg=jadix_naga_feature_msl_out_if_target_apple");
    }
    let jadix_naga_feature_petgraph = jadix_wgpu_feature_default;
    println!("cargo:rustc-check-cfg=cfg(jadix_naga_feature_petgraph)");
    if jadix_naga_feature_petgraph {
        println!("cargo:rustc-cfg=jadix_naga_feature_petgraph");
    }
    let jadix_naga_feature_pp_rs = jadix_wgpu_feature_default;
    println!("cargo:rustc-check-cfg=cfg(jadix_naga_feature_pp_rs)");
    if jadix_naga_feature_pp_rs {
        println!("cargo:rustc-cfg=jadix_naga_feature_pp_rs");
    }
    let jadix_naga_feature_serde = jadix_wgpu_feature_default;
    println!("cargo:rustc-check-cfg=cfg(jadix_naga_feature_serde)");
    if jadix_naga_feature_serde {
        println!("cargo:rustc-cfg=jadix_naga_feature_serde");
    }
    let jadix_naga_feature_serialize = requested_trace
        && ((!target_target_arch_wasm32)
            || (target_target_os_emscripten)
            || (requested_noop)
            || (requested_webgl));
    println!("cargo:rustc-check-cfg=cfg(jadix_naga_feature_serialize)");
    if jadix_naga_feature_serialize {
        println!("cargo:rustc-cfg=jadix_naga_feature_serialize");
    }
    let jadix_naga_feature_spirv = jadix_wgpu_feature_default;
    println!("cargo:rustc-check-cfg=cfg(jadix_naga_feature_spirv)");
    if jadix_naga_feature_spirv {
        println!("cargo:rustc-cfg=jadix_naga_feature_spirv");
    }
    let jadix_naga_feature_spv_in = jadix_wgpu_feature_spirv;
    println!("cargo:rustc-check-cfg=cfg(jadix_naga_feature_spv_in)");
    if jadix_naga_feature_spv_in {
        println!("cargo:rustc-cfg=jadix_naga_feature_spv_in");
    }
    let jadix_naga_feature_spv_out = (!target_target_arch_wasm32
        && (requested_windows
            || target_target_os_linux
            || target_target_os_android
            || target_target_os_freebsd
            || target_target_os_netbsd)
        && (requested_windows
            || target_target_os_linux
            || target_target_os_android
            || target_target_os_freebsd)
        && requested_vulkan)
        || (!target_target_arch_wasm32
            && target_target_vendor_apple
            && requested_vulkan_portability)
        || ((requested_windows
            || target_target_os_linux
            || target_target_os_android
            || target_target_os_freebsd
            || target_target_os_netbsd)
            && (requested_windows
                || target_target_os_linux
                || target_target_os_android
                || target_target_os_freebsd)
            && target_target_os_emscripten
            && requested_vulkan)
        || ((requested_windows
            || target_target_os_linux
            || target_target_os_android
            || target_target_os_freebsd
            || target_target_os_netbsd)
            && (requested_windows
                || target_target_os_linux
                || target_target_os_android
                || target_target_os_freebsd)
            && requested_noop
            && requested_vulkan)
        || ((requested_windows
            || target_target_os_linux
            || target_target_os_android
            || target_target_os_freebsd
            || target_target_os_netbsd)
            && (requested_windows
                || target_target_os_linux
                || target_target_os_android
                || target_target_os_freebsd)
            && requested_vulkan
            && requested_webgl)
        || (target_target_os_emscripten
            && target_target_vendor_apple
            && requested_vulkan_portability)
        || (target_target_vendor_apple && requested_noop && requested_vulkan_portability)
        || (target_target_vendor_apple && requested_vulkan_portability && requested_webgl);
    println!("cargo:rustc-check-cfg=cfg(jadix_naga_feature_spv_out)");
    if jadix_naga_feature_spv_out {
        println!("cargo:rustc-cfg=jadix_naga_feature_spv_out");
    }
    let jadix_naga_feature_stderr = jadix_wgpu_feature_default;
    println!("cargo:rustc-check-cfg=cfg(jadix_naga_feature_stderr)");
    if jadix_naga_feature_stderr {
        println!("cargo:rustc-cfg=jadix_naga_feature_stderr");
    }
    let jadix_naga_feature_termcolor = (requested_glsl) || (requested_naga_ir) || (requested_spirv);
    println!("cargo:rustc-check-cfg=cfg(jadix_naga_feature_termcolor)");
    if jadix_naga_feature_termcolor {
        println!("cargo:rustc-cfg=jadix_naga_feature_termcolor");
    }
    let jadix_naga_feature_unicode_ident = jadix_wgpu_feature_default;
    println!("cargo:rustc-check-cfg=cfg(jadix_naga_feature_unicode_ident)");
    if jadix_naga_feature_unicode_ident {
        println!("cargo:rustc-cfg=jadix_naga_feature_unicode_ident");
    }
    let jadix_naga_feature_wgsl_in = (!target_target_arch_wasm32)
        || (target_target_os_emscripten && requested_wgsl)
        || (requested_noop && requested_wgsl)
        || (requested_webgl && requested_wgsl);
    println!("cargo:rustc-check-cfg=cfg(jadix_naga_feature_wgsl_in)");
    if jadix_naga_feature_wgsl_in {
        println!("cargo:rustc-cfg=jadix_naga_feature_wgsl_in");
    }
    let jadix_naga_feature_wgsl_out =
        requested_webgpu && ((requested_glsl) || (requested_naga_ir) || (requested_spirv));
    println!("cargo:rustc-check-cfg=cfg(jadix_naga_feature_wgsl_out)");
    if jadix_naga_feature_wgsl_out {
        println!("cargo:rustc-cfg=jadix_naga_feature_wgsl_out");
    }
    let jadix_wgpu_core_deps_apple_enabled = target_target_vendor_apple
        && ((!target_target_arch_wasm32 && requested_angle)
            || (!target_target_arch_wasm32 && requested_metal)
            || (!target_target_arch_wasm32 && requested_vulkan_portability)
            || (target_target_os_emscripten && requested_angle)
            || (target_target_os_emscripten && requested_metal)
            || (target_target_os_emscripten && requested_vulkan_portability)
            || (requested_angle && requested_noop)
            || (requested_angle && requested_webgl)
            || (requested_metal && requested_noop)
            || (requested_metal && requested_webgl)
            || (requested_noop && requested_vulkan_portability)
            || (requested_vulkan_portability && requested_webgl));
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_core_deps_apple_enabled)");
    if jadix_wgpu_core_deps_apple_enabled {
        println!("cargo:rustc-cfg=jadix_wgpu_core_deps_apple_enabled");
    }
    let jadix_wgpu_core_deps_apple_feature_angle = target_target_vendor_apple
        && requested_angle
        && ((!target_target_arch_wasm32)
            || (target_target_os_emscripten)
            || (requested_noop)
            || (requested_webgl));
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_core_deps_apple_feature_angle)");
    if jadix_wgpu_core_deps_apple_feature_angle {
        println!("cargo:rustc-cfg=jadix_wgpu_core_deps_apple_feature_angle");
    }
    let jadix_wgpu_core_deps_apple_feature_metal = target_target_vendor_apple
        && requested_metal
        && ((!target_target_arch_wasm32)
            || (target_target_os_emscripten)
            || (requested_noop)
            || (requested_webgl));
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_core_deps_apple_feature_metal)");
    if jadix_wgpu_core_deps_apple_feature_metal {
        println!("cargo:rustc-cfg=jadix_wgpu_core_deps_apple_feature_metal");
    }
    let jadix_wgpu_core_deps_apple_feature_vulkan_portability = target_target_vendor_apple
        && requested_vulkan_portability
        && ((!target_target_arch_wasm32)
            || (target_target_os_emscripten)
            || (requested_noop)
            || (requested_webgl));
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_core_deps_apple_feature_vulkan_portability)");
    if jadix_wgpu_core_deps_apple_feature_vulkan_portability {
        println!("cargo:rustc-cfg=jadix_wgpu_core_deps_apple_feature_vulkan_portability");
    }
    let jadix_wgpu_core_deps_emscripten_enabled = target_target_os_emscripten && requested_gles;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_core_deps_emscripten_enabled)");
    if jadix_wgpu_core_deps_emscripten_enabled {
        println!("cargo:rustc-cfg=jadix_wgpu_core_deps_emscripten_enabled");
    }
    let jadix_wgpu_core_deps_emscripten_feature_gles =
        target_target_os_emscripten && requested_gles;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_core_deps_emscripten_feature_gles)");
    if jadix_wgpu_core_deps_emscripten_feature_gles {
        println!("cargo:rustc-cfg=jadix_wgpu_core_deps_emscripten_feature_gles");
    }
    let jadix_wgpu_core_deps_wasm_enabled =
        (target_target_arch_wasm32 && !target_target_os_emscripten) && requested_webgl;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_core_deps_wasm_enabled)");
    if jadix_wgpu_core_deps_wasm_enabled {
        println!("cargo:rustc-cfg=jadix_wgpu_core_deps_wasm_enabled");
    }
    let jadix_wgpu_core_deps_wasm_feature_webgl =
        (target_target_arch_wasm32 && !target_target_os_emscripten) && requested_webgl;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_core_deps_wasm_feature_webgl)");
    if jadix_wgpu_core_deps_wasm_feature_webgl {
        println!("cargo:rustc-cfg=jadix_wgpu_core_deps_wasm_feature_webgl");
    }
    let jadix_wgpu_core_deps_windows_linux_android_enabled = (requested_windows
        || target_target_os_linux
        || target_target_os_android
        || target_target_os_freebsd)
        && ((!target_target_arch_wasm32)
            || (target_target_os_emscripten && requested_drm)
            || (target_target_os_emscripten && requested_dx12)
            || (target_target_os_emscripten && requested_gles)
            || (target_target_os_emscripten && requested_vulkan)
            || (requested_drm && requested_noop)
            || (requested_drm && requested_webgl)
            || (requested_dx12 && requested_noop)
            || (requested_dx12 && requested_webgl)
            || (requested_gles && requested_noop)
            || (requested_gles && requested_webgl)
            || (requested_noop && requested_vulkan)
            || (requested_vulkan && requested_webgl));
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_core_deps_windows_linux_android_enabled)");
    if jadix_wgpu_core_deps_windows_linux_android_enabled {
        println!("cargo:rustc-cfg=jadix_wgpu_core_deps_windows_linux_android_enabled");
    }
    let jadix_wgpu_core_deps_windows_linux_android_feature_drm = (requested_windows
        || target_target_os_linux
        || target_target_os_android
        || target_target_os_freebsd)
        && requested_drm
        && ((!target_target_arch_wasm32)
            || (target_target_os_emscripten)
            || (requested_noop)
            || (requested_webgl));
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_core_deps_windows_linux_android_feature_drm)");
    if jadix_wgpu_core_deps_windows_linux_android_feature_drm {
        println!("cargo:rustc-cfg=jadix_wgpu_core_deps_windows_linux_android_feature_drm");
    }
    let jadix_wgpu_core_deps_windows_linux_android_feature_dx12 = (requested_windows
        || target_target_os_linux
        || target_target_os_android
        || target_target_os_freebsd)
        && requested_dx12
        && ((!target_target_arch_wasm32)
            || (target_target_os_emscripten)
            || (requested_noop)
            || (requested_webgl));
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_core_deps_windows_linux_android_feature_dx12)");
    if jadix_wgpu_core_deps_windows_linux_android_feature_dx12 {
        println!("cargo:rustc-cfg=jadix_wgpu_core_deps_windows_linux_android_feature_dx12");
    }
    let jadix_wgpu_core_deps_windows_linux_android_feature_gles = (requested_windows
        || target_target_os_linux
        || target_target_os_android
        || target_target_os_freebsd)
        && requested_gles
        && ((!target_target_arch_wasm32)
            || (target_target_os_emscripten)
            || (requested_noop)
            || (requested_webgl));
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_core_deps_windows_linux_android_feature_gles)");
    if jadix_wgpu_core_deps_windows_linux_android_feature_gles {
        println!("cargo:rustc-cfg=jadix_wgpu_core_deps_windows_linux_android_feature_gles");
    }
    let jadix_wgpu_core_deps_windows_linux_android_feature_renderdoc = !target_target_arch_wasm32
        && (requested_windows
            || target_target_os_linux
            || target_target_os_android
            || target_target_os_freebsd);
    println!(
        "cargo:rustc-check-cfg=cfg(jadix_wgpu_core_deps_windows_linux_android_feature_renderdoc)"
    );
    if jadix_wgpu_core_deps_windows_linux_android_feature_renderdoc {
        println!("cargo:rustc-cfg=jadix_wgpu_core_deps_windows_linux_android_feature_renderdoc");
    }
    let jadix_wgpu_core_deps_windows_linux_android_feature_vulkan = (requested_windows
        || target_target_os_linux
        || target_target_os_android
        || target_target_os_freebsd)
        && requested_vulkan
        && ((!target_target_arch_wasm32)
            || (target_target_os_emscripten)
            || (requested_noop)
            || (requested_webgl));
    println!(
        "cargo:rustc-check-cfg=cfg(jadix_wgpu_core_deps_windows_linux_android_feature_vulkan)"
    );
    if jadix_wgpu_core_deps_windows_linux_android_feature_vulkan {
        println!("cargo:rustc-cfg=jadix_wgpu_core_deps_windows_linux_android_feature_vulkan");
    }
    let jadix_wgpu_web =
        !jadix_wgpu_emscripten && target_target_arch_wasm32 && jadix_wgpu_feature_web;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_web)");
    if jadix_wgpu_web {
        println!("cargo:rustc-cfg=jadix_wgpu_web");
    }
    let jadix_wgpu_send_sync = (!target_target_feature_atomics
        && jadix_wgpu_feature_fragile_send_sync_non_atomic_wasm)
        || (jadix_wgpu_native);
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_send_sync)");
    if jadix_wgpu_send_sync {
        println!("cargo:rustc-cfg=jadix_wgpu_send_sync");
    }
    let jadix_wgpu_webgpu =
        !jadix_wgpu_emscripten && !jadix_wgpu_native && jadix_wgpu_feature_webgpu;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_webgpu)");
    if jadix_wgpu_webgpu {
        println!("cargo:rustc-cfg=jadix_wgpu_webgpu");
    }
    let jadix_wgpu_webgl = !jadix_wgpu_emscripten && !jadix_wgpu_native && jadix_wgpu_feature_webgl;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_webgl)");
    if jadix_wgpu_webgl {
        println!("cargo:rustc-cfg=jadix_wgpu_webgl");
    }
    let jadix_wgpu_dx12 = target_target_os_windows && jadix_wgpu_feature_dx12;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_dx12)");
    if jadix_wgpu_dx12 {
        println!("cargo:rustc-cfg=jadix_wgpu_dx12");
    }
    let jadix_wgpu_metal = target_target_vendor_apple && jadix_wgpu_feature_metal;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_metal)");
    if jadix_wgpu_metal {
        println!("cargo:rustc-cfg=jadix_wgpu_metal");
    }
    let jadix_wgpu_vulkan = ((requested_windows
        || target_target_os_linux
        || target_target_os_android
        || target_target_os_freebsd)
        && jadix_wgpu_feature_vulkan)
        || (target_target_vendor_apple && jadix_wgpu_feature_vulkan_portability);
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_vulkan)");
    if jadix_wgpu_vulkan {
        println!("cargo:rustc-cfg=jadix_wgpu_vulkan");
    }
    let jadix_wgpu_drm = (target_target_os_linux
        || target_target_os_freebsd
        || target_target_os_netbsd
        || target_target_os_openbsd)
        && jadix_wgpu_feature_drm;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_drm)");
    if jadix_wgpu_drm {
        println!("cargo:rustc-cfg=jadix_wgpu_drm");
    }
    let jadix_wgpu_gles = ((requested_windows
        || target_target_os_linux
        || target_target_os_android
        || target_target_os_freebsd
        || jadix_wgpu_emscripten)
        && jadix_wgpu_feature_gles)
        || (target_target_vendor_apple && jadix_wgpu_feature_angle);
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_gles)");
    if jadix_wgpu_gles {
        println!("cargo:rustc-cfg=jadix_wgpu_gles");
    }
    let jadix_wgpu_noop = jadix_wgpu_feature_noop;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_noop)");
    if jadix_wgpu_noop {
        println!("cargo:rustc-cfg=jadix_wgpu_noop");
    }
    let jadix_wgpu_naga =
        (jadix_wgpu_feature_glsl) || (jadix_wgpu_feature_naga_ir) || (jadix_wgpu_feature_spirv);
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_naga)");
    if jadix_wgpu_naga {
        println!("cargo:rustc-cfg=jadix_wgpu_naga");
    }
    let jadix_wgpu_static_dxc = !target_target_arch_aarch64
        && target_target_env_msvc
        && target_target_os_windows
        && jadix_wgpu_feature_static_dxc;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_static_dxc)");
    if jadix_wgpu_static_dxc {
        println!("cargo:rustc-cfg=jadix_wgpu_static_dxc");
    }
    let jadix_wgpu_custom = jadix_wgpu_feature_custom;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_custom)");
    if jadix_wgpu_custom {
        println!("cargo:rustc-cfg=jadix_wgpu_custom");
    }
    let jadix_wgpu_core_send_sync = (!target_target_arch_wasm32
        || (jadix_wgpu_core_feature_fragile_send_sync_non_atomic_wasm
            && !target_target_feature_atomics))
        && jadix_wgpu_core_feature_std;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_core_send_sync)");
    if jadix_wgpu_core_send_sync {
        println!("cargo:rustc-cfg=jadix_wgpu_core_send_sync");
    }
    let jadix_wgpu_core_dx12 = target_target_os_windows && jadix_wgpu_core_feature_dx12;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_core_dx12)");
    if jadix_wgpu_core_dx12 {
        println!("cargo:rustc-cfg=jadix_wgpu_core_dx12");
    }
    let jadix_wgpu_core_webgl =
        !target_target_os_emscripten && target_target_arch_wasm32 && jadix_wgpu_core_feature_webgl;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_core_webgl)");
    if jadix_wgpu_core_webgl {
        println!("cargo:rustc-cfg=jadix_wgpu_core_webgl");
    }
    let jadix_wgpu_core_vulkan = (target_target_vendor_apple
        && jadix_wgpu_core_feature_vulkan_portability)
        || (jadix_wgpu_core_feature_vulkan && jadix_wgpu_core_windows_linux_android);
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_core_vulkan)");
    if jadix_wgpu_core_vulkan {
        println!("cargo:rustc-cfg=jadix_wgpu_core_vulkan");
    }
    let jadix_wgpu_core_drm = (target_target_os_linux
        || target_target_os_freebsd
        || target_target_os_netbsd
        || target_target_os_openbsd)
        && jadix_wgpu_core_feature_drm;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_core_drm)");
    if jadix_wgpu_core_drm {
        println!("cargo:rustc-cfg=jadix_wgpu_core_drm");
    }
    let jadix_wgpu_core_metal = target_target_vendor_apple && jadix_wgpu_core_feature_metal;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_core_metal)");
    if jadix_wgpu_core_metal {
        println!("cargo:rustc-cfg=jadix_wgpu_core_metal");
    }
    let jadix_wgpu_hal_send_sync = (!target_target_arch_wasm32)
        || (!target_target_feature_atomics
            && jadix_wgpu_hal_feature_fragile_send_sync_non_atomic_wasm);
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_hal_send_sync)");
    if jadix_wgpu_hal_send_sync {
        println!("cargo:rustc-cfg=jadix_wgpu_hal_send_sync");
    }
    let jadix_wgpu_hal_dx12 = target_target_os_windows && jadix_wgpu_hal_feature_dx12;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_hal_dx12)");
    if jadix_wgpu_hal_dx12 {
        println!("cargo:rustc-cfg=jadix_wgpu_hal_dx12");
    }
    let jadix_wgpu_hal_gles = jadix_wgpu_hal_feature_gles;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_hal_gles)");
    if jadix_wgpu_hal_gles {
        println!("cargo:rustc-cfg=jadix_wgpu_hal_gles");
    }
    let jadix_wgpu_hal_gles_with_std = (!target_target_arch_wasm32
        || (target_target_vendor_unknown && target_target_os_unknown)
        || target_target_os_emscripten)
        && jadix_wgpu_hal_feature_gles;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_hal_gles_with_std)");
    if jadix_wgpu_hal_gles_with_std {
        println!("cargo:rustc-cfg=jadix_wgpu_hal_gles_with_std");
    }
    let jadix_wgpu_hal_metal = target_target_vendor_apple && jadix_wgpu_hal_feature_metal;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_hal_metal)");
    if jadix_wgpu_hal_metal {
        println!("cargo:rustc-cfg=jadix_wgpu_hal_metal");
    }
    let jadix_wgpu_hal_vulkan = !target_target_arch_wasm32 && jadix_wgpu_hal_feature_vulkan;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_hal_vulkan)");
    if jadix_wgpu_hal_vulkan {
        println!("cargo:rustc-cfg=jadix_wgpu_hal_vulkan");
    }
    let jadix_wgpu_hal_drm = (target_target_os_linux
        || target_target_os_freebsd
        || target_target_os_netbsd
        || target_target_os_openbsd)
        && jadix_wgpu_hal_feature_drm;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_hal_drm)");
    if jadix_wgpu_hal_drm {
        println!("cargo:rustc-cfg=jadix_wgpu_hal_drm");
    }
    let jadix_wgpu_hal_static_dxc = !target_target_arch_aarch64
        && target_target_env_msvc
        && target_target_os_windows
        && jadix_wgpu_hal_feature_static_dxc;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_hal_static_dxc)");
    if jadix_wgpu_hal_static_dxc {
        println!("cargo:rustc-cfg=jadix_wgpu_hal_static_dxc");
    }
    let jadix_naga_dot_out = jadix_naga_feature_dot_out;
    println!("cargo:rustc-check-cfg=cfg(jadix_naga_dot_out)");
    if jadix_naga_dot_out {
        println!("cargo:rustc-cfg=jadix_naga_dot_out");
    }
    let jadix_naga_glsl_out = jadix_naga_feature_glsl_out;
    println!("cargo:rustc-check-cfg=cfg(jadix_naga_glsl_out)");
    if jadix_naga_glsl_out {
        println!("cargo:rustc-cfg=jadix_naga_glsl_out");
    }
    let jadix_naga_hlsl_out = (target_target_os_windows
        && jadix_naga_feature_hlsl_out_if_target_windows)
        || (jadix_naga_feature_hlsl_out);
    println!("cargo:rustc-check-cfg=cfg(jadix_naga_hlsl_out)");
    if jadix_naga_hlsl_out {
        println!("cargo:rustc-cfg=jadix_naga_hlsl_out");
    }
    let jadix_naga_msl_out = (target_target_vendor_apple
        && jadix_naga_feature_msl_out_if_target_apple)
        || (jadix_naga_feature_msl_out);
    println!("cargo:rustc-check-cfg=cfg(jadix_naga_msl_out)");
    if jadix_naga_msl_out {
        println!("cargo:rustc-cfg=jadix_naga_msl_out");
    }
    let jadix_naga_spv_out = jadix_naga_feature_spv_out;
    println!("cargo:rustc-check-cfg=cfg(jadix_naga_spv_out)");
    if jadix_naga_spv_out {
        println!("cargo:rustc-cfg=jadix_naga_spv_out");
    }
    let jadix_naga_wgsl_out = jadix_naga_feature_wgsl_out;
    println!("cargo:rustc-check-cfg=cfg(jadix_naga_wgsl_out)");
    if jadix_naga_wgsl_out {
        println!("cargo:rustc-cfg=jadix_naga_wgsl_out");
    }
    let jadix_naga_std = (jadix_naga_feature_fs) || (jadix_naga_feature_stderr) || (requested_test);
    println!("cargo:rustc-check-cfg=cfg(jadix_naga_std)");
    if jadix_naga_std {
        println!("cargo:rustc-cfg=jadix_naga_std");
    }
    let jadix_wgpu_wgpu_core = (jadix_wgpu_dx12)
        || (jadix_wgpu_gles)
        || (jadix_wgpu_metal)
        || (jadix_wgpu_native)
        || (jadix_wgpu_noop)
        || (jadix_wgpu_vulkan)
        || (jadix_wgpu_webgl);
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_wgpu_core)");
    if jadix_wgpu_wgpu_core {
        println!("cargo:rustc-cfg=jadix_wgpu_wgpu_core");
    }
    let jadix_wgpu_std =
        (target_panic_unwind) || (jadix_wgpu_feature_std) || (jadix_wgpu_send_sync);
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_std)");
    if jadix_wgpu_std {
        println!("cargo:rustc-cfg=jadix_wgpu_std");
    }
    let jadix_wgpu_core_gles = (target_target_os_emscripten && jadix_wgpu_core_feature_gles)
        || (target_target_vendor_apple && jadix_wgpu_core_feature_angle)
        || (jadix_wgpu_core_feature_gles && jadix_wgpu_core_windows_linux_android)
        || (jadix_wgpu_core_webgl);
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_core_gles)");
    if jadix_wgpu_core_gles {
        println!("cargo:rustc-cfg=jadix_wgpu_core_gles");
    }
    let jadix_wgpu_hal_webgl =
        !target_target_os_emscripten && target_target_arch_wasm32 && jadix_wgpu_hal_gles;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_hal_webgl)");
    if jadix_wgpu_hal_webgl {
        println!("cargo:rustc-cfg=jadix_wgpu_hal_webgl");
    }
    let jadix_wgpu_hal_emscripten = target_target_os_emscripten && jadix_wgpu_hal_gles;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_hal_emscripten)");
    if jadix_wgpu_hal_emscripten {
        println!("cargo:rustc-cfg=jadix_wgpu_hal_emscripten");
    }
    let jadix_wgpu_hal_any_backend = (jadix_wgpu_hal_dx12)
        || (jadix_wgpu_hal_gles)
        || (jadix_wgpu_hal_metal)
        || (jadix_wgpu_hal_vulkan);
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_hal_any_backend)");
    if jadix_wgpu_hal_any_backend {
        println!("cargo:rustc-cfg=jadix_wgpu_hal_any_backend");
    }
    let jadix_naga_no_std = !jadix_naga_std;
    println!("cargo:rustc-check-cfg=cfg(jadix_naga_no_std)");
    if jadix_naga_no_std {
        println!("cargo:rustc-cfg=jadix_naga_no_std");
    }
    let jadix_wgpu_no_std = !jadix_wgpu_std;
    println!("cargo:rustc-check-cfg=cfg(jadix_wgpu_no_std)");
    if jadix_wgpu_no_std {
        println!("cargo:rustc-cfg=jadix_wgpu_no_std");
    }
}
