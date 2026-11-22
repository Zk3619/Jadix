# jadix_core

[English](README.md) | [简体中文](README.zh.md)

无 UI 依赖的 Owner 作用域、Signal、Store 与响应式调度，不安装窗口或绘制器。

`UniformGrid::window_with_scroll_ahead` 与 `UniformWrap` 对应方法在正常挂载窗口
外加入有符号像素预测，各滚动轴最多扩展一个视口。真实可见范围和基础 overscan
保持不变；预测超过容量时回退正常窗口，基础窗口自身超预算仍报错。换行网格按
实际数据项计算末行预算。两种查询均为 O(1)，不分配逐格索引，并可直接使用共享
`ListScrollAhead` 的输出。

实验性的 `presentation` 模块提供共享节点／提交、事件、测量与矩形混合几何协议。
GPU 门面以 `jadix::presentation` 重导出同一模块；无窗口消费者可以直接使用本包，
无需引入渲染依赖。

本包属于 [Jadix](https://github.com/Zk3619/Jadix)。**Alpha；版本 `0.1.0`。**

## 宿主环境

`environment::{EnvironmentHost, Environment, EnvironmentSnapshot}` 复用 Store／Memo，提供每根独立的环境 writer、只读投影及修订冲突拒收；在 reactive owner 内创建并通过既有 Context 传递。相同快照不通知；投影可能重算，相等结果不会再次执行下游 effect。支持地区／语言顺序、带种类的时区、小时制、ISO 周首日、计量／温度单位、主题、减少动画、生命周期及宿主字号倍率，未知 OS 值保留 None。

JSON 入口限制 16 KiB、严格字段、语言和数值预算；不实现完整 CLDR／TZDB 查询。可选 `system-environment` feature 只读 Windows Win32 偏好，以及 Linux 进程 locale／时区与可用的 GNOME GSettings。其他系统／关闭 feature 显式 Unsupported。Linux 可读 GNOME 主题、小时制、动画和文字倍率；未知值保留 None，POSIX TZ 表达式不冒充 ISO offset。时区不修改进程 TZ，字号倍率不是 DPI，Windows adapter 不宣称读取系统文字缩放。

`SystemEnvironmentWatcher::start(notify)` 持有一个 worker 和最多一个最新快照。Windows 使用隐藏顶层窗口接收设置／主题／时间／显示广播；Linux 使用 GSettings monitor 与 `/etc` inotify。相等快照过滤，回调只唤醒宿主；宿主阻塞前 drain `take()`，在 UI 线程用 EnvironmentHost 发布并保留宿主生命周期。close／Drop 停止并 join，回收窗口／进程／fd 和 pending。Linux 实时桌面监听要求可用的 GNOME schema 与设置会话，monitor 退出明确报错；每个读取子进程限时两秒。此能力不引入 renderer／VM 依赖。

## 安装

`shortcut::KeyChord` 共享单次快捷键的解析和键帽标签，保留原绑定字符及修饰键顺序。
`CmdOrCtrl` 序列化时保留可移植身份，显示与分发时按当前宿主解析；共享菜单的
`MenuItem::key_chord` 与 Kbd 分组可接收同一模型。字符键要求 Ctrl、Alt 或 Super，
F1–F24 可单独使用；入口限 64 字节，拒收重复修饰键、空白与畸形键名。
键盘布局转换、多次按键序列、焦点作用域及事件消费仍由宿主负责。

在应用中添加依赖：

```toml
[dependencies]
jadix_core = "=0.1.0"
```

平台专用功能需要对应平台的 SDK。

## Feature 与边界

共享 `virtualization::VirtualAxis::append` 原子校验整批 `(key, extent)` 后追加，
不扫描旧 key，保留已测高度和滚动锚点；非法／重复行和超容量不发布半批结果。
有序索引与 Fenwick 更新的摊还复杂度为 O(k log(N+k))，单次 Vec 扩容仍可能复制旧存储。
核心 `virtual_axis_append` release 示例和 `jadix_native` 的真实 Windows
`windows_virtual_append` 集成回归消费同一 API；对照为 Jadix 自身 full replacement，
不代表当前 NSH 或完整 SDK／GPU 端到端 O(k) 已验。

`VirtualAxis::window_with_focus` 可在挂载预算内保留焦点行及前后邻行，供连续键盘遍历使用；
焦点移动后可用 `reveal` 计算最小滚动。具体虚拟列表组件仍须接入此窗口模型和宿主焦点事件；
本 API 不代替平台键盘或读屏导航验收。

`VirtualAxis::window_in_parent` 和 `window_with_focus_in_parent` 将实测行挂载窗口
接入祖先视口，保留有符号局部偏移与完整内容长度。预挂载从裁剪后的相交区扩展；
完全离屏或零尺寸时仅保留显式固定行、焦点及其邻行。所有窗口均按可见、预挂载、
固定和焦点行的去重并集检查容量，重叠行不重复计数。焦点合并复用排序后的挂载索引，
无需复制键。祖先发现、内容布局和实际滚动偏移应用仍由适配层负责。

`VirtualGrid::window_in_parent` 将父视口几何用于实测行高和列宽。任一轴为空或相交区域
没有面积时，两轴均停止普通挂载；显式固定行列仍保留。容量同时约束各轴去重索引数及
单元格乘积。规划期间仅保留连续区间及区间外固定索引，在分配两轴挂载数组之前检查
乘积预算。单轴窗口也复用此规划，避免为普通可见行逐个分配树节点。此处交付共享几何，
渲染与祖先滚动接入仍属于适配层。

默认开关：无。完整 feature 与依赖关系以 [Cargo.toml](Cargo.toml) 为准；存在开关不代表平台行为已完成验证。普通应用通过门面选择内部 GPU／窗口后端，不直接依赖平台内部实现。

## 文档

`virtualization::UniformGrid` 组合两个含间距的定长轴，无需建立逐行、逐列或逐单元格索引。
几何查询、单元格起点、双轴原子定位和受预算约束的矩形窗口均为常数时间、常数存储。
尺寸／偏移／间距数组按 `[x, y]` 排列，构造器数量按 `(行, 列)` 排列。
`GridViewport` 的实际可见区与 `UniformGridWindow` 的 overscan 挂载区分离；任一数量为零
时两个轴均为空，任一视口尺寸为零时不挂载单元格。挂载前检查行列乘积预算及整数溢出。
变长轴和固定行列继续使用独立的 keyed `VirtualGrid`。


[项目文档](https://github.com/Zk3619/Jadix/tree/main/docs)介绍应用配置、平台接入、功能选择与 API 用法。

## 许可证

Apache-2.0。保留 [LICENSE](LICENSE) 及依赖的来源声明。


`virtualization::UniformWrap` 将平面数据集按可用宽度和最小项宽自动分成等宽列，
项高与间距固定，过窄视口使用一个更窄的列。项／行列映射为 O(1)，拒绝末行填充格；
行窗口按实际数据项检查容量。换列前保存 `anchor`，重建宽度后用 `restore_anchor`
恢复同一数据项所在行及像素深度，末端按范围裁剪。连续换列应保留原始数据项锚点，
直到用户滚动，避免逐次改用行首数据项导致漂移。本模型无需渲染器或原生依赖。


`ListStickyHeaders` 一次排序去重分组标题索引；`UniformList::sticky_header` 和
`VirtualAxis::sticky_header` 返回当前标题及其相对自然位置的非负位移，包含下一标题
推离和内容末尾约束，支持有符号视口偏移。查询不分配内存，固定高度为 O(log 标题数)，
变高为 O(log 标题数 * log 行数)。忽略超出数据集的索引，零视口不吸顶。

`ListScrollPosition::remap_keys` 支持数据变更后的稳定键锚定：锚点被删除时，
先找后继存活旧行，再找前驱，并归零行内偏移；无存活旧键返回 None。
`UniformList::anchor` 包含行尾间隔；固定和测量几何均提供 `restore_position`，
统一处理半开行边界和视口末尾夹取。键索引由调用方持有。

`ListContent` 将固定／实测行几何投影到包含前后插槽的完整滚动内容。导航、
锚定恢复、可见比例和吸顶共用物理坐标；只显示插槽时不会伪造可见行。
短内容对齐包含插槽，可见视口比例仍以整个视口为分母，末个吸顶行在页脚前停止。
适配层不分配内存，保留底层查询复杂度。

`ListContentAlign::padding(content, viewport)` 无分配计算主轴短内容的起始／居中／
末端留白。Start 保持自然尺寸，Center 和 End 填满视口，空数据亦适用。
尺寸必须有限且非负；对齐独立于滚动导航和末端跟随策略。

`ListOrder` 在数据索引与双轴正序／倒序视觉顺序之间投影单行和半开区间，
映射互为自身逆运算，验证边界（含空区间），无需倒序集合，空间为常量。

`ListScrollAhead` 根据同步后的物理滚动偏移预测有方向的挂载扩展，时间和空间均为 O(1)。
扩展最多一个视口，反向时立即切换，静止时保留窗口；跳跃、视口变化和隐藏会清除历史。
数据修订、锚定／导航校正时调用 `reset`。预测不改变真实可见范围，也不调度帧。

`ParentViewport` 将内容的自然起点与祖先滚动偏移／尺寸投影为有符号的局部坐标。
固定／实测列表、`ListContent`、`UniformGrid` 与 `UniformWrap` 提供父视口查询：
完全离屏时可见范围为空，网格／换行的预挂载也为空；部分相交仍保留整个父视口
作为可见比例分母。导航返回按父内容总长度夹取的父坐标偏移，边缘加载按父视口
距离触发，完全越过相应边缘后解除触发。插槽保留物理坐标，末个吸顶行在页脚前
停止，完整内容完全离屏时不吸顶。几何查询无分配，固定尺寸保持 O(1)，实测保持
O(log 行数)。适配层须提供正确内容起点、路由父滚动并随布局变化更新；此模型
尚不提供祖先自动发现及布局占位，也不自动启用末端跟随或
反转数据集；逻辑边缘方向由调用方显式传入 `ListOrder`。

`ParentViewport::preserve_change` 仅在旧行／插槽区间完全位于视口上方且内容起点
已经滚过时补偿，部分可见区间的变化正常呈现。返回值按父内容完整滚动范围夹取；
同一轮布局逐次传入上次返回的快照，结束后将 `parent_offset()` 应用到父滚动容器。
每次传入变化后的父内容总长度，包含页面其余内容。`VirtualAxis::measure_in_parent`
以 O(log 行数) 完成测量补偿，内容整体已在视口上方时仍可稳定下方页面；
`splice_in_parent` 对键控区间替换应用相同规则，保留未受影响行的尺寸与间距，
`splice` 提供不带补偿的原子替换。区间替换重建索引，时间为
O((n + k) log(n + k))、空间为 O(n + k)；尾部追加仍可使用增量 `append`。
无效修改保持原索引不变。这些接口提供补偿几何，实际父容器滚动仍由适配层执行。
