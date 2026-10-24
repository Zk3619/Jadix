use crate::{
    components::molecule_view::MoleculeCamera,
    core::{reactivity::Signal, tree::Element},
    primitives::canvas::{canvas, CanvasPath2D},
    text::protein::{ProteinBackbone, SecondaryStructure},
};
use std::{cell::Cell, rc::Rc};

pub fn protein_view(protein: ProteinBackbone) -> Element {
    let camera = Signal::new(MoleculeCamera {
        yaw: 0.25,
        pitch: 0.3,
        zoom: 1.0,
    });
    let mut quads = protein.ribbon();
    let mut minimum = [f32::INFINITY; 3];
    let mut maximum = [f32::NEG_INFINITY; 3];
    for point in quads.iter().flat_map(|quad| quad.corners) {
        for axis in 0..3 {
            minimum[axis] = minimum[axis].min(point[axis]);
            maximum[axis] = maximum[axis].max(point[axis]);
        }
    }
    let diameter = (0..3)
        .map(|axis| maximum[axis] - minimum[axis])
        .fold(1.0_f32, f32::max);
    for point in quads.iter_mut().flat_map(|quad| &mut quad.corners) {
        for axis in 0..3 {
            point[axis] = (point[axis] - (minimum[axis] + maximum[axis]) * 0.5) / diameter;
        }
    }
    let mut element: Element = canvas()
        .w("100%")
        .h(340.0)
        .paint(move |context, size| {
            let pose = camera.peek();
            let scale = size.w.min(size.h) * pose.zoom * 0.85;
            let project = |point: [f32; 3]| {
                let (yaw_sin, yaw_cos) = pose.yaw.sin_cos();
                let (pitch_sin, pitch_cos) = pose.pitch.sin_cos();
                let horizontal = point[0] * yaw_cos + point[2] * yaw_sin;
                let depth = -point[0] * yaw_sin + point[2] * yaw_cos;
                [
                    size.w * 0.5 + horizontal * scale,
                    size.h * 0.5 - (point[1] * pitch_cos - depth * pitch_sin) * scale,
                    point[1] * pitch_sin + depth * pitch_cos,
                ]
            };
            let mut projected: Vec<_> = quads
                .iter()
                .map(|quad| (quad.corners.map(project), quad.structure))
                .collect();
            projected.sort_unstable_by(|left, right| {
                left.0
                    .iter()
                    .map(|point| point[2])
                    .sum::<f32>()
                    .total_cmp(&right.0.iter().map(|point| point[2]).sum::<f32>())
            });
            context.clear("transparent");
            for (corners, kind) in projected {
                let direction = [corners[1][0] - corners[0][0], corners[1][1] - corners[0][1]];
                let shade = (0.65
                    + 0.35 * direction[0].abs() / (direction[0].hypot(direction[1])).max(0.001))
                .clamp(0.4, 1.0);
                let color: [u8; 3] = match kind {
                    SecondaryStructure::Coil => [75, 162, 116],
                    SecondaryStructure::Helix => [222, 93, 112],
                    SecondaryStructure::Sheet => [220, 178, 62],
                };
                let color = color.map(|value| (f32::from(value) * shade) as u8);
                context.set_fill(&format!(
                    "#{:02x}{:02x}{:02x}",
                    color[0], color[1], color[2]
                ));
                let mut path = CanvasPath2D::new();
                path.move_to(corners[0][0], corners[0][1]);
                for point in &corners[1..] {
                    path.line_to(point[0], point[1]);
                }
                path.close_path();
                context.fill_path(path.commands());
            }
            crate::core::tree::LayoutStatus::Clean
        })
        .into();
    let dirty = element.attrs[crate::primitives::canvas::ATTR_CANVAS_PAINT].clone();
    let dragging = Rc::new(Cell::new(false));
    let down = dragging.clone();
    let moved = dragging.clone();
    element = element
        .on_mousedown(move |event| {
            if event.button_index() == 0 {
                down.set(true);
            }
        })
        .on_mousemove(move |event| {
            if moved.get() && event.buttons & 1 != 0 {
                let mut pose = camera.peek();
                pose.yaw += event.movement_x * 0.01;
                pose.pitch += event.movement_y * 0.01;
                camera.set_silent(pose);
                crate::primitives::canvas::mark_canvas_dirty_hex(&dirty);
                crate::app::runtime::request_redraw();
            }
        })
        .on_mouseup(move |_| dragging.set(false));
    element.set_attr("data-slot", "protein-view");
    element.set_attr("accesskit_role", "image");
    element.set_attr(
        "aria-label",
        "Protein C-alpha ribbon; pink helices, yellow sheets, green coils. Drag to rotate.",
    );
    element
}
