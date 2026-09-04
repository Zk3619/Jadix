# Crate 依赖审计（非正式）

> **地位**：gallery 体积 / 传递依赖备忘，**非执行源**。发布体积门见 [`size_and_memory.md`](size_and_memory.md) 与 AUTHORITY Wave P。  
> 原路径 `docs/reference.md`（2026-09-05 迁入 `ops/`）。

基于 `gallery`（`bundled-lucide` + `svg`）的实测数据与代码扫描，整理如下审计清单。

## 基线数据

| 指标 | 数值 |
|------|------|
| 传递依赖包数（gallery） | **534** |
| Release 二进制（gallery.exe） | **~11.5 MB**（12,017,152 bytes） |
| rui 直接依赖 | 25 个 |
| 重复版本组 | **~24 组**（`cargo tree --duplicates`） |
| 本地 `crates/glyphon` | 存在，但 **未接入 workspace**（仍用 crates.io 0.12） |

---

## 评分说明

每项按 1–5 分（5 = 问题最严重 / 收益最大）：

- **膨胀**：传递依赖体积与深度
- **重复**：同 crate 多版本共存
- **冗余**：API 使用面 vs 引入能力不匹配
- **难度**：优化实施成本（5 = 最容易）

**优先级** = 膨胀×0.3 + 重复×0.25 + 冗余×0.25 + 难度×0.2（满分 5）

---

## Tier S — 建议优先做（优先级 ≥ 3.8）

### 1. `image` 0.25 — 格式全开

| 膨胀 | 重复 | 冗余 | 难度 | **优先级** |
|:----:|:----:|:----:|:----:|:----------:|
| 5 | 3 | 5 | 4 | **4.35** |

**现状**：`Cargo.toml` 开了 15+ 格式 feature，传递链最重的是：

```
image → ravif → rav1e (+ av-scenechange, aligned-vec…)   # AVIF
image → exr → pulp                                       # EXR
image → tiff → zune-jpeg                                 # TIFF
```

**代码实际用法**（`media/image.rs`）：`decode_raster`、`resize`、`blur` 等 `imageops`；编码仅 `png` / `jpeg` / `webp`。文件对话框过滤器也只列 `png/jpg/jpeg/gif/webp/bmp`。

**建议**：
1. 拆成 feature：`image-png`、`image-jpeg`、`image-webp`、`image-gif`（GIF 已有自研路径，可只保留 decode）
2. 默认只开 `png` + `jpeg` + `webp`；`avif`/`exr`/`tiff`/`hdr`/`pnm`/`qoi`/`ff` 改为 opt-in
3. 长期：用 `png` + `jpeg-decoder` + `image-webp` 替换 `image` 单体（保留 `imageops` 薄封装或自写 resize）

**预期收益**：少 80–150 个传递 crate（主要是 `rav1e` 链），二进制可能减 **0.5–2 MB**。

---

### 2. `gif` 双版本 + 三重引入

| 膨胀 | 重复 | 冗余 | 难度 | **优先级** |
|:----:|:----:|:----:|:----:|:----------:|
| 2 | 5 | 3 | 5 | **3.65** |

**现状**：
- `rui` 直接依赖 `gif 0.14`（`media/gif.rs` 动画合成）
- `image` 也拉 `gif 0.14`
- `resvg`（`svg` feature）拉 `gif 0.13`

**建议**：
1. workspace 统一：`gif = "0.14"` + 对 `resvg` 尝试 `[patch]` 或等上游对齐
2. 评估是否去掉 `image` 的 `gif` feature（动画走自研 `media/gif.rs`）

---

### 3. `png` / `zune-jpeg` / `bitflags` 重复（image ↔ resvg）

| 膨胀 | 重复 | 冗余 | 难度 | **优先级** |
|:----:|:----:|:----:|:----:|:----------:|
| 3 | 5 | 2 | 3 | **3.55** |

**重复链**：
```
png 0.17 ← tiny-skia ← resvg (svg feature)
png 0.18 ← image
bitflags 1.3 ← png 0.17
bitflags 2.x ← 其余
zune-jpeg 0.4 ← resvg
zune-jpeg 0.5 ← image
```

**建议**：`svg` 已是 optional（做得好）。若 gallery 需要 SVG，关注 `resvg` 0.46+ 是否对齐 `png 0.18`；或把 emoji-svg 与通用 SVG 分开（Twemoji 可走 PNG fallback，已有 `emoji-svg` feature）。

---

### 4. `fontdb` 直接依赖 — 疑似冗余

| 膨胀 | 重复 | 冗余 | 难度 | 难度 | **优先级** |
|:----:|:----:|:----:|:----:|:----------:|
| 1 | 2 | 4 | 5 | **3.15** |

**现状**：`Cargo.toml` 声明 `fontdb`，但源码 **零处** `use fontdb::`；全部走 `glyphon::fontdb::`。

**建议**：删除 `fontdb` 直接依赖，只通过 `glyphon` 使用。`ttf-parser` 保留（`text.rs` 直接读 COLR/CPAL 表）。

---

## Tier A — 中期优化（优先级 2.8–3.5）

### 5. `palette` 0.7

| 膨胀 | 重复 | 冗余 | 难度 | **优先级** |
|:----:|:----:|:----:|:----:|:----------:|
| 2 | 1 | 5 | 4 | **3.05** |

**调用面**：仅 `style/color.rs`、`style/color_space.rs`（Oklab/Oklch 混色）。

**建议**：内联 Oklab 转换（~50 行）或依赖更轻的 `colorous`/`oklab` 单功能 crate；去掉 `palette_derive` + `phf` 传递链。

---

### 6. `serde` + `serde_json`

| 膨胀 | 重复 | 冗余 | 难度 | **优先级** |
|:----:|:----:|:----:|:----:|:----------:|
| 2 | 1 | 4 | 4 | **2.85** |

**调用面**：
- `app/hot_reload.rs` — 热重载 JSON 协议
- `utils/lucide_animated.rs` — 动画图标 JSON
- `rui_hot_reload_cli` — 开发工具

**建议**：新增 feature `hot-reload = ["dep:serde", "dep:serde_json"]`，默认关闭；生产二进制可减掉 serde 全家桶。

---

### 7. `pollster` 0.4

| 膨胀 | 重复 | 冗余 | 难度 | **优先级** |
|:----:|:----:|:----:|:----:|:----------:|
| 1 | 1 | 4 | 5 | **2.75** |

**调用面**：`lifecycle.rs`、`glass_gpu.rs`、`offscreen.rs`（共 4 处 `block_on`）。

**建议**：用 `std` future + 自写 10 行 `block_on`，或 `futures-lite::block_on`（更轻）。

---

### 8. `font-types` / `skrifa` / `read-fonts` 双栈

| 膨胀 | 重复 | 冗余 | 难度 | **优先级** |
|:----:|:----:|:----:|:----:|:----------:|
| 4 | 5 | 2 | 1 | **3.45** |

**重复链**：
```
cosmic-text → harfrust → read-fonts 0.37 / skrifa 0.40 / font-types 0.11
cosmic-text → swash   → read-fonts 0.41 / skrifa 0.44 / font-types 0.12
rui 直接依赖 swash（prepare_text COLR + icon_texture）
```

**建议**（难，但 ROI 高）：
1. 把本地 `crates/glyphon` 接入 workspace `[patch]`，裁剪 cosmic-text 路径
2. 评估能否去掉 `rui` 对 `swash` 的直接依赖，统一走 glyphon 内部 swash
3. 关注 `cosmic-text 0.20` 是否统一 font 栈

---

### 9. Windows crate 碎片化

| 膨胀 | 重复 | 冗余 | 难度 | **优先级** |
|:----:|:----:|:----:|:----:|:----------:|
| 3 | 5 | 2 | 2 | **3.15** |

**重复**：
```
windows-sys 0.52 ← winit
windows-sys 0.59 ← rui, rfd, gpu_probe
windows-sys 0.60 ← notify (hot_reload_cli)
windows-sys 0.61 ← walkdir chain
windows  0.59 ← clipboard-rs
windows  0.62 ← accesskit, wgpu-hal
```

**建议**：
1. `rui` 的 `windows-sys` 升级到 0.62，与 accesskit/wgpu-hal 对齐
2. `clipboard-rs` 评估换 `arboard`（更活跃）或薄封装 Win32 API
3. `rfd` 0.16+ 可能减少 windows 重复

---

## Tier B — 战略性保留 / 局部 fork

### 10. `glyphon` + `cosmic-text` 文本栈

| 膨胀 | 重复 | 冗余 | 难度 | **优先级** |
|:----:|:----:|:----:|:----:|:----------:|
| 5 | 2 | 2 | 2 | **3.10** |

**调用面**：~30 个文件、100+ 处 `glyphon::`；`prepare_text.rs` 3800+ 行深度定制。

**建议**：**不要手撕 shaping**，应：
1. 将 `crates/glyphon` 接入 workspace（`[patch.crates-io]`）
2. 在 fork 内裁剪 Msdf/Bake（注释已说只用 Coverage）
3. 继续在 fork 外围做 COLR/emoji（现有模式正确）

---

### 11. `resvg` 0.45（`svg` feature）

| 膨胀 | 重复 | 冗余 | 难度 | **优先级** |
|:----:|:----:|:----:|:----:|:----------:|
| 4 | 4 | 3 | 3 | **3.55** |

**调用面**：`media/svg_raster.rs`、`renderer/emoji_svg.rs`（6 处 `resvg::`）。

**建议**：保持 optional。若 Twemoji SVG 可降级 PNG，可把 `emoji-svg` 与 `svg` 解耦，让默认 gallery 不拉 `tiny-skia` + `usvg` + `rustybuzz`。

---

### 12. `rfd` 0.15

| 膨胀 | 重复 | 冗余 | 难度 | **优先级** |
|:----:|:----:|:----:|:----:|:----------:|
| 2 | 2 | 3 | 3 | **2.55** |

**调用面**：仅 `utils/file_dialog.rs`（5 处）。

**建议**：feature `file-dialog` 可选化；或 Windows 上用 ` IFileDialog` 薄封装（工作量大，非急）。

---

### 13. `clipboard-rs` 0.3

| 膨胀 | 重复 | 冗余 | 难度 | **优先级** |
|:----:|:----:|:----:|:----:|:----------:|
| 2 | 3 | 3 | 3 | **2.70** |

**调用面**：`utils/clipboard.rs` + `input_controller.rs`（纯文本剪贴板）。

**建议**：评估 `arboard`；文本场景可 Win32 `OpenClipboard` 薄封装（~80 行）。

---

## Tier C — 不建议动

| 依赖 | 原因 |
|------|------|
| `wgpu` 30 | GPU 核心，~200 crate 属正常；backend feature 已做得好 |
| `winit` 0.30 | 窗口/事件唯一选择 |
| `taffy` 0.13 | 布局引擎，50+ 处深度集成 |
| `accesskit` + `accesskit_winit` | 无障碍合规，平台差异大 |
| `bytemuck` | wgpu 生态标准，体积小 |
| `unicode-segmentation` / `unicode-linebreak` | emoji/BiDi 折行必需 |
| `log` / `tracing` | 已 optional，合理 |

---

## 重复版本速查表

| Crate | 版本 A | 版本 B | 根因 | 修复手段 |
|-------|--------|--------|------|----------|
| `bitflags` | 1.3 | 2.11 | resvg→png 0.17 | 升 resvg / 关 svg |
| `png` | 0.17 | 0.18 | resvg vs image | 同上 + image 瘦身 |
| `gif` | 0.13 | 0.14 | resvg vs rui/image | workspace 统一 |
| `font-types` | 0.11 | 0.12 | harfrust vs swash | fork glyphon / 等 cosmic-text |
| `skrifa` | 0.40 | 0.44 | 同上 | 同上 |
| `read-fonts` | 0.37 | 0.41 | 同上 | 同上 |
| `hashbrown` | 0.16 | 0.17 | accesskit vs wgpu | 等上游，影响小 |
| `rustc-hash` | 1.1 | 2.1 | naga vs cosmic-text | 等上游 |
| `smol_str` | 0.2 | 0.3 | winit vs cosmic-text | 等上游 |
| `syn` | 2.0 | 3.0 | rav1e vs thiserror | **砍掉 rav1e 即可消** |
| `windows` | 0.59 | 0.62 | clipboard vs accesskit | 换剪贴板 / 升 windows-sys |
| `windows-sys` | 0.52–0.61 | 四版本 | winit/rui/rfd/notify | 统一到 0.62 |
| `zune-jpeg` | 0.4 | 0.5 | resvg vs image | 对齐 resvg / 缩 image |

---

## 推荐执行顺序（4 个 Sprint）

```
Sprint 1（1–2 天，低风险）
├── 删 fontdb 直接依赖
├── image feature 拆分，默认只留 png/jpeg/webp
├── workspace 统一 gif = "0.14"
└── 验证 gallery / calculator 测试

Sprint 2（3–5 天，中风险）
├── serde 迁入 hot-reload feature
├── pollster → 内联 block_on
├── palette → 内联 Oklab
└── cargo bloat 对比前后二进制

Sprint 3（1–2 周）
├── crates/glyphon 接入 workspace patch
├── 裁剪 glyphon Msdf/Bake features
└── 评估 swash 直接依赖能否移除

Sprint 4（按需）
├── image → png/jpeg-decoder/webp 子集替换
├── clipboard-rs → arboard 或薄封装
├── windows-sys 统一到 0.62
└── resvg / emoji-svg 解耦
```

---

## 量化验证命令

每个 Sprint 后用这些对比：

```powershell
# 包数量
(cargo tree -p gallery --prefix none).Count

# 重复版本
cargo tree --duplicates

# 二进制体积
(Get-Item target\release\gallery.exe).Length

# 按 crate 体积（需 cargo install cargo-bloat）
cargo bloat --release -p gallery -n 30
```

---

**结论**：当前最值得动的是 **`image` 格式瘦身**（单项可能减掉整个 `rav1e` 链）和 **删除冗余直接依赖**（`fontdb`）；文本/GPU/布局栈继续 fork 而非手撕。若你愿意，我可以直接从 Sprint 1 的 `image` feature 拆分 + 删 `fontdb` 开始改代码。