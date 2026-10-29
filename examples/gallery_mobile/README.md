# Gallery 原生移动端与 OHOS 桌面宿主

`gallery` 提供完整展示内容和启动配置；此 crate 仅提供 Android NativeActivity 与 OHOS Ability 的 `cdylib` 入口。两端在启动前把内嵌主题、样式和示例资源安装到应用数据目录，使用同一份页面、Builder / `ui!` 和 full features。

## Android arm64

设置 `ANDROID_HOME`、`ANDROID_NDK_HOME`（或 `NDK_HOME`）、`JAVA_HOME`，安装 SDK platform 35 或更新版本、build-tools、Zig 和 cargo-zigbuild：

```powershell
./examples/gallery_mobile/build-android.ps1
```

脚本通过 zigbuild 编译，使用 NDK 的 Bionic libc 描述和库目录链接，再以 aapt2、zipalign 和 apksigner 打包。默认生成本地测试签名 APK；输出目录中的 P12 仅用于本机重复安装，不是发布身份，不能提交或用于商店发布。最低 API 28、target API 35，包含 arm64-v8a 原生库与 NativeActivity 入口。

## OHOS 手机 / 平板与桌面

设置 `DEVECO_SDK_HOME` 为 DevEco Studio 的 `sdk` 目录，安装 API 22 或更新版本，并设置 `JAVA_HOME`。使用 DevEco 自带 Node、OHPM 和 Hvigor：

```powershell
./examples/gallery_mobile/build-ohos.ps1 -Device phone
./examples/gallery_mobile/build-ohos.ps1 -Device desktop
```

两次编译都使用 `aarch64-unknown-linux-ohos`，但 features 与 HAP 声明不同：

| 产物 | deviceTypes | Rust features | 桌面集成 |
| --- | --- | --- | --- |
| phone | phone、tablet | full | 不编译 jadix_window_state，不导入桌面状态栏 kit |
| desktop | 2in1 | full、ohos-desktop | 编译窗口状态持久化，独立 DesktopAbility 提供状态栏图标、菜单和弹窗 |

Android 与 iOS 也不在 `jadix_window_state` 依赖图中。普通 Windows/Linux/macOS Gallery 保留该依赖。不要在 OHOS 手机构建上使用 `--all-features`，因为它会显式打开 `ohos-desktop`。

OHOS 使用 zigbuild 驱动及 SDK clang 链接器。Zig 0.16 对该目标的 libc/compiler-rt ABI 支持不足，不能用通用 Linux GNU/musl 库替代 OHOS 系统库。脚本打包 `libgallery_mobile.so` 与 SDK 的 `libc++_shared.so`。

每个配置均保留 release unsigned HAP；默认使用 SDK 签名工具生成本地测试证书链，另产出经签名并验证的 HAP。测试密钥仅保存在被忽略的输出目录中，不修改 SDK 或用户证书。该测试签名只适用于信任此测试根证书的系统，不等同于可安装到零售 HarmonyOS 设备的 Huawei 开发者签名。零售设备安装需要其真实设备授权与开发者 profile；`-Unsigned` 可只输出待正式签名的 HAP。

`-SkipBuild` 仅用于原生库已由对应配置编译完成后的重新打包，不会检查或改变库的 features。先备份已完成产物，再切换另一配置。

## richerfu 的图标与托盘方案

检索日期：2026-10-30。原始依据：

- richerfu 的 [GitHub 项目](https://github.com/richerfu)与 [ohos-rs/gpui-template](https://github.com/ohos-rs/gpui-template)：NativeAbility、插件工厂和 module/AppScope 资源图标。
- [harmony-contrib/openharmony-ability](https://github.com/harmony-contrib/openharmony-ability)，本次核对的 main commit 为 `7c91dfa975045ee9cfb46f774883df00fcd299d2`。仓库的 [statusbar Rust facade](https://github.com/harmony-contrib/openharmony-ability/tree/7c91dfa975045ee9cfb46f774883df00fcd299d2/crates/plugin-statusbar) 和 [ArkTS 插件](https://github.com/harmony-contrib/openharmony-ability/tree/7c91dfa975045ee9cfb46f774883df00fcd299d2/plugins/statusbar) 是成对桥接；OHPM 发布 `@ohos-rs/ability-plugin-statusbar@1.0.0-beta.3`，元数据作者为 richerfu。main 的 Rust manifest 已为 beta.4，使用时应匹配版本与会话契约。
- [WindowIcon.ets](https://github.com/harmony-contrib/openharmony-ability/blob/7c91dfa975045ee9cfb46f774883df00fcd299d2/plugins/window/src/main/ets/WindowIcon.ets) 对 API 22+ 的 pc/2in1 使用 `setMissionWindowIcon`；应用/启动图标由资源和 manifest 管理。当前 Jadix 的 winit-ohos 已调用 window 插件的 `set_window_icon`，无需为此改用 Linux 图标接口。
- 本机 DevEco API 22 的 `@kit.StatusBarExtensionKit` 导出 `statusBarManager`、`StatusBarViewExtensionAbility`；能力为 `SystemCapability.PCService.StatusBarManager`，支持添加、移除、更新图标/菜单和提示。旧命名 `DeskTopExtensionKit` 仍在 SDK 中。手机不能假定具备此能力。
- richerfu/tauri 的 tray 模块仍依赖 `tray-icon`；它不能证明通用 Linux tray 后端适用于 OHOS。

当前桌面宿主直接使用 SDK statusBarManager，并在运行时检查 syscap，管理自身状态栏项生命周期。它提供真正的桌面图标、恢复菜单和状态弹窗；框架通用 `jadix::app::tray_icon` 的 OHOS 适配仍需单独对接上述成对插件，不能把 Linux `ksni` 当作 OHOS 实现。Linux 使用 ksni，OHOS 在 Cargo 与原生托盘 cfg 中明确排除。

编译、包内结构与签名验证不能替代真机启动和托盘交互验证。
