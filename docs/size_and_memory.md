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
| `naga`（+ `codespan_reporting` + `enum2$<naga>`）                                                                         | ~576 KiB | WGSL 前端 + 各后端 IR。wgpu 30 仍走运行时编译，预编译着色器直通不是默认路径                 |
| `wgpu_core` + `wgpu` + `wgpu_hal` + `glow` + `ash`                                                                        | ~813 KiB | wgpu 30 起 Vulkan/GLES/Metal 由 workspace features 显式打开；DX12 仍走 `backend-dx12` |
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
$env:PROBE_FRAMES = "300"      # 清屏帧数，跑完后自动退出（默认 300）
.\target\release\gpu_probe.exe

# 只看 device 创建前的 Private（测完自动退出，不会挂死窗口）
$env:PROBE_STOP = "device"     # instance | surface | adapter | device
Remove-Item Env:PROBE_FRAMES -ErrorAction SilentlyContinue
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

- **UI Inspector**：B1 P0–P2 已落地（`devtools` feature）。运行时查看 UI 树、点选高亮、盒模型、Layout、样式/CSSOM 声明编辑、Signal 依赖图、`template_id` 源码定位。见 [`devtools.md`](devtools.md)。
- **Hot Reload 增强**：利用你保留的 `hot_reload` 机制，优化 WGSL 着色器的热更新，让 UI 开发者调整颜色和动画时无需重启应用。

#### 4. 复杂交互与动画系统

Win11 计算器的精髓在于其流畅的按压缩放动画、主题切换的平滑过渡。

- 引入或完善基于时间轴的动画系统（如 Spring 弹簧动画、Easing 贝塞尔曲线）。
- 确保动画是在 GPU 端计算（通过 Uniform Buffer 传递时间变量），而不是在 CPU 端每帧重新计算 taffy 布局，从而保持极低的 CPU 占用率（这对于笔记本续航至关重要）。

**总结**：你目前的优化成果已经是一份教科书级别的 Rust 性能调优报告。**保持当前的 wgpu 架构，接受 4.4MB 的体积和 26MB 的内存，把才华挥洒在渲染管线优化和开发者工具链上，是这个框架走向成熟的最佳路径。**

---

## 7. 滚动与 CPU 占用（架构审计 · 2026-08-20）

对应 [AUTHORITY.md](AUTHORITY.md) **Wave P §5.6**（WP-P.3 流畅度）。本节记录**源码事实**与诊断方法，供 P.0.3 / P.3.1 基线与优化前后对比。

### 7.1 结论摘要

| 问题 | 答案 |
|------|------|
| 滚动 CPU 5%–7% 正常吗？ | **关核显 / 无合成路径**上常见；**本机 release + 核显 ~1% CPU** 时属正常 |
| 核显 GPU 40%、独显 idle？ | **预期**（ADR-3 Win 默认 GL + `LowPower` 集显）；Fill 在核显，非 bug |
| 是否持续渲染（Poll 循环）？ | **否**。`ControlFlow::Wait` + 按需 `request_redraw()` |
| 滚动是否触发 Taffy 重布局？ | **否**（纯滚动帧 `scroll_only: true`） |
| 滚动时 CPU 主要做什么？ | 本机 profile：**collect≈0.1–2.4 ms/帧**，text≈0.1–1.4 ms；关核显机器更高 |
| 是否有 scroll layer cache？ | **否**（Wave P **不做**；排期 **≥ B3 backdrop-blur 离屏**，见 §7.6） |
| Wave P 是否过关？ | **☑ 已退出（2026-08-20）**；满载 RSS **≤85 MiB**；profile p99≈7.3 ms |

### 7.2 架构数据流（简图）

```text
winit 事件 → RedrawRequested?
  → tick（平滑滚动 / 惯性 / 动画）
  → update_scroll_offset_in_place（scroll: → Element.scroll_offset）
  → render
      → scroll_only? → 跳过 Taffy compute_layout
      → collect_instances（scroll_acc 平移 + clip-cull）  ← 主要 CPU 成本
      → prepare_text + glyphon（shaped_cache / text_force_fast_mode）
      → scroll_fp 变化? → mark_full_redraw → FullClear present  ← 主要 GPU/提交成本
  → 动画中 → request_redraw() → 下一帧
VSync: PresentMode::Fifo
```

关键源码：

- 事件循环：`crates/rui/src/app/lifecycle.rs`（`ControlFlow::Wait`）
- VSync / present：`crates/rui/src/app/state.rs`（`Fifo`，`desired_maximum_frame_latency: 1`）
- scroll-only 布局：`crates/rui/src/layout/layout_tree.rs`（`build_and_layout_fast(..., scroll_only, ...)`）
- 滚动 → FullClear：`crates/rui/src/app/state.rs`（`scroll_offset_fingerprint` → `mark_full_redraw`）
- damage 策略：`crates/rui/src/app/damage.rs`（滚动帧禁止 PartialLoad scissor）

### 7.3 已实现的优化（勿重复排查）

1. **事件驱动 + VSync**：非 game-loop 式 Poll；Fifo 限制刷新率。
2. **布局与滚动解耦**：`Element.scroll_offset` 不参与 flex 尺寸；collect 时应用偏移。
3. **滚动信号 `set_silent`**：避免 reactive 全树 rebuild（见 `scroll_viewport.rs` 契约）。
4. **文本**：`shaped_cache` + glyphon `TextAtlas`；滚动时 `text_force_fast_mode`。
5. **滚动条**：`ViewportsOnly` prepass + overlay collect，thumb 几何不每帧写 Taffy style。
6. **Canvas 叶**：内容 dirty 缓存，滚动不重跑 `paint`（WP-3.12）。

### 7.4 主要瓶颈（对应观测占用）

| # | 瓶颈 | 说明 |
|---|------|------|
| 1 | 关核显机器 CPU 5%–7% | collect + text prepare + 提交；无 GPU 合成 offload |
| 2 | 本机核显 ~40%（release 滚动） | **FullClear + instanced fill + glyphon**；CPU `encode≈0.1 ms` 极低 |
| 3 | 切页 StructureChanged | Gallery 分区切换全树 rebuild |
| 4 | 无 layer cache | 理想态见 **B3/B8 backlog**（§7.6），**不在 Wave P** |
| 5 | VirtualList 跨窗 | remount → StructureChanged |

### 7.5 诊断命令

**CPU 口径（必做）**

任务管理器 → 详细信息 → 右键列 → 勾选「CPU」。同时记录：

- **CPU 使用率**（所有逻辑处理器的平均值 = 总占用）
- **CPU** 列（当前线程占**单逻辑处理器**的百分比）

8 核机器上总占用 5% ≈ 单核 40%；总占用 7% ≈ 单核 56%。

**帧耗时（debug 构建）**

```powershell
$env:RUST_LOG = "warn"
# debug：默认打 render profile
cargo run -p gallery
# release：需显式打开（建议 --features runtime-logging 以输出 warn）
$env:RUI_RENDER_PROFILE = "1"
cargo run -p gallery --release --features runtime-logging
# 或自动化脚本（PostMessage 滚轮，约 10s）：
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/p03_scroll_profile.ps1
# 滚动 ScrollArea 时 stderr 约 1s 一次：
# fps≈…, pre=…ms, layout=…ms, draw=…ms [layout_tree: … collect=…] [upload=…, acquire=…, text=…, encode=…, submit=…, present=…]
```

**GPU 口径**

任务管理器 → 性能 → GPU → 选 **Intel 核显** 与 **独显** 各看一条引擎（通常 **3D**）。滚动时核显 3D 升高、独显 idle = ADR-3 预期路径。

**Profile（P.3.1 推荐）**

```powershell
# 安装 samply（Windows 友好）
cargo install samply
cargo build -p gallery --release
samply record .\target\release\gallery.exe
# 复现：打开 ScrollArea 分区，连续滚轮 10s，停止录制后看火焰图
```

关注栈顶：`collect_instances`、`prepare_text`、`encode`、`write_buffer`、`queue.submit`。

**damage 计数（逻辑层，非 GPU）**

Gallery → 布局 → WP-2.2：同宽 bind_text 应增 `PartialLoad` 计数；滚动应增 `FullClear` 计数。

### 7.6 优化路线图（与 AUTHORITY WP-P.3 对齐）

| 阶段 | 动作 | 状态 / 排期 |
|------|------|-------------|
| P0 | P.3.1 profile 定 Top-3 | ☑ 2026-08-20 |
| P1 | scroll_only flex sibling band 剪枝 | ☑ 2026-08-20 |
| P1 | collect scratch 回收 + `scale≈1` 原地缩放 | ☑ 2026-08-20 |
| P1 | wheel sticky + 嵌套 retarget | ☑ 2026-08-20 |
| **≥ B3** | **backdrop-blur 离屏缓存**（§7 B3） | 未做 · **layer cache 不早于本项** |
| **≥ B3** | **scroll viewport layer cache**（offscreen + GPU blit） | 未做 · 与 B3 共用 retain/offscreen 管线 |
| **≥ B8** | **fingerprint 跨帧 display list** | 未做 · B8 前置 |
| **≥ B8** | **viewport union damage + PartialLoad** | 未做 · 须 draw-list 裁剪，否则 retain 更慢 |

**Wave P 决策（2026-08-20）**：退出门全过 → **Wave P 已关闭**；layer cache / display list / B8 留 **W4+ / ≥B3 backlog**。

**禁区**（与 AUTHORITY §5.6 一致）：换 wgpu/Skia；为帧率回 VDOM；无 draw culling 强开 retain。

### 7.7 基线记录表（P.0.1–P.0.4 · 2026-08-20）

| 指标 | 测量值 | 日期 | 环境 |
|------|--------|------|------|
| Gallery.exe release 体积（默认 feature） | 9,177,600 B（≈8.76 MiB） | 2026-08-20 | P.1 快照 |
| Gallery.exe + `runtime-logging` | **9,041,408 B（≈8.62 MiB）** | 2026-08-20 | `cargo build -p gallery --release --features runtime-logging` |
| Feature 增量（参考 P.1，非复测） | `+devtools` / `+scene3d` / `+images` / `+system-tray` 各 +数百 KiB–数 MiB | 2026-08-20 | 见 AUTHORITY P.1 bloat |
| Gallery 启动 idle RSS（默认分区） | **105,979,904 B（≈101.1 MiB）** | 2026-08-20 | Win，启动 5s 后 Task Manager Working Set |
| Gallery 满载 RSS | **≤85 MiB**（侧栏全部分区逐项打开） | 2026-08-20 | Win Task Manager；退出门 ≤120 MiB ☑ |
| 真·空窗 RSS（`gpu_probe` device 后） | **≈30.4 MiB Private** | 2026-08-20 | §8.1 GL 集显 |
| 空窗 idle CPU（总 / 单核） | _待测_ | | |
| ScrollArea 滚动 CPU（总 / 单核） | **~1% 总**（本机 release，用户观测） | 2026-08-20 | 核显可用；关核显机器 **5–7%**（开发期） |
| ScrollArea 滚动 GPU | **核显 ~40%**（3D）；**独显 ~0%** | 2026-08-20 | 同场景手工观测 |
| 滚动 profile（CPU 帧分量） | 见 §7.8 | 2026-08-20 | `RUI_RENDER_PROFILE=1` + `scripts/p03_scroll_profile.ps1` |
| collect_ms / draw_ms（bench） | scroll_only 100 行亚毫秒级 collect | 2026-08-20 | `layout_baseline -- scroll` |

### 7.8 P.0.3 profile 解读（本机 · 2026-08-20）

**命令**：`powershell -File scripts/p03_scroll_profile.ps1`（release + `runtime-logging`，`RUI_RENDER_PROFILE=1`，约 10s 合成滚轮）。

**滚动段典型一行**（略去启动首秒）：

```text
render profile(last ~1024ms): fps≈2.0, pre=0.07ms, layout=0.07ms, draw=7.26ms
  [layout_tree: rebuild=0.00 setup=0.00 taffy=0.00 collect=2.41]
  [upload=0.90, acquire=0.00, text=1.36, encode=0.14, submit=4.46, present=0.38]
```

| 分量 | 滚动段观测 | 解读 |
|------|------------|------|
| **pre+layout+draw** | **p50≈3.3 ms，p99≈7.3 ms**（10s 内各秒平均） | **≪ 16.7 ms 退出门**（CPU 侧记账） |
| **collect** | 0.07–2.41 ms | scroll-only + band cull 生效 |
| **text** | 0.13–1.36 ms | 非 Top-1 |
| **encode** | 0.01–0.14 ms | CPU 录 pass **极低** → 核显 40% **不是** encode 阻塞 |
| **submit** | 0.39–5.89 ms | 队列提交波动 |
| **present** | 0.22–**4.41 ms** | 高 present + 低 submit 秒 → **VSync 等待 / 帧 pacing**；与 FullClear **fill** 同场景出现 |
| **acquire** | ≈0 ms | **非** swapchain acquire 卡死 |

**fps 行说明**：脚本用 `PostMessage(WM_MOUSEWHEEL)`，每秒仅 ~2 次 profile 汇总，**不能**代替手工 60 fps 滚轮；用户本机手工滚动 ~1% CPU + 核显 40% 与上表 **CPU 帧分量过关** 一致。

**p99 是否在退出门内？** — **是（CPU 记账维度）**；手工 60 fps 无掉帧则 **Wave P 滚动帧率项可签**。

**核显 40% 更像 fill 还是 present 等待？** — **以 FullClear + 栅格 fill 为主**（任务管理器 3D 引擎）；profile 中 **present 可达 ~4 ms** 为 **VSync  pacing 分量**，**acquire≈0** 排除等待换链图像；**encode** 只计 CPU 编码时间，**不含** GPU 填色耗时。

---

## 8. wgpu 显存与 GPU 资源池化（P.2.1 审计 · 2026-08-20）

对应 [AUTHORITY.md](AUTHORITY.md) **WP-P.2.1**。本节记录 ADR-3「Windows 默认 GL 降内存」的**落实核查**，以及纹理 atlas / GPU buffer 池化的源码事实与后续项映射。

### 8.1 ADR-3 落实核查

| 检查项 | 现状 | 结论 |
|--------|------|------|
| Windows 普通窗口后端顺序 | `wgpu_backend.rs`：`GL → (dx12*) → (vulkan*)`；`*` 仅当 `rui/backend-dx12` / `backend-vulkan` feature 开启 | ✅ 已落实 |
| 工作区 wgpu features | 根 `Cargo.toml` 默认 `gles` + `metal`；**不含** `dx12` / `vulkan` | ✅ 避免多后端链接膨胀 |
| `RUI_GPU_BACKEND` 覆盖 | 指定后端提到队首，其余仍作兜底；未编译的后端静默跳过 | ✅ |
| `MemoryHints::MemoryUsage` | `state.rs` / `glass_gpu.rs` device 创建均使用 | ✅ |
| `PowerPreference::LowPower` | 优先集显，降低工作集 | ✅ |
| 透明 / 模糊次窗 | `glass_gpu.rs` 独立 Vulkan（或 `backend-dx12` 时 DX12）；主窗仍 GL | ✅ 有意例外（GL 无 PreMultiplied alpha） |
| macOS | 仅 Metal | ✅ |
| Linux / 其他 | GL 优先，Vulkan 为 opt-in feature | ✅ |

**本机复测（2026-08-20，`gpu_probe` + `PROBE_BACKEND=gl`，Intel RaptorLake-S 集显）**

| 阶段 | Private |
|------|---------|
| 进程启动 | 1.7 MB |
| `request_adapter`（GL） | 29.9 MB |
| `request_device`（GL） | **30.4 MB** |

与 §2 历史数据（GL ~35 MB vs DX12 ~117 MB）同量级：**驱动私有堆仍是主因，ADR-3 选型有效**。

诊断命令：

```powershell
cargo build --release -p gpu_probe
$env:PROBE_BACKEND = "gl"    # 默认已与 rui 对齐
$env:PROBE_FRAMES = "300"      # 默认 300 帧后自动退出
# 或仅测到 device：$env:PROBE_STOP = "device"
.\target\release\gpu_probe.exe
# 日志应含 backend: Gl；PROBE_STOP 时最后一行 probe stop=…
```

### 8.2 纹理与 Atlas

| 资源 | 实现 | 回收 / 上限 | 评价 |
|------|------|-------------|------|
| **用户纹理**（Image / Canvas / Path / Scene3d / Capture / Effect） | `TexturePool`（`resources.rs`） | LRU，`DEFAULT_MAX_ENTRIES = 256`；每帧 `begin_frame` + draw 时 `touch` | ✅ 有界 |
| **字形 atlas** | glyphon `TextAtlas`（`text.rs`） | 由 glyphon/etagere 管理；**每帧 `trim_glyph_atlas()`**（P.2.4 ☑） | ✅ LRU + trim；仍随唯一字形 grow |
| **Emoji 位图** | `EmojiGlyphCache`（`text.rs`） | raster LRU 默认 64、node LRU 默认 32；`RUI_EMOJI_*_CACHE_MAX` 可调 | ✅ 有界 |
| **Shaped 文本** | `shaped_cache: Vec<ShapedTextEntry>` | 随 `text_nodes.len()` truncate；key miss 时 reshape | ✅ 与可见文本节点对齐 |

注意：`prepare([])` 在仍有 text nodes 时会** wipe atlas**（`prepare_text.rs` 注释），因此 cull 路径禁止空 prepare。

### 8.3 GPU Buffer 池化

| Buffer | 位置 | 策略 | 评价 |
|--------|------|------|------|
| 圆角矩形 instance | `state.rs` `instance_buffer` | 容量 grow-only；未满则 `queue.write_buffer` | ✅ 复用 |
| 贴图 quad instance | `upload_textured_instance_buffer` | 同上，初始 cap 256 | ✅ 复用 |
| 下划线 / Input chrome | `state.rs` / `secondary_window.rs` | **每帧** `create_buffer_init`（仅非空时） | ⚠️ 未池化；量小，非 RSS 主因 → 可选 P.3 |
| Pipeline 静态 VB/IB | `RuiPipeline` / `TexturedQuadPipeline` | 创建后常驻 | ✅ |
| Swapchain / retain | `present_retain` | 按 surface 尺寸 ensure | ✅ |

### 8.4 结论与后续映射

- **ADR-3 已落实**：默认构建路径在 Windows 上首选 GL，驱动私有内存与 §2 结论一致；透明窗例外路径有文档且与 `multi_window.md` 一致。
- **纹理池化**：用户侧 `TexturePool` + emoji LRU + glyph atlas 每帧 trim 已到位（§11）；可选 `RUI_TEXTURE_POOL_STALE_FRAMES` 按帧龄 evict。
- **Buffer 池化**：主 draw instance 已复用；underline/chrome 为每帧临时 buffer，属 CPU 提交开销而非显存膨胀，可与 **P.3.5** 批次优化一并考虑。

---

## 9. Element / LayoutTree 常驻开销（P.2.2 审计 · 2026-08-20）

对应 [AUTHORITY.md](AUTHORITY.md) **WP-P.2.2**。本节量化空闲窗口下 **Element 树 + LayoutTree（Taffy）** 的节点规模及其在进程 RSS 中的占比。

### 9.1 诊断 API

源码：`crates/rui/src/core/tree_stats.rs` + `LayoutTree::stats()`（`layout_tree.rs`）。

```powershell
$env:RUI_TREE_STATS = "1"
cargo run -p calculator --release
# 首帧 stderr 一行，含 Private MiB 与节点计数
```

输出示例：

```text
[rui tree stats] private=46.0 MiB elements=74 depth=6 text_bytes=64 attrs=201 handlers=33 signals=0 images=0 | taffy=74 text_measure=33 root=1
```

字段说明：

| 字段 | 含义 |
|------|------|
| `elements` | Element 树 DFS 节点总数 |
| `depth` | 最大嵌套深度 |
| `text_bytes` | 所有 `text` / `text_spans` / shape_suffix 字节和 |
| `attrs` | 全部 `attrs` HashMap 条目数 |
| `handlers` | 带事件闭包 / `reactive_sync` 的节点数 |
| `signals` | 挂载 `switch_checked_sync` 等组件信号槽的节点数 |
| `taffy` | LayoutTree 中 Taffy 节点数（通常与 `elements` 1:1） |
| `text_measure` | 文本叶 measure 元数据条目（≤ `taffy`） |

### 9.2 本机实测（2026-08-20 · Win · GL · release）

| 场景 | Private | elements | taffy | text_measure | depth | handlers |
|------|---------|----------|-------|--------------|-------|----------|
| `gpu_probe`（仅 wgpu 地板，§8） | **30.1 MiB** | — | — | — | — | — |
| `calculator` 空闲 | **46.0 MiB** | 74 | 74 | 33 | 6 | 33 |
| `gallery` 空闲（默认「基础」分区） | **47.2 MiB** | 122 | 122 | 60 | 10 | 36 |

**RSS 分解（粗算）**

| 层级 | Calculator | Gallery Δ |
|------|------------|-----------|
| GPU 驱动地板（`gpu_probe`） | ~30 MiB | 同左 |
| 框架 + 字体 + 文本栈 + 应用逻辑 | ~16 MiB | +~1 MiB |
| Element/Taffy 树（+48 节点） | （含于上） | 边际极小 |

Gallery 比 Calculator 多 **48 个 Element 节点**，Private 仅增 **~1.2 MiB**；**树结构不是 RSS 主因**（与 §2「框架 ~10 MB」结论一致，此处 GL 地板已更新为 ~30 MiB）。

### 9.3 源码事实

| 项 | 现状 | 评价 |
|----|------|------|
| Element ↔ Taffy 映射 | 每 Element 一个 `layout_node`；`total_nodes` 与 Element 计数同步 | ✅ 无「幽灵 Taffy 节点」泄漏 |
| `text_measurements` | 文本叶 layout 元数据；随 rebuild 修剪 | ✅ 规模 ≤ 文本叶数 |
| `measure_layout_cache` | 单次 `compute_layout` 内 memo，帧末清空 | ✅ 不跨帧膨胀 |
| Element 单节点体积 | `size_of::<Element>()` ≈ 数百字节 + 堆上 `text`/`attrs`/闭包 | ⚠️ 闭包节点（handlers）重于纯 div |
| 结构虚拟化 | VirtualList / VirtualMasonry（experimental）减少 **mounted** 节点 | ✅ 大数据场景已走路径 |
| Portal hoist | Dialog/Toast 子树提升到 portal 层，不复制 Taffy | ✅ 不双倍计数 |

### 9.4 结论

- **空闲窗口 Element + LayoutTree 开销可忽略**：74–122 节点量级，相对 ~46 MiB Private 占比远低于 GPU（~30 MiB）与字体/文本栈（~10–16 MiB）。
- **无需为 P.2 专门做树瘦身**；内存收敛主战场仍是 P.2.1（后端）、P.2.4（glyph atlas）；信号注册表见 §10（P.2.3 ☑）。
- 长列表/大表单场景继续依赖 **VirtualList / Masonry** 控制 mounted 节点，而非全量 DOM 式常驻。

---

## 10. 信号 / Store 注册表（P.2.3 审计 · 2026-08-20）

对应 [AUTHORITY.md](AUTHORITY.md) **WP-P.2.3**；机制基线 **WP-2.4 ☑**（Owner dispose → `REGISTERED_*` 清表）。本节确认**长会话不泄漏动态注册表**，并给出 Gallery 空闲快照。

### 10.1 诊断 API

源码：`crates/rui/src/core/registry_stats.rs`；底层计数来自 `app/reactive.rs` + `reactivity_debug_snapshot()`。

```powershell
$env:RUI_REGISTRY_STATS = "1"
cargo run -p calculator --release
# 首帧 stderr 一行
```

输出示例（Calculator 空闲）：

```text
[rui registry stats] private=44.9 MiB anchors=2 bindings=2 props=0 children=0 keyed=0 | owners=42 effects=5 memos=0 signals=220 named=130 pending_e=0 pending_m=0 root_deps=0 e_edges=4 m_edges=0
```

| 字段 | 含义 |
|------|------|
| `anchors` | `REGISTERED_ANCHORS`（`reactive_anchor` / `reactive_view`） |
| `bindings` | `REGISTERED_BINDINGS`（`bind_text` / `bind_*`） |
| `props` / `children` | `COMPONENT_PROPS` / `COMPONENT_CHILDREN` stash |
| `keyed` | `for_keyed_reactive` 项级 Owner 数 |
| `owners` / `effects` / `signals` | 运行时图 live 计数 |
| `named` | `use_signal("key", …)` 全局命名槽（**有意跨 rebuild 保留**） |
| `root_deps` | 根 `app_builder` effect 裸订阅数；**应为 0** |

### 10.2 库测（必须）

```powershell
cargo test -p rui registry
```

| 测试 | 断言 |
|------|------|
| `registry_owner::*` | Owner dispose 后 Anchors/Bindings/COMPONENT_* 归零 |
| `registry_session::switch_reactive_section_cycles_*` | Gallery 式 `switch_reactive` 切换 40 轮，注册表计数不增长 |
| `registry_session::named_signals_plateau_*` | 重复 `use_signal` 同 key 不重复注册 |

**2026-08-20：8 passed**（含 image/window/cursor 等含 registry 字样的无关测）。

### 10.3 Gallery 空闲实测（release · GL）

| 指标 | Calculator | Gallery（默认「基础」） |
|------|------------|-------------------------|
| `anchors` | 2 | 2 |
| `bindings` | 2 | 12 |
| `props` / `keyed` | 0 | 0 |
| `named` | 130 | 160 |
| `owners` | 42 | 70 |
| `effects` | 5 | 15 |
| `root_deps` | **0** | **0** |
| Private | ~45 MiB | ~46 MiB |

解读：

- **`anchors` 不随 Gallery 组件数量膨胀**：侧栏 + `switch_reactive` 仅 2 个稳定岛；分区切换走同一 anchor id（`examples/gallery/App:section_body`）。
- **`named` 较高属预期**：`use_signal("组件id__…")` 全局命名槽刻意保留 Input 焦点 / 滚动状态；**首次访问后 plateau**，不会随切换分区线性增长（库测 `named_signals_plateau`）。
- **`root_deps=0`**：根 effect 未裸订阅业务 signal，符合 §3 红线。
- **Store**：无独立全局 Store 注册表；字段路径走 Owner 下 Signal，`Store::field` 不扩大根订阅面（WP-R.3 ☑）。

### 10.4 Gallery 长会话手工（建议 · WP-2.4 卡片）

`cargo run -p gallery` → 侧栏「布局」→ **WP-2.4) Owner 注册表 + scoped ID**：

1. 卸载岛 → 刷新 → 记卸载基线  
2. 挂载/卸载循环 ≥10 次  
3. 卸载态 `a/b/p` 均 ≤ 基线  

与 §10.2 库测互补：卡片含 Gallery 全局其它岛的基线偏移，**判「≤ 基线」而非归零**。

### 10.5 结论

- **动态注册表（Anchors/Bindings/COMPONENT_*）**：Owner dispose + unmount 清表，40 轮 mount/dispose 与 Gallery 式分区切换**无泄漏**（WP-2.4 机制有效）。
- **`named` 全局槽**：数量与 app 中 distinct `use_signal` key 成正比，** idle 后稳定**；不应期望进程级归零。
- **RSS**：注册表不是 Private 主因（§9）；长会话监控用 `RUI_REGISTRY_STATS=1` + `cargo test -p rui registry` 即可。

---

## 11. 纹理 / Glyph 缓存回收（P.2.4 审计 · 2026-08-20）

对应 [AUTHORITY.md](AUTHORITY.md) **WP-P.2.4**。本节审计用户纹理池、glyphon `TextAtlas`、Emoji 位图与文本 shadow 纹理在长会话中是否只增不减，并记录已落地的回收策略。

### 11.1 诊断 API

源码：`crates/rui/src/core/texture_stats.rs`；纹理池 `crates/rui/src/renderer/resources.rs`；字形 `crates/rui/src/renderer/text.rs`。

```powershell
$env:RUI_TEXTURE_STATS = "1"
cargo run -p gallery --release
# 首帧 stderr 一行（与 §9–§10 相同，仅输出一次）
```

输出示例：

```text
[rui texture stats] private=47.0 MiB pool=3/256 frame=42 stale120=0 shadow=0 | page[shaped=18 emoji_n=0 emoji_r=0 emoji_id=0] overlay[shaped=0 emoji_n=0 emoji_r=0 emoji_id=0] obands=0
```

| 字段 | 含义 |
|------|------|
| `pool` / `256` | `TexturePool` 当前条目 / 硬上限 |
| `frame` | 池内帧计数（每帧 `begin_frame` +1） |
| `stale120` | 超过 120 帧未 `touch` 的池条目数 |
| `shadow` | 文本 drop-shadow GPU 纹理（`shadow_tex_cache`） |
| `page[shaped=…]` | 主层 `shaped_cache` + Emoji LRU 计数 |
| `overlay[…]` / `obands` | Portal overlay 独立 atlas 与 band 数 |

可选：按帧龄驱逐未 touch 的用户纹理（默认关闭，避免误删仍可能被引用的离屏纹理）：

```powershell
$env:RUI_TEXTURE_POOL_STALE_FRAMES = "600"   # 约 10s@60fps 未绘制则 evict
```

### 11.2 源码审计

| 资源 | 实现 | 回收 / 上限 | P.2.4 结论 |
|------|------|-------------|------------|
| **用户纹理** `TexturePool` | `resources.rs` | 硬 cap **256**；insert 时 LRU；draw 时 `touch` 更新 `last_frame` | ✅ 有界；**非「不可见即删」**——未 touch 条目可长期驻留直至 cap 或 `RUI_TEXTURE_POOL_STALE_FRAMES` |
| **glyphon `TextAtlas`** | 每 `RuiTextRenderer` 一对 mask/color atlas | glyphon 内部 **LRU packer** + 可 **2× grow** 至 `max_texture_dimension_2d`；`glyph_cache` 无硬顶 | ⚠️ 随**历史唯一字形**增长；**2026-08-20 起每帧 `trim_glyph_atlas()`**（对齐 glyphon 官方示例），允许下帧 evict 未引用 slot |
| **`SwashCache`** | cosmic-text CPU 栅格 | `image_cache` / `outline_command_cache` 无 trim API；rui 主路径用 `get_image_uncached` | ✅ prepare 路径不持久写入 cache；风险低于 atlas |
| **`shaped_cache`** | `Vec<ShapedTextEntry>` | 随 `text_nodes.len()` **truncate** | ✅ 与可见文本节点对齐 |
| **Emoji** `EmojiGlyphCache` | raster LRU **64** / node LRU **32**（env 可调） | `id_rasters` **永不清**（防 glyphon Some→None panic） | ✅ 有界；`emoji_id` 可缓慢增 |
| **文本 shadow** `shadow_tex_cache` | `HashMap<u64, GpuImageTexture>` | 仅 insert，**无 evict** | ⚠️ 随唯一 shadow 样式增；通常 << atlas |
| **多 atlas** | 主窗 + portal overlay 各 band 独立 `RuiTextRenderer` | 各 atlas 独立 grow | ⚠️ 多 Dialog/Toast 叠层时乘性占用；仍受 per-atlas LRU |

**P.2.4 关键发现（已修复）**：glyphon 要求在每帧 prepare+render 后调用 `TextAtlas::trim()` 清空 `glyphs_in_use`，否则 LRU 无法回收本帧未绘制的字形 slot，长会话 atlas **只增不减**。此前 rui **未调用**；现于 `state.rs` / `secondary_window.rs` 在全部 text prepare 完成后统一 `trim_all_glyph_atlases()`。

**不可见纹理**：collect 阶段 clip-cull 跳过离屏 text node 的 prepare，但已上传的 `TexturePool` 条目不会因「本帧未 draw」自动删除——依赖 cap/LRU 或可选 `RUI_TEXTURE_POOL_STALE_FRAMES`。与浏览器「离屏 Image 仍占 GPU 内存」同类，属预期 trade-off。

### 11.3 本机实测（2026-08-20 · Win · GL · release）

| 场景 | Private | pool | shaped (page) | shadow | 备注 |
|------|---------|------|---------------|--------|------|
| `gallery` 空闲（「基础」） | ~47 MiB | 0–3 | ~18 | 0 | 无 heavy 图片分区 |
| `gallery` 滚动 / 切换分区 | ~47 MiB | ≤ cap | ≤ 可见 text 节点 | 0 | Private **plateau**；pool 不随切换线性增 |
| `calculator` 空闲 | ~46 MiB | 0 | ~12 | 0 | 与 §9 同量级 |

长会话判据：**切换 Gallery 全部分区 + ScrollArea 滚动 2 分钟**后 `RUI_TEXTURE_STATS=1` 复测，`pool` 与 `shaped` 不持续爬升；`private` 波动 ±1 MiB（驱动噪声）。glyph atlas **GPU 侧**无公开计数 API，以 plateau + `trim` 机制为准。

### 11.4 结论与后续映射

- **用户纹理**：`TexturePool` 256 上限 + touch LRU 已足够；可选 `RUI_TEXTURE_POOL_STALE_FRAMES` 供极端 Gallery（大量 Canvas/Path）长会话。
- **Glyph atlas**：**必须**每帧 `trim()`（已落地）；唯一字形集仍可使 atlas **grow**，属 glyphon 设计——极端多语言/多字号场景监控 Private，后续可考虑 P.3 的字形集预热或 atlas 重建策略。
- **`shadow_tex_cache`**：当前无回收；若 shadow 样式组合爆炸再补 LRU（低优先级）。
- **主战场**：运行内存仍由 §8 GPU 地板（~30 MiB GL）主导；P.2.4 收敛的是「缓存无界增长」类缺陷，而非树/注册表（§9–§10 ☑）。

---

## Release checklist（体积 / 性能）

发布或打 tag 前建议：

1. **体积**：`cargo build -p calculator --release`，记录 exe 大小；对照本节历史表。
2. **内存**：可选跑 `examples/gpu_probe` + 计算器，确认 GL 默认路径 Private Bytes 未回退；`RUI_TREE_STATS=1` / `RUI_REGISTRY_STATS=1` / `RUI_TEXTURE_STATS=1` 核对树、注册表与纹理缓存（§9–§11）。
3. **布局基线**：`cargo bench -p rui --bench layout_baseline`（见 `crates/rui/benches/layout_baseline.rs`）；可选 `--features tracing`。
4. **滚动 / CPU 基线**（Wave P）：见 §7.5–§7.7；AUTHORITY WP-P.0.3。
5. **测试**：`cargo test -p rui --lib` + CI `offscreen` job（`RUI_FORCE_SOFTWARE=1`）。
6. **Features**：默认精简；文档列出 `bundled-*` / `emoji-svg` / `backend-dx12` / `system-tray` 对体积的影响（见 [PUBLISH.md](PUBLISH.md)）。
