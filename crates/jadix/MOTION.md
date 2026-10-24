# Jadix 组件级 Motion 动画

启用 `animation-ex` 使用交互 Motion 与自动布局投影；`animation` 是包含基础层与增强层的聚合入口。
路径跟随／描边／morph 另需 `animation-path`，它传递开启增强层但不要求 SVG。
默认仅开启 `animation-base`，包含 CSS 动画、Spring/tween/inertia、AnimatedNumber 与 Timeline。
关闭基础层时应用静态目标并立即完成 presence；未开启增强层时布局和 SVG 图标自动 morph 直接应用目标。
SVG 图标绘制仍需独立的 `svg`。完整能力边界见仓库 `docs/FEATURES.md`。

`jadix::motion` 在现有 CSS transition / keyframes、`AnimatedNumber`、spring / inertia、
timeline 和 retained 元素树之上提供组件绑定，不另起时钟，也不引入依赖。

参考：

- [Motion](https://github.com/motiondivision/motion)：声明式目标、手势优先级、可中断弹簧、布局投影。
- [gpui-motion](https://github.com/1bye/gpui-motion)：持久化状态、原生输入绑定、布局位置、预处理矢量路径。

## 交互与绘制样式

```rust,no_run
# #[cfg(feature = "animation-ex")]
# {
use jadix::prelude::*;
use jadix::style::{SpringOptions, ValueDriver};

let state = use_motion("save-motion");
let button = Element::new()
    .focus_id("save")
    .focusable(true)
    .on_click(|| {})
    .motion(&state,
        Motion::new(MotionTarget::new().opacity(1.0))
            .initial(MotionTarget::new().opacity(0.0))
            .while_hover(MotionTarget::new().scale(1.04)
                .background_color([0.1, 0.4, 0.9, 1.0]))
            .while_tap(MotionTarget::new().scale(0.96))
            .while_focus(MotionTarget::new().border_radius(12.0))
            .transition(ValueDriver::Spring(SpringOptions::visual(0.3, 0.15))));
# }
```

- 优先级：普通目标 < 键盘焦点 < hover < tap < drag。不同属性叠加，同一属性由高优先级接管。
- `translate`、`scale_xy`、`radii`、颜色等数组是整个属性的目标，不是逐分量的可选覆盖。
- 离开 hover / 释放指针后恢复低优先级目标；没有目标的属性恢复首次挂载的作者样式。
- `while_tap` 支持主指针按钮；可聚焦且有 `on_click` 的元素也支持 Space / Enter，保留原有点击语义。
- `while_focus` 跟随框架的键盘 focus-visible；不会擅自创建 tab stop。
- `.disabled(true)`、`disabled`、`aria-disabled="true"`、`data-disabled` 取消交互姿态，不取消普通目标。
- `state.set_target(...)` 原地切换目标；`state.set_dragging(...)` 可由既有拖拽回调驱动 `while_drag`。
- 既有 hover、鼠标、键盘与 `reactive_sync` 回调均组合执行，不覆盖组件回调。
- 复用状态句柄时从实时位置接续，弹簧保留速度；相同目标不重新计时。`.delay(ms)` 只用于初始动画。
- `width` / `height` / `padding` / `gap` 会重排；颜色、圆角、透明度及变换只重绘。
- 大小目标为逻辑像素。首个未指定数值起点的大小立即应用；自动/百分比基线恢复时立即还原其声明。
- 不要让 CSS transition、其他 motion 或自定义同步回调同时写同一属性。先配置作者样式，再绑定 motion。

## 布局位置与大小

```rust,no_run
# #[cfg(feature = "animation-ex")]
# {
use jadix::prelude::*;

let layout = use_layout_motion("card-layout", LayoutAnimation::Both);
let card = Element::new().w(240.0).h(100.0).layout_motion(&layout);
# }
```

保持每个列表 key 对应独立且稳定的 `LayoutMotion`，正常修改 flex/grid 排列、间距、尺寸或顺序即可。
新布局一次提交，绘制和命中通过独立 projection 插值，不把动画中间值写回 CSS 或 Taffy。
`Position` 只平移、不拉伸文字；`Size` 只投影大小；`Both` 同时处理位置与大小。
首帧不从原点飞入，零尺寸安全退化，动画中再次布局从当前呈现矩形续接。
位置使用父局部坐标，因此父子同时平移不会叠加同一段位移，滚动本身也不会触发布局补间。

没有绑定投影的后代仍随父容器缩放。给需要保持字号、图标或圆点尺寸的子节点绑定
`LayoutAnimation::Position`：系统取消祖先的布局投影缩放，并同时补偿位置；作者 CSS scale 仍保留。
给子节点绑定 `Both` 则独立插值子节点自己的大小，不会重复乘上祖先的布局大小投影。
投影不改变 Taffy 的排版结果；内容换行仍一次切到新布局。需要自然重排时用数值 width/height 动画。

### 跨父容器共享元素

```rust,no_run
use jadix::prelude::*;

let chip = Element::new().w(120.0).h(48.0).layout_id("selected-chip");
```

在不同父容器中交替构建同一 `layout_id`，新节点会从上一帧的实际呈现矩形接续，
包括跨父坐标换算、宽高及弹簧速度；可先 `.layout_motion(&handle)` 自定义模式和驱动。
ID 的作用域是窗口内的 `LayoutTree`，不会跨窗口串扰。一个 ID 同时只能有一个可见所有者；
重复 ID 不做共享转移，`display:none` 不占所有权。快照允许一个收集帧的空档，之后自动过期，
避免已删除 ID 无界积累。共享元素遵循目标容器的裁剪和层叠，不会自动升为 portal 或做交叉淡化。
独立 CSS translate/scale 和滚动参与坐标换算；包含旋转或 transform-list 的祖先链不做共享接续，
而是退化为新节点的局部布局动画。跨窗口、任意旋转之间的形状配准不在本接口内。

Gallery 新增“动画”分区（`JADIX_GALLERY_SECTION=motion`），可切换排列、间距、顺序，
跨容器移动共享元素，检查嵌套子内容尺寸；“布局”分区的响应式卡片也绑定了断点投影。
普通 `#[component]` 的裸 `Signal::get()` 不会订阅重建；通过现有 `bind_*` 或
`reactive_component_live_sized` 建立局部响应式布局，再保留动画句柄，避免只有信号变了而布局没有更新。
退场保留/浮层生命周期继续使用既有 keyed overlay、floating/modal motion；本接口不拦截卸载。

## 矢量绘制

```rust,no_run
# #[cfg(feature = "animation-path")]
# {
use jadix::prelude::*;
use jadix::primitives::path::{attach_path, PathCommand, PathStyle};
use jadix::style::{AnimatedNumber, TimingFunction};
use std::rc::Rc;

let commands = vec![
    PathCommand::MoveTo((8.0, 40.0)),
    PathCommand::CubicTo { c1: (40.0, 0.0), c2: (80.0, 80.0), to: (120.0, 40.0) },
];
let path = Rc::new(PreparedPath::new(&commands));
let progress = AnimatedNumber::tween(0.0, 1.0, 600.0, TimingFunction::EaseInOut);
let drawing = attach_path(Element::new().w(128.0).h(80.0), &commands,
    &PathStyle::stroke([0.2, 0.5, 0.9, 1.0], 3.0));
let drawing = bind_path_reveal(drawing, path.clone(), progress.clone());
let follower = bind_path_follow(Element::new().w(8.0).h(8.0), path, progress, true);
# }
```

- `PreparedPath` 一次展平/测长，采样按弧长二分定位，不连接多个 `MoveTo` 子路径，并计入闭合边。
- `sample(t)` 返回位置、切线角度；`trim(start, end)` 返回路径窗口，可用于 canvas 的 `stroke_path`。
- `bind_path_reveal` 写 retained path 的几何数据，只重绘；绘制缓存指纹包含路径几何，避免重放空首帧。
  部分揭示使用测量后的折线，完整揭示恢复原曲线。
  这是几何裁剪；若要保持填充不变而只揭示描边，应分别绘制填充层和描边层。
- `PathMorph::new(from, to)` 校验同拓扑命令，`bind_path_morph` 保留二次/三次控制点和弧参数插值；
  不兼容命令、不同弧标志或非有限坐标返回错误，不静默改成一条折线。
- `bind_path_follow` 将路径偏移/切线旋转叠加到绑定时的作者变换；不要同时绑定 motion 到这些属性。
- 动画数值在 paint 或同步回调中用 `peek()`；不要在根组件的 effect 中逐帧 `get()`。

## 响应式外壳

Gallery 所有分区的 `p-3 sm:p-4 lg:p-6` 容器显式声明 `padding 200ms ease-out`，
因此 640px / 1024px 断点上的 12 / 16 / 24px 留白走布局过渡，不只动画示例卡片生效。
这是响应式样式的显式过渡，不是给所有布局变更默认添加动画。
首次挂载按当前断点、主题及 data 状态建立过渡基线；进入页面不从未选中的基础断点播放。
悬停 / 焦点等交互仍从当前呈现值过渡；已挂载页面真实跨断点时仍有中间布局帧。

`Sidebar` 在 768px 断点上复用宽度 / 最小宽度轨道收拢到零，退出完成才设为 `display:none`；
返回桌面时先恢复显示，再从当前宽度展开。连续跨断点会从当前呈现宽度重定向，
不修改桌面展开偏好，也不替换原有移动 Sheet 的内容迁移和焦点管理。
桌面图标折叠校正不遍历移动 Sheet，避免把迁移后的菜单文字再次压成零宽。
`SidebarInset` 的外边距、圆角同步过渡。上述轨道服从减少动态效果设置，直接到末态。

## 生命周期与可访问性

使用窗口已有 `tick_animations`，没有独立计时线程。状态及元素释放后，弱引用注册表不再推进动画。
`MotionState::is_active()` / `value(property)`、`LayoutMotion::bounds()` / `is_active()` 可观察呈现状态。
`MotionState::pause()` / `play()` / `stop()` 原地控制轨道；新的交互或目标会重新接管已停止的属性。
全局 `prefers_reduced_motion` 对这些绑定即时生效，包括正在播放的动画；路径绑定直接到末态。
`AnimatedNumber::jump(value)` 立即设置值，取消队列、pending target、跟随、速度和播放状态，保持信号身份。

验证：`cargo test -p jadix --lib --features animation-path motion:: -- --test-threads=1`。
