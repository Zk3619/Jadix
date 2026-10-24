//! Ball-and-stick orthographic molecular graphics, separate from text layout.

use std::cell::Cell;
use std::rc::Rc;

use crate::components::base::{paint_slot, SlotChrome};
use crate::core::reactivity::Signal;
use crate::core::tree::{Element, LayoutStatus};
use crate::primitives::canvas::{canvas, CanvasPath2D};
use crate::text::Molecule;
use jadix_macros::component;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MoleculeCamera {
    pub yaw: f32,
    pub pitch: f32,
    pub zoom: f32,
}
impl Default for MoleculeCamera {
    fn default() -> Self {
        Self {
            yaw: 0.0,
            pitch: 0.0,
            zoom: 1.0,
        }
    }
}

pub fn molecule_view(molecule: Molecule) -> MoleculeViewBuilder {
    MoleculeViewBuilder {
        molecule,
        camera: None,
        height: 280.0,
        chrome: SlotChrome::default(),
    }
}

pub struct MoleculeViewBuilder {
    molecule: Molecule,
    camera: Option<Signal<MoleculeCamera>>,
    height: f32,
    chrome: SlotChrome,
}
impl MoleculeViewBuilder {
    crate::impl_slot_chrome_methods!();
    pub fn camera_signal(mut self, camera: Signal<MoleculeCamera>) -> Self {
        self.camera = Some(camera);
        self
    }
    pub fn viewport_h(mut self, height: f32) -> Self {
        self.height = height.max(1.0);
        self
    }
}

pub(crate) fn render_molecule_source(source: &str, format: &str) -> Element {
    if format == "pdb" {
        if let Ok(protein) = crate::text::protein::ProteinBackbone::from_pdb(source) {
            return crate::components::protein_view::protein_view(protein);
        }
    }
    let molecule = match format {
        "xyz" => Molecule::from_xyz(source),
        "pdb" => Molecule::from_pdb(source),
        "mol" => Err("MDL MOL requires a custom DocumentBlockRenderer; molecule blocks accept JSON, xyz or pdb".into()),
        _ => Molecule::from_json(source),
    };
    match molecule {
        Ok(molecule) => molecule_view(molecule).into(),
        Err(error) => crate::components::math_view::render_diagnostic(source, &error),
    }
}

impl From<MoleculeViewBuilder> for Element {
    fn from(builder: MoleculeViewBuilder) -> Self {
        let camera = builder
            .camera
            .unwrap_or_else(|| Signal::new(MoleculeCamera::default()));
        let atoms = builder.molecule.atoms();
        let mut minimum = [f64::INFINITY; 3];
        let mut maximum = [f64::NEG_INFINITY; 3];
        for atom in atoms {
            for axis in 0..3 {
                minimum[axis] = minimum[axis].min(f64::from(atom.position[axis]));
                maximum[axis] = maximum[axis].max(f64::from(atom.position[axis]));
            }
        }
        let center = std::array::from_fn::<_, 3, _>(|axis| (minimum[axis] + maximum[axis]) * 0.5);
        let diameter = (0..3)
            .map(|axis| maximum[axis] - minimum[axis])
            .fold(1.0_f64, f64::max);
        let coordinates: Vec<_> = atoms
            .iter()
            .map(|atom| {
                std::array::from_fn::<_, 3, _>(|axis| {
                    ((f64::from(atom.position[axis]) - center[axis]) / diameter) as f32
                })
            })
            .collect();
        let colors: Vec<_> = atoms.iter().map(|atom| atom_color(&atom.element)).collect();
        let radii: Vec<_> = atoms.iter().map(|atom| match atom.element.to_ascii_uppercase().as_str() {
            "H" | "D" => 0.19,
            "O" | "N" | "C" => 0.30,
            _ => 0.38,
        } / diameter as f32).collect();
        let atom_count = atoms.len();
        let bond_count = builder.molecule.bonds().len();
        let molecule = builder.molecule;
        let mut element: Element = canvas()
            .w("100%")
            .h(builder.height)
            .paint(move |context, size| {
                let pose = camera.try_peek().unwrap_or_default();
                let scale = size.w.min(size.h) * pose.zoom.clamp(0.1, 20.0) * 0.75;
                let (sin_yaw, cos_yaw) = pose.yaw.sin_cos();
                let (sin_pitch, cos_pitch) = pose.pitch.sin_cos();
                let projected: Vec<_> = coordinates
                    .iter()
                    .map(|[horizontal, vertical, depth]| {
                        let horizontal_rotated = horizontal * cos_yaw + depth * sin_yaw;
                        let depth_rotated = -horizontal * sin_yaw + depth * cos_yaw;
                        [
                            size.w * 0.5 + horizontal_rotated * scale,
                            size.h * 0.5
                                + (vertical * cos_pitch - depth_rotated * sin_pitch) * scale,
                            vertical * sin_pitch + depth_rotated * cos_pitch,
                        ]
                    })
                    .collect();
                context.clear("transparent");
                context.set_stroke("#7f858c");
                context.set_line_width((scale * 0.035).clamp(2.0, 10.0));
                for bond in molecule.bonds() {
                    let from = projected[bond.atoms[0]];
                    let to = projected[bond.atoms[1]];
                    for order in 0..bond.order {
                        let shift = (f32::from(order) - f32::from(bond.order - 1) * 0.5) * 3.0;
                        let mut path = CanvasPath2D::new();
                        path.move_to(from[0], from[1] + shift);
                        path.line_to(to[0], to[1] + shift);
                        context.stroke_path(path.commands());
                    }
                }
                let mut order: Vec<_> = (0..projected.len()).collect();
                order.sort_unstable_by(|left, right| {
                    projected[*left][2].total_cmp(&projected[*right][2])
                });
                for position in order {
                    let radius = (radii[position] * scale).clamp(2.0, 42.0);
                    let point = projected[position];
                    if point[0] < -radius
                        || point[0] > size.w + radius
                        || point[1] < -radius
                        || point[1] > size.h + radius
                    {
                        continue;
                    }
                    let color = colors[position];
                    let channels = [1..3, 3..5, 5..7]
                        .map(|range| u8::from_str_radix(&color[range], 16).unwrap());
                    let steps = if atom_count < 500 { 14 } else { 3 };
                    for step in 0..steps {
                        let fraction = step as f32 / steps as f32;
                        let ring = radius * (1.0 - fraction * 0.8);
                        let horizontal = point[0] - radius * fraction * 0.24;
                        let vertical = point[1] - radius * fraction * 0.28;
                        let mut path = CanvasPath2D::new();
                        path.arc(horizontal, vertical, ring, 0.0, std::f32::consts::PI);
                        path.arc(
                            horizontal,
                            vertical,
                            ring,
                            std::f32::consts::PI,
                            std::f32::consts::TAU,
                        );
                        path.close_path();
                        let shaded = channels.map(|channel| {
                            ((f32::from(channel) * (0.55 + fraction * 0.45)) + 70.0 * fraction)
                                .min(255.0) as u8
                        });
                        context.set_fill(&format!(
                            "#{:02x}{:02x}{:02x}",
                            shaded[0], shaded[1], shaded[2]
                        ));
                        context.fill_path(path.commands());
                    }
                }
                LayoutStatus::Clean
            })
            .into();
        let paint_id = element
            .attrs
            .get(crate::primitives::canvas::ATTR_CANVAS_PAINT)
            .cloned()
            .unwrap();
        let dragging = Rc::new(Cell::new(false));
        let down = dragging.clone();
        element = element.on_mousedown(move |event| {
            if event.button_index() == 0 {
                down.set(true);
            }
        });
        let moved = dragging.clone();
        let dirty = paint_id.clone();
        element = element.on_mousemove(move |event| {
            if moved.get() && event.buttons & 1 != 0 {
                let mut pose = camera.peek();
                pose.yaw += event.movement_x * 0.01;
                pose.pitch += event.movement_y * 0.01;
                camera.set_silent(pose);
                crate::primitives::canvas::mark_canvas_dirty_hex(&dirty);
                crate::app::runtime::request_redraw();
            }
        });
        element = element
            .on_mouseup(move |_| dragging.set(false))
            .on_double_click(move |_| {
                camera.set_silent(MoleculeCamera::default());
                crate::primitives::canvas::mark_canvas_dirty_hex(&paint_id);
                crate::app::runtime::request_redraw();
            });
        let dirty = element.attrs[crate::primitives::canvas::ATTR_CANVAS_PAINT].clone();
        let previous_pose = Cell::new(camera.peek());
        element = element.reactive_sync(move |_, _| {
            let pose = camera.try_peek().unwrap_or_default();
            if previous_pose.replace(pose) != pose {
                crate::primitives::canvas::mark_canvas_dirty_hex(&dirty);
            }
            LayoutStatus::Clean
        });
        element.set_attr("accesskit_role", "image");
        element.set_attr("aria-label", format!("Molecular structure: {atom_count} atoms, {bond_count} bonds. Drag to rotate; double-click to reset."));
        element.set_attr("data-atom-count", atom_count.to_string());
        element.set_attr("data-bond-count", bond_count.to_string());
        crate::components::text_view::redraw_on_snapshot(
            paint_slot(element, "molecule-view", &builder.chrome),
            Some(camera),
        )
    }
}

fn atom_color(element: &str) -> &'static str {
    match element.to_ascii_uppercase().as_str() {
        "H" => "#d9dce0",
        "O" => "#e94b4b",
        "N" => "#4479df",
        "S" => "#d5b830",
        "P" => "#e58e3c",
        "CL" | "F" => "#49ab66",
        _ => "#666f7a",
    }
}

#[component(crate_path = "crate")]
#[allow(non_snake_case)]
pub fn MoleculeView(
    molecule: Option<Molecule>,
    camera: Option<Signal<MoleculeCamera>>,
    viewport_h: Option<f32>,
) -> Element {
    let Some(molecule) = molecule else {
        return crate::components::math_view::render_diagnostic(
            "",
            "No molecular structure provided",
        );
    };
    let mut builder = molecule_view(molecule);
    if let Some(camera) = camera {
        builder = builder.camera_signal(camera);
    }
    if let Some(height) = viewport_h {
        builder = builder.viewport_h(height);
    }
    builder.into()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn molecular_graph_raster_is_nonblank_without_per_atom_ui_nodes() {
        let molecule = Molecule::from_xyz("3\nwater\nO 0 0 0\nH 1 0 0\nH 0 1 0\n").unwrap();
        let element: Element = molecule_view(molecule).into();
        assert!(element.children.is_empty());
        let (pixels, _, _) = crate::primitives::canvas::rasterize_canvas_premul_for_test(
            &element.attrs,
            200.0,
            160.0,
        )
        .unwrap();
        assert!(pixels.chunks_exact(4).any(|pixel| pixel[3] > 0.5));
    }

    #[test]
    fn water_raster_contains_bonds_between_atoms() {
        let molecule =
            Molecule::from_xyz("3\nwater\nO 0 0 0\nH 0.958 0 0\nH -0.239 0.927 0\n").unwrap();
        let element: Element = molecule_view(molecule).into();
        assert_eq!(
            element.get_attr("data-bond-count").map(String::as_str),
            Some("2")
        );
        let (pixels, _, _) = crate::primitives::canvas::rasterize_canvas_premul_for_test(
            &element.attrs,
            200.0,
            160.0,
        )
        .unwrap();
        assert!(pixels[(34 * 200 + 112) * 4 + 3] > 0.5);
    }
}
