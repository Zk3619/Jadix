# 应用图标与托盘接入

## 响应式间距与帧率

Gallery 页面外边距沿用 `<640 / 640–1023 / ≥1024` 逻辑像素时的 `12 / 16 / 24`。
640 与 1024 对应常见的 Tailwind `sm`、`lg` 默认断点；12、16、24 都在 4 像素间距体系内。
这是适合紧凑桌面应用的设计约定，不是 W3C 或各平台强制标准。触控内容较多时可将窄屏外边距提高到 16；断点应由内容是否拥挤决定。
这里的逻辑像素会随系统 DPI 缩放，不能与物理屏幕像素直接比较。

渲染调度的默认上限为 **120 FPS**，空闲时仍按需绘制。显示器刷新率、垂直同步、后台节流和实际工作负载可能使观测值低于 120。
可在 UI 线程调用 `set_frame_rate_limit(NonZeroU32::new(90))` 改为 90，传 `None` 取消应用调度上限。

## 运行时接口

`AppIcon` 是经过尺寸与 RGBA 长度校验、可廉价 clone 的共享图像。
`from_bytes` / `from_path` 使用 Jadix 现有图片解码器，支持 PNG、ICO 等格式；SVG 需要 `svg` feature，动图取第一帧。
用户选择的图片应先成功解码，再替换当前图标；需要跨重启保存时，由应用持久化用户路径或图片副本。

```rust
use jadix::prelude::*;
use jadix::app::{AppIcon, WindowOptions, TrayIcon, tray_icon, current_window};

let icon = AppIcon::from_bytes(include_bytes!("../assets/app-icon.png"))?;
let options = WindowOptions::new().icon(icon.clone());
// jadix::app::run_with(options, build_scene);
// Builder 入口：jadix::app::app().icon(icon.clone()).mount(build_scene);
// RSX 入口：<App icon={icon.clone()}> ... </App>

// 事件循环启动后，在 UI 线程创建并保存句柄。
let tray: TrayIcon = tray_icon()
    .icon(icon)
    .tooltip("My application")
    .try_build()?;

// 文件选择器回调中：
let custom = AppIcon::from_path("user-selected.png")?;
if let Some(window) = current_window() {
    window.set_icon(Some(custom.clone()))?;
}
tray.set_icon(custom)?;
// window.set_icon(None)?; // 清除当前窗口图标覆盖
```

启用 `jadix` 的 `system-tray` feature 后可创建 Windows、macOS、Linux 原生托盘。
`try_build()` 和 `set_icon()` 返回实际错误；`is_native()` 检查当前句柄是否有原生图标。
原有 `build()` 保留失败时降为 stub 的兼容行为。托盘句柄必须保持存活，drop 后移除托盘。
所有托盘操作在创建它的 UI 线程执行，跨线程调用请通过 `run_on_ui_thread`。
Linux 还需要 tray-icon 所要求的 GTK/AppIndicator 依赖以及桌面环境的托盘支持。

`WindowOptions` / `AppBuilder` 在创建窗口前传入图标；`<App icon={...}>` 在 UI 构建时应用窗口图标更新。需要创建时即指定默认图标时使用前两种入口。
`WindowHandle::set_icon` 可替换主窗口和次级窗口，`AppIcon::window_supported()` 可预检平台支持。
Windows 和支持此能力的 Linux/BSD 后端可用；Wayland 合成器可能忽略请求。
macOS Dock、Android/iOS 启动器和 Web 图标由打包资源管理，`WindowHandle::set_icon` 会返回不支持错误。

独立原生示例可直接运行，提供窗口与托盘图标切换、重置和文件选择：

```sh
cargo run -p jadix --example application_icons --features system-tray
```

Gallery 已在启动时设置默认图标，并在“导航 → 系统托盘”提供 Show tray / Choose icon / Reset icon。
选择会更新当前窗口、已有托盘及后续创建的托盘；此演示选择只在本次运行内保存。

## 桌面与移动端资源配置

复制 `docs/app-icons.example.toml`，将 `source` 指向方形主图，建议至少 1024×1024 PNG。
路径相对于 TOML 文件；桌面各平台、托盘、Android 前景和 iOS 主图可分别覆盖。
脚本需要 Python 3.11+ 与 Pillow：

```sh
python scripts/app-icons.py docs/app-icons.example.toml --output .tmp/generated-icons
# 可限定平台；可重复传入 --platform。
python scripts/app-icons.py app-icons.toml --output build/icons --platform android
```

此命令生成资源和合并片段，不会自动修改打包工程。

| 平台 | 生成资源 | 接入位置 |
| --- | --- | --- |
| Windows | 多尺寸 `desktop/app.ico`、`window.png` | EXE 的 RC `ICON` 资源与安装包；运行时窗口用 AppIcon |
| macOS | `desktop/app.icns` | `.app/Contents/Resources`，Info.plist `CFBundleIconFile` |
| Linux | `desktop/hicolor/*/apps/app.png` | 安装到 hicolor 对应目录，`.desktop` 中设置 `Icon=应用名称` |
| 托盘 | `desktop/tray.png` | `AppIcon::from_bytes` 后传给 `tray_icon().icon(...)` |
| Android | legacy mipmap、108dp 自适应前景/背景 XML、可选 API 33 单色层 | 合并 `android/res` 到应用模块 `src/main/res`，应用清单设置 `android:icon="@mipmap/ic_launcher"` |
| iOS | 主图与可选备用图标的 `.appiconset`、Info.plist 合并片段 | 加入 Xcode asset catalog；主图名称设为 `AppIcon`，备用图标加入 Alternate App Icon Sets 构建设置 |

Windows 可使用既有资源编译工具将 `IDI_APP_ICON ICON ".../desktop/app.ico"` 编进 EXE；运行时设置不会更改磁盘 EXE 或安装包图标。
macOS 图标变化由 bundle/Dock API 管理；菜单栏建议独立的高对比单色素材。
Android 图层为 108dp，脚本将前景约束在中央 66dp 区域，降低系统形状遮罩裁切风险。若提供单色图，API 33 目录生成 themed icon 声明。
iOS 输出无 alpha 的 RGB PNG；不要预先裁成圆角。`Info-icons.plist` 必须合并，不能覆盖工程现有 Info.plist。

移动端的“用户自定义”必须遵循平台入口：Android 可预声明 activity-alias 再使用 PackageManager 切换；iOS 必须预打包备用图标，再通过 `UIApplication.setAlternateIconName` 选择。
本次提供统一素材配置与备用资源生成；Jadix 尚未封装上述移动端原生切换 API，不能把任意用户文件直接用作 iOS 启动器图标。

## 参考与验证边界

参考官方材料：Tailwind CSS Responsive design / Spacing、Android Adaptive icons、Apple UIApplication.setAlternateIconName 与 App icon asset catalog。
生成资源可用本地测试验证尺寸、透明度与配置内容；Android/iOS 最终打包和系统启动器展示仍需对应 SDK、签名工程及真机验收。
