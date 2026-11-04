use jadix_shell::{runtime::ResourceLimits, schema::Schema, *};
use std::{collections::BTreeSet, time::Instant};

fn main() {
    let mut session = Session::with_limits(ResourceLimits::default()).unwrap();
    session
        .verify(PackageManifest {
            package: "example.baseline".into(),
            protocol_version: 1,
            requested_permissions: BTreeSet::new(),
        })
        .unwrap();
    session.authorize(&BTreeSet::new()).unwrap();
    session.load(|| Ok(())).unwrap();
    let root = session.mount_root().unwrap();
    let started = Instant::now();
    for _ in 0..30 {
        let handles: Vec<_> = (0..1024)
            .map(|_| {
                session
                    .export_event(root.id(), Schema::Null, |_| {})
                    .unwrap()
            })
            .collect();
        assert_eq!(
            session.export_event(root.id(), Schema::Null, |_| {}),
            Err(ShellError::HandleLimit)
        );
        for handle in handles {
            session.release_handle(&handle).unwrap();
        }
    }
    let handles_ms = started.elapsed().as_secs_f64() * 1000.0;
    #[cfg(all(feature = "native-vm", not(target_arch = "wasm32")))]
    {
        let vm = runtime::native::BudgetedRuntime::new(ResourceLimits::default()).unwrap();
        let mut samples = vec![];
        for iteration in 0..35 {
            let started = Instant::now();
            vm.eval_string("JSON.stringify({text:'中文😀', count:42})")
                .unwrap();
            vm.drain_jobs().unwrap();
            if iteration >= 5 {
                samples.push(started.elapsed().as_secs_f64() * 1000.0);
            }
        }
        samples.sort_by(f64::total_cmp);
        println!(
            "{{\"vm_samples\":30,\"eval_p50_ms\":{},\"eval_p95_ms\":{}}}",
            samples[14], samples[28]
        );
    }
    println!("{{\"handle_batches\":30,\"handles_per_batch\":1024,\"total_ms\":{handles_ms}}}");
    session.dispose();
}
