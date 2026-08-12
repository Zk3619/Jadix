# RUI 细粒度响应式完善计划（R-WP）

> **冻结展开（2026-08-12）**：勾选、排期以 [`AUTHORITY.md`](AUTHORITY.md) **WP-2.4 / 2.5 / 2.7 / WP-R.*** 为**唯一执行源**。本文仅保留 R-WP 验收矩阵与加深细则；旧编号对照见 AUTHORITY §9。**禁止**在此改勾选充进度。  
> **原地位**：把 [`REACTIVITY.md`](REACTIVITY.md) 的开放缺口落成 R-WP。行为状态机下沉归组件域，不把开合塞进反应性包。  
> **前提**：ADR-1 已锁；主路径 API 已齐；**优先硬化，禁止为对齐 Leptos/Floem 扩表面**。  
> **日期**：2026-08-12  

交叉：[`AUTHORITY.md`](AUTHORITY.md) · [`REACTIVITY.md`](REACTIVITY.md) · [`reactivity_and_hot_reload.md`](reactivity_and_hot_reload.md) · [`element_identity.md`](element_identity.md)

---

## 0. 目标与原则

**总目标**：反应性从「Alpha 可用」升到「长会话无泄漏、更新路径可预测、Store/异步契约可写入 SemVer」。

| 原则 | 含义 |
|------|------|
| **硬化 > 扩面** | 先 G1–G3 / WP-2.4；不先做 proxy Store 全家桶 |
| **不挡工程化** | R-WP 与 W1 冒烟、W2 文本/golden、W4 发布**并行**；不以「Solid 追平」为 Beta 前置 |
| **一门禁一勾选** | WP-2.4 / 2.5 关闭条件仍记在总计划；R-WP-1/4 提供审计清单与加深，不另开第二套勾选互掐 |
| **原生优先对照** | 实现灵感：Floem UpdaterEffect / Scope dispose；图协议：Leptos；**禁止**抄 SSR |

**硬退出（反应性专项 DoD）**：

- [ ] WP-2.4 ☑（总计划）：Anchors/Bindings 随 Owner dispose；scoped id 辅助 API；无表膨胀测  
- [ ] WP-2.5 ☑（总计划）：produce 文档+测；Resource Err→EB e2e；SemVer 面草稿  
- [ ] R-WP-2：根裸 `get()` 可观测或 debug 断言；文档红线可测  
- [ ] [`REACTIVITY.md`](REACTIVITY.md) §6 表 G1–G5 关闭或显式降级为 P2/Backlog  
- [ ] [`reactivity_and_hot_reload.md`](reactivity_and_hot_reload.md) 与 identity §7 与实现同步  

---

## 1. 与现有 WP 的映射（禁止重复）

| 总计划 WP | 本计划 | 分工 |
|-----------|--------|------|
| **WP-2.4** Anchors/Bindings→Owner + scoped ID | **R-WP-1** | 2.4 = 任务与 ☐ 勾选；R-WP-1 = 审计清单、测矩阵、`COMPONENT_*` 覆盖 |
| **WP-2.5** Store/Resource 契约 | **R-WP-3**（Store 加深，2.5 之后）· **R-WP-4**（异步契约，承接 2.5 测） | 2.5 只做 produce 契约 + EB e2e + 实验面标注；proxy / 自动桥为可选加深 |
| （无对应） | **R-WP-2** 双路径 / 根订阅 | 纯反应性卫生 |
| （无对应） | **R-WP-5** 列表 reconcile | 纲领「部分 keyed/Index」 |
| （无对应） | **R-WP-6** 热重载×反应槽 | 手册 + 回归 |
| （无对应） | **R-WP-7** 宏降级 | DX |
| （无对应） | **R-WP-8** 可观测 | P2 / Backlog 切片 |

组件侧：**C-WP-V.5 / 2.6** 管身份分类与空串；R-WP-1 提供 Owner 前缀能力后由组件计划消费。

---

## 2. 波次建议

```mermaid
flowchart LR
  subgraph gate [巩固期门禁]
    WP24[WP-2.4]
    WP25[WP-2.5]
    R1[R-WP-1]
    R4a[R-WP-4 测]
  end
  subgraph deepen [加深 / 可与 W3 交错]
    R2[R-WP-2]
    R5[R-WP-5]
    R6[R-WP-6]
    R7[R-WP-7]
  end
  subgraph later [P2 / Backlog]
    R3[R-WP-3]
    R8[R-WP-8]
  end
  WP24 --> R1
  WP25 --> R4a
  R1 --> R2
  R4a --> R3
```

| 阶段 | 包 | 说明 |
|------|----|------|
| **Gate（与 W2 并行）** | WP-2.4 + R-WP-1；WP-2.5 + R-WP-4 契约测 | 阻塞「可预测更新模型」门槛 |
| **Deepen** | R-WP-2 · 5 · 6 · 7 | 不阻塞 crates.io 0.1 Win-first，但进 Beta 建议 R-WP-2 关闭 |
| **Later** | R-WP-3 · 8 | 明确 P2；可进 Backlog |

---

## 3. 工作包明细

### R-WP-1 Registry Owner 收口验收

| | |
|--|--|
| **对齐** | 总计划 **WP-2.4**（勾选仍在总计划） |
| **路径** | `app/reactive.rs`（`REGISTERED_*`、`COMPONENT_PROPS`/`CHILDREN`）、`core/reactivity.rs` |
| **任务** | ① 审计：Anchor / Binding / COMPONENT_* 是否均在 owner dispose 或等价路径移除；② Binding 补 `on_cleanup`（若仅 unmount 不足）；③ 长会话挂载/卸载测：registry 键数不单调膨胀；④ 与 C-WP-2.6 对照全局 `"{id}__…"` 清单 |
| **验收** | 测矩阵写入本包附录或 `crate_tests/reactive/`；dispose 后无残留键；更新 [REACTIVITY.md](REACTIVITY.md) G1 |
| **状态** | ☐ |

### R-WP-2 双路径收敛 / 根订阅可观测

| | |
|--|--|
| **路径** | `app/lifecycle`（根 effect）、`app/reactive.rs`、可选 debug feature |
| **任务** | ① 文档红线可测：根 `app_builder` 裸 `get()` 检测（debug assert 或计数钩子）；② 统计或测试锁定「优先 bind / Show，避免整树 diff」；③ 缩小必须走全树属性 diff 的场景说明 |
| **验收** | 至少一测证明误用可被发现或文档示例全部走三档；更新手册红线节 |
| **状态** | ☐ |
| **对照** | Floem：UpdaterEffect 推 `update_state`，非重建 View；Leptos：RenderEffect 绑叶子 |

### R-WP-3 Store 路径订阅加深（P2）

| | |
|--|--|
| **依赖** | **WP-2.5 退出后** |
| **路径** | `core/store.rs` |
| **任务** | 二选一写进 ADR 小记：**(A)** 文档冻结「仅 `store_select` + `produce`」，标实验；**(B)** 引入路径触发器（思想对齐 `reactive_stores`，实现自研） |
| **验收** | SemVer 面明确；若选 B 须有字段级更新测 |
| **状态** | ☐ 默认倾向 A，直到有真实组件痛点 |

### R-WP-4 Async 边界完整契约

| | |
|--|--|
| **对齐** | 总计划 **WP-2.5** 任务② |
| **路径** | `app/resource.rs`、`app/error_boundary.rs`、`core/resource.rs` |
| **任务** | ① Resource Err→EB **端到端测**（2.5 门禁）；② 首载 / refetch / generation 取消策略表写入 REACTIVITY 或手册；③ 自动桥 vs 显式 `throw_error` 二选一文档化（巩固期允许显式） |
| **验收** | 2.5 勾选 + 策略表；无 SSR 语义 |
| **状态** | ☐ |

### R-WP-5 列表 reconcile 硬化

| | |
|--|--|
| **路径** | `app/reactive.rs`（`for_reactive` / `for_keyed`）、`app/state.rs` reconcile |
| **任务** | ① keyed 移动/复用测；② 文档区分 For vs `for_each`（Index 近似）vs 真 Index；③ 对照 Floem `dyn_stack` 仅学 diff 纪律 |
| **验收** | 库测覆盖插入/删除/换位；REACTIVITY §4 Index 行更新 |
| **状态** | ☐ |

### R-WP-6 热重载 × 反应槽不变量

| | |
|--|--|
| **路径** | `app/hot_reload.rs`、`reactivity_and_hot_reload.md` |
| **任务** | ① 结构岛稳定 `template_id` 指南（gallery 抽 2 例）；② 空 templates 退化整树策略写明；③ 与 `reconcile` 精确匹配回归（禁止 fuzzy） |
| **验收** | 手册 § 更新；至少一回归测或清单项 |
| **状态** | ☐ |

### R-WP-7 宏降级完备性

| | |
|--|--|
| **路径** | `rui_macros`、`#[component(reactive)]`、gallery 试点 |
| **任务** | ① `isolate_reads` 下漏绑回归；② Switch/Match 糖评估（可延后）；③ gallery 扩 `ReactiveStatusBadge` 模式 |
| **验收** | 漏绑有测或已知限制表 |
| **状态** | ☐ |

### R-WP-8 信号图可观测（P2 / Backlog）

| | |
|--|--|
| **任务** | effect/owner/订阅计数或泄漏诊断钩子；**非**完整 DevTools UI |
| **验收** | feature 门或 debug API 文档；进纲领 P2 |
| **状态** | ☐ |

---

## 4. WP-2.4 / WP-2.5 验收清单（镜像，勾选以总计划为准）

### WP-2.4（镜像）

- [ ] Anchors：owner dispose → 键消失（已有部分 `on_cleanup`，审计全覆盖）  
- [ ] Bindings：同等保证  
- [ ] scoped `template_id` / `focus_id` 辅助 API  
- [ ] 长会话无膨胀测  
- [ ] 文档：`reactivity_and_hot_reload.md` + `element_identity.md` §7  

### WP-2.5（镜像）

- [ ] `produce` 语义与文档示例  
- [ ] Resource Err → ErrorBoundary e2e  
- [ ] 实验 API 列入 CHANGELOG 草案  

---

## 5. 明确不做

- 为追 Leptos 而引入 SSR / hydrate / `Resource` 编解码  
- 把 `core/reactivity` 替换为依赖 `reactive_graph` / `floem_reactive`（对照学习可以，换核需独立 ADR）  
- 巩固期实现完整 Store proxy（除非 R-WP-3 选 B 且有组件痛点）  
- 用反应性专项阻塞 Win-first 0.1 发布（门禁仅 WP-2.4 泄漏测建议进 Beta；0.1 Alpha 可注明已知限制）  

---

## 6. 工作约定

1. **一 R-WP 一 PR**（或与对应 WP-2.x 同 PR，描述写清双勾选）。  
2. 关闭 G 项时同步改 [`REACTIVITY.md`](REACTIVITY.md) §6。  
3. 测试基线同总计划 §10；反应性改动优先 `crate_tests/reactive/` + 模块内测。  
4. 对照代码只读 `reference/leptos-main`、`reference/floem-main`，禁止无 ADR 复制粘贴进 `crates/rui`。  

---

## 7. 文档拓扑

```text
AUTHORITY.md                        ← ★执行源（WP-2.4 / 2.5 / 2.7 / WP-R.*）
ENGINEERING_GRADE_FRAMEWORK.md
REACTIVITY.md                       ← 领域规范
REACTIVITY_PLAN.md                  ← 本文件（R-WP 冻结展开）
reactivity_and_hot_reload.md        ← 手册
element_identity.md
```

---

*冻结：勾选改 [`AUTHORITY.md`](AUTHORITY.md)；本文仅保留 R-WP 细则。*
