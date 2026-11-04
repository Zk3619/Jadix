use jadix::app::tasks::{BlockingTask, TaskError, TaskErrorKind, TaskHandle};
use jadix::media::{queue_media_frame, MediaFrame};
use jadix_splash::prelude::*;
use std::{sync::Arc, time::Duration};

fn texture_key(epoch: u64) -> String {
    format!("startup-media-{epoch}")
}

fn main() {
    let arguments: Vec<_> = std::env::args().collect();
    let has = |flag: &str| arguments.iter().any(|argument| argument == flag);
    let source = arguments
        .windows(2)
        .find(|pair| pair[0] == "--image")
        .map(|pair| pair[1].clone());
    let borderless = has("--borderless");
    let transparent = has("--transparent");
    let independent = has("--window");
    let blur = has("--blur");
    let video_surface = has("--video-surface");
    let exit_ready = has("--exit-ready");
    if has("--reduced-motion") {
        jadix::style::set_prefers_reduced_motion(Some(true));
    }
    let view = move |snapshot: StartupSnapshot, controller: StartupController| {
        let key = texture_key(snapshot.epoch);
        let backdrop: Element = if video_surface {
            let state = use_signal(&format!("startup-media-video-{}", snapshot.epoch), || {
                MediaSnapshot {
                    state: PlaybackState::Playing,
                    video_texture: Some(key),
                    video_size: Some(MediaVideoSize {
                        width: 128,
                        height: 128,
                    }),
                    ..Default::default()
                }
            });
            jadix::primitives::media::video()
                .controller(MediaController::new(state, |_| Ok(())))
                .fit("contain")
                .class("w-full h-full")
                .into()
        } else {
            image()
                .texture(key)
                .class("w-full h-full object-contain")
                .into()
        };
        splash()
            .title("Jadix")
            .description(if independent {
                "Custom media in a separate startup window"
            } else {
                "Custom media in the primary window"
            })
            .background(if transparent || blur {
                "transparent"
            } else {
                "background"
            })
            .backdrop(backdrop)
            .state(snapshot)
            .controller(controller)
            .child(
                button("startup-media-close")
                    .text("Close")
                    .on_click(jadix::app::ui_bridge::request_exit),
            )
            .into()
    };
    let startup = if independent {
        StartupConfig::window(
            WindowOptions::new()
                .title("Jadix Startup Splash")
                .size(480.0, 360.0)
                .resizable(false)
                .decorations(!(borderless || transparent || blur))
                .transparent(transparent || blur)
                .blur(blur),
            view,
        )
    } else {
        StartupConfig::component(view)
    };
    let executor = Arc::new(|work: BlockingTask| {
        std::thread::Builder::new()
            .name("startup-media-source".into())
            .spawn(work)
            .map_err(|error| TaskError::new(TaskErrorKind::Rejected, error.to_string()))?;
        Ok(TaskHandle::default())
    });
    let startup = startup.blocking(executor, move |controller| {
        let key = texture_key(controller.snapshot().epoch);
        let mut produced = 0;
        let result = (|| {
            if let Some(source) = source.as_deref() {
                controller.stage("Decoding local startup media");
                controller.cancellation().checkpoint()?;
                if jadix::style::prefers_reduced_motion() {
                    let image =
                        jadix::media::load_src(source, jadix::media::DecodeOptions::image())
                            .ok_or_else(|| {
                                TaskError::new(
                                    TaskErrorKind::Failed,
                                    "Unsupported or missing image; GIF requires image-gif",
                                )
                            })?;
                    jadix::queue_image_rgba8(&key, image.width, image.height, image.rgba);
                } else if !jadix::queue_image_from_path(&key, source) {
                    return Err(TaskError::new(
                        TaskErrorKind::Failed,
                        "Unsupported or missing image; GIF requires image-gif",
                    ));
                }
            }
            for frame in 0..120 {
                controller.cancellation().checkpoint()?;
                if source.is_none() && (frame == 0 || !jadix::style::prefers_reduced_motion()) {
                    let mut rgba = vec![0; 128 * 128 * 4];
                    for (index, pixel) in rgba.chunks_exact_mut(4).enumerate() {
                        let column = index % 128;
                        let row = index / 128;
                        let distance = (column as f32 - 64.0).hypot(row as f32 - 64.0);
                        pixel.copy_from_slice(&[
                            48 + ((column + frame) % 160) as u8,
                            70 + ((row + frame * 2) % 140) as u8,
                            220,
                            if distance < 60.0 { 210 } else { 0 },
                        ]);
                    }
                    queue_media_frame(&key, MediaFrame::rgba8(128, 128, &rgba));
                    produced += 1;
                }
                if frame % 20 == 0 {
                    controller.stage("Preparing application assets");
                    controller.progress(frame as u64, 120);
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            controller.progress(120, 120);
            Ok(())
        })();
        println!(
            "startup-media: source stopped epoch={} produced={produced}",
            controller.snapshot().epoch
        );
        result
    });
    let mut mounted = false;
    let mut application = jadix::app::app()
        .title("Jadix Startup Media")
        .size(680.0, 480.0)
        .startup(startup);
    if !independent && (transparent || blur) {
        application = application.transparent(true).frameless().blur(blur);
    }
    application.mount(move || {
        if !mounted {
            mounted = true;
            println!("startup-media: business root mounted");
            if exit_ready {
                std::thread::spawn(|| {
                    std::thread::sleep(Duration::from_millis(800));
                    jadix::app::ui_bridge::request_exit();
                });
            }
        }
        Element::new()
            .w("100%")
            .h("100%")
            .bg("background")
            .flex_col()
            .items_center()
            .justify_center()
            .gap("1rem")
            .child(text("Application ready").size("2rem"))
            .child(text(if independent {
                "The startup window is released after this content is presented."
            } else {
                "The media producer has stopped; this is the same window."
            }))
            .child(
                button("media-ready-close")
                    .text("Close")
                    .on_click(jadix::app::ui_bridge::request_exit),
            )
    });
}
