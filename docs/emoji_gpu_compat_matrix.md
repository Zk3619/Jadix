# 彩色 Emoji 与无 GPU 兼容回归清单

## 1. 核心文本样例

在每个平台都验证以下文本（输入框 + 普通 Text 节点）：

- `Hello 😀 😁 😭`
- `Family: 👨‍👩‍👧‍👦`
- `SkinTone: 👍🏽`
- `Flag: 🇨🇳 🇺🇸`
- `Keycap: 1️⃣ 2️⃣ 9️⃣`
- `Mixed: Rust 👩‍💻 + 中文 + عربي`

验收项：

- 彩色显示（非黑白轮廓）
- 无裁剪（尤其是下探笔画）
- 换行后无错位
- 输入态光标与文本命中区域正常

## 2. GPU 后端矩阵

### Windows

- DX12（默认）
- DX11（驱动降级）
- 软件适配器（`RUI_FORCE_SOFTWARE=1`）

### Linux

- Vulkan
- OpenGL
- 软件光栅（llvmpipe）

### macOS

- Metal

## 3. 关键观测指标

- `prepare_text_frame` 每帧耗时（均值/95 分位）
- emoji 缓存条目数（`emoji_cache.entries.len()`）
- 文本缓存命中率（`shaped_cache` 命中比例）
- 软件适配器下 FPS 与输入延迟

## 4. 失败分级

- P0: 文本不可见、崩溃、输入失效
- P1: Emoji 裁剪、严重错位、回退为方块
- P2: 轻微基线偏移、滚动时偶发闪烁

## 5. 发布前最小通过标准

- 三平台至少各 1 台设备通过核心样例
- Windows 软件适配器可启动并完成输入编辑
- 无 P0/P1 缺陷

## 6. Windows 实时输入专项（本期新增）

### 6.1 执行命令

- 默认 GPU 路径：
  - `cargo run -p gallery`
- 软件适配器 + CPU 文本混合：
  - `RUI_FORCE_SOFTWARE=1 cargo run -p gallery`
- 可选文本优先保底（仅用于故障排查）：
  - `RUI_FORCE_SOFTWARE=1 RUI_CPU_TEXT_HYBRID_TEXT_ONLY=1 cargo run -p gallery`

### 6.2 调试开关

- Emoji 基线调节：
  - `RUI_EMOJI_BASELINE_FACTOR=0.12`
- Emoji 调试可视化（边框/基线）：
  - `RUI_EMOJI_DEBUG=1`

### 6.3 实时输入用例

在 `gallery` 的 Emoji Regression 区块，逐项验证：

- 逐字输入：`😀 😁 😂`
- 候选上屏：`👨‍👩‍👧‍👦`、`🧑‍💻`
- 粘贴多串：`👍🏽 🇨🇳 🇺🇸 1️⃣`
- 编辑行为：退格删除、区间替换、光标移动后插入

验收：

- 输入后当帧或下一帧可见
- 无底部裁剪、无明显基线跳动
- `emoji_cache` 日志持续输出且命中率随重复输入上升

### 6.4 日志指标采集

建议保存运行日志并提取以下关键行：

- `emoji_cache stats: ...`
- `cpu_text_hybrid stats: ...`
- `render profile(last ...): ... text=...`

用于回归比较的最小字段：

- `lookups / hits / hit_rate`
- `uploads / upload_bytes`
- `avg_prepare_ms / avg_text_nodes`

## 7. 双模式验收矩阵

### 7.1 默认模式（稳定发布）

- 环境：不设置 `RUI_EMOJI_CUSTOM_GLYPH`
- 目标：行为与历史版本一致，无彩色残影、无控件污染
- 关键检查：
  - 输入/滚动/checkbox/radio/switch 视觉不被污染
  - emoji 日志应显示 `mode=disabled`

### 7.2 灰度模式（真实 Emoji 图源）

- 环境：`RUI_EMOJI_CUSTOM_GLYPH=1`
- 图源策略：Twemoji 本地资源优先，缺失时字体光栅回退
- 关键检查：
  - Emoji 可见且无背景残影
  - 命中率随重复输入上升
  - 关闭开关后可立即回退到稳定模式

## 8. 发布闸门与回退

- 发布闸门：
  - 默认模式必须通过全部 P0/P1 验收
  - 灰度模式至少完成 Windows 主机 + 软件适配器回归
- 回退策略：
  - 发现异常时立刻关闭 `RUI_EMOJI_CUSTOM_GLYPH`
  - 保留 `RUI_CPU_TEXT_HYBRID_TEXT_ONLY=1` 作为应急文本可用兜底
- 日志判断：
  - 仅灰度模式允许出现 `mode=gray`
  - 正式发布默认应保持 `mode=disabled`

## 9. 本地 Twemoji 目录维护

### 9.1 同步全量资源（PNG / SVG）

默认从 **[jdecked/twemoji](https://github.com/jdecked/twemoji)** 同一 tag 拉取，避免 PNG 与 SVG 跨仓库版本漂移；默认版本 `v16.0.1`（可在脚本参数中覆盖）。

**推荐：一次下载、一次解压，同时写入 PNG 与 SVG**

- 源码 zip 内同时包含 `assets/72x72` 与 `assets/svg`，无需跑两遍下载。
- 执行（`-AssetKind all` 可省略，即为默认）：
  - `powershell -ExecutionPolicy Bypass -File scripts/sync_twemoji.ps1`
  - 或显式：`... -File scripts/sync_twemoji.ps1 -AssetKind all`
- 输出目录：`assets/twemoji/72x72`、`assets/twemoji/svg`（**不要**与 `-AssetKind all` 同时使用 `-TargetDir`）。
- 预期输出示例：
  - `Done. Synced N png and M svg files (one download / one extract).`

**仅同步一种（仍只下载解压一次包，只复制对应子目录）**

- 仅 PNG：`powershell -ExecutionPolicy Bypass -File scripts/sync_twemoji.ps1 -AssetKind png`
  - 可选：`-TargetDir assets/twemoji/72x72`
- 仅 SVG：`powershell -ExecutionPolicy Bypass -File scripts/sync_twemoji.ps1 -AssetKind svg`
  - 可选：`-TargetDir assets/twemoji/svg`
- 预期输出示例：`Done. Synced N png files.` 或 `Done. Synced N svg files.`

**兼容旧源（twitter 归档包，仅当需要与历史 PNG 完全一致时）**

- 示例：`powershell -ExecutionPolicy Bypass -File scripts/sync_twemoji.ps1 -AssetKind png -Repo twitter/twemoji -Version v14.0.2`

临时文件目录：仓库根目录 `.tmp/`（脚本结束后会清理 zip 与解压目录）。

### 9.2 文件命名与样例校验

**PNG**

- 执行：
  - `powershell -ExecutionPolicy Bypass -File scripts/verify_twemoji_names.ps1`
- 校验项：
  - 文件名符合 `lowercase-hex(-hex).png`
  - 关键样例（家庭/肤色/旗帜/keycap/职业）存在
- 预期输出：
  - `Twemoji naming verification passed.`

**SVG**

- 执行：
  - `powershell -ExecutionPolicy Bypass -File scripts/verify_twemoji_svg_names.ps1`
- 校验项：同上，扩展名为 `.svg`
- 预期输出：
  - `Twemoji SVG naming verification passed.`

### 9.3 Emoji 缓存环境变量（运维）

- `RUI_EMOJI_RASTER_CACHE_MAX`：光栅条目上限（默认 `2048`），超出按 LRU 淘汰。
- `RUI_EMOJI_NODE_CACHE_MAX`：按文本节点缓存的 emoji 自定义字形条目上限（默认 `128`）。

### 9.4 灰度回归记录

- 启动灰度：
  - `set RUI_EMOJI_CUSTOM_GLYPH=1 && cargo run`（在 `examples/gallery`）
- 必看日志：
  - `emoji_cache stats: mode=gray ...`
- 注意：
  - 若未触发 emoji 输入或未滚动到相关区域，`lookups` 可能暂时为 `0`，属于可接受现象。

## 10. SVG 灰度实验

### 10.1 开关与优先级

- 仅当以下两个开关同时开启时启用 SVG：
  - `RUI_EMOJI_CUSTOM_GLYPH=1`
  - `RUI_EMOJI_SVG=1`
- 资源优先级：
  - SVG
  - PNG
  - 字体回退

### 10.2 目录与命令

- 默认 SVG 目录：`assets/twemoji/svg`（需先按 §9.1 同步 SVG，或指向自定义目录）
- 可用环境变量覆盖：`RUI_TWEMOJI_SVG_DIR`
- 同步本地 SVG：见 §9.1（推荐默认 `all` 一次拉齐 PNG+SVG；仅需 SVG 时用 `-AssetKind svg`）；校验命名：§9.2 `verify_twemoji_svg_names.ps1`
- 启动示例（在 `examples/gallery`）：
  - `set RUI_EMOJI_CUSTOM_GLYPH=1 && set RUI_EMOJI_SVG=1 && cargo run`

### 10.3 指标与降级阈值

- 新增指标：
  - `svg_attempts`
  - `svg_hits`
  - `svg_raster_ms`
  - `svg_fallback_to_png`
- 自动降级：
  - 当平均 `svg_raster_ms` 超过阈值时停止走 SVG，改走 PNG
  - 阈值可配置：`RUI_EMOJI_SVG_RASTER_MS_THRESHOLD`（默认 8ms）

### 10.4 光栅缓存（按 grapheme + 字号）

- 实现：`EmojiGlyphCache.raster_entries`，键为 **emoji 序列字符串** + **`pixel_size`（逻辑字号取整后的像素高度）**。
- 作用：同一 grapheme、同一字号在**不同文本节点**或**同节点内多次出现**时，复用已解码/已栅格化的 RGBA，避免重复读盘与 SVG 光栅。
- 与节点级 `EmojiCacheKey` 的关系：节点键仍缓存整段布局下的 `CustomGlyph` 与对应贴图；光栅缓存是更细粒度的跨节点去重层。
