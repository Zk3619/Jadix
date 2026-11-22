use crate::layout::{LayoutError, LayoutStats, MeasureRequest, NativeLayout};
use crate::presentation::{
    CommitBatch, MeasureResult, NodeDescriptor, PlatformEvent, PresentationNode, RootId,
};
use crate::{BackendError, HeadlessRoot, ResourceLimits};

pub use jadix_core::presentation::ApplyReceipt;

/// One shared monitor for the declarative root and its platform display loop.
#[cfg(feature = "performance")]
#[derive(Default)]
pub struct NativePerformance {
    monitor: jadix_core::performance::Monitor,
    nodes: usize,
    retired: usize,
}
#[cfg(feature = "performance")]
impl NativePerformance {
    pub fn display_tick(&mut self, refresh: Option<f64>, work: Option<std::time::Duration>) {
        self.monitor.tick(refresh);
        if let Some(work) = work { self.monitor.frame(work); }
    }
    pub fn snapshot(&mut self) -> jadix_core::performance::Snapshot {
        self.monitor.snapshot(self.nodes, self.retired)
    }
}

pub trait PlatformHost {
    fn register_font(&mut self, _: &[u8]) -> Result<Vec<String>, String> { Err("font registration unavailable on this host".into()) }
    /// Changes when native shaping resources change independently of descriptors.
    fn measurement_revision(&self) -> u64 { 0 }
    fn request_file(&mut self, _: &jadix_core::file_request::FileRequest) -> Result<std::sync::mpsc::Receiver<jadix_core::file_request::FileResult>, String> {
        Err("file operations are unavailable on this host".into())
    }
    fn clipboard(&mut self, _: &jadix_core::clipboard::ClipboardRequest) -> Result<std::sync::mpsc::Receiver<jadix_core::clipboard::ClipboardResult>, String> {
        Err("clipboard is unavailable on this host".into())
    }
    #[cfg(feature = "performance")]
    fn bind_performance(&mut self, _: std::rc::Weak<std::cell::RefCell<NativePerformance>>) {}
    /// Read root-local OS preferences on the UI thread. Unknown hosts keep the
    /// application's explicit snapshot; a failed read never replaces it.
    fn read_environment(&self) -> Result<Option<crate::environment::EnvironmentSnapshot>, String> { Ok(None) }
    fn updater(&self) -> Option<jadix_core::updater::Updater> { None }
    fn websocket(&self) -> Option<jadix_net::websocket::SocketService> { None }
    fn fetch(&self) -> Option<jadix_net::service::FetchService> { None }
    fn desktop_services(&self) -> Option<jadix_core::desktop_services::DesktopServices> { None }
    fn speech(&self) -> Option<jadix_core::speech::SpeechService> { None }
    fn audio(&self) -> Option<jadix_core::audio::AudioService> { None }
    fn pump_audio(&mut self, _: RootId) {}

    /// Execute queued web work. Wake batches after releasing the application borrow.
    fn pump_web(&mut self) -> Vec<jadix_core::web_evaluation::EvaluationBatch> { Vec::new() }
    /// Run returned provider jobs after releasing the application/root borrow.
    fn take_resource_work(&mut self) -> Vec<jadix_core::web_resource::ResourceWork> { Vec::new() }
    fn set_tray(&mut self, _: RootId, _: Option<&jadix_core::tray::Tray>) -> Result<(), String> {
        Err("status items are unavailable on this host".into())
    }
    fn take_tray_events(&mut self) -> Vec<String> { Vec::new() }

    fn evaluate_web(&mut self, _: crate::presentation::NodeId, _: &str) -> jadix_core::web_evaluation::WebEvaluation {
        jadix_core::web_evaluation::WebEvaluation::failed(jadix_core::web_evaluation::WebEvaluationError::Unavailable)
    }
    fn show_share(
        &mut self,
        _: &jadix_core::desktop::Share,
    ) -> Result<std::sync::mpsc::Receiver<jadix_core::desktop::ShareResult>, String> {
        Err("sharing is unavailable on this host".into())
    }

    fn open_url(
        &mut self,
        _: &str,
    ) -> Result<std::sync::mpsc::Receiver<jadix_core::desktop::OpenUrlResult>, String> {
        Err("opening URLs is unavailable on this host".into())
    }

    fn window_declarations(&self) -> Option<jadix_core::window::WindowDeclarations> {
        None
    }

    fn request_window(
        &mut self,
        _: &jadix_core::window::WindowRequest,
    ) -> Result<std::sync::mpsc::Receiver<jadix_core::window::WindowResult>, String> {
        Err("window requests are unavailable on this host".into())
    }

    fn is_alive(&self) -> bool {
        true
    }

    fn capabilities(&self) -> crate::presentation::capabilities::BackendCapabilities {
        let mut capabilities = crate::presentation::capabilities::BackendCapabilities::headless();
        capabilities
            .node_kinds
            .retain(|kind| !matches!(kind, crate::presentation::capabilities::NodeKind::Popover | crate::presentation::capabilities::NodeKind::Hosted));
        capabilities
    }

    fn measure(
        &mut self,
        request: MeasureRequest,
        descriptor: &NodeDescriptor,
    ) -> Result<MeasureResult, LayoutError>;
    fn publish(
        &mut self,
        root: RootId,
        revision: u64,
        roots: &[PresentationNode],
    ) -> Result<(), String>;

    fn publish_batch(
        &mut self,
        batch: &CommitBatch,
        roots: &[PresentationNode],
    ) -> Result<ApplyReceipt, String> {
        self.publish(batch.root, batch.revision, roots)?;
        Ok(ApplyReceipt::for_batch(batch))
    }

    /// Coalesce a presentation refresh after a shared reactive deadline.
    fn schedule_reactive_wake(&mut self, _: Option<std::time::Duration>) {}

    fn dispose(&mut self);

    fn take_events(&mut self) -> Result<Vec<PlatformEvent>, String> {
        Ok(Vec::new())
    }

    fn bind_navigation(&mut self, _: &[crate::presentation::NodeId]) -> Result<bool, String> {
        Ok(false)
    }

    fn bind_grid_navigation(&mut self, _: &[crate::presentation::NodeId]) -> Result<bool, String> {
        Ok(false)
    }

    fn bind_drag(&mut self, _: &[crate::presentation::NodeId]) -> Result<bool, String> {
        Ok(false)
    }

    fn scroll_to(&mut self, _: crate::presentation::NodeId, _: [f64; 2]) -> Result<(), String> {
        Err(String::from(
            "programmatic scrolling is unavailable on this host",
        ))
    }

    /// Stop native inertia/animation while preserving the current document offset.
    fn stop_scroll(&mut self, _: crate::presentation::NodeId) -> Result<(), String> {
        Err("stopping scrolling is unavailable on this host".into())
    }

    fn zoom_to(&mut self, _: crate::presentation::NodeId, _: f64, _: [f64; 2]) -> Result<(), String> {
        Err("viewport magnification is unavailable on this host".into())
    }

    fn configure_deep_links(&mut self, _: jadix_core::deep_link::DeepLinkConfig) -> Result<(), String> { Err("deep links unavailable".into()) }
    fn take_deep_links(&mut self) -> Vec<jadix_core::deep_link::DeepLink> { Vec::new() }
    /// False hands a second process's launch to the already-running primary.
    fn configure_activation(&mut self, _: jadix_core::deep_link::ApplicationActivation) -> Result<bool, String> { Err("application activation unavailable".into()) }
    fn take_application_launches(&mut self) -> Vec<jadix_core::deep_link::ApplicationLaunch> { Vec::new() }

    fn set_app_address(&mut self, _: RootId, _: Option<&str>) -> Result<(), String> {
        Ok(())
    }
}

#[derive(Debug)]
pub enum HostError {
    Backend(BackendError),
    Layout(LayoutError),
    Platform(String),
    #[cfg(feature = "motion")]
    Motion(crate::motion::MotionError),
}

impl std::fmt::Display for HostError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for HostError {}

pub struct NativeRoot<Host: PlatformHost> {
    #[cfg(feature = "performance")]
    performance: std::rc::Rc<std::cell::RefCell<NativePerformance>>,
    #[cfg(feature = "performance")]
    layout_time: std::time::Duration,
    #[cfg(feature = "performance")]
    script_time: std::time::Duration,
    model: HeadlessRoot,
    layout: NativeLayout,
    host: Host,
    disposed: bool,
    last_layout: LayoutStats,
    last_dirty: Vec<crate::presentation::NodeId>,
    last_operations: [usize; 5],
    environment: Option<(u64, u64)>,
    app_address: Option<String>,
    host_environment: Option<crate::environment::EnvironmentSnapshot>,
    measurement_revision: u64,
    #[cfg(feature = "i18n")]
    locale_context: std::rc::Rc<jadix_core::i18n::LocaleContext>,
}

impl<Host: PlatformHost> NativeRoot<Host> {
    pub fn new(host: Host) -> Self {
        Self::with_limits(host, ResourceLimits::default())
    }

    pub fn with_limits(host: Host, limits: ResourceLimits) -> Self {
        Self::with_lifetime(
            host,
            crate::presentation::RootLifetime::new(RootId::new()),
            limits,
        )
    }

    pub fn with_lifetime(
        mut host: Host,
        lifetime: crate::presentation::RootLifetime,
        limits: ResourceLimits,
    ) -> Self {
        let model = HeadlessRoot {
            limits,
            ..HeadlessRoot::with_lifetime(lifetime, host.capabilities())
        };
        let layout = NativeLayout::new(model.lifetime());
        #[cfg(feature = "performance")]
        let performance = {
            let monitor = std::rc::Rc::new(std::cell::RefCell::new(NativePerformance::default()));
            host.bind_performance(std::rc::Rc::downgrade(&monitor));
            monitor
        };
        Self {
            #[cfg(feature = "performance")]
            performance,
            #[cfg(feature = "performance")]
            layout_time: Default::default(),
            #[cfg(feature = "performance")]
            script_time: Default::default(),
            model,
            layout,
            host,
            disposed: false,
            last_layout: LayoutStats::default(),
            last_dirty: Vec::new(),
            last_operations: [0; 5],
            environment: None,
            app_address: None,
            host_environment: None,
            measurement_revision: 0,
            #[cfg(feature = "i18n")]
            locale_context: std::rc::Rc::new(jadix_core::i18n::LocaleContext::new()),
        }
    }

    #[cfg(feature = "performance")]
    pub fn performance(&mut self) -> jadix_core::performance::Snapshot {
        let usage = self.model.resource_usage();
        let mut performance = self.performance.borrow_mut();
        performance.nodes = usage.nodes;
        performance.retired = usage.retired_nodes;
        performance.snapshot()
    }
    #[cfg(feature = "performance")]
    pub fn display_tick(&mut self, refresh_rate: Option<f64>, work: Option<std::time::Duration>) {
        self.performance.borrow_mut().display_tick(refresh_rate, work);
    }
    #[cfg(feature = "performance")]
    pub fn script_metrics(&mut self, busy: std::time::Duration, heap: Option<jadix_core::performance::HeapSample>) {
        self.script_time = self.script_time.saturating_add(busy);
        self.performance.borrow_mut().monitor.set_heap(heap);
    }

    pub fn model(&self) -> &HeadlessRoot {
        &self.model
    }
    pub fn host(&self) -> &Host {
        &self.host
    }

    pub fn host_mut(&mut self) -> &mut Host {
        &mut self.host
    }

    pub fn evaluate_web(&mut self, node: crate::presentation::NodeId, script: &str) -> jadix_core::web_evaluation::WebEvaluation {
        use jadix_core::{hosted::HostedContent, presentation::NodeContent, web_evaluation::{WebEvaluation, WebEvaluationError}};
        let valid = !self.disposed && self.model.node(node).is_some_and(|node| {
            matches!(&node.descriptor.content, NodeContent::Hosted { specification }
                if matches!(HostedContent::from_json(specification), Ok(HostedContent::Web { .. })))
        });
        if valid { self.host.evaluate_web(node, script) }
        else { WebEvaluation::failed(WebEvaluationError::Unavailable) }
    }

    pub fn pump_web(&mut self) -> Vec<jadix_core::web_evaluation::EvaluationBatch> {
        jadix_core::reactivity::collect_reactive_garbage();
        self.host.pump_web()
    }
    pub fn take_resource_work(&mut self) -> Vec<jadix_core::web_resource::ResourceWork> { self.host.take_resource_work() }

    /// Translate and build within this root's independent locale, fallback and timezone.
    #[cfg(feature = "i18n")]
    pub fn with_locale<R>(&self, body: impl FnOnce() -> R) -> R { self.locale_context.with(body) }

    #[cfg(feature = "i18n")]
    pub fn locale_context(&self) -> std::rc::Rc<jadix_core::i18n::LocaleContext> {
        std::rc::Rc::clone(&self.locale_context)
    }

    #[cfg(feature = "i18n")]
    pub fn set_locale_context(&mut self, context: std::rc::Rc<jadix_core::i18n::LocaleContext>) -> Result<(), HostError> {
        if self.disposed { return Err(HostError::Backend(BackendError::Disposed)); }
        if let Some(snapshot) = &self.host_environment {
            context.update_environment(&snapshot.locale.locale, snapshot.locale.time_zone.as_ref().map(|tz| tz.id.as_str())).map_err(HostError::Platform)?;
        }
        self.locale_context = context;
        Ok(())
    }

    pub fn set_tray(&mut self, tray: Option<&jadix_core::tray::Tray>) -> Result<(), HostError> {
        if self.disposed { return Err(HostError::Backend(BackendError::Disposed)); }
        self.host.set_tray(self.model.id(), tray).map_err(HostError::Platform)
    }

    pub fn app_address(&self) -> Option<&str> {
        self.app_address.as_deref()
    }

    pub fn environment_snapshot(&self) -> Option<&crate::environment::EnvironmentSnapshot> {
        self.host_environment.as_ref()
    }

    /// Refresh before building reactive content so locale and time zone are
    /// already available to its projections, as well as to native layout.
    pub fn refresh_environment(&mut self) -> Result<bool, HostError> {
        if self.disposed || self.model.lifetime().is_disposed() || !self.host.is_alive() {
            self.dispose();
            return Err(HostError::Backend(BackendError::Disposed));
        }
        match self.host.read_environment().map_err(HostError::Platform)? {
            Some(snapshot) => self.set_environment_snapshot(snapshot),
            None => Ok(false),
        }
    }

    /// Host notifications enter through the same validated contract as other roots.
    pub fn set_environment_snapshot(&mut self, snapshot: crate::environment::EnvironmentSnapshot) -> Result<bool, HostError> {
        snapshot.validate().map_err(|e| HostError::Platform(e.to_string()))?;
        if self.disposed { return Err(HostError::Backend(BackendError::Disposed)); }
        if self.host_environment.as_ref() == Some(&snapshot) { return Ok(false); }
        #[cfg(feature = "i18n")]
        self.locale_context.update_environment(&snapshot.locale.locale, snapshot.locale.time_zone.as_ref().map(|tz| tz.id.as_str())).map_err(HostError::Platform)?;
        self.host_environment = Some(snapshot);
        self.layout.invalidate_measurements();
        Ok(true)
    }

    /// Set the address of this root. Relative addresses are kept for local navigation.
    pub fn set_app_address(&mut self, address: Option<&str>) -> Result<(), HostError> {
        if self.disposed || self.model.lifetime().is_disposed() || !self.host.is_alive() {
            self.dispose();
            return Err(HostError::Backend(BackendError::Disposed));
        }
        if let Some(address) = address {
            jadix_core::deep_link::validate_app_address(address).map_err(|error| HostError::Platform(error.into()))?;
        }
        self.host
            .set_app_address(self.model.id(), address)
            .map_err(HostError::Platform)?;
        self.app_address = address.map(str::to_owned);
        Ok(())
    }

    pub fn inspection(&self, include_text: bool) -> serde_json::Value {
        let mut result = self.model.inspection(include_text);
        result["layout"] = serde_json::json!({"nodes": self.last_layout.nodes, "measurements": self.last_layout.measurements,
            "cache_hits": self.last_layout.cache_hits, "cached_nodes": self.layout.cached_nodes()});
        result["last_commit"] = serde_json::json!({"creates": self.last_operations[0], "updates": self.last_operations[1],
            "moves": self.last_operations[2], "removes": self.last_operations[3], "focus": self.last_operations[4],
            "dirty_nodes": self.last_dirty.iter().map(|node| node.token()).collect::<Vec<_>>()});
        if let Some((identity, revision)) = self.environment {
            result["environment"] = serde_json::json!({"identity":identity.to_string(),"revision":revision.to_string()});
        }
        result
    }

    pub fn scroll_to(
        &mut self,
        node: crate::presentation::NodeId,
        offset: [f64; 2],
    ) -> Result<(), HostError> {
        if self.disposed || self.model.lifetime().is_disposed() || !self.host.is_alive() {
            self.dispose();
            return Err(HostError::Backend(BackendError::Disposed));
        }
        if offset
            .iter()
            .any(|value| !value.is_finite() || *value < 0.0)
            || !self.model.node(node).is_some_and(|node| {
                matches!(
                    node.descriptor.content,
                    crate::presentation::NodeContent::Scroll
                )
            })
        {
            return Err(HostError::Backend(BackendError::InvalidEvent));
        }
        self.host
            .scroll_to(node, offset)
            .map_err(HostError::Platform)
    }

    pub fn stop_scroll(&mut self, node: crate::presentation::NodeId) -> Result<(), HostError> {
        if self.disposed || self.model.lifetime().is_disposed() || !self.host.is_alive() {
            self.dispose(); return Err(HostError::Backend(BackendError::Disposed));
        }
        if !self.model.node(node).is_some_and(|n| matches!(n.descriptor.content, crate::presentation::NodeContent::Scroll)) {
            return Err(HostError::Backend(BackendError::InvalidEvent));
        }
        self.host.stop_scroll(node).map_err(HostError::Platform)
    }

    /// Magnify around a viewport-local anchor, measured in logical pixels.
    pub fn zoom_to(&mut self, node: crate::presentation::NodeId, scale: f64, anchor: [f64; 2]) -> Result<(), HostError> {
        if self.disposed || self.model.lifetime().is_disposed() || !self.host.is_alive() {
            self.dispose();
            return Err(HostError::Backend(BackendError::Disposed));
        }
        let policy = self.model.node(node).filter(|n| matches!(n.descriptor.content, crate::presentation::NodeContent::Scroll))
            .and_then(|n| n.descriptor.style.scroll_zoom);
        let Some(policy) = policy else { return Err(HostError::Backend(BackendError::InvalidEvent)); };
        if !scale.is_finite() || scale <= 0.0 || anchor.into_iter().any(|v| !v.is_finite()) {
            return Err(HostError::Backend(BackendError::InvalidEvent));
        }
        self.host.zoom_to(node, scale.clamp(policy.min, policy.max), anchor).map_err(HostError::Platform)
    }

    pub fn present(
        &mut self,
        roots: &mut [PresentationNode],
        viewport: [f32; 2],
        environment: u64,
    ) -> Result<LayoutStats, HostError> {
        self.refresh_environment()?;
        if self.environment.take().is_some() {
            self.layout.invalidate_measurements();
        }
        if let Some(snapshot) = &self.host_environment {
            let mut projected = roots.to_vec();
            let mut pending: Vec<_> = projected.iter_mut().collect();
            while let Some(node) = pending.pop() {
                if node.descriptor.style.direction.is_none() {
                    node.descriptor.style.direction = snapshot.layout_direction;
                }
                node.descriptor.style.font_size *= snapshot.font_scale;
                pending.extend(node.children.iter_mut());
            }
            let stats = self.prepare_layout(&mut projected, viewport, environment)?;
            self.publish_layout(&projected, stats)
        } else {
            let stats = self.prepare_layout(roots, viewport, environment)?;
            self.publish_layout(roots, stats)
        }
    }

    #[cfg(feature = "consistent-ui")]
    pub fn present_consistent(
        &mut self,
        roots: &[PresentationNode],
        viewport: [f32; 2],
        environment: &crate::environment::Environment,
        theme: &crate::ConsistentTheme,
    ) -> Result<LayoutStats, HostError> {
        if !environment.is_alive() {
            return Err(HostError::Platform("environment owner disposed".into()));
        }
        self.model
            .check_resources(roots)
            .map_err(HostError::Backend)?;
        let mut projected = theme
            .project(roots, &environment.snapshot())
            .map_err(|error| HostError::Platform(error.into()))?;
        self.present_environment(&mut projected, viewport, environment)
    }

    /// Bind a validated root-local environment to measurement invalidation.
    /// Applications map preferences to content/style; this does not fabricate
    /// missing platform mappings or reset node/edit identities.
    pub fn present_environment(
        &mut self,
        roots: &mut [PresentationNode],
        viewport: [f32; 2],
        environment: &crate::environment::Environment,
    ) -> Result<LayoutStats, HostError> {
        if !environment.is_alive() {
            return Err(HostError::Platform("environment owner disposed".into()));
        }
        let key = (environment.identity(), environment.revision());
        if self.environment != Some(key) {
            self.layout.invalidate_measurements();
        }
        let stats = self.prepare_layout(roots, viewport, key.1)?;
        let result = self.publish_layout(roots, stats)?;
        self.environment = Some(key);
        Ok(result)
    }

    #[cfg(feature = "motion")]
    pub fn present_environment_with_motion(
        &mut self,
        roots: &mut [PresentationNode],
        viewport: [f32; 2],
        environment: &crate::environment::Environment,
        motion: &mut crate::motion::LayoutMotion,
        now: std::time::Duration,
    ) -> Result<LayoutStats, HostError> {
        let result = (|| {
            if !environment.is_alive() {
                return Err(HostError::Platform("environment owner disposed".into()));
            }
            let key = (environment.identity(), environment.revision());
            if self.environment != Some(key) {
                self.layout.invalidate_measurements();
            }
            let stats = self.prepare_layout(roots, viewport, key.1)?;
            motion
                .apply(
                    &self.model.lifetime(),
                    roots,
                    now,
                    crate::motion::MotionPolicy::from_environment(&environment.snapshot()),
                )
                .map_err(HostError::Motion)?;
            let result = self.publish_layout(roots, stats)?;
            self.environment = Some(key);
            Ok(result)
        })();
        if self.model.lifetime().is_disposed() {
            motion.dispose();
        }
        result
    }

    #[cfg(all(feature = "motion", feature = "consistent-ui"))]
    pub fn present_consistent_with_motion(
        &mut self,
        roots: &[PresentationNode],
        viewport: [f32; 2],
        environment: &crate::environment::Environment,
        theme: &crate::ConsistentTheme,
        motion: &mut crate::motion::LayoutMotion,
        now: std::time::Duration,
    ) -> Result<LayoutStats, HostError> {
        if !environment.is_alive() {
            return Err(HostError::Platform("environment owner disposed".into()));
        }
        self.model
            .check_resources(roots)
            .map_err(HostError::Backend)?;
        let mut projected = theme
            .project(roots, &environment.snapshot())
            .map_err(|error| HostError::Platform(error.into()))?;
        self.present_environment_with_motion(&mut projected, viewport, environment, motion, now)
    }

    #[cfg(feature = "motion")]
    pub fn present_with_motion(
        &mut self,
        roots: &mut [PresentationNode],
        viewport: [f32; 2],
        environment: u64,
        motion: &mut crate::motion::LayoutMotion,
        now: std::time::Duration,
        policy: crate::motion::MotionPolicy,
    ) -> Result<LayoutStats, HostError> {
        let result = (|| {
            if self.environment.take().is_some() {
                self.layout.invalidate_measurements();
            }
            let stats = self.prepare_layout(roots, viewport, environment)?;
            motion
                .apply(&self.model.lifetime(), roots, now, policy)
                .map_err(HostError::Motion)?;
            self.publish_layout(roots, stats)
        })();
        if self.model.lifetime().is_disposed() {
            motion.dispose();
        }
        result
    }

    fn prepare_layout(
        &mut self,
        roots: &mut [PresentationNode],
        viewport: [f32; 2],
        environment: u64,
    ) -> Result<LayoutStats, HostError> {
        if self.disposed || self.model.lifetime().is_disposed() || !self.host.is_alive() {
            self.dispose();
            return Err(HostError::Backend(BackendError::Disposed));
        }
        self.model
            .check_resources(roots)
            .map_err(HostError::Backend)?;
        let mut pending: Vec<_> = roots.iter().collect();
        let revision = self.host.measurement_revision();
        if self.measurement_revision != revision {
            self.layout.invalidate_measurements();
            self.measurement_revision = revision;
        }
        while let Some(node) = pending.pop() {
            if !self
                .model
                .capabilities()
                .supports_descriptor(&node.descriptor)
            {
                return Err(HostError::Backend(BackendError::Unsupported(
                    node.descriptor.content.kind(),
                )));
            }
            pending.extend(&node.children);
        }
        #[cfg(feature = "performance")]
        let started = std::time::Instant::now();
        let host = &mut self.host;
        let result = self.layout
            .layout(roots, viewport, environment, |request, descriptor| {
                host.measure(request, descriptor)
            })
            .map_err(HostError::Layout);
        #[cfg(feature = "performance")]
        { self.layout_time = started.elapsed(); }
        result
    }

    fn publish_layout(
        &mut self,
        roots: &[PresentationNode],
        stats: LayoutStats,
    ) -> Result<LayoutStats, HostError> {
        #[cfg(feature = "performance")]
        let started = std::time::Instant::now();
        let mut dirty = std::collections::BTreeSet::new();
        let mut operations = [0usize; 5];
        if let Some(batch) = self.model.diff_batch(roots).map_err(HostError::Backend)? {
            // Validate before publishing, but retain only the changed descriptors for
            // text-only batches. The live model advances after the host receipt.
            let text_updates = self.model.prepare_text_updates(&batch).map_err(HostError::Backend)?;
            let candidate = if text_updates.is_none() {
                Some(self.model.prepare_commit(batch.clone()).map_err(HostError::Backend)?
                    .ok_or(HostError::Backend(BackendError::Revision))?)
            } else { None };
            let applied = self.host.publish_batch(&batch, roots);
            if applied.as_ref() != Ok(&ApplyReceipt::for_batch(&batch)) {
                let error = applied.err().unwrap_or_else(|| {
                    String::from("platform returned a stale or foreign apply receipt")
                });
                self.dispose();
                return Err(HostError::Platform(error));
            }
            if self.model.lifetime().is_disposed() {
                self.dispose();
                return Err(HostError::Backend(BackendError::Disposed));
            }
            for operation in &batch.operations {
                use crate::presentation::Operation;
                let (index, node) = match operation {
                    Operation::Create { node, .. } => (0, Some(*node)),
                    Operation::Update { node, .. } => (1, Some(*node)),
                    Operation::Move { node, .. } => (2, Some(*node)),
                    Operation::Remove { node } => (3, Some(*node)),
                    Operation::Focus { node } => (4, *node),
                };
                operations[index] += 1;
                if let Some(node) = node {
                    dirty.insert(node);
                }
            }
            if let Some((updates, text)) = text_updates {
                self.model.apply_text_updates(updates, text, batch.revision);
            } else if let Some(candidate) = candidate {
                self.model = candidate;
            }
            #[cfg(feature = "performance")]
            {
                let usage = self.model.resource_usage();
                let mut performance = self.performance.borrow_mut();
                performance.nodes = usage.nodes;
                performance.retired = usage.retired_nodes;
                performance.monitor.commit(std::mem::take(&mut self.script_time), self.layout_time.saturating_add(started.elapsed()));
            }
        }
        self.host.schedule_reactive_wake(jadix_core::clock::next_wake_delay());
        self.last_layout = stats.clone();
        self.last_dirty = dirty.into_iter().collect();
        self.last_operations = operations;
        Ok(stats)
    }

    pub fn event(&mut self, event: &PlatformEvent) -> Result<(), BackendError> {
        if self.model.lifetime().is_disposed() || !self.host.is_alive() {
            self.dispose();
            return Err(BackendError::Disposed);
        }
        self.model.accept_event(event)
    }

    pub fn focus(&mut self, node: Option<crate::presentation::NodeId>) -> Result<(), HostError> {
        if self.disposed || self.model.lifetime().is_disposed() || !self.host.is_alive() {
            self.dispose();
            return Err(HostError::Backend(BackendError::Disposed));
        }
        if self.model.focused() == node {
            return Ok(());
        }
        let batch = CommitBatch {
            root: self.model.id(),
            previous: self.model.revision(),
            revision: self
                .model
                .revision()
                .checked_add(1)
                .ok_or(HostError::Backend(BackendError::Revision))?,
            operations: vec![crate::presentation::Operation::Focus { node }],
        };
        let candidate = self
            .model
            .prepare_commit(batch.clone())
            .map_err(HostError::Backend)?
            .ok_or(HostError::Backend(BackendError::Revision))?;
        let mut built = std::collections::BTreeMap::new();
        let mut pending: Vec<_> = self
            .model
            .roots()
            .iter()
            .rev()
            .map(|identifier| (*identifier, false))
            .collect();
        while let Some((identifier, visited)) = pending.pop() {
            let current = self
                .model
                .node(identifier)
                .ok_or(HostError::Backend(BackendError::UnknownNode(identifier)))?;
            if !visited {
                pending.push((identifier, true));
                pending.extend(
                    current
                        .children
                        .iter()
                        .rev()
                        .map(|identifier| (*identifier, false)),
                );
            } else {
                let children = current
                    .children
                    .iter()
                    .map(|identifier| built.remove(identifier).unwrap())
                    .collect();
                built.insert(
                    identifier,
                    PresentationNode {
                        id: identifier,
                        descriptor: current.descriptor.clone(),
                        children,
                    },
                );
            }
        }
        let roots: Vec<_> = self
            .model
            .roots()
            .iter()
            .map(|identifier| built.remove(identifier).unwrap())
            .collect();
        let applied = self.host.publish_batch(&batch, &roots);
        if applied.as_ref() != Ok(&ApplyReceipt::for_batch(&batch))
            || candidate.lifetime().is_disposed()
        {
            let error = applied.err().unwrap_or_else(|| {
                String::from("platform focus returned a stale receipt or disposed root")
            });
            self.dispose();
            return Err(HostError::Platform(error));
        }
        self.model = candidate;
        self.last_operations = [0, 0, 0, 0, 1];
        self.last_dirty = node.into_iter().collect();
        Ok(())
    }

    pub fn poll_events(&mut self) -> Result<Vec<PlatformEvent>, HostError> {
        if self.disposed || self.model.lifetime().is_disposed() || !self.host.is_alive() {
            self.dispose();
            return Err(HostError::Backend(BackendError::Disposed));
        }
        self.host.pump_audio(self.model.id());
        let events = match self.host.take_events() {
            Ok(events) => events,
            Err(error) => {
                self.dispose();
                return Err(HostError::Platform(error));
            }
        };
        for event in &events {
            self.event(event).map_err(HostError::Backend)?;
        }
        Ok(events)
    }

    pub fn dispose(&mut self) {
        if !self.disposed {
            self.disposed = true;
            self.model.dispose();
            self.layout.dispose();
            self.last_layout = LayoutStats::default();
            self.last_dirty.clear();
            self.last_operations = [0; 5];
            self.app_address = None;
            self.host.dispose();
            #[cfg(feature = "i18n")]
            self.locale_context.dispose();
        }
    }
}

impl<Host: PlatformHost> Drop for NativeRoot<Host> {
    fn drop(&mut self) {
        self.dispose();
    }
}
