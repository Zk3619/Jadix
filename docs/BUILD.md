# Building and packaging Jadix

[English](BUILD.md) | [简体中文](BUILD.zh.md) | [Documentation index](README.md)

This guide covers SDK preparation, building the workspace and Gallery, application packaging, and installation. Run commands from the repository root unless another directory is specified.

## Platforms and architectures

| Platform | Rust targets | Build host and deliverable |
| --- | --- | --- |
| Windows | `x86_64-pc-windows-msvc`, `aarch64-pc-windows-msvc` | Windows with MSVC/Windows SDK; executable plus resources |
| macOS | `aarch64-apple-darwin`, `x86_64-apple-darwin` | macOS with Xcode; executable, then an application bundle |
| Linux | `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu` | Linux or WSL with target system libraries; executable/resource archive |
| Android | `aarch64-linux-android`, `x86_64-linux-android` | Current packaging script runs on Windows with Android SDK/NDK/JDK; APK |
| iOS | `aarch64-apple-ios`, `aarch64-apple-ios-sim`, `x86_64-apple-ios` | macOS with Xcode and an iOS application host; `.app`/IPA as appropriate |
| OHOS phone/tablet | `aarch64-unknown-linux-ohos`, `x86_64-unknown-linux-ohos` | Current packaging script runs on Windows with DevEco SDK; HAP |
| OHOS desktop/2in1 | Same OHOS targets | Separate desktop feature/configuration; HAP |
| Web | `wasm32-unknown-unknown` | Any Rust build host; JavaScript + Wasm + static HTTP(S) hosting |

An x86_64 package is useful for a matching emulator; it is not a substitute for an ARM64 device package. OHOS phone and desktop builds have different application configuration and native features even when their CPU architecture matches.

## Shared preparation

Install Git, rustup and a recent stable Rust toolchain. The workspace has no single pinned toolchain or declared workspace MSRV: respect the `rust-version` of the entire dependency graph, including maintenance forks (some currently require Rust 1.89). Install each target with `rustup target add`; this installs Rust's target libraries, not the platform SDK, linker, system libraries or signing tools.

```sh
git clone https://github.com/Zk3619/Jadix.git
cd Jadix
rustup update stable
cargo fetch --locked
cargo run -p gallery
```

`cargo run` requires the native dependencies for the host described below. Preserve `Cargo.lock` and the selected `vendor/` forks. `--locked` intentionally fails if a manifest change requires a lockfile update; review that change rather than silently dropping the flag.

The [Gallery manifest](../examples/gallery/Cargo.toml) defaults to `full`, which enables the showcase's optional capabilities, including 3D, document/media integrations, WebView, runtime/network adapters and development tools. The mobile library forwards that feature. Feature selection does not guarantee that every platform service is available at runtime. `ohos-desktop` additionally enables desktop window state on OHOS; do not enable it for an OHOS phone. Android and iOS do not compile `jadix_window_state` through the Gallery dependency graph. See [features](FEATURES.md) and [WebView](webview.md) for capability boundaries.

Release builds enable fat LTO and use one codegen unit; linking can require substantial time, RAM and disk space. Reduce `CARGO_BUILD_JOBS` on constrained machines. Desktop executables need `theme.toml`, `styles.toml` and `assets/` from `examples/gallery` in their resource working directory. The mobile entries call `prepare_resources` in an application-writable directory; preserve that initialization when creating a new host.

## Windows

Install Visual Studio Build Tools with Desktop development with C++, the MSVC toolchain and a Windows SDK. ARM64 builds also need the ARM64 C++ tools/libraries. Use a developer PowerShell if compiler/SDK discovery fails. The Gallery build script embeds the application icon using the SDK resource compiler; `RC` can select a specific resource compiler when automatic discovery fails.

```powershell
rustup target add x86_64-pc-windows-msvc aarch64-pc-windows-msvc
cargo build -p gallery --bin gallery --release --locked --target x86_64-pc-windows-msvc
cargo build -p gallery --bin gallery --release --locked --target aarch64-pc-windows-msvc
```

Outputs are `target/<target>/release/gallery.exe`. Package each architecture separately with the Gallery theme, styles and assets, and start it with that directory as the working directory. MSVC runtime redistribution, installer generation and executable/installer signing are distribution work. Check the architecture and redistributable requirements of native dependencies before shipping ARM64.

If `link.exe`, Windows SDK libraries or `rc.exe` are missing, repair the C++/SDK installation rather than mixing MSVC and GNU libraries. A release GUI application has no console by default: set `RUST_LOG` and `JADIX_LOG_FILE` when collecting startup failures. Check GPU drivers/backend initialization and resource paths for a black window. Embedded icon changes may also require refreshing the shell's icon cache. WebView availability depends on the platform WebView runtime; see the [integration guide](webview.md).

## macOS

Use macOS with full Xcode, select its developer directory, accept its license and install the required SDKs. Check `xcode-select -p` and `xcrun --sdk macosx --show-sdk-path`. Set `SDKROOT` or `MACOSX_DEPLOYMENT_TARGET` only to values consistent with the selected SDK and all native dependencies; Rust's minimum supported OS is not a guarantee for the complete Gallery.

```sh
rustup target add aarch64-apple-darwin x86_64-apple-darwin
cargo build -p gallery --bin gallery --release --locked --target aarch64-apple-darwin
cargo build -p gallery --bin gallery --release --locked --target x86_64-apple-darwin
```

These commands produce `target/<target>/release/gallery`, not a ready-to-distribute `.app`. They need execution and validation on a macOS build host. To finish the product package:

1. Create `Jadix Gallery.app/Contents/MacOS/gallery`, an `Info.plist` with the executable, bundle identifier and minimum OS, and the icon/resources under `Contents/Resources`. The manifest's bundle metadata alone does not assemble this bundle.
2. Resolve bundled resources independently of the terminal's working directory. Store generated files and window state in a writable application-data directory; an installed app bundle must not be used as a writable working directory.
3. If distributing Universal 2, build both macOS architectures and merge compatible executables with `lipo`, checking all bundled native libraries. Sign after final bundle assembly.
4. Configure signing, entitlements, archive/export and notarization according to the distribution channel. Local ad hoc signing is not an App Store or Developer ID distribution identity.
5. Verify Finder launch, Metal rendering, input/IME, scaling, media/WebView and suspend/resume on the intended hardware and OS versions.

Wrong-architecture libraries, a stale SDK selection, inconsistent deployment targets and unsigned/modified nested code are common build or launch failures. A Windows cross-link alone cannot validate this packaging workflow.

## Linux

Full Gallery builds require GTK 3, WebKitGTK 4.1, X11/Wayland and graphics development libraries. On Ubuntu 24.04, a starting set is:

```sh
sudo apt update
sudo apt install build-essential clang cmake pkg-config libx11-dev libxkbcommon-dev libxkbcommon-x11-dev libwayland-dev libgtk-3-dev libwebkit2gtk-4.1-dev libvulkan-dev mesa-vulkan-drivers libegl1-mesa-dev libgl1-mesa-dev fonts-noto-cjk
rustup target add x86_64-unknown-linux-gnu
cargo build -p gallery --bin gallery --release --locked --target x86_64-unknown-linux-gnu
```

Use `aarch64-unknown-linux-gnu` on an ARM64 build host with matching dependencies. Package the executable with the Gallery resources. The maintained [Linux packaging script](../examples/gallery/build-linux.sh) uses cargo-zigbuild and builds with `--all-features`:

```sh
cargo install cargo-zigbuild --locked
zig version
bash examples/gallery/build-linux.sh x86_64-unknown-linux-gnu.2.39
```

Install Zig separately and put it on `PATH`. The script produces an executable/resource directory and `jadix-gallery-x86_64-unknown-linux-gnu.2.39-release.tar.gz` with a checksum under `examples/gallery_mobile/dist/linux/`; `GALLERY_OUTPUT_DIR` overrides the output directory. `.2.39` is a cargo-zigbuild glibc selector, not part of the target passed to `rustup`. The current package baseline is Ubuntu 24.04/glibc 2.39. Choosing an older suffix does not make newer GTK/WebKit system libraries compatible with older distributions.

For cross-compiling ARM64, install `aarch64-unknown-linux-gnu` and provide an ARM64 sysroot containing GTK/WebKit and their dependencies. Configure `PKG_CONFIG_SYSROOT_DIR`, `PKG_CONFIG_LIBDIR` and, when appropriate, `PKG_CONFIG_ALLOW_CROSS` for that sysroot before invoking the script with `aarch64-unknown-linux-gnu.2.39`. Do not resolve host `.pc` files into target libraries. Zig supplies neither the GUI sysroot nor a portable fully static musl version of the full Gallery.

Runtime packages and WSLg details are in [the Linux runtime notes](../examples/gallery/LINUX.md). In particular:

- Install GTK/WebKit runtimes, `libxkbcommon-x11`, fonts and a working GPU driver. A dynamically loaded library may be absent without appearing in `ldd` output.
- Launch inside an X11/Wayland session; SSH/headless environments need their own display arrangement. With WSLg, `env -u WAYLAND_DISPLAY ./gallery` can select X11 when the Wayland path fails.
- `JADIX_GPU_BACKEND=vulkan ./gallery` explicitly selects Vulkan for diagnosis when supported by the driver.
- Tray support depends on desktop services such as StatusNotifierItem. An executable link or an available feature does not prove that the current desktop exposes a tray.

## Android

Use the [Android build script](../examples/gallery_mobile/build-android.ps1) with PowerShell 7 on Windows. Install Android SDK platform 35 or later, build-tools (`aapt2`, `zipalign`, `apksigner`), platform-tools (`adb`), an NDK containing both requested ABIs, JDK 17, Zig and cargo-zigbuild. The exercised NDK was 29.0.14206865; keep SDK/NDK changes explicit when reproducing a failure. The manifest currently uses minimum API 28 and target API 35.

```powershell
$env:ANDROID_HOME = 'C:/Android/Sdk'
$env:ANDROID_NDK_HOME = "$env:ANDROID_HOME/ndk/29.0.14206865"
$env:JAVA_HOME = 'C:/Java/jdk-17'
$env:PATH = "$env:ANDROID_HOME/platform-tools;$env:PATH"
rustup target add aarch64-linux-android x86_64-linux-android
cargo install cargo-zigbuild --locked
zig version
pwsh -File examples/gallery_mobile/build-android.ps1 -Architecture arm64
pwsh -File examples/gallery_mobile/build-android.ps1 -Architecture x86_64
```

Replace example SDK/JDK paths with your installations. `NDK_HOME` is a fallback for `ANDROID_NDK_HOME`. The script uses the NDK's `windows-x86_64` host tools and Windows wrappers: it needs adaptation to run on a Unix host, even if PowerShell is installed there. Leave `CARGO_TARGET_DIR` unset for the mobile scripts, which read the workspace's `target/` directory.

The script builds `gallery_mobile` as a release `cdylib` with the full Gallery, packages the native library and Android resources, aligns the APK and signs it with a local test key. This uses NDK/Bionic settings; substituting a GNU/Linux or musl target is invalid.

| Architecture | Rust target / APK ABI | Default output |
| --- | --- | --- |
| ARM64 | `aarch64-linux-android` / `arm64-v8a` | `examples/gallery_mobile/dist/android-arm64/jadix-gallery-android-arm64-release.apk` |
| x86_64 | `x86_64-linux-android` / `x86_64` | `examples/gallery_mobile/dist/android-x86_64/jadix-gallery-android-x86_64-release.apk` |

`-OutputDirectory` changes the package destination. `-SkipBuild` repackages an existing native library; it does not verify that the library includes your latest source or feature changes. Keep the generated `local-gallery-release.p12` private and outside version control. These test keys and their public script passwords are unsuitable for production. Separate output directories can generate different keys; replacing an installed package signed by another key requires reusing its signing identity or deliberately uninstalling it, which deletes application data. A production signing workflow, store metadata and an AAB pipeline remain separate work.

The current APK packager does not compile the WebView Java helper into DEX. To use Android WebView, add `JadixWebViewHost.java` to a Java-enabled application host/build pipeline and retain it during minification, following the [WebView integration guide](webview.md). Enabling `full` alone does not complete that integration.

To install and collect startup evidence on a selected device/emulator:

```powershell
adb devices -l
pwsh -File examples/gallery_mobile/verify-android.ps1 -Serial emulator-5554 -Apk examples/gallery_mobile/dist/android-x86_64/jadix-gallery-android-x86_64-release.apk
```

Replace the serial with an actual `adb devices` entry and select a matching ABI. The verifier installs, launches, checks process/foreground state and saves logs and a screenshot. A nonuniform screenshot is only a smoke check: inspect it and exercise navigation, text input, scrolling and animated 3D content manually. Hardware-independent emulation does not establish physical-device GPU correctness.

| Symptom | Investigation and corrective work |
| --- | --- |
| White screen followed by exit | Capture startup `logcat`, crash buffer and process state. Diagnose native initialization, panic/tombstone, GPU limits and resource setup before changing UI content. |
| GLES validation or black output | Mobile startup uses constrained GLES capabilities; preserve the mobile limits and compatible clip texture formats. Test the emulator's software GLES mode if its host Vulkan path fails. Do not assume a compute-capable desktop GPU. |
| Repeated task creation eventually crashes | Preserve the Android/OHOS pooled thread-local state; excessive platform TLS keys can exhaust the per-process budget. |
| `INSTALL_FAILED_NO_MATCHING_ABIS` or loader error | Match the APK ABI to the device and ensure its native dependencies use Android/Bionic for that ABI. |
| `INSTALL_FAILED_UPDATE_INCOMPATIBLE` | Compare signing certificates. Reuse the existing key, or explicitly uninstall after preserving needed data. |
| NDK, linker or SDK-tool discovery failure | Check the environment paths, installed API/build-tools, NDK host directory and Java version; installing a Rust target alone is insufficient. |
| 16 KB page-size compatibility | The script requests 16 KB ZIP alignment, but also check native ELF load-segment alignment and dependencies. ZIP alignment alone does not establish 16 KB device compatibility. |
| A feature compiles but has no visible service | Check the mobile host integration and platform prerequisites, especially WebView/media. An APK containing a Rust feature does not automatically supply every Java-side integration. |

## iOS

The framework has an iOS backend, but the repository does not yet contain an iOS Gallery application host, Xcode project or IPA export script. The existing mobile library exports Android and OHOS entries only. Its successful library compilation would not create a launchable iOS Gallery.

Use macOS with full Xcode, iPhoneOS/iPhoneSimulator SDKs and the appropriate signing/device setup. The targets are:

| Destination | Rust target |
| --- | --- |
| ARM64 iPhone/iPad | `aarch64-apple-ios` |
| Apple Silicon simulator | `aarch64-apple-ios-sim` |
| Intel simulator | `x86_64-apple-ios` |

The following are framework compilation checks to run on that host, not verified application packaging commands:

```sh
rustup target add aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios
cargo check -p jadix --lib --locked --target aarch64-apple-ios --no-default-features --features backend-metal,macros
cargo check -p jadix --lib --locked --target aarch64-apple-ios-sim --no-default-features --features backend-metal,macros
cargo check -p jadix --lib --locked --target x86_64-apple-ios --no-default-features --features backend-metal,macros
```

Select the correct SDK for each destination with Xcode/`xcrun`; if setting `SDKROOT`, use the iPhoneOS SDK for device builds and iPhoneSimulator for simulator builds. Keep `IPHONEOS_DEPLOYMENT_TARGET` consistent across Rust and native dependencies. Do not use a macOS SDK to satisfy an iOS link.

To finish iOS support for the Gallery:

1. Add an iOS application target and a Rust entry compatible with the UIKit/winit backend. Decide whether the host links a Rust executable or an explicitly exported library entry; the current Android/OHOS entry points are not an iOS host.
2. Connect lifecycle and main-thread event-loop ownership correctly, initialize a writable resource directory with `prepare_resources`, then enter `run_gallery` through that host. Avoid starting a second competing event loop.
3. Configure bundle identifier, icons, launch screen, orientation, permissions/capabilities, required frameworks and bundled resources in Xcode.
4. Link the matching Rust destination into the corresponding `.app`. Device ARM64 and simulator ARM64 are different platforms; do not merge them with `lipo`. Use an XCFramework when distributing a library across SDKs.
5. Configure a development team/provisioning for devices and archive/export for the distribution channel. A simulator `.app` is not a device IPA.
6. Verify startup, safe areas, scale, touch, keyboard/IME, Metal and background/resume on simulator and device. Diagnose SDK, provisioning and architecture issues independently.

## OHOS

The [OHOS build script](../examples/gallery_mobile/build-ohos.ps1) packages phone/tablet and desktop/2in1 variants separately. Use PowerShell 7 on Windows with DevEco Studio's SDK, its OpenHarmony native toolchain, Node/OHPM/Hvigor tooling, Java, Zig and cargo-zigbuild. The exercised SDK/emulators use API 22 (HarmonyOS 6.0.2). An arbitrary OpenHarmony SDK or older SDK may not provide the same HarmonyOS application APIs.

```powershell
$env:DEVECO_SDK_HOME = 'C:/Program Files/Huawei/DevEco Studio/sdk'
$env:JAVA_HOME = 'C:/Java/jdk-17'
$devecoRoot = Split-Path -Parent $env:DEVECO_SDK_HOME
$env:PATH = "$devecoRoot/tools/node;$env:DEVECO_SDK_HOME/default/openharmony/toolchains;$env:PATH"
rustup target add aarch64-unknown-linux-ohos x86_64-unknown-linux-ohos
cargo install cargo-zigbuild --locked
zig version
pwsh -File examples/gallery_mobile/build-ohos.ps1 -Device phone -Architecture arm64 -Unsigned
pwsh -File examples/gallery_mobile/build-ohos.ps1 -Device phone -Architecture x86_64 -Unsigned
pwsh -File examples/gallery_mobile/build-ohos.ps1 -Device desktop -Architecture arm64 -Unsigned
pwsh -File examples/gallery_mobile/build-ohos.ps1 -Device desktop -Architecture x86_64 -Unsigned
```

Replace the SDK location as needed. The script configures the SDK native sysroot, Clang/linker and `LIBCLANG_PATH`, builds the Rust library, stages an ArkTS application, runs OHPM/Hvigor, and checks the library's ELF machine type against the requested architecture. It uses cargo-zigbuild with the OHOS SDK linker configuration; a generic GNU/Linux/musl compiler does not replace the OHOS SDK. The script currently assumes Windows host executables and the workspace's default `target/` location.

| Variant | Feature selection | Rust target | Default artifact directory |
| --- | --- | --- | --- |
| Phone ARM64 | `full` | `aarch64-unknown-linux-ohos` | `examples/gallery_mobile/dist/ohos-phone-arm64/` |
| Phone x86_64 | `full` | `x86_64-unknown-linux-ohos` | `examples/gallery_mobile/dist/ohos-phone-x86_64/` |
| Desktop ARM64 | `full,ohos-desktop` | `aarch64-unknown-linux-ohos` | `examples/gallery_mobile/dist/ohos-desktop-arm64/` |
| Desktop x86_64 | `full,ohos-desktop` | `x86_64-unknown-linux-ohos` | `examples/gallery_mobile/dist/ohos-desktop-x86_64/` |

Unsigned packages are named `jadix-gallery-ohos-<device>-<architecture>-release-unsigned.hap`. Desktop configuration declares `2in1`; phone configuration declares `phone` and `tablet`. Select both CPU architecture and device type for the intended system. `-OutputDirectory` selects the artifact location, and `-ProjectDirectory` selects a short generated ArkTS project path, useful for Windows/Hvigor path-length limits. `-SkipBuild -NativeLibrary <path>` can package an explicitly preserved library after cleaning Cargo outputs; it checks architecture, but does not prove the library has current source/features or is the right phone/desktop build.

The staged project declares the dynamically imported native module through `runtimeOnly.packages` and its native type package; preserve those declarations and package the SDK's matching `libc++_shared.so`. The desktop status-bar integration is enabled only when the runtime reports the PC status-bar capability. An application icon and desktop tray/status-bar service are separate integrations; do not infer OHOS tray support from Linux tray code. See [application icons](application-icons.md).

OHOS WebView embedding still requires a host bridge and is currently unsupported. The full-feature HAP does not supply that bridge; see the [platform capability table](webview.md).

### Signing and installation

Choose exactly one signing mode:

- `-Unsigned`: suitable only for systems/emulators that explicitly accept unsigned HAPs. This worked on the exercised x86_64 phone and desktop emulators; retail devices can reject it.
- `-SigningConfigFile <build-profile.json5> [-SigningConfigName <name>]`: use a genuine Huawei signing configuration. The script validates profile/certificate inputs before building and verifies the signed HAP with the SDK. The configuration name defaults to `default`.
- `-LocalTestSignature`: creates a locally signed test package. SDK verification confirms package signature consistency, but does not enroll the signing root or profile into a retail device's trust chain. This is not a device-installation workaround.

For example, after configuring the signing material outside the repository:

```powershell
pwsh -File examples/gallery_mobile/build-ohos.ps1 -Device phone -Architecture arm64 -SigningConfigFile 'D:/Signing/gallery/build-profile.json5'
hdc list targets
pwsh -File examples/gallery_mobile/verify-ohos.ps1 -Serial 127.0.0.1:5557 -Hap examples/gallery_mobile/dist/ohos-phone-x86_64/jadix-gallery-ohos-phone-x86_64-release-unsigned.hap
```

The signed build and unsigned emulator verification are independent examples. Replace the serial and package path for the actual device. Signed outputs use `release-signed.hap`; locally signed test outputs use `release-local-test-signed.hap`. Obtain a profile for `org.jadix.gallery`, the correct certificate and device type, authorized device identifiers when required, and a valid expiry date. Never commit private keys, passwords, profiles or device identifiers. Do not reuse an expired profile or another application's bundle identity.

The verifier checks `hdc install` output, launches the ability and collects process, log and screenshot evidence. `hdc` may return exit code zero while the install text reports failure, so check the actual result. Inspect the rendered content and test interaction; a built HAP or a live process alone does not prove correct rendering. The x86_64 emulator results do not establish ARM64 physical-device acceptance.

| Symptom | Investigation and corrective work |
| --- | --- |
| `9568257`, failed to verify PKCS7 | Check the actual Huawei trust chain, profile, certificate, bundle identity, expiry and authorized devices. A locally self-signed HAP can pass SDK verification and still fail device trust. |
| `9568320`, no signature file | An unsigned HAP reached a system requiring signing. Use an accepted signing identity/profile; renaming or repacking does not add trust. |
| Native library cannot be loaded | Match CPU/device configuration, check ELF machine type and packaged `libc++_shared.so`, and retain native-module packaging/type declarations. |
| Hvigor path too long | Use a short absolute `-ProjectDirectory`, for example `D:/ohos-build/gallery-phone-x64`; use a different directory for each build variant. |
| SDK symbol/API missing | Check the API level and HarmonyOS versus generic OpenHarmony SDK. Installing the Rust target does not install ArkTS/native APIs. |
| Starts, then exits or stays blank | Collect `hilog`, application `gallery.log`/panic output and GPU initialization evidence; verify writable resource initialization and supported GLES limits. |
| Desktop tray/status bar absent | Check device type and the runtime PC status-bar capability before exposing the service. Phone systems need not offer it. |

## Cross-compilation, disk space and verification

The target table lists the maintained 64-bit build paths. There is no complete Gallery packaging/verification flow for 32-bit ARM, x86 or other unlisted architectures. Supporting them requires checking the entire Rust/native dependency graph, SDK and application host before adding a target to a script.

Zig helps with compilation/linking; it does not supply Android, Apple, OHOS or Linux desktop SDKs, signing identities, provisioning or a running display system. In particular, the Android/OHOS packaging scripts currently target Windows hosts, the Linux script needs a Linux environment, and Apple application workflows need macOS/Xcode. Do not describe a cross-compiled library as an installed application.

If a Rust build runs out of disk space:

1. Stop the failed build and preserve completed executables, resource bundles, APKs/HAPs, checksums and needed native libraries outside `target/`. Keep source, lockfiles, signing material and test evidence intact. The scripts' `dist/` directories are outside Cargo's target directory, but verify custom output paths before cleaning.
2. Run `cargo clean` in the affected Cargo workspace, then retry the failed platform/architecture. Do not clean while another build is using the same target directory.
3. If space is still insufficient, inspect SDK/emulator, temporary and Zig caches separately. Relocate `TEMP`/`TMP` and `ZIG_GLOBAL_CACHE_DIR` to a disk with room where appropriate. `cargo clean` does not remove those caches or generated ArkTS projects. Avoid deleting unrelated SDKs or signing keys.


## Web / WebAssembly

The [browser entry](../examples/gallery_web/) reuses the Gallery's pages and Builder / `ui!` examples, with 3D and image streams enabled. It renders into a canvas, not a DOM widget tree. Its feature selection deliberately omits native WebViews, system trays, filesystem watching, window-state persistence and the threaded performance sampler. Desktop `full` is not a browser deployment profile. Native clipboard integration is not compiled; browser clipboard integration is not yet implemented.

Install the Rust target, Python 3.11+ and the **wasm-bindgen CLI version matching `wasm-bindgen` in `Cargo.lock`** (currently `0.2.114`). The CLI is a build tool, not an application dependency. Download the matching executable from the official [wasm-bindgen releases](https://github.com/wasm-bindgen/wasm-bindgen/releases/tag/0.2.114), extract it, and put its directory on `PATH`. No Node.js bundler or mobile SDK is required; the host Rust toolchain still needs its normal linker and build tools for procedural macros.

```sh
rustup target add wasm32-unknown-unknown
wasm-bindgen --version
python examples/gallery_web/build.py --serve --port 8765
```

Open `http://127.0.0.1:8765/`. The script builds the release library, generates JavaScript bindings with `--target web`, and serves `target/gallery-web/` (or the equivalent under `CARGO_TARGET_DIR`). Stop the foreground server with Ctrl+C. To serve an existing bundle without rebuilding:

```sh
python examples/gallery_web/build.py --no-build --serve --port 8765
```

The generated directory is deployable as static files. Keep `index.html` and the `pkg/` directory together, serve `.wasm` as `application/wasm`, and use HTTPS except on localhost. Do not open `index.html` as `file://`. A relative module URL permits hosting under a subdirectory. A hash selects the initial Gallery page, for example `/#forms`, `/#scene`, `/#documents` or `/#code`; page changes inside the Gallery use its own reactive navigation.

The script supplies `--cfg web_sys_unstable_apis`, required by the locked WebGPU bindings. For a manual build, set `CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUSTFLAGS="--cfg web_sys_unstable_apis"`, run `cargo build --release --locked -p gallery_web --target wasm32-unknown-unknown`, then run `wasm-bindgen --target web --out-dir target/gallery-web/pkg target/wasm32-unknown-unknown/release/gallery_web.wasm` and copy the example's `index.html` alongside `pkg/`. Existing `RUSTFLAGS` must retain that cfg. A CLI/schema mismatch requires updating the CLI to the lockfile version, not editing generated JavaScript.

Use a browser with WebGPU enabled and a working graphics adapter. WebGL is a fallback attempt, not a guarantee of equivalent shader, texture or device-limit support. Missing GPU support, insecure origins, driver failures and validation errors appear in browser developer tools; the HTML host also reports startup exceptions. A downloaded Wasm module or an HTTP 200 response alone does not establish successful rendering.

Browser startup awaits GPU initialization without blocking the main thread. Animation and interaction clocks use `web-time`; WebGPU validation scopes resolve asynchronously and report deferred errors to the console, so synchronous native pipeline fallback is not available there. Canvas presentation uses an sRGB render view over the browser's non-sRGB swapchain format so linear shader output, component colors and translucent overlays retain their intended appearance. Native OS windows, process launches, filesystem dialogs and runtime adapters still need host-specific implementations; compiling their API surface does not make those services work in the browser.

The Web host cannot enumerate system fonts. It includes Inter, Fira Mono, the complete Noto Sans CJK SC Regular face, Arabic, Hebrew, Thai, Devanagari, math/symbol fonts and Noto Color Emoji. CJK input is not restricted to characters in example strings. Only one CJK face is embedded rather than the entire font collection; this trades a larger Wasm download for general Chinese input coverage. Color fonts are registered separately from normal text shaping to use the isolated emoji rasterizer. Font assets and licenses are listed in the [Web example](../examples/gallery_web/README.md). Register additional font bytes with `jadix::utils::font::register_font_data` before startup for characters outside these fonts. Missing optional emoji fonts must not abort text rendering.

Browser text input uses a transparent, focusable textarea synchronized with the canvas caret. Composition preedit/commit events feed the framework's IME path; ordinary keyboard input remains on its keyboard path. Keep this DOM input available rather than hiding it with `display: none`. The browser and OS own candidate windows and virtual keyboards; test those on the intended browser/device, including composition cancellation, focus changes and mobile input modes.

## References

- [Repository feature selection](FEATURES.md) and [mobile host and scripts](../examples/gallery_mobile/README.md).
- [Rust target support](https://doc.rust-lang.org/rustc/platform-support.html), [Apple iOS targets](https://doc.rust-lang.org/rustc/platform-support/apple-ios.html), [OpenHarmony targets](https://doc.rust-lang.org/rustc/platform-support/openharmony.html).
- [cargo-zigbuild](https://github.com/rust-cross/cargo-zigbuild), [Android NDK integration](https://developer.android.com/ndk/guides/other_build_systems), [Android 16 KB page sizes](https://developer.android.com/guide/practices/page-sizes).
- [Xcode application distribution](https://developer.apple.com/documentation/xcode/distributing-your-app-for-beta-testing-and-releases).
