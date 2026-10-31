# Gallery 原生移动端与 OHOS 桌面宿主

`gallery` 提供完整展示内容和启动配置；此 crate 仅提供 Android NativeActivity 与 OHOS Ability 的 `cdylib` 入口。两端在启动前把内嵌主题、样式和示例资源安装到应用数据目录，使用同一份页面、Builder / `ui!` 和 full features。

## Android arm64 / x86_64

设置 `ANDROID_HOME`、`ANDROID_NDK_HOME`（或 `NDK_HOME`）、`JAVA_HOME`，安装 SDK platform 35 或更新版本、build-tools、Zig 和 cargo-zigbuild：

```powershell
./examples/gallery_mobile/build-android.ps1
./examples/gallery_mobile/build-android.ps1 -Architecture x86_64
```

脚本通过 zigbuild 编译，使用对应架构的 NDK Bionic libc 描述、展开库和系统库目录链接，再以 aapt2、zipalign 和 apksigner 打包。默认架构为 arm64；x86_64 输出到 `dist/android-x86_64`，包内 ABI 为 x86_64，可在对应 Android 模拟器中直接运行。默认生成本地测试签名 APK；输出目录中的 P12 仅用于本机重复安装，不是发布身份，不能提交或用于商店发布。最低 API 28、target API 35。不同架构若使用不同签名密钥，切换安装前需卸载旧包，或预先将同一测试 P12 放入各输出目录。

构建后连接设备（或支持 arm64 转译的 Android 模拟器），执行安装、启动与首帧检查：

```powershell
./examples/gallery_mobile/verify-android.ps1 -Serial <adb设备序列号> -Apk ./examples/gallery_mobile/dist/android-arm64/jadix-gallery-android-arm64-release.apk
./examples/gallery_mobile/verify-android.ps1 -Serial emulator-5554 -Apk ./examples/gallery_mobile/dist/android-x86_64/jadix-gallery-android-x86_64-release.apk
```

检查脚本保留启动日志、前台 Activity 和截图，进程提前退出、未进入前台或画面近乎单色都会失败。截图仍需人工核对实际内容。GLES 路径使用无 compute/storage 要求的 limits，并以整数纹理保存裁剪数据，支持片段着色器 storage buffer 数量为零的 GLES 3.0 设备。Android 上 Jadix 的线程局部状态共享一个系统 TLS 槽，避免完整 Gallery 的组件初始化耗尽 pthread key。

默认等待 45 秒，供软件渲染模拟器完成首次着色器编译；可用 `-WaitSeconds` 调整。验证必须看到 Gallery 内容，启动 Activity 或进程存活不足以判定成功。

## OHOS 手机 / 平板与桌面

设置 `DEVECO_SDK_HOME` 为 DevEco Studio 的 `sdk` 目录，安装 API 22 或更新版本，并设置 `JAVA_HOME`。使用 DevEco 自带 Node、OHPM 和 Hvigor：

```powershell
./examples/gallery_mobile/build-ohos.ps1 -Device phone
./examples/gallery_mobile/build-ohos.ps1 -Device desktop
./examples/gallery_mobile/build-ohos.ps1 -Device phone -Architecture x86_64
./examples/gallery_mobile/build-ohos.ps1 -Device desktop -Architecture x86_64
```

默认架构使用 `aarch64-unknown-linux-ohos`；`-Architecture x86_64` 使用 `x86_64-unknown-linux-ohos` 和对应 SDK 库，供 x86_64 模拟器使用。每个设备类型、架构组合使用独立输出目录 `dist/ohos-<Device>-<Architecture>`，features 与 HAP 声明如下：

Windows 上 Hvigor 会拒绝超过 259 字符的生成路径。输出目录较深时，可额外传入 `-ProjectDirectory D:/tmp/gallery-ohos-phone-x64`，将临时工程放在短路径，最终 HAP 与原生库备份仍写入输出目录。不同设备类型、架构应使用不同工程目录。

| 产物 | deviceTypes | Rust features | 桌面集成 |
| --- | --- | --- | --- |
| phone | phone、tablet | full | 不编译 jadix_window_state，不导入桌面状态栏 kit |
| desktop | 2in1 | full、ohos-desktop | 编译窗口状态持久化，独立 DesktopAbility 提供状态栏图标、菜单和弹窗 |

Android 与 iOS 也不在 `jadix_window_state` 依赖图中。普通 Windows/Linux/macOS Gallery 保留该依赖。不要在 OHOS 手机构建上使用 `--all-features`，因为它会显式打开 `ohos-desktop`。

OHOS 使用 zigbuild 驱动及 SDK clang 链接器。Zig 0.16 对该目标的 libc/compiler-rt ABI 支持不足，不能用通用 Linux GNU/musl 库替代 OHOS 系统库。脚本打包 `libgallery_mobile.so` 与 SDK 的 `libc++_shared.so`。

每个配置均保留 release unsigned HAP。默认不生成自签名包：零售 HarmonyOS 设备不能直接安装 unsigned HAP，需要 Huawei 开发者证书及授权对应设备的 profile。

本机 HarmonyOS 6.0.2 x86_64 模拟器接受 unsigned HAP，真机签名要求仍然适用。宿主通过 `runtimeOnly.packages` 和本地原生模块类型包声明动态导入，确保 `@ohos-rs/ability` 能找到 `libgallery_mobile.so`。OHOS 与 Android 均使用 Jadix 的 TLS 汇总存储，避免系统 pthread key 限额导致原生 abort。原生启动日志和 panic 分别写入应用数据目录的 `gallery.log`、`gallery-panic.log`，供 HDC 调试时取回。

安装并检查模拟器启动时，显式选择 `hdc list targets` 中对应手机或桌面的目标：

```powershell
./examples/gallery_mobile/verify-ohos.ps1 -Serial 127.0.0.1:5557 -Hap ./examples/gallery_mobile/dist/ohos-phone-x86_64/jadix-gallery-ohos-phone-x86_64-release-unsigned.hap
./examples/gallery_mobile/verify-ohos.ps1 -Serial 127.0.0.1:5559 -Hap ./examples/gallery_mobile/dist/ohos-desktop-x86_64/jadix-gallery-ohos-desktop-x86_64-release-unsigned.hap
```

检查脚本核对设备返回的安装结果，启动 Ability，等待进程并保存 HiLog 和截图。必须进一步查看截图并操作实际控件，启动图标仍在或只有进程存活均不能视为完成渲染。

在 DevEco Studio 中打开 `examples/gallery_mobile/ohos`，连接目标设备，登录开发者账号，在 Signing Configs 为 `org.jadix.gallery` 启用自动签名，生成有效证书及包含目标设备 UDID 的 profile。然后把本机生成的配置传入构建脚本：

```powershell
./examples/gallery_mobile/build-ohos.ps1 -Device phone -SigningConfigFile ./examples/gallery_mobile/ohos/build-profile.json5
./examples/gallery_mobile/build-ohos.ps1 -Device desktop -SigningConfigFile ./examples/gallery_mobile/ohos/build-profile.json5
hdc install ./examples/gallery_mobile/dist/ohos-phone-arm64/jadix-gallery-ohos-phone-arm64-release-signed.hap
```

脚本通过 DevEco 的 JSON5 解析器读取指定配置，将选中的 `app.signingConfigs` 写入被忽略的构建目录，检查 profile 包名、有效期、设备授权列表及材料文件后，由 Hvigor 使用该身份签名；`-SigningConfigName` 默认 `default`。证书密码、设备 UDID 和生成的签名配置不能提交。profile 的本地检查及 `verify-app` 不能代替设备的证书信任与 UDID 校验。

`9568257 fail to verify pkcs7 file` 表示设备拒绝了签名/profile 的验证；本地测试根签名无法代替 Huawei 信任链。`9568320 no signature file` 表示安装了 unsigned 包。应重新生成适配包名、仍在有效期内且授权目标设备的 Huawei profile，并安装 `release-signed.hap`，不要通过重命名 unsigned 包解决。仅在系统已显式信任测试根时才使用 `-LocalTestSignature`；该开关输出带有 `local-test-signed` 标记的包并显示警告。

`-SkipBuild` 仅用于原生库已由对应配置编译完成后的重新打包，不会检查或改变库的 features。先备份已完成产物，再切换另一配置。重新签名已备份的手机或桌面库时，通过 `-SkipBuild -NativeLibrary <对应的备份.so>` 明确选择原生库，避免共享 target 目录中的另一配置被误装入包中。

