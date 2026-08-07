# 打包体积与运行内存

以 `examples/calculator` 为基准，记录一次体积 / 内存优化的实测数据、结论，以及后续继续裁剪依赖的判断方法。

测试环境：Windows 10.0.26200，Intel(R) UHD Graphics（驱动 32.0.101.6790），`rustc 1.95.0`，`profile.release`（`opt-level="z"` + fat LTO + `strip` + `panic="abort"`）。

---

## 1. 优化结果

| 指标                             | 优化前                 | 优化后                     |
| -------------------------------- | ---------------------- | -------------------------- |
| 可执行体积                       | 6,086,656 B（5.80 MB） | **4,635,136 B（4.42 MB）** |
| `.text` 段                       | 4.4 MiB                | 3.5 MiB                    |
| 任务管理器「内存」（专用工作集） | ~140 MB                | **26.1 MB**                |
| Private Bytes                    | 126.6 MB               | 43.6 MB                    |
| 工作集                           | 137.9 MB               | 90.8 MB                    |

`cargo test --workspace` 全部通过（56 项）。GL 后端下渲染与 DX12 一致：中文、`x²` 上标、圆角、描边均无差异。

---

## 2. 内存 140MB 的真实来源

不是框架占的。用 `examples/gpu_probe`（只有 winit + wgpu 清屏，**零 rui 代码**）分阶段测量：

| 阶段                  | Private      |
| --------------------- | ------------ |
| 进程启动              | 1.9 MB       |
| 创建窗口              | 2.9 MB       |
| `wgpu::Instance::new` | 3.5 MB       |
| `request_adapter`     | **54.8 MB**  |
| `request_device`      | **116.8 MB** |
| 稳定态（300 帧后）    | 117.4 MB     |

一个什么都不画的空白 wgpu 窗口就占 117 MB。计算器当时是 127 MB，也就是说 **rui 框架 + 字体 + 排版 + 应用逻辑总共只有约 10 MB**，其余全是 D3D12 用户态驱动在 `request_device` 时提交的私有堆。

同一颗 GPU、同一份探针代码，换后端：

| 后端   | Private     | 工作集   |
| ------ | ----------- | -------- |
| DX12   | 117.4 MB    | 121.3 MB |
| Vulkan | 103.7 MB    | 118.6 MB |
| **GL** | **35.4 MB** | 81.0 MB  |

这解释了参考项目 EUI-NEO 为什么只要 40 MB：它默认是 GLFW + **OpenGL**（Vulkan 仅可选）。差距来自图形驱动，不是 C++ 与 Rust 的差别，也不是框架实现优劣。

> 该数据来自 Intel 集显。NVIDIA / AMD 的 D3D12 驱动开销可能不同，换机器请重跑探针复测（见 §5）。

---

## 3. 已做的修改

### 3.1 GPU 后端按内存排序选择

`crates/rui/src/app/state.rs` 新增 `backend_candidates()` 与 `acquire_adapter()`：

- Windows 顺序为 GL → DX12 → Vulkan，任一步拿不到 adapter 自动降级；
- 全部失败后再尝试 WARP / lavapipe 软件适配器；
- 环境变量 `RUI_GPU_BACKEND=gl|dx12|vulkan|metal` 可把指定后端提到队首，其余仍作为兜底，所以请求一个未编译进来的后端不会崩，只会静默回退。

**收益：Private Bytes 127 MB → 43.6 MB。**

### 3.2 用静态区间表替换 `regex`

`regex` 全家（`regex` + `regex-automata` + `aho-corasick` + `regex-syntax`，约 398 KiB `.text`）在整个框架里**只被用于一处**：`emoji_detect.rs` 判断 `\p{Extended_Pictographic}`。

新增 `crates/rui/src/renderer/extended_pictographic.rs`：从 Unicode 官方 [`emoji-data.txt`](https://www.unicode.org/Public/UCD/latest/ucd/emoji/emoji-data.txt)（2025-07-25）提取 `Extended_Pictographic` 行并合并相邻区间，得到 156 个区间做二分查找，约 1.2 KB 数据。`regex` 依赖已从 `crates/rui/Cargo.toml` 删除。

**收益：-918 KB。** 原有 emoji 测试全部通过。

数据需要随 Unicode 版本更新时，重新生成该表即可（合并相邻区间是为了缩表，不影响语义）。

### 3.3 D3D12 后端改为可选

工作区 `wgpu` 依赖去掉 `dx12` feature，改由 `rui/backend-dx12` 按需开启。既然默认走 GL，D3D12 路径（含 wgpu-hal dx12、DXC 编译器、gpu-allocator）就是净负担。

**收益：-521 KB。**

需要 WARP 软件回退、或要对比 D3D12 行为时再开：

```toml
rui = { path = "../../crates/rui", features = ["backend-dx12"] }
```

### 3.4 清理无用依赖

- `euclid`：在 `crates/rui/Cargo.toml` 里声明了，但全项目零引用，已删除直接依赖（它仍会经 `etagere` ← `glyphon` 传递引入）。
- `clipboard-rs`：默认开启 `image` feature，会拖进 `image 0.25` 及其 bmp/png/jpeg 解码器，而我们只调用 `get_text` / `set_text`。改为 `default-features = false`。

**收益：体积 0 字节**（见 §4 的解释），但依赖图和编译时间都变小了。

---

## 4. 关于"移除用不到的依赖"

这是本次最反直觉的一条结论，先记下来避免以后做无用功。

**在 fat LTO 下，「不可达的依赖代码」已经不占体积了。** §3.4 移除 `euclid` 和 `image` 之后重新构建，`calculator.exe` 是 **4,635,136 字节，与移除前一字节不差**。链接器早就把它们全清了。

所以依赖分三类，只有第二类值得动：

### A 类：已经不可达 —— 删了只省编译时间

`euclid`、`clipboard-rs` 的 `image`、release 下的 `serde` / `serde_json`（`hot_reload` 的调用点全在 `#[cfg(debug_assertions)]` 里，所以整个模块在 release 中不可达，`cargo bloat` 的 60 条列表里根本没有 serde 的身影）。

这类清理的价值在于编译速度和供应链面积，**不要指望它减小体积**。

### B 类：可达但可选 —— 需要 `#[cfg(feature)]` 切断调用点才有收益

关键差别在于代码是否真的会被调用到。以 `png` 为例：计算器界面上没有 emoji，但 `decode_png` 是从 twemoji 渲染路径调用的，而那条路径在运行期对任意文本都可能触发，编译器无法证明它不可达，于是整份 PNG 解码器都被保留了下来。**光把 `png` 从 `Cargo.toml` 里删掉是编译不过的，必须先给调用点加 feature 门。**

当前二进制里这类候选项及其实测占用：

| 候选                                                                         | `.text` 占用 | 代价                    |
| ---------------------------------------------------------------------------- | ------------ | ----------------------- |
| `png` + `fdeflate` + `simd_adler32` + `crc32fast`（twemoji PNG 解码）        | ~45 KiB      | 失去彩色 emoji 位图回退 |
| `accesskit` + `accesskit_windows` + `accesskit_consumer`（+ 部分 `windows`） | ~50–70 KiB   | 失去读屏 / 无障碍支持   |
| `backend-dx12`                                                               | 521 KiB      | 已默认关闭 ✅           |

`png` 与 `accesskit` 加起来约 100–115 KiB，占当前体积 2.5% 左右，且都会牺牲真实功能。**建议先不做**，除非明确要做单一用途的极小分发。

### C 类：动不了的部分

| 组件                                                                                                                      | `.text`  | 说明                                                                                         |
| ------------------------------------------------------------------------------------------------------------------------- | -------- | -------------------------------------------------------------------------------------------- |
| `naga`（+ `codespan_reporting` + `enum2$<naga>`）                                                                         | ~576 KiB | WGSL 前端 + GLSL/HLSL 后端。wgpu 22 在 DX12 上不支持预编译着色器直通，去不掉                 |
| `wgpu_core` + `wgpu` + `wgpu_hal` + `glow` + `ash`                                                                        | ~813 KiB | Windows 上 wgpu 22 通过自身 target cfg **强制**链接 Vulkan 与 GLES 后端，无法用 feature 关掉 |
| `std`                                                                                                                     | 545 KiB  | 已开 `panic="abort"`；进一步压缩需 nightly（见 §6）                                          |
| 文本栈：`skrifa` + `rustybuzz` + `zeno` + `ttf_parser` + `cosmic_text` + `read_fonts` + `swash` + `unicode_bidi` + `yazi` | ~621 KiB | 复杂文本整形的必要代价                                                                       |
| `taffy`                                                                                                                   | 212 KiB  | 布局引擎                                                                                     |
| `rui`                                                                                                                     | 300 KiB  | 框架自身                                                                                     |

---

## 5. 后续使用

### 选择 GPU 后端

默认即最省内存（GL）。需要覆盖时：

```powershell
$env:RUI_GPU_BACKEND = "vulkan"   # gl | dx12 | vulkan | metal
.\target\release\calculator.exe
```

请求 `dx12` 而未启用 `backend-dx12` feature 时会自动回落到 GL，不会失败。

### 发版构建必须用 `-p`

**这是个真实的打包陷阱。** Cargo 会在一次构建内统一整个工作区的 feature，`examples/gallery` 启用了 `rui/bundled-lucide`（内嵌 0.8 MB 图标字体），于是：

```bash
cargo build --release --workspace     → calculator.exe 5.20 MB
cargo build --release -p calculator   → calculator.exe 4.42 MB
```

发版一律用 `-p <目标包>`，并以隔离构建的产物为准做体积回归。

### 复测内存

```powershell
# 分阶段看 GPU 驱动的内存地板（GL / Vulkan 可直接比，DX12 需 --features dx12）
cargo build --release -p gpu_probe
$env:PROBE_BACKEND = "gl"      # gl | vulkan | dx12
.\target\release\gpu_probe.exe

# 看实际进程占用（对齐任务管理器口径）
$p = Start-Process .\target\release\calculator.exe -PassThru -NoNewWindow
Start-Sleep 8
Get-Counter "\Process(calculator)\Working Set - Private"
Stop-Process -Id $p.Id -Force
```

`gpu_probe` 的 `dx12` feature 默认关闭是**故意的**：feature 统一会把 D3D12 后端带回其它二进制里，只在需要一次性对比时用 `--features dx12` 打开。

### 复测体积

```powershell
cargo build --release -p calculator
cargo bloat --release -p calculator --crates -n 40 --config "profile.release.strip=false"
```

---

## 6. 关于 2MB 目标

**保留 wgpu 的前提下达不到。**

最直接的证据是 `gpu_probe.exe` 本身：同一套 release profile，除清屏外什么都不做、不含任何 rui 代码，编出来 **2.37 MB**。这就是 wgpu 的地板。再叠加 taffy（212 KiB）、文本整形栈（约 621 KiB）、rui 自身（300 KiB），4.42 MB 已接近实际下限。

还能挤的部分，乐观估计到 3.8–4.0 MB：

- **nightly `build-std` + `panic_immediate_abort`**：`std` 目前 545 KiB，这条通常能砍掉其中大部分，是剩余最大的单项。需要 nightly 工具链 + `rust-src` 组件，当前机器只有 stable，未验证。
- `png` 可选化：~45 KiB。
- `accesskit` 可选化：~50–70 KiB。

真要到 2 MB，只能走 EUI-NEO 的路线：去掉 wgpu + naga（约 1.39 MB `.text`）换成手写 GL 3.3 后端配预编译 GLSL，并用 FreeType 替掉 cosmic-text 整形栈。这与"保留 wgpu"直接冲突，属于架构取舍而非优化。

相较之下，内存目标（40 MB）已经达成并大幅超出：26.1 MB。

你的这份实测文档写得非常专业且透彻。特别是通过 `gpu_probe` 剥离出驱动层内存开销、发现 fat LTO 下“不可达依赖不占体积”的结论，以及敏锐地指出 Cargo 工作区 feature 统一的陷阱，直接击中了 Rust GUI 优化的核心痛点。

基于你的测试数据和分析，我的核心建议是：**强烈建议继续保持当前 wgpu 实现，停止针对“2MB 极限体积”的死磕，将研发重心转移到功能完善与渲染管线调优上。**

以下是具体的分析与下一步行动指南：

### 一、 为什么建议“保持现状”？

1. **内存优化已达成“超额完成”的成就**
   26.1 MB 的专用工作集（Private Bytes）对于包含完整文本整形栈（cosmic-text/rustybuzz）、布局引擎（taffy）和现代 GPU 渲染管线的自绘框架来说，是一个**极其优秀**的数据。它不仅远低于 Electron/WebView 方案（150MB+），甚至优于许多传统 C++ 框架在 Win11 上的表现。既然已经打破了“wgpu 必定吃内存”的刻板印象，就没有必要为了省几 MB 内存去重写底层渲染器。
2. **体积优化的边际收益已极度递减**
   从 5.80 MB 降到 4.42 MB，你砍掉了近 1.5 MB 的冗余。但剩下的 2 MB 差距（4.42MB vs 2.37MB wgpu 地板）中，大头是 `std` 和 `naga`。
   - 如果用 Nightly `build-std` 压缩 `std`，会引入极高的 CI 维护成本和潜在的不稳定性。
   - 如果去掉 `naga` 改手写 GLSL，将彻底失去 wgpu 跨平台着色器编译的核心优势，直接退化回 EUI-NEO 的技术栈，得不偿失。
3. **4.42 MB 在桌面端完全是“黄金体积”**
   对比 Win11 自带的 UWP 计算器（依赖大量系统共享组件且安装包臃肿），或者动辄几十 MB 的跨平台方案，4.42 MB 的单文件绿色可执行程序在分发和用户体验上已经具备绝对优势。

---

### 二、 如果非要再挤一点（可选的微调方案）

如果你仍有“洁癖”想把体积逼近 3.5MB，可以尝试以下**低风险**操作（不建议动用 Nightly 工具链）：

1. **Feature-gate `accesskit` 和 `png` (twemoji)**
   正如你文档中 §4.B 提到的，这两个加起来约 100~115 KB。可以将它们做成默认关闭的 feature。对于不需要无障碍读屏和彩色 Emoji 的极简场景（如嵌入式设备或纯内部工具），可节省约 0.1 MB。
2. **尝试 UPX 加壳（终极物理压缩）**
   使用 UPX (Ultimate Packer for eXecutables) 对最终的 `calculator.exe` 进行压缩。对于 Rust 这种包含大量连续只读数据（如字体、Unicode 表）的二进制，UPX 通常能再压缩 **30% ~ 50%** 的体积（可能直接降到 2.5 MB 左右）。
   _注意：Windows Defender 偶尔会对 UPX 加壳的程序误报，且启动时会有一瞬间的解压 CPU 开销，需自行权衡。_
3. **剥离 `wgpu` 的 `dx12` 和 `vulkan` 验证层**
   确保在 release 模式下，`wgpu` 的 `validation` feature 被彻底关闭。有时默认的 `wgpu` 依赖树中会隐式带入验证层代码，显式在 `Cargo.toml` 中 `default-features = false` 并只开启 `gl` 后端，可能还能再挤出几十 KB。

---

### 三、 下一阶段研发重心转移建议（从“瘦身”到“强身”）

既然“骨架”已经足够轻盈，下一步建议将精力投入到以下维度，这才是决定一个 UI 框架能否被大规模采用的关键：

#### 1. 跨平台后端的 Fallback 稳定性测试

你的探针主要验证了 Windows 下的 GL/DX12/Vulkan。需要重点补充：

- **macOS**：macOS 下没有 GL 后端（只有 Metal）。你的 `backend_candidates()` 降级逻辑需要确保在 macOS 下能平滑请求 Metal，而不会因为找不到 GL 而 Panic。
- **Linux (Wayland/X11)**：验证在缺少 Vulkan 驱动的环境下，GL 后端是否能正常接管，以及 `winit` 在 Wayland 下的表现。

#### 2. 渲染管线的 Draw Call 优化 (Batching)

体积和内存达标后，下一个性能瓶颈通常是 **CPU 到 GPU 的指令提交开销**。

- 检查你的渲染器是否实现了 **Batching（批处理）**。比如将多个相同材质的 UI 矩形（如计算器的所有数字按键）合并为一个 Draw Call。
- 确保文本渲染使用了 **Texture Atlas（纹理图集）**，而不是每个字符单独上传纹理。

#### 3. 开发者体验 (DX) 与生态建设

- **UI Inspector**：类似 Flutter DevTools 或 React Developer Tools，允许开发者在运行时查看 UI 树、检查布局边界（taffy 的 Flexbox 计算结果）、审查样式。这比单纯减小体积更能吸引开发者使用你的框架。
- **Hot Reload 增强**：利用你保留的 `hot_reload` 机制，优化 WGSL 着色器的热更新，让 UI 开发者调整颜色和动画时无需重启应用。

#### 4. 复杂交互与动画系统

Win11 计算器的精髓在于其流畅的按压缩放动画、主题切换的平滑过渡。

- 引入或完善基于时间轴的动画系统（如 Spring 弹簧动画、Easing 贝塞尔曲线）。
- 确保动画是在 GPU 端计算（通过 Uniform Buffer 传递时间变量），而不是在 CPU 端每帧重新计算 taffy 布局，从而保持极低的 CPU 占用率（这对于笔记本续航至关重要）。

**总结**：你目前的优化成果已经是一份教科书级别的 Rust 性能调优报告。**保持当前的 wgpu 架构，接受 4.4MB 的体积和 26MB 的内存，把才华挥洒在渲染管线优化和开发者工具链上，是这个框架走向成熟的最佳路径。**

---

## Release checklist（体积 / 性能）

发布或打 tag 前建议：

1. **体积**：`cargo build -p calculator --release`，记录 exe 大小；对照本节历史表。
2. **内存**：可选跑 `examples/gpu_probe` + 计算器，确认 GL 默认路径 Private Bytes 未回退。
3. **布局基线**：`cargo bench -p rui --bench layout_baseline`（见 `crates/rui/benches/layout_baseline.rs`）；可选 `--features tracing`。
4. **测试**：`cargo test -p rui --lib` + CI `offscreen` job（`RUI_FORCE_SOFTWARE=1`）。
5. **Features**：默认精简；文档列出 `bundled-*` / `emoji-svg` / `backend-dx12` / `system-tray` 对体积的影响（见 [PUBLISH.md](PUBLISH.md)）。
