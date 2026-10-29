use super::*;

#[test]
fn dump_lists_pass_kinds() {
    let g = RenderGraph::standard_composite(["jadix_canvas:1".into()]);
    let d = g.dump();
    assert!(d.contains("Geometry2d"));
    assert!(d.contains("Composite"));
    assert!(d.contains("GlyphText"));
    assert!(d.contains("Present"));
    assert!(d.contains("jadix_canvas:1"));
}

#[test]
fn dump_lists_compute_kind() {
    let mut g = RenderGraph::default();
    g.push(
        RenderPassNode::new(PassKind::Compute, "blur-cs")
            .read("jadix_capture:1")
            .write("jadix_fx:1"),
    );
    let d = g.dump();
    assert!(d.contains("Compute"));
    assert!(d.contains("blur-cs"));
}

#[test]
fn last_dump_roundtrip() {
    store_last_dump("hello-graph".into());
    assert_eq!(take_last_dump(), "hello-graph");
}
