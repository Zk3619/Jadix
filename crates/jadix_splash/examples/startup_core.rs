use jadix::{app::tasks, prelude::*};
use jadix_splash::{StartupConfig, StartupController, StartupPhase, StartupSnapshot};
use std::{sync::Arc, time::Duration};

fn main() {
    let custom = std::env::args().any(|argument| argument == "--component");
    let independent = std::env::args().any(|argument| argument == "--window");
    let borderless = std::env::args().any(|argument| argument == "--borderless");
    let view = |snapshot: StartupSnapshot, controller: StartupController| {
        let mut root = Element::new()
            .w("100%")
            .h("100%")
            .flex_col()
            .items_center()
            .justify_center()
            .gap("1rem")
            .child(text(snapshot.stage));
        if matches!(
            snapshot.phase,
            StartupPhase::Failed(_) | StartupPhase::Cancelled
        ) {
            root = root.child(
                Element::new()
                    .text("Retry")
                    .focusable(true)
                    .focus_id("custom-startup-retry")
                    .accesskit_role("button")
                    .on_click(move || controller.retry()),
            );
        }
        root
    };
    let startup = if independent {
        StartupConfig::window(
            WindowOptions::new()
                .title("Framework startup splash")
                .size(480.0, 320.0),
            view,
        )
        .borderless(borderless)
    } else if custom {
        StartupConfig::component(view)
    } else {
        StartupConfig::system()
    };
    let executor = Arc::new(|work: tasks::BlockingTask| {
        std::thread::Builder::new()
            .name("prepare-assets".into())
            .spawn(work)
            .map_err(|error| {
                tasks::TaskError::new(tasks::TaskErrorKind::Rejected, error.to_string())
            })?;
        Ok(tasks::TaskHandle::default())
    });
    let startup = startup.blocking(executor, |controller| {
        for completed in 0..4 {
            controller.cancellation().checkpoint()?;
            controller.stage(format!("Preparing local assets {completed}/4"));
            controller.progress(completed, 4);
            std::thread::sleep(Duration::from_millis(800));
        }
        Ok(())
    });
    app()
        .title("Framework startup")
        .size(640.0, 480.0)
        .startup(startup)
        .mount(|| text("Application ready").into());
}
