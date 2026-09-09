# RUI 高级色彩：Display P3、系统色彩管理与 HDR

> **文档地位**：广色域（Display P3）、系统色彩管理、HDR / 场景线性 / PQ 的**设计方案**。今日 LDR sRGB 契约仍以 [`color.md`](color.md) 为准；令牌名与明暗见 [`theme.md`](theme.md)。  
> **冻结状态**：**未冻结**（2026-09-06）。**未实现**；本文是实施前契约，**不改当前代码**。落地时须同步改 [`color.md`](color.md) 的「输出编码」假设与 `color_freeze`。  
> **实现锚点（现状，供对照）**：[`color_space.rs`](../../crates/rui/src/style/color_space.rs) · [`color.rs`](../../crates/rui/src/style/color.rs) · [`theme.rs`](../../crates/rui/src/style/theme.rs) · [`app/state.rs`](../../crates/rui/src/app/state.rs)（swapchain）· [`rounded_rect.wgsl`](../../crates/rui/src/renderer/rounded_rect.wgsl) · [`drawing.md`](../drawing.md)（Canvas / Scene3d）· [`media.md`](media.md)（视频 = 媒体叶，非播放器）。

---

## 0. 文档边界

| 文档 | 管什么 |
|------|--------|
| [`color.md`](color.md) | 已落地：OKLCH→display sRGB、linear lerp、Oklab mix、`semantic_tone`、sRGB 交换链二次编码 |
| [`theme.md`](theme.md) | `DesignTokens` 名、明暗、`theme.toml` |
| [`media.md`](media.md) | 动态纹理 / 视频帧槽 / 播放器边界；**不是**本文的 DisplayTransform |
| **本文** | P3 录入与色域映射、显示变换、图像标签、HDR 合成与 PQ/scRGB/EDR |

**P3 与 HDR 正交**：P3 是 **色域**（xy 三角更大）；HDR 是 **动态范围**（相对 0–1 之外的亮度）。可以有「P3 + SDR 8-bit（映射进 sRGB 屏）」而无 HDR；也可以有「sRGB 色域 + HDR 亮度」。实施顺序见 §8：先 P3-A（不换表面格式），再显示探测，HDR 接在同一 `DisplayTransform` 上。

---

## 1. 今日实现基线（代码事实）

这些约束决定方案必须 **默认路径像素级不变**。

| 层 | 现状 |
|----|------|
| 令牌存储 | `DesignTokens` 字段是 `[f32; 4]`，**构造时** `oklch_to_display_srgba` 编成 display sRGB。超出 sRGB 三角的 OKLCH 在 encode 时被分量 clamp。 |
| 解析 | `parse_color` / `try_parse_color` 返回 display sRGB。`parse_css_color_function` 支持 `rgb/hsl/hwb/oklch/oklab/color-mix`，**不**解析 `color(display-p3 …)`。 |
| 混色 | CPU：`lerp_display_srgb_linear`、`mix_display_srgb_in_oklab`。工作空间实际是 **linear sRGB / Oklab**，两端都经 display sRGB 编解码。 |
| 交换链 | `surface_caps.formats` 优先 `is_srgb()`（8-bit `*Srgb`）。`SurfaceConfiguration.color_space = Auto`。offscreen / Scene3d 目标为 `Rgba8UnormSrgb`。 |
| 片元 | `rounded_rect.wgsl` 把 **display sRGB 输入** `linearize` 后再写入 sRGB 附件，避免二次编码。 |
| 位图 | Image / emoji / glyphon / path CPU 光栅均为 8-bit RGBA；**无 ICC / cICP**。未标记图按 sRGB 理解。 |
| 平台 | `crate::platform` 无色域 / HDR 峰值探测。挂钩设计见本文 §4.3；**不是** [`platform.md`](platform.md) 的当前阶段（该文 P3 = 字体/剪贴板，P3c = GPU 序；色域属本文 P3-B，见其 §12）。 |

**结论**：今日管线是 **display-referred sRGB LDR**。任何广色域/HDR 方案都必须把「工作空间」和「输出编码」拆开；不能继续在令牌构造时一次性 bake 死 sRGB。

---

## 2. 设计原则

1. **不 fork 入口**：扩展 `parse_color` / `parse_css_color_function`，不另起 `parse_p3_color`。
2. **默认像素不变**：`DisplayTransform::sdr_srgb()` 下，现有 Gallery / `color_freeze` / golden 与今日逐像素一致。
3. **公开 `[f32; 4]` 在 SDR 路径仍表示 display sRGB 0–1**。P3-A 把超色域色 **映射进** sRGB 再返回；不把「未映射的 P3 通道」塞进现有字段。
4. **工作空间用 Oklab / XYZ D65**（与 CSS Color 4 / 现有 `color_space.rs` 矩阵同源），不用在 CPU 上再引入第二套 OKLab。
5. **不做完整 ICC CMM**（lcms2 全 profile 链）作为核心依赖；P3 用 3×3 + 传递函数；完整 ICC 仅可选 feature。
6. **UI chrome 是 SDR paper-white**：HDR 开启后按钮/面板停在纸白（约 80–203 nits 可配），高光只来自 Scene3d / 视频 / 显式 HDR 叶。
7. **与 `semantic_tone` 解耦**：语气色仍是 Tailwind 色阶名；它们随工作空间走同一 DisplayTransform，不单独做 P3 表。
8. **Emoji / 文本 atlas 永不升 HDR**：8-bit sRGB，合成时当作 SDR 层贴在纸白。

---

## 3. 颜色表示与管线（核心模型）

```text
  指定色 Specified          工作色 Working              输出色 Encoded
  oklch() / #hex /          Oklab 或 XYZ D65            交换链 texel
  color(display-p3)         （混色、lerp、SDF 输入）      display sRGB 8-bit
  theme 令牌源字符串         HDR 时允许 Y > 1（相对纸白）   或 P3/scRGB/PQ
           │                         │                          │
           └──── parse ──────────────┤                          │
                                     ├──── mix / lerp ──────────┤
                                     └──── DisplayTransform ────┘
```

### 3.1 三态（设计类型，未实现）

```rust
/// 解析后、映射前。Alpha 直通道 0–1。
enum SpecifiedColor {
    Oklch { l: f32, c: f32, h_deg: f32, a: f32 },
    Oklab { l: f32, a: f32, b: f32, alpha: f32 },
    /// Display P3，通道为 **display-referred**（sRGB 式 γ，CSS `color(display-p3)`）。
    DisplayP3 { r: f32, g: f32, b: f32, a: f32 },
    /// 已是 display sRGB（hex / rgb() / 今日令牌 bake 结果）。
    DisplaySrgb { rgba: [f32; 4] },
}

/// 混色与合成。线性光，D65。相对纸白：SDR 下 1.0 = 白；HDR 下 1.0 = paper white。
struct WorkingColor {
    xyz: [f32; 3], // 或等价 Oklab，二选一为真源
    alpha: f32,
}

struct DisplayTransform {
    gamut: OutputGamut,       // Srgb | DisplayP3
    transfer: OutputTransfer, // Srgb | Linear | PqSt2084 | WindowsScRgb
    /// 色域外：CSS Color 4 式 OKLCH 降彩 vs 硬 clip。
    gamut_map: GamutMap,      // OklchChromaReduce | Clip
    paper_white_nits: f32,    // HDR；SDR 忽略
    peak_nits: f32,           // HDR；SDR = 80
}

enum OutputGamut { Srgb, DisplayP3 }
enum OutputTransfer { Srgb, Linear, PqSt2084, WindowsScRgb }
```

`WorkingColor` 真源建议 **XYZ D65**（矩阵已在 `color_space.rs`），Oklab 仅用于感知 mix / 色域映射。避免「有时 linear sRGB、有时 linear P3」两套 lerp。

### 3.2 对现有 API 的兼容策略

| API | SDR 默认（必须保持） | P3-A 之后 | HDR-C 之后 |
|-----|---------------------|-----------|------------|
| `parse_color` → `[f32; 4]` | display sRGB | 仍 display sRGB（超色域已映射） | **仍建议**返回「当前输出编码的 0–1 相对值」或新增 `parse_working_color`；禁止无文档地把 PQ 码值塞进旧字段 |
| `DesignTokens` 字段 | display sRGB bake | `compose_theme` **按当前** `DisplayTransform` 再编码；默认 transform 与今日相同 | 同上；HDR 窗口可编码为 scRGB 线性（1.0 = 纸白） |
| WGSL 输入 | display sRGB | 不变（已映射） | 改为 working linear + uniform `DisplayTransform`，或预编码 texel |

**推荐不破坏 SemVer 0.1**：0.1 只做 P3-A（解析 + 映射到现有 sRGB 表面）。P3-B/HDR 换表面格式视为引擎能力，走 §6.3 的 Cargo feature 与窗口创建选项，默认关。

### 3.3 令牌何时转换？

今日在 `Theme::light()` 里就把 OKLCH bake 成 sRGB，导致：

- 换显示（外接 P3 屏）无法重编码；
- `set_theme_mode` 也只是换另一张已 bake 的表。

**方案**：令牌 **源**保持 OKLCH 字符串或 `SpecifiedColor`（`theme.toml` 已是字符串，正好）；`compose_theme` / `install_theme_overrides` 的最后一步才 `Specified → Working → Encoded`。默认 `DisplayTransform::sdr_srgb()` 的 Encoded 必须匹配当前 `oklch_to_display_srgba`（允许 ULP 级差，`color_freeze` 钉死关键色）。

`set_theme_mode` 已与 patch merge 组合（见 [`theme.md`](theme.md)）；将来 **显示变换变更** 也应走同一 `compose_theme`，避免切 HDR 丢品牌 patch。

---

## 4. Display P3 与系统色彩管理

### 4.1 目标

1. 设计稿 / `theme.toml` 可写 **sRGB 三角外** 的品牌色（`oklch` 高彩或 `color(display-p3 …)`）。
2. **sRGB 屏**：感知色域映射后显示，禁止无声 clip 成荧光或假中性。
3. **P3 屏**（P3-B）：交换链或合成器以 Display P3 / 扩展线性输出，品牌色少映射、少裁切。
4. **位图**（P3-C）：识别 Display P3 标签，与 UI 向量令牌进同一工作空间再输出。

### 4.2 P3-A — 录入与映射（不换表面）

**交付**：

- `color(display-p3 r g b [/ a])`：CSS Color 4 通道为 0–1，**display P3 传递函数与 sRGB 相同**（不是 linear-P3）。
- 可选：`color(srgb-linear …)`（与已有 `color-mix(in srgb-linear)` 对齐）。
- `oklch()` 已存在：高彩值不再在 `encode_linear_srgb_component` 里默默 clamp 到「错误的绿」；改为 **工作空间 → gamut map → sRGB encode**。
- `GamutMap::OklchChromaReduce`：固定 L、h，降低 C 直到线性 sRGB 通道 ∈ [0, 1]（对齐 CSS Color 4 映射思路）。`Clip` 仅作调试/对比。
- 默认路径：无 P3 输入且 OKLCH 本就在 sRGB 内 → **跳过映射**，与今日逐像素一致。

**片元**：不改 WGSL。CPU 仍喂 display sRGB。

**测试**：

| 用例 | 断言 |
|------|------|
| `oklch(0.205 0 0)` | 与当前 `oklch_to_display_srgba` 一致 |
| `color(display-p3 1 0 0)` 在 sRGB 输出 | 映射后 R 通道接近 1，G/B ≥ 0，**不是** clip 后的 `(1,0,0)` 与 P3 红相同 |
| 往返 `mix in oklab` 两 sRGB 色 | 与现 `mix_display_srgb_in_oklab` 一致 |
| 未知 `color(rec2020 …)` | `try_parse_color` → `None`（P3-A 不承诺 Rec.2020） |

### 4.3 P3-B — `DisplayTransform` 与平台探测

**交付**：每窗口一份 `DisplayTransform`（多窗口可不同：笔记本 sRGB + 外接 P3）。

```text
probe_display(window)
  → gamut: Srgb | DisplayP3
  → peak_nits / edr_max   （HDR 用，P3-B 可填 80 并忽略）
  → 失败 → sdr_srgb()
compose_theme() + 重建 pipeline uniform / 令牌 Encoded
```

| 平台 | 探测（设计，绑定 `crate::platform`） | 失败回退 |
|------|--------------------------------------|----------|
| Windows | DXGI 输出色空间 / Advanced Color；普通 SDR 桌面当 sRGB | sRGB |
| macOS | `NSScreen.colorSpace` 是否近似 Display P3；`maximumPotentialExtendedDynamicRangeColorComponentValue` 留给 HDR | sRGB |
| Linux | 无稳定协议时 **一律 sRGB**；不把「显示器广告 P3」当已校准 | sRGB |

**wgpu 接缝**（`app/state.rs`）：

- 今日：`find(|f| f.is_srgb())` + `color_space: Auto`。
- P3-B：若探测为 P3 且 caps 允许，可选用 **非 sRGB 10-bit / 16-float** 表面，并把 `color_space` 设成与合成器约定的值（以当时 wgpu 枚举为准）。
- **默认仍选 sRGB 8-bit**。P3 表面必须 opt-in（窗口标志或 `RUI_COLOR_MANAGED=1`），避免合成器把线性当 γ 或把 P3 当 sRGB（经典「过饱和」）。

无 ICC 配置文件时：**不**读系统 `.icc`。P3-B 只区分「sRGB 三角 vs Display P3 三角」，白点 D65。这不是 ColorSync 替代品。

**shader**：增加 `DisplayTransform` uniform 的时机放在 P3-B 或 HDR-A（二者都需要「工作线性 → 输出」）。若 P3-B 仍只输出 8-bit sRGB，可继续 CPU 编码，shader 不动。

### 4.4 P3-C — Tagged 位图

**范围**：

| 输入 | 行为 |
|------|------|
| 无标签 JPEG/PNG/WebP | 当 **sRGB**（与今日一致） |
| PNG/WebP 带 Display P3 ICC **或** cICP / CICP 标识 P3 | 3×3 P3→XYZ→工作空间，再 DisplayTransform |
| SVG / 矢量 / 令牌色 | 不走 ICC；走 Specified 令牌 |
| 完整 ICC v4 LUT、CMYK、打印机 profile | **核心不做**；可选 `color-icc` feature 再议 |

Image 上传路径（`image_registry`）今日按 RGBA8 sRGB 采样。P3-C 两种实现选一：

- **CPU 预转换**：解码后转成当前 Encoded 再上传 8-bit（简单，换屏要重解码）；
- **GPU 采样 + 矩阵**：纹理标 `Srgb`/`P3`，片元乘 3×3（换 `DisplayTransform` 不必重传图）。

推荐 **GPU 矩阵**（换外接屏只更新 uniform）。Emoji atlas 强制 `Srgb`，不读 ICC。

### 4.5 与 CSS / shadcn 的对齐

- 令牌 **源**继续 OKLCH（[`theme.md`](theme.md)），与 shadcn v4 文档站一致。
- shadcn 默认色大多在 sRGB 内；P3 是 **应用品牌覆盖** 能力，不是改默认表。
- `color-mix(in oklab, …)`：P3-A 起应在 Working/Oklab 混合，再映射；sRGB 输入时数值对齐今日实现。

---

## 5. HDR / 场景线性 / PQ

### 5.1 目标

把 UI 合成从「display sRGB 0–1」升级为 **相对纸白的场景线性**，再按显示器峰值做 tone map 并编码到平台 HDR 表面。3D / 视频高光可以 > 1；控件 chrome 保持纸白。

**不是**把所有令牌亮度乘 4；那会让 Button 变成灯箱。

### 5.2 亮度约定

| 名称 | 含义 | 典型值 |
|------|------|--------|
| **相对场景线性** | `WorkingColor` 的 Y；**1.0 = paper white** | UI 白 = 1.0 |
| **paper_white_nits** | 1.0 对应的绝对亮度 | Windows HDR 滑条 / macOS EDR 参考；设计默认 **203**（BT.2408 图形白）或跟系统 SDR 内容亮度 |
| **peak_nits** | 显示器峰值 | 400–1600；探测失败则 800 |
| **PQ (ST 2084)** | 绝对编码，10000 nits 满码 | 输出阶段；内部不算 PQ |

Windows **scRGB**：线性，历史 1.0 ≈ 80 nits；现代 Advanced Color 窗口常把 1.0 当纸白。RUI 内部 **只认相对纸白**，写出 scRGB 时乘 `paper_white_nits / 80` 或按 DXGI 色空间文档换算，**禁止**两种约定混用。

macOS **EDR**：CALayer `wantsExtendedDynamicRangeContent`；1.0 为 SDR 白，> 1 为扩展。映射到同一相对纸白。

Linux：无稳定 HDR 协议则 **永不进入 HDR-C**。

### 5.3 HDR-A — 片元场景线性（SDR 外观不变）

**交付**（纯重构，用户无可见 HDR）：

- 片元内部在 **linear-light** 合成（阴影、SDF、blur 已有 linearize；统一「输入已是 working linear」还是「输入 display 再 linearize」）。
- Uniform 预留 `DisplayTransform`（gamut + transfer + paper/peak）；SDR 时 transfer=Srgb、peak=paper=80，编码路径等于今日 `linearize` + sRGB 附件。
- CPU lerp 仍走 [`color.md`](color.md) §B，直到 HDR-B 把动画也迁到 Working。

**验收**：`color_freeze` + 现有 golden / Gallery 像素不变（允许 1 ULP）。

### 5.4 HDR-B — `ToneMapParams` 与 3D/视频 overlay

```rust
struct ToneMapParams {
    paper_white_nits: f32,
    peak_nits: f32,
    /// UI 层：Identity（已在纸白）。HDR 媒体：Bt2390 或 Hable 仅作用于 media 叶。
    ui_mode: ToneMapUiMode, // Identity
    media_mode: ToneMapMediaMode, // Bt2390 | ClampToPeak
}
```

**合成顺序**（设计）：

```text
1. UI 层（控件、文字、emoji）：Working Y ≤ 1，不进媒体 tone map
2. Scene3d / HDR Canvas / 视频：可 Y > 1，按 media_mode 压到 peak
3. 两者在 **同一 working linear** 上 alpha 合成
4. 一次 DisplayTransform 写出
```

禁止：UI 与 3D 各 bake 到 8-bit 再叠（今日 Scene3d `Rgba8UnormSrgb` 正是这种限制）。HDR-B 要求 Scene3d 目标改为 **16-float working** 或与主 pass 同格式，再共享 tone map。见 [`drawing.md`](../drawing.md) 实验面。

**「视频」是合成类别，不是控件。** Scene3d 是本阶段第一消费者；带 PQ/HLG/working-linear 标签的动态纹理（ImageStream）是同一 `media_mode` 的第二源。解码器、`<Video>` 播放器、FFmpeg **不在**本设计范围内，见 [`media.md`](media.md)。音频不参与 `DisplayTransform`。

**transition / color-mix**：在 Working 落地前继续 [`color.md`](color.md) §B；落地后 lerp 在 XYZ 或 Oklab，**不要**在 PQ 码值上插值。

### 5.5 HDR-C — 16-bit / 平台 HDR 表面

**交付**：

- 交换链候选：`Rgba16Float`（scRGB/EDR 友好）或平台 10-bit HDR10（PQ）。以 wgpu `SurfaceCapabilities` 为准，无则保持 SDR。
- 窗口 opt-in：`WindowOptions { hdr: Auto | Off | On }`，默认 `Off`（0.1 承诺面不自动开 HDR）。
- PQ 路径：working → 绝对 nits（`Y * paper_white_nits`）→ ST 2084；仅当表面色空间是 HDR10/PQ 时。
- 回退：任何一步失败 → `sdr_srgb()`，日志一次。

**emoji / glyphon**：仍 8-bit sRGB 纹理，采样后当 SDR（Y≤1）参与合成。

### 5.6 与 P3 的组合

| 模式 | gamut | transfer | 表面 |
|------|-------|----------|------|
| 今日 / 默认 | sRGB | sRGB | 8-bit `*Srgb` |
| P3-A | sRGB（映射后） | sRGB | 同上 |
| P3-B opt-in | Display P3 | sRGB 或 linear | 10-bit 或 16f |
| HDR-C | sRGB 或 P3 | PQ 或 scRGB | 16f / HDR10 |

色域映射在 **tone map 之前**：先把 xy 收进输出 gamut，再压亮度。不要在 PQ 曲线上做 OKLCH 降彩。

---

## 6. 打包形态：feature、crate，而非 plugin

§4（广色域）与 §5（HDR）**共享** §3 的 `DisplayTransform` / `WorkingColor` / 延迟 `compose_theme`。二者**不应**各做成一块 runtime plugin，也**不应**各对应一个整块 Cargo feature；按 **阶段（P3-A…C、HDR-A…C）** 切默认核心与 opt-in 能力。

### 6.1 结论

| 维度 | Display P3（§4） | HDR（§5） |
|------|------------------|-----------|
| **独立 plugin crate？** | 否 | 否 |
| **整块一个 feature？** | 否（拆 P3-A / P3-B / P3-C） | 否（拆 HDR-A / HDR-B / HDR-C） |
| **推荐形态** | P3-A 进默认 `rui`；P3-B/C → `wide-gamut` | HDR-A 进默认 `rui`；HDR-B 绑 `scene3d`；HDR-C → `hdr-surface` |
| **可选 workspace crate** | 仅纯数学（见 §6.4） | 同上（PQ / tone-map 纯函数） |

### 6.2 为什么不是 runtime plugin

[`runtime_extension_points.md`](runtime_extension_points.md) 的 plugin 模型适合 **组件帧钩子、RSX 标签、图标注册**——边界清晰、可自注册。

P3 / HDR 是 **引擎管线横切能力**，贯穿 `parse_color`、`compose_theme`、`app/state` 交换链、`renderer/*.wgsl`、`platform` 探测、`image_registry`、Scene3d / Canvas 中间目标。与 `scene3d` / `svg`（可选**叶节点**）不同：颜色是 **每个像素必经的管道**。

做成第三方 runtime plugin 会导致渲染器到处 trait / 动态分发，难保证默认路径与 `color_freeze` 一致，也不符合 ADR-3「锁定 wgpu + 自研管线」。**第三方不应「注册一套 DisplayTransform」**；应用只能通过 `WindowOptions` / 环境变量 opt-in，由引擎统一实现。

### 6.3 Cargo feature 矩阵（设计名，落地时写入 `crates/rui/Cargo.toml`）

```text
默认 rui（无 feature）
├── P3-A   color(display-p3)、OKLCH 降彩 → 仍 8-bit sRGB 面     ← 默认开启，无 feature 名
├── HDR-A  片元场景线性、SDR 像素不变                           ← 默认开启，无 feature 名
│
├── wide-gamut（opt-in）
│   ├── P3-B   platform 探测 + P3 交换链 / compose 重编码
│   └── P3-C   tagged 位图、GPU 3×3 采样
│
├── hdr-surface（opt-in；可依赖 wide-gamut）
│   └── HDR-C   Rgba16Float / PQ / scRGB 表面；窗口 `hdr: Off|Auto|On`，默认 Off
│
├── scene3d（已有 feature）上叠加
│   └── HDR-B   16f 中间目标 + ToneMapParams（与 Scene3d / HDR Canvas 合成）
│
└── color-icc（opt-in，§4.4）
    └── 完整 ICC CMM（如 lcms2）；核心路径仍 3×3 + 传递函数
```

**依赖关系**（feature 声明，非运行时）：

```toml
# 设计草案 — 未实现
wide-gamut = []                    # P3-B + P3-C
hdr-surface = ["wide-gamut"]       # HDR-C 需要 DisplayTransform 与 P3-B 同构
# HDR-B 不单独 feature：随 scene3d 或 hdr-surface 启用时编译相关合成路径
color-icc = ["wide-gamut"]         # 可选；拉取 ICC 解析依赖
```

| 阶段 | 默认 `rui` | `wide-gamut` | `scene3d` | `hdr-surface` | `color-icc` |
|------|------------|--------------|-----------|---------------|-------------|
| P3-A | ☑ | — | — | — | — |
| P3-B | — | ☑ | — | （推荐） | — |
| P3-C | — | ☑ | — | — | 可选增强 |
| HDR-A | ☑ | — | — | — | — |
| HDR-B | — | — | ☑ | ☑ | — |
| HDR-C | — | — | — | ☑ | — |

**与现有惯例对齐**（见 [`ENGINEERING_GRADE_FRAMEWORK.md`](../ENGINEERING_GRADE_FRAMEWORK.md) §2、`crates/rui/Cargo.toml`）：

| 先例 | 形态 | 与本文关系 |
|------|------|------------|
| `scene3d` | feature，默认关 | HDR-B 叠在同一渲染子路径 |
| `svg` / `emoji-svg` | feature，默认关 | 仅叶节点；≠ 全管线 |
| `image-hdr` | feature | **HDR 文件解码**，≠ 显示 HDR（勿混淆） |
| `image-stream`（媒体方案） | feature，默认关 | **YUV 上传**，≠ 播放器、≠ 显示 HDR；见 [`media.md`](media.md) |
| OKLCH / `theme` | 默认核心 | P3-A / HDR-A 同属默认路径 |

**SemVer / 0.1**：P3-A、HDR-A 可在不发版破坏承诺的前提下合入默认构建；P3-B 及之后、HDR-C **不进 0.1 必达**（见 §9）。公开 prelude **不导出** `DisplayTransform` / `ToneMapParams`；先内部 + `WindowOptions`。

### 6.4 可选 workspace crate（仅纯数学）

集成层 **必须留在 `rui`**（`theme`、`state`、WGSL uniform、窗口创建）。仅当纯函数体量变大时，可抽 workspace crate（类比 `rui_style_tokens`）：

```text
rui_color_space/          # 或扩展现有 style/color_space.rs
  specified.rs            # SpecifiedColor
  working.rs              # WorkingColor, XYZ / Oklab
  transform.rs            # DisplayTransform, gamut_map, pq_encode
```

`platform` 探测可留在 `rui::platform`，或在该 crate 内只放 **trait + 无 OS 依赖的默认实现**。

**不建议** `rui_display_p3` + `rui_hdr` 两个对外 crate：P3+HDR 组合、版本耦合、prelude 导出成本高于「单 `rui` + 少量 feature」。

### 6.5 冻结句式（实施前契约）

> P3-A / HDR-A：`rui` 默认路径，不增 feature。  
> `wide-gamut`：P3-B / P3-C（窗口 opt-in）。  
> `hdr-surface`：HDR-C（默认 `Off`；Linux 可永久无实现）。  
> HDR-B：与 `scene3d`（及未来 HDR Canvas）共享合成路径。  
> 不做 runtime plugin；完整 ICC 仅 `color-icc` feature。

---

## 7. 模块接缝（落地时改哪里）

| 模块 | 角色 |
|------|------|
| `style/color_space.rs` | XYZ↔P3 矩阵、OKLCH 降彩映射、PQ/scRGB encode（纯函数，无平台） |
| `style/color.rs` | `color(display-p3)`；可选 `parse_working_color` |
| `style/theme.rs` + `theme_override.rs` | 源色延迟编码；`compose_theme` 吃 `DisplayTransform` |
| `app/state.rs` | 表面格式 / `color_space`；每窗口 transform |
| `crate::platform` | 探测色域与 nits；失败回退 |
| `renderer/*.wgsl` | HDR-A 起：working linear + 输出编码 |
| `renderer/scene3d.rs` / Canvas | HDR-B：16f 中间目标，禁止过早 8-bit bake |
| `app/image_registry.rs` | P3-C：纹理色空间标签 + 矩阵 |
| `color_freeze` | SDR 回归钉死；P3/HDR 新测另开 `color_advanced` 模块 |

公开 prelude：**0.1 不导出** `DisplayTransform` / `ToneMapParams`。先内部 + 窗口选项。

---

## 8. 明确不做（本设计范围内）

- 完整 ICC CMM、打印机 / CMYK、每应用自定义 `.icc` 安装。
- Rec.2020 UI 令牌、`color(rec2020)`（P3-A 拒绝解析）。
- 自动把所有窗口开成 HDR。
- 在 PQ 或 display sRGB 上做物理 lerp 冒充场景线性。
- 用改 token 掩盖 sRGB 交换链二次编码（今日 `rounded_rect.wgsl` 的 `linearize` 仍有效，见 [`color.md`](color.md)）。
- 把 Toast / `semantic_tone` 改成 `chart-*` 或 P3 专用色表。
- 把 §4 / §5 **整块**做成 runtime plugin 或两个独立对外 crate（见 §6）。
- 在颜色管线里实现播放器 / 解码器 / `<Video>` 控件（媒体叶 ≠ 播放器，见 [`media.md`](media.md)）。

---

## 9. 阶段、依赖与验收

```text
P3-A  解析 + OKLCH 降彩 → 现有 sRGB 表面          无平台、无 WGSL
  │
  ├─ P3-B  探测 + opt-in P3 表面 / compose 重编码   依赖 platform
  │     │
  │     └─ P3-C  位图标签 + 采样矩阵                 依赖 P3-A 工作空间
  │
HDR-A  shader 场景线性、SDR 像素不变                 可与 P3-A 并行
  │
HDR-B  ToneMapParams、Scene3d 16f 合成              依赖 HDR-A
  │
HDR-C  16f/PQ 表面 opt-in                           依赖 HDR-B + P3-B 的 DisplayTransform
```

| 阶段 | 用户可见结果 | 门禁 |
|------|----------------|------|
| P3-A | `theme.toml` 可写 P3；sRGB 屏上看起来「像设计」而非 clip | 库测映射；默认令牌 golden 不变 |
| P3-B | P3 屏 opt-in 后品牌红更满 | 无 caps 时与 P3-A 相同 |
| P3-C | P3 照片与按钮色域一致 | 无标签图与今日一致 |
| HDR-A | 无 | golden / `color_freeze` |
| HDR-B | 3D 高光不把 UI 洗白 | Gallery Scene3d + Button 同帧 |
| HDR-C | 系统 HDR 开、窗口 opt-in 时纸白稳定 | 失败回退 SDR；Linux 可永久跳过 |

**0.1 / Beta 挡板**：不把 P3-B 及之后列为发版必达。P3-A 可在发版后作为增强，仍须守默认像素不变。Cargo feature 命名与矩阵见 §6.3。

---

## 10. 参考

- CSS Color Module Level 4：`color()`、OKLCH、色域映射  
- CSS Color HDR（草案）：纸白、scene vs display referred  
- BT.2408 / BT.2390：图形白、PQ tone map  
- wgpu `SurfaceConfiguration.color_space`、16-float 附件  
- Windows Advanced Color / scRGB；Apple EDR  
- 本文对照的实现：[`color.md`](color.md) §A–B、[`theme.md`](theme.md) `compose_theme`  
- 扩展点边界（**非**颜色管线）：[`runtime_extension_points.md`](runtime_extension_points.md)  
- 动态纹理 / 视频槽 / 播放器分层（**非**本文）：[`media.md`](media.md) · 计划 [`plans/media-av.md`](../plans/media-av.md)
