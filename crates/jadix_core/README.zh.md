# jadix_core

[English](README.md) | [简体中文](README.zh.md)

无 UI 依赖的 Owner 作用域、Signal、Store 与响应式调度，不安装窗口或绘制器。

实验性的 `presentation` 模块提供共享节点／提交、事件、测量与矩形混合几何协议。
GPU 门面以 `jadix::presentation` 重导出同一模块；无窗口消费者可以直接使用本包，
无需引入渲染依赖。

本包属于 [Jadix](https://github.com/Zk3619/Jadix)。**Alpha；版本 `0.1.0`。**

## 宿主环境

`environment::{EnvironmentHost, Environment, EnvironmentSnapshot}` 复用 Store／Memo，提供每根独立的环境 writer、只读投影及修订冲突拒收；在 reactive owner 内创建并通过既有 Context 传递。相同快照不通知；投影可能重算，相等结果不会再次执行下游 effect。支持地区／语言顺序、带种类的时区、小时制、ISO 周首日、计量／温度单位、主题、减少动画、生命周期及宿主字号倍率，未知 OS 值保留 None。

JSON 入口限制 16 KiB、严格字段、语言和数值预算；不实现完整 CLDR／TZDB 查询。可选 `system-environment` feature 只读 Windows Win32 偏好，以及 Linux 进程 locale／时区与可用的 GNOME GSettings。其他系统／关闭 feature 显式 Unsupported。Linux 可读 GNOME 主题、小时制、动画和文字倍率；未知值保留 None，POSIX TZ 表达式不冒充 ISO offset。时区不修改进程 TZ，字号倍率不是 DPI，Windows adapter 不宣称读取系统文字缩放。

`SystemEnvironmentWatcher::start(notify)` 持有一个 worker 和最多一个最新快照。Windows 使用隐藏顶层窗口接收设置／主题／时间／显示广播；Linux 使用 GSettings monitor 与 `/etc` inotify。相等快照过滤，回调只唤醒宿主；宿主阻塞前 drain `take()`，在 UI 线程用 EnvironmentHost 发布并保留宿主生命周期。close／Drop 停止并 join，回收窗口／进程／fd 和 pending。Linux 实时桌面监听要求可用的 GNOME schema 与设置会话，monitor 退出明确报错；每个读取子进程限时两秒。此能力不引入 renderer／VM 依赖。

## 安装

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

默认开关：无。完整 feature 与依赖关系以 [Cargo.toml](Cargo.toml) 为准；存在开关不代表平台行为已完成验证。普通应用通过门面选择内部 GPU／窗口后端，不直接依赖平台内部实现。

## 文档

[项目文档](https://github.com/Zk3619/Jadix/tree/main/docs)介绍应用配置、平台接入、功能选择与 API 用法。

## 许可证

Apache-2.0。保留 [LICENSE](LICENSE) 及依赖的来源声明。
