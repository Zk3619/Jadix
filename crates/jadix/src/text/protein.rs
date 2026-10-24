use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SecondaryStructure {
    Coil,
    Helix,
    Sheet,
}

#[derive(Clone, Debug)]
pub struct BackboneResidue {
    pub chain: String,
    pub number: i32,
    pub position: [f32; 3],
    pub structure: SecondaryStructure,
}

#[derive(Clone, Debug)]
pub struct ProteinBackbone {
    pub segments: Arc<[Vec<BackboneResidue>]>,
}

#[derive(Clone, Debug)]
pub struct RibbonQuad {
    pub corners: [[f32; 3]; 4],
    pub structure: SecondaryStructure,
}

fn field(line: &str, start: usize, end: usize) -> &str {
    line.get(start..end).unwrap_or("").trim()
}

impl ProteinBackbone {
    pub fn from_pdb(source: &str) -> Result<Self, String> {
        let mut annotations = Vec::new();
        for line in source.lines() {
            let (kind, chain, first, last) = match field(line, 0, 6) {
                "HELIX" => (
                    SecondaryStructure::Helix,
                    field(line, 19, 20),
                    field(line, 21, 25),
                    field(line, 33, 37),
                ),
                "SHEET" => (
                    SecondaryStructure::Sheet,
                    field(line, 21, 22),
                    field(line, 22, 26),
                    field(line, 33, 37),
                ),
                _ => continue,
            };
            if let (Ok(first), Ok(last)) = (first.parse::<i32>(), last.parse::<i32>()) {
                annotations.push((kind, chain.to_string(), first, last));
            }
        }
        let mut segments: Vec<Vec<BackboneResidue>> = vec![Vec::new()];
        let mut previous_key = String::new();
        let mut count = 0;
        for line in source.lines() {
            let record = field(line, 0, 6);
            if record == "ENDMDL" {
                break;
            }
            if record == "TER" {
                segments.push(Vec::new());
                previous_key.clear();
                continue;
            }
            if record != "ATOM"
                || field(line, 12, 16) != "CA"
                || !matches!(field(line, 16, 17), "" | "A")
            {
                continue;
            }
            let key = field(line, 21, 27).to_string();
            if key == previous_key {
                continue;
            }
            previous_key = key;
            let number = field(line, 22, 26)
                .parse::<i32>()
                .map_err(|_| "Invalid PDB residue number")?;
            let position = [
                field(line, 30, 38),
                field(line, 38, 46),
                field(line, 46, 54),
            ]
            .map(|value| value.parse::<f32>().ok().filter(|value| value.is_finite()));
            let [Some(horizontal), Some(vertical), Some(depth)] = position else {
                return Err("Invalid PDB backbone coordinates".into());
            };
            let position = [horizontal, vertical, depth];
            let chain = field(line, 21, 22).to_string();
            let structure = annotations
                .iter()
                .find(|(_, target, first, last)| {
                    *target == chain && (*first..=*last).contains(&number)
                })
                .map(|entry| entry.0)
                .unwrap_or(SecondaryStructure::Coil);
            if segments.last().unwrap().last().is_some_and(|previous| {
                previous.chain != chain || length(sub(position, previous.position)) > 4.8
            }) {
                segments.push(Vec::new());
            }
            segments.last_mut().unwrap().push(BackboneResidue {
                chain,
                number,
                position,
                structure,
            });
            count += 1;
            if count > 50_000 {
                return Err("Protein ribbon exceeds the 50,000 residue budget".into());
            }
        }
        segments.retain(|segment| segment.len() >= 2);
        if segments.is_empty() {
            return Err("Protein ribbon requires at least two connected C-alpha atoms".into());
        }
        Ok(Self {
            segments: segments.into(),
        })
    }

    pub fn ribbon(&self) -> Vec<RibbonQuad> {
        let mut quads = Vec::new();
        for segment in self.segments.iter() {
            let mut previous_edges = None;
            let mut normal = [0.0, 0.0, 1.0];
            for index in 0..segment.len() - 1 {
                let control = [
                    segment[index.saturating_sub(1)].position,
                    segment[index].position,
                    segment[index + 1].position,
                    segment[(index + 2).min(segment.len() - 1)].position,
                ];
                let kind = segment[index].structure;
                for step in 0..=8 {
                    let progress = step as f32 / 8.0;
                    let point = spline(control, progress);
                    let tangent = normalize(sub(
                        spline(control, (progress + 0.01).min(1.0)),
                        spline(control, (progress - 0.01).max(0.0)),
                    ));
                    let projected = sub(normal, scale(tangent, dot(normal, tangent)));
                    normal = if length(projected) < 0.01 {
                        normalize(cross(
                            tangent,
                            if tangent[1].abs() < 0.9 {
                                [0.0, 1.0, 0.0]
                            } else {
                                [1.0, 0.0, 0.0]
                            },
                        ))
                    } else {
                        normalize(projected)
                    };
                    let mut width = match kind {
                        SecondaryStructure::Coil => 0.24,
                        SecondaryStructure::Helix => 1.1,
                        SecondaryStructure::Sheet => 1.4,
                    };
                    if kind == SecondaryStructure::Sheet
                        && segment
                            .get(index + 1)
                            .is_none_or(|next| next.structure != kind)
                    {
                        width *= (1.0 - progress).max(0.04);
                    }
                    let edges = [
                        sub(point, scale(normal, width)),
                        add(point, scale(normal, width)),
                    ];
                    if let Some(previous) = previous_edges {
                        let previous: [[f32; 3]; 2] = previous;
                        quads.push(RibbonQuad {
                            corners: [previous[0], previous[1], edges[1], edges[0]],
                            structure: kind,
                        });
                    }
                    previous_edges = Some(edges);
                }
            }
        }
        quads
    }
}

fn spline(control: [[f32; 3]; 4], progress: f32) -> [f32; 3] {
    std::array::from_fn(|axis| {
        let [before, start, end, after] = control.map(|point| point[axis]);
        0.5 * (2.0 * start
            + (-before + end) * progress
            + (2.0 * before - 5.0 * start + 4.0 * end - after) * progress.powi(2)
            + (-before + 3.0 * start - 3.0 * end + after) * progress.powi(3))
    })
}
fn add(left: [f32; 3], right: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|axis| left[axis] + right[axis])
}
fn sub(left: [f32; 3], right: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|axis| left[axis] - right[axis])
}
fn scale(point: [f32; 3], factor: f32) -> [f32; 3] {
    point.map(|value| value * factor)
}
fn dot(left: [f32; 3], right: [f32; 3]) -> f32 {
    (0..3).map(|axis| left[axis] * right[axis]).sum()
}
fn cross(left: [f32; 3], right: [f32; 3]) -> [f32; 3] {
    [
        left[1] * right[2] - left[2] * right[1],
        left[2] * right[0] - left[0] * right[2],
        left[0] * right[1] - left[1] * right[0],
    ]
}
fn length(point: [f32; 3]) -> f32 {
    dot(point, point).sqrt()
}
fn normalize(point: [f32; 3]) -> [f32; 3] {
    scale(point, 1.0 / length(point).max(0.000001))
}
