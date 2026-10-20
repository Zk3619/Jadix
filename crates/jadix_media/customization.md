# 可定制 VideoPlayer 与扩展接入

2026-10-20。保持默认 shadcn 外观，借鉴 ArtPlayer 的 controls / layers / settings 分层；不移植其 DOM、HTML 字符串、媒体执行或全局插件管理器。

参考的上游原始接口：

- [ArtPlayer](https://github.com/zhw2590582/ArtPlayer)
- [Component / ComponentOption](https://github.com/zhw2590582/ArtPlayer/blob/master/packages/artplayer/types/component.d.ts)：命名控制项、位置、排序、生命周期与 selector。
- [Setting](https://github.com/zhw2590582/ArtPlayer/blob/master/packages/artplayer/types/setting.d.ts)：选择、开关、范围输入与请求回调。

这些链接是设计参考，不是 Jadix 的运行时依赖；本文只描述下列已经实现的界面契约，不承诺已经实现字幕、弹幕、雪碧图或流切换。

## 三个定制层级

1. 零配置：VideoPlayer 保留主题 token、边框卡片、圆角视频、定位条、控制按钮、状态与错误反馈。
2. 局部定制：MediaPlayerSlots 对默认区域追加、替换、隐藏或设置 chrome；未修改的区域仍由框架维护。
3. 完全重组：VideoPlayer 的 children 或 Builder.child 闭包替换整个默认 body，使用既有零件、Video 原语和任意业务组件。自定义 body 优先于 slots，不会执行被忽略的插槽工厂。

不创建 UI 插件注册中心。应用或扩展 crate 提供普通元素工厂，使用现有 owner 管理资源、上下文与订阅。播放器卸载不会隐式 Stop，也不会替扩展取消网络任务。

## 默认区域

| MediaPlayerSlot | data-slot | 默认内容 / 扩展用途 |
| --- | --- | --- |
| Header | media-header | 标题、作者；可加徽章、业务标题 |
| Viewport | media-surface | MediaViewport；可换画面比例、poster 或自定义渲染端点 |
| Overlay | media-overlays | 无；字幕、翻译、弹幕、水印、画面内操作 |
| Timeline | media-timeline | MediaSeekSlider；章节、雪碧图预览、标记 |
| ControlsStart | media-controls-start | 播放、前后跳转、时间 |
| ControlsEnd | media-controls-end | 音量、倍速、循环；可追加菜单触发器 |
| Settings | media-settings | 无；字幕、画质、翻译设置，或现有 Popover / Select / Switch |
| Sidebar | media-sidebar | 无；选集、字幕稿、业务面板，窄宽度换行 |
| Footer | media-footer | 状态与错误；可追加扩展错误或统计 |

默认画面位于 media-stage 中；Overlay 相对 stage 绝对定位，不参加视频的尺寸测量。隐藏或替换 Viewport 后，应用需为自己的画面元素提供有效尺寸。Sidebar 默认 16rem 且不超过容器宽度，可通过 slot chrome 改宽。所有区域使用现有 class / style 系统；内容可使用普通组件而非专用私有子件。

~~~rust
let layout = MediaPlayerSlots::new()
    .after(MediaPlayerSlot::ControlsEnd, || {
        // 在当前播放器上下文中创建菜单或业务按钮。
        media_rate_button(2.0)
    })
    .replace(MediaPlayerSlot::Header, || {
        let player = use_media_player();
        Element::new().text(format!("Player {}", player.id))
    })
    .style(MediaPlayerSlot::Header, |chrome| chrome.class("gap-4"))
    .hide(MediaPlayerSlot::ControlsStart);

let player: Element = video_player()
    .controller(controller.clone())
    .slots(layout.clone())
    .class("rounded-xl")
    .into();

ui! { <VideoPlayer controller={controller} slots={layout} /> }
~~~

before / after 各按调用顺序排列，二者分别位于默认内容或 replacement 两侧。replace 只替换该区域，最后一次 replace 生效；hide 是空 replacement，保留同区域的 before / after。被替换的默认内容不会构造，不创建其订阅。没有内容的区域不生成空白占位。

工厂在建立播放器 body 时、其 owner 和最近的 MediaPlayerCtx 中执行，而非配置时执行。配置可以 Clone；同一配置用于不同实例时，每次解析的是当前实例上下文，不能在配置时缓存 use_media_player() 的结果。嵌套播放器仍使用最近的作用域。运行时增删选集或控制项使用现有响应式组件生命周期重组 UI，不操作全局字符串注册表。

## 画面层与命中测试

~~~rust
let slots = MediaPlayerSlots::new().after(MediaPlayerSlot::Overlay, || {
    ui! {
        <MediaLayer class="p-4">
            <Label color="white">"由字幕扩展提供的当前文本"</Label>
        </MediaLayer>
    }
});
~~~

MediaLayer 默认 pointer-events=none，字幕和弹幕不会挡住画面操作。interactive=true 时使用 box-none：层自身不吃输入，其按钮等子元素可交互。Builder 等价写法为 media_layer().interactive(true).child(button)。Overlay 容器本身也为 box-none；多层按 children / after 添加顺序叠放，可用普通定位和样式进一步调整。

播放器不提供字幕解析、翻译 API、弹幕时间调度或弹幕网络连接。扩展按确认的 snapshot 和 load_session 推送展示内容，使用原有 Label / Canvas 等元素渲染；字幕文本无需逐帧设置 live region，避免高频打断读屏。

## 受控选择，不假报成功

MediaChoice 是通用单选请求按钮，可用于画质、字幕语言、翻译语言、选集等；共享的 MediaChoiceState 只记录应用确认的选中值、可用性和所属会话。每个控件仍是标准按钮，以 aria-pressed 表示确认状态，继承键盘、焦点和禁用行为。

~~~rust
let quality = Signal::new(MediaChoiceState {
    load_session: controller.current().load_session,
    selected: Some("720p".into()),
    enabled: true,
});

let request: Rc<dyn Fn(MediaChoiceRequest)> = Rc::new(move |request| {
    // 只入队；扩展接管实际请求及失败反馈，不在这里提前写 quality.selected。
    send_quality_request(request);
});

let slots = MediaPlayerSlots::new().replace(MediaPlayerSlot::Settings, move || {
    ui! {
        <MediaControls aria_label="Quality">
            <MediaChoice value="720p" state={quality} on_select={request.clone()}>"HD"</MediaChoice>
            <MediaChoice value="1080p" state={quality} on_select={request.clone()}>"Full HD"</MediaChoice>
        </MediaControls>
    }
});
~~~

Builder 等价入口为 media_choice("1080p", quality).on_select(handler).child(label)。状态会话不匹配、enabled=false、播放器 disabled、空白 value 或未提供回调时，按钮禁用。点击时再次读取最新状态，防止旧已渲染按钮绕过禁用或切源检查；不会修改确认值。选中态使用主题 accent，并暴露 data-value / data-state=on|off。

扩展回传选项成功时，在 UI 线程同时检查 request.load_session 与当前播放器、选择状态的 load_session，再更新 selected。失败保留确认值，扩展自己的错误 Signal 可在 Footer / Settings 展示。切源后旧选项自动不可操作、也不再显示为选中；扩展收到新源 metadata 后显式建立新会话的选项状态。

~~~rust
if response_session == controller.current().load_session
    && quality.peek().load_session == response_session
{
    quality.update(|state| state.selected = Some(confirmed_quality));
}
~~~

MediaChoiceRequest 携带 load_session 和 value，不包含媒体 URL、脚本或任意后端命令。选集映射成哪个源、画质切换是否保留位置、字幕轨道是否外置，均由扩展决定。选集可显式调用 controller.load()；真正切源必须建立新会话，不能把旧异步响应重新贴上当前 token。字幕 / 翻译等独立请求在同一 Load 内的竞态需由扩展自己的请求代数处理。

简单倍速继续使用 MediaRateButton 与已确认的 playback_rate；丰富倍速菜单可以用现有 Select 组合并请求 SetPlaybackRate。设置开关和范围输入复用 Switch / Slider，不为每种未来功能重复发明新状态机。

## 时间轴预览与雪碧图接口

~~~rust
let preview = Signal::new(None::<MediaTimelinePreview>);
let slots = MediaPlayerSlots::new().replace(MediaPlayerSlot::Timeline, move || {
    media_seek_slider_with_preview(preview)
});
// RSX: <MediaSeekSlider preview={preview} />
~~~

悬停时输出 load_session、[0,1] 的 fraction 和实际 position；非零起点 DVR 窗口使用相同映射。没有合法定位窗口、宽度为零或坐标非有限时不输出预览。离开、能力丢失、时间窗口变化和切源时清除旧预览，避免 duration 或 DVR 窗口更新后位置与比例不一致。预览不会发送 Seek，也不推进已确认 position；原有键盘和读屏定位行为不变。

雪碧图扩展可根据 position 查章节 / VTT 索引，下载自己的图集并渲染预览；返回数据必须核对 load_session 和最新预览位置，避免快速移动时旧图片覆盖新位置。请求节流、缓存、图集 UV 计算和取消属于扩展。默认未配置 preview 时不会创建预览 Signal、计时器或额外订阅。此接口目前是悬停预览，不保证触摸长按预览。

## 扩展落点与边界

| 功能 | UI 落点 | 应用 / 外部扩展负责 |
| --- | --- | --- |
| 雪碧图、章节 | Timeline + preview | 索引、图集、裁剪与缓存 |
| 个性弹幕 | Overlay + MediaLayer / Canvas | 调度、轨道碰撞、网络、过滤与资源回收 |
| 字幕、双语、翻译 | Overlay + Settings / Sidebar | 字幕解析、时间匹配、翻译服务和请求代数 |
| 画质、音轨 | Settings / ControlsEnd + MediaChoice | 流映射、后端切换、确认及失败恢复 |
| 倍速、循环 | 现有控制零件或自定义菜单 | 执行 MediaCommand 并回传确认 |
| 选集、播放列表 | Sidebar + MediaChoice / List | 源映射、显式 Load、下一集业务规则 |
| 宽屏、窗口全屏、比例、镜像、旋转 | Presentation 插槽和 MediaPresentationController | 应用根据 hide_surroundings 隐藏外部侧栏、解除宽度限制 |
| 系统全屏、画中画 | MediaFullscreenButton / MediaPictureInPictureButton | 宿主执行请求、确认实际窗口状态、关闭和资源生命周期 |
| 投屏 | 自定义按钮和画面区域 | 设备发现、连接和外部播放能力 |

MediaBackend、Audio / Video 原语和会话过滤契约不因 UI 扩展而耦合插件包。编码媒体、网络密钥和翻译凭证不应放进样式或任意 HTML 字符串；本设计只接受受类型约束的元素、值与闭包。

## 验证

- cargo test -p jadix --test media_customization --features a11y
- cargo test -p jadix --test media_player --test media_sessions --features a11y
- cargo test -p jadix --lib components::media_player::tests --features a11y
- cargo check -p gallery

Gallery 的绘图页包含自定义字幕占位层、时间轴预览时间、受控画质 / 字幕选项与选集侧栏。请求只记日志，另有明确命名的“模拟后端确认”按钮；不会隐式播放、加载字幕或下载雪碧图。
