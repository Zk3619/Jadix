# @NAME@

独立 Android 原生应用；业务页面位于 `src/lib.rs`，Activity 与原生探针位于 `android/src`。`src/android.rs` 连接 JNI 和拥有的 Rust 根；关闭 Activity 释放根及响应式 owner。应用包含计数器、受控输入、复选框和恢复检查点。

使用生成报告中的仓库工具路径，显式选择 SDK、NDK、JDK 和 ABI：

```powershell
python "@ROOT@/scripts/native.py" build --platform android --project . --android-sdk C:/Android/Sdk --ndk C:/Android/Sdk/ndk/29.0.14206865 --java-home D:/Java/jdk-17 --android-target x86_64-linux-android --offline --output ../android-build.json
python "@ROOT@/scripts/native.py" inspect --artifact ../android-build.json --output ../android-inspect.json
python "@ROOT@/scripts/native.py" run --artifact ../android-build.json --adb C:/Android/Sdk/platform-tools/adb.exe --serial emulator-5580 --output ../android-run.json
python "@ROOT@/scripts/native.py" package --artifact ../android-build.json --output ../android-package.json
```

开发 APK 的包名为 `@APPLICATION_ID@`；Activity 和 JNI 桥接类保留 `dev.jadix.nativeui` 命名空间，动态库固定为 `libjadix_native_controls.so`。修改 Cargo 包名或 application ID 时同步修改 `native-android.json` 和 manifest。输出目录放在工程外部或 `target/`，构建冻结源码和锁文件，拒绝中途变化。

`NativeAppChecks` 是应用内的程序化原生探针，覆盖原生点击／编辑、控件保留、状态恢复和旧根拒收。统一 `run` 用新鲜 nonce 绑定本次探针，显式选择设备并检查 ABI／API；安装前拒绝任何同名包登记，结束后只清理本次拥有的安装并确认包登记恢复。添加 `--process-recovery` 后，运行器另开 Activity task，通过实际控件点击与输入设置计数／文本／勾选，退到后台并 `am kill`，确认旧 PID 消失后重开保留的 task，检查不同的新 PID 与业务值恢复。此路径依赖 Activity 保存的 task 状态，不覆盖 force-stop、磁盘日志或断电恢复。

Activity 保存有大小上限的 Bundle 检查点以应对重建；未提供磁盘日志、断电恢复、更新迁移或生产签名。检查点缺失或无效时明确报错后打开新页面。验证记录不代表物理 IME、辅助技术、其他系统镜像或发行验收。
