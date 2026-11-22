# jadix_components

[English](README.md) | [简体中文](README.zh.md)

原生产品控件，函数、Builder 与可选 RSX 共用实现；只在需要时启用 macros。

本包属于 [Jadix](https://github.com/Zk3619/Jadix)。**Alpha；版本 `0.1.0`。**

VirtualList、VirtualGrid 和 VirtualWrap 在发布视口及加载边缘通知前，会重新采样条目渲染回调造成的数量或 data_key 变化；网格与折行列表也会重新采样动态单元尺寸。失效后立即停止旧窗口绑定，每次同步最多尝试四轮，持续变化则请求下一帧。建议渲染回调保持纯粹；修改条目但数量不变时，应更新 data_key。保护同时覆盖 VirtualList 首次构建及父滚动适配。

## 安装

在应用中添加依赖：

```toml
[dependencies]
jadix_components = "=0.1.0"
```

平台专用功能需要对应平台的 SDK。

## Feature 与边界

默认开关：`gpu`，保留完整产品行为；RSX 继续通过 `macros` 显式选择。禁用默认 features 后按需选择宏与主题，使用[便携样例](../../examples/portable-contract/README.md)所示接口。便携写法投影为强类型描述，由 [jadix_native 平台宿主](../jadix_native/README.md)创建系统／DOM 控件并声明能力。登记子集含基本控件、Dialog、基本组合 Select 以及 Popover Builder／组合 RSX；Popover 由 DOM 与 Android PopupWindow 宿主支持，UIKit 明确拒收。完整依赖关系以 [Cargo.toml](Cargo.toml) 为准，存在开关不代表平台行为已完成验证。

## 文档

GPU Command／Combobox Builder 支持 `.query_signal(Signal<String>)`，包括 `.child(...)`
组合入口；仍可用 `command_query`／`combobox_query` 控制器读写。挂载时以外部文本为准，
后续外部更新在失焦且没有输入法预编辑时于过滤／渲染前生效。本地编辑、清空、会话重置
及批处理中本地提交优先于等待中的外部文本。绑定属于组件 owner，输入框暂时卸载后
仍有效；Command 无搜索框列表保留独立的 typeahead 导航。ComboBox 查询与选中 ID 分离：
单选关闭时发布已选标签，多选关闭时清空查询。外部异步搜索自行管理请求，可配合
`filterable(false)` 保持外部结果排序；该绑定不改变焦点。

VirtualGrid 与 VirtualWrap 支持 `.use_parent_scroll()`：网格按物理轴分别寻找最近注册的祖先；响应式排列借用纵向所有者，根据自身容器宽度重排。两者保留完整自然尺寸，导航写入实际父容器并保留周围内容范围。隐藏或未绑定时导航排队，不回退到全局视口。父滚动模式关闭排列自身的锚定、跟随末尾与剩余空间对齐；预测、交互保留、内容插槽、可见性和加载边缘保留挂载预算及完整父视口语义。

VirtualGrid 可独立设置 `.row_key_fn(...)` 和 `.column_key_fn(...)`，在插入、排序后保留已挂载单元格外壳，渲染回调收到当前坐标。键索引占用 O(行数 + 列数) 空间，仅在数量或 data-key 变化时更新；滚动不扫描键，也不索引全部逻辑单元格。离屏重复键同样报错并清空窗口。默认按有键轴保持编辑前业务项与像素深度，可用 `.maintain_visible_content_position(false)` 关闭；显式导航优先，父滚动与隐藏状态保留所有者偏移。未设置键的轴继续使用坐标身份。

GPU `virtual_grid(id)` 在单元格预算内挂载二维可见矩形、各轴 overscan 与仍持有交互的单元格：

```rust,ignore
virtual_grid("sheet")
    .row_count(100_000).column_count(1_000)
    .cell_size(100.0, 28.0).cell_gap(2.0, 2.0).height(400.0)
    .render_cell(|row, column| Element::new().text(format!("{row}:{column}")))
```

`row_count_fn`、`column_count_fn`、`cell_size_fn`、`data_key_fn` 每帧轮询，不重建外层视图。
滚动及尺寸／数量变化时按坐标保留已挂载单元格；内容编辑或重排后变更 `data_key_fn`，
仅刷新挂载区内容。`VirtualGridHandle::scroll_to_cell` 接受 `[x, y]` 对齐／额外偏移数组，
等待实际视口尺寸后双轴跳转；`scroll_to_end` 跳至两个轴末端。通过 `viewport()` 或
`on_viewport_change` 读取实际可见几何。间距与总范围共用无需渲染器的
`jadix_core::virtualization::UniformGrid`。
默认最多挂载 16,384 个单元格，可用 `max_mounted_cells` 修改。非法尺寸或超出预算时
正文为空，并附带 `data-virtual-grid-error`；有效几何恢复后自动继续挂载，预算失败仍保留
滚动范围。这是定长单元格 GPU Builder 基础，尚不包含业务键重排保活、变长尺寸、冻结行列、
网格键盘／选择语义、专用 RSX 或原生描述符宿主。


GPU Collapsible 与 Accordion 内容可选择物理弹簧展开：
`.content_with(|c| c.reveal_spring(SpringPhysics::default_css()).text("Body"))`
（Accordion 在 item 上调用），RSX 内容标签属性为 `reveal_spring={spring}`。
从 `jadix_components::style` 导入 `SpringPhysics`；省略时沿用 keyframes。
递归 TreeView 可设 `.variant("collapsible").reveal_spring(spring)`，或使用
`<TreeView variant="collapsible" reveal_spring={spring}>`。所有分支（含展开时
才创建的后代）继承配置；flat 虚拟列表模式不受影响。
启用 `animation-base` 后，反向切换保留速度，关闭内容等弹簧收敛后才卸载，
高度裁剪限制在自然内容范围内。减少动画或禁用动画时立即结束；无效参数回退
默认物理配置，极端配置设 20 秒安全上限。

静态可选文本支持双击选词、三击选硬段落，之后向任一方向拖动可按词／段落扩展，
保留最初选中的完整单元。分词遵循 Unicode 边界，保留组合字符和 emoji 序列；
段落选区不包含 CRLF。这一交互作用于单个可选文本叶子，尚不包含跨叶子选择或
触摸长按 token 选择。

`text_view(id).highlights(ranges)` 与
`diff_view(id, diff).highlights(original_ranges, modified_ranges)` 接收
`jadix::utils::text_annotations::TextDecoration`。范围使用各自源文档中的绝对
UTF-8 字节偏移，定位发生在 Tab 展开之前。前景色、背景色、下划线与删除线保留
文本尺寸和可选择的源文本；重叠范围的颜色以后者优先。高亮绑定初始文档快照，
替换文档会清除旧高亮；更新高亮时，使用新快照与对应范围重新构建视图。

DiffView 的统一／分栏模式默认自动突出行内变更。共享
`jadix_document::DiffDocument` 按位置配对替换行，以字素簇比较并一次性计算源字节范围，
保留 CRLF 和未改动文本的源坐标。手工高亮可覆盖自动底色；
`.inline_changes(false)` 关闭自动高亮，RSX 属性为 `inline_changes`。
自动范围随新快照更新。细化有行对及快照总量上限，超限时退回整行标记，
可通过 `has_coarse_inline_changes()` 查询；未配对的插入／删除保持整行变更。

启用 `syntax-highlighting` 后，两种 Builder 均支持 `.language("rs")`，RSX 属性为
`language`。共享文档核心在后台解析完整源文档，保留跨行语法状态及源字节坐标；Diff 两侧独立解析。
视图先显示原文，最新结果到达后刷新可见行；待处理快照只保留最新一份，取消或卸载无需等待解析完成。
语法前景可与行内差异底色、手工装饰组合，手工样式优先。替换快照或切换明暗主题会刷新语法。
行索引仍在 UI 线程构建，尚无追加增量解析；单源超过 2 MiB 或任意一行超过
64 KiB 时共享解析器降级为纯文本。未启用该 feature 时同一 API 保留纯文本行为。

启用 `document` 后，`document_view(id).on_link_hover(handler)` 提供绘制后的行内链接
悬停变化，供应用展示预览；RSX 的 `on_link_hover` 接受 `DocumentLinkHoverHandler`。
`DocumentLinkHover` 包含视图 ID、可选目标地址和窗口逻辑坐标。同一链接内重复移动不重复
通知；移到普通文本、离开、按下及卸载会清理活动预览。同一文档的多个视图各自保存状态，
共享回调时应使用不同视图 ID。点击导航仍用 `.on_link(...)`。预览内容、键盘交互，以及
滚动或文档变化后静止指针下的预览刷新由应用负责。

Tooltip 与 HoverCard Builder 使用 `.open_delay(300)` 和 `.close_delay(0)`，
数值默认以毫秒为单位。共享打开延迟默认为 300ms，零值立即打开；
RSX 根节点或 Trigger 使用 `openDelay` 与 `closeDelay`。

便携 Tabs 支持 `tabs` Builder、`TabsList`／`TabsTrigger`／`TabsContent` 组合 RSX，
共用受控值、惰性面板、水平／垂直方向键、Home／End、禁用项跳过与原生焦点请求。
非活动面板卸载原生控件，保留 Element 与业务信号，重新激活时用新原生身份恢复文本。
条件可见性变更等待当前已投影文本的组合输入结束；通用 portal 与其他任意结构删除不由此策略保护。
跨面板原生选区／撤销、单一 Tab 停靠、专用 Tab／TabPanel 辅助技术 provider、主题变体
及物理 IME 验收仍未完成。`native-tabs` 提供 Windows 实际产品入口。

GPU 专属控件、文档、媒体、动画与缩放预览开关会同时选择 GPU 组件接口及对应运行能力；仅启用宏与主题仍可使用不依赖 GPU／窗口后端的便携接口。

[项目文档](https://github.com/Zk3619/Jadix/tree/main/docs)介绍应用配置、平台接入、功能选择与 API 用法。

## 许可证

Apache-2.0。保留 [LICENSE](LICENSE) 及依赖的来源声明。


GPU `virtual_wrap(id)` 根据实测视口宽度自动为平面数据分列：

```rust,ignore
let handle = VirtualGridHandle::default();
virtual_wrap("gallery").handle(handle.clone())
    .item_count(100_000).minimum_item_width(120.0)
    .item_height(80.0).item_gap(8.0, 8.0).height(400.0)
    .render_item(|index| Element::new().text(format!("Item {index}")));
// 挂载后：handle.scroll_to_item(index, ListScrollAlign::Nearest, 0.0)。
```

各列均分可用宽度，视口过窄时改为一个更窄的列，并禁用横向滚动。按数据项索引复用
挂载状态；连续换列保留首个可见数据项所在行及像素深度，直到滚动或定位建立新锚点，
末端按范围裁剪。`item_count_fn`、`item_height_fn`、`data_key_fn` 支持动态更新，
内容编辑或重排后变更 data key。`overscan` 按行计，挂载预算只计实际数据项，排除
末行填充格。`item_origin` 与项定位接受平面索引，几何共用 `UniformWrap`；渲染和
句柄复用既有 `VirtualGrid`。当前为固定项高；键盘／选择策略、专用 RSX 与原生宿主
仍属后续工作。

可变数据可用 `.item_key_fn(|index| stable_key(index))` 提供唯一业务键，保留挂载包装
节点的身份和编辑前的首项及像素深度。删除锚点时依次选择后继、前驱存活项并回到项首；
显式定位优先。`.maintain_visible_content_position(false)` 关闭数据编辑锚定，换列仍
保留位置。键表占用 O(数据量) 空间，仅数量或 data key 变化时扫描，滚动与空闲帧不
扫描整表。同数量编辑必须变更 `data_key_fn`。移动项或数据修订会用当前索引重渲染
内容；包装节点属性和模板身份保留，后代状态需要作者提供稳定 ID。重复键会清空挂载
窗口并暴露 `data-virtual-grid-error=DuplicateKey`，修复键并推进 data key 后恢复。
默认索引模式不分配与数据量成比例的存储。

VirtualWrap 也支持 `.viewability(ListViewability::ItemFraction(0.75))`、
`.minimum_view_time(Duration::from_millis(100))` 和
`.on_viewable_items_change(|change| { /* 当前／之前的平面数据索引 */ })`。
比例按纵向重叠计算，完整可见项总是满足；共享 `UniformWrap::viewable_items`
以 O(1) 返回平面范围，排除行间距、overscan、预测窗口和末行填充格。
VirtualGrid 和 VirtualWrap 同样自动保留已挂载焦点、有效文本选区和 portal 所属单元，按当前几何与稳定业务键定位，不改变视口回调或滚动位置。`max_mounted_cells` 优先容纳请求窗口，离屏交互单元仅使用剩余额度；隐藏或无效几何、已删除身份释放交互保留。

连续停留复用 VirtualList 的视口有界跟踪器和节点截止调度；未配置回调时不安装
观察者或定时器，空闲时不请求唤醒。数量／data key 变化、无效几何、隐藏／离屏恢复
重新计时；换列仅保留仍满足条件的索引的停留时间。即使配置业务键，事件仍报告数据
索引；不扣除祖先裁剪和浮层遮挡面积。

VirtualList 和 VirtualWrap 的分页请求复用 `jadix_core::ListLoadEdges`：
`.on_load_more(callback)` 与 `.on_load_more_at_start(callback)` 按实际视口判定，
排除 overscan／预测窗口。`.load_more_threshold(viewports)` 默认两端均为 1 个视口；
`.load_more_at_start_threshold(viewports)` 独立覆盖逻辑首端，不受调用顺序影响。
0 表示到边界才触发，无效值保留原设置。反向列表在双轴均按数据顺序应用阈值。
每次靠近只触发一次，数量／data key 变化会重置仍在附近的边缘；空数据在已测量
视口中仅从尾端请求第一页。缺少视口、分列几何无效或容量错误时不触发，恢复后
重新判定。回调可读取当前 handle、更新数据；加载中、耗尽和重试由应用保护，
组件不会自行发起网络请求。


VirtualWrap 支持 `.initial_scroll_at_end()`、`.follow_end(bool)` 与
`.follow_end_threshold(px)`。首次跳尾等待有效、非空、已测量的布局；handle 显式导航
优先并结束首次请求。尾端跟随在键身份／列数／行高恢复前，以当前用户偏移对比最近一次
成功可见布局的尾端。追加、列数重排、行高与视口变化时保留尾端阅读；离开阈值后停止，
回到尾端再恢复。阈值内静止同步不会跳动。无效阈值保留设置，后续 `.follow_end(false)`
可禁用跟随。空数据／隐藏／错误不会消耗首次请求；隐藏／错误不会覆盖最近成功的跟随几何，
失败布局不会发布滚动校正。
该策略仅增加常量状态，不扫描数据、不设定时器、不增加挂载项。

VirtualWrap 支持 `.content_align(ListContentAlign::Start | Center | End)`，用于
内容不足视口时的行对齐。Center/End 填满视口高度，空数据也适用；Start 保留自然高度。
共享 `UniformWrap::with_content_align` 统一项目与 handle 坐标、导航、可见范围、预测和
内容尺寸。直接使用共享几何时，视口变化后应重新应用对齐；组件会自动处理。
内容对齐不会开启首次跳尾或尾端跟随，需分别配置。


VirtualWrap 支持全宽滚动 `.header(height, content)`、`.footer(height, content)` 和
`.empty(height, content)`；空态位于头尾之间，仅在零数据时显示。高度包含所需间距，
`item_gap` 只作用于数据行列。最多三个用户节点保留身份，跨数据切换、隐藏和错误
恢复时暂存；它们不属于数据索引、可见性回调或数据项挂载预算。共享
`UniformWrap::with_content([before, after], viewport, align)` 统一包含头尾的物理尺寸、
导航、锚点、预测及跟尾坐标，`content()` 返回含对齐留白的几何。对齐作用于完整内容；
`grid()` 保留原始行几何。组件在视口未测量或错误时暂存槽节点。空数据保留可用
宽度供全宽内容布局，仍不产生数据行列。


VirtualList 的 `.use_parent_scroll()` 将完整长度内容嵌入最近的同轴注册滚动祖先，
可穿过正交滚动容器；自然布局坐标确定起点，有符号局部偏移与完整父视口共同用于
可见性、停留和加载边界。句柄跳转与焦点定位写入父容器，离屏列表不挂载普通
overscan 行，显式保留行仍存活。测量只补偿完全位于视口上方的行；整个列表位于
视口上方时也补偿总长度变化。此模式关闭倒序、初始/自动跟尾和键重排锚定。
祖先缺失或隐藏时延迟跳转；窗口视口不是隐式回退，固定视口高度与私有滚动条设置不生效。

VirtualList 会自动保留含当前键盘焦点、有效非空文本选区或已声明 portal
的已挂载行；portal 提升后的来源槽位同样有效。离散行保持自然位置，不改变滚动
和可见范围回调；交互结束即释放。稳定键可定位移动后的行，已删除键和无效选区
不会保留。内容指纹变化仍重新构建行内容；无键数据替换不保留旧身份。共享查询
`jadix::utils::virtual_list::subtree_needs_virtual_retention` 可供其他虚拟化适配器复用。
常规开销随已挂载子树增长：稳定键未移动时检查缓存索引，测量模式移动时查索引表，
固定高度的键移动只共用一次数据扫描。

VirtualList 的 `.retained_indices_fn(|| vec![overlay_host, selection_endpoint])`
显式保留视口外的数据行，不触发滚动；每帧轮询，忽略重复及越界索引。
离散间隔占位保持行的自然位置和完整内容尺寸，只额外挂载请求的离屏行及当前吸顶标题。
覆盖双轴、测量尺寸和倒序列表，可见性与停留时间回调仍以实际视口为准。
数据移动时由调用方更新保留索引，`item_key_fn` 保留存活节点；无需状态时应释放索引。
该入口不自动发现弹层或选择端点，也不施加未声明的保留数量限制。

VirtualList 支持 `.sticky_header_indices([0, 20, 50])`，适用于横向、纵向及固定/测量
行尺寸；`.sticky_header_offset(px)` 及 `_fn` 版本设置有符号视口偏移。仅额外挂载
当前吸顶标题，下一标题推离和内容末尾约束复用共享几何。偏移变化保留节点且不调用
`render_item`；列表自有包装节点保护用户子节点样式和自然尺寸测量。
`sticky_header_indices_fn` 仅在数量或 data key 改变时重算，分组变化需更新 data key。
句柄 `sticky_header()` 返回最近同步的标题；可见性和停留时间回调仍按自然几何计算，
不扣除吸顶标题的遮挡面积。

VirtualList 配置 `item_key_fn` 后默认在前插、删除和重排时保留首部稳定键及
行内偏移，覆盖双轴和测量模式。锚点删除时先选后继存活旧行、再选前驱，
并归零行内偏移；整批替换保留像素偏移并按末尾夹取。可通过
`.maintain_visible_content_position(false)` 关闭键变更时的位置保持。
同数量修改必须更新 `data_key`；主动导航和 `follow_end` 优先。普通帧复用键，
仅变更批次建立临时索引。固定几何仍为 O(1)，配置稳定键时存储为 O(N)。
视口和布局动画的偏移同步重定位。

VirtualList 的 `.header(extent, node)`、`.footer(extent, node)`、`.empty(extent, node)`
支持双轴固定／实测列表的滚动页眉、页脚和空态。尺寸包含插槽自身所需的间距，
内容应适配该尺寸；`item_gap` 只分隔数据行。空态仅在无数据时放在页眉页脚间，
有数据时暂存并保留节点。倒序列表同时反转页眉页脚位置。插槽最多三个，持续挂载，
不进入行测量及行复用计数；跳转、锚定、可见比例和吸顶共用完整内容坐标。
末个吸顶行在页脚前停止；视口仅显示插槽时不报告可见行。
不提供插槽自动测量或隐式插槽间距。

VirtualList 的 `.content_align(ListContentAlign::Center)` 或 `End` 支持双轴短内容
居中／末端排列，覆盖固定与实测尺寸。尺寸变化复用 spacer 和已挂载行；
`row_offset` 包含首部留白，`index == count` 的末尾哨兵和视口 `total_extent`
包含两端留白，空数据也填满视口。位置／可见区回调继续描述行；短内容标题在
吸顶偏移到达其自然位置之前不吸顶。默认 Start 保持自然尺寸。
`.follow_end_threshold(10.0)` 启用距离旧末尾十个逻辑单位内的自动跟随，滚远后暂停；
非法距离保留原配置。`.follow_end(false)` 关闭跟随，`.follow_end(true)` 保留已配置
阈值（默认零）。对齐、跟随与 `.initial_scroll_at_end()` 分别配置；聊天列表可组合
End 对齐、初始末端和跟随。显式导航继续优先。

VirtualList 的 `.inverted()` 将逻辑第 0 行放在底部／右端，默认从该处打开，
短内容和空内容贴末端。支持双轴、实测尺寸、间隔、焦点和句柄跳转。渲染／键／
焦点回调、视口区间、停留成员和加载边缘均按数据索引解释；位置回调报告视觉
首行及其从上／左边缘算起的行内偏移。滚动坐标、跳转对齐及 `scroll_to_end()`
仍按视觉方向，末端指底部／右端。稳定键在变更时保持视觉首行键；无用户键的
固定尺寸列表以 O(1) 空间保持距末端距离，测量列表复用原测量索引并以逻辑索引
作为身份。同数量修改仍需更新 data key。末端吸附默认开启，可由
`follow_end(false)` 关闭；无键固定列表的距末端保持仍生效。显式跳转优先，
此模式禁用吸顶标题。倒序在构造时固定，不变换用户子节点。

VirtualList 可用 `.scroll_ahead(true)` 在 `.overscan(n)` 外增加最多一个视口的定向
预渲染。双轴、测量行及倒序列表复用共享 `ListScrollAhead`；静止同步保持挂载窗口。
数据修订、视口变化、锚定和跳转会重置预测。可见性、viewability 和加载回调仍使用
真实视口。额外像素范围有界，但不限制任意微小测量行的数量；此功能默认关闭。

VirtualGrid 与 VirtualWrap 同样支持默认关闭的 `.scroll_ahead(true)`。网格两轴
分别预测，换行网格仅沿纵轴预测，复用共享核心的预测器和有界窗口查询。
预测超过 `max_mounted_cells` 时回退正常窗口，保证正常可见单元继续挂载；基础
overscan 仍计入预算。视口宽高、尺寸／数量、数据修订、锚定修正及导航重置历史，
静止同步保留节点。句柄与视口回调只报告真实可见范围；换列保留既有数据项锚点。
