//! Startup policy and preparation state machine.

use std::sync::{
    atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering},
    Arc, Mutex,
};

use jadix::core::tree::Element;
use jadix_tasks::{BlockingExecutor, CancellationToken, TaskError};

static NEXT_EPOCH: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SplashMode {
    System,
    Component,
    Window,
}

#[derive(Clone, Debug, PartialEq)]
pub enum StartupPhase {
    Preparing,
    Ready,
    Failed(String),
    Cancelled,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StartupSnapshot {
    pub epoch: u64,
    pub phase: StartupPhase,
    pub stage: String,
    pub progress: Option<(u64, u64)>,
}

#[derive(Clone)]
pub struct StartupController {
    shared: Arc<Mutex<StartupSnapshot>>,
    cancellation: CancellationToken,
    wake_pending: Arc<AtomicBool>,
}

impl StartupController {
    fn new() -> Self {
        Self {
            shared: Arc::new(Mutex::new(StartupSnapshot {
                epoch: NEXT_EPOCH.fetch_add(1, Ordering::Relaxed),
                phase: StartupPhase::Preparing,
                stage: "Starting".into(),
                progress: None,
            })),
            cancellation: CancellationToken::new(),
            wake_pending: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn snapshot(&self) -> StartupSnapshot {
        self.shared
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }

    pub fn cancellation(&self) -> CancellationToken {
        self.cancellation.clone()
    }

    fn update(&self, update: impl FnOnce(&mut StartupSnapshot)) -> bool {
        self.update_phase(false, update)
    }

    fn update_phase(&self, allow_ready: bool, update: impl FnOnce(&mut StartupSnapshot)) -> bool {
        {
            let mut snapshot = self
                .shared
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            if snapshot.phase != StartupPhase::Preparing
                && !(allow_ready && snapshot.phase == StartupPhase::Ready)
            {
                return false;
            }
            update(&mut snapshot);
            if snapshot.phase != StartupPhase::Preparing {
                log::info!(
                    "startup terminal epoch={} phase={:?}",
                    snapshot.epoch,
                    snapshot.phase
                );
            }
        }
        if !self.wake_pending.swap(true, Ordering::AcqRel) {
            if !jadix::app::startup::notify_changed() {
                self.wake_pending.store(false, Ordering::Release);
            }
        }
        true
    }

    pub fn stage(&self, stage: impl Into<String>) {
        self.update(|snapshot| snapshot.stage = stage.into());
    }

    pub fn progress(&self, completed: u64, total: u64) {
        self.update(|snapshot| {
            snapshot.progress = (total > 0).then_some((completed.min(total), total));
        });
    }

    pub fn ready(&self) {
        self.update(|snapshot| snapshot.phase = StartupPhase::Ready);
    }

    pub fn fail(&self, message: impl Into<String>) {
        if self.update(|snapshot| snapshot.phase = StartupPhase::Failed(message.into())) {
            self.cancellation.cancel();
        }
    }

    pub fn cancel(&self) {
        if self.update(|snapshot| snapshot.phase = StartupPhase::Cancelled) {
            self.cancellation.cancel();
        }
    }

    pub fn retry(&self) {
        jadix::app::startup::request_retry(self.snapshot().epoch);
    }
}

type StartupView = Box<dyn FnMut(StartupSnapshot, StartupController) -> Element>;
type Preparation = Box<dyn FnMut(StartupController)>;

pub struct StartupConfig {
    pub(crate) mode: SplashMode,
    enabled: bool,
    view: Option<StartupView>,
    preparation: Option<Preparation>,
    window: Option<jadix::app::WindowOptions>,
}

impl StartupConfig {
    pub fn system() -> Self {
        Self {
            mode: SplashMode::System,
            enabled: true,
            view: None,
            preparation: None,
            window: None,
        }
    }

    pub fn component(
        view: impl FnMut(StartupSnapshot, StartupController) -> Element + 'static,
    ) -> Self {
        Self {
            mode: SplashMode::Component,
            enabled: true,
            view: Some(Box::new(view)),
            preparation: None,
            window: None,
        }
    }

    /// Show the startup component in a separate desktop window while the primary is hidden.
    /// Unsupported hosts fall back to the same-host component. The window closes only
    /// after the primary content is presented; closing it earlier exits the application.
    pub fn window(
        options: jadix::app::WindowOptions,
        view: impl FnMut(StartupSnapshot, StartupController) -> Element + 'static,
    ) -> Self {
        let mut config = Self::component(view);
        config.mode = SplashMode::Window;
        config.window = Some(options);
        config
    }

    /// Override decorations on the independent splash window only.
    /// Shared-host System and Component modes retain the primary window configuration.
    pub fn borderless(mut self, borderless: bool) -> Self {
        if let Some(options) = self.window.as_mut() {
            options.decorations = !borderless;
        }
        self
    }

    /// Disable additional branding while retaining preparation, failure and first-frame gates.
    /// Mandatory mobile launch resources remain controlled by the operating system.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// The callback must return promptly; dispatch IO or CPU work through an executor.
    pub fn prepare(mut self, preparation: impl FnMut(StartupController) + 'static) -> Self {
        self.preparation = Some(Box::new(preparation));
        self
    }

    pub fn blocking(
        self,
        executor: Arc<dyn BlockingExecutor>,
        work: impl Fn(StartupController) -> Result<(), TaskError> + Send + Sync + 'static,
    ) -> Self {
        let work = Arc::new(work);
        self.prepare(move |controller| {
            let work = work.clone();
            let disposition = Arc::new(AtomicU8::new(0));
            let completion = StartupCompletion {
                controller: controller.clone(),
                disposition: disposition.clone(),
            };
            let job = controller.clone();
            match executor.spawn_blocking(Box::new(move || {
                let result = job.cancellation().checkpoint().and_then(|()| {
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| work(job)))
                        .unwrap_or_else(|_| {
                            Err(TaskError::new(
                                jadix_tasks::TaskErrorKind::Panicked,
                                "Startup task panicked",
                            ))
                        })
                });
                match result {
                    Ok(()) if completion.controller.cancellation().is_cancelled() => {
                        completion.controller.cancel()
                    }
                    Ok(()) => completion.controller.ready(),
                    Err(error) if error.kind == jadix_tasks::TaskErrorKind::Cancelled => {
                        completion.controller.cancel()
                    }
                    Err(error) => completion.controller.fail(error.to_string()),
                }
            })) {
                Ok(handle) => {
                    if disposition.fetch_or(StartupCompletion::ACCEPTED, Ordering::AcqRel)
                        & StartupCompletion::DROPPED
                        != 0
                    {
                        controller.fail("Startup executor abandoned the accepted task");
                    }
                    controller.cancellation().link_handle(handle);
                }
                Err(error) => controller.fail(error.to_string()),
            }
        })
    }
}

struct StartupCompletion {
    controller: StartupController,
    disposition: Arc<AtomicU8>,
}

impl StartupCompletion {
    const ACCEPTED: u8 = 1;
    const DROPPED: u8 = 2;
}

impl Drop for StartupCompletion {
    fn drop(&mut self) {
        if self.disposition.fetch_or(Self::DROPPED, Ordering::AcqRel) & Self::ACCEPTED != 0 {
            self.controller
                .fail("Startup executor abandoned the accepted task");
        }
    }
}

use jadix::app::startup::SurfaceStamp;

pub(crate) struct StartupSession {
    config: StartupConfig,
    pub controller: StartupController,
    pub stamp: SurfaceStamp,
    pub startup_frame: bool,
    pub content_frame: bool,
    drawable: bool,
    started: bool,
}

impl StartupSession {
    pub fn new(mut config: StartupConfig) -> Self {
        if config.mode == SplashMode::Window && !jadix::platform::host_capabilities().multi_window {
            log::warn!(
                "startup window unsupported on {}; using same-host component",
                jadix::platform::host_kind()
            );
            config.mode = SplashMode::Component;
        }
        Self {
            config,
            controller: StartupController::new(),
            stamp: SurfaceStamp {
                scene: 1,
                generation: 0,
            },
            startup_frame: false,
            content_frame: false,
            drawable: false,
            started: false,
        }
    }

    pub fn mode(&self) -> SplashMode {
        self.config.mode
    }

    pub fn system_presentation(&self) -> bool {
        self.config.enabled && self.mode() == SplashMode::System
    }

    pub fn window_options(&self) -> Option<jadix::app::WindowOptions> {
        (self.config.enabled && self.mode() == SplashMode::Window && !self.content_ready())
            .then(|| self.config.window.clone())
            .flatten()
    }

    pub fn window_failed(&mut self) {
        self.config.mode = SplashMode::Component;
    }

    pub fn start(&mut self) {
        if self.started {
            return;
        }
        self.started = true;
        log::info!(
            "startup begin epoch={} mode={:?} enabled={}",
            self.controller.snapshot().epoch,
            self.config.mode,
            self.config.enabled
        );
        if let Some(prepare) = self.config.preparation.as_mut() {
            if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                prepare(self.controller.clone())
            }))
            .is_err()
            {
                self.controller.fail("Startup preparation panicked");
            }
        } else {
            self.controller.ready();
        }
    }

    pub fn attach(&mut self) {
        self.stamp.generation += 1;
        self.drawable = true;
        self.startup_frame = false;
        self.content_frame = false;
    }

    pub fn detach(&mut self) {
        self.stamp.generation += 1;
        self.drawable = false;
        self.startup_frame = false;
        self.content_frame = false;
    }

    pub fn retry(&mut self, epoch: u64) -> bool {
        let snapshot = self.controller.snapshot();
        if snapshot.epoch != epoch
            || !matches!(
                snapshot.phase,
                StartupPhase::Failed(_) | StartupPhase::Cancelled
            )
        {
            return false;
        }
        self.controller.cancellation.cancel();
        self.controller = StartupController::new();
        self.started = false;
        self.startup_frame = false;
        self.content_frame = false;
        true
    }

    pub fn content_ready(&self) -> bool {
        self.controller.snapshot().phase == StartupPhase::Ready
    }

    pub fn renderer_failed(&self, message: String) {
        if !self.content_frame
            && self.controller.update_phase(true, |snapshot| {
                snapshot.phase = StartupPhase::Failed(message);
            })
        {
            self.controller.cancellation.cancel();
        }
    }

    pub fn acknowledge_update(&self) {
        self.controller.wake_pending.store(false, Ordering::Release);
    }

    pub fn view(&mut self) -> Element {
        let snapshot = self.controller.snapshot();
        let mut view = match self.config.view.as_mut().filter(|_| self.config.enabled) {
            Some(factory) => factory(snapshot, self.controller.clone()),
            None => neutral_view(&snapshot, self.controller.clone()),
        };
        view.set_attr(
            "data-startup-epoch",
            self.controller.snapshot().epoch.to_string(),
        );
        view.set_attr("data-startup-view", "true");
        view
    }

    pub fn frame(&self, root: &Element) -> Option<(SurfaceStamp, bool)> {
        let epoch = root.get_attr("data-startup-epoch")?.parse::<u64>().ok()?;
        (epoch == self.controller.snapshot().epoch)
            .then_some((self.stamp, root.get_attr("data-startup-view").is_none()))
    }

    pub fn presented(&mut self, stamp: SurfaceStamp, content: bool) -> bool {
        if !self.drawable || stamp != self.stamp {
            return false;
        }
        if content && self.content_ready() {
            if !self.content_frame {
                log::info!(
                    "startup content_frame epoch={} scene={} generation={}",
                    self.controller.snapshot().epoch,
                    stamp.scene,
                    stamp.generation
                );
            }
            self.content_frame = true;
            true
        } else if !content {
            if !self.startup_frame {
                log::info!(
                    "startup startup_frame epoch={} scene={} generation={}",
                    self.controller.snapshot().epoch,
                    stamp.scene,
                    stamp.generation
                );
            }
            self.startup_frame = true;
            true
        } else {
            false
        }
    }
}

fn neutral_view(snapshot: &StartupSnapshot, controller: StartupController) -> Element {
    let cancelled = jadix::tr_default("startup.cancelled", "Startup cancelled");
    let message = match &snapshot.phase {
        StartupPhase::Failed(error) => error.as_str(),
        StartupPhase::Cancelled => &cancelled,
        _ => &snapshot.stage,
    };
    let mut view = Element::new()
        .w("100%")
        .h("100%")
        .flex_col()
        .items_center()
        .justify_center()
        .gap("1rem")
        .bg(jadix::core::tree::parse_color("background"))
        .child(jadix::primitives::text::text(message));
    view.set_attr("role", "status");
    view.set_attr("aria-live", "polite");
    if matches!(
        snapshot.phase,
        StartupPhase::Failed(_) | StartupPhase::Cancelled
    ) {
        view = view.child(
            Element::new()
                .text(jadix::tr_default("startup.retry", "Retry"))
                .focusable(true)
                .focus_id("startup-retry")
                .accesskit_role("button")
                .on_click(move || controller.retry()),
        );
    } else if snapshot.phase == StartupPhase::Preparing {
        view = view.child(
            Element::new()
                .text(jadix::tr_default("startup.cancel", "Cancel"))
                .focusable(true)
                .focus_id("startup-cancel")
                .accesskit_role("button")
                .on_click(move || controller.cancel()),
        );
    }
    view
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_root(
        startup: Option<&std::cell::RefCell<StartupSession>>,
        content: impl FnOnce() -> Element,
    ) -> Element {
        jadix::app::startup::build_root(
            startup.map(|session| {
                session as &std::cell::RefCell<dyn jadix::app::startup::StartupHost>
            }),
            content,
        )
    }

    #[test]
    fn borderless_is_an_explicit_reversible_window_override() {
        let config = StartupConfig::window(
            jadix::app::WindowOptions::new().decorations(true),
            |_, _| Element::new(),
        );
        assert!(config.window.as_ref().unwrap().decorations);
        let config = config.borderless(true);
        assert!(!config.window.as_ref().unwrap().decorations);
        let config = config.borderless(false);
        assert!(config.window.as_ref().unwrap().decorations);
        let config = StartupConfig::window(
            jadix::app::WindowOptions::new().decorations(false),
            |_, _| Element::new(),
        );
        assert!(!config.window.as_ref().unwrap().decorations);
    }

    #[test]
    fn borderless_does_not_change_shared_host_modes() {
        let system = StartupConfig::system().borderless(true);
        assert_eq!(system.mode, SplashMode::System);
        assert!(system.window.is_none());
        let component = StartupConfig::component(|_, _| Element::new()).borderless(true);
        assert_eq!(component.mode, SplashMode::Component);
        assert!(component.window.is_none());
    }

    #[test]
    fn independent_window_does_not_mount_component_or_content_in_hidden_primary() {
        if !jadix::platform::host_capabilities().multi_window {
            return;
        }
        let calls = std::rc::Rc::new(std::cell::Cell::new(0));
        let view_calls = calls.clone();
        let session = std::cell::RefCell::new(StartupSession::new(
            StartupConfig::window(
                jadix::app::WindowOptions::new().title("Splash"),
                move |_, _| {
                    view_calls.set(view_calls.get() + 1);
                    Element::new()
                },
            )
            .prepare(|_| {}),
        ));
        session.borrow_mut().start();
        let root = build_root(Some(&session), || panic!("business mounted before ready"));
        assert_eq!(
            root.get_attr("data-startup-view").map(String::as_str),
            Some("true")
        );
        assert_eq!(calls.get(), 0);
        assert!(session.borrow().window_options().is_some());
        session.borrow_mut().view();
        assert_eq!(calls.get(), 1);
        session.borrow().controller.ready();
        let root = build_root(Some(&session), || Element::new().text("business"));
        assert!(root.get_attr("data-startup-view").is_none());
        assert!(session.borrow().window_options().is_none());
    }

    #[test]
    fn already_ready_window_skips_branding_and_mounts_content() {
        let session = std::cell::RefCell::new(StartupSession::new(
            StartupConfig::window(jadix::app::WindowOptions::new(), |_, _| {
                panic!("ready startup invoked branding")
            })
            .prepare(|controller| controller.ready()),
        ));
        session.borrow_mut().start();
        assert!(session.borrow().window_options().is_none());
        let root = build_root(Some(&session), || Element::new().text("business"));
        assert_eq!(root.children[0].text.as_deref(), Some("business"));
        assert!(root.get_attr("data-startup-view").is_none());
    }

    #[test]
    fn disabled_window_retains_preparation_without_creating_branding() {
        let mut session = StartupSession::new(
            StartupConfig::window(jadix::app::WindowOptions::new(), |_, _| {
                panic!("disabled branding invoked")
            })
            .enabled(false)
            .prepare(|_| {}),
        );
        session.start();
        assert!(session.window_options().is_none());
        assert!(!session.content_ready());
        assert_eq!(
            session
                .view()
                .get_attr("data-startup-view")
                .map(String::as_str),
            Some("true")
        );
        session.controller.ready();
        assert!(session.content_ready());
    }

    #[test]
    fn failed_window_creation_falls_back_without_restarting_preparation() {
        let mut session = StartupSession::new(
            StartupConfig::window(jadix::app::WindowOptions::new(), |_, _| {
                Element::new().text("fallback")
            })
            .prepare(|_| {}),
        );
        session.start();
        let epoch = session.controller.snapshot().epoch;
        session.window_failed();
        assert_eq!(session.mode(), SplashMode::Component);
        assert!(session.window_options().is_none());
        assert_eq!(session.view().text.as_deref(), Some("fallback"));
        assert_eq!(session.controller.snapshot().epoch, epoch);
        assert!(!session.content_ready());
    }

    #[test]
    fn stale_frames_and_cancelled_results_cannot_release_content() {
        let mut session = StartupSession::new(StartupConfig::system().prepare(|_| {}));
        session.start();
        session.attach();
        let old = session.stamp;
        session.attach();
        assert!(!session.presented(old, false));
        assert!(!session.presented(session.stamp, true));
        assert!(session.presented(session.stamp, false));
        session.controller.cancel();
        session.controller.ready();
        assert_eq!(session.controller.snapshot().phase, StartupPhase::Cancelled);
        assert!(!session.presented(session.stamp, true));
    }

    #[test]
    fn surface_recreation_does_not_repeat_preparation() {
        let calls = std::rc::Rc::new(std::cell::Cell::new(0));
        let count = calls.clone();
        let mut session = StartupSession::new(StartupConfig::system().prepare(move |controller| {
            count.set(count.get() + 1);
            controller.ready();
        }));
        session.start();
        session.attach();
        assert!(session.presented(session.stamp, true));
        session.attach();
        session.start();
        assert!(session.content_ready());
        assert!(!session.content_frame);
        assert_eq!(calls.get(), 1);
        assert!(session.presented(session.stamp, true));
    }

    #[test]
    fn retry_has_a_new_epoch_and_ignores_old_task_completion() {
        let mut session = StartupSession::new(StartupConfig::system().prepare(|_| {}));
        session.start();
        let old = session.controller.clone();
        old.fail("offline");
        assert!(session.retry(old.snapshot().epoch));
        session.start();
        old.ready();
        assert!(!session.content_ready());
        assert_ne!(old.snapshot().epoch, session.controller.snapshot().epoch);
        assert!(!session.retry(old.snapshot().epoch));
        session.controller.ready();
        assert!(session.content_ready());
    }

    #[test]
    fn renderer_failure_after_preparation_ready_remains_retryable() {
        let mut session = StartupSession::new(StartupConfig::system());
        session.attach();
        session.start();
        let previous = session.controller.clone();
        assert!(session.content_ready());
        session.renderer_failed("No adapter".into());
        assert_eq!(
            previous.snapshot().phase,
            StartupPhase::Failed("No adapter".into())
        );
        assert!(previous.cancellation().is_cancelled());
        assert!(session.retry(previous.snapshot().epoch));
        session.start();
        assert!(session.content_ready());
        assert_ne!(
            previous.snapshot().epoch,
            session.controller.snapshot().epoch
        );
    }

    #[test]
    fn late_bootstrap_failure_cannot_replace_presented_content() {
        let mut session = StartupSession::new(StartupConfig::system());
        session.attach();
        session.start();
        assert!(session.presented(session.stamp, true));
        session.renderer_failed("Stale renderer error".into());
        assert!(session.content_ready());
    }

    #[test]
    fn abandoned_work_is_a_failure_instead_of_an_infinite_splash() {
        let executor =
            Arc::new(|_task: jadix_tasks::BlockingTask| Ok(jadix_tasks::TaskHandle::default()));
        let mut session =
            StartupSession::new(StartupConfig::system().blocking(executor, |_| Ok(())));
        session.start();
        assert!(matches!(
            session.controller.snapshot().phase,
            StartupPhase::Failed(_)
        ));
    }

    #[test]
    fn rejected_executor_preserves_its_error_when_it_drops_the_task() {
        let executor = Arc::new(|_task: jadix_tasks::BlockingTask| {
            Err(TaskError::new(
                jadix_tasks::TaskErrorKind::Rejected,
                "executor rejected this task",
            ))
        });
        let mut session =
            StartupSession::new(StartupConfig::system().blocking(executor, |_| Ok(())));
        session.start();
        assert_eq!(
            session.controller.snapshot().phase,
            StartupPhase::Failed("executor rejected this task".into())
        );
        assert!(session.controller.cancellation().is_cancelled());
    }

    #[test]
    fn disabled_branding_keeps_preparation_and_does_not_call_the_component() {
        let mut session = StartupSession::new(
            StartupConfig::component(|_, _| panic!("disabled brand view"))
                .enabled(false)
                .prepare(|_| {}),
        );
        session.start();
        assert!(!session.system_presentation());
        assert!(!session.content_ready());
        assert!(session.view().get_attr("data-startup-view").is_some());
        session.controller.fail("offline");
        assert!(matches!(
            session.controller.snapshot().phase,
            StartupPhase::Failed(_)
        ));
    }

    #[test]
    fn all_rebuild_paths_keep_business_unmounted_until_ready() {
        let session =
            std::cell::RefCell::new(StartupSession::new(StartupConfig::system().prepare(|_| {})));
        session.borrow_mut().start();
        let root = build_root(Some(&session), || panic!("business before ready"));
        assert!(root.get_attr("data-startup-view").is_some());
        session.borrow().controller.ready();
        let root = build_root(Some(&session), || Element::new().text("business"));
        assert!(root.get_attr("data-startup-view").is_none());
    }

    #[test]
    fn stale_epoch_roots_and_bootstrap_roots_do_not_count_as_current_frames() {
        let session =
            std::cell::RefCell::new(StartupSession::new(StartupConfig::system().prepare(|_| {})));
        session.borrow_mut().start();
        session.borrow_mut().attach();
        let root = build_root(Some(&session), || panic!("not ready"));
        assert!(session.borrow().frame(&root).is_some());
        assert!(session.borrow().frame(&Element::new()).is_none());
        let controller = session.borrow().controller.clone();
        controller.fail("offline");
        assert!(session.borrow_mut().retry(controller.snapshot().epoch));
        assert!(session.borrow().frame(&root).is_none());
    }

    #[test]
    fn preparation_panics_are_reported_as_failure() {
        let mut session =
            StartupSession::new(StartupConfig::system().prepare(|_| panic!("bad preparation")));
        session.start();
        assert!(matches!(
            session.controller.snapshot().phase,
            StartupPhase::Failed(_)
        ));
    }

    #[test]
    fn cancelling_a_terminal_result_does_not_change_its_token() {
        let controller = StartupController::new();
        controller.ready();
        controller.cancel();
        assert_eq!(controller.snapshot().phase, StartupPhase::Ready);
        assert!(!controller.cancellation().is_cancelled());
    }

    #[test]
    fn detached_surface_rejects_its_old_frame_without_cancelling_preparation() {
        let mut session = StartupSession::new(StartupConfig::system().prepare(|_| {}));
        session.start();
        session.attach();
        let old = session.stamp;
        session.detach();
        assert!(!session.presented(old, false));
        assert!(!session.controller.cancellation().is_cancelled());
        session.controller.ready();
        session.attach();
        assert!(session.presented(session.stamp, true));
    }

    #[test]
    fn cancelled_executor_result_is_not_a_failure_or_ready() {
        let executor = Arc::new(|task: jadix_tasks::BlockingTask| {
            task();
            Ok(jadix_tasks::TaskHandle::default())
        });
        let mut session = StartupSession::new(StartupConfig::system().blocking(executor, |_| {
            Err(TaskError::new(
                jadix_tasks::TaskErrorKind::Cancelled,
                "cancelled",
            ))
        }));
        session.start();
        assert_eq!(session.controller.snapshot().phase, StartupPhase::Cancelled);
        assert!(session.controller.cancellation().is_cancelled());
    }
}

impl From<StartupConfig> for jadix::app::startup::StartupHandle {
    fn from(config: StartupConfig) -> Self {
        std::rc::Rc::new(std::cell::RefCell::new(StartupSession::new(config)))
    }
}

impl jadix::app::startup::StartupHost for StartupSession {
    fn epoch(&self) -> u64 {
        self.controller.snapshot().epoch
    }
    fn content_presented(&self) -> bool {
        self.content_frame
    }
    fn cancel(&self) {
        self.controller.cancel();
        self.controller.cancellation().cancel();
    }
    fn present_system(
        &self,
        window: Arc<dyn winit::window::Window>,
        icon: Option<&jadix::app::AppIcon>,
    ) -> Result<Option<Box<dyn jadix::app::startup::StartupPresentation>>, String> {
        crate::system::present(window, icon).map(|presentation| {
            presentation
                .map(|value| Box::new(value) as Box<dyn jadix::app::startup::StartupPresentation>)
        })
    }
    fn presentation_failed(&self, window: Option<&dyn winit::window::Window>, message: &str) {
        crate::system::failed(window, message);
    }
    fn handoff(&self, content: bool) {
        crate::system::handoff(content);
    }
    fn content_ready(&self) -> bool {
        StartupSession::content_ready(self)
    }
    fn system_presentation(&self) -> bool {
        StartupSession::system_presentation(self)
    }
    fn window_options(&self) -> Option<jadix::app::WindowOptions> {
        StartupSession::window_options(self)
    }
    fn window_failed(&mut self) {
        StartupSession::window_failed(self)
    }
    fn start(&mut self) {
        StartupSession::start(self)
    }
    fn attach(&mut self) {
        StartupSession::attach(self)
    }
    fn detach(&mut self) {
        StartupSession::detach(self)
    }
    fn retry(&mut self, epoch: u64) -> bool {
        StartupSession::retry(self, epoch)
    }
    fn renderer_failed(&self, message: String) {
        StartupSession::renderer_failed(self, message)
    }
    fn acknowledge_update(&self) {
        StartupSession::acknowledge_update(self)
    }
    fn view(&mut self) -> Element {
        StartupSession::view(self)
    }
    fn frame(&self, root: &Element) -> Option<(SurfaceStamp, bool)> {
        StartupSession::frame(self, root)
    }
    fn presented(&mut self, stamp: SurfaceStamp, content: bool) -> bool {
        StartupSession::presented(self, stamp, content)
    }
}
