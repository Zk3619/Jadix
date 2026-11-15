#![cfg(feature = "gpu-host")]
use jadix_core::presentation::{component::ComponentSchema, NodeContent};
use jadix_shell::{application::ScriptApplication, gpu::GpuApplication, wire::ComponentRegistry};
fn registry() -> ComponentRegistry {
    let mut registry = ComponentRegistry::default();
    let schemas: Vec<ComponentSchema> = serde_json::from_str(include_str!(
        "../../jadix_components/schema/portable-v1.json"
    ))
    .unwrap();
    for schema in schemas.into_iter().chain(
        serde_json::from_str::<Vec<jadix_core::presentation::component::ComponentSchema>>(
            include_str!("../../jadix_components/schema/extended-v1.json"),
        )
        .unwrap(),
    ) {
        registry.register(schema).unwrap();
    }
    registry
}

#[test]
#[ignore = "explicit real-GPU sparse Checkbox projection against full mount"]
fn sdk_control_leaf_projection_matches_full_mount_on_large_business_tree() {
    use jadix::renderer::offscreen::{OffscreenGpu, SceneRenderer};
    let (owner, ()) = jadix_core::create_root(|| {
        let make = |partial| {
            let source = format!(
                "globalThis.leafPatches={partial};globalThis.scaleRows=1024;\n{}",
                include_str!("fixtures/control-leaves.js")
            );
            GpuApplication::new(
                ScriptApplication::development(
                    "tests.gpu.control-leaf",
                    &source,
                    registry(),
                    Default::default(),
                )
                .unwrap(),
            )
            .unwrap()
        };
        let (mut sparse, mut full) = (make(true), make(false));
        let (mut sparse_renderer, mut full_renderer) = (
            SceneRenderer::new(OffscreenGpu::new().expect("GPU")),
            SceneRenderer::new(OffscreenGpu::new().expect("GPU")),
        );
        let (mut sparse_root, mut full_root) = (sparse.element().unwrap(), full.element().unwrap());
        sparse
            .update_root(&mut sparse_renderer, &mut sparse_root)
            .unwrap();
        full.update_root(&mut full_renderer, &mut full_root)
            .unwrap();
        let before = sparse_renderer
            .render(
                &mut sparse_root,
                [320, 240],
                1.,
                [-1.; 2],
                wgpu::Color::BLACK,
            )
            .unwrap();
        click_text(&sparse_root, "[ ] Keep enabled");
        let stats = sparse
            .update_root(&mut sparse_renderer, &mut sparse_root)
            .unwrap();
        assert!(!stats.full_projection);
        assert_eq!(stats.visited_nodes, 1);
        assert_eq!(stats.projected_nodes, 1);
        assert_eq!(stats.leaf_patches, 1);
        click_text(&full_root, "[ ] Keep enabled");
        let full_stats = full
            .update_root(&mut full_renderer, &mut full_root)
            .unwrap();
        // A full SDK mount still visits the whole source tree, even when its
        // descriptor diff can reuse the same leaf replacement afterwards.
        assert!(full_stats.visited_nodes > 1024);
        assert_eq!(full.application().work().full_mounts, 2);
        assert_eq!(sparse.application().work().full_mounts, 1);
        let (actual, expected) = (
            sparse_renderer
                .render(
                    &mut sparse_root,
                    [320, 240],
                    1.,
                    [-1.; 2],
                    wgpu::Color::BLACK,
                )
                .unwrap(),
            full_renderer
                .render(&mut full_root, [320, 240], 1., [-1.; 2], wgpu::Color::BLACK)
                .unwrap(),
        );
        assert_eq!(actual.pixels, expected.pixels);
        assert_ne!(actual.pixels, before.pixels);
        assert!(!expected.pixels.is_empty());
        sparse.dispose();
        full.dispose();
    });
    owner.dispose();
}

#[test]
#[ignore = "explicit real-GPU scaling; serial same-output comparison"]
fn sdk_gpu_projection_scaling_matches_full_output() {
    scaling(false);
}

#[test]
#[ignore = "explicit real-GPU sparse SDK protocol scaling"]
fn sdk_sparse_protocol_scaling_matches_full_output() {
    scaling(true);
}

fn scaling(sparse: bool) {
    use jadix::renderer::offscreen::{OffscreenGpu, SceneRenderer};
    let (owner, ()) = jadix_core::create_root(|| {
        let mut rows_report = Vec::new();
        for rows in [32, 256, 1024, 3072] {
            let source = format!(
                "globalThis.scaleRows={rows};\n{}",
                include_str!("fixtures/scaling.js")
            );
            let make = |delta| {
                GpuApplication::new(
                    ScriptApplication::development(
                        "tests.scaling",
                        &format!("globalThis.leafPatches={delta};\n{source}"),
                        registry(),
                        jadix_shell::runtime::ResourceLimits {
                            turn_milliseconds: 500,
                            memory_bytes: 128 * 1024 * 1024,
                            ..Default::default()
                        },
                    )
                    .unwrap(),
                )
                .unwrap()
            };
            let (mut full, mut patch) = (make(false), make(sparse));
            let mut a = SceneRenderer::new(OffscreenGpu::new().expect("real GPU"));
            let mut b = SceneRenderer::new(OffscreenGpu::new().expect("real GPU"));
            let (mut ar, mut br) = (full.element().unwrap(), patch.element().unwrap());
            a.render(&mut ar, [320, 240], 1., [-1.; 2], wgpu::Color::BLACK)
                .unwrap();
            b.render(&mut br, [320, 240], 1., [-1.; 2], wgpu::Color::BLACK)
                .unwrap();
            patch.update_root(&mut b, &mut br).unwrap(); // untimed index warmup
            assert_eq!(
                patch.update_root(&mut b, &mut br).unwrap().projected_nodes,
                0
            );
            // Prime paint fingerprints after that full reconciliation. Give
            // the baseline the same number of untimed render/readback frames.
            a.render(&mut ar, [320, 240], 1., [-1.; 2], wgpu::Color::BLACK)
                .unwrap();
            b.render(&mut br, [320, 240], 1., [-1.; 2], wgpu::Color::BLACK)
                .unwrap();
            let retained = br.children[0].children[1].layout_node_id();
            for round in 0..6 {
                let before_full = full.application().work();
                let before_patch = patch.application().work();
                click_text(&ar, "Advance");
                click_text(&br, "Advance");
                let mut run_full = || {
                    let start = std::time::Instant::now();
                    a.replace_root(&mut ar, full.element().unwrap());
                    let projection = start.elapsed().as_secs_f64() * 1000.;
                    let image = a
                        .render(&mut ar, [320, 240], 1., [-1.; 2], wgpu::Color::BLACK)
                        .unwrap();
                    (image, start.elapsed().as_secs_f64() * 1000., projection)
                };
                let mut work = None;
                let mut run_patch = || {
                    let start = std::time::Instant::now();
                    work = Some(patch.update_root(&mut b, &mut br).unwrap());
                    let projection = start.elapsed().as_secs_f64() * 1000.;
                    let image = b
                        .render(&mut br, [320, 240], 1., [-1.; 2], wgpu::Color::BLACK)
                        .unwrap();
                    (image, start.elapsed().as_secs_f64() * 1000., projection)
                };
                let (left, right) = if round % 2 == 0 {
                    (run_full(), run_patch())
                } else {
                    let r = run_patch();
                    (run_full(), r)
                };
                assert_eq!(
                    left.0.pixels, right.0.pixels,
                    "same output rows={rows} round={round}"
                );
                assert_eq!(br.children[0].children[1].layout_node_id(), retained);
                let work = work.unwrap();
                assert!(!work.full_projection);
                assert_eq!(work.projected_nodes, 1);
                assert_eq!(work.visited_nodes, if sparse { 1 } else { rows + 2 });
                assert!(b.last_frame_stats().unwrap().collect_replayed > 0);
                rows_report.push(serde_json::json!({"rows":rows,"round":round,"first":if round%2==0{"full"}else{"patch"},
                    "full_total_ms":left.1,"patch_total_ms":right.1,"full_projection_reconcile_ms":left.2,
                    "patch_projection_reconcile_ms":right.2,"projected_nodes":work.projected_nodes,"admission_nodes":work.visited_nodes,
                    "full_collect_ms":a.last_frame_stats().unwrap().layout.collect_ms,"patch_collect_ms":b.last_frame_stats().unwrap().layout.collect_ms,
                    "full_fingerprint_ms":a.last_frame_stats().unwrap().layout.collect_stamp_ms,
                    "patch_fingerprint_ms":b.last_frame_stats().unwrap().layout.collect_stamp_ms,
                    "patch_collect_replayed":b.last_frame_stats().unwrap().collect_replayed,
                    "sparse_protocol":sparse,"full_message_bytes":full.application().work().message_bytes-before_full.message_bytes,
                    "patch_message_bytes":patch.application().work().message_bytes-before_patch.message_bytes}));
            }
            if rows == 32 {
                // A pending replay request cannot survive invalid output, resize,
                // or a scale change. Each recovery matches full collection.
                patch.update_root(&mut b, &mut br).unwrap();
                assert!(b
                    .render(&mut br, [0, 240], 1., [-1.; 2], wgpu::Color::BLACK)
                    .is_err());
                assert!(b.last_frame_stats().is_none());
                for (size, scale) in [([320, 240], 1.), ([360, 260], 1.), ([360, 260], 1.25)] {
                    assert_eq!(
                        patch.update_root(&mut b, &mut br).unwrap().projected_nodes,
                        0
                    );
                    let expected = a
                        .render(&mut ar, size, scale, [-1.; 2], wgpu::Color::BLACK)
                        .unwrap();
                    let actual = b
                        .render(&mut br, size, scale, [-1.; 2], wgpu::Color::BLACK)
                        .unwrap();
                    assert_eq!(actual.pixels, expected.pixels);
                    assert_eq!(b.last_frame_stats().unwrap().collect_replayed, 0);
                }
            }
        }
        let directory = std::path::PathBuf::from(
            std::env::var_os("JADIX_GPU_ACCEPTANCE_DIR").expect("evidence directory"),
        );
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(
            directory.join(if sparse {
                "sparse-scaling.json"
            } else {
                "scaling.json"
            }),
            serde_json::to_vec_pretty(&rows_report).unwrap(),
        )
        .unwrap();
    });
    owner.dispose();
}

#[test]
fn sdk_structural_gpu_updates_retain_layout_identity_and_match_fresh_pixels() {
    use jadix::renderer::offscreen::{OffscreenGpu, SceneRenderer};
    let (owner, ()) = jadix_core::create_root(|| {
        let mut app = GpuApplication::new(
            ScriptApplication::development(
                "tests.gpu.retained",
                include_str!("fixtures/structural.js"),
                registry(),
                Default::default(),
            )
            .unwrap(),
        )
        .unwrap();
        let mut renderer = SceneRenderer::new(OffscreenGpu::new().expect("real GPU adapter"));
        let mut oracle = SceneRenderer::new(OffscreenGpu::new().expect("oracle GPU adapter"));
        let mut root = app.element().unwrap();
        renderer
            .render(&mut root, [320, 160], 1., [-1.; 2], wgpu::Color::BLACK)
            .unwrap();
        let root_id = root.layout_node_id().unwrap();
        let a = root.children[0].children[1].layout_node_id().unwrap();
        let b = root.children[0].children[2].layout_node_id().unwrap();
        let mut samples = Vec::new();
        for phase in 1..=12 {
            click_text(&root, "Advance");
            let started = std::time::Instant::now();
            let work = app.update_root(&mut renderer, &mut root).unwrap();
            assert!(work.full_projection, "reorder/insert/remove must fall back");
            let update_ms = started.elapsed().as_secs_f64() * 1000.;
            // The oracle is independently projected and rendered without old layout IDs.
            let mut fresh = app.element().unwrap();
            let expected = oracle
                .render(&mut fresh, [320, 160], 1., [-1.; 2], wgpu::Color::BLACK)
                .unwrap();
            let actual = renderer
                .render(&mut root, [320, 160], 1., [-1.; 2], wgpu::Color::BLACK)
                .unwrap();
            assert_eq!(root.layout_node_id(), Some(root_id));
            assert_eq!(
                root.children[0].children[1].layout_node_id(),
                Some(if phase % 2 == 1 { b } else { a })
            );
            assert_eq!(
                root.children[0].children[2].layout_node_id(),
                Some(if phase % 2 == 1 { a } else { b })
            );
            assert_eq!(actual.pixels, expected.pixels, "phase {phase}");
            let stats = renderer.last_frame_stats().unwrap();
            assert_eq!(stats.frame, phase + 1);
            assert!(
                stats.resident.taffy_nodes <= 8,
                "removed Taffy nodes must be pruned"
            );
            samples.push(serde_json::json!({"phase":phase,"update_ms":update_ms,
                "render_readback_ms":stats.cpu_elapsed_ms,
                "layout_sync_ms":stats.layout.rebuild_ms,"layout_compute_ms":stats.layout.taffy_compute_ms,
                "collect_ms":stats.layout.collect_ms,"resident_nodes":stats.resident.taffy_nodes,
                "rectangles":stats.rectangles,"text_nodes":stats.text_nodes}));
        }
        assert!(renderer
            .render(&mut root, [0, 160], 1., [-1.; 2], wgpu::Color::BLACK)
            .is_err());
        assert!(
            renderer.last_frame_stats().is_none(),
            "failure must not report stale frame metrics"
        );
        assert_eq!(renderer.rendered_frames(), 13);
        if let Some(directory) = std::env::var_os("JADIX_GPU_ACCEPTANCE_DIR") {
            let directory = std::path::PathBuf::from(directory);
            std::fs::create_dir_all(&directory).unwrap();
            std::fs::write(
                directory.join("retained-frames.json"),
                serde_json::to_vec_pretty(&samples).unwrap(),
            )
            .unwrap();
        }
    });
    owner.dispose();
}

#[test]
fn disposed_gpu_application_rejects_late_callbacks_even_after_host_owner_drops() {
    let (owner, (mut app, callback)) = jadix_core::create_root(|| {
        let mut app = GpuApplication::new(
            ScriptApplication::development(
                "tests.gpu.lifetime",
                include_str!("fixtures/structural.js"),
                registry(),
                Default::default(),
            )
            .unwrap(),
        )
        .unwrap();
        let root = app.element().unwrap();
        let callback = root.children[0].children[0].on_click.clone().unwrap();
        (app, callback)
    });
    app.dispose();
    (callback.borrow_mut())();
    assert!(app.element().is_err());
    owner.dispose();
    (callback.borrow_mut())();
    drop(app);
}

#[cfg(feature = "text-undo")]
#[test]
fn retained_sdk_editor_keeps_selection_undo_redo_and_preedit_across_sibling_updates() {
    use jadix::{
        renderer::offscreen::{OffscreenGpu, SceneRenderer},
        utils::{text_edit_history::*, text_field::*},
    };
    let (owner, ()) = jadix_core::create_root(|| {
        let mut app = GpuApplication::new(
            ScriptApplication::development(
                "tests.gpu.editor.retained",
                include_str!("fixtures/application.js"),
                registry(),
                Default::default(),
            )
            .unwrap(),
        )
        .unwrap();
        let mut renderer = SceneRenderer::new(OffscreenGpu::new().expect("real GPU adapter"));
        let mut root = app.element().unwrap();
        renderer
            .render(&mut root, [400, 240], 1., [-1.; 2], wgpu::Color::BLACK)
            .unwrap();
        assert!(
            app.update_root(&mut renderer, &mut root)
                .unwrap()
                .full_projection
        );
        assert_eq!(
            app.update_root(&mut renderer, &mut root)
                .unwrap()
                .projected_nodes,
            0
        );
        let input_id = app.application().node_at(&[2]).unwrap();
        let name = format!("shell:{}:{}", input_id.root.token(), input_id.generation());
        let layout_id = root.children[0].children[2].layout_node_id();
        commit_value_with_policy(&name, "draft 中文😀".into(), SetTextUndoPolicy::Undoable);
        assert!(can_undo(&name));
        assert!(
            app.update_root(&mut renderer, &mut root)
                .unwrap()
                .full_projection
        );
        for _ in 0..4 {
            click_text(&root, "Increment");
            assert_eq!(
                app.update_root(&mut renderer, &mut root)
                    .unwrap()
                    .leaf_patches,
                1
            );
            renderer
                .render(&mut root, [400, 240], 1., [-1.; 2], wgpu::Color::BLACK)
                .unwrap();
            assert_eq!(root.children[0].children[2].layout_node_id(), layout_id);
            assert!(
                can_undo(&name),
                "unrelated SDK render must preserve the live undo hook"
            );
        }
        assert!(undo_field(&name));
        assert!(
            app.update_root(&mut renderer, &mut root)
                .unwrap()
                .full_projection
        );
        assert!(
            matches!(&app.application().roots()[0].children[2].descriptor.content,NodeContent::Input{value,..} if value.is_empty())
        );
        assert!(redo_field(&name));
        assert!(
            app.update_root(&mut renderer, &mut root)
                .unwrap()
                .full_projection
        );
        assert_eq!(
            jadix::authoring::text::observe_edit(&name).value,
            "draft 中文😀"
        );
        jadix_core::use_signal(&input_ime_anchor_signal_key(&name), || None::<usize>).set(Some(0));
        jadix_core::use_signal(&input_preedit_signal_key(&name), String::new).set("输入😀".into());
        jadix_core::use_signal(&input_preedit_sel_signal_key(&name), || {
            None::<(usize, usize)>
        })
        .set(Some((6, 10)));
        let before = jadix::authoring::text::observe_edit(&name);
        assert!(before.composing.is_some());
        click_text(&root, "Increment");
        app.update_root(&mut renderer, &mut root).unwrap();
        let after = jadix::authoring::text::observe_edit(&name);
        assert_eq!(
            after, before,
            "UTF-16 selection and preedit must survive sibling changes"
        );
        drop(root);
        assert!(
            !undo_slot_exists(&name),
            "final unmount must release undo storage"
        );
    });
    owner.dispose();
}

#[test]
fn offscreen_portal_fades_blur_and_rich_ellipsis_produce_real_pixels() {
    use jadix::{core::tree::Element, style::TextSpan};
    let (owner, ()) = jadix_core::create_root(|| {
        let gpu = jadix::renderer::offscreen::OffscreenGpu::new().expect("real GPU adapter");
        let mut renderer = jadix::renderer::offscreen::SceneRenderer::new(gpu);
        let mut render = |root: &mut Element| {
            renderer
                .render(root, [240, 96], 1., [-1.; 2], wgpu::Color::BLACK)
                .unwrap()
        };
        let caption = |mode: &str| {
            Element::new()
                .w(100.)
                .h(32.)
                .text_spans(vec![
                    TextSpan::new("Bold 中文 ").weight(900),
                    TextSpan::new("family 👩‍👩‍👧‍👦 and a long tail").color([255, 80, 40, 255]),
                ])
                .text_size(20.)
                .text_color("#ffffff")
                .text_overflow(mode)
        };
        let clipped = render(&mut caption("clip"));
        let ellipsis = render(&mut caption("ellipsis"));
        assert!(
            clipped.pixels.chunks_exact(4).any(|p| p[0] > 100),
            "fixture must paint visible glyphs"
        );
        assert!(
            clipped.pixels != ellipsis.pixels,
            "rich text must actually truncate, not just scissor"
        );
        assert!(
            render(&mut caption("clip")).pixels == clipped.pixels,
            "overflow cache switch must work in both directions"
        );
        let background = || {
            Element::new().w(240.).h(96.).bg("#000000").child(
                Element::new()
                    .absolute()
                    .left(80.)
                    .top(0.)
                    .w(80.)
                    .h(96.)
                    .bg("#ffffff"),
            )
        };
        let mut sharp = background();
        let before = render(&mut sharp);
        let overlay = |opacity| {
            let layer: Element = jadix::primitives::overlay::overlay(1000)
                .color([0., 0., 0., 0.])
                .backdrop_blur(6.)
                .into();
            background().child(layer.opacity(opacity))
        };
        let faint = render(&mut overlay(0.25));
        let full = render(&mut overlay(1.));
        assert!(
            before.pixels != full.pixels,
            "backdrop must blur the captured page"
        );
        assert!(
            faint.pixels != full.pixels,
            "animation opacity must affect the blur"
        );
        assert!(full.pixel(76, 40).unwrap()[0] > before.pixel(76, 40).unwrap()[0] + 10);
        let mut nested = overlay(1.).child(
            jadix::primitives::overlay::overlay(1001)
                .color([0., 0., 0., 0.])
                .backdrop_blur(12.),
        );
        let nested = render(&mut nested);
        assert!(
            nested.pixels != full.pixels,
            "later blur must capture the already blurred prefix"
        );
        let mut local = background().child(jadix::utils::popup::portal_panel(
            Element::new()
                .absolute()
                .left(40.)
                .top(16.)
                .w(160.)
                .h(64.)
                .bg([0., 0., 0., 0.01])
                .backdrop_blur(6.)
                .rounded(12.),
            1000,
        ));
        let local = render(&mut local);
        assert!(local.pixel(76, 40).unwrap()[0] > before.pixel(76, 40).unwrap()[0] + 10);
        assert_eq!(
            local.pixel(76, 6),
            before.pixel(76, 6),
            "local blur must preserve outside pixels"
        );
        if let Some(directory) = std::env::var_os("JADIX_GPU_ACCEPTANCE_DIR") {
            let directory = std::path::PathBuf::from(directory);
            std::fs::create_dir_all(&directory).unwrap();
            for (name, image) in [
                ("rich-clip", clipped),
                ("rich-ellipsis", ellipsis),
                ("sharp", before),
                ("portal-faint", faint),
                ("portal-full", full),
                ("portal-nested", nested),
                ("local-blur", local),
            ] {
                std::fs::write(directory.join(format!("{name}.rgba")), image.pixels).unwrap();
            }
            std::fs::write(
                directory.join("pixels.json"),
                "{\"width\":240,\"height\":96,\"format\":\"RGBA8 display sRGB\"}",
            )
            .unwrap();
        }
    });
    owner.dispose();
}

#[test]
fn retained_scroll_observation_sees_silent_writes_once_and_unsubscribes() {
    use jadix::core::scroll_viewport::*;
    use std::{cell::RefCell, rc::Rc};
    let (owner, ()) = jadix_core::create_root(|| {
        let observed = Rc::new(RefCell::new(Vec::new()));
        let subscription = Rc::new(RefCell::new(None::<ScrollOffsetSubscription>));
        let output = observed.clone();
        let dispose = subscription.clone();
        *subscription.borrow_mut() =
            Some(observe_scroll_offset("silent-sdk-scroll", move |offset| {
                output.borrow_mut().push(offset);
                // Callback may remove its own subscription without borrowing the registry.
                dispose.borrow_mut().take();
            }));
        let _quiet = (0..64)
            .map(|index| observe_scroll_offset(&format!("quiet-scroll-{index}"), |_| {}))
            .collect::<Vec<_>>();
        let y =
            jadix_core::use_signal(&scroll_offset_y_signal_key("silent-sdk-scroll"), || 0.0_f32);
        y.set_silent(42.0);
        assert!(observed.borrow().is_empty());
        assert_eq!(flush_scroll_observers(), 1);
        assert_eq!(*observed.borrow(), vec![[0.0, 42.0]]);
        y.set_silent(84.0);
        assert_eq!(flush_scroll_observers(), 0);
        assert_eq!(observed.borrow().len(), 1);
    });
    owner.dispose();
}
#[test]
fn actual_gpu_scene_renders_tsx_and_click_roundtrip_changes_pixels() {
    let (owner, mut app) = jadix_core::create_root(|| {
        GpuApplication::new(
            ScriptApplication::development(
                "tests.gpu",
                include_str!("fixtures/application.js"),
                registry(),
                Default::default(),
            )
            .unwrap(),
        )
        .unwrap()
    });
    jadix_core::with_owner(owner.id(), || {
        let gpu = jadix::renderer::offscreen::OffscreenGpu::new()
            .expect("a real DX12 adapter is required for this acceptance");
        let mut renderer = jadix::renderer::offscreen::SceneRenderer::new(gpu);
        let mut root = app.element().unwrap();
        let first = renderer
            .render(&mut root, [400, 240], 1., [-1.; 2], wgpu::Color::BLACK)
            .unwrap();
        assert!(first.pixels.iter().any(|v| *v != 0));
        let input = &root.children[0].children[2];
        let textarea = &root.children[0].children[3];
        let multiline = &root.children[0].children[4];
        assert_eq!(input.viewport_layout_bounds()[3], 32.0);
        assert_eq!(textarea.viewport_layout_bounds()[3], 64.0);
        assert_eq!(multiline.viewport_layout_bounds()[3], 36.0);
        assert!(multiline.viewport_layout_bounds()[2] <= 180.0);
        assert!(multiline
            .text_style
            .line_height_scale
            .is_some_and(|scale| (scale - 1.25).abs() < f32::EPSILON));
        assert!(input.focusable && input.on_click.is_some());
        assert!(textarea.focusable && textarea.on_click.is_some());
        let button = &root.children[0].children[1];
        let callback = button.on_click.clone().unwrap();
        callback.borrow_mut()();
        root = app.element().unwrap();
        assert_eq!(
            app.application().roots()[0].children[0].descriptor.content,
            NodeContent::Text("Count: 1".into())
        );
        let second = renderer
            .render(&mut root, [400, 240], 1., [-1.; 2], wgpu::Color::BLACK)
            .unwrap();
        assert_ne!(first.pixels, second.pixels);
    });
    owner.dispose();
}

#[test]
fn shared_editor_preedit_selection_commit_and_cancel_reach_js_in_utf16() {
    let (owner, mut app) = jadix_core::create_root(|| {
        GpuApplication::new(
            ScriptApplication::development(
                "tests.gpu.ime",
                include_str!("fixtures/application.js"),
                registry(),
                Default::default(),
            )
            .unwrap(),
        )
        .unwrap()
    });
    jadix_core::with_owner(owner.id(), || {
        let _ = app.element().unwrap();
        let id = app.application().node_at(&[2]).unwrap();
        let name = format!("shell:{}:{}", id.root.token(), id.generation());
        use jadix::utils::text_field::*;
        use jadix_core::use_signal;
        use_signal(&input_ime_anchor_signal_key(&name), || None::<usize>).set(Some(0));
        use_signal(&input_preedit_signal_key(&name), String::new).set("中文😀".into());
        use_signal(&input_preedit_sel_signal_key(&name), || {
            None::<(usize, usize)>
        })
        .set(Some((6, 10)));
        let before = jadix::authoring::text::observe_edit(&name);
        assert_eq!(before.value, "中文😀");
        assert_eq!(before.selection, [2, 4]);
        assert_eq!(before.composing, Some([0, 4]));
        let _ = app.element().unwrap();
        assert!(
            matches!(&app.application().roots()[0].children[2].descriptor.content,NodeContent::Input{value,..} if value=="中文😀")
        );
        use_signal(&input_preedit_signal_key(&name), String::new).set(String::new());
        let _ = app.element().unwrap();
        assert!(
            matches!(&app.application().roots()[0].children[2].descriptor.content,NodeContent::Input{value,..} if value.is_empty())
        );
        commit_value(&name, "committed 中文😀".into());
        let _ = app.element().unwrap();
        assert!(
            matches!(&app.application().roots()[0].children[2].descriptor.content,NodeContent::Input{value,..} if value=="committed 中文😀")
        );
    });
    owner.dispose();
}

#[test]
fn gpu_pump_reads_one_dirty_field_for_a_1024_row_business_tree() {
    let source = format!(
        "globalThis.scaleRows=1024;globalThis.leafPatches=true;globalThis.fieldRows=true;\n{}",
        include_str!("fixtures/controlled-leaves.js")
    );
    let (owner, mut app) = jadix_core::create_root(|| {
        GpuApplication::new(
            ScriptApplication::development(
                "tests.gpu.pump.field",
                &source,
                registry(),
                Default::default(),
            )
            .unwrap(),
        )
        .unwrap()
    });
    jadix_core::with_owner(owner.id(), || {
        let _ = app.element().unwrap();
        let _ = app.element().unwrap();
        assert_eq!(app.edit_observations().count(), 1026);
        let id = app.application().node_at(&[0]).unwrap();
        let name = format!("shell:{}:{}", id.root.token(), id.generation());
        jadix::utils::text_field::commit_value(&name, "one changed field".into());
        let _ = app.element().unwrap();
        assert_eq!(
            app.last_pump_stats(),
            jadix_shell::gpu::PumpStats {
                field_candidates: 1,
                control_candidates: 0,
                scroll_candidates: 0,
            }
        );
        assert!(matches!(
            &app.application().roots()[0].children[0].descriptor.content,
            NodeContent::Input { value, .. } if value == "one changed field"
        ));
    });
    owner.dispose();
}

#[test]
fn gpu_pump_observes_notifying_and_flushed_silent_scroll_writes() {
    let (owner, mut app) = jadix_core::create_root(|| {
        GpuApplication::new(
            ScriptApplication::development(
                "tests.gpu.pump.scroll",
                include_str!("fixtures/capabilities.js"),
                registry(),
                Default::default(),
            )
            .unwrap(),
        )
        .unwrap()
    });
    jadix_core::with_owner(owner.id(), || {
        use jadix::core::scroll_viewport::*;
        let _ = app.element().unwrap();
        let _ = app.element().unwrap();
        let id = app.application().node_at(&[7]).unwrap();
        let name = format!("shell:{}:{}", id.root.token(), id.generation());
        let y = jadix_core::use_signal(&scroll_offset_y_signal_key(&name), || 0.0_f32);
        y.set(23.0);
        let _ = app.element().unwrap();
        assert_eq!(app.last_pump_stats().scroll_candidates, 1);
        assert!(matches!(
            &app.application().roots()[0].children[0].descriptor.content,
            NodeContent::Text(value) if value == "Scroll:0,23"
        ));
        y.set_silent(42.0);
        flush_scroll_observers();
        let _ = app.element().unwrap();
        assert_eq!(app.last_pump_stats().scroll_candidates, 1);
        assert!(matches!(
            &app.application().roots()[0].children[0].descriptor.content,
            NodeContent::Text(value) if value == "Scroll:0,42"
        ));
        let _ = app.element().unwrap();
        assert_eq!(app.last_pump_stats(), Default::default());
    });
    owner.dispose();
}

#[test]
fn gpu_pump_reads_one_dirty_select_for_a_1024_row_business_tree() {
    let source = format!(
        "globalThis.scaleRows=1024;globalThis.leafPatches=true;globalThis.controlRows=true;\n{}",
        include_str!("fixtures/control-leaves.js")
    );
    let (owner, mut app) = jadix_core::create_root(|| {
        GpuApplication::new(
            ScriptApplication::development(
                "tests.gpu.pump.select",
                &source,
                registry(),
                Default::default(),
            )
            .unwrap(),
        )
        .unwrap()
    });
    jadix_core::with_owner(owner.id(), || {
        let _ = app.element().unwrap();
        let mut root = app.element().unwrap();
        assert_eq!(
            app.application().roots()[0]
                .children
                .iter()
                .filter(|node| { matches!(node.descriptor.content, NodeContent::Select { .. }) })
                .count(),
            1025
        );
        let id = app.application().node_at(&[2]).unwrap();
        let name = format!("shell:{}:{}", id.root.token(), id.generation());
        let trigger = find_element(&root, &|element| {
            element.focus_id.as_deref() == Some(format!("{name}:trigger").as_str())
        })
        .expect("Select trigger");
        (trigger.on_click.clone().unwrap().borrow_mut())();
        root = app.element().unwrap();
        let option = find_element(&root, &|element| {
            element
                .attrs
                .get("jadix_select_item")
                .is_some_and(|value| value == "two")
        })
        .expect("second Select option");
        (option.on_click.clone().unwrap().borrow_mut())();
        let _ = app.element().unwrap();
        assert_eq!(
            app.last_pump_stats(),
            jadix_shell::gpu::PumpStats {
                field_candidates: 0,
                control_candidates: 1,
                scroll_candidates: 0,
            }
        );
        assert!(matches!(
            &app.application().roots()[0].children[2].descriptor.content,
            NodeContent::Select { value, .. } if value == "two"
        ));
    });
    owner.dispose();
}

fn find_element<'a>(
    root: &'a jadix::core::tree::Element,
    predicate: &impl Fn(&jadix::core::tree::Element) -> bool,
) -> Option<&'a jadix::core::tree::Element> {
    if predicate(root) {
        return Some(root);
    }
    root.children
        .iter()
        .find_map(|child| find_element(child, predicate))
}
fn click_text(root: &jadix::core::tree::Element, text: &str) {
    let element = find_element(root, &|e| {
        e.text.as_deref() == Some(text) && e.on_click.is_some()
    })
    .expect(text);
    (element.on_click.clone().unwrap().borrow_mut())();
}
#[test]
fn remaining_sdk_variants_map_controls_events_and_static_gpu_pixels() {
    let (owner, mut app) = jadix_core::create_root(|| {
        GpuApplication::new(
            ScriptApplication::development(
                "tests.capabilities",
                include_str!("fixtures/capabilities.js"),
                registry(),
                Default::default(),
            )
            .unwrap(),
        )
        .unwrap()
    });
    jadix_core::with_owner(owner.id(), || {
        use jadix::app::portal_overlay::*;
        clear_overlay_stack();
        let gpu = jadix::renderer::offscreen::OffscreenGpu::new().expect("real GPU adapter");
        let mut renderer = jadix::renderer::offscreen::SceneRenderer::new(gpu);
        let mut root = app.element().unwrap();
        let image = renderer
            .render(&mut root, [640, 480], 1., [-1.; 2], wgpu::Color::BLACK)
            .unwrap();
        assert!(
            image
                .pixels
                .chunks_exact(4)
                .any(|p| p[0] > 240 && p[1] < 15 && p[2] < 15),
            "PNG reached GPU texture upload"
        );
        let rich = find_element(&root, &|e| {
            e.text
                .as_deref()
                .is_some_and(|text| text.starts_with("A long label"))
        })
        .unwrap();
        let span = rich
            .text_spans
            .as_ref()
            .unwrap()
            .iter()
            .find(|span| span.text == "long")
            .unwrap();
        assert_eq!(span.weight, Some(700));
        assert!(span.italic && span.underline);
        assert_eq!(span.background, Some([30, 40, 70, 255]));
        let scroll = app.application().node_at(&[7]).unwrap();
        let name = format!("shell:{}:{}", scroll.root.token(), scroll.generation());
        let viewport = find_element(&root, &|e| {
            e.focus_id.as_deref() == Some(format!("scroll:{name}").as_str())
        })
        .expect("shared scroll focus survives projection");
        assert_eq!(viewport.viewport_layout_bounds()[3], 45.0);
        jadix_core::use_signal(
            &jadix::core::scroll_viewport::scroll_offset_y_signal_key(&name),
            || 0.0_f32,
        )
        .set(23.0);
        root = app.element().unwrap();
        assert!(
            matches!(&app.application().roots()[0].children[0].descriptor.content,NodeContent::Text(s) if s=="Scroll:0,23")
        );
        click_text(&root, "[ ] Check");
        root = app.element().unwrap();
        assert!(matches!(
            &app.application().roots()[0].children[4].descriptor.content,
            NodeContent::Checkbox { checked: true, .. }
        ));
        let select = app.application().node_at(&[3]).unwrap();
        let select_name = format!("shell:{}:{}", select.root.token(), select.generation());
        let trigger = find_element(&root, &|e| {
            e.focus_id.as_deref() == Some(format!("{select_name}:trigger").as_str())
        })
        .unwrap();
        (trigger.on_click.clone().unwrap().borrow_mut())();
        root = app.element().unwrap();
        // Production overlay bands now also render through the windowless seam.
        let overlay_pixels = renderer
            .render(&mut root, [640, 480], 1., [-1.; 2], wgpu::Color::BLACK)
            .unwrap();
        assert!(overlay_pixels.pixels.chunks_exact(4).any(|p| p[0] > 100));
        let option = find_element(&root, &|e| {
            e.attrs.get("jadix_select_item").is_some_and(|v| v == "b")
        })
        .expect("real shared Select menu");
        (option.on_click.clone().unwrap().borrow_mut())();
        root = app.element().unwrap();
        assert!(
            matches!(&app.application().roots()[0].children[0].descriptor.content,NodeContent::Text(s) if s=="Selected:b")
        );
        click_text(&root, "Show popover");
        root = app.element().unwrap();
        // Production overlay bands now also render through the windowless seam.
        let overlay_pixels = renderer
            .render(&mut root, [640, 480], 1., [-1.; 2], wgpu::Color::BLACK)
            .unwrap();
        assert!(overlay_pixels.pixels.chunks_exact(4).any(|p| p[0] > 100));
        assert!(top_stacked_exclusive_id().is_some());
        assert!(try_close_top());
        root = app.element().unwrap();
        assert!(
            matches!(&app.application().roots()[0].children[0].descriptor.content,NodeContent::Text(s) if s=="Popover:false")
        );
        click_text(&root, "Show popover");
        let _ = app.element().unwrap();
        jadix::utils::popup::arm_underlay_close();
        assert!(try_dismiss_outside_press());
        root = app.element().unwrap();
        assert!(
            matches!(&app.application().roots()[0].children[0].descriptor.content,NodeContent::Text(s) if s=="Popover:true")
        );
        click_text(&root, "Show dialog");
        root = app.element().unwrap();
        // Production overlay bands now also render through the windowless seam.
        let overlay_pixels = renderer
            .render(&mut root, [640, 480], 1., [-1.; 2], wgpu::Color::BLACK)
            .unwrap();
        assert!(overlay_pixels.pixels.chunks_exact(4).any(|p| p[0] > 100));
        assert!(stacked_modal_open());
        jadix::utils::popup::arm_underlay_close();
        let underlay = find_element(&root, &|e| {
            e.template_id
                .as_deref()
                .is_some_and(|id| id.ends_with("__modal_overlay"))
                && e.on_click.is_some()
        })
        .expect("modal underlay");
        (underlay.on_click.clone().unwrap().borrow_mut())();
        root = app.element().unwrap();
        assert!(
            matches!(&app.application().roots()[0].children[0].descriptor.content,NodeContent::Text(s) if s=="Dialog:true"),
            "actual dialog result: {:?}",
            app.application().roots()[0].children[0].descriptor.content
        );
        click_text(&root, "Show dialog");
        root = app.element().unwrap();
        click_text(&root, "Remove overlays");
        let _ = app.element().unwrap();
        assert!(!stacked_modal_open());
        assert!(top_stacked_exclusive_id().is_none());
        clear_overlay_stack();
    });
    owner.dispose();
}

fn render_sdk_text_case(
    renderer: &mut jadix::renderer::offscreen::SceneRenderer,
    text: &str,
    style: serde_json::Value,
    tag: &str,
) -> Vec<u8> {
    render_sdk_text_case_scaled(renderer, text, style, tag, 1.0)
}

fn render_sdk_text_case_scaled(
    renderer: &mut jadix::renderer::offscreen::SceneRenderer,
    text: &str,
    style: serde_json::Value,
    tag: &str,
    scale: f32,
) -> Vec<u8> {
    let source = format!(
        "globalThis.textSpec={};\n{}",
        serde_json::json!({"text": text, "style": style}),
        include_str!("fixtures/text-rendering.js")
    );
    let mut app = GpuApplication::new(
        ScriptApplication::development("tests.gpu.text", &source, registry(), Default::default())
            .unwrap(),
    )
    .unwrap();
    let mut root = app.element().unwrap();
    let size = [
        (240.0 * scale).round() as u32,
        (120.0 * scale).round() as u32,
    ];
    let image = renderer
        .render(&mut root, size, scale, [-1.; 2], wgpu::Color::BLACK)
        .unwrap();
    if let Some(path) = std::env::var_os("JADIX_GPU_TEXT_EVIDENCE") {
        let path = std::path::PathBuf::from(path);
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(path.join(format!("{tag}.rgba")), &image.pixels).unwrap();
        std::fs::write(
            path.join(format!("{tag}.json")),
            serde_json::to_vec_pretty(
                &serde_json::json!({"size":size,"scale":scale,"text":text,"style":style}),
            )
            .unwrap(),
        )
        .unwrap();
    }
    image.pixels
}

#[test]
fn sdk_inline_underline_changes_real_glyph_pixels_and_cache_switches_back() {
    let (owner, ()) = jadix_core::create_root(|| {
        use jadix::renderer::offscreen::{OffscreenGpu, SceneRenderer};
        let mut renderer = SceneRenderer::new(OffscreenGpu::new().expect("real GPU"));
        let text = "AAAA     BBBB";
        let plain = render_sdk_text_case(
            &mut renderer,
            text,
            serde_json::json!({}),
            "underline-plain",
        );
        let decorated = render_sdk_text_case(
            &mut renderer,
            text,
            serde_json::json!({
                "inline_styles": [{"range":[0,4], "underline":true}]
            }),
            "underline-on",
        );
        assert!(
            plain != decorated,
            "SDK inline underline must produce visible pixels"
        );
        let changed: Vec<_> = plain
            .chunks_exact(4)
            .zip(decorated.chunks_exact(4))
            .enumerate()
            .filter_map(|(i, (a, b))| (a != b).then_some((i % 240, i / 240)))
            .collect();
        assert!(changed.len() > 10);
        assert!(
            changed.iter().all(|(x, _)| *x < 90),
            "decoration must stay in its span"
        );
        assert!(
            render_sdk_text_case(
                &mut renderer,
                text,
                serde_json::json!({}),
                "underline-reset"
            ) == plain,
            "reset must restore the plain glyph pixels"
        );
    });
    owner.dispose();
}

#[test]
fn sdk_two_line_ellipsis_preserves_complete_lines_without_truncation() {
    let (owner, ()) = jadix_core::create_root(|| {
        use jadix::renderer::offscreen::{OffscreenGpu, SceneRenderer};
        let mut renderer = SceneRenderer::new(OffscreenGpu::new().expect("real GPU"));
        for rich in [false, true] {
            let mut style = serde_json::json!({"max_lines":2,"text_overflow":"clip"});
            if rich {
                style["inline_styles"] = serde_json::json!([{"range":[0,4],"bold":true}]);
            }
            let clip = render_sdk_text_case(
                &mut renderer,
                "AAAA\nBBBB",
                style.clone(),
                &format!("clamp-{rich}-fit-clip"),
            );
            style["text_overflow"] = serde_json::json!("ellipsis");
            let ellipsis = render_sdk_text_case(
                &mut renderer,
                "AAAA\nBBBB",
                style,
                &format!("clamp-{rich}-fit-ellipsis"),
            );
            assert!(
                clip == ellipsis,
                "a fitting two-line caption must not lose line 2 or gain ellipsis (rich={rich})"
            );
        }
    });
    owner.dispose();
}

#[test]
fn sdk_multiline_budget_matches_visible_reference_and_preserves_height_constraints() {
    let (owner, ()) = jadix_core::create_root(|| {
        use jadix::renderer::offscreen::{OffscreenGpu, SceneRenderer};
        let mut renderer = SceneRenderer::new(OffscreenGpu::new().expect("real GPU"));
        for rich in [false, true] {
            for mode in ["clip", "ellipsis", "wrap"] {
                let mut style = serde_json::json!({"max_lines":2,"text_overflow":mode});
                if rich {
                    style["inline_styles"] = serde_json::json!([{"range":[0,4],"bold":true}]);
                }
                let actual = render_sdk_text_case(
                    &mut renderer,
                    "AAAA\nBBBB\nCCCC",
                    style.clone(),
                    &format!("budget-{rich}-{mode}"),
                );
                let visible = if mode == "ellipsis" {
                    "AAAA\nBBBB…"
                } else {
                    "AAAA\nBBBB"
                };
                let expected = render_sdk_text_case(
                    &mut renderer,
                    visible,
                    style.clone(),
                    &format!("reference-{rich}-{mode}"),
                );
                assert!(
                    actual == expected,
                    "line budget must match its visible reference, rich={rich}, mode={mode}"
                );
                let ink = |top: usize, bottom: usize| {
                    actual[(top * 240 * 4)..(bottom * 240 * 4)]
                        .chunks_exact(4)
                        .filter(|p| p[..3].iter().any(|v| *v > 60))
                        .count()
                };
                assert!(ink(25, 50) > 40, "line 2 must be visible");
                assert_eq!(ink(50, 120), 0, "line 3 must not leak");

                style["min_height"] = serde_json::json!(100);
                let tall = render_sdk_text_case(
                    &mut renderer,
                    "AAAA\nBBBB\nCCCC",
                    style.clone(),
                    &format!("min-height-{rich}-{mode}"),
                );
                assert!(
                    tall == actual,
                    "minimum box height must not increase the visible line budget"
                );
                style.as_object_mut().unwrap().remove("min_height");
                style["max_height"] = serde_json::json!(12);
                let short = render_sdk_text_case(
                    &mut renderer,
                    "AAAA\nBBBB\nCCCC",
                    style,
                    &format!("max-height-{rich}-{mode}"),
                );
                assert!(
                    short[12 * 240 * 4..]
                        .chunks_exact(4)
                        .all(|p| p[..3] == [0, 0, 0]),
                    "author max_height must survive the line cap"
                );
            }
        }
    });
    owner.dispose();
}

#[test]
fn sdk_inline_strikethrough_and_combined_decoration_render_without_capability_rejection() {
    let (owner, ()) = jadix_core::create_root(|| {
        use jadix::renderer::offscreen::{OffscreenGpu, SceneRenderer};
        let mut renderer = SceneRenderer::new(OffscreenGpu::new().expect("real GPU"));
        for underline in [false, true] {
            let text = "AAAA     BBBB";
            let plain =
                render_sdk_text_case(&mut renderer, text, serde_json::json!({}), "strike-plain");
            let decorated = render_sdk_text_case(
                &mut renderer,
                text,
                serde_json::json!({"inline_styles":[{"range":[0,4],"strikethrough":true,"underline":underline}]}),
                &format!("strike-{underline}"),
            );
            let changed: Vec<_> = plain
                .chunks_exact(4)
                .zip(decorated.chunks_exact(4))
                .enumerate()
                .filter_map(|(i, (a, b))| (a != b).then_some(i % 240))
                .collect();
            assert!(changed.len() > 10);
            assert!(changed.iter().all(|x| *x < 90));
        }
    });
    owner.dispose();
}

#[test]
fn sdk_unicode_soft_wrap_clamp_survives_resize_and_fractional_dpr() {
    let (owner, ()) = jadix_core::create_root(|| {
        use jadix::renderer::offscreen::{OffscreenGpu, SceneRenderer};
        let mut renderer = SceneRenderer::new(OffscreenGpu::new().expect("real GPU"));
        let text = "中A 👩‍👩‍👧‍👦 é العربية אבג tail 0123456789 more words";
        for scale in [1.0, 1.5] {
            for width in [96, 180, 64, 180] {
                let mut style = serde_json::json!({"width":width,"max_lines":2,"font_size":20,
                    "inline_styles":[{"range":[0,4],"bold":true,"foreground":[100,220,255,255]}],
                    "text_overflow":"clip"});
                let tag = format!("unicode-{scale}-{width}");
                let clip = render_sdk_text_case_scaled(
                    &mut renderer,
                    text,
                    style.clone(),
                    &(tag.clone() + "-clip"),
                    scale,
                );
                style["text_overflow"] = serde_json::json!("ellipsis");
                let ellipsis = render_sdk_text_case_scaled(
                    &mut renderer,
                    text,
                    style.clone(),
                    &(tag.clone() + "-ellipsis"),
                    scale,
                );
                assert!(
                    clip != ellipsis,
                    "overflow must produce a marker at width={width}, scale={scale}"
                );
                let pixel_w = (240.0 * scale).round() as usize;
                let cutoff = (50.0 * scale).round() as usize;
                assert!(ellipsis[cutoff * pixel_w * 4..]
                    .chunks_exact(4)
                    .all(|p| p[..3] == [0, 0, 0]));
                let mut fresh =
                    SceneRenderer::new(OffscreenGpu::new().expect("independent GPU renderer"));
                assert!(
                    render_sdk_text_case_scaled(&mut fresh, text, style, &(tag + "-fresh"), scale)
                        == ellipsis,
                    "reused renderer must match a fresh renderer after width changes"
                );
            }
        }
    });
    owner.dispose();
}

#[test]
fn sdk_clamped_hidden_emoji_does_not_leak_custom_glyph_ink() {
    let (owner, ()) = jadix_core::create_root(|| {
        use jadix::renderer::offscreen::{OffscreenGpu, SceneRenderer};
        let mut renderer = SceneRenderer::new(OffscreenGpu::new().expect("real GPU"));
        for rich in [false, true] {
            for mode in ["clip", "ellipsis", "wrap"] {
                let mut style = serde_json::json!({"max_lines":2,"text_overflow":mode});
                if rich {
                    style["inline_styles"] = serde_json::json!([{"range":[0,4],"bold":true}]);
                }
                let reference = render_sdk_text_case(
                    &mut renderer,
                    "AAAA\nBBBB\nCCCC",
                    style.clone(),
                    &format!("hidden-{rich}-{mode}-reference"),
                );
                let hidden = render_sdk_text_case(
                    &mut renderer,
                    "AAAA\nBBBB\n👩‍👩‍👧‍👦",
                    style,
                    &format!("hidden-{rich}-{mode}-emoji"),
                );
                assert!(
                    hidden == reference,
                    "hidden emoji must not paint at a fallback position (rich={rich}, mode={mode})"
                );
            }
        }
    });
    owner.dispose();
}

#[cfg(feature = "text-undo")]
#[test]
fn controlled_editor_leaf_sdk_packets_use_complete_gpu_projection_and_match_full_pixels() {
    use jadix::renderer::offscreen::{OffscreenGpu, SceneRenderer};
    let (owner, ()) = jadix_core::create_root(|| {
        let mut apps = vec![];
        for partial in [false, true] {
            let source = format!(
                "globalThis.leafPatches={partial};globalThis.scaleRows=32;\n{}",
                include_str!("fixtures/controlled-leaves.js")
            );
            let mut app = GpuApplication::new(
                ScriptApplication::development(
                    "tests.gpu.editorleaves",
                    &source,
                    registry(),
                    Default::default(),
                )
                .unwrap(),
            )
            .unwrap();
            let mut renderer = SceneRenderer::new(OffscreenGpu::new().expect("real GPU adapter"));
            let mut root = app.element().unwrap();
            app.update_root(&mut renderer, &mut root).unwrap();
            renderer
                .render(&mut root, [400, 240], 1., [-1.; 2], wgpu::Color::BLACK)
                .unwrap();
            apps.push((app, renderer, root));
        }
        for (path, value) in [(0, "中文 🦀"), (1, "多行\n🙂"), (0, "SERVER text")] {
            let mut pixels = vec![];
            for (app, renderer, root) in &mut apps {
                let id = app.application().node_at(&[path]).unwrap();
                let name = format!("shell:{}:{}", id.root.token(), id.generation());
                jadix::utils::text_field::commit_value_with_policy(
                    &name,
                    value.into(),
                    jadix::utils::text_edit_history::SetTextUndoPolicy::Undoable,
                );
                let stats = app.update_root(renderer, root).unwrap();
                assert!(stats.full_projection);
                assert_eq!(stats.projected_nodes, 36);
                pixels.push(
                    renderer
                        .render(root, [400, 240], 1., [-1.; 2], wgpu::Color::BLACK)
                        .unwrap()
                        .pixels,
                );
            }
            assert_eq!(pixels[0], pixels[1]);
        }
        for (app, _, _) in &mut apps {
            app.dispose();
        }
    });
    owner.dispose();
}
