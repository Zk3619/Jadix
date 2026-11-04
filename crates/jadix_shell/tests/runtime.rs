use jadix_shell::{runtime::*, Session, ShellError};

#[test]
fn quotas_have_bounded_configuration_and_zero_admission() {
    assert!(ResourceLimits::default().is_valid());
    assert!(!ResourceLimits {
        memory_bytes: usize::MAX,
        ..Default::default()
    }
    .is_valid());
    assert!(matches!(
        Session::with_limits(ResourceLimits {
            handles: 4097,
            ..Default::default()
        }),
        Err(ShellError::InvalidLimits)
    ));
}

#[cfg(all(feature = "native-vm", not(target_arch = "wasm32")))]
#[test]
fn selected_vm_runs_on_ui_thread_and_interrupts_unbounded_script() {
    use native::*;
    let runtime = BudgetedRuntime::new(ResourceLimits::default()).unwrap();
    assert_eq!(
        runtime
            .eval_string("JSON.stringify({text: '中文😀', number: 42})")
            .unwrap(),
        "{\"text\":\"中文😀\",\"number\":42}"
    );
    assert_eq!(
        runtime.eval_string("while (true) {}"),
        Err(VmError::Timeout)
    );
    assert_eq!(runtime.eval_string("'recovered'"), Ok("recovered".into()));
    assert_eq!(
        runtime.eval_string("typeof process + ':' + typeof document + ':' + typeof fetch"),
        Ok("undefined:undefined:undefined".into())
    );
}

#[cfg(all(feature = "native-vm", not(target_arch = "wasm32")))]
#[test]
fn microtask_self_scheduling_cannot_drain_without_a_bound() {
    use native::*;
    let runtime = BudgetedRuntime::new(ResourceLimits {
        jobs_per_turn: 8,
        ..Default::default()
    })
    .unwrap();
    runtime
        .eval_string("function again() { Promise.resolve().then(again) }; again(); 'queued'")
        .unwrap();
    assert_eq!(runtime.drain_jobs(), Err(VmError::JobLimit));
}

#[cfg(all(feature = "native-vm", not(target_arch = "wasm32")))]
#[test]
fn memory_and_stack_limits_reject_exhaustion_and_allow_recovery() {
    use native::*;
    let runtime = BudgetedRuntime::new(ResourceLimits {
        memory_bytes: 1024 * 1024,
        stack_bytes: 64 * 1024,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(
        runtime.eval_string("new ArrayBuffer(8 * 1024 * 1024); 'allocated'"),
        Err(VmError::Execution)
    );
    assert_eq!(
        runtime.eval_string("function recurse() { return 1 + recurse() }; recurse(); 'returned'"),
        Err(VmError::Execution)
    );
    assert_eq!(runtime.eval_string("'recovered'"), Ok("recovered".into()));
}
