# 启动准备与开屏交接

应用通过 `StartupConfig` 显式选择 `System`、`Component` 或桌面 `Window`，使用相同的准备任务、取消、失败、重试与首帧交接。未配置启动协调器时保持普通入口行为。开屏能力由独立的 `jadix_splash` crate 提供；`jadix` 仅保留窗口宿主、渲染与帧交接适配，不依赖 `jadix_splash` 或 `jadix_components`。

```rust,no_run
use jadix_splash::prelude::*;

fn main() {
    let startup = StartupConfig::component(|snapshot, _controller| {
        text(snapshot.stage).into()
    }).prepare(|controller| {
        controller.stage("Preparing assets");
        controller.ready();
    });
    app().title("Application").startup(startup).mount(|| {
        text("Application ready").into()
    });
}
```

`prepare` 在 UI 线程执行一次，必须及时返回；后台 IO／CPU 工作使用 `StartupConfig::blocking(executor, work)`，执行器采用既有 `BlockingExecutor` 契约。应用持有执行器和需要提交的数据；`StartupController` 可跨线程报告状态，不能用它操作未挂载的 Signal 或 Element。`blocking` 的 `Ok(())` 自动报告 ready，错误、执行器拒绝、任务被丢弃与可捕获的 panic 报告失败；合作取消报告 Cancelled。`panic=abort` 无法恢复。

`stage` 报告真实阶段，`progress(completed, total)` 只在总量已知时报告进度；总量为零表示不确定。共享状态只存最新快照，终态一经写入就忽略迟到的进度和结果。失败／取消后显式 Retry 建立新 epoch；业务应保证重试时副作用边界清楚。surface 丢失与重建只改变显示代次，保留应用的准备结果。

## 独立 crate 与依赖边界

```toml
[dependencies]
jadix = { version = "0.1.0", default-features = false, features = ["backend-gles"] }
jadix_splash = { version = "0.1.0", default-features = false, features = ["system", "components", "macros"] }
```

所有 feature 默认关闭：`system` 只为支持的桌面目标引入 softbuffer；`components` 引入产品组件依赖并提供 `Splash`／`splash()`；`macros` 开启 RSX。只使用 System 或自定义 Element 工厂时不需要 `components` 或 `macros`。GPU 后端与透明窗口合成仍在 `jadix` 上选择。

`StartupConfig`、`StartupController`、`StartupPhase`、`StartupSnapshot`、`SplashMode` 从 `jadix_splash` 导入；产品 `Splash` 也已迁入，旧的 `jadix`／`jadix_components` 路径不再提供它们。`AppBuilder::startup` 和 `run_with_startup` 接受外部开屏提供者，调用方式不变。新 crate 通过运行时 `StartupHost` 契约接线，单向依赖为 `jadix_splash → jadix`；可选产品组件分支为 `jadix_splash → jadix_components → jadix`，没有反向依赖或循环。

OS 在进程运行前使用的 Android／Apple／OHOS 启动资源仍由应用宿主打包，不能挪进 Rust 库后期创建。图片、GIF、帧流和 Video 表面继续复用 `jadix` 媒体能力，不复制播放器或窗口系统。

## 三种呈现方式

| 选择 | 行为 |
| --- | --- |
| `StartupConfig::system()` | 桌面启用 `jadix_splash/system` 后，以 softbuffer 在同一主宿主提交 CPU 图像，再接中性等待／错误视图或业务首帧。移动端保留打包的 OS 启动资源。 |
| `StartupConfig::component(factory)` | 最小 GPU／文字资源可用后，在同一宿主挂载工厂返回的 Element；可选择产品 Splash 或自己的组件。 |
| `StartupConfig::window(options, factory)` | 最小渲染运行时可用后，在独立桌面次窗口挂载相同类型的 Element；准备期间隐藏主窗口，主窗口内容有效呈现后销毁开屏窗口。 |
| `.enabled(false)` | 关闭额外品牌呈现，保留准备任务、必要中性等待／错误与内容首帧门。移动系统启动屏由 OS 控制。 |

System 的桌面 CPU 呈现需要可选 feature；缺少 feature 或平台能力时记录 `System Unsupported` 与实际中性回退，不自动选择产品 Splash。System／Component 复用主窗口，不创建临时品牌窗口；显式 Window 模式复用既有次窗口路径，透明开屏可能使用独立合成 GPU。Component／Window 无法覆盖渲染器初始化前的时间；移动端的系统启动屏也不能被它们禁用。

准备已完成时，首个有效内容帧直接完成交接，不强制补播启动组件。准备尚未完成时，当前 surface 的 StartupFrame 接管平台覆盖；Ready 后，当前代次 ContentFrame 完成交接。零尺寸、后台、旧代次与 bootstrap 空根不算有效内容帧。`present` 是应用提交证据，真实用户可见性仍需平台捕获确认。

业务根可以使用导航宿主或响应式子树替换。启动帧身份由运行时的稳定宿主层持有，不依附于业务根的模板身份；应用无需手工添加内部帧属性。

Windows 最小化期间不会将 GPU 提交确认成可见内容首帧。独立 Window 失焦或最小化时，准备任务可以完成，但不会自动显示／激活隐藏的业务主窗；显式恢复并激活开屏后继续交接。二次启动优先唤醒当前可见开屏，而不是提前暴露隐藏主窗；交接后正常恢复业务主窗。

产品进度条通过原生可访问性桥提供只读 0–100 范围和当前值，Cancel／Retry 复用按钮动作。Windows UI Automation 首次订阅会主动请求完整树，不依赖开屏动画继续刷新；交接后提供业务语义，不保留已销毁开屏的操作。其他平台的读屏能力仍取决于各自可访问性适配，不由 Windows 的实现推断。

## 产品 Splash 与双写

```rust,ignore
use jadix_splash::prelude::*;

let startup = StartupConfig::component(|snapshot, controller| {
    let brand: Element = text("My application").size("2rem").into();
    ui! {
        <Splash title="Welcome" state={snapshot} controller={controller}>
            {brand}
        </Splash>
    }
});
let same_view: Element = splash().title("Welcome")
    .description("Preparing local assets").child(text("My application")).into();
```

`Splash` 与 `splash()` 使用同一实现；支持标题、说明、快照、控制器、背景颜色、媒体底层与子内容插槽。组件显示可量化百分比、状态播报、取消／重试按钮；内置操作文案支持 i18n 的英文和中文默认值。业务阶段、错误及品牌文本由应用提供。组件不执行任务、不改变 OS 窗口、不用计时器自动放行。

## 可选无边框

Windows 的无边框同时关闭系统投影、DWM 边线与强制圆角，不只是隐藏标题栏；透明／模糊路径也禁用非客户区绘制，避免材质扩展帧重新带出阴影。应用自己的描边、圆角或阴影仍由组件样式控制。只配置 `.transparent(true)` 不代表无边框，需要配合 `.decorations(false)`（主窗口可用 `.frameless()`）。

独立开屏保留传入的 `WindowOptions` 装饰设置；可用 `.borderless(true)` 显式去掉系统边框和标题栏，用 `.borderless(false)` 恢复系统装饰。也可直接在 `WindowOptions` 上设置 `.decorations(false)`。这与透明、模糊独立，不要求启用透明或 DComp。

```rust,ignore
let startup = StartupConfig::window(
    WindowOptions::new().title("Starting").size(480.0, 320.0),
    |snapshot, controller| splash().title("Application").state(snapshot).controller(controller).into(),
).borderless(true);
app().title("Application").startup(startup).mount(app_root);
```

该开关仅影响独立开屏次窗口，关闭／交接后不会改变主窗口的边框。System／Component 与业务页共享主窗口，`.borderless(...)` 不修改它们的宿主；若需要只让开屏无边框而业务窗口保留装饰，应选择 Window 模式。移动 OS 启动屏由宿主和系统控制。示例 `startup`／`startup_media` 支持 `--window --borderless`。

## 高级动画、透明窗口与媒体

高级定制应优先选择 `StartupConfig::component`，而不是把系统启动资源改造成视频播放器。系统／宿主早期占位与运行期动画是两个阶段：前者覆盖渲染器不可用的时间，后者复用主窗口现有渲染器。移动系统启动资源仍需保留；选择 Component 不代表可以移除它们。

`Splash` 的 `background` 默认是主题 `background`，可显式指定 `transparent`；`backdrop` 接受任意 Element，以绝对定位铺满开屏且不参与命中，不被辅助技术重复播报。标题、状态、进度、取消／重试与 `children` 在前景。媒体层不能截获这些控件的输入。无需产品布局时，工厂也可直接返回自己的 Element 树。

```rust,ignore
let startup = StartupConfig::component(|snapshot, controller| {
    let media: Element = image().texture("startup-frames")
        .class("w-full h-full object-contain").into();
    ui! {
        <Splash title="Welcome" background="transparent" backdrop={media}
            state={snapshot} controller={controller}/>
    }
});
// Builder 等价：splash().background("transparent").backdrop(media).state(snapshot)
// 桌面窗口也必须允许 alpha；仅清空组件背景不能让不透明宿主透出桌面。
jadix::app::app().transparent(true).frameless().startup(startup).mount(app_root);
```

| 素材／效果 | 接入方式与约束 |
| --- | --- |
| 静态图片／透明 PNG | `image().src(...)` 或预加载后使用 `texture`；解码失败应保留可读状态与重试，不阻塞 UI 线程 |
| GIF | `image-gif` feature 和现有图像解码／纹理路径；只为实际绘制的 GIF 维持帧泵，开屏退场后停止推进，CPU 解码缓存仍可保留；减少动态效果时不推进帧 |
| 自定义逐帧图像 | `queue_media_frame(key, MediaFrame::rgba8(...))`；同一纹理键只保留最新待上传帧，队列由空转非空时唤醒事件循环，不依赖鼠标或阶段进度更新；生产者必须响应当前启动取消令牌并在 Ready／失败／关闭前退出 |
| 视频 | 复用 `video().controller(...)`／`MediaSnapshot.video_texture` 显示外部解码器输出；它不是 MP4 解码器。现有 I420／NV12 需 `image-stream`；透明视频是否有 alpha 取决于解码输出，不由 Splash 补出透明度 |
| 位移／缩放／淡入淡出 | 在自定义 Element 上复用现有 motion／CSS 动画，遵循 `prefers_reduced_motion`；动画结束不是业务准备完成条件 |
| 桌面透明／系统材质 | `.transparent(true)` 配合透明组件；按需 `.frameless().blur(true)`。合成器、GPU 后端和平台可能限制材质与 alpha，不能当作全平台保证 |

### 独立开屏窗口

需要独立尺寸、透明轮廓或不同于业务窗口的装饰时，可显式使用额外的原生次窗口。它是独立顶层宿主，不是裁剪在主窗口内的子控件，也不是独立进程／专用 UI 线程。普通产品仍可优先选择同主宿主的 Component。

```rust,no_run
use jadix_splash::prelude::*;

let startup = StartupConfig::window(
    WindowOptions::new().title("Starting").size(480.0, 320.0)
        .resizable(false).decorations(false).transparent(true),
    |snapshot, _controller| {
        text(snapshot.stage).into()
    },
).prepare(|controller| {
    controller.ready();
});
app().title("Application").size(1000.0, 720.0).startup(startup)
    .mount(|| text("Application ready").into());
```

- 仅支持具备多窗口能力的桌面宿主；移动端和 Web 记录不支持并回退同宿主 Component。独立窗口创建／渲染资源建立失败也回退，不重复准备任务。
- 准备期间主窗口保持隐藏且不挂载业务根；失败／取消留在同一开屏窗口，Retry 更新 epoch 而不新建窗口。准备已经完成则直接显示业务，不强制创建或补播开屏。
- Ready 后先显示并呈现原主窗口，确认当前 epoch／surface 的有效 ContentFrame，再销毁开屏窗口；不把隐藏预绘、动画结束或计时器当作交接。显示提交与真实桌面合成仍是不同的验证层。
- 提前关闭开屏按应用退出处理，取消准备并释放隐藏主窗口；沿用窗口生命周期关闭策略。开屏不参与窗口位置持久化，且随主窗口关闭，不支持把它留作常驻工具窗。
- 开屏 `WindowOptions` 不改变业务主窗口的尺寸、透明度、装饰。GIF、帧流、Video 表面及减少动态效果沿用相同组件和媒体路径；透明仍需实际支持 alpha 的后端。

次窗口依赖已启动的运行时，不能绕过渲染器初始化，也不能代替移动系统启动资源。焦点、任务栏和跨屏位置仍受平台窗口管理器约束；Wayland 不保证应用能指定全局位置。

可运行示例复用已有媒体能力，不新增播放器或定时调度框架：

```sh
cargo run -p jadix_splash --example startup_media --features components,macros,jadix/default,jadix/runtime-logging,jadix/image-gif -- --transparent
# 窗口材质：--blur；外部视频帧显示表面：--video-surface
# 本地 PNG／GIF：--image /absolute/path/brand.gif；静态退化：--reduced-motion
# Windows 透明／材质宿主：增加 jadix/windows-dcomp feature；GL 的 Opaque 回退不代表实现桌面透出。
cargo run -p jadix_splash --example startup_media --features components,macros,jadix/default,jadix/runtime-logging,jadix/image-gif,jadix/windows-dcomp -- --transparent
# 额外独立窗口；透明／材质仅作用于开屏，业务主窗口保持普通装饰
cargo run -p jadix_splash --example startup_media --features components,macros,jadix/default,jadix/runtime-logging,jadix/image-gif,jadix/windows-dcomp -- --window --transparent
```

示例无素材参数时生成有界 RGBA 帧流；`--video-surface` 只是让同一帧流经过 Video 表面，**不表示已实现文件视频解码**。示例刻意用六秒模拟准备任务，生产代码不应为播完整动画人为延迟 Ready。未加 `--window` 时透明／无边框选项属于整个主窗口，交接后业务根改为不透明背景，不自动恢复系统装饰；加 `--window` 后这些选项仅作用于开屏次窗口。GIF 全帧解码仍会占用素材尺寸 × 帧数的内存，需由应用控制启动资产预算。

渲染器初始化失败时，即使业务准备已 Ready，仍转入可重试失败状态；桌面标题提示 Enter 重试，重用原主窗口与新 epoch。普通组件失败／取消继续通过按钮重试。渲染器不可用时不能承诺绘制产品错误组件，移动端／无标题栏宿主仍需平台错误反馈方案。

运行交互示例：

```sh
# 框架单用，无过程宏、无产品组件；加 -- --component 切换自定义 Element
cargo run -p jadix_splash --example startup_core --no-default-features --features jadix/backend-gles,system,jadix/runtime-logging
# 产品 Splash
cargo run -p jadix_splash --example startup --features components,macros,jadix/default,system,jadix/runtime-logging
# 切换系统模式；--no-splash 禁用品牌，--fail-once 演示失败后重试
cargo run -p jadix_splash --example startup --features components,macros,jadix/default,system,jadix/runtime-logging -- --system
# 独立次窗口；可组合 --fail-once 或 --no-splash
cargo run -p jadix_splash --example startup --features components,macros,jadix/default,system,jadix/runtime-logging -- --window
```

Gallery 原生入口使用 `JADIX_SPLASH=system|component|window|off`，其中 `window` 显式选择独立桌面开屏；Web 使用 `?splash=system|component|off`。`off` 同样经过初始化门。两端默认 Component，快路径可能直接进入内容。框架单用的 `startup` 示例也支持 `--window`，不需要依赖产品组件库。

## 移动宿主与验证边界

| 平台 | 宿主资源与当前边界 |
| --- | --- |
| Android | [NativeActivity 宿主](../examples/gallery_mobile/android/AndroidManifest.xml)、API 28 起的启动背景、API 31 起的 SplashScreen theme、日夜背景和图标；不增加第二个 Splash Activity，不拦截系统首帧等待业务 IO。 |
| iOS／iPadOS | [Xcode 宿主](../examples/gallery_mobile/ios/Gallery.xcodeproj/project.pbxproj)、静态 LaunchScreen storyboard 和 assets；由 UIKit 启动 Rust 主入口。当前 UIKit 后端仍使用 UIApplication 生命周期，尚未实现独立 UIWindowScene／多 scene 启动协调。 |
| OHOS | NativeAbility／XComponent、module 的启动图标／背景与日夜资源；需要对应 SDK 构建 HAP，并在目标设备验证。 |
| Web | HTML 中性占位在实际 StartupFrame／ContentFrame 提交后移除，Wasm 初始化返回本身不算交接；渲染失败显示错误。 |

打包和工具链说明见[移动宿主](../examples/gallery_mobile/README.md)。Android 的启动资源需要通过实际 aapt2 编译、链接，并与 native 库一起打入应用包；仅通过资源编译不能作为设备运行验证。

本 API 与资源接线不表示全平台已验证。macOS、Linux X11／Wayland、Android、iOS、OHOS 仍需各自真实会话／设备的 System／Component、禁用对照、首帧、恢复与退出验证；额外桌面 Window 模式也需各窗口管理器的独立交接、焦点与装饰隔离验证。iOS 多 scene 是当前明确的实现缺口。
