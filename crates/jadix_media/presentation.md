# 播放器展示与窗口接入

展示状态位于 jadix::components::media_player，由 MediaPresentationController 管理；MediaSnapshot 和 jadix_media 播放协议不承担窗口状态。Controller 在 UI owner 下创建，并在重建播放器之外保持存活；克隆共享状态和请求通道。每个播放器使用独立的展示 Controller。

## 内置展示能力

| 能力 | 行为 |
| --- | --- |
| 宽屏 | MediaLayoutMode::Wide 隐藏默认布局的 Sidebar；应用订阅 state().get().hide_surroundings()，隐藏外部兄弟侧栏并解除 max-width |
| 窗口全屏 | MediaLayoutMode::WindowFullscreen 把原有播放器内容移动到当前客户区 Portal；保留控件、绑定和媒体端点；退出按钮或 Esc 返回原位置、之前的普通 / 宽屏布局及焦点 |
| 系统全屏 | 宿主确认 fullscreen=true 后，播放器内容占满窗口；宿主负责将窗口置于真实系统全屏 |
| 画中画 | 宿主执行独立视频窗口 / 平台 PiP，确认 picture_in_picture=true；框架不创建假 PiP 小窗 |
| 镜像 | 水平 / 垂直独立开关，仅改变视频纹理 UV；字幕、弹幕与控制栏保持方向 |
| 旋转 | 0 / 90 / 180 / 270 度，先按旋转后的可用区域计算 contain / cover 几何；Native 视口比例同时交换宽高 |
| 容器比例 | viewport_ratio：Native / 16:9 / 4:3 / 1:1 / 9:16；仅改变视口形状；没有源尺寸时 Native 回退 16:9；全屏时视口填充剩余客户区 |
| 强制画面比例 | content_ratio：同一组比例选项；独立覆盖采样画面的显示比例，可能拉伸内容；Native 保持源比例 |
| 适配方式 | fit：Contain 留边 / Cover 裁剪 / Fill 拉伸；旋转及镜像均经过同一纹理裁剪与变换链 |

窗口全屏不改变操作系统窗口大小。系统全屏和画中画互斥；切换时先请求退出当前宿主模式，收到确认后再请求进入另一模式。展示控制不发出播放 / 暂停 / Seek，也不推进媒体时钟。宿主能力默认关闭，未连接平台时对应按钮禁用。能力撤销后仍允许退出已经确认的宿主模式。

## RSX 与 Builder

~~~rust
use jadix::prelude::*;

let presentation = MediaPresentationController::default();
let player = ui! {
    <VideoPlayer controller={media.clone()} presentation={presentation.clone()} />
};
let player: Element = video_player()
    .controller(media)
    .presentation(presentation.clone())
    .slots(MediaPlayerSlots::new().hide(MediaPlayerSlot::Presentation))
    .into();

presentation.set_layout(MediaLayoutMode::Wide);
presentation.set_viewport_ratio(MediaAspectRatio::Standard);
presentation.set_content_ratio(MediaAspectRatio::Square);
presentation.set_mirror(true, false);
presentation.set_rotation(MediaRotation::Clockwise90);
presentation.set_fit(MediaFit::Contain);
~~~

MediaPlayer / VideoPlayer / AudioPlayer 均接受 presentation prop；展示按钮在 AudioPlayer 下禁用。自定义组合使用 media_player_presentation_scope(id, media, presentation, kind, disabled, body)，或在 RSX 播放器内组合零件。原有 media_player_scope 继续可用，会创建本地展示 Controller。

默认视频布局新增 MediaPlayerSlot::Presentation，包含宽屏、窗口全屏、系统全屏、画中画、两种镜像、旋转、两种比例、适配和重置按钮。可用 replace / before / after / hide / style 定制。自定义 children 替换默认布局后，应用负责布局填充，窗口全屏仍提供独立退出按钮。

~~~rust
ui! {
    <VideoPlayer controller={media} presentation={presentation}>
        <MediaViewport><MediaLayer><Label>"字幕"</Label></MediaLayer></MediaViewport>
        <MediaControls>
            <MediaPlayButton />
            <MediaWideButton />
            <MediaWindowFullscreenButton />
            <MediaFullscreenButton />
            <MediaPictureInPictureButton />
            <MediaMirrorButton vertical=true />
            <MediaRotateButton />
            <MediaAspectRatioButton />
            <MediaAspectRatioButton content=true />
            <MediaFitButton />
            <MediaResetPictureButton />
        </MediaControls>
    </VideoPlayer>
}
~~~

比例按钮循环当前组的所有比例；适配按钮循环 Contain / Cover / Fill。需要固定选项菜单时，用现有 Select / Menu 调用相应 setter。各按钮使用 ghost / icon-sm，并提供名称、按下状态、禁用状态、待确认状态及键盘按钮行为。MediaErrorMessage 同时呈现宿主拒绝或确认失败。

不使用播放器组件时，Video / video() 原语也接受 content_ratio、rotation、mirror_horizontal、mirror_vertical 和 fit。通用 aspect_ratio 设置原语容器比例。

## 宿主适配器协议

~~~rust
let requests = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
let queued = requests.clone();
let presentation = MediaPresentationController::new(
    MediaPresentationCapabilities { fullscreen: true, picture_in_picture: false },
    move |request| {
        queued.borrow_mut().push(request);
        Ok(())
    },
);

let id = presentation.request(MediaHostAction::Fullscreen(true))?;
// 宿主消费队列，在正确窗口执行 set_fullscreen，观察实际窗口状态后在 UI 线程确认：
presentation.publish(id, Ok(MediaHostState {
    fullscreen: true,
    picture_in_picture: false,
}));
~~~

上例队列只展示协议接线；实际宿主必须执行窗口操作后再确认。请求成功只表示已入队，按钮保持未按下，pending() 暴露尚未确认的请求。每个 Controller 只允许一个在途宿主请求；publish 只接受匹配的请求 ID，重复或迟到回复不修改状态。不符合请求的状态或互斥状态会记录错误。拒绝用 publish(id, Err(error)) 回传，保持之前已确认状态。宿主必须最终确认或报告错误，不能一直丢弃请求。

用户按系统 Esc、关闭 PiP 窗口或平台主动退出时调用 observe_host(actual_state)，清除在途请求并覆盖已确认宿主状态；旧请求回复随后被忽略。窗口全屏内按 Esc 直接退出本地布局；系统全屏内按 Esc 请求宿主退出，保留已确认布局直到实际退出。宿主通过 set_capabilities 更新运行时可用性。

PiP 扩展负责视频纹理 / 设备资源的共享、拥有者生命周期、窗口关闭、回到原位置及错误处理。平台只支持普通置顶窗口时，应明确暴露该能力，不能当作原生 PiP。播放器 owner 销毁后拒绝回调；Controller 的 Drop 不执行窗口或媒体操作，宿主应自行结束并回收窗口资源。

