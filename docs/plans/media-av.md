# 音视频实施计划（WP-M）

> **文档地位**：[`media.md`](../engine/media.md) 的**执行清单**（勾选、分期、门禁）。  
> **约束**：不挡 [`AUTHORITY.md`](../AUTHORITY.md) W3/W4 / SemVer 0.1 / Beta。巩固期 **不** 新开挡发版 WP。Backlog 编号 **B18**。  
> **关联**：[`color_advanced.md`](../engine/color_advanced.md)（P3-C 色标签、HDR-B 媒体叶）· [`drawing.md`](../drawing.md) · [`remote_asset.md`](../ops/remote_asset.md) · [`crate_deps.md`](../ops/crate_deps.md)  
> **日期**：2026-09-07  
> **状态**：契约已写（WP-M.0）；**WP-M.1 / M.2 / M.3 / M.6 已落地**（2026-09-07）。M.3：`image-stream` + BT.709 YUV→RGBA（CPU 核 + WGSL 对照）；Gallery RGBA 默认、feature 开时 I420。HDR / 播放器 crate 未开始。

---

## 0. 目标与非目标

### 目标

| 阶段 | 时机 | 交付 |
|------|------|------|
| **M.0** | 现在 | 分层契约进 `engine/media.md`；AUTHORITY / 颜色 / 自绘交叉引用 |
| **M.1–M.2** | 0.1 之后或作为 GIF/Canvas 性能修补并行 | 同键原地上传；`queue_media_frame` RGBA MVP |
| **M.3–M.6** | 颜色 P3-C / HDR-B 落地窗口 | YUV opt-in、HDR 叶、Gallery 合成流 |
| **M.7** | 可选 | 仓内 **example** 链解码器（核心 `Cargo.toml` 零解码依赖） |
| **M.8–M.9** | 明确延后 | 播放器 crate、摄像头 |

### 非目标（全阶段）

- `rui` 依赖 FFmpeg / GStreamer / cpal / 摄像头 SDK
- HWND / 系统播放器浮层
- 0.1 承诺面出现 `<Video>` / `<Audio>`
- 现在创建空的 `rui_video` / `rui_audio`
- Shadertoy audio/video channel
- 完整 HTML5 媒体元素语义

### 用户故事

1. **应用解码器**：工人线程每帧 `queue_media_frame("cam", nv12)`，UI 上 `<Image texture="cam">` 圆角裁剪、滚动、HDR 纸白合成都正确。
2. **无解码器验收**：Gallery 用 CPU 生成的 I420 色条证明叶与 feature，不在 CI 装 FFmpeg。
3. **产品播放器（后置）**：`rui_video` 提供默认皮 + 插件槽，内部只依赖层 1；解码由应用注入。

---

## 1. 现状基线（2026-09-07）

| 已有 | 缺口 |
|------|------|
| `queue_image_rgba8` 任意线程排队，同键去重 | 每帧可能重建纹理；无色标签 |
| GIF 切帧 + `DYNAMIC_TEX_EPOCH` | 无通用「流」API |
| `<Image texture>` 叶：圆角 / clip / 命中 | 无 YUV、无 HDR 叶标记 |
| `remote-asset` 注入 HTTP | 不负责解封装 |
| Canvas `put_image_data` / `request_frame` | 带宽差，不适合 1080p60 |
| Scene3d 8-bit bake | HDR-B 尚未改 16f（颜色方案） |

---

## 2. 依赖图

```text
WP-M.0  契约（本文 + media.md）                    ☑ 文档
  │
WP-M.1  同键同尺寸 write_texture 原地更新          无颜色依赖
  │
WP-M.2  queue_media_frame（RGBA + DisplaySrgb）    依赖 M.1
  │
  ├─ WP-M.3  image-stream：YUV GPU                 依赖 M.2；P3-C 矩阵可后接
  │
  ├─ WP-M.5  MediaClock 协议（可只文档+trait）      依赖 M.2 公开槽
  │
  └─ WP-M.6  Gallery 合成流（RGBA 先，YUV 随 M.3）
           │
WP-M.4  HDR 媒体叶                                 依赖 HDR-B + M.2
           │
WP-M.7  example 解码器                             依赖 M.2/M.3；永不进 lib
           │
WP-M.8  rui_video / rui_audio                      依赖 M.2 + 组件扩展点成熟
WP-M.9  摄像头                                     依赖 M.8 或仅 M.2 + 应用
```

与 [`color_advanced.md`](../engine/color_advanced.md) 的并行规则：

| 颜色阶段 | 媒体 |
|----------|------|
| P3-A / HDR-A | **零媒体工作**；不要挡 |
| P3-C | 色标签字段与 M.2/M.3 **共用**；谁先落地谁引入 `TextureColorSpace`，另一侧接上 |
| HDR-B | Scene3d 先成为媒体叶；M.4 把同一 `media_mode` 接到 ImageStream |
| HDR-C | 与推帧无关 |

---

## 3. WP-M.0 — 契约与导航

**状态**：☑ 2026-09-07（文档；无代码）

- [x] [`engine/media.md`](../engine/media.md) 三层模型、禁区、冻结句式
- [x] 本计划 WP-M 分期
- [x] AUTHORITY 争议路由 + Backlog B18
- [x] `color_advanced` / `drawing` / inventory / CHANGELOG 交叉引用

**门禁**：合并文档即可关闭本项。禁止借本项改 `Cargo.toml` 或加 crate。

---

## 4. WP-M.1 — 同键原地上传

**状态**：☑ 2026-09-07

- [x] `image_registry` / `TexturePool::try_overwrite_rgba8`：同尺寸 `write_texture`
- [x] `state.rs` drain：能 overwrite 则不 `create_texture`
- [x] 库测：`create_count` 同尺寸不变、改尺寸 +1

### 4.1 改哪里

| 模块 | 工作 |
|------|------|
| `app/image_registry.rs` | 记录 `(key, width, height, format)`；同尺寸更新 texel，不同尺寸重建 |
| `app/pending_images.rs` | 保持队列深度 1；epoch 仍 bump |
| 库测 | 同键两帧、改尺寸一帧；不要求像素 golden 变 |

### 4.2 验收

- 连续 `queue_image_rgba8("k", w, h, …)` 不在同尺寸下分配新 `wgpu::Texture`（测：`TexturePool::create_count`）。
- 默认 Gallery / `color_freeze` / 现有 image 测不变。
- 与 M.2 同 PR 导出 `queue_media_frame`（本项本身不增 Cargo feature）。

### 4.3 风险

上传仍在 UI 线程 drain——1080p60 RGBA 会顶内存带宽。本项只消灭「重建纹理」成本；带宽问题交给 M.3（YUV）或应用降分辨率。

---

## 5. WP-M.2 — 公开流槽（RGBA MVP）

**状态**：☑ 2026-09-07

**目的**：层 1 的最小公开面。应用可推 RGBA 流；仍无解码器。

### 5.1 API（设计名，落地时以 rustdoc 为准）

```rust
pub fn queue_media_frame(key: impl Into<String>, frame: MediaFrame<'_>);
// MediaFrame MVP：仅 PixelLayout::Rgba8Unorm + TextureColorSpace::DisplaySrgb
```

- `<Image texture="key">` 即为容器；**本阶段不强制**新 RSX 标签。
- Prelude：可只导出 `queue_media_frame`；`MediaFrame` 标 experimental。
- `queue_image_rgba8` 保留为薄封装。

### 5.2 测试

| 用例 | 断言 |
|------|------|
| 30 次同尺寸投递 | 原地更新；最后一帧可见（offscreen 采样或 epoch） |
| 损坏尺寸 / 缓冲过短 | 丢帧、保留上一 GPU 内容、不 panic |
| 非 UI 线程投递 | 与今日 `queue_image_rgba8` 相同安全 |

### 5.3 验收

- rustdoc 写明「不解码、不播放、无控件」。
- 无 `video` feature、无新依赖。

---

## 6. WP-M.3 — `image-stream`：YUV GPU 转换

**状态**：☑ 2026-09-07（BT.709 有限范围；CPU 核走既有 RGBA 原地写；WGSL 对照测）

**目的**：NV12 / I420 / P010 → working RGB。默认 feature **关**。

### 6.1 交付

- [x] `Cargo.toml`：`image-stream = []`；**零**系统解码库。
- [x] `PixelLayout::{Nv12, I420, P010}` + `MediaPlanes::Yuv`。
- [x] BT.709 SDR 矩阵（[`yuv.rs`](../../crates/rui/src/media/yuv.rs)）；同系数 WGSL [`yuv_convert.wgsl`](../../crates/rui/src/renderer/yuv_convert.wgsl)。
- [x] 无 feature 时对这些 layout 丢帧、保留上帧、不 panic。

GPU **直写** Image dest（`RENDER_ATTACHMENT`）未接 drain——公开 API 不变，可后补。P010 先扩成 8-bit 再套同一矩阵。

### 6.2 验收

- [x] 库测：I420/NV12 灰条、P010 白、短缓冲丢弃；feature 关时 YUV 不入队。
- [x] CI **不** 安装 FFmpeg；`cargo test --features image-stream yuv`。
- [x] `crates/rui` 的 `[dependencies]` 不出现 `ffmpeg-next` / `gstreamer`。

### 6.3 明确不做（本 WP）

HDR PQ 帧（→ M.4）；色彩管理完整 ICC。

---

## 7. WP-M.4 — HDR 媒体叶

**依赖**：[`color_advanced.md`](../engine/color_advanced.md) **HDR-B**（`ToneMapParams`、Scene3d 16f）已落地或至少内部 API 可用。

### 7.1 交付

- `TextureColorSpace::{WorkingLinear, PqSt2084, Hlg}` 接到同一 media tone map。
- ImageStream 叶与 Scene3d **禁止**各自 bake 8-bit 再叠。
- SDR 窗口：HDR 帧映射进纸白（`ClampToPeak` 或 BT.2390），UI chrome 不变亮。

### 7.2 验收

- 合成测：Button（Y≤1）+ 人造 HDR 帧（Y>1）同帧；chrome 不被洗白。
- 无 `hdr-surface` 时回退 SDR 映射，与 HDR-C 失败策略一致。
- 公开 prelude 仍可不导出 `ToneMapParams`（颜色方案 §7）。

**排序**：若 HDR-B 尚未开始，本 WP **排队**，不提前分叉一套视频专用 tone map。

---

## 8. WP-M.5 — `MediaClock` + 帧钩子样板

**目的**：层 3 到来前，先有「播放/暂停/seek 是谁的责任」的协议；避免将来在 `window_events.rs` 加 `tick_video_drag`。

### 8.1 交付

- `MediaClock` trait（[`media.md`](../engine/media.md) §4.8）；可放 `rui::media` 或先 `pub(crate)`。
- 一个 **文档 + 单测假时钟**，证明不调度音频设备。
- 若 [`runtime_extension_points.md`](../engine/runtime_extension_points.md) 的 `FrameHook` 已存在：示例 hook 只 `request_redraw`；**禁止**新硬编码 tick 名单。

### 8.2 验收

- `rui` 仍无 cpal。
- 注释写清：同步实现属于层 2。

可与 M.2 之后并行；不挡 M.3。

---

## 9. WP-M.6 — Gallery 合成流（无解码器）

**状态**：☑ 2026-09-07（RGBA 色条；`image-stream` 时 I420）

**目的**：人眼可验收「这是一条会动的图」，CI 可复现。

### 9.1 交付

- Gallery「绘制」或独立实验页：CPU 生成的移动色条 / 棋盘，RGBA 路径必有；`image-stream` 开时再加 I420。
- 页脚文案：解码器由应用提供，本页无 FFmpeg。
- 可选库测 offscreen 抓 2 帧 epoch 变化。

### 9.2 验收

- 默认 feature 构建可跑 RGBA 演示。
- 不把该页标成 SemVer 控件；inventory 进 §1.10 experimental / planned。

---

## 10. WP-M.7 — 可选 example 解码器

**目的**：证明层 2 注入，而不是把解码器产品化进库。

### 10.1 形态

```text
examples/ffmpeg_video_player/     # 或 examples/media_stream_ffmpeg/
  Cargo.toml                      # 自己声明 ffmpeg 依赖
  src/main.rs                     # decode → queue_media_frame
```

- **禁止** 在 `crates/rui/Cargo.toml` 写 `ffmpeg-next`（即便 optional）。
- CI 默认 **skip**（无系统库）；文档说明如何本地跑。
- LICENSE 提示：该 example 的 GPL/LGPL 边界与 MIT 的 `rui` 分离。

### 10.2 验收

- README：核心零解码依赖；example 可删不影响 `cargo test -p rui`。
- 与 EUI `ffmpeg_video_player` 同构：示例，非核。

无 FFmpeg 的贡献者 **跳过** 本 WP 不算缺口。

---

## 11. WP-M.8 — 产品 crate（ArtPlayer 层）

**时机**：0.1 发布之后；且 M.2 稳定、组件 `FrameHook` 可自注册。

### 11.1 交付（到点再开 PR，不提前建空仓）

| Crate | 内容 |
|-------|------|
| `rui_video`（或 `jadix_video`） | 无头播放状态机 + 默认 shadcn 皮（播放/暂停、进度 Slider、音量、时间、全屏窗口 API 若已有）+ overlay 槽 |
| `rui_audio` | 仅控件 + 可选 Canvas 可视化；**不**实现 mixer |

插件：字幕 / 弹幕 = 普通 Element children；解码器 = 应用实现的 `MediaClock` + 推帧。

### 11.2 验收

- 依赖 `rui`，**不**反向把播放器 `use` 进 `crates/rui`。
- 默认皮可完全拆掉（headless）。
- 无解码器仍能用假时钟 + Gallery 色条跑通 chrome。
- 不进 `rui` 默认 prelude。

### 11.3 明确不做（本 WP）

HLS 客户端、弹幕协议、广告 SDK、Cast。那些是插件或应用。

---

## 12. WP-M.9 — 摄像头

Freya F03 同档。预览 = ImageStream；捕获 SDK 在应用或未来 `rui_camera`。**不** 进本计划的 0.1 窗口。开做前另写短计划，不要扩大本文禁区例外。

---

## 13. 验收总表

| WP | 用户可见 | 门禁 | 挡 0.1？ |
|----|----------|------|----------|
| M.0 | 文档 | 交叉引用齐全 | 否 |
| M.1 | 无（性能） | 同尺寸不重建纹理；golden 不变 | 否 · ☑ |
| M.2 | 应用可推 RGBA | 库测；无新依赖 | 否 · ☑ |
| M.3 | YUV feature | 色条测；CI 无 FFmpeg | 否 · ☑ |
| M.4 | HDR 帧不洗白 UI | 依赖 HDR-B | 否 |
| M.5 | 无 | trait + 无 cpal | 否 |
| M.6 | Gallery 色条 | 默认构建可跑 | 否 · ☑ RGBA / I420 feature |
| M.7 | 可选 example | 核心 Cargo 干净 | 否 |
| M.8 | 独立播放器 crate | headless + 默认皮；不进 prelude | 否 |
| M.9 | — | 另案 | 否 |

---

## 14. 风险与回退

| 风险 | 回退 |
|------|------|
| 过早公开 `<Video>` 标签 | 只保证 `queue_media_frame` + `<Image texture>` |
| YUV 矩阵与 P3-C 分叉 | 单一 `TextureColorSpace`；禁止视频专用第二套 uniform |
| 有人把 FFmpeg 加进 `rui` 「optional」 | 本计划 §10 + [`crate_deps.md`](../ops/crate_deps.md) 拒绝；example 隔离 |
| HDR-B 延期 | M.4 排队；M.1–M.3 仍可在 SDR 完成 |
| 播放器范围膨胀成「第二个 UI 框架」 | chrome 只用现有组件；禁止新渲染器 |
| 音频 crate 对称强迫症 | [`media.md`](../engine/media.md) §6.4；音频不碰 DisplayTransform |

---

## 15. 建议实施顺序（不排进巩固期周表）

1. 推进 [`color_advanced.md`](../engine/color_advanced.md) **P3-A / HDR-A**（与 WP-M 无关）。
2. 需要 GIF/Canvas 少分配时做 **M.1**（可当普通性能 PR）。
3. P3-C 引入纹理色空间时，按 `media.md` 字段命名，避免日后重命名。
4. HDR-B 以 Scene3d 为第一媒体叶；**M.4** 接同一参数。
5. 有真实应用要推摄像头/影片时再做 **M.2/M.3/M.6**。
6. **M.8** 只在至少一例真实解码注入 + chrome 需求后开 crate。

勾选只改 **本文**；领域争议改 [`media.md`](../engine/media.md)。不在 AUTHORITY 周表里用本 WP 充巩固期进度。
