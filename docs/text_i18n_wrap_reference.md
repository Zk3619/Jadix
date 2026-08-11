# 多语言 / Emoji 测量与换行：参考框架对照

> 背景：RUI gallery「Text Compatibility」在 **resize** 时卡顿；此前为防溢出引入 `glyphon_fit_soft_wrap`，又为保帧率在 fast 路径跳过 fit，导致溢出回归。  
> 本文对照 `Reference/` 下六套实现，提炼可落地的架构启示（不改代码，仅分析）。

**对照对象**

| 项目 | 路径 | 文本栈 |
|------|------|--------|
| pretext | `Reference/pretext-main` | Canvas `measureText` + 自研 line-break（TS） |
| egui | `Reference/egui-main` | epaint 自研 layout（skrifa）+ GalleyCache |
| iced | `Reference/iced-master` | **cosmic-text** + cryoglyph/glyphon（与 RUI 同栈） |
| gpui | `Reference/gpui` | 平台文本 / wgpu 上 cosmic-text；**自研 wrap** |
| xilem | `Reference/xilem-main` | **parley**（masonry Label/Prose） |
| EUI-NEO | `Reference/EUI-NEO-main` | FreeType + HarfBuzz |

---

## 1. 一句话结论

| 框架 | 核心策略 | 对 RUI resize 的启示 |
|------|----------|----------------------|
| **pretext** | `prepare` 一次测宽，`layout` 纯算术算行高 | **高度估算不要每帧 glyphon 整形** |
| **iced** | `Difference::Bounds` → 只 `set_size` + `shape_until_scroll` | **改宽≠全量 reshape**；同栈最值得抄 |
| **gpui** | 先 shape 成一行，再算 `WrapBoundary` | **整形与换行解耦**；宽度变只重算边界 |
| **xilem** | 按 `max_advance` 兼容复用 Layout | **新宽度若仍装得下旧 layout，直接复用** |
| **egui** | `GalleyCache` 按 job 哈希；代际 GC | 缓存要带 wrap，但 hit 要稳；段落级分缓存 |
| **EUI-NEO** | 尺寸 LRU 缓存（含 wrap/maxWidth） | 简单有效，但每 miss 仍整段测 |

RUI 当前痛点可以概括为：**把「墨迹 fit」和「宽度变化」绑在同一热路径上，且缺少 iced 式 Bounds 差分**。

---

## 2. 各框架怎么做

### 2.1 pretext — prepare / layout 两阶段（resize 黄金路径）

**来源**：`src/layout.ts`、`src/measurement.ts`、`src/line-break.ts`、`src/bidi.ts`、`README.md`

**架构**

```
prepare(text, font)  ──►  分段 + canvas 测每段宽度（缓存）
layout(prepared, maxWidth)  ──►  纯算术走线，算行数/高度
```

- 文档声称：resize 时约 **0.0002ms / 文本块**（相对 DOM 测高的 30ms+ 量级）。
- **i18n**：`Intl.Segmenter` 处理 CJK / Thai 等；标点粘连、尾部空白挂出等对齐 CSS。
- **Emoji**：canvas 与 DOM 宽度差做校正，**按 font 缓存一次**校正量（Chrome/Firefox macOS 常见膨胀）。
- **Bidi**：`bidi.ts` 只给 rich 路径提供 embedding level；**换行引擎不消费 bidi levels**（断行按逻辑顺序 + 段宽）。

**值得抄**

1. 布局高度 = 段宽算术；GPU/整形只在 paint。
2. Resize 热路径禁止「再测字形」。
3. Emoji 校正是测宽层的事，不是每帧 reshape。

**局限**：浏览器专用；不负责最终字形方向视觉（自定义绘制才用 bidi 元数据）。

---

### 2.2 iced — 与 RUI 同栈，差分最清晰

**来源**：`graphics/src/text/paragraph.rs`、`graphics/src/text/cache.rs`、`core/src/text.rs`

**关键类型**

- `Paragraph`：持有 `cosmic_text::Buffer`（`Arc`）+ wrapping / bounds / version。
- `Difference::{ None, Bounds, Shape }`：

| 差分 | 含义 | 动作 |
|------|------|------|
| `None` | 内容/字体/wrap 策略都不变 | 复用 |
| `Bounds` | **仅包围盒变了** | `buffer.set_size` + `shape_until_scroll` |
| `Shape` | 内容/字号/字体/shaping/wrapping… | 重建 Buffer / `set_text` |

`Paragraph::resize`（Bounds 路径）核心：

```rust
paragraph.buffer.set_size(Some(new_w * hint), Some(new_h * hint));
paragraph.buffer.shape_until_scroll(font_system.raw(), false);
```

**不做**：重新 `set_text`、多轮「收紧 wrap 再 shape」的 fit 循环。

**Widget 常驻路径**（比绘制侧 `Cache` 更重要）：

```
Plain::update / compare
  None   → 不动
  Bounds → paragraph.resize(new_bounds)   // 只 set_size + shape_until_scroll
  Shape  → with_text 整段重建
draw → Text::Paragraph { Weak }            // 不 reshape，共享同一 Arc<Buffer>
```

**缓存**：`Cache::allocate` 按 content/size/line_height/font/bounds/shaping/align 哈希；并对「实际 min_bounds」建 **alias**，避免等价包围盒重复整形。每帧 `trim` 丢掉未用条目。注意：Cached Key 当前 **未把 wrapping/ellipsis 打进 hash**（潜在撞键，RUI 勿抄此疏漏）。

**Emoji / 多语言**：交给 cosmic-text Advanced shaping + 系统彩色字体（`SwashContent::Color` / cryoglyph color mode）；无 Twemoji、无 CustomGlyph。`Shaping::Auto`：`is_ascii()` → Basic，否则 Advanced。

**值得抄（对 RUI 优先级最高）**

1. **宽度变化 = Bounds，不是 Shape。**  
   RUI 现在很多路径把 leaf 宽变化当成「必须 `glyphon_fit` + 全量 miss shaped_cache」。
2. **同 Buffer 上 `set_size` 重排**，保留已 shape 的 run 结构由 cosmic 增量处理。
3. **measure 拥有 Buffer，paint 只持 Weak/Arc** —— 避免绘制树克隆或每帧重建。
4. Cache alias：fit 后的有效宽可映射回同一 entry；Key 必须含 wrap/ellipsis。

**局限**：仍受 cosmic 双向软换行墨迹偶发超宽影响；iced 用 buffer 宽约束，不额外 chase ink。RTL 显式对齐时可能二次 reshape。

---

### 2.3 gpui — 整形一次，换行另算

**来源**：`gpui/src/text_system/{line.rs,line_layout.rs,line_wrapper.rs}`、`gpui_wgpu/src/cosmic_text_system.rs`

**架构**

```
shape_line / shape_text  ──►  LineLayout（未 wrap 的 shaped runs）
                    └─►  WrapBoundary[]（按 wrap_width 切）
                    └─►  WrappedLineLayout
```

- cosmic 路径注释明确：`None` wrap — **「We do our own wrapping」**。
- `LineWrapper`：按字符宽缓存（ASCII 数组 + HashMap），在边界候选上找断点（编辑器截断 / `line_clamp`）。
- `LineLayout::compute_wrap_boundaries`：在**已 shape 的 glyph x** 上跑相近规则，产出折行点（与 `wrap_line` 逻辑部分重复）。
- `LineLayoutCache`：**双帧**（previous/current）；`shape_line_by_hash` **cache hit 时不 materialize 字符串**；subtree 可按 index range `reuse_layouts`。
- **Resize**：wrapped key 含 `wrap_width` 会 miss，但先查 `layout_line(..., wrap=None)` → **未换行 shaping 跨宽复用**，只重算 boundary（便宜）。
- Bidi：对 `Bidi_Class=B` 段落分隔符**先切开再 shape**；软换行仍是偏 LTR/CJK 的启发式，非完整 UAX#14。
- Emoji：按字体标 `is_emoji`（如 NotoColorEmoji / AppleColorEmoji），`paint_emoji` 走彩色 atlas；ZWJ 不拆簇。

**值得抄**

1. **Shape ≠ Wrap**：改宽只重算 `WrapBoundary`，不重跑整段 HarfBuzz/cosmic shape。
2. 两帧布局缓存 + content hash 探测 + miss 再 materialize。
3. 字符宽微缓存降低 wrap 探测成本。
4. Bidi 段落分隔先切再 shape（对 cosmic 一类 shaper 更稳）。

**局限**：自研 wrap 与 cosmic UAX#9 软换行语义可能不一致；RUI 若继续用 cosmic wrap，不能原样搬边界算法，但「解耦」思想可借鉴。

---

### 2.4 xilem / masonry — `max_advance` 兼容复用

**来源**：`masonry/src/widgets/label.rs`（parley `Layout`）

- 每个 Label 维护 **多个** `TextLayout` 缓存槽（含 `max_advance`、对齐、使用时间戳）。
- 复用条件（概念上）：
  - 新约束与旧 `max_advance` **兼容**；且
  - 旧 `layout.width()` 仍装得进新宽度（带 `EPSILON≈0.01`）。
- 换行 / 多语言 / emoji：交给 **parley**（非 cosmic）。

**值得抄**

1. 「略变窄但仍大于内容宽」→ **零重算**。
2. 多槽 LRU，避免只有单一 wrap 宽导致抖动。

---

### 2.5 egui — GalleyCache + 代际 GC

**来源**：`epaint/src/text/{fonts.rs,text_layout.rs,text_layout_types.rs}`

- 栈：**skrifa + harfrust + vello_cpu**（非 cosmic）；`LayoutJob` → shape → soft-wrap → **立即 tessellate** 进 `Galley`（measure 与 paint 同一产物）。
- `TextWrapping { max_width, max_rows, break_anywhere, overflow_character }`。
- `Fonts::layout_job` → `GalleyCache`：job 哈希含 wrap + `pixels_per_point`；**同帧命中 `Arc::clone`**。
- **入缓存前 `max_width = max_width.round()`**（整点），专门打断「报告宽 → 下帧 wrap 宽」振荡（egui #4927 一类问题）。
- `effective_wrap_width`：GUI 取整时用 `max_width + 0.5`，避免临界提前折行。
- `flush_cache`：只保留本代 `last_used`，再 `generation++`。
- 长文：可按 `\n` **分段缓存段落**再 `Galley::concat`，编辑局部时不全文重排。
- 流程是 **先整段 shape，再按 glyph 流 wrap**（非边 shape 边 wrap）。
- Emoji：默认字体栈挂 NotoEmoji / emoji-icon-font；**outline 单色**（无 COLR/CBDT 彩色管线）；cluster 补零宽 continuation 保 caret 映射。
- Bidi：**无完整 UBA**；控制符常被吞成零宽；注释写明待 script-aware splitting。

**值得抄**

1. **Wrap 宽整型量化** + `+0.5` 容差，抗 resize 振荡（与 RUI 1px shaped_cache 桶可对照）。
2. 代际 GC（比无限 HashMap 更稳）。
3. 段落级缓存降编辑/局部更新成本。
4. 声明式 `LayoutJob`（可 Hash）→ 不可变 `Arc<Galley>`。

**局限**：无真 BiDi、无彩色 emoji；shape 时写 atlas，测量即可能光栅。RUI 应保留 cosmic BiDi/彩色能力，只抄缓存与 reflow 策略。

---

### 2.6 EUI-NEO — FreeType 测宽 LRU

**来源**：`core/render/text.cpp`

- `TextSizeCache`：key 含 font / size / weight / wrap / maxWidth / 文本。
- `FontStackCache`：按路径+字号缓存 face 栈；识别 emoji/彩色字体。
- Wrap：按段落测宽，超 `maxWidth` 则累加行高（相对朴素）。
- 注释强调：长可编辑文本会频繁 measure，因此必须缓存。

**值得抄**：测宽结果 LRU；emoji 字体显式进栈。  
**局限**：无 iced 式 Bounds 差分；miss 仍整段测量。

---

## 3. 对照 RUI 现状

RUI 当前相关路径（摘要；**P0 已落地**）：

| 阶段 | 行为 | 问题 / 状态 |
|------|------|-------------|
| `measure_leaf` / `layout_tree` | 非 ASCII → `bidi_soft_wrap_height`：**算术**；FIT 命中才抬高 | **P1：布局不 compute fit** |
| `FIT_CACHE` | text+size+weight+lh+target；resize 用 8px 桶 | **由 paint spill 写入** |
| `prepare_text` | Exact / Bounds / Miss；wrap=缓存否则 leaf | spill → 一次 `glyphon_fit` + `set_size` |
| resize fast | Bounds：`set_size`；spill 才 fit | 禁止布局端抢先 fit |

---

## 4. 可借鉴优先级

### P0 — iced 式 Bounds 路径（已实施）

对每个 shaped 节点区分：

- **Shape miss**：文本/字号/字体/颜色变 → `set_text` + shape，wrap = `glyphon_fit`。  
- **Bounds miss**：仅 leaf 宽/高变 → **同一 Buffer** `set_size(fit.wrap_width)` + `shape_until_scroll`（无 `set_text`）。  
- **不变量**：可换行非 ASCII 在 FIT_CACHE 命中时用 `fit.wrap`；未命中则先 leaf，**仅当 paint 发现 ink spill** 才 `glyphon_fit`（P1）。

辅以 egui 式 **2px 量化**（fast）降低 Exact churn；fit 查找用 8px target 桶与 layout 对齐。

预期：resize 从「N × (fit×2~3 + 全量 reshape)」降为「N × FIT 查找 + 一次 set_size」。

### P1 — pretext 式高度：算术为主，fit 为辅（已实施，含正确性修正）

- 布局高度：ASCII / MinContent → 段宽算术；可换行非 ASCII → FIT_CACHE 命中用 `fit.line_count`。  
- **resize fast**：布局 miss **不** compute fit（只算术）；paint spill 也不 fit。  
- **松手 settle**：布局 miss 一次 fit；paint 水平/垂直 spill → fit + 必要时 relayout。  
- MinContent / `layout_w≤max_seg`：只用算术。

### P2 — xilem 式兼容复用 + egui 代际 GC（已实施）

- Bounds：若 `measured_text_w ≤ new_leaf_w` **且** `runs*lh ≤ leaf_h` → 跳过 `set_size`。  
- `FIT_CACHE`：`fit_cache_frame_tick` 代际 trim（软上限 1024，保留近几帧触及项）。  
- `shaped_cache`：仍按节点下标对齐，`truncate` 丢弃多余槽（无需独立代际表）。

### P3 — gpui 式解耦（中长期）

若 cosmic 软换行墨迹超宽长期无法根治：

- 选项 A：接受轻微 clip / leaf scissor（产品策略）。  
- 选项 B：自研 wrap 边界 + cosmic 只 shape（工程量大，需 bidi 测试矩阵）。

### 不建议

- 在 resize fast 路径「跳过 fit 并用满宽」——已验证会溢出。  
- 为 chase ink 在热路径做多轮收紧（破坏 UAX#9 视觉顺序 + 卡顿）。  
- 把 gallery 样例靠大量 isolate 掩盖引擎问题（样例可保留，但不能当唯一修复）。

---

## 5. 功能对照简表

| 能力 | pretext | egui | iced | gpui | xilem | EUI-NEO | RUI 今日 |
|------|---------|------|------|------|-------|---------|----------|
| 多语言测宽 | Segmenter + canvas | 自研 + 字体栈 | cosmic | 平台/cosmic | parley | FT/HB | cosmic + 段宽 |
| Bidi | 元数据，断行不用 | 有限 | cosmic UAX#9 | unicode_bidi + shape | parley | 依赖 HB | cosmic；样例用 FSI |
| Emoji | canvas 校正 | emoji 字体优先 | 系统回退 | emoji 字体标记 | parley | 彩色字体 | Twemoji 占位 + custom glyph |
| 软换行 | 自研算术 | 自研 | cosmic Buffer | **自研 WrapBoundary** | parley | 朴素行累加 | cosmic + fit 收紧 |
| Resize 策略 | **纯 layout 算术** | Galley 哈希重算/命中 | **Bounds→set_size** | wrap 边界重算 | **max_advance 复用** | 尺寸 LRU | **Bounds + xilem 兼容跳过；spill→fit** |
| 缓存 | 段宽 / emoji 校正 | Galley 代际 | Buffer + alias | 两帧 + hash | 多槽 Layout | TextSize LRU | PREPARE + FIT + shaped（shape_hash / key_hash） |

---

## 6. 与 RUI 文档的关系

- 总览对照：[`REFERENCE_COMPARISON.md`](./REFERENCE_COMPARISON.md)（文本栈一行表）  
- Emoji GPU：[`emoji_gpu_compat_matrix.md`](./emoji_gpu_compat_matrix.md)  
- 本文聚焦：**测量 / 换行 / resize 缓存架构**，指导后续「卡顿与溢出」双约束下的改造，而不替代组件级对照。

---

## 7. 建议的验证矩阵（改造后）

1. gallery Text Compatibility：拉窄/拉宽窗口，双向与 RTL+emoji **不溢出**、断行顺序可接受。  
2. 同页快速拖拽边框：感知流畅（目标：无「每像素 Advanced×N」）。  
3. 松手静止一帧：与拖拽中视觉差 ≤ 1 行高抖动（允许），无永久错乱。  
4. 回归：`gallery_bidi*`、`fast_mode_*`、emoji 软换行、textarea 多行高度。

---

*调研基于仓库内 `Reference/` 快照源码；版本随 vendor 树更新，实施前请再核对对应文件。*  
*补充细节来自并行对照：[调研 egui 文本换行](53aa80ba-1233-4422-a6d2-a16ae942b4dc)、[调研 iced 文本换行](18cfb8d5-5ab3-4ed3-a8a4-e740ec2342ef)、[调研 gpui 文本换行](0dedad5c-78ae-474d-a32e-39bbf8dcbc37)。*
