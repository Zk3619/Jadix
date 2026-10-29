use jadix::core::tree::{Element, TemplateDescription};
use std::time::Instant;

fn builder() -> Element {
    Element::new().child(Element::new().text("baseline"))
}

fn registered() -> Element {
    builder()
        .template_id("explicit-baseline")
        .hot_reload_description(TemplateDescription::new("baseline", "declarations"))
}

#[cfg(feature = "macros")]
fn static_ui() -> Element {
    jadix::ui! { <Box>"baseline"</Box> }
}

fn sample(name: &str, construct: fn() -> Element) {
    for _ in 0..128 {
        std::hint::black_box(construct());
    }
    let mut samples = Vec::new();
    for _ in 0..31 {
        let started = Instant::now();
        for _ in 0..128 {
            std::hint::black_box(construct());
        }
        samples.push(started.elapsed().as_nanos() as u64);
    }
    samples.sort_unstable();
    let median = samples[15];
    let p95 = samples[29];
    let spread = if median == 0 {
        0.0
    } else {
        (p95 as f64 / median as f64 - 1.0) * 100.0
    };
    println!("HR0 {name}: batches=31 warmup=128 trees_per_batch=128 median_ns={median} p95_ns={p95} p95_spread_percent={spread:.2}");
}

#[test]
#[ignore = "serial construction baseline; no native frame or Release performance claim"]
fn hot_reload_three_entry_construction_baseline() {
    assert_eq!(builder().children[0].text.as_deref(), Some("baseline"));
    assert_eq!(registered().children[0].text.as_deref(), Some("baseline"));
    sample("builder-unregistered", builder);
    sample("builder-registered", registered);
    #[cfg(feature = "macros")]
    {
        assert_eq!(static_ui().children[0].text.as_deref(), Some("baseline"));
        sample("static-ui", static_ui);
    }
}
