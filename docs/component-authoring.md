# 自定义组件与共享作者接口

`jadix::authoring` 提供自定义组件和 Builder 共用的框架接口：`BaseComponentBuilder`、`SlotChrome`、`SlotMap`，以及槽位、文字覆盖和 children 打包辅助函数。它们不属于某套产品组件的默认外观。

`jadix::components::base` 仍重导出同一组类型与函数；既有 `jadix::components::{BaseComponentBuilder, SlotChrome, SlotMap, SlotStyleFn}` 和 prelude 的 `SlotChrome`／`SlotMap` 保持兼容。新自定义组件优先引用 `jadix::authoring`，便于后续将产品代码迁到独立库。

## 普通函数与 Builder

组件领域参数由函数决定，通用样式交给 chrome。槽位名是组件对外承诺，下面的函数使用 `data-slot=custom-caption`：

```rust
use jadix::authoring::{paint_slot, SlotChrome};
use jadix::core::tree::Element;

pub fn caption(value: impl Into<String>, chrome: SlotChrome) -> Element {
    paint_slot(Element::new().text(value.into()), "custom-caption", &chrome)
}

let element = caption(
    "Ready",
    SlotChrome::default()
        .class("p-2")
        .style(|builder| builder.text_size(18.0).opacity(0.9)),
);
```

`SlotChrome::overlay` 合并 class，并按先内层、后外层的顺序执行样式；调用方可以覆盖默认值。`SlotMap` 按 `data-slot` 管理具名样式，`paint_slot` 先写槽位、匹配样式表，再应用调用方 chrome。

`BaseComponentBuilder::finish()` 保持既有语义；其中 `.text()` 写入供产品 Builder 消费的 label 元数据，不能当作修改普通 Element 正文的接口。普通函数直接使用 `Element::text`。

## 组件宏与显式运行时路径

`#[component]` 注入的 `base` 回调使用 `jadix::authoring::BaseComponentBuilder`，普通、泛型、`reactive` 与显式 `base_builder` 分支共享这项契约。领域参数与 children 的写法沿用现有组件宏。

Cargo 将框架依赖重命名为 `ui_runtime` 时，可显式指定属性宏的运行时路径：

```rust
use ui_runtime::core::tree::Element;

#[ui_runtime::component(crate_path = "ui_runtime")]
pub fn Caption(value: String, children: Option<Element>) -> Element {
    Element::new()
        .text(value)
        .children(Element::unpack_fragment_children(children))
}
```

调用方可以从另一 crate 引入生成的 `Caption`／`CaptionProps`。`impl_slot_map_methods!`、`impl_overlay_content_slot!`、`impl_simple_slot_builder!` 等既有声明式 Builder 宏从框架作者接口生成样式代码，宏名和调用方式不变。

`jadix::ui!` 携带运行时路径；外部产品门面还可通过隐藏的 `__ui_runtime!` 显式选择 `components`、`dnd`、`icons` 模块路径。特殊标签的编译规则仍位于 `jadix_macros`，迁包时须迁移对应规则和测试；过程宏在当前版本依旧是必选依赖。关闭 `components` 可以独立构建框架。

2026-10-26 已确认后续 R2 迁包要求：组件、图标及高层 DnD 提供不依赖 UI 宏／解析包的普通函数／Builder 模式，并通过默认关闭的可选 `macros` feature 保留宏作者写法；Gallery／宏示例迁移时显式开启。框架的传递依赖、生成的公共类型和源码复制闭包一并处理；该计划尚未在当前版本实现，不能把 `--no-default-features` 当作已排除 `jadix_macros`。实施与验收约束见 [宏专项](../development-docs/plans/SPLITTING.md#6-宏专项独立包不等于可选作者层)。

## 自定义控件的指针拖动

共享轴向和矩形使用 `jadix::core::geometry::{Axis, Rect}`，供边缘滚动、指针动作与自定义控件复用；它们不持有拖拽会话。旧 `utils::drag::geometry::{Axis, Rect}` 和 `utils::drag::{Axis, Rect}` 继续转发同一类型，可以直接混用，不需要转换。

`jadix::utils::drag::routing` 提供基础控件的宿主接入。产品在拖动开始时调用 `register`，传入 `DragHandlers::new(on_move, on_finish)`；key 应使用模块独有的静态字符串，例如 `module_path!()`。同 key 的新登记会替换旧登记，已准备的旧移动不会再提交。回调采用函数指针，产品继续管理自己的状态。

滚动策略由产品选择：`Allow` 保留滚动，`BlockPan` 阻止内容拖动滚动，`Freeze` 同时冻结 wheel 和惯性。结束时产品须调用 `unregister`、清理待提交工作并释放自己持有的指针捕获；宿主抬起统一执行 `finish_all`，产品原有松手吸附、回弹或关闭逻辑仍由 `on_finish` 实现。

需要跟随布局更新的控件可用 `frame_sampling(sample, drain)`：`sample` 可以读取树、更新非响应式几何缓存或记录待提交坐标，不能写 Signal。宿主释放树的借用后，移动经 `PreparedMove::dispatch` 提交，帧样本经 `drain_frame` 提交。移动处理器须让较新的指针移动覆盖旧帧样本；结束须丢弃排队工作。

登记和已准备更新按窗口隔离，提交恢复原窗口并验证登记代际；产品仍负责结束时的捕获与状态清理。它不提供高层拖放载荷、碰撞或排序。`pointer_drag` 原有多指会话 API 保持独立，当前宿主登记路由服务现有主指针控件。外部产品重命名依赖消费验证仍基于工作区构建产物。

## 高层拖放的宿主接入

`jadix::utils::drag::runtime` 提供高层产品的宿主协议。`DragRuntime` 由产品实现并通过 `register(Rc<dyn DragRuntime>)` 安装；接口接收 `Element`、指针、视口、键盘意图和普通几何／绘制数据，不包含内置 DnD 的 `DragHit`、Provider、排序或碰撞类型。内置 DnD 通过自己的 `session_runtime` 适配；独立产品可以实现同一接口。

`RuntimeState` 提供 dragging／active／coasting、待激活截止时间和最后指针；产品须保持这些状态一致。宿主在按下、移动、vsync、延时唤醒和抬起时调用采样接口。`MoveMode::Follow` 用于高频指针跟随，`Reconcile` 用于需要重新判断目标的时机；两者的策略均由产品决定。`prepare_move`／`prepare_end`／`prepare_autoscroll`／`prepare_inertia` 返回的 `PreparedUpdate` 必须在树借用释放后 `dispatch`；采样不能触发响应式通知或同步重建树。

布局侧 `sync_tree`／`after_layout`、collect 几何与祖先裁剪、保持 collect 活跃以及调试矩形也经同一运行时分派。产品返回的 `CollectGeometry`／`DebugRect` 是普通数据；框架不读取 DnD 专属属性或会话类型。布局回调在宿主借用树期间执行，只能进行该阶段允许的布局／呈现同步，不能同步 remount 宿主。

每个 UI 线程的每个窗口只有一个高层运行时。重复登记同一实例保留待提交工作；替换、卸载或卸载后重新登记会使旧 `PreparedUpdate` 失效。内置 Provider／DragOverlay／启用碰撞调试或开始内置会话会安装内置适配器。已准备更新捕获窗口与登记生命周期；内置会话额外绑定 Owner。协议不提供多产品运行时叠加或统一 pointer-cancel。

自动滚动采样允许以 `set_silent` 更新滚动偏移，然后收集滚动后的命中几何；产品拖拽回调与响应式写入放进返回的更新闭包。惯性采样只保留命中快照，步进、移动和结束回调均在提交阶段执行。内置 DnD 的待提交滚动更新还绑定具体会话，取消、结束、重新开始或惯性恢复后，旧更新不能推进新会话。

R1 的运行时协议尚未冻结：实现者现在须提供 `prepare_autoscroll`／`prepare_inertia`，同仓外部实现与源码消费样例已同步迁移。原 `pump_autoscroll`／`pump_inertia` 同步入口保留，适合调用方已保证无宿主树借用的场景；宿主使用采样加延后 `dispatch`。这只保证本协议中的拖拽产品回调已移出借用，布局回调仍遵循前述约束。`drag` 与内置 `dnd` 已分别门控；真实 `jadix_dnd` 包及发布依赖闭包归 R2–R5。

[外部 BoardDrag 示例](../crates/jadix/tests/fixtures/authoring/board_drag.rs)只依赖公开框架接口。同一份源码既作为独立 provider crate 的模块编译，也直接作为消费者模块嵌入；`scripts/check-authoring.ps1 -Offline` 验证重命名运行时下的两种消费方式。该验证使用工作区构建产物，尚未验证已发布 crate 安装，也不代表现有完整组件库或 Lucide 资源已经可以直接复制使用。

## 分发边界

本接口可以供项目内自定义组件或外部组件 crate 使用。独立 `jadix_components`／`jadix_lucide` 的 Cargo 安装和按需源码、资源复制仍按后续真实迁移验收，现有产品源码不能据此视为复制即用。开发验证见 [R1 作者接口记录](../development-docs/ops/r1-authoring-20261025.md)，当前能力开关见 [FEATURES](FEATURES.md)。


## 挂载、命中与保留绘制

`authoring::mount` 为外部产品代码提供 `with_inline_mount`、`is_reactive_placeholder`、`replace_portal_content`、`collision_bounds`、`child_scroll_offset`、`painted_scale`、`invalidate_paint`、`invalidate_collect_paint` 和 `pin_overlay_origin`。这些函数复用框架的挂载、坐标及 damage 规则，调用者无需访问响应式占位 hash、portal hoist 属性或节点的私有绘制快照。

`with_inline_mount` 支持嵌套，闭包返回或 panic 展开后恢复之前模式。`replace_portal_content` 在宿主树同步阶段调用，保留 portal 挂载身份、非零 z-index 和原有 `reactive_sync`；它会替换内容节点及其余内容属性。绘制、布局和命中操作必须在对应宿主阶段使用，不能从响应式回调中重入借用中的宿主树。

通用树查询可用 `core::tree::{find_in_tree, find_path_in_tree, element_clips_overflow}`。`core::geometry::{paint_nudge, paint_bounds}` 为绘制及 damage 共享同一视觉边界。`authoring::text` 导出既有编辑类型和输入过滤策略；普通文本选区的 key、复制与绘制同步属于 `utils::text_selection`。Label 和 Input 原有公共入口继续兼容。

## 节点呈现回调

在构建节点时，通过 `authoring::presentation::attach` 附加呈现工作。回调由节点持有，节点释放后释放其捕获状态；重复附加按登记顺序组合，不覆盖已有回调。返回 `LayoutStatus` 告知宿主变化级别，框架合并变化并失效相关保留绘制缓存。同一阶段按树的前序访问节点，每个节点内按登记顺序执行；所有 AfterLayout 生产者完成后才进入 AfterLayoutDependents。

| 阶段 | 时序与用途 |
| --- | --- |
| `Phase::Hover` | 指针 hover 状态更新后，产品将状态写入节点外观 |
| `Phase::State` | 常规受控状态外观更新；主窗沿用原来的滚动快速帧门控 |
| `Phase::BeforeLayout` | 样式完成后、布局前处理保留节点和兄弟树外观 |
| `Phase::AfterLayout` | 布局完成、collect 前更新依赖实测几何的状态 |
| `Phase::AfterLayoutDependents` | 所有布局后生产者完成后，更新依赖它们的节点 |
| `Phase::AfterGeometry` | 新几何可用后再次调和溢出行 |

回调只应修改自己的节点及其受控子树。结构修改必须保持当前遍历的节点有效，不能同步重挂整个宿主树；高层 DnD 的用户回调仍须经 `prepare_*`／`dispatch` 两阶段处理。新回调应在挂载前附加；对已经挂载的树修改接入时，要通过宿主调和或显式路径失效更新祖先缓存摘要。此接口不是任意生命周期事件的全局注册表。

当呈现依赖子节点最终计算样式时，用 `authoring::presentation::attach_after_style` 附加绘制回调。宿主在既有样式遍历处理完该节点的 children 后调用，按登记顺序组合；通过本次调用提供的 hover 查询读取命中，返回是否改变绘制。回调不得保存查询、替换或重排 children。ButtonGroup／ToggleGroup 的拼接策略采用此方式，节点释放即释放其捕获状态。

主、副窗均接入 Hover／State，布局 collect 接入两个 AfterLayout 阶段。当前 Switch、Checkbox、RadioGroup、Tabs、Sidebar 菜单按钮、TreeView 行、Command、ContextMenu、DropdownMenu 及 MessageScroller 已使用该接口；Button、Table、Avatar／Bubble 等也通过节点回调接入；Sidebar、MediaPlayer 与 Drawer 的跨树行为由节点持有。

## 宏与源码复制的验证范围

通过 `jadix::ui!` 调用 RSX 时，展开会携带实际运行时路径，Cargo 将依赖改名为 `ui_runtime` 后也可使用普通组件、基础节点和现有产品特殊标签。`#[component(crate_path = "runtime")]` 的显式路径契约继续适用。直接使用底层 `jadix_macros::ui!` 仍保留原来的 `jadix` 路径约定；应用和组件作者优先使用运行时重导出的宏。

`scripts/check-authoring.ps1` 验证跨 crate 自定义组件及同一 BoardDrag 源码的两种消费方式。`scripts/check-dnd-source.ps1` 把完整高层 DnD 实现复制到全新目录，保留共享的 pointer／routing／runtime 协议身份后编译独立外部 crate，验证会话独立、布局、Owner 清理和旧任务失效。它们使用当前工作区构建产物，不证明发布 registry、无根 patch 消费或所有组件源码都能直接复制。

`scripts/check-lucide-source.ps1` 复制静态／动画实现、四项资源及许可，在仅启用 `svg,animation` 的框架上验证独立 provider、Builder／宏、hover、morph、字体与纹理布局。纯 Rust 文件复制不足以代替该闭包。库安装和源码／资源复制两条路径仍分别在 R2–R5 验证。

## 窗口、定时与输入

`authoring::window::window_local!` 定义按 UI 线程和窗口隔离的惰性槽；关闭窗口释放其捕获，迟到清理只能获得临时默认值，不能复活窗口状态。`Scope::capture().enter()` 让产品在延迟回调或 Owner 清理期间恢复原窗口。槽的闭包不持有全局存储借用；内部 RefCell 等仍遵守各自借用规则。

`authoring::deadline::attach` 将截止任务附在节点上。`Visibility::Visible` 离屏暂停，`Always` 适用于浮层退出和清理；回调接收 `now`／`resumed`，返回是否重绘和下一截止时间。宿主先采样索引，再释放树借用并在原 Owner／窗口执行；移除节点、Owner 销毁和关闭窗口使旧任务失效。Carousel、Toast、Drawer 使用该协议。不要在布局或采样期间写 Signal。

节点的 `text_edit_key_handler`、`authoring::keys` 窗口快捷键、焦点变化声明、光标截止请求和 `authoring::input` 的滚动版本／几何变化状态分别承担输入契约。它们让产品保留快捷键和编辑策略，不要求宿主调用具体组件。

## 跨树呈现与资源

`presentation::attach_tree` 在对应节点阶段之后处理整个窗口的保留树；可移动既有节点，不能同步 remount 宿主。`attach_tree_with_cleanup` 在节点移除或 Owner 销毁后的下一匹配阶段执行一次清理；关闭窗口直接释放。清理只能恢复保留外观，不能读写已销毁 Owner 的 Signal。`Context::width_budget` 提供祖先宽度约束；隐藏节点和跨树回调的成本遵循已有摘要遍历。

`media::texture::RgbaTexture` 持有 CPU 纹理，GPU 上传由各窗口实际使用的纹理 key 驱动。`utils::font::register_font_data` 校验并登记字体数据，须在创建窗口／FontSystem 前调用；既有 FontSystem 不会自动重载。SVG 文档栅格化通过公开 `media::svg_raster` 接口。

`IconProvider` 可以解析字形、提供 SVG 和配置节点的 `authoring::icon::Playback`。框架持有通用播放回调、共享动画和 morph；图标库持有目录、配方、播放状态与资源。未开启 `lucide` 时不编译内置目录；复制的外部图标库仍能驱动通用 Icon。静态、配方播放和 morph 分别遵循 feature 依赖，不能把动画配方开关当作全部动画开关。

WebView 的窗口原生宿主位于 `platform::webview`，UI 产品只负责页面组件的组合。平台服务与组件产品分开，Windows 编译证据不代表其他目标已经交叉编译或运行。

## 产品状态与共享语义

`authoring::state::State` 是节点持有的按类型存储；`get`／`get_mut`／`set`／`remove` 不要求框架认识产品类型，克隆后修改采用写时复制。Button 的 variant、禁用描述和悬停缓动位于产品模块。`Element::authored_style` 只向 Inspector 返回普通 Style，`disabled_sync` 负责产品禁用策略，`paint_fill_inset`／`paint_caption_scope`／`owns_focus_paint` 声明局部绘制边界和焦点外观职责。框架不根据 Button variant 计算外观。

Element 中历史命名的 `switch_checked_sync`／`checkbox_sync`／`radio_selected_sync` 仍保留同类型兼容接口；它们仅承载布尔／选择 Signal，用于共享 Switch／CheckBox／RadioButton 可访问性角色、checked 状态、订阅和诊断。具体产品的颜色、勾选标记、布局和状态推进由节点呈现回调完成。`TextEditPaintSync` 属于输入／IME／选择／光标的共享编辑协议；它不要求内置 Input／Textarea 产品模块参与编译。


节点可用 `paint_opaque_fill` 声明不透明背景／边框在节点可见时保持填充，文字和图标仍使用节点透明度；`paint_region_replay` 让局部合成变化重绘周围页面区域；`paint_children_unculled` 让溢出且移动的子节点在仅滚动收集时仍被访问。默认全部关闭。框架不按组件的 `data-slot` 或名称猜测这些策略，属性参与绘制缓存指纹。

`paint_edge: Option<authoring::presentation::EdgePaint>` 在底边或右边绘制不参与布局的装饰；产品提供边、宽度与 RGBA 颜色，框架负责裁剪、透明度与缓存。Tabs 的线条样式由 Tabs 自己声明。

## Surface／Content／Overlay 分层

`SlotMap::layers(surface, content, overlay)` 是可选的通用作者接口；三个参数分别为 `Option<Element>`、`Element`、`Option<Element>`。返回相对定位容器，按绘制顺序放入绝对铺满的 surface、参与正常布局的 content、绝对铺满的 overlay。槽位名公开为 `SLOT_SURFACE`／`SLOT_CONTENT`／`SLOT_OVERLAY`，对应 `data-slot="surface"`／`"content"`／`"overlay"`。

surface 不接收命中；overlay 容器本身穿透命中，其交互子节点仍可接收输入。背景、材质、模糊放到 surface，正文和输入放到 content，装饰或浮动操作放到 overlay。效果只作用于设置效果的节点及其子树；给最外层设置 filter 仍会影响全部内容。此接口不自动启用原生窗口材质，窗口材质继续由窗口配置和平台宿主管理。

```rust
use jadix::authoring::SlotMap;
use jadix::core::tree::Element;

let panel = SlotMap::default()
    .surface(|slot| slot.class("bg-card rounded-lg"))
    .content(|slot| slot.class("p-4"))
    .layers(
        Some(Element::new()),
        Element::new().text("Content"),
        None,
    );
```

这是显式组合入口，既有组件可以逐步采用；当前 WebView 用独立 surface 承载加载背景。实现位于不依赖产品组件的 `authoring` 层，没有将主题、组件名或产品策略引入宿主。本接口不代表产品 crate 拆分或 R1/R2 阶段签收。

## 滚动默认值与端部箭头

未指定行为时，滚动视口及内建列表统一使用 `smooth`。`ScrollArea`／`ScrollViewport` 的 `scroll_behavior="jump"`、VirtualList／VirtualMasonry Builder 的 `.scroll_behavior("jump")`、MessageScrollerViewport 的同名 Builder／RSX 属性可显式选择立即滚动。消息的自动跟随和默认操作按钮沿用视口策略；`MessageScrollerCtx::scroll_to_end(true)`／`scroll_to_start(true)` 可显式立即跳转。首次布局的初始位置、内容缩短后的偏移约束和拖动拇指直接跟随指针属于定位／输入约束。

`Scrollbar` 的 `.arrows(true)`／`arrows={true}` 与 `ScrollArea` 的 `.scrollbar_arrows(true)`／`scrollbar_arrows={true}` 可启用滚动槽两端外部的三角按钮，默认关闭。竖向为 ▲／▼，横向为 ◀／▶；单次点击移动 25 个逻辑像素，遵循所属视口的 smooth／jump 策略并约束在内容范围内。`.arrow_class(...)`／`arrow_class="…"` 和 `.scrollbar_arrow_class(...)`／`scrollbar_arrow_class="…"` 定制两个按钮的样式，支持 `hover:`；按钮槽位为 `scrollbar-arrow-start`／`scrollbar-arrow-end`。端部按钮保留可见的操作入口，轨道和拇指沿用原有自动淡出策略；轴禁止滚动或 auto 模式下无溢出时整组收起。

独立产品实现可以通过公开的 `authoring::input::scroll_by(id, dx, dy)` 按逻辑像素导航，通过 `core::scrollbar::configure_scrollbar_parts` 登记轨道和拇指，通过 `configure_scrollbar_group` 登记包围轨道及端部控件的 flex 组。几何、范围和输入策略属于框架；箭头外观、默认开关和槽位属于产品，不需引用私有 `app` 模块。

## 容器命名与整组透明度

Rust 容器入口使用 `div()`，RSX 支持 `<div>`／`<Div>`，与 `<Box>` 等价；模块入口为 `jadix::primitives::div`。`r#box()`、`primitives::r#box` 和 `boxed()` 保留兼容。Rust 的小写 `box` 是保留字，大写 `Box` 不是。Label 的关联目标用 `for_target()`，已有 `r#for()` 仍是兼容别名；RSX 继续使用 `for="…"`。

`Element::opacity_group(bool)` 和 `div().opacity_group(bool)` 声明节点的整组合成边界，默认关闭。RSX 通用属性为 `opacity_group={true}`（兼容 `opacityGroup`），支持 `.get()` 表达式的细粒度绑定。需要就地修改时，`Element::set_opacity_group(bool)` 返回 `LayoutStatus`，应由现有节点更新／呈现接口合并并失效祖先缓存。匿名 children 打包会保留显式声明的合成边界。

```rust
use jadix::prelude::*;

let panel: Element = div().opacity_group(true).opacity(0.5)
    .child(div().w(32.0).h(32.0).bg("#ff0000"))
    .into();
let rsx: Element = ui! {
    <Div opacity_group={true} opacity={0.5}>
        <div w="32px" h="32px" bg="#ff0000" />
    </Div>
};
```

应用的 `app().opacity_groups(bool)`／`<App opacity_groups={…}>` 设置进程级总开关，也可直接调用 `set_opacity_groups_enabled(bool)`，通过 `opacity_groups_enabled()` 查询。总开关默认开启，仅决定是否执行节点已请求的合成；不会自动给全部节点开启整组合成。关闭后回到逐节点透明度，保留节点请求，重新开启即可恢复。多个窗口共享这一策略，最后一次显式设置生效；省略 App 属性时保持当前策略。运行时切换请求所有已登记窗口重绘，不重建组件或清空文字资源缓存。

合成在有效透明度介于 0 和 1 时使用离屏纹理，对容器与其绘制子树统一应用透明度；节点自带非空 filter 或 modal zoom 捕获时沿用已有捕获路径。`jadix_opacity_group` 是内部命名空间标记，用于区分框架协议与用户自定义属性；应用和可独立分发的组件源码应使用公开方法。实现、策略、缓存失效及基础宏入口均属于框架；产品组件只提出节点请求，不向框架引入产品依赖、默认样式或组件名判断。本接口不代表 R1/R2 阶段签收。
