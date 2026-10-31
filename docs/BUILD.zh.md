# Jadix 编译与打包指南

[English](BUILD.md) | [简体中文](BUILD.zh.md) | [文档索引](README.md)

本指南介绍 SDK 配置、工作区与 Gallery 编译、应用打包和安装。除另有说明外，命令均在仓库根目录执行。

## 平台与架构

| 平台 | Rust target | 构建宿主与成品 |
| --- | --- | --- |
| Windows | `x86_64-pc-windows-msvc`、`aarch64-pc-windows-msvc` | Windows + MSVC/Windows SDK；可执行文件及资源 |
| macOS | `aarch64-apple-darwin`、`x86_64-apple-darwin` | macOS + Xcode；可执行文件，随后组装应用包 |
| Linux | `x86_64-unknown-linux-gnu`、`aarch64-unknown-linux-gnu` | Linux 或 WSL + 目标系统库；可执行文件与资源压缩包 |
| Android | `aarch64-linux-android`、`x86_64-linux-android` | 当前打包脚本使用 Windows + Android SDK/NDK/JDK；APK |
| iOS | `aarch64-apple-ios`、`aarch64-apple-ios-sim`、`x86_64-apple-ios` | macOS + Xcode + iOS 应用宿主；对应的 `.app`/IPA |
| OHOS 手机/平板 | `aarch64-unknown-linux-ohos`、`x86_64-unknown-linux-ohos` | 当前打包脚本使用 Windows + DevEco SDK；HAP |
| OHOS 桌面/2in1 | 同上两个 OHOS target | 独立的桌面 feature 与应用配置；HAP |
| Web | `wasm32-unknown-unknown` | 支持 Rust 的构建宿主；JavaScript + Wasm + 静态 HTTP(S) 服务 |

x86_64 包用于匹配的模拟器，不能代替 ARM64 真机包。OHOS 手机和桌面即使使用相同 CPU 架构，应用配置和原生 feature 也不同。

## 通用准备

安装 Git、rustup 和较新的 stable Rust。工作区没有统一固定的工具链版本或工作区级 MSRV，应遵守整个依赖图中各包的 `rust-version`，包括维护叉；其中部分包当前要求 Rust 1.89。`rustup target add` 只安装 Rust 目标库，不会安装平台 SDK、链接器、系统库或签名工具。

```sh
git clone https://github.com/Zk3619/Jadix.git
cd Jadix
rustup update stable
cargo fetch --locked
cargo run -p gallery
```

`cargo run` 需要先安装下文对应宿主的原生依赖。保留 `Cargo.lock` 及依赖图选定的 `vendor/` 维护叉。`--locked` 会在清单变更需要更新锁文件时主动失败；应审核锁文件变更，不要直接去掉该参数。

[Gallery 清单](../examples/gallery/Cargo.toml) 默认启用 `full`，包含展示所需的可选能力，如 3D、文档/媒体集成、WebView、运行时/网络适配和开发工具；移动端库转发该 feature。启用 feature 不代表平台运行时一定提供对应服务。`ohos-desktop` 在 OHOS 上额外启用桌面窗口状态，手机端不应启用；Android/iOS 不通过 Gallery 依赖图编译 `jadix_window_state`。能力边界见 [feature 文档](FEATURES.md) 和 [WebView 文档](webview.md)。

Release 使用 fat LTO 和单个 codegen unit，链接时可能消耗较多时间、内存和磁盘空间。资源受限时降低 `CARGO_BUILD_JOBS`。桌面可执行文件需要工作目录中包含 `examples/gallery` 的 `theme.toml`、`styles.toml` 和 `assets/`。移动端入口会在应用可写目录调用 `prepare_resources`；新增宿主时必须保留该初始化。

## Windows

安装 Visual Studio Build Tools 的“使用 C++ 的桌面开发”、MSVC 工具链与 Windows SDK。ARM64 构建还需要 ARM64 C++ 工具和库。编译器或 SDK 自动发现失败时使用开发者 PowerShell。Gallery 的构建脚本通过 SDK 资源编译器嵌入应用图标；自动发现失败时可用 `RC` 指定资源编译器路径。

```powershell
rustup target add x86_64-pc-windows-msvc aarch64-pc-windows-msvc
cargo build -p gallery --bin gallery --release --locked --target x86_64-pc-windows-msvc
cargo build -p gallery --bin gallery --release --locked --target aarch64-pc-windows-msvc
```

输出为 `target/<target>/release/gallery.exe`。按架构分别附带 Gallery 的主题、样式和资源，以资源所在目录作为工作目录启动。MSVC 运行库再分发、安装程序制作及程序/安装包签名属于后续分发工作。发布 ARM64 前，检查所有原生依赖的架构与运行库要求。

找不到 `link.exe`、Windows SDK 库或 `rc.exe` 时，应修复 C++/SDK 安装，避免混用 MSVC 与 GNU 库。Release GUI 程序默认没有控制台，可设置 `RUST_LOG` 和 `JADIX_LOG_FILE` 收集启动失败信息。黑屏时检查 GPU 驱动、后端初始化和资源路径。嵌入图标更新后，系统图标缓存也可能需要刷新。WebView 还依赖平台运行时，见 [接入指南](webview.md)。

## macOS

使用安装完整 Xcode 的 macOS，选择开发者目录、接受许可并安装所需 SDK。通过 `xcode-select -p` 和 `xcrun --sdk macosx --show-sdk-path` 检查。若设置 `SDKROOT` 或 `MACOSX_DEPLOYMENT_TARGET`，其值必须与选定 SDK 及全部原生依赖一致；Rust 自身的最低系统版本不代表完整 Gallery 的最低系统版本。

```sh
rustup target add aarch64-apple-darwin x86_64-apple-darwin
cargo build -p gallery --bin gallery --release --locked --target aarch64-apple-darwin
cargo build -p gallery --bin gallery --release --locked --target x86_64-apple-darwin
```

这些命令生成 `target/<target>/release/gallery`，不会直接生成可分发的 `.app`，且需要在 macOS 宿主实际执行和验证。完成应用包还需：

1. 组装 `Jadix Gallery.app/Contents/MacOS/gallery`，提供包含可执行文件名、bundle identifier、最低系统版本的 `Info.plist`，将图标与资源放入 `Contents/Resources`。清单中的 bundle 元数据本身不会组装应用包。
2. 让资源定位独立于终端工作目录。生成文件和窗口状态写入可写的应用数据目录，不应把安装后的应用包作为可写工作目录。
3. 若提供 Universal 2，先分别构建两个 macOS 架构，再用 `lipo` 合并兼容的可执行文件，并检查附带原生库。所有内容组装完成后再签名。
4. 按分发渠道配置签名、entitlements、归档/导出和公证。本地 ad hoc 签名不能代替 App Store 或 Developer ID 分发身份。
5. 在目标硬件和系统版本验证 Finder 启动、Metal、输入/IME、缩放、媒体/WebView 和休眠恢复。

常见失败包括原生库架构错误、SDK 选择过时、deployment target 不一致，以及嵌套代码未签名或签名后被修改。Windows 上完成交叉链接不能证明上述打包流程已通过验证。

## Linux

完整 Gallery 需要 GTK 3、WebKitGTK 4.1、X11/Wayland 与图形开发库。Ubuntu 24.04 可从以下依赖开始：

```sh
sudo apt update
sudo apt install build-essential clang cmake pkg-config libx11-dev libxkbcommon-dev libxkbcommon-x11-dev libwayland-dev libgtk-3-dev libwebkit2gtk-4.1-dev libvulkan-dev mesa-vulkan-drivers libegl1-mesa-dev libgl1-mesa-dev fonts-noto-cjk
rustup target add x86_64-unknown-linux-gnu
cargo build -p gallery --bin gallery --release --locked --target x86_64-unknown-linux-gnu
```

ARM64 原生构建使用 `aarch64-unknown-linux-gnu` 和匹配的系统依赖。分发时附带 Gallery 资源。仓库维护的 [Linux 打包脚本](../examples/gallery/build-linux.sh) 使用 cargo-zigbuild，以 `--all-features` 构建：

```sh
cargo install cargo-zigbuild --locked
zig version
bash examples/gallery/build-linux.sh x86_64-unknown-linux-gnu.2.39
```

Zig 需要另外安装并加入 `PATH`。脚本在 `examples/gallery_mobile/dist/linux/` 输出可执行文件/资源目录、`jadix-gallery-x86_64-unknown-linux-gnu.2.39-release.tar.gz` 和校验值；可用 `GALLERY_OUTPUT_DIR` 更改输出目录。`.2.39` 是 cargo-zigbuild 的 glibc 选择后缀，不属于传给 `rustup` 的 target。当前包以 Ubuntu 24.04/glibc 2.39 为基线，单纯降低后缀不能让较新的 GTK/WebKit 系统库兼容旧发行版。

交叉编译 ARM64 时，安装 `aarch64-unknown-linux-gnu`，准备包含 ARM64 GTK/WebKit 及其依赖的 sysroot，配置 `PKG_CONFIG_SYSROOT_DIR`、`PKG_CONFIG_LIBDIR`，必要时配置 `PKG_CONFIG_ALLOW_CROSS`，再向脚本传入 `aarch64-unknown-linux-gnu.2.39`。不要把宿主 `.pc` 文件解析为目标库。Zig 不提供 GUI sysroot，也不会把完整 Gallery 自动转换为可移植的 musl 全静态程序。

运行时依赖及 WSLg 说明见 [Linux 运行说明](../examples/gallery/LINUX.md)，尤其注意：

- 安装 GTK/WebKit 运行库、`libxkbcommon-x11`、字体和正常工作的 GPU 驱动。动态加载的库缺失不一定能从 `ldd` 中看出。
- 在 X11/Wayland 会话内启动；SSH/无头环境需另行提供显示会话。WSLg 的 Wayland 路径失败时，可尝试 `env -u WAYLAND_DISPLAY ./gallery` 使用 X11。
- 驱动支持时，可用 `JADIX_GPU_BACKEND=vulkan ./gallery` 显式选择 Vulkan 排查。
- 托盘依赖 StatusNotifierItem 等桌面服务；链接成功或存在 feature 不代表当前桌面提供托盘。

## Android

使用 PowerShell 7 在 Windows 运行 [Android 构建脚本](../examples/gallery_mobile/build-android.ps1)。需要 Android SDK 平台 35 或更高版本、build-tools（`aapt2`、`zipalign`、`apksigner`）、platform-tools（`adb`）、包含所需 ABI 的 NDK、JDK 17、Zig 和 cargo-zigbuild。本次验证使用 NDK 29.0.14206865，复现问题时应明确记录 SDK/NDK 的变化。当前清单的最低 API 为 28，target API 为 35。

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

SDK/JDK 路径需替换为本机安装位置；未设置 `ANDROID_NDK_HOME` 时可回退到 `NDK_HOME`。脚本使用 NDK 的 `windows-x86_64` 宿主工具与 Windows 包装器，Unix 宿主即使安装 PowerShell，也需先适配脚本。移动脚本读取工作区内的 `target/`，因此应保持 `CARGO_TARGET_DIR` 未设置。

脚本将 `gallery_mobile` 构建为包含完整 Gallery 的 release `cdylib`，随后打包原生库、Android 资源，对齐 APK 并使用本地测试密钥签名。这里使用 NDK/Bionic 配置，不能替换为 GNU/Linux 或 musl target。

| 架构 | Rust target / APK ABI | 默认成品 |
| --- | --- | --- |
| ARM64 | `aarch64-linux-android` / `arm64-v8a` | `examples/gallery_mobile/dist/android-arm64/jadix-gallery-android-arm64-release.apk` |
| x86_64 | `x86_64-linux-android` / `x86_64` | `examples/gallery_mobile/dist/android-x86_64/jadix-gallery-android-x86_64-release.apk` |

`-OutputDirectory` 可更改包输出目录。`-SkipBuild` 只重新打包已有原生库，不会确认其是否包含最新源码或 feature 变化。生成的 `local-gallery-release.p12` 应保密且不得提交；测试密钥及脚本中的公开测试密码不适合生产分发。不同输出目录可能生成不同密钥，覆盖安装时需复用原签名身份，或在明确备份必要数据后卸载旧应用；卸载会删除应用数据。生产签名、商店信息和 AAB 流程需另行配置。

当前 APK 打包器不会把 WebView Java helper 编译进 DEX。若要使用 Android WebView，需按 [WebView 接入指南](webview.md)，将 `JadixWebViewHost.java` 加入具备 Java 编译流程的应用宿主，并在代码压缩时保留该类。仅启用 `full` 不会完成该集成。

安装并收集指定设备/模拟器的启动证据：

```powershell
adb devices -l
pwsh -File examples/gallery_mobile/verify-android.ps1 -Serial emulator-5554 -Apk examples/gallery_mobile/dist/android-x86_64/jadix-gallery-android-x86_64-release.apk
```

将序列号替换为 `adb devices` 的实际条目，并选择匹配的 ABI。验证器执行安装、启动、进程/前台状态检查，保存日志和截图。截图不是纯色只是一项冒烟检查，仍需查看实际内容，并手动操作导航、文本输入、滚动和 3D 动画。模拟器运行不能证明真机 GPU 兼容性。

| 现象 | 排查与修复方向 |
| --- | --- |
| 白屏随后退出 | 收集启动 `logcat`、crash buffer 和进程状态，先定位原生初始化、panic/tombstone、GPU limits 和资源初始化问题。 |
| GLES 校验错误或黑屏 | 移动启动使用受限 GLES 能力，保留移动 limits 与兼容的裁剪纹理格式；模拟器宿主 Vulkan 路径异常时可测试软件 GLES，不应假设设备具备桌面 compute 能力。 |
| 反复创建任务后崩溃 | 保留 Android/OHOS 线程本地状态池，过量申请平台 TLS key 可能耗尽进程额度。 |
| `INSTALL_FAILED_NO_MATCHING_ABIS` 或加载器错误 | APK ABI 与设备匹配，所有原生依赖也应使用对应 ABI 的 Android/Bionic。 |
| `INSTALL_FAILED_UPDATE_INCOMPATIBLE` | 比较签名证书，复用已有密钥，或在保留必要数据后明确卸载旧包。 |
| 找不到 NDK、链接器或 SDK 工具 | 检查环境路径、已安装 API/build-tools、NDK 宿主目录和 Java 版本；只安装 Rust target 不够。 |
| 16 KB 页面兼容 | 脚本请求 16 KB ZIP 对齐，但还需检查原生 ELF 的加载段对齐及依赖；仅 ZIP 对齐不能证明兼容 16 KB 设备。 |
| feature 编译成功但没有对应服务 | 检查移动宿主接入与平台前置条件，尤其 WebView/媒体；APK 包含 Rust feature 不会自动提供全部 Java 侧集成。 |

## iOS

框架已有 iOS 后端，但仓库尚无 Gallery 的 iOS 应用宿主、Xcode 工程或 IPA 导出脚本。当前移动库只导出 Android 与 OHOS 入口，即使库编译成功，也不会自动得到可启动的 iOS Gallery。

需要安装完整 Xcode、iPhoneOS/iPhoneSimulator SDK 的 macOS，以及对应签名和设备配置。目标区分如下：

| 运行目的地 | Rust target |
| --- | --- |
| ARM64 iPhone/iPad 真机 | `aarch64-apple-ios` |
| Apple Silicon 模拟器 | `aarch64-apple-ios-sim` |
| Intel 模拟器 | `x86_64-apple-ios` |

以下为需要在该宿主执行的框架编译检查，不是已经验证的应用打包命令：

```sh
rustup target add aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios
cargo check -p jadix --lib --locked --target aarch64-apple-ios --no-default-features --features backend-metal,macros
cargo check -p jadix --lib --locked --target aarch64-apple-ios-sim --no-default-features --features backend-metal,macros
cargo check -p jadix --lib --locked --target x86_64-apple-ios --no-default-features --features backend-metal,macros
```

通过 Xcode/`xcrun` 为各目的地选择正确 SDK；若显式设置 `SDKROOT`，真机使用 iPhoneOS，模拟器使用 iPhoneSimulator。Rust 与原生依赖的 `IPHONEOS_DEPLOYMENT_TARGET` 应一致，不能使用 macOS SDK 完成 iOS 链接。

要完成 Gallery 的 iOS 支持，还需：

1. 增加 iOS 应用 target 及兼容 UIKit/winit 后端的 Rust 入口，确定宿主链接 Rust 可执行文件还是显式导出的库入口；现有 Android/OHOS 入口不能充当 iOS 宿主。
2. 正确接入生命周期和主线程事件循环，先在可写资源目录调用 `prepare_resources`，再通过宿主进入 `run_gallery`，避免启动第二个竞争的事件循环。
3. 在 Xcode 配置 bundle identifier、图标、启动屏幕、方向、权限/能力、所需 framework 与资源。
4. 把正确目的地的 Rust 产物链接到对应 `.app`。真机 ARM64 与模拟器 ARM64 属于不同平台，不能用 `lipo` 合并；跨 SDK 分发库时使用 XCFramework。
5. 为真机设置开发团队和 provisioning，为对应分发渠道完成归档/导出。模拟器 `.app` 不能作为真机 IPA。
6. 在模拟器和真机验证启动、安全区、缩放、触摸、键盘/IME、Metal 和后台恢复，分别排查 SDK、签名配置和架构问题。

## OHOS

[OHOS 构建脚本](../examples/gallery_mobile/build-ohos.ps1) 分别打包手机/平板与桌面/2in1。当前使用 Windows + PowerShell 7，需要 DevEco Studio SDK、其中的 OpenHarmony 原生工具链、Node/OHPM/Hvigor 工具、Java、Zig 和 cargo-zigbuild。本次验证的 SDK/模拟器使用 API 22（HarmonyOS 6.0.2）；其他 OpenHarmony SDK 或旧版 SDK 不一定提供相同的 HarmonyOS 应用 API。

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

按实际安装位置替换 SDK 路径。脚本设置 SDK 原生 sysroot、Clang/链接器和 `LIBCLANG_PATH`，构建 Rust 库、生成 ArkTS 应用、运行 OHPM/Hvigor，并检查 ELF machine 是否与请求架构一致。虽然使用 cargo-zigbuild，但采用 OHOS SDK 的链接配置，通用 GNU/Linux/musl 编译器不能替代 OHOS SDK。脚本目前假设 Windows 宿主工具和工作区默认 `target/` 目录。

| 变体 | feature | Rust target | 默认产物目录 |
| --- | --- | --- | --- |
| 手机 ARM64 | `full` | `aarch64-unknown-linux-ohos` | `examples/gallery_mobile/dist/ohos-phone-arm64/` |
| 手机 x86_64 | `full` | `x86_64-unknown-linux-ohos` | `examples/gallery_mobile/dist/ohos-phone-x86_64/` |
| 桌面 ARM64 | `full,ohos-desktop` | `aarch64-unknown-linux-ohos` | `examples/gallery_mobile/dist/ohos-desktop-arm64/` |
| 桌面 x86_64 | `full,ohos-desktop` | `x86_64-unknown-linux-ohos` | `examples/gallery_mobile/dist/ohos-desktop-x86_64/` |

未签名包命名为 `jadix-gallery-ohos-<device>-<architecture>-release-unsigned.hap`。桌面配置声明 `2in1`，手机配置声明 `phone` 和 `tablet`，必须同时匹配 CPU 架构与设备类型。`-OutputDirectory` 控制成品目录；`-ProjectDirectory` 控制生成的 ArkTS 工程目录，可用较短路径规避 Windows/Hvigor 路径长度限制。`-SkipBuild -NativeLibrary <path>` 可在清理 Cargo 后使用明确保留的原生库打包；该路径会检查架构，但不能证明库包含最新源码/feature，也不能代替手机/桌面变体检查。

生成工程通过 `runtimeOnly.packages` 和原生类型包声明动态导入的 native module，应保留这些声明，并附带 SDK 中匹配架构的 `libc++_shared.so`。桌面状态栏集成仅在运行时报告 PC 状态栏能力时启用。应用图标与桌面托盘/状态栏属于不同接入，不能从 Linux 托盘实现推断 OHOS 托盘可用。图标说明见 [应用图标](application-icons.md)。

OHOS WebView 嵌入仍需宿主桥接，目前未支持。完整 feature 的 HAP 不包含该桥接，实现边界见 [平台能力表](webview.md)。

### 签名与安装

三种签名模式只能选择一种：

- `-Unsigned`：仅适用于明确接受未签名 HAP 的系统/模拟器。本次 x86_64 手机、桌面模拟器接受该模式，零售设备可能拒绝。
- `-SigningConfigFile <build-profile.json5> [-SigningConfigName <name>]`：使用真实 Huawei 签名配置。脚本在构建前检查 profile/证书输入，并通过 SDK 验证签名后的 HAP；配置名默认 `default`。
- `-LocalTestSignature`：生成本地测试签名包。SDK 校验只证明包签名一致，不会将本地根证书或 profile 加入零售设备信任链，不能借此绕过真机安装校验。

例如，在仓库外配置签名材料后：

```powershell
pwsh -File examples/gallery_mobile/build-ohos.ps1 -Device phone -Architecture arm64 -SigningConfigFile 'D:/Signing/gallery/build-profile.json5'
hdc list targets
pwsh -File examples/gallery_mobile/verify-ohos.ps1 -Serial 127.0.0.1:5557 -Hap examples/gallery_mobile/dist/ohos-phone-x86_64/jadix-gallery-ohos-phone-x86_64-release-unsigned.hap
```

这里的真机签名构建与未签名模拟器验证是两个独立示例，请按实际设备替换序列号和包路径。正式签名输出后缀为 `release-signed.hap`，本地测试签名为 `release-local-test-signed.hap`。应申请匹配 `org.jadix.gallery`、证书、设备类型、有效期的 profile，并在要求时授权设备标识。私钥、密码、profile 和设备标识不得提交。其他应用的 bundle identity 或已过期 profile 不能复用。

验证器检查 `hdc install` 的实际输出、启动 ability 并采集进程、日志和截图。`hdc` 可能在退出码为零时仍输出安装失败，应检查安装结果文本。需要查看内容并测试交互，HAP 构建完成或进程存活都不能单独证明渲染正确。x86_64 模拟器结果不能证明 ARM64 真机接受该包。

| 现象 | 排查与修复方向 |
| --- | --- |
| `9568257`，PKCS7 验证失败 | 检查真实 Huawei 信任链、profile、证书、bundle identity、有效期及授权设备；本地自签 HAP 即使通过 SDK 校验，也可能不受设备信任。 |
| `9568320`，没有签名文件 | 系统要求签名而收到未签名包，应使用被接受的签名身份/profile，重命名或重新压缩不会增加信任。 |
| 原生库加载失败 | 匹配 CPU/设备配置，检查 ELF machine 和随包 `libc++_shared.so`，保留 native module 的打包/类型声明。 |
| Hvigor 路径过长 | 使用较短的绝对 `-ProjectDirectory`，如 `D:/ohos-build/gallery-phone-x64`，各变体使用不同目录。 |
| SDK 缺少符号/API | 检查 API 级别与 HarmonyOS/通用 OpenHarmony SDK 差异；Rust target 不包含 ArkTS/原生平台 API。 |
| 启动后退出或空白 | 收集 `hilog`、应用 `gallery.log`/panic 输出及 GPU 初始化证据，检查可写资源初始化和 GLES limits。 |
| 桌面托盘/状态栏不可用 | 展示服务前检查设备类型及运行时 PC 状态栏能力，手机系统不一定提供该能力。 |

## 交叉编译、空间管理与验证

表中列出当前维护的 64 位构建路径，尚无完整 Gallery 的 32 位 ARM、x86 或其他未列出架构的打包/验证流程。增加这些 target 前，需要检查整个 Rust/原生依赖图、SDK 和应用宿主。

Zig 辅助编译和链接，但不提供 Android、Apple、OHOS 或 Linux 桌面 SDK，也不提供签名身份、provisioning 或显示会话。Android/OHOS 打包脚本目前面向 Windows，Linux 脚本需要 Linux 环境，Apple 应用流程需要 macOS/Xcode。不能把交叉编译出的库描述为已经安装运行的应用。

Rust 构建因磁盘空间不足失败时：

1. 停止失败的构建，将已完成的可执行文件、资源包、APK/HAP、校验值和后续需要的原生库备份到 `target/` 之外。保留源码、锁文件、签名材料及测试证据。脚本默认 `dist/` 不在 Cargo target 目录内，但自定义路径仍需先确认。
2. 在受影响的 Cargo 工作区执行 `cargo clean`，再重试失败的平台/架构。不要在其他构建仍使用同一 target 目录时清理。
3. 若空间仍不足，分别检查 SDK/模拟器、临时目录与 Zig 缓存；可按需将 `TEMP`/`TMP`、`ZIG_GLOBAL_CACHE_DIR` 迁移到空间充足的磁盘。`cargo clean` 不会清理这些缓存或生成的 ArkTS 工程，避免删除无关 SDK 或签名密钥。


## Web / WebAssembly

[浏览器入口](../examples/gallery_web/) 复用 Gallery 页面及 Builder / `ui!` 混用示例，启用 3D 与图像流，使用 Canvas 绘制而非 DOM 控件树。浏览器配置不启用原生 WebView、系统托盘、文件监听、窗口状态持久化及线程式性能采样器；桌面 `full` 不适合作为 Web 部署配置。不编译原生剪贴板依赖，浏览器剪贴板适配尚未实现。

准备 Rust target、Python 3.11+，以及与 `Cargo.lock` 中 `wasm-bindgen` **完全相同版本的 wasm-bindgen CLI**，当前为 `0.2.114`。CLI 是构建工具，不是应用库依赖。可从官方 [wasm-bindgen Releases](https://github.com/wasm-bindgen/wasm-bindgen/releases/tag/0.2.114) 下载匹配宿主的预编译工具，解压后将目录加入 `PATH`；不需要 Node.js 打包器或移动端 SDK，但宿主 Rust 工具链仍需正常链接器和编译过程宏所用的构建工具。

```sh
rustup target add wasm32-unknown-unknown
wasm-bindgen --version
python examples/gallery_web/build.py --serve --port 8765
```

打开 `http://127.0.0.1:8765/`。脚本构建 release 库，使用 `--target web` 生成 JavaScript 绑定，并提供 `target/gallery-web/` 的静态服务；指定 `CARGO_TARGET_DIR` 时输出随之移动。前台服务使用 Ctrl+C 停止。已有产物可直接启动：

```sh
python examples/gallery_web/build.py --no-build --serve --port 8765
```

将生成目录整体部署到静态服务，保留 `index.html` 与 `pkg/` 的相对位置；`.wasm` 的 MIME 应为 `application/wasm`，除 localhost 外使用 HTTPS。不要以 `file://` 直接打开页面。模块引用为相对 URL，可部署到站点子目录。URL hash 可指定初始页面，如 `/#forms`、`/#scene`、`/#documents`、`/#code`；Gallery 内部切页仍使用自身响应式导航。

脚本自动设置锁定 WebGPU 绑定所需的 `--cfg web_sys_unstable_apis`。手动构建时，设置环境变量 `CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUSTFLAGS="--cfg web_sys_unstable_apis"`，执行 `cargo build --release --locked -p gallery_web --target wasm32-unknown-unknown`，随后执行 `wasm-bindgen --target web --out-dir target/gallery-web/pkg target/wasm32-unknown-unknown/release/gallery_web.wasm`，并将示例的 `index.html` 放在 `pkg/` 旁。已有 `RUSTFLAGS` 时也要保留该 cfg。CLI/schema 不匹配时应按锁文件调整 CLI，不应修改生成的 JavaScript。

浏览器需要启用 WebGPU 且能够取得可工作的 GPU adapter。WebGL 是后备尝试，不保证与 WebGPU 具有相同的着色器、纹理和设备限制支持。缺少 GPU、非安全来源、驱动及校验错误应检查浏览器开发者工具；HTML 宿主也会显示启动异常。仅成功下载 Wasm 或返回 HTTP 200 不代表绘制成功。

浏览器启动异步等待 GPU，不阻塞主线程；动画与交互时钟使用 `web-time`。WebGPU 校验作用域异步完成并向控制台报告延迟错误，不支持原生路径那样的同步管线回退。系统窗口、进程启动、文件对话框及运行时适配仍有宿主边界，API 编译通过不代表浏览器提供对应服务。Web 宿主内置 Inter 和 OFL 许可的 Gallery 中文子集，因为无法扫描系统字体；要输入超出该子集的文本，应在启动前调用 `jadix::utils::font::register_font_data` 注册额外字体。字体与许可证位于示例目录。

## 参考资料

- 仓库内的 [feature 选择](FEATURES.md)与[移动宿主和脚本](../examples/gallery_mobile/README.md)。
- [Rust target 支持](https://doc.rust-lang.org/rustc/platform-support.html)、[Apple iOS target](https://doc.rust-lang.org/rustc/platform-support/apple-ios.html)、[OpenHarmony target](https://doc.rust-lang.org/rustc/platform-support/openharmony.html)。
- [cargo-zigbuild](https://github.com/rust-cross/cargo-zigbuild)、[Android NDK 集成](https://developer.android.com/ndk/guides/other_build_systems)、[Android 16 KB 页面](https://developer.android.com/guide/practices/page-sizes)。
- [Xcode 应用分发](https://developer.apple.com/documentation/xcode/distributing-your-app-for-beta-testing-and-releases)。
