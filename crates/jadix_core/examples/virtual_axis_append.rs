//! Compare keyed append with full replacement for the same growing list.
//! Run: cargo run --locked --release -p jadix_core --example virtual_axis_append
use jadix_core::virtualization::VirtualAxis;
use std::{hint::black_box, time::Instant};

fn median(values: &mut [u128]) -> u128 {
    values.sort_unstable();
    values[values.len() / 2]
}

fn main() {
    println!("storage,existing,added,append_ns,replacement_ns,replacement_over_append");
    for prepared in [false, true] {
        for count in [1_024u64, 16_384, 131_072] {
            for added in [1u64, 32] {
                let mut appended = Vec::new();
                let mut replaced = Vec::new();
                let end_before = count + u64::from(prepared);
                for round in 0..13 {
                    let mut axis = VirtualAxis::new(0..count, 24.0).unwrap();
                    let mut reference = VirtualAxis::new(0..count, 24.0).unwrap();
                    axis.measure(&7, 41.0).unwrap();
                    reference.measure(&7, 41.0).unwrap();
                    // Both sides receive the same preceding row. This exercises
                    // append after its first storage growth, without timing setup.
                    if prepared {
                        axis.append([(count, 24.0)]).unwrap();
                        reference.append([(count, 24.0)]).unwrap();
                    }
                    let append_run = |axis: &mut VirtualAxis<u64>| {
                        let start = Instant::now();
                        axis.append((end_before..end_before + added).map(|key| (key, 24.0)))
                            .unwrap();
                        black_box(axis.total());
                        start.elapsed().as_nanos()
                    };
                    let replace_run = |axis: &mut VirtualAxis<u64>| {
                        let start = Instant::now();
                        axis.replace(0..end_before + added, 24.0).unwrap();
                        black_box(axis.total());
                        start.elapsed().as_nanos()
                    };
                    let (a, b) = if round % 2 == 0 {
                        (append_run(&mut axis), replace_run(&mut reference))
                    } else {
                        let b = replace_run(&mut reference);
                        (append_run(&mut axis), b)
                    };
                    assert_eq!(axis.len(), reference.len());
                    assert_eq!(axis.total(), reference.total());
                    for index in 0..=(end_before + added) as usize {
                        assert_eq!(axis.position(index), reference.position(index));
                    }
                    for offset in [0.0, 7.0 * 24.0, count as f64 * 24.0] {
                        assert_eq!(
                            axis.window(offset, 240.0, 0.0, &[], 12).unwrap(),
                            reference.window(offset, 240.0, 0.0, &[], 12).unwrap()
                        );
                    }
                    if round > 1 {
                        appended.push(a);
                        replaced.push(b);
                    }
                }
                let a = median(&mut appended);
                let b = median(&mut replaced);
                let storage = if prepared {
                    "after_first_append"
                } else {
                    "first_append"
                };
                println!(
                    "{storage},{count},{added},{a},{b},{:.2}",
                    b as f64 / a.max(1) as f64
                );
            }
        }
    }
}
