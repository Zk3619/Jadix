use super::*;

#[test]
fn string_buffer_replace_insert_delete() {
    let mut b = StringBuffer::from("hello");
    b.replace(5..5, "!");
    assert_eq!(b.as_str(), "hello!");
    b.replace(0..1, "");
    assert_eq!(b.as_str(), "ello!");
    b.replace(0..4, "hi");
    assert_eq!(b.as_str(), "hi!");
    assert_eq!(b.slice(0..2), "hi");
    assert_eq!(b.text().as_ref(), "hi!");
}

#[test]
fn string_impl_matches_buffer() {
    let mut s = String::from("ab");
    s.replace(1..1, "X");
    assert_eq!(s, "aXb");
}

/// Mid-buffer insert: `String` / [`StringBuffer`] vs `ropey::Rope`.
/// Clone sits outside the timed window. ASCII so rope char index == byte index.
///
/// ```text
/// cargo test -p jadix --lib --ignored text_buffer_mid_insert -- --nocapture
/// ```
#[test]
#[ignore]
fn text_buffer_mid_insert() {
    use std::hint::black_box;
    use std::time::Instant;

    use ropey::Rope;

    fn median_ns(samples: &[u128]) -> u128 {
        let mut v = samples.to_vec();
        v.sort_unstable();
        v[v.len() / 2]
    }

    println!(
        "text_buffer_mid_insert  host={}  profile=lib-test(debug unless --release)",
        std::env::consts::OS
    );
    println!(
        "{:<8} {:<6} {:<22} {:>12}",
        "size", "pay", "impl", "median_ns"
    );

    for &(size, iters) in &[(1usize << 20, 7usize), (10 << 20, 5), (50 << 20, 3)] {
        let base = "x".repeat(size);
        let mid = size / 2;
        let kb = "y".repeat(1024);
        for payload in ["y", kb.as_str()] {
            let pay_label = if payload.len() == 1 { "1B" } else { "1KB" };

            let mut string_ns = Vec::with_capacity(iters);
            for _ in 0..iters {
                let mut s = base.clone();
                let t0 = Instant::now();
                s.insert_str(mid, payload);
                string_ns.push(t0.elapsed().as_nanos());
                black_box(s);
            }

            let mut buf_ns = Vec::with_capacity(iters);
            for _ in 0..iters {
                let mut b = StringBuffer::from_string(base.clone());
                let t0 = Instant::now();
                b.replace(mid..mid, payload);
                buf_ns.push(t0.elapsed().as_nanos());
                black_box(b);
            }

            let rope_src = Rope::from_str(&base);
            let mut rope_ns = Vec::with_capacity(iters);
            for _ in 0..iters {
                let mut r = rope_src.clone();
                let t0 = Instant::now();
                r.insert(mid, payload);
                rope_ns.push(t0.elapsed().as_nanos());
                black_box(r);
            }

            let row = |name: &str, samples: &[u128]| {
                println!(
                    "{:<8} {:<6} {:<22} {:>12}",
                    format!("{}MB", size / (1 << 20)),
                    pay_label,
                    name,
                    median_ns(samples)
                );
            };
            row("String::insert_str", &string_ns);
            row("StringBuffer::replace", &buf_ns);
            row("Rope::insert", &rope_ns);
        }
    }
}
