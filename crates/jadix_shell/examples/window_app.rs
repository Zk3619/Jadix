//! Run a development SDK/TSX bundle in the ordinary GPU window runtime.
//! Production hosts should load ScriptApplication::from_package instead.
use jadix_shell::{application::ScriptApplication, gpu::GpuApplication, wire::ComponentRegistry};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::io::Read;
    let registry = ComponentRegistry::standard();
    let mut path = None;
    let mut retained = false;
    let mut benchmark_rows = None;
    let mut sparse = false;
    let mut receipt = None;
    let mut size = [420.0_f64, 280.0_f64];
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--retained" => retained = true,
            "--sparse" => sparse = true,
            "--benchmark-rows" => {
                let rows = args.next().ok_or("row count required")?.parse::<usize>()?;
                if !(1..=3072).contains(&rows) {
                    return Err("row limit".into());
                }
                benchmark_rows = Some(rows);
            }
            "--receipt" => {
                receipt = Some(std::path::PathBuf::from(
                    args.next().ok_or("receipt path required")?,
                ))
            }
            "--width" | "--height" => {
                let value = args
                    .next()
                    .ok_or("window dimension required")?
                    .parse::<f64>()?;
                if !(64.0..=4096.0).contains(&value) {
                    return Err("window dimension out of range".into());
                }
                size[usize::from(arg == "--height")] = value;
            }
            "--bundle" => path = Some(args.next().ok_or("bundle path required")?),
            _ => return Err("use --bundle PATH and/or --receipt PATH".into()),
        }
    }
    let source = if let Some(path) = path {
        let mut bytes = vec![];
        std::fs::File::open(path)?
            .take(1024 * 1024 + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() > 1024 * 1024 {
            return Err("bundle too large".into());
        }
        String::from_utf8(bytes)?
    } else {
        include_str!("../tests/fixtures/application.js").into()
    };
    let source = if let Some(rows) = benchmark_rows {
        format!("globalThis.scaleRows={rows};globalThis.leafPatches={sparse};\n{source}")
    } else {
        source
    };
    let limits = if benchmark_rows.is_some() {
        jadix_shell::runtime::ResourceLimits {
            turn_milliseconds: 500,
            memory_bytes: 128 * 1024 * 1024,
            ..Default::default()
        }
    } else {
        Default::default()
    };
    let app =
        ScriptApplication::development("jadix.window.development", &source, registry, limits)?;
    let mut gpu = GpuApplication::new(app)?;
    let mut builds = 0u64;
    let mut preedit_seen = false;
    let mut updates = Vec::new();
    let options = jadix::app::WindowOptions {
        title: "Jadix Shell SDK GPU".into(),
        inner_width: Some(size[0]),
        inner_height: Some(size[1]),
        window_label: None,
        ..Default::default()
    };
    if retained {
        let gpu = std::rc::Rc::new(std::cell::RefCell::new(gpu));
        let history = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let preedit_seen = std::cell::Cell::new(false);
        let observer = std::rc::Rc::new(
            move |update: jadix_shell::gpu::WindowUpdate, app: &GpuApplication| {
                let mut history = history.borrow_mut();
                history.push(update);
                if let Some(path) = &receipt {
                    let content: Vec<_> = app
                        .application()
                        .roots()
                        .iter()
                        .flat_map(|r| {
                            r.children.iter().take(if benchmark_rows.is_some() {
                                3
                            } else {
                                usize::MAX
                            })
                        })
                        .map(|n| format!("{:?}", n.descriptor.content))
                        .collect();
                    let observations:Vec<_> = app.edit_observations().map(|(id,edit)|{
                        preedit_seen.set(preedit_seen.get() || edit.composing.is_some());
                        serde_json::json!({"node":id.generation(),"value":edit.value,"selection":edit.selection,"composing":edit.composing})
                    }).collect();
                    std::fs::write(path,serde_json::json!({"updates":*history,"content":content,"observations":observations,"preedit_seen":preedit_seen.get(),
                    "work":app.application().work(),"runtime":"real jadix::run_with retained bindings",
                    "bind_misses":jadix::app::reactive::bind_miss_count()}).to_string()).unwrap();
                }
            },
        );
        jadix::run_with(options, move || {
            GpuApplication::retained_window_element(gpu.clone(), observer.clone())
        });
        return Ok(());
    }
    jadix::run_with(options, move || {
        let started = std::time::Instant::now();
        let element = gpu.element().expect("development application failed");
        updates.push(jadix_shell::gpu::WindowUpdate {
            full_build: true,
            patched_nodes: 0,
            elapsed_ms: started.elapsed().as_secs_f64() * 1000.,
        });
        builds += 1;
        if let Some(path) = &receipt {
            let descriptions: Vec<_> = gpu
                .application()
                .roots()
                .iter()
                .flat_map(|r| {
                    r.children.iter().take(if benchmark_rows.is_some() {
                        3
                    } else {
                        usize::MAX
                    })
                })
                .map(|n| format!("{:?}", n.descriptor.content))
                .collect();
            let observations:Vec<_>=gpu.edit_observations().map(|(id,edit)| {
                preedit_seen |= edit.composing.is_some();
                serde_json::json!({"node":id.generation(),"value":edit.value,"selection":edit.selection,"composing":edit.composing})
            }).collect();
            std::fs::write(path,serde_json::json!({"builds":builds,"updates":updates,"work":gpu.application().work(),"content":descriptions,"observations":observations,"preedit_seen":preedit_seen,"runtime":"jadix::run_with","vm":"existing BudgetedRuntime QuickJS"}).to_string()).expect("receipt write failed");
        }
        element
    });
    Ok(())
}
