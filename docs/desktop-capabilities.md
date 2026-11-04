# 桌面窗口、区域穿透、3D 与 Live2D

新增窗口接口位于 jadix_winit 的 Window trait，Jadix 的 WindowHandle 提供对应便捷方法。
它们不依赖 jadix_native；本次没有修改 jadix_native 的宿主或呈现协议。

## 窗口和指针

在事件回调或 UI 构建作用域内取得当前窗口：

```rust
use jadix::app::{current_window, PointerPassthrough};

let window = current_window().ok_or("no current window")?;
let pointer = window.global_cursor_position()?;
let monitors = window.available_monitors()?;
for monitor in monitors {
    println!("{:?} {:?}", monitor.name(), monitor.size());
}
window.set_opacity(0.75)?;
window.set_click_through(true)?;
window.set_pointer_passthrough(PointerPassthrough::empty())?;
```

WindowHandle 可以保存到应用状态。current_window / current_global_cursor_position 依赖当前窗口作用域；
后台任务应使用已经保存的句柄。available_monitors 每次查询当前显示器集合，支持热插拔后的再次枚举。
current_monitor / primary_monitor 返回 Option，不能假定系统一定提供主显示器。

| 能力 | Windows | macOS | Linux X11 | Linux Wayland |
| --- | --- | --- | --- | --- |
| 整窗 OS 点击穿透 | 重定向窗口原生 hit-test 开关 | ignoresMouseEvents | 原生输入区域 | 原生输入区域 |
| 按左/右/中键、侧键、滚轮分别穿透 | 暂不支持 | 暂不支持 | 暂不支持 | 暂不支持 |
| 全局鼠标位置 | GetCursorPos | NSEvent.mouseLocation | XIQueryPointer | 返回 NotSupported |
| 显示器枚举 | 原有 winit 后端 | 原有 winit 后端 | 原有 winit 后端 | 原有 winit 后端 |
| 整窗透明度 | layered window | NSWindow.alphaValue | _NET_WM_WINDOW_OPACITY | 返回 NotSupported |

PointerPassthrough 定义 LEFT、RIGHT、MIDDLE、BACK、FORWARD、WHEEL、MOTION、OTHER_BUTTONS 和 ALL。
ALL 关闭窗口命中测试，空集合恢复；部分集合目前在所有后端明确返回 NotSupported，且不修改原状态。
这次尚未实现逐按键 OS 穿透。Jadix 的 pointer-events / on_click 过滤只影响应用内部事件分发，不能替代该功能。

set_opacity 接受有限的 0..=1，1 表示完全不透明。Windows 使用 SetLayeredWindowAttributes，
透明度和整窗穿透分别保存状态，恢复其中一个不会意外撤销另一个。
使用 WS_EX_NOREDIRECTIONBITMAP 的 DirectComposition 窗口在透明度和启用穿透时返回 NotSupported；不能强行添加 layered
样式破坏已有合成链。普通重定向窗口可用，X11 属性是否显示由合成器决定。
移动系统和 Web 的新增全局指针/透明度接口返回 NotSupported。

MouseEvent 的 client_x / client_y 是当前事件的窗口逻辑坐标，x / y 会随元素事件分发转换。
新增 desktop_position: Option<[f64; 2]> 将事件的 client 坐标通过窗口原点转换到桌面物理坐标；
它对应事件发生的位置，区别于稍后查询鼠标当前位置。无法获得绝对窗口位置时为 None，
例如 Wayland。screen_x / screen_y 在位置可用时同步更新；位置不可用时保留兼容值，
需要判断桌面坐标是否有效的代码应读取 desktop_position。
macOS 使用当前窗口的 scale_factor 转换，坐标约定与 winit 的窗口/显示器位置一致。
OS 穿透期间，本窗口不会收到穿透的点击，不能依赖这些事件记录全局点击。

可在 Windows 运行隐藏窗口探针，校验原生 alpha、输入样式恢复、全局位置和显示器枚举：

```powershell
cargo run -p jadix_winit --no-default-features --features system-tray --example desktop_probe
```

## OS 区域穿透

Window trait 和 Jadix WindowHandle 均提供 set_input_region。区域坐标为客户区物理像素；
多个矩形取并集，并集内接收输入，并集外透传。矩形为左闭右开、上闭下开。
窗口缩放、DPI 或布局改变后，宿主应重新提交区域。

~~~rust
use jadix::app::InputRegionRect;

window.set_input_region(Some(&[
    InputRegionRect::new(0, 0, 240, 48),
    InputRegionRect::new(80, 80, 160, 120),
]))?;
window.set_input_region(Some(&[]))?; // 空区域：整窗无输入。
window.set_input_region(None)?;      // 恢复默认区域。
~~~

- X11 使用 Shape Input，Wayland 使用 wl_surface 输入区域：不裁剪可见画面。
  Wayland 在宿主的下一次 surface commit 时生效，接口会请求重绘而不擅自提交 surface。
- Windows 使用 SetWindowRgn，**同时裁剪可见画面与输入**，不是保留区域外可见内容的
  input-only 实现。适用于普通窗口及无重定向窗口，但后者仍不能启用 layered 整窗透明度/穿透。
  若必须保留区域外绘制内容，应将此 Windows 后端视为不满足该用途，而非透明叠层方案。
- macOS、移动端和 Web 当前返回 NotSupported。不支持的平台不会静默退化为 UI 事件忽略。
- 区域作用于所有鼠标按键及滚轮，并非按键级穿透；pointer_events 仍只控制 UI 内部命中，
  不会把已交给当前原生窗口的事件转发给其他进程。不要同时启用整窗无输入又期待区域内可点击。
- Windows 隐藏窗口探针验证区域并集、空区域、恢复和溢出拒绝；尚未做跨进程真实点击验证。
  X11/Wayland 仅完成跨目标编译，未在对应桌面会话验证实际输入。

## 登录启动

启用 Jadix 的 autostart feature，直接复用 auto-launch 0.6。
没有新建 jadix_autostart，也没有复制上游实现。此特性不要求 gpu，允许仅使用 Jadix 的非 GPU 接口。

```rust
let mut builder = jadix::autostart::current_app("com.example.desktop-widget")?;
builder.set_args(&["--background"]);
let launch = builder.build()?;
let enabled = launch.is_enabled()?;
// 在用户设置确认开启时调用 launch.enable()?；关闭时调用 launch.disable()?。
```

current_app 默认选择 Windows 当前用户、macOS LaunchAgent、Linux XDG Autostart。
创建 builder / handle 不会自动注册，也不会因启动应用而擅自更改系统设置。
其余上游选项直接由 AutoLaunchBuilder 公开，包括 macOS 的应用包模式、参数与平台模式。
打包应用如需传入应用包路径，应直接使用 AutoLaunchBuilder 并明确配置路径。
此处的“开机自启”是当前用户登录后启动，不是系统服务或登录前运行。
Android / iOS / OHOS / Web 不编译该依赖或公开该模块。

## 托盘

保留已有 jadix::app::tray_icon、TrayMenuItem 与 TrayIcon；启用 system-tray 后使用现有 tray-icon 后端。
优先调用 try_build，处理平台不支持或原生创建失败；把句柄保存在应用状态，释放后图标会移除。

新增 jadix_winit/system-tray 可选特性，公开 jadix_winit::tray，直接复用同一 tray-icon 的图标、菜单和事件接口。
这使直接使用 jadix_winit 的宿主也能创建托盘，无需另建 jadix_tray。
应在系统 UI 线程创建/更新托盘，并将托盘事件接回自己的事件循环。
Linux 后端使用 ksni；显示还取决于桌面环境是否提供 StatusNotifier 支持。
默认构建不引入托盘依赖，也不改变 jadix_native 的运行方式。

## glTF / GLB / VRM 场景

新增独立 jadix_3d crate，负责资源、场景树、动画采样、蒙皮和 GPU 绘制。
它可由任何使用工作区同一 wgpu device 的宿主直接使用，不依赖 Jadix UI 或 jadix_native。
Jadix 的可选 scene3d-models 特性把它接入已有 scene3d 离屏纹理和损伤刷新流程；
原 scene3d 的轻量立方体路径保持可用，默认构建不加载 glTF 依赖。

```rust
use jadix::primitives::scene3d::{jadix_3d::Scene, scene3d, SceneView};

let scene = Scene::load("avatar.vrm")?;
let view = SceneView::new(scene);
if !view.scene().animations.is_empty() {
    view.set_animation(Some(0))?;
}
// 将 view 保存在应用状态或 UI 闭包中，生命周期覆盖整个视口。
let element = scene3d().model(&view).w(640.0).h(480.0);
```

实际支持：

- glTF/GLB 外部或内嵌 buffer/image；.vrm 按 GLB 导入。
- 所有节点、父子变换、命名场景及 select_scene，多 mesh / primitive / 实例。
- 透视与正交相机、方向/点/聚光灯；视口可用 SceneView::set_camera_node 选择文件中的相机。
- 三角形、triangle strip / fan；顶点颜色、基色贴图与采样器 wrap/filter。
- 基色、metallic-roughness、normal、occlusion、emissive 贴图及因子，独立采样器；基色/emissive 按 sRGB、其他贴图按线性空间读取。
- unlit、alpha mask / blend、双面材质与深度缓冲。
- 四关节权重蒙皮、inverse bind matrix、morph position/normal 和 morph 权重动画。
- 平移/旋转/缩放/权重动画，STEP / LINEAR / CUBICSPLINE；旋转使用四元数。
- VRM 0.x / VRMC_vrm 1.x 根扩展元数据与 humanoid 骨骼映射保留。VRM 必需扩展不再被 glTF 库误拒绝，其他必需扩展仍按 glTF 库校验。

独立使用时调用 Scene::pose(clip, seconds, repeat)，将 Pose 交给 Renderer::render；
或使用 LocalPose / Animator 实现暂停、跳转、速度、循环和可中断的交叉淡入淡出。
Scene::bounds / raycast、Camera::ray 和 OrbitCamera 提供动画后包围盒、三角形拾取与轨道相机；
geometry 提供平面、立方体与球体；humanoid_bone / apply_expression / solve_ik 提供 VRM 骨骼查询、morph 表情和 CCD IK。
SceneView 的便捷接口按视口时钟循环播放选定 clip，None 为绑定姿态。
SceneView 使用弱注册表且在释放时清理；树中只有标识，因此必须保留句柄，不能只在构建函数内创建临时句柄。
视口沿用已有 1024×1024 像素总量上限。cube 的 label/light/material/yaw 设置目前只作用于旧立方体，
模型使用导入的材质/灯光及相机；模型无灯时提供默认光。

当前边界：CPU 执行蒙皮/morph 后上传顶点，尚非 GPU 蒙皮；最多八个可见 punctual lights。
没有阴影、IBL 或 mipmap 生成；纹理仅支持 UV0，无 UV0 或其他 UV set 会明确报错。
没有 VRM MToon、spring bone 物理或 retarget；表情仅处理 morph binds，未处理材质/纹理变换 binds。
CCD IK 在非均匀缩放下为近似解。
VRM avatar 的基础网格、骨架和已有 glTF 动画可用，不能宣称完整 VRM 渲染规范已实现。
超过四个关节影响、非三角形图元和不支持的必需压缩/材质扩展会返回错误。

运行模型视口及测试：

```powershell
cargo run -p jadix --features scene3d-models --example model_scene -- path/to/avatar.vrm
cargo run -p jadix_3d --example inspect -- path/to/model.glb
cargo test -p jadix_3d --features gpu-tests -- --nocapture
```

gpu-tests 显式启用原生后端并要求可用适配器。测试构造真实 GLB，验证层级、蒙皮、morph、
各类插值、时间边界、VRM 必需扩展、动作混合、IK、表情、拾取和错误输入，
并通过 GPU 像素读回验证骨骼运动、RGBA/BGRA、贴图、PBR maps、颜色空间及深度遮挡。
如果禁用 Jadix 默认特性，运行模型视口时须另选 backend-dx12、backend-vulkan、backend-metal 或 backend-gles。

## Live2D（可选，实验性兼容）

独立 jadix_live2d crate 不依赖 jadix_native；通过 jadix/live2d 可重导出为 jadix::live2d。
两个模型 crate 均不在 workspace default-members 中，jadix 默认特性也不引入它们；
显式 --workspace 或启用相应特性才会构建。Cargo.lock 中的资源解析不代表默认编译。

Player 加载 model3/moc3、纹理、motions、expressions、pose3、physics3，提供动作播放、
表情、视线、眨眼、呼吸、宿主提供的口型幅度及参数覆盖。update 使用有限步长驱动运行时。
View::fit 负责等比适配和物理指针到模型坐标转换，可配合 runtime.hit_test 使用。
GpuModel 持有 GPU 资源，更新变形顶点，编码普通/加法/乘法混合、正向/反向裁剪遮罩，
使用宿主同一个 jadix_wgpu device、目标纹理和提交队列。
直接依赖时 default-features = false 可只使用 CPU；native-backends 显式启用原生 GPU 后端。

**这不是完整官方 Cubism 支持。** 当前复用实验性纯 Rust Mocari 0.4.0，未接入官方 Core，
未验证所有 moc3 版本、新版 Cubism 特性、复杂遮罩模型或 SDK 动作优先级/事件行为。
没有捆绑专有二进制及模型，宿主必须取得模型/SDK 授权，并自行负责音频采集与播放。
合成网格 GPU 测试不能替代真实授权模型验收。

~~~powershell
cargo test -p jadix_live2d --features gpu-tests -- --nocapture
cargo run -p jadix_live2d --example inspect -- path/to/avatar.model3.json
cargo run -p jadix_live2d --features native-backends --example render_model -- path/to/avatar.model3.json avatar.ppm 2 Idle 0
$env:JADIX_LIVE2D_MODEL = 'path/to/avatar.model3.json'
cargo test -p jadix_live2d --test real_model -- --ignored --nocapture
~~~

render_model 输出带深灰背景的离屏 PPM；时间、动作组与索引可省略。
真实模型测试默认明确 ignored；没有授权资产时不将其计入已通过的兼容性验证。
更多 API 见 crates/jadix_live2d/README.md。
