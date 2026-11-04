#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResourceLimits {
    pub requests: usize,
    pub mounts: usize,
    pub handles: usize,
    pub memory_bytes: usize,
    pub stack_bytes: usize,
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
            let context = rquickjs::Context::full(&runtime).map_err(|_| VmError::Initialization)?;
            Ok(Self {
                context,
                runtime,
                deadline,
                limits,
            })
        }

        pub fn eval_string(&self, source: &str) -> Result<String, VmError> {
            if source.len() > 1024 * 1024 {
                return Err(VmError::SourceLimit);
            }
            self.deadline.set(Some(
                Instant::now() + Duration::from_millis(self.limits.turn_milliseconds),
            ));
            let result = self
                .context
                .with(|context| context.eval::<String, _>(source));
            let expired = self
                .deadline
                .get()
                .is_some_and(|deadline| Instant::now() >= deadline);
            self.deadline.set(None);
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
                    self.runtime
                        .execute_pending_job()
                        .map_err(|_| VmError::Execution)?;
                    count += 1;
                }
                Ok(count)
            })();
            self.deadline.set(None);
            result
        }
    }
}
