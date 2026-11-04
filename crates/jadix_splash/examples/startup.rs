use jadix_splash::prelude::*;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::Duration;

fn main() {
    let arguments: Vec<_> = std::env::args().collect();
    let system = arguments.iter().any(|argument| argument == "--system");
    let independent = arguments.iter().any(|argument| argument == "--window");
    let fail_once = arguments.iter().any(|argument| argument == "--fail-once");
    let exit_ready = arguments.iter().any(|argument| argument == "--exit-ready");
    let disabled = arguments.iter().any(|argument| argument == "--no-splash");
    let slow = arguments.iter().any(|argument| argument == "--slow");
    let prepare_ms = arguments
        .windows(2)
        .find(|pair| pair[0] == "--prepare-ms")
        .map(|pair| {
            pair[1]
                .parse::<u64>()
                .expect("--prepare-ms requires milliseconds")
        })
        .unwrap_or(if slow { 6000 } else { 1600 });
    let instance_id = arguments
        .windows(2)
        .find(|pair| pair[0] == "--single-instance")
        .map(|pair| pair[1].clone());
    let attempts = Arc::new(AtomicUsize::new(0));
    let borderless = arguments.iter().any(|argument| argument == "--borderless");
    let view = |snapshot: StartupSnapshot, controller: StartupController| {
        ui! { <Splash title="Jadix" description="Starting the application" state={snapshot} controller={controller}/> }
    };
    let startup = if independent {
        StartupConfig::window(
            WindowOptions::new()
                .title("Jadix Startup Splash")
                .size(480.0, 320.0)
                .resizable(false),
            view,
        )
        .borderless(borderless)
    } else if system {
        StartupConfig::system()
    } else {
        StartupConfig::component(view)
    };
    let executor = Arc::new(|work: jadix::app::tasks::BlockingTask| {
        std::thread::Builder::new()
            .name("example-startup".into())
            .spawn(work)
            .map_err(|error| {
                jadix::app::tasks::TaskError::new(
                    jadix::app::tasks::TaskErrorKind::Rejected,
                    error.to_string(),
                )
            })?;
        Ok(jadix::app::tasks::TaskHandle::default())
    });
    let startup = startup
        .enabled(!disabled)
        .blocking(executor, move |controller| {
            let attempt = attempts.fetch_add(1, Ordering::Relaxed);
            for completed in 0..4 {
                controller.cancellation().checkpoint()?;
                controller.stage(format!("Preparing assets {completed}/4"));
                controller.progress(completed, 4);
                std::thread::sleep(Duration::from_millis(prepare_ms / 4));
            }
            if fail_once && attempt == 0 {
                return Err(jadix::app::tasks::TaskError::new(
                    jadix::app::tasks::TaskErrorKind::Failed,
                    "Example failure: choose Retry",
                ));
            }
            controller.progress(4, 4);
            Ok(())
        });
    let mut mounted = false;
    let mut application = jadix::app::app()
        .title("Jadix Startup Example")
        .size(640.0, 480.0)
        .startup(startup);
    if let Some(id) = instance_id {
        application = application
            .single_instance(id)
            .on_second_instance(|launch| {
                println!("startup-example: second instance args={:?}", launch.args);
            });
    }
    application.mount(move || {
        if !mounted {
            mounted = true;
            println!("startup-example: business root mounted");
            if exit_ready {
                std::thread::spawn(|| {
                    std::thread::sleep(Duration::from_millis(800));
                    jadix::app::ui_bridge::request_exit();
                });
            }
        }
        let heading: Element = jadix::primitives::text::text("Application ready").size("2rem").into();
        ui! { <VStack gap="1rem">{heading}<Text>"The business root is mounted after preparation."</Text></VStack> }
    });
}
