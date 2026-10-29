# Jadix Gallery Linux release

此包包含完整 Gallery、主题、样式与示例资源。将整个目录解压到可写位置，运行 `./gallery`；窗口状态文件保存在当前工作目录。

## 运行环境

本次包以 x86_64、Ubuntu 24.04 / glibc 2.39 为基线，需要可用的 X11 或 Wayland 会话，以及 GTK 3、WebKitGTK 4.1 与图形驱动。在 Ubuntu 24.04 可安装：

```sh
sudo apt-get install libgtk-3-0t64 libwebkit2gtk-4.1-0 libxkbcommon-x11-0 mesa-vulkan-drivers fonts-noto-cjk
./gallery
```

`libxkbcommon-x11-0` 由 X11 输入后端动态加载；缺少该库时，即使 `ldd gallery` 没有缺失项，X11 窗口仍无法启动。中文显示需要 Noto CJK 或其他受支持的系统字体。托盘需要桌面环境提供 StatusNotifierItem 服务。

Gallery 同时编译 GLES 与 Vulkan，GL 不可用时可继续尝试 Vulkan。在仅提供 CPU 驱动的环境会使用 llvmpipe，性能不代表硬件 GPU。可通过 `JADIX_GPU_BACKEND=vulkan ./gallery` 优先选择 Vulkan。

WSLg 的窗口桥接、主题 portal 与 GPU 能力取决于宿主环境。本次已在 WSLg 的 X11 会话验证实际渲染；若 Wayland 会话连接异常，可用 `env -u WAYLAND_DISPLAY ./gallery` 检查 X11 路径。

## 从源码构建

安装 Rust、Zig、cargo-zigbuild、pkg-config、`libgtk-3-dev` 与 `libwebkit2gtk-4.1-dev`，在仓库根目录运行 `bash examples/gallery/build-linux.sh`。目标默认为 `x86_64-unknown-linux-gnu.2.39`。使用其他 glibc 基线时，GTK/WebKit 等系统库的 libc 要求也必须匹配。
