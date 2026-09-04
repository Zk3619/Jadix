你的判断是对的。`window_events.rs` 里确实有不少**按组件名硬编码**的逻辑，这在当前 monolithic `rui` 里能跑，但和「组件将来拆出 crate」的目标不一致。

## 现状：它在做什么

你光标附近这段是典型的「帧级轮询清单」：

```1151:1167:crates/rui/src/app/window_events.rs
        let slider_dragging = crate::components::slider::tick_slider_drag(slider_drag_x);
        let toast_dragging = crate::components::toast::tick_toast_drag(x, y);
        let color_dragging = crate::components::color_picker::tick_color_drag(x, y);
        let carousel_dragging = crate::components::carousel::tick_carousel_drag(x, y);
        let resizable_dragging = crate::components::resizable::tick_resizable_drag(x, y);
        if slider_dragging
            || toast_dragging
            || color_dragging
            || carousel_dragging
            || resizable_dragging
            || drawer_dragging
            || crate::components::internal::modal_motion::tick_modal_presence()
            || crate::components::accordion::tick_accordion_panel_presence()
            || crate::primitives::canvas::take_canvas_wants_redraw()
        {
            window.request_redraw();
        }
```

同类模式还出现在：

| 位置 | 模式 |
|------|------|
| `window_events.rs` mouseup | 逐个 `clear_*_drag_if_any()` |
| `lifecycle.rs` | `tick_toast_lifecycle`、`tick_carousel_autoplay`、`tick_number_step_hold` |
| `state.rs` 帧同步 | 逐个 `sync_*_subtree`（button、tabs、checkbox…） |
| `state.rs` hoist 后 | `modal_motion::flush/tick`、`accordion::tick` |

本质都是：**runtime 知道「有哪些组件需要在每帧/每次指针事件时被推进」**。

## 为什么会变成这样

这是典型的**增量演进**，不是刻意设计：

1. **早期缺通用扩展点** — 没有 `FrameService` / hook registry，新组件只能直接在 `window_events` 加一行 `tick_xxx`。
2. **拖拽各自为政** — Slider、Drawer、Resizable 各有一套全局 drag 状态 + `tick_*`，runtime 只负责问「要不要 redraw」。
3. **Input/Scrollbar 更特殊** — 它们和 hit-test、IME、focus 强耦合，逻辑直接嵌在事件路径里（这不只是 tick 问题）。
4. **Modal/Accordion 的 presence** — 已有 `PRESENCE_REGISTRY` 雏形（`modal_motion` 内部注册），但 **tick 入口仍暴露在 app 层**，没有统一调度。

所以不是「应该针对组件处理却写错了」，而是**还没有抽象成 runtime 协议，暂时用显式调用堆出来的**。

## 应该怎样才通用

拆 crate 后，`rui`（core/runtime）不应 `use rui_components::slider`。更合理的分层：

### 1. 帧调度：`FrameLoop` + 注册表

```rust
// rui crate — 不知道具体组件
trait FrameHook: 'static {
    fn on_pointer_move(&mut self, ctx: &mut PointerFrameCtx) -> FrameAction;
    fn on_idle_tick(&mut self, now: Instant) -> FrameAction;
}

struct FrameAction {
    needs_redraw: bool,
    next_wake: Option<Instant>,
}
```

组件 crate 在 init 时 `register_frame_hook(Box::new(SliderDragHook))`。  
`window_events` 只做：

```rust
for hook in frame_hooks.iter_mut() {
    if hook.on_pointer_move(ctx).needs_redraw { request_redraw(); }
}
```

`modal_motion::tick_modal_presence`、`accordion::tick` 都变成 hook，而不是 app 直接 import。

### 2. 拖拽：统一的 `PointerCapture` 协议

现在每个组件有 `tick_*_drag` + `clear_*_drag_if_any` + `is_*_dragging`。应合并为：

- 按下时 `capture(CaptureKind::Drag { id, handler })`
- move 时分发给当前 capture
- 抬起时 `release_capture()`

runtime 只维护**一个** capture 栈，不再枚举 slider/drawer/resizable。

### 3. 绘制同步：已有方向，但未统一

`state.rs` 里大量 `sync_*_subtree` 和 `ReactiveSync` 是同一类问题。理想状态：

- 组件通过 `Element::reactive_sync` 或 **paint pass 注册** 自同步
- runtime 只跑 `sync_reactive_elements_subtree_ex`，不逐个 import button/tabs/checkbox

Modal motion 的 `flush_modal_shell_motion_subtree` 更适合作为 **portal hoist 后的 layout hook**，而不是 `state.rs` 硬编码调用。

### 4. Input / Scrollbar：协议可以 generic，实现可外置

Input 的 signal key、IME、caret 和 runtime 绑定较深，但 app 层应只认：

- `FocusId` 前缀协议（`__rui_input:` 等）
- `TextEditHost` trait

具体 `input_value_signal_key` 留在 `rui-input` crate，runtime 通过 trait 或注册表调用。

## 哪些已经部分通用

| 机制 | 状态 |
|------|------|
| `ReactiveSync` | 组件自管 sync，runtime 遍历触发 — **正确方向** |
| `modal_motion::PRESENCE_REGISTRY` | 内部注册，但 tick 入口仍漏在 app — **半成品** |
| `portal_sync` / overlay stack | app 层通用，不依赖 Dialog 实现 — **较好** |
| `request_redraw()` | 通用 — **OK** |
| 逐组件 `tick_*` / `sync_*` | 硬编码 — **待重构** |

## 和 Dialog  bug 的关系

Modal 问题根源在 **motion 构建/hoist 时序**，不在 `window_events` 是否 call `tick_modal_presence`。  
但 `tick_modal_presence` 放在 `window_events` 说明：**presence 生命周期没有纳入统一帧模型**，这也是架构债的一部分。

## 建议的演进顺序（若准备拆 crate）

1. **先抽 `app::frame_hooks`** — 把 `window_events` + `lifecycle` 里所有 `tick_*` / `clear_*` 迁入注册表；行为不变，删掉 app 对组件名的 import。
2. **统一 PointerCapture** — 合并 drag 系列，减少 mousemove/mouseup 分支。
3. **hoist/post-hoist hooks** — `flush_modal_shell_motion` 等作为 `LayoutPassHook`。
4. **Input/IME 协议化** — 最后动，因为和 `tree_hit`、`input_controller` 耦合最深。

---

**结论**：你的直觉正确 — `window_events.rs` 不应长期保留「组件点名簿」。这是 monolith 阶段的权宜之计；拆 crate 前应先抽出 **runtime 扩展点（frame hook + pointer capture + layout pass hook）**，让组件自注册，而不是让 app 认识每个组件。

如果你希望，我可以先做一个最小 refactor：新增 `app/frame_hooks.rs`，把 mousemove 里那 8 个 tick 和 mouseup 里的 6 个 clear 迁过去，作为拆 crate 的第一步。