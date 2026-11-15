use jadix_shell::{runtime::*, Session, ShellError};

#[test]
fn quotas_have_bounded_configuration_and_zero_admission() {
    assert!(ResourceLimits::default().is_valid());
    for load_milliseconds in [0, 10_001, u64::MAX] {
        assert!(!ResourceLimits {
            load_milliseconds,
            ..Default::default()
        }
        .is_valid());
    }
    assert!(ResourceLimits {
        load_milliseconds: 10_000,
        ..Default::default()
    }
    .is_valid());
    assert_eq!(ResourceLimits::default().turn_milliseconds, 20);
    assert_eq!(ResourceLimits::default().load_milliseconds, 1000);

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
#[cfg(all(feature = "native-vm", not(target_arch = "wasm32")))]
#[test]
fn host_callback_overrun_is_checked_even_after_the_final_microtask() {
    use native::*;
    let runtime = BudgetedRuntime::new(ResourceLimits {
        turn_milliseconds: 5,
        ..Default::default()
    })
    .unwrap();
    runtime
        .bind_json("slow", |_| {
            std::thread::sleep(std::time::Duration::from_millis(25));
            String::new()
        })
        .unwrap();
    runtime
        .eval_string("Promise.resolve().then(()=>slow('')); ''")
        .unwrap();
    assert_eq!(runtime.drain_jobs(), Err(VmError::Timeout));
    assert_eq!(runtime.eval_string("'recovered'"), Ok("recovered".into()));
}

#[cfg(all(feature = "native-vm", not(target_arch = "wasm32")))]
#[test]
fn maximum_vm_stack_is_scoped_on_a_small_thread_and_recovers() {
    // Execute in a child process: native stack exhaustion must never kill the
    // test runner or be mistaken for an ordinary QuickJS exception.
    if std::env::var_os("JADIX_VM_STACK_CHILD").is_none() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "maximum_vm_stack_is_scoped_on_a_small_thread_and_recovers",
                "--nocapture",
            ])
            .env("JADIX_VM_STACK_CHILD", "1")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("SCOPED_VM_STACK_PASS"));
        return;
    }
    std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(|| {
            use native::*;
            for _ in 0..12 {
                eprintln!("stack probe: new");
                let runtime = BudgetedRuntime::new(ResourceLimits {
                    stack_bytes: 2 * 1024 * 1024,
                    turn_milliseconds: 500,
                    ..Default::default()
                })
                .unwrap();
                eprintln!("stack probe: eval");
                assert_eq!(
                    runtime.eval_string("function recurse(){return 1+recurse()} recurse(); ''"),
                    Err(VmError::Execution)
                );
                assert_eq!(runtime.eval_string("'recovered'"), Ok("recovered".into()));
                runtime
                    .eval_string("globalThis.stackReason=''; Promise.resolve().then(()=>recurse()).catch(e=>{stackReason=String(e)}); ''")
                    .unwrap();
                eprintln!("stack probe: drain");
                assert_eq!(runtime.drain_jobs(), Ok(2));
                assert_eq!(runtime.eval_string("String(stackReason.includes('stack'))"),Ok("true".into()));
                assert_eq!(
                    runtime.eval_string("'jobs recovered'"),
                    Ok("jobs recovered".into())
                );
                eprintln!("stack probe: drop");
                drop(runtime);
            }
        })
        .unwrap()
        .join()
        .unwrap();
    println!("SCOPED_VM_STACK_PASS");
}
