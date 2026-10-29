use super::*;
use crate::layout::layout_tree::LayoutTree;

const OK_FS: &str = r#"
        @fragment
        fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
            return vec4<f32>(in.uv, 0.2, 1.0);
        }
    "#;

#[test]
fn shader_leaf_layout_bounds() {
    let leaf: Element = shader().w(120.0).h(90.0).wgsl(OK_FS).into();
    assert!(leaf.children.is_empty());
    assert!(leaf.get_attr(ATTR_SHADER).is_some());
    let mut root = Element::new().w(200.0).h(140.0).child(leaf);
    let mut tree = LayoutTree::new();
    let result = tree.build_and_layout(&mut root, 200.0, 140.0, [-1.0, -1.0]);
    let [_, _, w, h] = root.children.first().expect("leaf").layout_bounds;
    assert!(
        w > 110.0 && w < 130.0 && h > 80.0 && h < 100.0,
        "shader leaf bounds [{w}, {h}]"
    );
    let jobs = take_shader_jobs();
    assert!(
        !jobs.is_empty(),
        "layout collect must queue a GPU job without calling runtime::request_redraw"
    );
    assert!(result
        .textured_draws
        .iter()
        .any(|d| d.texture_key.starts_with("jadix_shader:")));
}

#[test]
fn shader_resize_queues_new_extent() {
    let mut attrs = std::collections::HashMap::new();
    let id = register_source(OK_FS.into());
    attrs.insert(ATTR_SHADER.to_string(), format!("{id:x}"));
    let (k0, w0, h0) = shader_tex_from_element_attrs(&attrs, 64.0, 48.0).expect("64x48");
    let jobs0 = take_shader_jobs();
    assert_eq!(jobs0[0].width, 64);
    assert_eq!(jobs0[0].height, 48);
    let (k1, w1, h1) = shader_tex_from_element_attrs(&attrs, 128.0, 96.0).expect("128x96");
    let jobs1 = take_shader_jobs();
    assert_eq!(k0, k1, "texture key is id-stable across resize");
    assert_eq!((w1, h1), (128.0, 96.0));
    assert_ne!((w0, h0), (w1, h1));
    assert_eq!(jobs1[0].width, 128);
    unregister_source(id);
}

#[test]
fn shader_collect_does_not_reenter_appstate_borrow() {
    let mut attrs = std::collections::HashMap::new();
    let id = register_source(OK_FS.into());
    attrs.insert(ATTR_SHADER.to_string(), format!("{id:x}"));
    crate::app::runtime::call_while_app_state_mutably_borrowed(|| {
        assert!(shader_tex_from_element_attrs(&attrs, 64.0, 48.0).is_some());
    });
    assert!(!take_shader_jobs().is_empty());
    unregister_source(id);
}

#[test]
fn shader_hit_covers_whole_leaf() {
    let leaf: Element = shader().w(80.0).h(60.0).wgsl(OK_FS).into();
    let mut root = Element::new().w(200.0).h(140.0).child(leaf);
    let mut tree = LayoutTree::new();
    let _ = tree.build_and_layout(&mut root, 200.0, 140.0, [-1.0, -1.0]);
    let [x, y, w, h] = root.children[0].layout_bounds;
    assert!(w >= 79.0 && h >= 59.0, "leaf must occupy its box");
    let cx = x + w * 0.5;
    let cy = y + h * 0.5;
    assert!(cx >= x && cx <= x + w && cy >= y && cy <= y + h);
    let _ = take_shader_jobs();
}

#[test]
fn shader_texture_queues_src_key() {
    let _ = take_shader_jobs();
    let leaf: Element = shader()
        .w(40.0)
        .h(24.0)
        .wgsl(OK_FS)
        .texture("src", "jadix_canvas:deadbeef")
        .into();
    assert_eq!(
        leaf.get_attr(ATTR_SHADER_TEX).map(|s| s.as_str()),
        Some("jadix_canvas:deadbeef")
    );
    shader_tex_from_element_attrs(&leaf.attrs, 40.0, 24.0).expect("job");
    let mut graph = crate::renderer::RenderGraph::default();
    enrich_shader_graph(&mut graph);
    assert!(
        graph
            .nodes
            .iter()
            .any(|n| n.reads.iter().any(|k| k == "jadix_canvas:deadbeef")),
        "graph must read the sampled compositor key before ShaderGpu flush"
    );
    let jobs = take_shader_jobs();
    assert_eq!(
        jobs[0].tex_key.as_deref(),
        Some("jadix_canvas:deadbeef"),
        "layout collect must pass the sampled key to ShaderGpu"
    );
}

#[test]
fn shader_preset_blur_sets_builtin_and_mode() {
    let _ = take_shader_jobs();
    let leaf: Element = shader().w(32.0).h(24.0).preset("blur").into();
    assert!(leaf.get_attr(ATTR_SHADER).is_some());
    shader_tex_from_element_attrs(&leaf.attrs, 32.0, 24.0).expect("job");
    let jobs = take_shader_jobs();
    assert!(
        jobs[0].source.contains("blur9") || jobs[0].custom[0] > 1.5,
        "preset blur must ship builtin FS, custom={:?}",
        jobs[0].custom
    );
    assert!((jobs[0].custom[0] - 2.0).abs() < 0.1, "blur mode=2");
    assert!(jobs[0].custom[1] > 0.2, "default strength");
}

#[test]
fn shader_buffer_queues_feedback_read() {
    let _ = take_shader_jobs();
    let leaf: Element = shader().w(16.0).h(12.0).wgsl(OK_FS).buffer(true).into();
    assert_eq!(
        leaf.get_attr(ATTR_SHADER_BUFFER).map(String::as_str),
        Some("1")
    );
    let (key, _, _) = shader_tex_from_element_attrs(&leaf.attrs, 16.0, 12.0).expect("job");
    let mut graph = crate::renderer::RenderGraph::default();
    enrich_shader_graph(&mut graph);
    let buf = format!("{key}:buf");
    assert!(
        graph
            .nodes
            .iter()
            .any(|n| n.reads.iter().any(|k| k == &buf)),
        "buffer leaf must read previous ping texture"
    );
    let jobs = take_shader_jobs();
    assert!(jobs[0].buffer);
}

#[test]
fn shader_texture_src1_queues_slot1() {
    let _ = take_shader_jobs();
    let leaf: Element = shader()
        .w(40.0)
        .h(24.0)
        .wgsl(OK_FS)
        .texture("src1", "jadix_path:slot1")
        .into();
    assert_eq!(
        leaf.get_attr(ATTR_SHADER_TEX1).map(|s| s.as_str()),
        Some("jadix_path:slot1")
    );
    assert!(leaf.get_attr(ATTR_SHADER_TEX).is_none());
    shader_tex_from_element_attrs(&leaf.attrs, 40.0, 24.0).expect("job");
    let mut graph = crate::renderer::RenderGraph::default();
    enrich_shader_graph(&mut graph);
    assert!(
        graph
            .nodes
            .iter()
            .any(|n| n.reads.iter().any(|k| k == "jadix_path:slot1")),
        "graph must read the slot-1 compositor key"
    );
    let jobs = take_shader_jobs();
    assert_eq!(jobs[0].tex_key.as_deref(), None);
    assert_eq!(jobs[0].tex_key1.as_deref(), Some("jadix_path:slot1"));
}

#[test]
fn shader_texture_src2_queues_slot2() {
    let _ = take_shader_jobs();
    let leaf: Element = shader()
        .w(40.0)
        .h(24.0)
        .wgsl(OK_FS)
        .texture("src2", "jadix_path:slot2")
        .into();
    assert_eq!(
        leaf.get_attr(ATTR_SHADER_TEX2).map(|s| s.as_str()),
        Some("jadix_path:slot2")
    );
    assert!(leaf.get_attr(ATTR_SHADER_TEX).is_none());
    shader_tex_from_element_attrs(&leaf.attrs, 40.0, 24.0).expect("job");
    let mut graph = crate::renderer::RenderGraph::default();
    enrich_shader_graph(&mut graph);
    assert!(
        graph
            .nodes
            .iter()
            .any(|n| n.reads.iter().any(|k| k == "jadix_path:slot2")),
        "graph must read the slot-2 compositor key"
    );
    let jobs = take_shader_jobs();
    assert_eq!(jobs[0].tex_key2.as_deref(), Some("jadix_path:slot2"));
}

#[test]
fn shader_buffer_with_src_reads_ping_on_slot1() {
    let _ = take_shader_jobs();
    let leaf: Element = shader()
        .w(16.0)
        .h(12.0)
        .wgsl(OK_FS)
        .texture("src", "jadix_canvas:a")
        .buffer(true)
        .into();
    let (key, _, _) = shader_tex_from_element_attrs(&leaf.attrs, 16.0, 12.0).expect("job");
    let mut graph = crate::renderer::RenderGraph::default();
    enrich_shader_graph(&mut graph);
    let buf = format!("{key}:buf");
    assert!(
        graph.nodes.iter().any(|n| {
            n.reads.iter().any(|k| k == "jadix_canvas:a") && n.reads.iter().any(|k| k == &buf)
        }),
        "user src + buffer must read compositor and previous ping"
    );
    let jobs = take_shader_jobs();
    assert_eq!(jobs[0].tex_key.as_deref(), Some("jadix_canvas:a"));
    assert!(jobs[0].buffer);
    assert!(jobs[0].tex_key1.is_none());
}
