# Element 身份契约（`id` / `focus_id` / `template_id` / a11y）

> **文档地位**：跨**组件库**与**框架运行时**的身份单一权威契约。凡涉及「给控件起名」、热重载 reconcile、焦点、AccessKit `NodeId`、全局 `use_signal` 键，以本文为准。  
> **从属关系**：服从 [`ENGINEERING_GRADE_FRAMEWORK.md`](ENGINEERING_GRADE_FRAMEWORK.md)；组件 API 面摘要见 [`COMPONENT_LIBRARY.md`](COMPONENT_LIBRARY.md) §4.2；执行排期见 [`AUTHORITY.md`](AUTHORITY.md) WP-CV.4 / WP-2.4 / WP-2.8。  
> **事实来源**：`core/tree.rs`（`Element` 字段）、`a11y/bridge.rs`、`app/hot_reload.rs`、`app/reactive.rs`、`rui_macros`、`components/*` Builder。  
> **日期**：2026-08-09  

交叉：[`a11y_smoke.md`](a11y_smoke.md) · [`keyboard_nav.md`](keyboard_nav.md) · [`REACTIVITY.md`](REACTIVITY.md) · [`REACTIVITY_PLAN.md`](REACTIVITY_PLAN.md) · [`reactivity_and_hot_reload.md`](reactivity_and_hot_reload.md) · [`custom_components.md`](custom_components.md)

---

## 0. 一句话

**用户可见的 `id` 不是 HTML `id`，也不是 AccessKit 全局主键。**  
它最多是「实例键」的一种输入；`focus_id`、`template_id`、信号命名空间、读屏 `NodeId` 是**不同轴**，禁止混用、禁止用空字符串当「有身份」。

---

## 1. 为何需要本文（事故原型）

### 1.1 现象

验收读屏（Narrator / AccessKit）时，无障碍树里出现**重复或无效的子节点 id**，适配器/OS 侧断言失败或崩溃。根因常被误判为「AccessKit 桥写错了」——桥侧补丁可以止血，但**身份债在组件与宏**：

| 步骤 | 事实 |
|------|------|
| 1 | RSX 省略 `id` 或 Builder 传入 `""` → 多个组件共享**空字符串**实例键 |
| 2 | 实现把该键直接写入 `Element.template_id` 和/或 `Element.focus_id`（`Some("")`） |
| 3 | AccessKit 曾用 `template_id` / `focus_id` 哈希为 `NodeId`，**空串或同串 → 同一 `NodeId`** |
| 4 | 同一父节点的 `children` 列表出现重复 id → 平台适配器崩溃 |

### 1.2 已有针对性修复（止血，非根治）

`a11y::bridge::stable_node_key` / `alloc_unique_node_id`：

- 忽略空 `focus_id` / `template_id`；
- 非空 `template_id` 按 **`rui:tpl:{tid}@{path}`** 消歧（reconcile 身份 ≠ 全局 a11y 身份）；
- 重复 `focus_id` / 哈希冲突时回退到 path 键。

库测锚点：`anon_empty_template_ids_do_not_duplicate_children`、`reused_template_id_disambiguated_by_path`、`duplicate_focus_id_second_node_gets_path_key`。

**判定**：桥侧防御是**框架不变量**，必须保留；但若组件继续把空串当身份、把 `template_id` 当全局主键，仍会破坏焦点路由、信号撞键、热重载与测试选择器。根治在 **C-WP-V.5 / C-WP-2.6（组件）+ WP-2.4（scoped 身份，框架）**。

---

## 2. 四条身份轴（禁止混为一谈）

| 轴 | `Element` / API 载体 | 语义 | 全局唯一？ | 用户是否常写 |
|----|----------------------|------|------------|--------------|
| **实例键** | 今日多由组件 `id: String` 派生 | 非受控 `use_signal("{id}__…")`、hover 键、`{id}__open` | **在同一信号表内必须唯一** | 仅多实例 / 跨 rebuild 持久时 |
| **`focus_id`** | `Element.focus_id` | Tab / 点击焦点、浮层锚点、AccessKit **首选**节点键、Action 路由 | **应用内焦点空间唯一**（重复时桥会消歧，但焦点路由仍坏） | 可编程焦点、a11y、锚点时 |
| **`template_id`** | `Element.template_id` | 热重载 / `reactive!` **槽位** reconcile；绑定路径查找 | **槽位内**稳定即可，**非**全树全局主键 | 热补丁或显式 `reactive!("…")` |
| **测试 / 调试选择器** | 可与上重合 | 自动化、日志 | 测试约定 | 需要时 |

另：**AccessKit `NodeId`** 是由桥从上述键 **派生** 的 `u64`，不是第五个用户 API；派生规则见 §4。

### 2.1 消费者矩阵（谁读哪一轴）

| 消费者 | 实例键 | `focus_id` | `template_id` | 空串是否合法 |
|--------|--------|------------|---------------|--------------|
| 非受控 Signal / 注册表 | ✓ | | | **否**（撞键） |
| Tab 序 / `dispatch_key_for_focused` | | ✓ | | **否**（找不到节点） |
| 浮层 `anchor` / `focus_trap` ring | | ✓ | | **否** |
| AccessKit 树 / Action | | ✓（首选） | 仅消歧辅助 | 空 = 无键，须 path |
| 热重载注入 / `update_element_by_template_id` | | | ✓ | 空 = 不可寻址 |
| `bind_*` 路径缓存 | | | ✓ | 空 = 不可缓存 |
| Gallery / 冒烟脚本 | 可选 | 常用 | 少用 | 示范勿堆无意义 id |

---

## 3. 硬性不变量（Invariants）

下列任一条被违反即视为 **P0 身份缺陷**（可标 `broken` / 阻塞稳定面），不得靠「读屏时碰巧不崩」蒙混。

### I-1 · 空串不是身份

- `focus_id` / `template_id` 为 `None` 或**不得**为 `Some("")` 并当作有效身份。  
- 组件若接受用户 `id`：空串必须 **拒绝、生成会话内唯一键、或明确不写** `template_id`/`focus_id`——三者择一，禁止原样写入。  
- AccessKit：`should_be_accesskit_node` 与 `stable_node_key` 必须对空串与「有文本/有 click」解耦处理，**永不**因多个 `""` 产生重复 `NodeId` 子列表。

### I-2 · `template_id` ≠ 全局 a11y / 焦点主键

- Reconcile / 热重载可在多处复用相同 template 尾缀（宏行号碰撞、列表项复用）。  
- AccessKit **禁止**单独用裸 `template_id` 哈希为全树唯一 `NodeId`；须 path 消歧或改用 `focus_id`。  
- 组件文档与 `//!` **禁止**写「把 template_id 当 focus_id」。

### I-3 · `focus_id` 空间唯一（产品承诺）

- 同一窗口焦点空间内，两个可聚焦控件不得长期共享同一非空 `focus_id`。  
- 桥侧 path 消歧只保证「树不崩」，**不**修复错误的焦点/Action 路由——重复 `focus_id` 仍是组件 bug。

### I-4 · 前缀即契约

- 角色推断依赖前缀时须稳定：`input:`、`textarea:`（若有）、`tabstrip:`、`scroll:` 等。  
- 新增前缀：同时改 `a11y/bridge` + [`keyboard_nav.md`](keyboard_nav.md) + 本文件 §5 表。

### I-5 · 自动 id 的寿命

- 会话内递增（`btn-N`、`__rui_input_rt_N`）只保证**当下**不撞信号键。  
- **不**承诺跨 rebuild / 热重载 / 进程的同一逻辑实例。  
- 需要稳定身份时：显式用户键、受控 Signal、或未来 Owner 作用域键（WP-2.4）。

### I-6 · 双书写面同语义

- RSX 省略 `id` 与 Builder `id: ""` / 可选 id **必须**落到同一套空串处理规则（I-1），不得一边自动生成、一边写入 `""`。

---

## 4. AccessKit 派生规则（框架真源）

权威实现：[`crates/rui/src/a11y/bridge.rs`](../crates/rui/src/a11y/bridge.rs)。

```text
preferred_key =
  non-empty focus_id                          →  bare focus_id   (Action/焦点对齐)
  else non-empty template_id                  →  rui:tpl:{tid}@{path}
  else                                        →  rui:path:{path}

NodeId = hash(preferred_key).max(2)           // 1 = 窗口根
若 NodeId 已占用                              →  rui:path:{path}[#n] 直至唯一
```

| 规则 | 说明 |
|------|------|
| 窗口根 | 固定 `NodeId(1)` |
| 可聚焦控件 | 应有非空 `focus_id`；读屏 Focus/Click 依赖它 |
| 仅展示文本节点 | 可无 `focus_id`；用 path / 消歧后的 template 键 |
| 重复子 id | **框架 bug**；库测必须锁死 |

手工验收步骤：[`a11y_smoke.md`](a11y_smoke.md)。桥防御**不豁免**组件 I-1～I-3。

---

## 5. `focus_id` 前缀表（摘要）

| 前缀 | 控件 / 用途 | AccessKit 角色提示 |
|------|-------------|-------------------|
| `input:` | Input 根 | `TextInput` + value |
| （textarea 约定） | Textarea | 同 TextInput 路径 |
| `tabstrip:` | Tabs 条带 | `TabList` |
| `scroll:` | ScrollArea 焦点 | 滚动容器 |
| 无前缀 / 组件自定 | Button、菜单项等 | `accesskit_role` 或推断 |

完整键盘行为见 [`keyboard_nav.md`](keyboard_nav.md)；本表只约束**身份字符串形状**。

---

## 6. 分层职责（组件 vs 框架 vs 宏）

```text
  应用 / Gallery
       │  可选：稳定实例键、aria_label、测试 id
       ▼
  components/          ← 决定：空 id 策略；是否写入 focus_id / template_id；信号键命名空间
       │
  rui_macros (ui!)     ← 决定：省略 id 时的默认；为热重载注入 file:line:col template_id
       │
  Element 字段         ← 载体，不解释业务
       │
  ┌────┴─────┬──────────────┐
  ▼          ▼              ▼
 reactivity  hot_reload   a11y/bridge
 (信号键)   (template 槽)  (NodeId 派生 + 消歧)
```

| 层 | 必须做 | 禁止 |
|----|--------|------|
| **组件** | 空 id → 自动唯一键 **或** 不设置 focus/template；多实例文档化；可聚焦控件非空 `focus_id` | 把 `""` 写入 focus/template；用 template 当焦点键；展示型强迫用户 id |
| **宏** | 热重载用的 `file:line:col` template 与组件实例键分离；省略 id 与 Builder 空 id 同策略 | 默默把省略 id 变成共享 `""` 身份 |
| **框架 · 热重载** | `template_id` 槽位匹配；不假设全树唯一 | 要求每个叶子都有用户 id |
| **框架 · a11y** | I-1 防御；path 消歧；Role/Action | 用桥补丁代替修组件撞 `focus_id` |
| **框架 · 反应性** | Owner / scoped 信号减少全局字符串（WP-2.4） | 新 API 再引入「必填魔法 id」 |

---

## 7. 现状与目标态

### 7.1 现状（2026-08-09）

| 项 | 状态 |
|----|------|
| 规范摘要 | [`COMPONENT_LIBRARY.md`](COMPONENT_LIBRARY.md) §4.2 已纠偏「过度 id」 |
| Button / Input 空 id | 自动 `btn-N` / `__rui_input_rt_N`（会话内） |
| 大量 Builder | 首位仍强制 `id: impl Into<String>`；Gallery 堆 `gal-*` |
| RSX 省略 id | 部分路径仍可能落到空串再写入 Element（事故源） |
| AccessKit 桥 | `a11y/bridge.rs`：空串忽略、`rui:tpl:…@{path}`、`alloc_unique_node_id` + 库测（**WP-1.3 止血**） |
| 组件空串写入 | **未根治**（仍可能 `template_id = Some("")`）→ C-WP-V.5 / C-WP-2.6 |
| Owner 作用域 id | 未完成（总计划 WP-2.4） |

### 7.2 目标态

1. **展示型**：用户 API 无必填 `id`；默认不写 `focus_id`/`template_id`（除非测试）。  
2. **有状态非受控**：框架或宏提供**非空**实例键；优先 `use_scoped_signal`，少用全局 `"{id}__…"`。  
3. **可聚焦**：始终非空、焦点空间唯一的 `focus_id`（可有前缀）。  
4. **热重载**：宏 `file:line:col` 与用户实例键分离；列表项用显式 `reactive!(…)` 槽。  
5. **a11y**：主路径控件 Role + 名 + Focus/Click；空/撞键在库测阶段失败，不进读屏。

---

## 8. 工作包映射（同步推进）

本文是契约；勾选与排期在 [`AUTHORITY.md`](AUTHORITY.md)。

| 工作包 | 本文关注点 |
|--------|------------|
| **WP-CV.4** `id` 过度使用审计 | 表列每组件：必填/可选；是否写入 focus/template；空串行为 |
| **WP-2.8** 身份 API 收敛 | 展示型去必填；信号迁 scoped；Gallery 减噪 |
| **WP-1.3** AccessKit 主路径 | **仅**桥 Role/Action + §4 防御测 + 读屏冒烟；**不含**组件身份 API 收敛 |
| **WP-2.4** Anchors / scoped ID | Owner 前缀 `template_id`/`focus_id`；注册表随 dispose 清理 |
| （建议）**身份库测门禁** | `a11y::bridge` 空串/复用 template 测不得删；组件侧增加「空 id 不写 `Some("")`」测 |

**推进顺序**：CV.4 审计 → 桥/测锁死 I-1（已大部分完成）→ WP-2.8 空串与必填 → WP-2.4 scoped 降全局键依赖。禁止只加桥分支、不改组件空串写入。

---

## 9. PR / 设计检查表

新增或改动组件、宏默认 id、`Element` 身份字段、AccessKit 桥时：

- [ ] 标明本控件依赖哪几轴（实例键 / focus / template / 无）  
- [ ] 空用户 `id`：自动非空 **或** 不设置 focus/template；有测  
- [ ] 可聚焦 → 非空 `focus_id`；多实例不撞  
- [ ] 未把 `template_id` 当作焦点或 a11y 全局键  
- [ ] 新 `focus_id` 前缀已更新 §5 + `keyboard_nav.md` + bridge  
- [ ] 若动 bridge：保留/扩展 `anon_empty_*` / `reused_template_*` 测  
- [ ] Gallery 不为「好看」给无状态控件堆 id  
- [ ] 文档：`//!` 写明多实例与空 id 契约  

命令：

```text
cargo test -p rui --lib a11y::
cargo test -p rui --lib
cargo check -p gallery
```

---

## 10. 反例（禁止合入稳定路径）

| 反例 | 后果 |
|------|------|
| `el.template_id = Some(builder.id)` 且 `id == ""` | 多节点同键 → 历史 a11y 崩溃；reconcile 糊成一团 |
| `focus_id("")` 且 `focusable(true)` | Tab/Action 无法稳定寻址 |
| 列表每行复用同一非空 `focus_id` | 焦点跳错；桥消歧后读屏与键盘不一致 |
| 仅靠 bridge path 消歧「修复」组件重复 focus | 掩盖产品 bug |
| 文档教「每个控件都写 id」 | 放大身份债；与 §7.2 冲突 |
| 新组件再引入「首位必填 id」无信号需求 | 阻塞 C-WP-2.6 |

---

## 11. 文档拓扑

```text
AUTHORITY.md                            ← ★执行源（WP-CV.4 / WP-2.4 / WP-2.8）
ENGINEERING_GRADE_FRAMEWORK.md          ← 总纲领（P1 DX：scoped id）
REACTIVITY.md                           ← 细粒度口径
COMPONENT_LIBRARY.md §4.2               ← 组件 API 摘要 → 指向本文
element_identity.md                     ← 本文件（跨层身份权威）
a11y_smoke.md / keyboard_nav.md / reactivity_and_hot_reload.md
```

争议：先改本文不变量与职责表，再改代码；禁止在组件 PR 内静默重新定义「id = template_id = focus_id」。Owner 注册表执行见 [`AUTHORITY.md`](AUTHORITY.md) WP-2.4。

---

*修订时同步更新 §7 现状表与 §8 勾选指针；新增身份相关公开 API 必须落入本契约。*
