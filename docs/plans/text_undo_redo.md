# 文本撤销 / 恢复与大文档缓冲

> **文档地位**：专题**设计稿**（非 [`AUTHORITY.md`](../AUTHORITY.md) 勾选源；**本稿不改代码**）。  
> **日期**：2026-09-09（同日吸收落地审查：瞬时坐标、内部同步隔离、Owner 持有历史、RAII 事务、大替换 Marker）  
> **关联**：[`text_edit_history.rs`](../../crates/rui/src/utils/text_edit_history.rs) · [`text_field.rs`](../../crates/rui/src/utils/text_field.rs) · [`input_controller.rs`](../../crates/rui/src/app/input_controller.rs) · [`ime.md`](../ops/ime.md) · [`runtime_extension_points.md`](../engine/runtime_extension_points.md)（`TextEditHost`）· [`keyboard_nav.md`](../ops/keyboard_nav.md) · [`size_and_memory.md`](../ops/size_and_memory.md)（lean default / feature 门） · **不是** [`platform.md`](../engine/platform.md) P3（该文 §9.8） · Freya F05 / gpui 编辑器对照见 [`FREYA_COMPARISON.md`](../comparison/FREYA_COMPARISON.md)、[`GPUI_ECOSYSTEM_COMPARISON.md`](../comparison/GPUI_ECOSYSTEM_COMPARISON.md)

---

## 0. 结论

1. **撤销/恢复作为独立 Cargo feature 提供**（建议名 `text-undo`），**不进 `default`**。与 `system-tray` / `devtools` / `scene3d` 一样：核心 crate 保持 lean；要桌面输入框手感的应用显式打开。打开后 Input/Textarea **默认启用**；仍可用每控件开关和上限微调。
2. **「撤销栈」不是系统调用栈，现在和换方案之后都在堆上。** 今日的 `Vec` + `String` 快照已是堆分配；换成 `VecDeque` / `Arc<str>` / rope 仍是堆。条目多带来的是**内存、合并、大替换、深度上限、事件回环**，不是递归把线程栈压爆。
3. **应当暴露一小撮上限参数**（条数、历史字节、合并时间窗），并带安全默认与钳位。不要把合并矩阵、`applying` / `is_internal_sync`、rope 内部当公开旋钮。
4. **类 VSCode / Cursor 的超大、多文件编辑，不能建立在「`Signal<String>` + 全文快照」上。** 长期形态：

```text
Cargo feature text-undo
+ 可替换 TextBuffer（先 String，后 rope / piece table，后者另 feature）
+ 可逆差量（TextChange）
+ UndoGroup 合并
+ 条数与字节双上限（可配）
+ 每资源一份历史（关文件丢、存盘不清）
```

`rui` 核心交付 **可选协议 + 表单默认实现**；IDE 级会话放在后续编辑器层，不塞进 `Input`。

**落地必守（审查已采纳，实现时勿回退）：**

1. `TextChange.range` 是**该步发生时的瞬时坐标**；Redo 正序、Undo **倒序**。L1/L2 默认一组一步；多段只允许在事务/L3，且不得把「组开始基准坐标」拿去正序 `replace`。
2. 用户编辑、撤销应用、受控 Signal 回写必须分源；仅 `applying_undo` 不够，还要 **`is_internal_sync`**，否则 `signal.set` → 重建 → 再 `replace` 会把回写推进历史甚至重入。
3. **P1 废弃进程级 thread-local `HashMap`**。历史跟 Owner / 控件状态走，Drop 即释放；动态列表不得靠全局 id 泄漏。
4. 超大替换记 **Marker**，**禁止** `ClearHistory` 把用户前面的小步全部扔掉。
5. 事务用 **RAII Guard**，禁止裸 `begin`/`end` 跨 panic 悬挂。

审查原文与本稿的对应关系见 **§0.1**；实现以本节 + §7–§9 为准，勿回退到旧段落里的「组开始基准坐标 / 全局 Map / ClearHistory 大替换」。

---

## 0.1 审查采纳对照

下列条目来自架构审查，**全部采纳**。旧稿若与此冲突，以本表和落地必守为准。

| 审查点 | 旧稿风险 | 本稿约束 |
|--------|----------|----------|
| 组内多 `TextChange` 坐标 | 「组开始基准坐标」正序 `replace` 会偏移/越界 | **瞬时坐标**；Redo 正序、Undo **倒序**；L1/L2 默认一组一步 |
| Signal 回写 | 仅靠 `applying_undo` 拦不住 `signal.set` → 重建 → 再 `replace` | 必须 **`is_internal_sync`**（§7.5） |
| thread-local `HashMap` | 动态 `field_id` / 卸载不淘汰 → 泄漏或串栈 | **P1 废弃**；历史跟 Owner 走（§7.4） |
| 裸 `begin`/`end` | panic / 提前 return 悬挂 `open_group` | **`EditTransactionGuard` RAII**（§7.6） |
| `Arc<str>` 当零拷贝 | `String` slice 再 `Arc::from` **必拷** | `TextBuffer::Slice` 关联类型，L3 才 `RopeSlice`（§7.1） |
| 超大替换 `ClearHistory` | 一次 20MB 粘贴清掉前面 100 步小编辑 | **`LargeReplaceMarker`**，禁止为此清空更早历史（§9.2） |
| `caret_before` / `after` 易写反 | Undo/Redo 恢复错编辑点 | **`state_before`（Undo）/ `state_after`（Redo）**（§7.3） |
| P1 / P2 / P3 分期 | Map 泄漏拖到很晚；换 rope 无数据 | P1 迁 Owner；P2 Guard；P3 **先 bench 再换缓冲**（§13） |

RUI 是 Solid 式按 Owner 重建 Element，**不要**把「组件结构体字段跨帧持有 `UndoManager`」写成 C++ 控件成员幻想：正确说法是 **与该字段同一 Owner 的 `Signal<UndoManager>`（或等效 slot）**，Owner cleanup 时 Drop。

---

## 1. Cargo feature：`text-undo`

与 crate 现状对齐：[`Cargo.toml`](../../crates/rui/Cargo.toml) 写明 `default = []`，可选能力一律 opt-in。撤销/恢复走同一纪律。

### 1.1 名称与边界

| Feature | 含义 |
|---------|------|
| **`text-undo`** | 编译进撤销+恢复整条能力：历史结构、快捷键处理、控件默认实现、公开配置。 |
| （不设 `text-redo`） | 恢复不是独立 feature。拆开会让「能撤不能恢复」成为合法组合，体验残缺。 |
| **`text-undo-rope`**（P3，可选） | 依赖 `text-undo`；换 rope/piece table 实现。默认 crate **不**强制 `ropey`。 |

复制/剪切/粘贴/全选（A/C/V/X）**不属于**本 feature，始终在默认路径。

### 1.2 建议 Cargo 形态（实现时再改 toml，本稿不落地）

```toml
# crates/rui/Cargo.toml — 示意，未合入
[features]
default = []
text-undo = []
# 以后：
# text-undo-rope = ["text-undo", "dep:ropey"]
```

应用：

```toml
rui = { version = "0.1", features = ["text-undo"] }
```

Gallery / 桌面示例在启用本能力后应打开该 feature（否则 Gallery 里 Ctrl+Z 无历史）。工作区 **feature 统一** 陷阱见 [`size_and_memory.md`](../ops/size_and_memory.md)：只有某个 example 打开时，会把历史代码链进同一次 workspace 构建的其它二进制——文档与 CI 需写明谁启用。

### 1.3 feature 关闭时

| 项 | 行为 |
|----|------|
| 代码 | `UndoManager` / 记账点 `#[cfg(feature = "text-undo")]` 切掉；无历史状态 |
| 内存 | 不为撤销付费 |
| Ctrl/Cmd+Z、Y、Shift+Z | **仍消费快捷键**（与现在栈空时一样），避免把 `z`/`y` 打进字段；不改 buffer |
| 控件 API | `max_undo_entries` 等 builder 在无 feature 时 **不存在或空实现**（实现阶段二选一，推荐 `cfg` 掉方法，避免「配了但不生效」） |
| 密码 / IME | 与今天非撤销路径相同 |

### 1.4 feature 打开时

- Input / Textarea：**默认 `undo_enabled = true`**（密码除外，见 §10）。
- 每控件仍可 `undo_enabled(false)` 关掉（例如一次性验证码旁路）。
- 快捷键、合并、IME 分组、受控 `SetTextUndoPolicy` 全部生效。

两层开关不要混为一谈：

```text
编译期  text-undo     → 二进制里有没有这套代码
运行期  undo_enabled  → 这个字段记不记历史（仅 feature 开启后有意义）
```

### 1.5 与现状代码的关系

当前 MVP（全文快照、100 步、已接 Z/Y）**尚未**套 feature，等于「永远编译进 lib」。迁到独立 feature 时：

1. 先 `cfg` 包住现有 `text_edit_history` + `input_controller` 记账（P0）。
2. **P1 迁出 thread-local Map**，改 Owner 绑定；默认 crate / 计算器等 lean 示例 **不**开 feature；Gallery 开。
3. 再演进差量、Guard 与 `TextBuffer`，仍只在该 feature 内。

---

## 2. 两套「栈」与堆

### 2.1 对照

| | 系统调用栈 | 撤销历史栈 |
|--|-------------|------------|
| 存放 | 线程栈帧 | **堆**（`Vec` / `VecDeque` / `String` / `Arc<str>` / rope 节点） |
| 涨的原因 | 递归、极深 Drop、事件同步重入 | 用户编辑次数 × 每步保存的旧/新文本 |
| 爆了会怎样 | 进程 abort | 内存涨、卡顿、被上限丢掉旧步 |
| 和撤销的关系 | 只有递归执行复合命令、撤销触发记录、极深命令树时才相关 | 正常路径 |

本稿默认：**迭代应用 UndoGroup，禁止递归 undo；撤销应用期间不记账。** 调用栈不是设计约束。

### 2.2 「切换方案后才用堆吗？」

**不是。现在就已经在堆上。**

| | 现在（无 feature 门的 MVP） | 方案后（`text-undo`） |
|--|---------------------------|------------------------|
| 每步 | 整份 `String` 克隆（堆） | 差量（`String`/`Arc<str>`/`RopeSlice`，仍是堆；见 §7.1） |
| 容器 | `Vec` + `remove(0)`（堆） | `VecDeque` 从头丢（堆） |
| 上限 | 只限 100 条 | 条数 + 字节（可配） |
| 大文件本体 | `Signal<String>` 整串（堆） | L3 才换 rope（仍在堆，少复制） |

换方案改的是**怎么记、记多少、何时丢掉、编不编进二进制**，不是从调用栈搬到堆。

真正要管的预算：

| 预算 | 为什么 |
|------|--------|
| **条数** | 可预测的「能撤几步」 |
| **字节** | 一条全选粘贴可能大于 100 条打字 |
| **文档本体** | `String::insert` 在百万字符上是 O(n) 拷贝 |
| **打开文件数** | 每资源一份历史；关页必须释放 |
| **回环** | `undo()` → `on_change` → `record()` 会污染栈或同步重入 |
| **是否链进二进制** | `text-undo` 关闭则上述预算为 0 |

---

## 3. 公开配置（应当暴露）

feature 打开后，用 `TextUndoConfig` + builder 暴露**少量**参数。不配也能用；编辑器或内存敏感应用再改。

### 3.1 应当公开

| 参数 | 作用 | 默认（feature 开启） |
|------|------|----------------------|
| `undo_enabled` | 该字段是否记账 | `true`；`type=password` 视为 `false` |
| `max_undo_entries` | 最多几步 | Input 200；Textarea 500；文档会话 1000 |
| `max_undo_memory` | 历史占用字节（**不含**文档本体） | Input 1–2 MiB；Textarea 8–16 MiB；会话 32–64 MiB |
| `typing_merge_timeout` | 连续打字/删除合成一步的时间窗 | 800ms（与现状 `COALESCE` 一致，可改为 500–800） |

后加、先不必进第一版公开面：

- 单次大替换阈值（超过则记 Marker、不进两端全文，见 §9.2；**不是** `ClearHistory`）
- `SetTextUndoPolicy`（这是写入 API 语义，不是滑条）

### 3.2 不应当公开

- `applying` / `is_internal_sync`、合并 kind 矩阵、`VecDeque` 实现
- 撤销 / 恢复分成两个独立上限（易配出「能撤五步、只能恢复一步」）
- rope 节点大小、piece table 块参数
- 密码框「仍然保存明文历史」作为普通默认（若提供，必须显式且失焦即清）

### 3.3 钳位

实现时拒绝或钳到默认，避免无声坏掉：

- `max_undo_entries`：至少 1，建议上限（如 10_000）防止误写成 `usize::MAX`
- `max_undo_memory`：至少能放下一步中等粘贴（如 ≥ 64 KiB）；0 视为「只用条数上限」或拒绝，**不要**变成「刚打完字立刻不可撤」且不说明
- `typing_merge_timeout`：0 表示不按时窗合并（仍按 kind / 光标相邻规则）

Builder 示意（双书写落地时再对齐命名）：

```text
input("x")
    .undo_enabled(true)
    .max_undo_entries(200)
    .max_undo_memory(1 << 20)
textarea("bio").typing_merge_timeout_ms(800)
```

---

## 4. 现状基线（2026-09-09）

已落地（表单级 MVP，**尚未**套 `text-undo` feature）：

| 项 | 现状 |
|----|------|
| 存储 | 每 `field_id` 一份 `FieldHistory`（thread-local `HashMap`，**堆**） |
| 记录 | **全文快照** `TextEditSnapshot { value, caret, selection }` |
| 合并 | 连续 `Insert` / `Delete` / `Step` 在 800ms 内合成一步；`Replace`（粘贴/剪切/选区删/IME 上屏）不与打字合并 |
| 上限 | `past` 最多 100 步；超出 `Vec::remove(0)`；**无字节上限、无公开配置** |
| 快捷键 | Ctrl/Cmd+Z 撤；Ctrl/Cmd+Y 与 Ctrl/Cmd+Shift+Z 恢复；组字中不抢 IME |
| IME | preedit 不进栈；`Ime::Commit` / 失焦折入记 `Replace` |
| 程序化 `commit_value` | **不**自动入栈；有 `reset(field_id)` 但 Combobox 等路径尚未普遍调用 |
| 值本体 | Input/Textarea 仍是 `__rui_input_value_{id}` 的 `Signal<String>` |
| 绘制 | Textarea ≥64KB / ≥2048 视觉行才走高度估算虚拟化；缓冲本身仍是整串 |

已知缺口（相对本稿目标）：

- **未**独立 feature；lean 二进制也链入历史路径。
- 没有 **字节上限** / **公开配置**。
- 没有 **差量**；每次 `value.clone()`。
- 没有 **`applying_undo` / `is_internal_sync`**：靠「不要从 `commit_value` 记账」约定，撤销或受控回写容易污染历史或重入。
- **密码框仍记栈**（本地明文）。
- 历史在 **进程级 thread-local `HashMap`**，**不随 Owner/字段卸载淘汰**（动态 `field_id` 会漏）。
- `redo` 推 `past` 时未再检查上限（被「最多撤 100 次」间接卡住）。
- 无法把缓冲换成 rope；CodeEditor / 多文件不存在。

对照：Freya 有 `freya-edit` + ropey + `code-editor`；gpui-component 把编辑器/LSP 放在庞大的 input 目录。RUI 表单控件更完整，**编辑器层几乎空白**——本稿补的是这条缺的协议，而不是把 VSCode 搬进 `rui` 默认 crate。

---

## 5. 目标与非目标

### 目标

- **编译期可选**：`text-undo` 独立 feature，默认关闭。
- feature 开启后，表单 Input/Textarea：**开箱可撤销**（打字、删除、粘贴、剪切、选区替换、数字步进、清除）。
- **可配上限**：条数、历史字节、合并时间窗。
- 机制 **可插拔**：应用或未来 CodeEditor 可换 `UndoManager` / `TextBuffer`，不必 fork `input_controller`。
- 接口预留到：**多文件、大缓冲、编辑事务、LSP/格式化一组撤销**。

### 非目标（本稿与近期实现）

- 把撤销打进 `default` feature（与 lean crate 纪律冲突）。
- 拆成 `undo` / `redo` 两个 Cargo feature。
- 在 `rui` 核心里做完整 IDE（LSP、多光标、Minimap、Git blame）。
- 撤销树 / 非线性时间线（VSCode 也是线性栈 + 新编辑清 redo）。
- 协同 CRDT / OT（可在 TextChange 日志上后接，不进默认实现）。
- 用递归命令树表达每一次按键。
- 把百万行文件放进 `Signal<String>` 再靠虚拟化「看起来能编辑」。

### 产品分档

| 档 | 谁用 | 缓冲 | 历史 | 何时有 |
|----|------|------|------|--------|
| **L1 表单** | Input、短 Textarea、搜索、数字 | `String` | 快照或小差量 | `text-undo` |
| **L2 长 Textarea** | 备注、Markdown、日志粘贴 | `TextBuffer`（可仍是 String 实现） | 差量 + 双上限 | `text-undo` |
| **L3 文档会话** | 类 VSCode 页签 / Cursor 多文件 | rope / piece table | 每 URI 一份 + 工作区注册表 | `text-undo` + 编辑器层；rope 另 `text-undo-rope` |

L1 可以继续用今天的快照，只要套上 feature、字节上限、密码策略、回环标志。L1/L2 **禁止**再依赖进程级 thread-local 历史 Map；L3 用显式 `DocumentRegistry`。

---

## 6. 分层：机制 / 默认 / 会话

```text
┌─────────────────────────────────────────────────────────┐
│  L3  DocumentRegistry（可选，编辑器层）                    │
│      uri → DocumentSession { buffer, undo, view_state }  │
└───────────────────────────┬─────────────────────────────┘
                            │ 实现
┌───────────────────────────┴─────────────────────────────┐
│  L0  协议（cfg text-undo）                                │
│      TextBuffer · UndoManager · UndoableTextEdit         │
│      SetTextUndoPolicy · EditTransactionGuard · TextUndoConfig │
└───────────────────────────┬─────────────────────────────┘
                            │ 默认实现
┌───────────────────────────┴─────────────────────────────┐
│  L1  Input / Textarea / input_controller / IME           │
│      快捷键、合并、密码、受控 Signal 桥、is_internal_sync     │
└─────────────────────────────────────────────────────────┘
```

与 [`runtime_extension_points.md`](../engine/runtime_extension_points.md) 的 `TextEditHost` 对齐：runtime 只认 **焦点前缀 + 缓冲/历史协议**，不写死「值一定是 String Signal」。无 `text-undo` 时 Host 只有 caret/IME，没有历史。

---

## 7. 核心模型

### 7.1 `TextBuffer` 与 slice 类型

撤销只通过缓冲接口读写。**`Arc<str>` 不是 String 上的零拷贝**：从大 `String` `slice` 再 `Arc::from` **必拷一份**；只有多处共享同一块、或 rope 的结构共享才省内存。因此 slice 用关联类型，L3 才能换成 `RopeSlice`：

```rust
pub trait TextBuffer {
    type Slice<'a>: AsRef<str> + Clone
    where
        Self: 'a;

    fn len(&self) -> usize;
    fn is_empty(&self) -> bool;
    fn text(&self) -> std::borrow::Cow<'_, str>;
    fn slice(&self, range: std::ops::Range<usize>) -> Self::Slice<'_>;
    fn replace(&mut self, range: std::ops::Range<usize>, insert: &str);
}
```

约定：`range` 为 **UTF-8 字节**，且落在字符边界（与现 caret 一致）。  
`StringBuffer::Slice` 可以是 `Arc<str>` 或 `String`（拷贝可接受）；`RopeBuffer::Slice` 为零拷贝视图（历史里要 `'static` 所有权时再 `Arc` 或 rope 句柄）。

L1 可用「`Signal<String>` 适配器」。实现演进：`StringBuffer` → `RopeBuffer` / `PieceTableBuffer`。

### 7.2 `TextChange`：瞬时坐标（禁止「组开始基准 + 正序 replace」）

```rust
pub struct TextChange<S> {
    /// **该 Change 发生当下**、应用 `new_text` 之前的替换区间（瞬时坐标）。
    pub range: std::ops::Range<usize>,
    pub old_text: S,
    pub new_text: S,
}
```

- 插入：空区间，`old_text` 空。
- 删除：`new_text` 空。
- 替换：两端都有内容。

**旧稿「一组内多段都记组开始时的基准坐标」是错的。** 正序执行第一条会改文档长度，第二条的基准 `range` 会偏，轻则改错字，重则越界 panic。

正确语义：

| 方向 | 遍历 | 坐标 |
|------|------|------|
| 正常编辑 / Redo | **正序** | 每条 `range` 都是**当时**文档上的瞬时位置，直接 `replace` |
| Undo | **严格倒序** | 倒着撤时，文档长度回到该条记录时的状态，瞬时 `range` 再次合法 |

**L1 / L2 默认：一个 `UndoGroup` 只含一条 `TextChange`。** 连续打字的合并是**加长这一条**的 `new_text`（或前移删除区间），不是往组里 append 第二条。多光标、一次格式化多段，留到 L3 或显式事务。

L3 同一时刻多段（多光标）：要么按瞬时顺序记录并 Undo 倒序；要么先换成「组开始前基准坐标」并在 **应用时从后往前**（按 `range.end` 降序）以免偏移。两种都不要「基准坐标 + 正序 apply」。

跨组只靠后进先出。组与组之间不做 OT。

### 7.3 UndoGroup、编辑点、线性栈

命名避免写反：

```rust
pub struct EditPoint {
    pub caret: usize,
    pub selection: Option<(usize, usize)>,
}

pub struct UndoGroup<S> {
    pub payload: UndoPayload<S>,
    /// 本组**发生前**的 caret/选区。**Undo 后**恢复到这里。
    pub state_before: EditPoint,
    /// 本组**发生后**的 caret/选区。**Redo 后**恢复到这里。
    pub state_after: EditPoint,
    pub kind: EditKind,
    pub at: std::time::Instant,
}

pub enum UndoPayload<S> {
    /// L1/L2：通常 `changes.len() == 1`。
    Changes(Vec<TextChange<S>>),
    /// 超过硬顶：不保存两端全文，也不清空更早的历史。见 §9.2。
    LargeReplaceMarker { old_hash: u64, new_hash: u64, byte_len: usize },
}

pub enum EditKind {
    Insert,
    Delete,
    Replace,      // 选区替换、IME 上屏
    Paste,
    Cut,
    Step,         // 数字步进
    Programmatic,
    Other,
}
```

`UndoManager` 持有 `VecDeque<UndoGroup<_>>`、`redo: Vec<_>`、`config`、`current_bytes`、`applying`、`is_internal_sync`。新用户编辑 `redo.clear()`。超条数或字节则 `pop_front`。

`applying == true` **或** `is_internal_sync == true` 时 `record` 直接返回。

### 7.4 所有权：P1 废弃进程级 Map

RUI 是 Solid 式重建，**不能**指望 `Input` 结构体字段跨帧活着，但**也不能**用进程寿命的 `thread_local HashMap<field_id, History>`：动态 `For` 行、销毁后 id 复用都会泄漏或串栈。

| 层 | 历史放哪 |
|----|----------|
| L1/L2 | **Owner 绑定状态**：与该字段同一 Owner 的 `Signal<UndoManager>`（或等效 slot）。Owner cleanup / 控件卸载时 Drop，历史释放。 |
| 稳定 `id` 的页签切换 | 仅当产品要「切走再切回仍可撤」时，才把 manager 放进 L3 `DocumentRegistry`，显式 `unregister` 或 `Weak`。 |
| 禁止 | 无 cleanup 的全局 `HISTORIES` Map |

`field_id` 仍可作信号键前缀，但生命周期必须等于字段 Owner，而不是 UI 线程活多久 Map 就活多久。

### 7.5 程序化写入与内部同步隔离

```rust
pub enum SetTextUndoPolicy {
    /// 不入栈（表单绑定、外部 store 覆盖）。今日 `commit_value` 默认。
    NotUndoable,
    /// 作为一步可撤销编辑（查找替换、自动补全、格式化）。
    Undoable,
    /// 写入后清空该资源历史（reset 表单、**换文档**）。不要用于「一次大粘贴」。
    ClearHistory,
}

pub enum EditOrigin {
    User,
    UndoRedo,
    /// buffer ↔ Signal 镜像，禁止再 record。
    InternalSync,
    External, // 失焦拉取等
}
```

仅靠「失焦才 `NotUndoable`」不够。焦点内典型回环：

```text
按键 → buffer.replace → signal.set → 重建/effect
    → 再 replace 整份值 → record 当成新用户编辑
```

控件必须维护 **`is_internal_sync`**（可与 `applying` 分立）：

- 用户输入：`record` → `replace` → 设 `is_internal_sync` → `signal.set` → 清标志。
- Signal 外来变化：设 `is_internal_sync` → `replace` 对齐 buffer → 清标志；按 `SetTextUndoPolicy` 决定是否 `record`（焦点内外来覆盖通常 `NotUndoable`）。
- Undo/Redo：`applying=true` → 改 buffer 与 caret → mirror Signal → 清标志。

测试：受控 `value_signal` 在用户输入后不得使 undo 栈多出一步「相同文本」。

### 7.6 事务：RAII，禁止裸 begin/end

裸 `begin_edit_group` / `end_edit_group` 在 panic 或提前 return 时会留下 `open_group`，之后所有打字并进同一组。

```rust
pub fn edit_group(&mut self, kind: EditKind) -> EditTransactionGuard<'_> { /* 开启 */ }

impl Drop for EditTransactionGuard<'_> {
    fn drop(&mut self) {
        // 正常路径 commit；panic 时也必须收口，不得把后续无关按键继续吸入。
        // 实现二选一并写进测试：commit 已记录差量，或 rollback 到 guard 创建时的缓冲快照。
    }
}
```

P2 起公开 Guard；若保留 `begin`/`end`，只作 Guard 内部实现，**禁止**作为跨调用的公开 API。失焦时若仍有开着的 Guard，Drop 会收口。

用于：自动补全、格式化、IME 一次 Commit 多段、L3 多光标。L1 连续打字**不**开事务，只靠合并加长单条 `TextChange`。

事务内多条 `TextChange` 仍遵守 §7.2：每条记瞬时坐标；Undo 倒序、Redo 正序。

---

## 8. 合并策略（撤销单位）

目标：一次 Z 撤的是 **用户心里的一步**，不是一个码点。

| 规则 | 行为 |
|------|------|
| 时间窗 | `typing_merge_timeout`（默认 800ms）；超时开新组 |
| 同类 | 仅 `Insert`↔`Insert`、`Delete`↔`Delete`、`Step`↔`Step` 可合并 |
| 粘贴 / 剪切 / 选区替换 | **永不**与打字合并（`Paste` / `Cut` / `Replace`） |
| 光标跳跃 | 插入点与上一组终点不相邻 → 新组 |
| IME | 组字过程不记账；一次 `Commit` = 一组 `Replace` |
| 事务未结束 | Guard 存活期间只往当前组追加，不按时间窗拆；Guard Drop 即收口 |

连续 Backspace 合成一步：一次 Z 回到删除爆发前。这是正确默认；不需要每字符一条。

---

## 9. 内存与大编辑

### 9.1 双上限（必做，可配）

见 §3.1 默认值。只限条数不够：全选 10MB 再粘贴 10MB，一条就能撑满。

估算：`old_text`/`new_text` 的逻辑长度 + 元数据。`String` 实现上 slice→`Arc<str>` **仍按拷贝后的字节计**；合并插入若真共享同一 `Arc`，估算可按引用计，淘汰仍按组。L3 rope 共享后再改估算公式。

### 9.2 大替换降级（禁止为此清空历史）

| 单次 `old+new` | 策略 |
|----------------|------|
| &lt; 阈值（如 1 MiB） | 正常记差量 |
| 阈值～硬顶（如 16 MiB） | 单独成组，仍可撤这一步 |
| 超过硬顶 | **不把两端全文放进历史**；推入 `UndoPayload::LargeReplaceMarker { old_hash, new_hash, byte_len }`。**禁止**对此调用 `ClearHistory`：前面已记下的小步必须保留 |

`ClearHistory` **只**用于换文档、reset 表单、密码失焦清空，**不是**大粘贴的降级手段。一次 20MB 替换若清掉前面 100 步，是 UX 灾难。

用户对 Marker 按 Z 时的产品选择（实现期钉一种并测）：

1. **跳过该步**：弹出或状态栏说明「此步修改量过大，已跳过」，继续撤更早的组；或
2. **确认后跳过**（对话框）；或
3. L3 rope 已能廉价保留差量时，本档降级可取消。

Marker 不得假装能还原那一次大替换的文本（除非 L3 用结构共享真正记下了）。栈与文档的「一致性」靠「这一步不可逆还原」表达，而不是把更早历史删掉。

L3 用 persistent rope 时，大替换的「旧文本」可由缓冲结构共享，不必复制 100MB 进 `old_text`；那是 `text-undo-rope` 的收益。P3 **先**对 `String::insert` vs `Rope::insert` 在 1MB / 10MB / 50MB 上做 bench，再用数据决定是否换缓冲。

### 9.3 文档本体

L3 禁止：

```text
每键：document.clone() → Signal::set(整份)
```

视口虚拟化只解决**绘制**，不解决 **O(n) 插入** 和 **O(n) 快照**。大文件必须先换 `TextBuffer`。

---

## 10. 密码与安全

`type=password`（即使开了 `text-undo`）：

- **默认禁用撤销**（不写历史；Z/Y 仍消费按键，避免把 `z` 打进字段）。
- 若产品坚持可撤：须显式 API，历史仅存活于焦点内，**失焦 / 提交 / 切换 reveal 为隐藏** 时 `clear_undo_history`。
- 禁止把密码历史打进日志 / DevTools 文本转储。

OTP 分格输入不走文档栈。

---

## 11. 接到现有运行时

全部记账点包在 `cfg(feature = "text-undo")` 内。

| 路径 | 规则 |
|------|------|
| `input_controller` 用户键 | `record` + `buffer.replace`；已有 `commit_user_edit` 迁到差量 |
| IME Commit / 失焦折入 | 一组 `Replace`；preedit 忽略 |
| 选区拖拽移动 | 一组（删旧 + 插新，或一条 replace） |
| 清除按钮、数字步进 | `Replace` / `Step` |
| `commit_value` / 受控拉取 | `is_internal_sync` 下对齐 buffer；策略 `NotUndoable`（失焦拉取）或 `ClearHistory`（**换文档/reset**，不是大粘贴） |
| 用户编辑后 `signal.set` | 必须在 `is_internal_sync` 内，禁止回写再 `record` |
| `undo` / `redo` | `applying=true` → 只改 buffer + caret（Undo 倒序 changes）→ 再 `mirror_store_to_controlled` |
| 字段卸载 | Owner Drop → `UndoManager` 释放；**禁止**依赖进程级 Map 的 `reset` 当唯一回收。L3 关页签时从 Registry `unregister` |

快捷键保持：Win/Linux `Ctrl+Z` / `Ctrl+Y` 与 `Ctrl+Shift+Z`；macOS `Cmd` 同理；`NamedKey::Undo/Redo`。组字中不抢。无 feature 时仍消费 Z/Y，不改值。

Tab 全选仍是选区，不是一步历史。

---

## 12. 类 VSCode / Cursor：多文件会话

这是 L3，**不是**给 Input 加的 API。依赖 `text-undo`（及可选 `text-undo-rope`）。

### 12.1 每资源一份历史

VSCode / Monaco：每个文本模型（文件 URI）自己的 undo/redo service。Cursor 同理（编辑器内核同源思路）。

```text
DocumentRegistry
  "file:///src/main.rs" → Session { buffer, undo, view_state }
  "file:///src/lib.rs"  → Session { … }
```

- **焦点在哪个页签，Z/Y 就打哪份栈。** 不是全应用一条全局栈（除非以后做「工作区级操作」如重构，那是另一类 Command）。
- **保存不清历史**（与 VSCode 一致）。
- **关闭页签释放 Session**（内存主阀门）。可配置「关闭后保留 N 分钟」但不作为默认。
- **磁盘上 1000 个文件**只对**打开中的缓冲**付费；未打开的不占撤销内存。

### 12.2 工作区级 vs 文档级

| 操作 | 栈 |
|------|----|
| 打字、删除、粘贴 | 文档 Session |
| 格式化整个文件 / 组织 import | 该文档一个 `Programmatic` 组 |
| 跨文件重构 | 将来：工作区 `WorkspaceCommand`（可逆文件集），**不要**塞进某个 Textarea 的 undo |

### 12.3 编辑器能力的接入顺序

1. 单缓冲差量撤销（L2 Textarea 已够用）。
2. `TextBuffer` 换 rope/piece table（`text-undo-rope`），打开 10–50MB 文本可插删。
3. `DocumentRegistry` + 多页签。
4. 多光标 = 一组多 `TextChange`（瞬时坐标 + Undo 倒序，或基准坐标 + **从后往前** apply；见 §7.2）。
5. LSP `applyEdit` = `edit_group()` RAII Guard（禁止裸 begin/end 跨调用）。
6. 协同 / 撤销树：明确不做，除非另开 ADR。

CodeEditor 组件（对照 Freya F05）应是 **另 crate 或 `components/code_editor`（planned）**，皮肤用 RUI，内核用 L0+L3，而不是把 LSP 写进 `input.rs`。

---

## 13. 分期（只设计，不排 AUTHORITY WP）

实施时再在 AUTHORITY 挂号。建议顺序：

| 阶段 | 内容 | 验收 |
|------|------|------|
| **P0** | 现快照路径套上 **`text-undo` feature**；Gallery 打开、lean 示例关闭；补测试：删除/粘贴/IME/撤销后新编辑清 redo；无 feature 时 Z 不插入字母 | `cargo test -p rui --lib` 默认绿；`--features text-undo` 历史测绿 |
| **P1** | 公开 `TextUndoConfig`；`applying` + **`is_internal_sync`**；`SetTextUndoPolicy`；**字节上限**；密码默认关；`VecDeque`；**废弃 thread-local 历史 Map，改为 Owner 绑定 `UndoManager`** | 可配条数/内存；控件卸载后历史释放（动态列表不泄漏）；受控 Signal 回写不污染栈；密码无历史 |
| **P2** | `TextChange` 差量（瞬时坐标、一组一步）；Textarea 默认走差量；**`EditTransactionGuard` RAII**（替代公开 begin/end）；超大替换 **Marker** 而非 `ClearHistory` | 长备注粘贴/全选删可撤；合并策略测全；事务 panic/提前 return 不悬挂；大替换后仍能撤更早的小步 |
| **P3** | `TextBuffer` + String 实现；**先 bench** `String::insert` vs rope（1MB / 10MB / 50MB）再决定 `text-undo-rope` | 有数据再换缓冲；百万字符插入不掉帧到不可用 |
| **P4** | `DocumentRegistry`（显式 register/unregister 或 Weak）；CodeEditor 实验面；多文件关页释放 | 多页签各自 Z/Y；关文件内存回落 |

P0–P1 不挡现有 Beta 表单（默认二进制行为 = 今天**关掉**历史，与 lean 一致；要旧 MVP 手感则开 feature）。P3–P4 是编辑器产品，不挡 Input verified。

---

## 14. 建议 API（`cfg(feature = "text-undo")`）

```rust
pub struct TextUndoConfig {
    pub enabled: bool,
    pub max_entries: usize,
    pub max_bytes: usize,
    pub typing_merge: std::time::Duration,
    pub secure_disable: bool,
}

pub trait UndoableTextEdit {
    fn can_undo(&self) -> bool;
    fn can_redo(&self) -> bool;
    fn undo(&mut self);
    fn redo(&mut self);
    /// 公开事务入口。返回 Guard，Drop 时 commit/rollback。
    fn edit_group(&mut self, kind: EditKind) -> EditTransactionGuard<'_>;
    fn clear_undo_history(&mut self);
}
```

可插拔：`with_undo_manager(impl UndoManager)` / `with_buffer(impl TextBuffer)` 给 L3；L1 不暴露也能工作。

---

## 15. 测试清单（实现阶段用）

**feature**：默认构建无历史符号（或模块 cfg 掉）；`--features text-undo` 下行为完整；无 feature 时 Ctrl+Z 不插入 `z`。

**配置**：改 `max_entries` / `max_bytes` 后淘汰最旧；非法 0 被钳位。

**操作**：插入、Backspace、Delete、选区替换、粘贴、剪切、全选删除、清除、步进、IME Commit、选区拖拽移动、程序化 `NotUndoable` / `Undoable` / `ClearHistory`。

**栈**：撤再恢复；恢复后再打字 redo 为空；连续打字一步撤；连续删一步撤；粘贴不与前后打字合并；光标跳开后新组。

**回环**：`applying` 期间受控 effect 不得 `will_user_edit`。用户输入 → `signal.set` → 重建后 **栈深度不变**（`is_internal_sync`）。

**坐标 / 事务**：组内多 change 时 Undo 倒序、Redo 正序不越界；Guard Drop（含 panic unwind）后下一次打字是新组。

**所有权**：卸载控件 / 动态列表移除行后，不得残留该 `field_id` 的历史（无全局 Map 条目）。

**边界**：空文档 Z；栈空仍消费快捷键；密码无历史；超 `max_bytes` 丢最旧（`pop_front`）；超大替换记 Marker **且**更早小步仍可撤；`ClearHistory` 不用于大粘贴。

**会话（P4）**：两文件交叉编辑，Z 只影响当前；关 B 不影响 A；save 后仍可撤。

**性能（P3）**：换 rope **之前**先跑 1MB / 10MB / 50MB 的 `String::insert` vs `Rope::insert` bench，把数字写进实现笔记；大缓冲中间插入的复杂度在选定实现后钉住。

---

## 16. 与「业内编辑器」对齐的一句话

VSCode / Monaco / Cursor 不是「无限 `Vec<String>` 全文」。而是：

```text
高效文本模型（piece tree / rope）
+ 范围替换日志
+ undo group（连续输入合并，粘贴/IME/格式化单独成组）
+ 每文本模型一份服务
+ 内存与关闭页签回收
```

RUI 先把能力收进 **`text-undo` feature**（堆上差量 + 可配双上限 + Owner 持有历史 + 可插拔缓冲）；大文件与多文件是 L3，不把 IDE 做进 `Input`，也不打进默认 crate。实现时以 **§0 落地必守** 为准：瞬时坐标、内部同步隔离、RAII 事务、大替换 Marker。
