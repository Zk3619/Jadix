# jadix_window_state

跨平台窗口状态持久化（位置 / 尺寸 / 最大化 / 全屏 / 装饰 / 可见性）。状态以 **TOML** 落盘；默认写在**运行路径**（`std::env::current_dir()`），可用 Builder 或环境变量覆盖。

这是 W4 伞形下的**可选能力 crate**（与 `jadix_icons` / `jadix_devtools` 同分层）：Jadix 核心不内置持久化，只保留极薄的 `WindowLifecycleHook`。应用显式依赖本 crate。

参考实现：[tauri-plugin-window-state](https://github.com/tauri-apps/plugins-workspace/tree/v2/plugins/window-state)（JSON → TOML，默认路径从 app-config-dir → 运行路径）。

| Platform | Supported |
|----------|-----------|
| Linux    | ✓         |
| Windows  | ✓         |
| macOS    | ✓         |
| Android / iOS | ×    |

## Install

```toml
[dependencies]
jadix_window_state = { version = "0.1", features = ["jadix"] }
```

## Usage

```rust
fn main() {
    jadix_window_state::Builder::new()
        .with_filename("my-app.window-state.toml")
        // .with_path("D:/data/windows.toml")  // 覆盖默认运行路径
        // .with_storage(StorageLocation::ExecutableDir)
        .install()
        .expect("window state");

    jadix::app::run(build_ui);
}
```

默认文件：`{cwd}/window-state.toml`。覆盖优先级：

1. `Builder::with_path` / `with_storage(File | Directory)`（代码显式路径）
2. 环境变量 `JADIX_WINDOW_STATE_PATH`（文件或目录）
3. `StorageLocation::CurrentDir`（默认运行路径）或 `ExecutableDir`

主窗默认 label 为 `main`。副窗须在 `WindowOptions::window_label` 上命名才会被跟踪。

通过 `Builder::install` 接入 Jadix 时，创建回调仅恢复几何等状态，首次显示和聚焦由
渲染器负责，避免 GPU 初始化期间提前暴露空白窗口。保存的可见性仍保留；手动调用
`WindowStateManager::restore` 时仍按 `StateFlags::VISIBLE` 恢复可见性。

```toml
[windows.main]
width = 1440
height = 900
x = 80
y = 40
prev_x = 80
prev_y = 40
maximized = false
visible = true
decorated = true
fullscreen = false
```
