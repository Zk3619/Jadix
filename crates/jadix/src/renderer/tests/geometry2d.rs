use super::*;
use crate::primitives::path::{polyline, PathStyle};

#[test]
fn stroke_polyline_emits_instances() {
    let cmds = polyline(&[(0.0, 0.0), (40.0, 0.0), (40.0, 20.0)]);
    let style = PathStyle::stroke([1.0, 1.0, 1.0, 1.0], 2.0);
    let inst = path_to_instances(&cmds, &style, [10.0, 5.0], None, 1.0);
    assert!(!inst.is_empty(), "tessellated stroke must emit quads");
    assert!(inst.iter().all(|i| i.rect_pos[0] >= 10.0 - 4.0));
}
