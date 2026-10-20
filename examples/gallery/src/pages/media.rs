use crate::shared::{MyCard, MyCardProps};
use jadix::prelude::*;
use std::time::Duration;

fn demo_snapshot(video: bool) -> MediaSnapshot {
    MediaSnapshot {
        state: PlaybackState::Paused,
        position: Duration::from_secs(42),
        duration: Some(Duration::from_secs(245)),
        volume: 0.7,
        metadata: MediaMetadata {
            title: if video {
                "Design in motion"
            } else {
                "Quiet architecture"
            }
            .into(),
            artist: "Jadix Studio".into(),
            album: "Media UI / controlled preview".into(),
            ..Default::default()
        },
        capabilities: MediaCapabilities {
            audio: true,
            video,
            play: true,
            pause: true,
            stop: true,
            seek: true,
            volume: true,
            mute: true,
            playback_rate: true,
            looping: true,
        },
        video_texture: video.then(|| "gallery-media-preview".into()),
        video_size: video.then_some(MediaVideoSize {
            width: 160,
            height: 90,
        }),
        ..Default::default()
    }
}

#[component]
pub(crate) fn MediaPlayerShowcase() -> Element {
    let command_log = use_signal("gallery-media-command", || "尚未发送媒体命令".to_string());
    let audio_state = use_signal("gallery-media-audio", || demo_snapshot(false));
    let video_state = use_signal("gallery-media-video", || demo_snapshot(true));
    let presentation = use_signal(
        "gallery-media-presentation",
        MediaPresentationController::default,
    )
    .get();
    let display = presentation.state();
    let audio_controller = MediaController::new(audio_state, move |command| {
        command_log.set(format!("Audio → {command:?}"));
        Ok(())
    });
    let video_controller = MediaController::new(video_state, move |command| {
        command_log.set(format!("Video → {command:?}"));
        Ok(())
    });
    let mut pixels = Vec::with_capacity(160 * 90 * 4);
    for row in 0..90u8 {
        for column in 0..160u8 {
            pixels.extend_from_slice(&[32 + column, 30 + row, 100 + column / 2, 255]);
        }
    }
    jadix::media::queue_media_frame(
        "gallery-media-preview",
        jadix::media::MediaFrame::rgba8(160, 90, &pixels),
    );
    let report_play = video_controller.clone();
    let report_pause = video_controller.clone();
    let report_error = video_controller.clone();
    let preview = use_signal("gallery-media-storyboard", || None::<MediaTimelinePreview>);
    let quality = use_signal("gallery-media-quality", || MediaChoiceState {
        selected: Some("1080p".into()),
        ..MediaChoiceState::new(0)
    });
    let subtitles = use_signal("gallery-media-subtitles", || MediaChoiceState {
        selected: Some("off".into()),
        ..MediaChoiceState::new(0)
    });
    let episodes = use_signal("gallery-media-episodes", || MediaChoiceState {
        selected: Some("1".into()),
        ..MediaChoiceState::new(0)
    });
    let slots = MediaPlayerSlots::new()
        .after(MediaPlayerSlot::Overlay, || ui! {
            <MediaLayer class="p-4">
                <Label text_size="0.75rem" color="white">"字幕 / 弹幕图层 · 内容由扩展提供"</Label>
            </MediaLayer>
        })
        .replace(MediaPlayerSlot::Timeline, move || media_seek_slider_with_preview(preview))
        .after(MediaPlayerSlot::Timeline, move || ui! {
            <Label text_size="0.75rem" color="muted-foreground">{preview.get().map(|preview| {
                format!("预览位置 {} · 雪碧图扩展可按此时间取帧", format_media_time(preview.position))
            }).unwrap_or_else(|| "悬停进度条查看预览时间；此示例不加载雪碧图".into())}</Label>
        })
        .replace(MediaPlayerSlot::Settings, move || {
            let request: std::rc::Rc<dyn Fn(MediaChoiceRequest)> = std::rc::Rc::new(move |request| {
                command_log.set(format!("清晰度请求 → {} / session {}（等待外部确认）", request.value, request.load_session));
            });
            let caption_request: std::rc::Rc<dyn Fn(MediaChoiceRequest)> = std::rc::Rc::new(move |request| {
                command_log.set(format!("字幕请求 → {}（等待外部确认）", request.value));
            });
            ui! {
                <VStack gap="0.5rem">
                    <Label text_size="0.75rem" color="muted-foreground">"可定制设置 · 状态只在外部确认后改变"</Label>
                    <MediaControls aria_label="Quality">
                        <MediaChoice value="1080p" state={quality} on_select={request.clone()}>"1080p"</MediaChoice>
                        <MediaChoice value="720p" state={quality} on_select={request}>"720p"</MediaChoice>
                    </MediaControls>
                    <MediaControls aria_label="Subtitles">
                        <MediaChoice value="off" state={subtitles} on_select={caption_request.clone()}>"关闭字幕"</MediaChoice>
                        <MediaChoice value="zh" state={subtitles} on_select={caption_request}>"中文字幕"</MediaChoice>
                        <Button variant="ghost" size="sm" on_click={move || {
                            quality.update(|state| state.selected = Some("720p".into()));
                        }}>"模拟后端确认 720p"</Button>
                    </MediaControls>
                </VStack>
            }
        })
        .replace(MediaPlayerSlot::Sidebar, move || {
            ui! {
                <VStack gap="0.5rem">
                    <Label>"选集扩展"</Label>
                    <MediaChoice value="1" state={episodes} on_select={std::rc::Rc::new(move |request: MediaChoiceRequest| {
                        command_log.set(format!("选集请求 → {}（应用可显式 Load）", request.value));
                    }) as std::rc::Rc<dyn Fn(MediaChoiceRequest)>}>"01 · Introduction"</MediaChoice>
                    <MediaChoice value="2" state={episodes} on_select={std::rc::Rc::new(move |request: MediaChoiceRequest| {
                        command_log.set(format!("选集请求 → {}（应用可显式 Load）", request.value));
                    }) as std::rc::Rc<dyn Fn(MediaChoiceRequest)>}>"02 · Composition"</MediaChoice>
                    <Label text_size="0.75rem" color="muted-foreground">"Sidebar 可替换为选集、字幕稿或业务面板。"</Label>
                </VStack>
            }
        });
    ui! {
        <MyCard title="Audio / Video · 受控媒体组件">
            <VStack w="100%" min_w="0" max_w={if display.get().hide_surroundings() { "100%" } else { "48rem" }} gap="1rem">
                <Label text_size="0.75rem" color="muted-foreground">
                    "此处只演示 UI 与命令协议：按钮记录请求，下方按钮模拟外部后端回传。没有解码、声音输出或自动推进的播放时钟。"
                </Label>
                <VideoPlayer controller={video_controller} presentation={presentation} slots={slots} />
                <Label text_size="0.75rem" color="muted-foreground">
                    "宽屏隐藏选集侧栏并加宽播放区；窗口全屏可按 Esc 退出。支持镜像、旋转、容器比例、画面比例及适配方式；系统全屏与画中画尚未接入宿主，按钮保持禁用。"
                </Label>
                <HStack gap="0.5rem" wrap="wrap">
                    <Button variant="outline" size="sm" on_click={move || {
                        report_play.publish(MediaEvent::new(0, MediaEventKind::StateChanged { state: PlaybackState::Playing, play_when_ready: true }));
                    }}>"后端回传：播放中"</Button>
                    <Button variant="outline" size="sm" on_click={move || {
                        report_pause.publish(MediaEvent::new(0, MediaEventKind::StateChanged { state: PlaybackState::Paused, play_when_ready: false }));
                    }}>"后端回传：已暂停"</Button>
                    <Button variant="outline" size="sm" on_click={move || {
                        report_error.publish(MediaEvent::new(0, MediaEventKind::Error(MediaError::new(MediaErrorKind::Backend, "示例：媒体设备暂不可用"))));
                    }}>"后端回传：错误"</Button>
                </HStack>
                <AudioPlayer controller={audio_controller} class="gap-2">
                    <MediaTitle />
                    <MediaDescription />
                    <MediaSeekSlider />
                    <MediaControls>
                        <MediaPlayButton />
                        <MediaSeekButton seconds={-15.0} />
                        <MediaTimeDisplay />
                        <MediaMuteButton />
                        <MediaVolumeSlider />
                        <MediaRateButton rate={1.0} />
                    </MediaControls>
                    <MediaErrorMessage />
                </AudioPlayer>
                <Label text_size="0.75rem" color="muted-foreground">{command_log.get()}</Label>
            </VStack>
        </MyCard>
    }
}
