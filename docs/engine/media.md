# RUI 音视频：媒体叶、流容器与播放器边界

> **文档地位**：动态纹理 / 视频帧 / 音频时钟的**设计方案**（「是什么 / 禁区 / 分层」）。执行勾选见 [`plans/media-av.md`](../plans/media-av.md)（**WP-M**）。HDR 合成与纸白见 [`color_advanced.md`](color_advanced.md)；公开自绘叶见 [`drawing.md`](../drawing.md)；远程字节入口见 [`remote_asset.md`](../ops/remote_asset.md)。  
> **冻结状态**：**未冻结**（2026-09-07）。层 1 RGBA 槽 **WP-M.1 / M.2 已落地**；**WP-M.3 YUV**（`image-stream`，BT.709 有限范围 → RGBA）已落地。HDR 媒体叶 / 播放器 crate **未实现**。不挡 SemVer 0.1 / Beta。  
> **实现锚点（现状，供对照）**：[`pending_images.rs`](../../crates/rui/src/app/pending_images.rs)（`queue_image_rgba8` / GIF 帧 / `DYNAMIC_TEX_EPOCH`）· [`stream.rs`](../../crates/rui/src/media/stream.rs) · [`yuv.rs`](../../crates/rui/src/media/yuv.rs) · [`yuv_convert.wgsl`](../../crates/rui/src/renderer/yuv_convert.wgsl) · [`image.rs`](../../crates/rui/src/components/image.rs) · [`image_registry.rs`](../../crates/rui/src/app/image_registry.rs) · Canvas `request_frame`（[`drawing.md`](../drawing.md) §4）。

---

## 0. 一句话结论

`rui` **不是**多媒体引擎，也 **不是** ArtPlayer。引擎只提供与 `<Image>` 同级的 **带色标签动态纹理槽**（工作名 **ImageStream / MediaSurface**）；解码器永不进核心；带默认皮与插件的播放器是 **后置独立 crate**。

音频 **没有**对等的视觉容器：它不参与 `DisplayTransform`。波形 / 频谱是 Canvas 自己画。

```text
3. 播放器产品（后置 crate · 不进 0.1）
   控件皮、插件、默认 shadcn 样式
   依赖层 1；可选依赖层 2
        ▲
2. 解码 / 设备 / A-V 同步实现（永远不进 rui）
   demux、H.264/AV1、HLS、摄像头、cpal、DRM
        ▲
1. 引擎槽（rui · opt-in）
   动态纹理、YUV→采样、色标签、request_frame、
   布局/圆角/clip/命中/damage、a11y、HDR 媒体叶
```

| 选项 | 判定 |
|------|------|
| 框架提供完整音视频能力 | **否**（编解码、协议、设备、许可证） |
| 只提供 audio/video 流容器 | **对，且仅视频有视觉槽**；音频无像素容器 |
| 完全交给外部 crate（HWND / 系统播放器浮层） | **否**（拆掉 wgpu 合成、clip、圆角、HDR 纸白） |
| 像 ArtPlayer：两个可插件化独立 crate | **对，但是层 3 产品**；依赖层 1，不代替层 1 |

对照：EUI `ImageStream`（核只收帧，FFmpeg 在示例）· Freya `freya-video`（后置 crate）· HTML `<video>`（浏览器解码）+ ArtPlayer（包控件）。RUI 跟 **EUI 的核 + Freya 的后置 crate**。

---

## 1. 文档边界

| 文档 | 管什么 |
|------|--------|
| [`color.md`](color.md) | 已落地 LDR sRGB 色数学 |
| [`color_advanced.md`](color_advanced.md) | P3 / HDR / `DisplayTransform` / `ToneMapParams`；**媒体叶是合成类别** |
| [`drawing.md`](../drawing.md) | Svg / Canvas / Scene3d **叶**；Canvas 可 `put_image_data` |
| [`remote_asset.md`](../ops/remote_asset.md) | `http(s)` 字节入口；传输由应用注入 |
| [`runtime_extension_points.md`](runtime_extension_points.md) | 组件自注册 / `FrameHook`；**不是**解码器 plugin |
| [`assets_and_icons_architecture.md`](assets_and_icons_architecture.md) | 静态资源拆 crate；不含编解码器 |
| **本文** | 动态帧槽、YUV、色标签接缝、音频时钟协议、播放器 crate 边界 |
| [`plans/media-av.md`](../plans/media-av.md) | WP-M 分期、门禁、依赖 [`color_advanced.md`](color_advanced.md) 阶段 |

**三句勿混**：

| 名 | 是 | 不是 |
|----|----|------|
| `image-hdr` feature | HDR **文件**解码（`image` crate） | 显示 HDR 表面 |
| ImageStream / `queue_media_frame` | YUV / 动态帧 **上传** | 播放器、FFmpeg |
| `hdr-surface`（颜色方案） | 交换链 16f / PQ | 视频控件 |

---

## 2. 今日实现基线（代码事实）

| 层 | 现状 |
|----|------|
| 位图 | `<Image src>` / `texture` → `queue_image_from_src` / `queue_image_rgba8`；8-bit RGBA；无色标签（P3-C 未做） |
| 动画图 | GIF 全帧解码后按延迟切 `queue_image_rgba8` 同一键；`DYNAMIC_TEX_EPOCH` 打破 present 指纹 |
| **动态槽** | [`queue_media_frame`](../../crates/rui/src/media/stream.rs)：RGBA8 默认；`image-stream` 时 NV12/I420/P010 → BT.709 有限范围 RGBA，再走同键原地 `write_texture`（WP-M.1） |
| 动态叶 | Canvas / Scene3d 也走贴图键 + epoch；同键同尺寸不再重建 GPU 对象 |
| 远程 | `remote-asset`：widget 接受 URL；HTTP 由 `RemoteAssetFetcher` 注入；默认关 |
| 编解码 | `image`（png/jpeg/webp/gif/ico）+ 可选 `svg`；**无** FFmpeg / GStreamer / cpal / 摄像头 |
| 公开控件 | 无 `<video>` / `<audio>` / GifViewer；Freya F02/F35 均为后置 |
| a11y | `role="image"` → `Role::Image`；无 Video / Audio 角色映射 |
| HDR | 未实现；[`color_advanced.md`](color_advanced.md) §5.4 把「视频」写成媒体叶，**没有**播放器 API |

**结论**：RGBA 动态槽已通。`image-stream` 时 YUV 三 layout 可投递（CPU BT.709 核 + WGSL 对照；上传仍是 RGBA 原地写）。仍无色空间标签（P3-C）、无时钟协议、无解码器。视频仍是这条路的高频形态，不是第三套渲染器。

---

## 3. 设计原则

1. **YUV/动态帧是 Image 源的一种，不是第三套渲染器**（EUI §3.6；[`drawing.md`](../drawing.md) §9）。
2. **核不链解码器**。FFmpeg / GStreamer / Symphonia / cpal / 摄像头 SDK 只出现在应用、示例或后置 crate。
3. **禁止系统原生视频 HWND / AVPlayer 浮层** 叠在 wgpu 上绕开合成（破坏 clip、圆角、damage、HDR 纸白）。
4. **音频不对称**：无 `<audio>` 视觉叶；不进 `DisplayTransform` / `ToneMapParams`。
5. **不做 runtime 解码 plugin**。颜色管线与交换链禁止第三方注册（[`color_advanced.md`](color_advanced.md) §6.2）。播放器插件只活在层 3，走组件 `FrameHook`。
6. **现在不建空 `rui_video` / `rui_audio` crate**。空产品面与「0.1 默认精简」冲突。
7. **默认路径像素不变**：未推流的 `<Image>` / Gallery golden 与今日一致。
8. **双书写**：若公开 `<MediaSurface>` / `media_surface()`，标签与 Builder 同语义（ADR-9）；先内部 API 也可。

---

## 4. 层 1 — 引擎槽

### 4.1 名称

| 工作名 | 含义 | 公开时机 |
|--------|------|----------|
| **ImageStream** | 持续写入的 GPU 纹理源（EUI 同名契约） | 实现期；可只作模块名 |
| **MediaSurface** | 占用 taffy 叶的那块「换成动态图」的 Element | 若需要独立标签；否则复用 `<Image texture>` |
| `queue_media_frame` | 任意线程投递一帧（对位今日 `queue_image_rgba8`） | WP-M.2 |

MVP **不必**新标签：`<Image texture="movie">` + `queue_media_frame("movie", …)` 即容器。独立标签只在需要默认 `Role`、HDR 叶标记、或与静态 Image 语义分叉时再加。

### 4.2 帧契约（设计类型，未实现）

```rust
/// 纹理 / 流帧的色标签。与 P3-C 位图、HDR-B 媒体叶共用。
enum TextureColorSpace {
    /// 今日默认；未标记图。
    DisplaySrgb,
    /// P3-C；display-referred P3。
    DisplayP3,
    /// 已是相对纸白的 working linear；Y 可 > 1。
    WorkingLinear,
    /// ST 2084 绝对编码；内部先转 Working 再 tone map。
    PqSt2084 { mastering_max_nits: f32 },
    Hlg,
}

enum PixelLayout {
    Rgba8Unorm,
    Rgba16Float,
    Nv12,
    I420,
    P010,
}

enum MediaPlanes<'a> {
    Packed(&'a [u8]),
    Yuv {
        y: &'a [u8],
        u: &'a [u8],
        v: &'a [u8],
        y_stride: u32,
        uv_stride: u32,
    },
}

struct MediaFrame<'a> {
    width: u32,
    height: u32,
    layout: PixelLayout,
    color: TextureColorSpace,
    pts: Option<std::time::Duration>,
    planes: MediaPlanes<'a>,
}

/// 同一 `key` 重复投递只保留最后一帧（与今日 PENDING 去重一致）。
/// 尺寸不变则 `write_texture` 原地更新，禁止每帧重建 `wgpu::Texture`。
fn queue_media_frame(key: impl Into<String>, frame: MediaFrame<'_>);
```

`queue_image_rgba8` **保持**：等价于 `Rgba8Unorm` + `DisplaySrgb`。GIF / Canvas 继续走它，直到内部切到同一上传核。

### 4.3 上传与调度

| 规则 | 含义 |
|------|------|
| 任意线程可 `queue_*` | 与今日 `PENDING` Mutex 相同；解码在工人线程 |
| UI 线程 `render` 首段 drain | 上传 GPU；bump `DYNAMIC_TEX_EPOCH` |
| 同键同尺寸 | 原地 `write_texture` / 更新 YUV 源纹理 |
| 同键改尺寸 | 重建纹理；允许 |
| 背压 | 队列深度 1（只留最新帧）；不在引擎内做 vsync 插帧 |
| 损坏帧 | 保留上一有效 GPU 内容（对位 Shadertoy「失败保留上帧」） |

**不做**：引擎内环形缓冲多帧、帧率转换、音视频 PTS 对齐实现（那是层 2）。`pts` 只作可选元数据，供层 3 显示时间码。

### 4.4 YUV → 采样（opt-in）

GPU 转换（NV12/I420/P010 → working RGB）是 **合成关注点**，允许进 `rui` 的 opt-in feature（设计名 `image-stream`），因为：

- 应用若先在 CPU 转 RGBA 再上传，带宽与色准都更差；
- 转换矩阵必须吃 `TextureColorSpace`，与 P3-C 3×3 同源，不宜每个播放器复制一份 WGSL。

**默认 feature 关闭**。无此 feature 时 `queue_media_frame` 只接受 `Rgba8Unorm`（packed）；YUV layout 丢帧、保留上帧。

**现状（WP-M.3）**：转换核是 BT.709 有限范围（[`yuv.rs`](../../crates/rui/src/media/yuv.rs)），结果走既有 RGBA `write_texture`。同矩阵的 WGSL 在 [`yuv_convert.wgsl`](../../crates/rui/src/renderer/yuv_convert.wgsl)（offscreen 对照测）。把 RGBA dest 改成 `RENDER_ATTACHMENT` 后，drain 可改 GPU 直写而不改公开 API。P010 先在 CPU 扩成 8-bit 再套同一矩阵。

Shadertoy 叶 **不加** audio/video channel（EUI 自己也不做；[`drawing.md`](../drawing.md) §9）。

### 4.5 叶语义

与普通 Image 相同：taffy 叶、圆角、clip、opacity、transform、命中、`LayoutStatus`、damage。改像素 → `StyleChanged`（paint）；改 `width`/`height` 布局属性 → `LayoutChanged`。

HDR-B 起：带 `WorkingLinear` / `PqSt2084` / `Hlg` 的叶走 **`ToneMapParams.media_mode`**；UI chrome 仍 Identity、Y≤1。见 [`color_advanced.md`](color_advanced.md) §5.4。Scene3d 是该类别的 **第一个**实现消费者；ImageStream 是第二个，**共享**同一 tone map，禁止各 bake 8-bit 再叠。

### 4.6 无障碍

| 阶段 | 角色 |
|------|------|
| 仅纹理槽（无控件皮） | `Role::Image`（与今日 `<Image>` 一致）+ 可选 `alt` / 名称 |
| 层 3 播放器 | `Role::Video` / `Role::Audio`（AccessKit 若提供）+ 播放/暂停 Action；键盘走现有 Button/Slider |

引擎槽不实现媒体快捷键（空格暂停等）——那是播放器 chrome。

### 4.7 音频：无视觉容器

| 有 | 无 |
|----|----|
| 层 3 播放器用现有 Button / Slider / 时间文本 | `<audio>` 叶、波形原语、AudioContext |
| 可视化：应用或播放器在 `<canvas>` 里画 | 引擎 mixer、`cpal`、设备枚举 |
| 可选 `MediaClock` 协议（§4.8） | DisplayTransform 上的「音频通道」 |

摄像头预览 = **视频 ImageStream**（Freya F03 更后），不是第三种叶。

### 4.8 `MediaClock` 协议（设计，无默认实现）

对位 `RemoteAssetFetcher`：runtime **只认 trait**，实现在层 2 / 层 3。

```rust
pub trait MediaClock {
    fn play(&self);
    fn pause(&self);
    fn seek(&self, t: std::time::Duration);
    fn set_rate(&self, rate: f32);
    fn position(&self) -> std::time::Duration;
    fn duration(&self) -> Option<std::time::Duration>;
    fn paused(&self) -> bool;
}
```

`rui` 不提供时钟源、不保证 A/V 同步。播放器把 decoder 的时钟 **展示** 到 Slider；`queue_media_frame` 不因 `seek` 自动清空以外的语义——清空由投递方决定。

落地前可以不导出该 trait；先写在层 3 crate。若 Gallery 合成示例需要进度条，用 Signal 即可，不必等本协议。

---

## 5. 层 2 — 解码与设备（永不进 `rui`）

应用或示例拥有：

- 容器 / 编码（FFmpeg、GStreamer、Symphonia、平台 Media Foundation / AVFoundation）
- 网络流（HLS/DASH）与 DRM
- 音频输出（cpal / 平台）与 A/V PTS 对齐
- 摄像头捕获

向层 1 的唯一接缝：工人线程 `queue_media_frame` + 需要时 `request_redraw`（或依赖 epoch 已触发的 present 路径）。

远程媒体 URL **不**走 `Image src=` 的 `RemoteAssetFetcher` 当「自动播放」。Fetcher 只拉字节；解封装仍是层 2。产品播放器可以内部组合二者。

**许可证**：任何 GPL 解码器只允许出现在 **optional example** 或用户应用，不得成为 `rui` 的 Cargo 依赖（即使 `optional = true` 也禁止进核心 `Cargo.toml`，以免发布面沾边）。

---

## 6. 层 3 — 播放器产品（后置 crate）

### 6.1 何时存在

SemVer 0.1 / Beta **之后**。Freya F02 同档：后置能力表。实施前 **不** 在 workspace 建空 crate。

建议名（随仓库重命名 `jadix_*` 走）：`rui_video` / `rui_audio`（或 `jadix_video` / `jadix_audio`）。**不要**为对称强行让音频 crate 碰渲染器。

### 6.2 ArtPlayer 映射

| ArtPlayer | 落到 RUI |
|-----------|----------|
| 包 HTML5 `<video>` | 包层 1 `MediaSurface` / `<Image texture>` |
| 默认控件皮 | shadcn 式 `VideoPlayer` / `AudioPlayer`：现有 Button、Slider、Tooltip |
| `plugins` | 组件自注册 + overlay 槽（RSX children）；**禁止**注册进颜色/交换链 |
| HLS / FLV / 弹幕 | 层 2 + 层 3 插件 |
| `art.video` 暴露原生节点 | 暴露纹理键 + 可选 `MediaClock` |
| 高度自定义主题 | utility class / `theme.toml`；默认皮可换，不锁死一种皮肤 |

形态：无头骨架（播放状态机 + 槽位）+ 默认 chrome 产品面。对位 [`COMPONENT_LIBRARY.md`](../COMPONENT_LIBRARY.md) 的 headless / 产品分层，而不是一个「上帝类」。

### 6.3 插件边界

允许：字幕轨、弹幕层（普通 Element overlay）、画质切换 UI、缩略图预览、快捷键。

禁止：替换 `DisplayTransform`、替换 swapchain、注入第二套 wgpu 主循环、在 `window_events.rs` 加硬编码 `tick_video_*`（必须 `FrameHook`，见 [`runtime_extension_points.md`](runtime_extension_points.md)）。

### 6.4 两个 crate 不对称

| | `*_video` | `*_audio` |
|--|-----------|-----------|
| 依赖层 1 纹理槽 | 是 | 否（封面可用普通 `<Image>`） |
| 依赖层 2 | 可选（示例可注入） | 可选 |
| 默认皮 | 画面 + 控件 | 仅控件 + 可选 Canvas 可视化 |
| HDR 媒体叶 | 可以 | 无 |

共享（若值得抽）：极薄 `MediaClock` 约定、时间格式化。不要共享「解码器抽象」——那会把层 2 拖进 workspace。

---

## 7. Cargo feature 与 crate 形态

```text
默认 rui
├── queue_image_rgba8 / <Image> / GIF          ← 已有；M.1 只做同键原地上传
│
├── image-stream（opt-in，设计名）
│   └── YUV 布局 + GPU 转换；queue_media_frame 的 NV12/I420/P010
│
├── wide-gamut / hdr-surface / scene3d         ← color_advanced / drawing；媒体叶复用
│
└── 不存在的 feature
    ├── video / ffmpeg / gstreamer / cpal
    └── 整块「播放器」
```

| 物 | 进 `rui`？ | 独立 workspace crate？ |
|----|-----------|------------------------|
| RGBA 动态上传硬化 | 是（默认路径增强） | 否 |
| YUV GPU 转换 | 是，feature `image-stream` | 否 |
| 色标签字段 | 是（与 P3-C 同一字段） | 否 |
| FFmpeg 等 | **否** | 仅 example 或用户仓 |
| 播放器 UI | 否 | **后置** `rui_video` / `rui_audio` |
| 纯数学 YUV 矩阵 | 可与 `color_space.rs` 同住 | 仅当体量变大时再抽（对位 `rui_color_space` 设想） |

**SemVer / 0.1**：本文全部 **不进必达**。M.1（原地上传）若作为 Canvas/GIF 性能修复合入，须保持像素不变，且 **不** 因此公开视频 API。

---

## 8. 与颜色 / 自绘的接缝

```text
color_advanced
  P3-A / HDR-A     与媒体无关；按原计划推进
  P3-C             纹理色空间字段 ← Image 与 ImageStream 共用
  HDR-B            ToneMapParams.media_mode
                     ├─ 第一消费者：Scene3d 16f
                     └─ 第二消费者：HDR ImageStream（本文 M.4）
  HDR-C            表面格式；与谁推帧无关

drawing
  Image / Canvas / Scene3d 叶模型 ← ImageStream 复用，不新开渲染器
  Shadertoy                不加 audio/video channel
```

推进 [`color_advanced.md`](color_advanced.md) **不必**等播放器，也 **不必**等 YUV。P3-C 设计时把「纹理有色空间」写成通用字段，不要写成「仅静态 Image」。

---

## 9. 明确不做

- 在 `rui` 的 `Cargo.toml` 链 FFmpeg / GStreamer / libmpv / Symphonia / cpal / 摄像头 SDK。
- 系统原生视频 HWND / `AVPlayer` / `VideoToolbox` 层叠在 UI 上绕开 wgpu。
- 默认 feature 开 YUV 转换或任何播放器。
- `<video>` / `<audio>` HTML 语义全家桶（MSE、`buffered`、画中画 OS API、Cast）。
- Shadertoy audio/video channel。
- 把播放器 plugin 注册进 `DisplayTransform` / 交换链。
- 为音频可视化扩展颜色管线。
- 在巩固期 / 0.1 必达面加入 Video / Audio 控件。
- 现在脚手架空的 `rui_video` / `rui_audio`。
- 完整 ICC 视频 CMM（与颜色方案一致：3×3 + 传递函数；LUT 仅 `color-icc` 再议）。

---

## 10. 阶段摘要

细则与勾选：[`plans/media-av.md`](../plans/media-av.md)。

```text
M.1  同键原地 upload（GIF/Canvas 也受益）     不挡颜色工作
M.2  queue_media_frame + RGBA 流 + 文档/测     公开槽 MVP
M.3  image-stream：YUV BT.709（CPU 核 + WGSL）  opt-in ☑
M.4  HDR 媒体叶接 ToneMapParams                依赖 HDR-B
M.5  MediaClock 协议 + FrameHook 样板          可与 M.2 后并行
M.6  Gallery 合成 YUV（无 FFmpeg）             验收叶
M.7  可选 example 解码器（仓外依赖）           永不进 lib
M.8  rui_video / rui_audio 产品 crate          0.1 之后
M.9  摄像头预览                                更后（Freya F03）
```

**冻结句式（实施前契约）**

> 层 1：`rui` 只收带色标签的动态纹理；YUV 为 `image-stream` opt-in；默认不链解码器。  
> 层 2：解码 / 设备 / 同步实现永不进 `rui`。  
> 层 3：ArtPlayer 式播放器为后置独立 crate；不进 0.1；现不建空 crate。  
> 音频无视觉容器，不参与 `DisplayTransform`。  
> 「视频」在 HDR-B 中是媒体叶类别，不是控件。

---

## 11. 参考

- EUI-NEO `ImageStream` / `dynamic_texture` / 示例内 FFmpeg：[`EUI_NEO_VISUAL.md`](../comparison/EUI_NEO_VISUAL.md) §3.6  
- Freya `freya-video` / `freya-camera`：[`FREYA_COMPARISON.md`](../comparison/FREYA_COMPARISON.md) F02 / F03  
- HTML `<video>` 作为 replaced element + ArtPlayer 包皮（对照层 1 vs 层 3）  
- Flutter `Texture` + `video_player` + Chewie（引擎槽 / 解码插件 / chrome 三层）  
- [`color_advanced.md`](color_advanced.md) §5.4 · [`drawing.md`](../drawing.md) §9 · [`remote_asset.md`](../ops/remote_asset.md)  
- 扩展点（组件帧钩子，**非**解码器）：[`runtime_extension_points.md`](runtime_extension_points.md)
