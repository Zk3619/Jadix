# Audio / Video 与媒体播放器组件

## 分层与依赖

| 层 | 位置 | 职责 |
| --- | --- | --- |
| 独立契约 | jadix_media | 仅用标准库的 source、load options、snapshot、capabilities、command、event、error、backend trait |
| UI 绑定 | jadix::media::MediaController | 连接受控 Signal 与外部命令入口，验证请求，接收权威状态 |
| 原语 | jadix::primitives::{Audio, Video} | Audio 是零尺寸无控件端点；Video 显示外部纹理或 poster，复用现有合成链 |
| 组件 | jadix::components::media_player | shadcn 风格默认壳、无障碍名称、可组合控制零件、错误反馈 |
| 外部扩展 | 应用或独立 crate | 资源获取、解复用、解码、输出设备、播放时钟、音画同步、加载取消、线程与资源回收 |

jadix_media 不依赖 Jadix、wgpu 或媒体库，未来音频适配器可以只依赖该 crate。UI 随现有组件库提供，不引入 rodio / FFmpeg / GStreamer / 系统媒体实现，也没有动态后端注册表。

HTML 风格名称在这里表示端点。Audio / audio 无默认按钮、间距、背景或播放副作用。Video / video 无默认尺寸或控件，接受普通布局、圆角、clip 和 class。播放器默认卡片、16:9 视口与控制栏属于组件层。

## 接入外部库

~~~rust
use jadix::prelude::*;
use std::sync::mpsc;

let (commands, receiver) = mpsc::channel::<MediaCommand>();
let snapshot = Signal::new(MediaSnapshot::default());
let controller = MediaController::new(snapshot, move |command| {
    commands.send(command).map_err(|_| {
        MediaError::new(MediaErrorKind::Disconnected, "media worker stopped")
    })
});
let player: Element = video_player().controller(controller.clone()).into();

let load_session = controller.load(
    MediaSource::File("assets/demo.mp4".into()),
    MediaLoadOptions::default(),
)?;
~~~

应用把 receiver 交给自己的适配器线程。框架不会消费 channel 或实例化后端。渲染、重复渲染、挂载、卸载都不会隐式执行 Load / Play / Stop。更换源以及 autoplay / preload / loop 等加载选项由应用显式请求，适配器负责实施。

MediaBackend 提供非阻塞 dispatch(command) 和 poll_event()。扩展可实现该 trait，也可直接提供已有库的命令闭包。UI 线程命令闭包必须迅速返回，不能打开网络、等待解码或阻塞设备操作。

request() 的成功表示请求被接受，播放、定位、音量等控制等待后端确认。Load 有一项必要的同步动作：在派发前建立会话边界，清除上一源的 position、duration、buffered、seekable、metadata、capabilities、error 和视频引用，保留已确认的音量、静音、倍速和循环设置。此时 state 为 Idle，后端随后确认 Loading / Ready / Playing 等实际状态；加载选项仍由后端实施。

请求失败保存在独立 request_error() 并返回 Result；播放失败通过 MediaEventKind::Error 汇报。MediaErrorMessage 展示这两类错误。Load 派发失败且 snapshot 仍等于刚建立的边界时，Controller 恢复先前 snapshot；已改变状态的后端确认和更新的会话不会被回滚。成功提交新请求清除上次请求错误；恢复到非 Error 状态清除后端错误。

在 UI 线程通过 controller.publish(event) 发布状态，也可直接更新受控 snapshot。工作线程只传递可发送的 event，在 UI 线程应用：

~~~rust
let state = controller.snapshot();
let event = MediaEvent::new(
    load_session,
    MediaEventKind::PositionChanged(std::time::Duration::from_secs(42)),
);
jadix::app::run_on_ui_thread(move || {
    if let Some(mut snapshot) = state.try_peek() {
        if snapshot.apply(event) {
            state.try_set(snapshot);
        }
    }
});
~~~

Controller 是 UI 线程对象。上例投递 event 和 Signal 标识，在 UI 线程访问 Signal。应保持 Controller 的 owner 在整个播放器生命周期内存活，例如在应用根创建。界面卸载不代表后端停止，需要停止时由应用显式 dispatch / drop 适配器。Owner 已销毁时，Controller 拒绝请求，publish 返回 false。

### 会话隔离

每个 MediaCommand::Load 必须带 load_session，每个 MediaEvent 通过同名字段携带原始 Load token。推荐使用 controller.load()：共享 Controller 的所有克隆使用同一个单调计数器，返回本次 token。手动构造 Load 时也必须使用大于当前会话和已发出 token 的值。拒绝的请求仍消耗其 token，防止稍后到达的失败请求事件误归属新源；u64 耗尽时返回 InvalidInput，不会回绕。

MediaSnapshot::apply() 只接受与当前 load_session 完全相等的事件；过期和未来事件都返回 false 且保持 snapshot 不变。完整 Snapshot 事件内部的 load_session 也必须相等。返回 true 表示事件被接受，未必改变值；controller.publish() 进一步去重，未变化时不写 Signal、不申请重绘。默认 session=0 支持预先初始化的受控 snapshot。直接管理 Signal 的应用应在派发新 Load 前调用 begin_load(token, source)，或自行建立等价的受控状态边界。

后端必须在创建每次异步任务时捕获 Load 的 token，并让该任务的 metadata、duration、error、position 和视频事件始终使用该 token。任务完成时读取“当前 token”重新贴标会绕过隔离。取消旧任务、网络请求和解码仍由适配器负责，事件过滤不能替代取消。全量 Snapshot 回传也需要使用本次 token。

### 内存媒体源

MediaSource::Bytes(Arc<[u8]>) 支持打包资源或解密后的编码媒体；也可用 MediaSource::from(Vec<u8>)。克隆源共享底层字节，Debug 只显示长度。空内存源在命令入口被拒绝；格式识别、读取和解码交给后端。Bytes 表示完整的编码数据，渐进读取或自定义提供器留给有实际需求的外部扩展。

~~~rust
let source = MediaSource::from(encoded_bytes); // encoded_bytes: Vec<u8>
let load_session = controller.load(source, MediaLoadOptions::default())?;
~~~

## 能力和时间语义

Capabilities 全部默认 false。播放、暂停、停止、定位、音量、静音、倍速、循环分别声明，UI 按当前能力启用操作。音频后端设置 audio=true / video=false；CapabilitiesChanged 可在 metadata 到达后更新能力。已挂载的拖动条随 seek 能力和有效时间窗口动态启用 / 禁用，默认保留布局；需要隐藏的产品界面可按受控 snapshot 条件组合零件。

时间使用 Duration。duration=None 表示未知或实时媒体，显示 --:--。普通媒体由 [0, duration] 提供定位范围；实时 DVR 用显式 MediaTimeRange 提供非零起点窗口。没有范围或范围为空时禁用拖动与前后跳转。相对跳转钳制到范围；直接 Seek 越界返回 InvalidInput。

状态事件不会伪造播放时钟。Buffering 与播放意图分离：play_when_ready 区分缓冲后继续播放和暂停时缓冲。volume 是有限的 [0, 1]，倍速必须有限且为正；请求入口拒绝非法数值，snapshot 展示前规范化异常数字。

PositionChanged 的常规进度通知应在后端合并后再进入 UI 队列，建议间隔 100–250ms；RECOMMENDED_POSITION_EVENT_INTERVAL 为 200ms。不能把每帧解码回调直接作为进度事件发送。定位、暂停、结束等不连续变化应立即确认，不必等待节流周期；视频帧上传保持独立频率。更平滑的进度展示可由应用结合单调时间戳、最近一次 position 和 playback_rate 插值，并在暂停、缓冲、定位或切源时校正；UI 框架不提供播放时钟或持续计时任务。

拖动条复用 Slider：方向键调整、Home / End 首尾定位、Tab 聚焦，共享 focus ring。拖动可展示本地预览；后端更新重新对齐滑块，请求拒绝时回到已确认值。外部 position / volume 更新不反向生成 Seek / SetVolume。按钮沿用 Button 的 Space / Enter。

## 双书写面

VideoPlayer 支持保留默认 shadcn 壳的区域追加 / 替换 / 隐藏、独立插槽 chrome、画面叠层、受控扩展选择和时间轴预览。完整设计、示例及未来字幕 / 弹幕 / 雪碧图 / 画质 / 选集的接入位置见 [可定制 VideoPlayer](customization.md)。

~~~rust
ui! { <AudioPlayer controller={audio.clone()} /> }
ui! { <VideoPlayer controller={video.clone()} poster="assets/poster.png" /> }

audio_player().controller(audio.clone()).class("w-full")
video_player().controller(video.clone()).poster("assets/poster.png")
~~~

自由组合：

~~~rust
ui! {
    <MediaPlayer controller={audio.clone()} class="gap-2 p-3">
        <MediaTitle />
        <MediaDescription />
        <MediaSeekSlider />
        <MediaControls>
            <MediaPlayButton />
            <MediaSeekButton seconds={-15.0} />
            <MediaTimeDisplay />
            <MediaMuteButton />
            <MediaVolumeSlider class="w-20" />
            <MediaRateButton rate={1.0}>"1×"</MediaRateButton>
            <MediaRateButton rate={1.5}>"1.5×"</MediaRateButton>
            <MediaLoopButton />
        </MediaControls>
        <MediaStatus />
        <MediaErrorMessage />
    </MediaPlayer>
}

media_player().controller(audio).child(|| {
    vstack()
        .child(media_title())
        .child(media_seek_slider())
        .child(media_controls().child(media_play_button()).child(media_time_display()))
        .child(media_error_message())
        .into()
})
~~~

Builder 的 child 闭包与 RSX 根子节点在公开 media_player_scope 中执行，零件通过 use_media_player() 取得实例上下文。兄弟和嵌套播放器各自隔离。单独调用零件时必须提供 scope；缺失时明确报错。

默认壳和自定义布局复用同一组零件。每个零件都有 data-slot，RSX 提供 class / base；根 Builder 提供 class / style，零件 Builder 或 Element 提供 class 与普通布局。全局样式用 data-slot 选择器，无需私有 parts 配置。

| 零件 | 功能 / data-slot |
| --- | --- |
| MediaViewport | 视频端点和 poster / media-viewport |
| MediaControls | 可换布局的控制组 / media-controls |
| MediaTitle / MediaDescription | metadata 标题、作者、专辑或自定义 children / media-title、media-description |
| MediaPlayButton | 根据确认状态发送 Play / Pause / media-play |
| MediaStopButton | Stop / media-stop |
| MediaSeekButton | 有符号 seconds，默认 +10 / media-seek |
| MediaSeekSlider | VOD / DVR 定位 / media-seek-slider |
| MediaTimeDisplay | 已播放 / 总长 / media-time |
| MediaMuteButton / MediaVolumeSlider | 静音与线性音量 / media-mute、media-volume |
| MediaRateButton | 设为指定倍速，默认 1.5，可组合多个选项 / media-rate |
| MediaLoopButton | 循环开关 / media-loop |
| MediaStatus | 加载、缓冲、结束，polite live region / media-status |
| MediaErrorMessage | 后端与命令错误，assertive live region / media-error |

RateButton 表示设为该倍速，默认显示目标倍速文字；默认播放器提供 1× / 1.5×，可恢复普通速度。Loop / Mute / Rate 以 aria-pressed 暴露后端确认状态。图标按钮有可本地化名称；状态用 live region，时间更新不会逐次作为通知朗读。控制条带 slider 角色、值和时间描述。

## 视频帧

适配器按自己的时钟选择帧，通过既有 queue_media_frame(key, frame) 上传，并在确认非 Idle 状态后发布带会话 token 的 VideoTextureChanged(Some(key)) 和 VideoSizeChanged(Some(MediaVideoSize { width, height }))。同一会话可复用 texture key；不同 Load 应使用不同 key，或在上传前拦截旧会话的帧，避免过期上传覆盖当前 key 的像素。事件过滤只保护 snapshot，无法撤销已经入队的同 key 上传。RGBA 默认可用；YUV 沿用 image-stream feature 的 BT.709 转换限制。PTS 仅是数据，框架不据此调度或进行音画同步。

video_texture 是 Jadix 图片缓存的查询 key，上传后的 GPU 纹理由该缓存持有，字符串不转移 GPU 所有权。渲染器遇到缺失 key 会跳过绘制。后端释放自己的解码资源时，不能据此提前销毁仍由 UI 缓存持有的 GPU 资源；需要共享 GPU 句柄的零拷贝适配器应另行约定引用计数所有权，不能用 Arc<String> 代替资源所有权。

VideoTextureChanged(None) 在 UI 应用事件时清除活动视频引用，视口回退至 poster 或空内容。StateChanged(Idle) 同时清除纹理 key、视频尺寸和播放意图；Idle 下迟到的帧也不会重新绑定。Ended 可保留最后一帧。Stop 仍是请求，后端应在实际停止时确认 None / Idle；播放器仍挂载时，适配器销毁前也应在 UI 线程完成解绑。缓存自身的清理负责最终 GPU 回收，Controller 的 Drop 不隐式派发线程操作或 Stop。

~~~rust
ui! { <video controller={controller.clone()} fit="contain" w="100%" aspect_ratio="16/9" /> }
let endpoint: Element = video().controller(controller).fit("cover")
    .class("w-full aspect-video rounded-md overflow-hidden").into();
~~~

Contain 默认按视频尺寸居中留边，cover 保持比例裁剪 UV，fill 拉伸；几何继续经过现有纹理、transform、圆角与 clip 链。没有尺寸时保持 Image 的矩形采样行为。Poster 走图片入口，媒体 URL 绝不交给图片解码器。原语不查询后端、读取媒体文件或从 GPU 反推尺寸。

字幕、波形、封面、播放列表和音轨菜单可用现有 Label / Canvas / Image / Select / List 组合。字幕解析、音轨切换由外部扩展执行。播放器展示模式、窗口全屏及宿主全屏 / 画中画请求见 [展示与窗口接入](presentation.md)。

