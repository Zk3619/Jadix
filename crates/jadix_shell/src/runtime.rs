#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResourceLimits {
    pub requests: usize,
    pub mounts: usize,
    pub handles: usize,
    pub memory_bytes: usize,
    pub stack_bytes: usize,
    /// Initial application source evaluation; queued jobs retain turn limits.
    pub load_milliseconds: u64,
    /// Each interaction or microtask-drain deadline.
    pub turn_milliseconds: u64,
    pub jobs_per_turn: usize,
    pub timers: usize,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            requests: 64,
            mounts: 16,
            handles: 1024,
            memory_bytes: 32 * 1024 * 1024,
            stack_bytes: 512 * 1024,
            load_milliseconds: 1000,
            turn_milliseconds: 20,
            jobs_per_turn: 64,
            timers: 64,
        }
    }
}

impl ResourceLimits {
    pub fn is_valid(&self) -> bool {
        self.requests <= 1024
            && self.mounts <= 64
            && self.handles <= 4096
            && (1024 * 1024..=256 * 1024 * 1024).contains(&self.memory_bytes)
            && (64 * 1024..=2 * 1024 * 1024).contains(&self.stack_bytes)
            && (1..=10_000).contains(&self.load_milliseconds)
            && (1..=1000).contains(&self.turn_milliseconds)
            && (1..=1024).contains(&self.jobs_per_turn)
            && self.timers <= 1024
    }
}

#[cfg(all(feature = "native-vm", not(target_arch = "wasm32")))]
pub mod native {
    use super::ResourceLimits;
    use std::{
        cell::Cell,
        rc::Rc,
        time::{Duration, Instant},
    };

    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum VmError {
        InvalidLimits,
        Initialization,
        Execution,
        Timeout,
        JobLimit,
        SourceLimit,
    }

    pub struct BudgetedRuntime {
        context: rquickjs::Context,
        runtime: rquickjs::Runtime,
        deadline: Rc<Cell<Option<Instant>>>,
        limits: ResourceLimits,
    }

    impl BudgetedRuntime {
        /// Borrow the current UI thread; grow a temporary stack segment only
        /// when the VM budget plus Rust/host-callback headroom will not fit.
        /// The non-parallel rquickjs build does not refresh the stack top when
        /// Context::with moves onto a different segment. Refresh it explicitly
        /// while holding that context's lock. No permanent PE stack bump.
        fn with_stack<R>(limits: ResourceLimits, f: impl FnOnce() -> R) -> R {
            let required = limits.stack_bytes + 256 * 1024;
            stacker::maybe_grow(required, required + 256 * 1024, f)
        }
        fn refresh_stack(context: &rquickjs::Ctx<'_>) {
            // SAFETY: called synchronously inside Context::with on this UI
            // thread, with a live context/runtime and its lock held.
            unsafe {
                rquickjs::qjs::JS_UpdateStackTop(rquickjs::qjs::JS_GetRuntime(
                    context.as_raw().as_ptr(),
                ));
            }
        }
        /// Install a host JSON boundary in this runtime, before application code.
        /// Callbacks must be bounded and must not recursively enter this VM.
        pub fn bind_json(
            &self,
            name: &str,
            callback: impl Fn(String) -> String + 'static,
        ) -> Result<(), VmError> {
            self.context
                .with(|ctx| {
                    let globals = ctx.globals();
                    if globals.contains_key(name)? {
                        return Err(rquickjs::Error::Unknown);
                    }
                    globals.set(name, rquickjs::Function::new(ctx.clone(), callback)?)
                })
                .map_err(|_| VmError::Initialization)
        }

        pub fn new(limits: ResourceLimits) -> Result<Self, VmError> {
            if !limits.is_valid() {
                return Err(VmError::InvalidLimits);
            }
            let runtime = rquickjs::Runtime::new().map_err(|_| VmError::Initialization)?;
            runtime.set_memory_limit(limits.memory_bytes);
            runtime.set_max_stack_size(limits.stack_bytes);
            let deadline: Rc<Cell<Option<Instant>>> = Rc::new(Cell::new(None));
            let observed = deadline.clone();
            runtime.set_interrupt_handler(Some(Box::new(move || {
                observed
                    .get()
                    .is_some_and(|deadline| Instant::now() >= deadline)
            })));
            let context = Self::with_stack(limits, || rquickjs::Context::full(&runtime))
                .map_err(|_| VmError::Initialization)?;
            Ok(Self {
                context,
                runtime,
                deadline,
                limits,
            })
        }

        pub fn eval_string(&self, source: &str) -> Result<String, VmError> {
            self.eval_with_budget(source, self.limits.turn_milliseconds)
        }

        pub(crate) fn eval_startup(&self, source: &str) -> Result<String, VmError> {
            self.eval_with_budget(source, self.limits.load_milliseconds)
        }

        fn eval_with_budget(&self, source: &str, milliseconds: u64) -> Result<String, VmError> {
            if source.len() > 1024 * 1024 {
                return Err(VmError::SourceLimit);
            }
            self.deadline
                .set(Some(Instant::now() + Duration::from_millis(milliseconds)));
            let result = Self::with_stack(self.limits, || {
                self.context.with(|context| {
                    Self::refresh_stack(&context);
                    context.eval::<String, _>(source)
                })
            });
            let expired = self
                .deadline
                .get()
                .is_some_and(|deadline| Instant::now() >= deadline);
            self.deadline.set(None);
            if expired {
                return Err(VmError::Timeout);
            }
            result.map_err(|_| {
                if expired {
                    VmError::Timeout
                } else {
                    VmError::Execution
                }
            })
        }

        pub fn drain_jobs(&self) -> Result<usize, VmError> {
            self.deadline.set(Some(
                Instant::now() + Duration::from_millis(self.limits.turn_milliseconds),
            ));
            let result = (|| {
                let mut count = 0;
                while self.runtime.is_job_pending() {
                    if count >= self.limits.jobs_per_turn {
                        return Err(VmError::JobLimit);
                    }
                    if self
                        .deadline
                        .get()
                        .is_some_and(|deadline| Instant::now() >= deadline)
                    {
                        return Err(VmError::Timeout);
                    }
                    Self::with_stack(self.limits, || {
                        // The job entry also needs the current segment's top;
                        // non-parallel rquickjs otherwise keeps the previous one.
                        self.context.with(|context| Self::refresh_stack(&context));
                        self.runtime.execute_pending_job()
                    })
                    .map_err(|_| VmError::Execution)?;
                    count += 1;
                }
                Ok(count)
            })();
            let expired = self.deadline.get().is_some_and(|end| Instant::now() >= end);
            self.deadline.set(None);
            if expired {
                Err(VmError::Timeout)
            } else {
                result
            }
        }
    }
}
