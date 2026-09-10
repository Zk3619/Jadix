# 指针拖拽分层与可选 crate 拆分（架构草案）

> **文档地位**：把「所有叫拖的行为」套进与 [`assets_and_icons_architecture.md`](assets_and_icons_architecture.md) 相同的分层：核心契约 / 可选产品目录 / 组件库；对齐 W4 伞形与 `jadix_components`。  
> **状态**：草案（未实施）；执行前以 [`AUTHORITY.md`](../AUTHORITY.md) / [`W4_PRELAUNCH_CHECKLIST.md`](../ops/W4_PRELAUNCH_CHECKLIST.md) 排期为准。**不挡** W4 四 crate 发布；可选 crate 抽离排在 `jadix_icons` 模板之后。  
> **交叉**：[`runtime_extension_points.md`](runtime_extension_points.md)（FrameHook / PointerCapture）· [`headless.md`](../components/headless.md) · [`overlay_shells.md`](../components/overlay_shells.md) · [`drag.md`](../components/drag.md) · [`COMPONENT_LIBRARY.md`](../COMPONENT_LIBRARY.md) §3（双书写面）· [`SOLID_DND_COMPARISON.md`](../comparison/SOLID_DND_COMPARISON.md) · [`SORTABLE_JS_COMPARISON.md`](../comparison/SORTABLE_JS_COMPARISON.md) · [`INTERACT_JS_COMPARISON.md`](../comparison/INTERACT_JS_COMPARISON.md)

代码现状以工作区为准：`pointer_drag`、`utils/drag`、`pointer_action`、`resizable.rs`、`anchored_overlay::drag_handle`、`App::textarea_resize_drag`。本文只定分层，不改 SemVer 稳定面。

---

## 1. 目标形态

```
用户应用
  Cargo.toml       ← 显式依赖需要的「能力 crate」
  src/

jadix              ← 核心：reactivity / layout / renderer
                       + pointer_drag 会话表
                       + window_events / lifecycle 路由（薄接收端）
                       + 纯几何（resize_box / GestureState / 指数衰减）
                       + 浮层壳 Point 拖（anchored_overlay::drag_handle）
jadix_dnd          ← 可选：solid-dnd 域产品目录（Provider / Droppable / Sortable / Overlay）
jadix_pointer      ← 可选：interact 域会话编排（边/角 resize、双指配额、自由拖惯性）
                     也可暂作 jadix 内模块；与 dnd **分开门控**
jadix_components   ← Resizable 分栏、Textarea 角点、Slider / Carousel / Toast 拖
jadix_macros
jadix_icons        ← 图标库先行，作为「可选 crate + install()」模板
```

**原则**：`jadix` 核心持有 **指针会话契约与帧路由**，不把 Sortable / 八把手 / 双指配额当作默认稳定面；控件级拖跟组件库走，不跟 DnD 目录走。

与资源草案同构：核心零产品目录（对应「零 `include_bytes!`」）；可选 crate `depends on jadix`，核心 **不** `depends on` 产品 crate。

---

## 2. 四层模型

| 层级 | 谁管 | 内容 | 加载方式 |
|------|------|------|----------|
| **会话契约** | `jadix` | `pointer_drag`：按 `pointer_id` 的 begin / tick / clear；`DragKind` 或等价注册表 | 常驻；主循环扇出 |
| **框架能力（几何）** | `jadix` | `resize_box`、`GestureState`（distance / angle / scale / centroid）、与滚动 fling 同形的衰减 | 纯函数；无 UI、无会话 |
| **浮层壳** | `jadix` `utils/anchored_overlay` | Point 模式标题栏拖走 | 与 Portal / Esc 栈同级，不进 dnd crate |
| **DnD 目录** | `jadix_dnd`（目标） | Provider / Draggable / Droppable / Overlay / Sortable + 位移修饰 | 应用显式依赖；`install()` 登记 tick |
| **指针动作目录** | `jadix_pointer` 或独立 feature | `ResizeSession`、`GestureSession`、`PointerSet` 配额、自由拖 `Coasting` | 与 DnD **分 feature**；默认不随 dnd 打开 |
| **控件适配器** | `jadix_components`（目标；现 `components/`） | 一维分栏、textarea 调高、Slider、ColorPicker、Toast、Carousel | 只调会话契约 + 必要时内部调用几何函数 |

### 2.1 删除测试（缝在哪里）

| 删掉什么 | 谁还活着 | 说明 |
|----------|----------|------|
| `utils/drag` | 分栏、浮层标题栏、textarea 角点、Slider | DnD **产品目录** |
| `pointer_action` 会话编排 | `resizable.rs`、textarea、浮层拖 | interact **产品目录**；几何函数仍可留核心 |
| `pointer_drag` + 主循环扇出 | 上面全部断 | **唯一必须留在核心的钩子** |

`window_events` 今日是并排调用，不是一条管线：`tick_resizable_drag`、`tick_floating_panel_drag`、`drag::tick`、`pointer_action::on_move`，外加 `App::textarea_resize_drag`（尚未进会话表）。共享的是 **begin → tick(move) → end** 与指针捕获，不是同一套几何或不变量。

### 2.2 与现有 feature / 模块的关系

| 现状（`rui`） | 目标 |
|---------------|------|
| `utils/drag` 在主 crate、不进 `prelude`、Experimental | 过渡期保持模块；目标迁 `jadix_dnd` |
| `pointer_action` 与 `utils/drag` 兄弟模块 | 保持兄弟；**禁止**并进同一默认 Cargo feature |
| `pointer_drag` + `DragKind::{Slider, Resizable, FloatingPanel, Scope, PointerAction, …}` | 核心会话表；kind 演进为注册表（见 [`runtime_extension_points.md`](runtime_extension_points.md) §1） |
| textarea 调高在 `App` 私有字段 | Phase 0 并入 `pointer_drag`（卫生，不是产品合并） |
| `resizable.rs` 一维 flex 比例 | 留组件路径；**不**并入 `ResizeSession` |
| 滚动 fling（`scroll_support`） | 继续独立会话；可复用核心衰减公式，不共用 DnD `DragPhase` |

DnD 与指针动作 **不要混在同一 feature**（对齐资源草案：Lucide / CJK / Emoji 分 feature）。

---

## 3. `jadix` 核心如何接拖拽

用 **会话表 + 帧钩子**，避免核心硬依赖产品 crate。形态对齐图标的 `IconProvider` 与 Devtools 的 introspection API：pull 查询走 provider，push 的每帧 tick 走 **核心接收端**。

```rust
// jadix::pointer（草案：核心只定义契约）
pub trait PointerTick: Send + Sync {
    fn on_move(&self, pointer_id: u64, x: f32, y: f32) -> bool; // needs_redraw
    fn on_up(&self, pointer_id: u64) -> bool;
}

pub fn register_pointer_tick(kind: &'static str, h: Arc<dyn PointerTick>);
```

| 依赖组合 | 行为 |
|----------|------|
| 仅 `jadix` | 浮层 Point 拖、控件拖（分栏 / textarea / Slider…）可用；无 Sortable、无 Gallery 八把手 |
| `jadix` + `jadix_dnd` | `jadix_dnd::install()` 登记 `Scope` tick；`DragProvider` 等作者面可用 |
| `jadix` + `jadix_pointer` | `install()` 登记 `PointerAction`；resize / gesture / 自由拖惯性 |
| `jadix_components` | 默认 **不** 依赖 `jadix_dnd`；分栏与 textarea 只依赖会话契约 |

主循环（`window_events` / `lifecycle`）只遍历注册表 + 内置浮层/控件钩子。禁止为「核心零钩子」把 move/up 路由赶到应用层（对齐资源草案 §8.3 / §12）。

今日已有雏形：[`pointer_drag.rs`](../../crates/rui/src/utils/pointer_drag.rs) 按 `pointer_id` 的 `HashMap` 会话；缺的是「产品 tick 自注册」，不是再造一套 `Drag` 上帝 API。落地协议以 [`runtime_extension_points.md`](runtime_extension_points.md) 的 `FrameHook` / `PointerCapture` 为准，本文不另起第三套名字。

---

## 4. 两套可选目录的职责

interact.js 把 `drag` / `resize` / `gesture` 并列在同一 Interactable 下，共享指针配额；RUI 对齐的是 **并列**，不是焊进 `utils/drag` 的 `DragPhase`（会污染 `can_drop` / innermost / FLIP）。

### 4.1 `jadix_dnd`（solid-dnd 域）

从 `crates/rui/src/utils/drag/` 迁出（目标态）：

- Provider / Draggable / Droppable / Overlay / Sortable
- 碰撞、让位、自动滚、Esc / 方向键
- interact **位移修饰**（轴锁、钳位、网格）——修饰的是 DnD 增量，仍属本目录
- **Extendable §11**：[`registry`](../../crates/rui/src/utils/drag/registry.rs)（`data` / 空闲 `hits`）、[`sensor`](../../crates/rui/src/utils/drag/sensor.rs)（`run_scope_sensors` / `begin_draggable`）、`pull_clone` / `skip_*` — 见 [`drag.md`](../components/drag.md) §8

作者面见 [`drag.md`](../components/drag.md)。不进 `prelude`，不进 Beta SemVer 默认稳定面（与现状 Experimental 一致）。

### 4.2 `jadix_pointer`（interact 域会话）

从 `crates/rui/src/utils/pointer_action/` 迁出 **会话编排**（目标态）：

- `ResizeSession`（边/角、invert、aspect、min/max、from_center）
- `GestureSession` / `PointerSet` 配额
- 自由拖松手 `Coasting`（Sortable 立即 `commit_end`，不滑行）

**纯几何**（`resize_box`、双指 distance/scale、指数衰减）更像 glyphon：可留在 `jadix`，供 textarea 内部调用，不必把八把手做成公开组件。

crate vs feature 对应资源草案 §13「`EmojiProvider` 独立 crate vs `jadix_icons` feature」：允许晚定；**默认矩阵必须分开**。过渡期两个模块并列即可。

### 4.3 对外 API（稳定面草案）

```rust
// jadix_dnd
pub fn install(); // 登记 Scope tick
// 其余：现有 DragProvider / DragCtx / Sortable* Builder；RSX 由 macros 读本 crate build 产物

// jadix_pointer
pub fn install();
pub fn begin_resize(/* … */);
pub fn hit_edges(/* … */) -> Edges;
```

`jadix_macros` 的 `<DragProvider>` 等编译期路径改读 **产品 crate** 的描述符，不继续把标签写死在核心 match 里（见扩展点文档 RSX 节）。纯 Builder 不强制依赖宏（ADR-9）。

---

## 5. 与 `jadix_components` 拆分对齐

```
jadix_components/Cargo.toml
  jadix = { path = "../jadix" }
  # 默认不依赖 jadix_dnd / jadix_pointer
```

| 模块 | crate |
|------|--------|
| `Resizable` 一维分栏 | `jadix_components` → `jadix::pointer_drag` |
| Textarea 右下角调高 | 同上；内部可调 `resize_box`（底边 + `min_size`） |
| Slider / ColorPicker / Toast / Carousel 拖 | `jadix_components` |
| `Element` / `Signal` / `wgpu` / `pointer_drag` | `jadix` |
| `DragProvider` / Sortable | `jadix_dnd` |
| 边/角 / 双指会话 | `jadix_pointer`（或主 crate 模块） |
| `anchored_overlay::drag_handle` | **留 `jadix`**（浮层壳，与 Portal 同级） |

Gallery：`jadix` + `jadix_components` + 演示 DnD / 八把手时再加对应可选 crate。过渡期可用主 crate feature 重导出。

控件级拖是组件契约（ShadCN 分栏、对话框拖走、多行输入改高），不是「拖放套件的演示」。

---

## 6. 迁移路线（分 4 步）

与资源草案 §6 同相位；**图标库先行**，拖拽不插队 W4 四 crate 闭环。

### Phase 0 — 契约（1–2 天，仍在 `rui` 模块内）

- textarea 调高并入 `pointer_drag`（与 Slider / Resizable 同表）
- `window_events` 对 `Scope` / `PointerAction` 走向登记表，而不是无限 `if kind`（对接 `FrameHook`）
- 几何函数保持无 UI；textarea / 浮层若复用 `resize_box` 或 `VelocityTracker`，做成 **私有调用**
- 行为不变；不改 `prelude`、不改 AUTHORITY 勾选

### Phase 1 — 抽 `jadix_dnd`（跟在 `jadix_icons` 之后）

- 新建 `crates/jadix_dnd`，迁 `utils/drag`
- `install()` 登记 tick；`jadix` 删除 Sortable 产品面，**保留**会话表与浮层拖
- 宏侧 Drag 标签改依赖本 crate 描述符

### Phase 2 — 指针动作目录独立门控

- 会话编排随 `jadix_pointer` **或** `jadix_dnd` 的 **独立 feature** 走（开放问题 §11）
- 几何留核心
- 默认 `dnd` feature **不**顺带编译双指 / 八把手

### Phase 3 — `jadix_components`（与 `components/` 目录拆分同步）

- 分栏、textarea、Slider 等迁组件 crate
- 只依赖 `jadix` 会话契约；默认不依赖 `jadix_dnd`

W4 B4：layout / renderer 现阶段仍在主 crate 模块化。拖拽产品目录比 layout 更像 icons（可卸载的能力），故 **模块现在、crate 在 icons 模板之后**。

---

## 7. 用户项目示例

```toml
[dependencies]
jadix = { version = "0.1" }
jadix_components = { version = "0.1" }
# 看板 / Overlay / Sortable：
jadix_dnd = { version = "0.1", optional = true }
# 双指缩放 / 自由矩形边角：再显式打开，不随 dnd 带上
```

```text
my-app/src/main.rs
  // 仅分栏 + 对话框拖走：不必 install dnd
  // 看板：jadix_dnd::install();
  jadix::run(...)
```

纯 Builder 即可写控件拖与（若依赖了目录）`DragProvider`；`ui!` 为可选书写面。

---

## 8. 与热重载 / Devtools / 扩展点同构

资源草案 §8：「同模式，不同形态」。拖拽亦然。

| 维度 | 热重载 | Devtools | 指针产品目录 |
|------|--------|----------|----------------|
| 可选性 | feature + 独立 CLI | feature + in-process crate | feature + **in-process** crate |
| 进程 | CLI 可另进程 | 必须跟帧 | **必须跟帧** |
| 核心保留 | 协议 + reconcile 钩子 | snapshot / patch + 极薄 layout 钩子 | **会话表 + move/up 扇出** |
| 外置 crate | watcher / 编译链 | Dock / F12 / `wrap_app` | Sortable / resize 会话 / RSX 标签 |

**结论**：

1. **是**——`utils/drag` 与 pointer_action **会话编排**应成为 W4 伞形下的 **可选外部 crate**（或主 crate 内明确 optional feature），不进默认 release 稳定面。
2. **否**——不应做成独立进程；tick 必须在同一事件循环。
3. **核心不能为零钩子**：至少保留 `pointer_drag`（或 `PointerCapture`）与主循环遍历。

目标依赖图：

```text
jadix                    ← 默认：会话表 + 浮层/控件钩子；无 Sortable
jadix_dnd                ← depends on jadix；install() 注册 Scope
jadix_pointer            ← depends on jadix；install() 注册 PointerAction（可延后）
jadix_icons              ← 可选 crate 模板（先行）
jadix_devtools           ← 与拖拽无关；登记协议可复用 FrameHook
```

---

## 9. 双书写面（ADR-9）

拆 crate **不**废除 Builder。对齐资源草案 §9。

| 能力 | 不用 `ui!` | 不用产品 crate |
|------|------------|----------------|
| 分栏 / textarea 调高 / 浮层标题栏 | ✅ 组件 Builder | ✅ 仅 `jadix`（+ components） |
| Sortable / Overlay | ✅ `DragProvider` Builder | 需 `jadix_dnd` |
| 边/角、双指 | ✅ `begin_resize` 等 | 需 pointer_action 目录 |
| RSX 标签编译期校验 | 宏读产品 crate 描述符 | 不用 `ui!` 则可忽略 |

`ui!` 自动 bind 降级在纯 Builder 下需手写 `bind_*`，与组件库其余部分相同。

---

## 10. 禁区

- 把 Sortable、`resizable.rs`、textarea 角点、八把手、双指捏合成 **一个** Builder / 一个 `DragPhase`
- 为「方便以后扩」在只有一个调用方时抽 crate，却让核心 `depends on` 产品 crate（循环或假缝）
- 要求核心 **零** move/up 钩子，把路由赶到应用层
- 一个默认 Cargo feature 同时打开 DnD 目录与双指 / 自由矩形 resize（混目录）
- 让 `jadix_components` 默认依赖 `jadix_dnd`（Button 不该为了 check 图标以外的理由拉 DnD）
- 用拖拽拆 crate **插队** W4 四 crate 发布或抢在 `jadix_icons` 模板之前
- 拆分时废除 Builder 双书写面
- 把 `anchored_overlay` Point 拖从核心挪进 dnd crate（浮层壳与 Portal 同级）
- 把滚动 fling 与 DnD `Coasting` 合成同一会话（公式可共享，会话表分开）

---

## 11. 开放问题

- [ ] `jadix_pointer` 独立 crate vs `jadix_dnd` 的独立 feature `pointer-action`（对齐 emoji crate vs `jadix_icons` feature）
- [ ] `PointerTick` 是否直接复用 `FrameHook`，还是 `pointer_drag` 保持窄接口、FrameHook 做扇出适配
- [ ] 多指配额（`PointerSet`）放核心还是 pointer 目录（第二指升 Gesture 与 Slider 抢主指针时的仲裁）
- [ ] `jadix_macros` 对 `<DragProvider>` / 未来 resize 属性的 build 依赖方向
- [ ] 自由拖 inertia 与滚动 fling 是否抽共享 `VelocityTracker` 到 `jadix`（今日公式已同形）
- [ ] Experimental DnD 进入 SemVer 稳定面的门槛（对照、Gallery、库测已超 solid-dnd 0.7.5，稳定面仍可后置）

---

## 12. 当前应保持的形状（W4 前）

执行拆 crate 之前，代码已接近 Phase 0 的模块切分：

| 模块 | 保留 |
|------|------|
| `utils/pointer_drag` | 核心会话表；多指 id |
| `utils/anchored_overlay` | 浮层 Point 拖 |
| `components/resizable` + textarea / Slider… | 控件适配器 |
| `utils/drag` | DnD 目录（模块，不进 prelude） |
| `utils/pointer_action` | 指针动作目录（模块，不并进 `DragPhase`） |

扩能力（snap 边、多指 >2、restrict）改 **目录内部**，不改 crate 边界、不先开新 feature 矩阵。
