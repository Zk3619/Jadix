# 细粒度 damage（WP-2.2）

对应 [ENGINEERING_IMPLEMENTATION_PLAN.md](../archive/ENGINEERING_IMPLEMENTATION_PLAN.md) **WP-2.2**（策略可测已签署）；GPU PartialLoad / 树级 collect 增量见同文 **§何时做 GPU PartialLoad** 与计划 §7 **B8**。

**能力分级（2026-08-24 口径）**

| 能力 | 状态 | 说明 |
|------|------|------|
| **逻辑 damage 策略**（`frame_clear_policy`、局部脏区、计数、Gallery） | **正式默认**（WP-2.2 已签） | 真实产品契约，非实验 |
| **GPU PartialLoad / retain**（`RUI_PARTIAL_DAMAGE=1`） | **实验** | 升默认须 B8 全套；勿单独开 |
| **树级 collect 剪枝**（脏子树 patch DL、跳过干净兄弟） | **规划（B8）** | 见下「四层模型」；≠ 替代脏区 |

## 术语

| 词 | 含义 |
|----|------|
| **脏区** | 本帧相对上一帧可能变化的屏幕矩形（逻辑 px `[x,y,w,h]`），经 `rects_union` 合并 |
| **逻辑 PartialLoad** | `frame_clear_policy` 判定：结构未变且脏区局部 → 策略上「可局部更新」（Gallery 计数） |
| **逻辑 FullClear** | 首帧 / 强制重绘 / 结构·布局·滚动 / 脏区铺满或越界 → 策略上必须整帧失效 |
| **GPU FullClear** | 对 swapchain（或 retain）`LoadOp::Clear` 后完整绘制（**当前默认 present**） |
| **GPU PartialLoad / retain** | 在可保留的离屏色缓冲上 `Load` + scissor，再 blit 到 swapchain（**默认关闭**；`RUI_PARTIAL_DAMAGE=1` 实验） |
| **树级失效剪枝** | 只对脏 Element/Taffy 节点及必要祖先·子孙做 layout/collect；跳过父下干净兄弟与无关分支（布局侧已有 `mark_dirty`；绘制侧见 B8） |

## 四层模型（勿混层）

| 层 | 目标 | 现状 | 与「只重绘受影响组件」 |
|----|------|------|------------------------|
| **1. 响应式** | 只改受影响 Element 属性 | ✅ Solid 式 `bind_*` / `batch` | ✅ 已是此层 |
| **2. 布局** | 脏节点 + 上溢升父；干净兄弟不重算 | ✅ `mark_dirty`；Taffy 脏缓存；live 仍从 app root compute（见 [layout_subtree.md](layout_subtree.md)） | ✅ 布局语义已对齐；子树 root live 未开 |
| **3. Collect / display list** | 只重建脏子树 draw ops；跳过干净兄弟 | ❌ 主路径仍整树/可见带 collect；`scroll_only` band cull、Canvas 叶 dirty 为子集 | → **B8 fingerprint DL** |
| **4. Present** | 屏上只写变化像素 | 默认 GPU FullClear；逻辑 PartialLoad 可测；GPU retain 实验 | 需 **脏区 ∪ 层缓存**，不能单靠树身份 |

**决策（ADR 摘要 · 2026-08-24）**

1. **不以 Solid「组件重绘」替代脏区。** Solid 改的是 DOM；像素由浏览器合成。RUI 自绘，几何脏区是 present/retain 的正确性输入。
2. **不以「只重绘脏子树、跳过干净兄弟」替代脏区。** 树剪枝省的是 CPU collect；默认 FullClear 下跳过兄弟 = 像素消失。要「不重画仍看得见」须跨帧 display list 或 layer cache，再叠加 damage/scissor。
3. **理想增量路径（B8）三者叠加，非二选一：**  
   `脏节点集 →（可选）patch 跨帧 DL → union damage_rect → retain Load+scissor / viewport layer blit`
4. **滚动 / Layout / Structure** 仍强制逻辑 + 默认 GPU FullClear（有意）；滚动少画须 layer cache，不能只开 PartialLoad。

## 与 Solid 对齐的模型

Solid 的细粒度更新改的是**节点属性**（`batch` + 绑定 / `createRenderEffect`），一次 flush 后由浏览器合成**完整**像素——不是「只重画 scissor 里一半 framebuffer」，也不是应用层「按组件身份跳过兄弟像素」。

| 层 | Solid | RUI |
|----|--------|-----|
| 响应式 | `batch` + 属性绑定 | `bind_*` / `batch`：就地改 Element（[reactivity_and_hot_reload.md](../ops/reactivity_and_hot_reload.md)） |
| 脏区策略 | （浏览器内部） | `frame_clear_policy` + `damage_rect`：逻辑 Clear vs PartialLoad（可测计数） |
| 像素 | 合成器完整上色 | **默认 swapchain FullClear**；GPU PartialLoad 仅作后续优化 |

原则：**先 Solid 式改树 + 完整 present；能「少画」后再开 retain PartialLoad +（B8）树级 DL 剪枝。**

## WP-2.2 已交付（策略可测 · 正式能力）

- `app/damage.rs`：`rects_union` / `frame_clear_policy` / `accumulate_paint_damage` / `push_frame_paint_damage`
- 同宽 `bind_text`、hover / invalid 等 paint-only → 局部脏区，**不** `mark_full_redraw`
- 结构 / 布局 / 滚动偏移变化 → `mark_full_redraw`（逻辑 FullClear）
- 计数：`partial_load_frame_count` / `full_clear_frame_count` / `reset_frame_policy_counters`
- GPU present：**默认始终 FullClear 到 swapchain**（与引入 retain 前同成本）；`present_retain.*` 为 B8 预留

### 签署

| | |
|--|--|
| **状态** | ☑ **策略可测已签署**（2026-08-10，Kingdom / Win 10.0.26200）；**逻辑策略 = 正式默认**（2026-08-24 口径澄清） |
| **证明** | 库测 `cargo test -p rui --lib damage::`；Gallery **布局 / WP-2.2** 逻辑计数验收 |
| **非范围** | GPU PartialLoad 默认路径、跨帧 display list、树级 collect 剪枝、draw-list 按脏区裁剪（→ **B8**） |

## 逻辑策略（计数 / 验收）

- 局部 paint（同宽 `bind_text`、hover、invalid 边框色）→ 逻辑 `PartialLoad`
- 结构 / 布局 / 滚动 / 脏区铺满或越界 → 逻辑 `FullClear`
- 物理 scissor 若启用 GPU 路径：必须落在 render target 内（越界回退 FullClear）

## Gallery

```powershell
cargo run -p gallery
```

侧栏 **布局** → **WP-2.2) 细粒度 damage**：

1. **重置** → 连点 **同宽 +1** → **刷新快照**：`ΔPartialLoad`↑（逻辑计数；默认 GPU 仍 FullClear）
2. **切换结构** → 刷新：`ΔFullClear`↑
3. 表单页快速 **Mark invalid**：应无卡顿、无半帧；勿开 `RUI_PARTIAL_DAMAGE` 做该验收

## 自动化

```powershell
cargo test -p rui --lib damage::
```

## 滚动与 FullClear（WP-P 审计 · 2026-08-20）

滚动偏移变化时，`state.rs` 比较 `scroll_offset_fingerprint`，不等则 **`mark_full_redraw`**。这是**有意设计**，不是遗漏：

- 若滚动帧仍用 hover / `bind_text` 遗留的局部 scissor PartialLoad，只有脏区内像素会应用新 offset，其余区域保持旧帧 → Gallery 可复现「滚一下再跳变」 corruption。
- 因此 WP-2.2 策略：**滚动 = 逻辑 FullClear + 默认 GPU FullClear**（见 [`size_and_memory.md`](../ops/size_and_memory.md) §7、[AUTHORITY.md](../AUTHORITY.md) §5.6 WP-P 架构审计）。

要让滚动也「少画」，须 **B8 全套**（**≥ B3 backdrop-blur 离屏管线**）：scroll viewport **layer cache** 或 viewport union dirty + **fingerprint display list / draw-list 按 clip 裁剪** + retain 合成——不能单独开 `RUI_PARTIAL_DAMAGE=1`。

## 如何把 GPU PartialLoad / retain 做好（B8 设计约束）

要对齐 Solid「完整像素」且真正赢过 FullClear，须同时满足：

1. **retain = 合成后缓冲**  
   UI 只画进可 `Load` 的离屏纹理；swapchain 只收完整 blit/copy。禁止对 swapchain `Load`（FLIP_DISCARD / GLES）。

2. **真正少画（几何 + 树）**  
   - PartialLoad 时按脏区**裁剪 draw-list**（实例 / 文本 / 阴影只提交与脏区相交者），再 scissor。  
   - **指纹跨帧 display list**：paint-only 只 patch 脏节点段；父下干净兄弟**复用**上帧 ops（树级剪枝）。无跨帧 DL 时「跳过兄弟 collect」在 FullClear 下会丢像素。  
   - 若仍全量 draw + 全屏 blit，必然比 surface FullClear 慢（Mark invalid / 表单连点已证实）。

3. **脏区语义正确**  
   - paint-only：局部脏区（含文案收缩的**旧包围盒**）  
   - 布局变宽/变窄、滚动、切页、结构：整视口失效（或整 viewport layer）  
   - 重叠 / z-order：脏子盖住干净兄弟时，须重画下层或依赖 layer；纯树标记不够  
   - Portal / absolute / overlay：失效作用域 ≠ 简单父子链（见 `state.rs` portal-local LayoutChanged）  
   - scissor 严格夹在 target 内  

4. **失效干净**  
   滚动 / Layout / Structure 后禁止用旧 retain 打补丁；FullClear 后重新 warm 或 copy 同步。

5. **验收**  
   Mark invalid 连点、悬停后滚动、切页：无卡顿、无半帧；同宽 +1 在开启 GPU 路径时才验真正的 `LoadOp::Load`；开启树级 DL 后干净兄弟像素不丢、无残影。

实验开关（非正式默认）：

```text
RUI_PARTIAL_DAMAGE=1
```

## 何时做 GPU PartialLoad / 树级 DL（规划）

| 时机 | 建议 |
|------|------|
| **现在（WP-2.2）** | ☑ **逻辑策略正式默认** + GPU FullClear。不阻塞 Wave 2 / Beta。 |
| **值得开工（B8）** | **W4+ / Beta 后**；不早于 **B3** backdrop-blur 离屏缓存。前置：指纹 DL 或按脏区裁剪 draw-list；或 profiling 证 FullClear 填色为瓶颈（本机核显 scroll 已观测 fill，但 **Wave P profile CPU p99 已过关**，非阻塞项）。 |
| **升为 GPU 正式默认** | 仅当 B8 验收全过（retain + 裁剪/DL + 滚动 layer 或等价 FullClear 安全路径）；此前保持实验开关。 |
| **不要抢做** | 用「只重绘脏组件」替代脏区；无跨帧 DL/层却跳过兄弟 collect；无裁剪却开 retain+blit。 |

跟踪项：[ENGINEERING_IMPLEMENTATION_PLAN.md](../archive/ENGINEERING_IMPLEMENTATION_PLAN.md) §7 **B8**。
