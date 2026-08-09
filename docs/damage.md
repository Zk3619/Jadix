# 细粒度 damage（WP-2.2）

对应 [ENGINEERING_IMPLEMENTATION_PLAN.md](ENGINEERING_IMPLEMENTATION_PLAN.md) **WP-2.2**（策略可测已签署）；GPU PartialLoad 实装见同文 **§何时做 GPU PartialLoad** 与计划 §7 **B8**。

## 术语

| 词 | 含义 |
|----|------|
| **脏区** | 本帧相对上一帧可能变化的屏幕矩形（逻辑 px `[x,y,w,h]`），经 `rects_union` 合并 |
| **逻辑 PartialLoad** | `frame_clear_policy` 判定：结构未变且脏区局部 → 策略上「可局部更新」（Gallery 计数） |
| **逻辑 FullClear** | 首帧 / 强制重绘 / 结构·布局·滚动 / 脏区铺满或越界 → 策略上必须整帧失效 |
| **GPU FullClear** | 对 swapchain（或 retain）`LoadOp::Clear` 后完整绘制（**当前默认 present**） |
| **GPU PartialLoad / retain** | 在可保留的离屏色缓冲上 `Load` + scissor，再 blit 到 swapchain（**默认关闭**；`RUI_PARTIAL_DAMAGE=1` 实验） |

## 与 Solid 对齐的模型

Solid 的细粒度更新改的是**节点属性**（`batch` + 绑定 / `createRenderEffect`），一次 flush 后由浏览器合成**完整**像素——不是「只重画 scissor 里一半 framebuffer」。

| 层 | Solid | RUI |
|----|--------|-----|
| 响应式 | `batch` + 属性绑定 | `bind_*` / `batch`：就地改 Element（[reactivity_and_hot_reload.md](reactivity_and_hot_reload.md)） |
| 脏区策略 | （浏览器内部） | `frame_clear_policy` + `damage_rect`：逻辑 Clear vs PartialLoad（可测计数） |
| 像素 | 合成器完整上色 | **默认 swapchain FullClear**；GPU PartialLoad 仅作后续优化 |

原则：**先 Solid 式改树 + 完整 present；能「少画」后再开 retain PartialLoad。**

## WP-2.2 已交付（策略可测）

- `app/damage.rs`：`rects_union` / `frame_clear_policy` / `accumulate_paint_damage` / `push_frame_paint_damage`
- 同宽 `bind_text`、hover / invalid 等 paint-only → 局部脏区，**不** `mark_full_redraw`
- 结构 / 布局 / 滚动偏移变化 → `mark_full_redraw`（逻辑 FullClear）
- 计数：`partial_load_frame_count` / `full_clear_frame_count` / `reset_frame_policy_counters`
- GPU present：**默认始终 FullClear 到 swapchain**（与引入 retain 前同成本）；`present_retain.*` 为 B8 预留

### 签署

| | |
|--|--|
| **状态** | ☑ **策略可测已签署**（2026-08-10，Kingdom / Win 10.0.26200） |
| **证明** | 库测 `cargo test -p rui --lib damage::`；Gallery **布局 / WP-2.2** 逻辑计数验收 |
| **非范围** | GPU PartialLoad 默认路径、draw-list 按脏区裁剪（→ **B8**） |

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

## 如何把 GPU PartialLoad / retain 做好（B8 设计约束）

要对齐 Solid「完整像素」且真正赢过 FullClear，须同时满足：

1. **retain = 合成后缓冲**  
   UI 只画进可 `Load` 的离屏纹理；swapchain 只收完整 blit/copy。禁止对 swapchain `Load`（FLIP_DISCARD / GLES）。

2. **真正少画**  
   PartialLoad 时按脏区**裁剪 draw-list**（实例 / 文本 / 阴影只提交与脏区相交者），再 scissor。  
   若仍全量 draw + 全屏 blit，必然比 surface FullClear 慢（Mark invalid / 表单连点已证实）。

3. **脏区语义正确**  
   - paint-only：局部脏区  
   - 布局变宽/变窄、滚动、切页、结构：整视口失效  
   - 文案收缩须覆盖**旧包围盒**（防残影）  
   - scissor 严格夹在 target 内  

4. **失效干净**  
   滚动 / Layout / Structure 后禁止用旧 retain 打补丁；FullClear 后重新 warm 或 copy 同步。

5. **验收**  
   Mark invalid 连点、悬停后滚动、切页：无卡顿、无半帧；同宽 +1 在开启 GPU 路径时才验真正的 `LoadOp::Load`。

实验开关（非正式默认）：

```text
RUI_PARTIAL_DAMAGE=1
```

## 何时做 GPU PartialLoad（规划）

| 时机 | 建议 |
|------|------|
| **现在（WP-2.2）** | ☑ 策略可测 + 默认 FullClear。不阻塞 Wave 2 / Beta。 |
| **值得开工（B8）** | Wave 2 其余项与 golden（WP-2.6）稳定之后；或单独性能 WP。前置：**实例/文本按脏区剔除**已落地，或 profiling 证明 FullClear 填色是瓶颈（高 DPI / 大窗 / 高频 hover）。 |
| **不要抢做** | 布局/文本/滚动正确性未稳；或没有裁剪却开 retain+blit。 |

跟踪项：[ENGINEERING_IMPLEMENTATION_PLAN.md](ENGINEERING_IMPLEMENTATION_PLAN.md) §7 **B8**。
