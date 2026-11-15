use jadix_components::{authoring::native::ElementProjection, prelude::*};
use jadix_core::reactivity::create_root;
use jadix_native::{host::NativeRoot, presentation::*, windows::WindowsHost};
use std::io::Write;

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    let probe = match arguments.as_slice() {
        [] => None,
        [mode, nonce, output] if mode == "--probe" && !nonce.is_empty() && nonce.len() <= 128 => Some((nonce, output)),
        _ => return Err("Usage: @NAME@ [--probe NONCE NEW_REPORT_JSON]".into()),
    };
    let host = WindowsHost::new("@NAME@", [640, 480])?;
    let mut root = NativeRoot::new(host);
    let mut projection = ElementProjection::with_lifetime(root.model().lifetime());
    let (owner, (count, mut element)) = create_root(|| {
        let count = create_signal(0usize);
        let button: Element = button("increment")
            .child(text("Continue"))
            .on_click(move || count.set(count.get().saturating_add(1)))
            .into();
        let element = vstack()
            .p(16.0)
            .gap(12.0)
            .child(jadix_components::app::reactive::bind_text("count", move || format!("Count: {}", count.get())).h(32.0))
            .child(button.h(44.0));
        (count, element)
    });
    let mut nodes = vec![projection.snapshot(&mut element).map_err(|error| format!("{error:?}"))?];
    root.present(&mut nodes, root.host().viewport(), root.host().environment())?;
    if let Some((nonce, output)) = probe {
        let mut pending = root.model().roots().to_vec();
        let mut button = None;
        while let Some(identifier) = pending.pop() {
            let node = root.model().node(identifier).ok_or("missing native node")?;
            if matches!(node.descriptor.content, NodeContent::Button { .. }) {
                button = Some(identifier);
            }
            pending.extend(&node.children);
        }
        root.host().activate(button.ok_or("missing button")?)?;
        for event in root.poll_events()? {
            if event.payload == EventPayload::Activate {
                projection.dispatch(&mut element, &event).map_err(|error| format!("{error:?}"))?;
            }
        }
        if count.get() != 1 {
            return Err("native callback did not update the Rust signal".into());
        }
        let mut nodes = vec![projection.snapshot(&mut element).map_err(|error| format!("{error:?}"))?];
        root.present(&mut nodes, root.host().viewport(), root.host().environment())?;
        let report = serde_json::json!({"schema": 1, "nonce": nonce, "native_controls": true,
            "count": count.get(), "synthetic_native_messages": true, "native": root.host().diagnostics(),
            "assistive_technology_accepted": false});
        let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(output)?;
        writeln!(file, "{}", serde_json::to_string_pretty(&report)?)?;
    } else {
        root.host().show();
        let mut environment = 0;
        loop {
            root.host().pump();
            if root.host().is_closed() {
                break;
            }
            let mut changed = false;
            for event in root.poll_events()? {
                if event.payload == EventPayload::Activate {
                    projection.dispatch(&mut element, &event).map_err(|error| format!("{error:?}"))?;
                    changed = true;
                }
            }
            let next = root.host().environment();
            if changed || environment != next {
                environment = next;
                let mut nodes = vec![projection.snapshot(&mut element).map_err(|error| format!("{error:?}"))?];
                root.present(&mut nodes, root.host().viewport(), environment)?;
            }
            root.host().wait_for_events(u32::MAX)?;
        }
    }
    root.dispose();
    projection.dispose();
    drop(owner);
    Ok(())
}
