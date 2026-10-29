//! Typed atomic coordinates and distance-based connectivity for coordinate-only files.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MoleculeAtom {
    pub element: String,
    pub position: [f32; 3],
    #[serde(default)]
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MoleculeBond {
    pub atoms: [usize; 2],
    #[serde(default = "single_bond")]
    pub order: u8,
    #[serde(default)]
    pub inferred: bool,
}

fn single_bond() -> u8 {
    1
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MoleculeData {
    pub atoms: Vec<MoleculeAtom>,
    #[serde(default)]
    pub bonds: Vec<MoleculeBond>,
}

#[derive(Clone, Debug)]
pub struct Molecule(Arc<MoleculeData>);

impl PartialEq for Molecule {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for Molecule {}

impl Molecule {
    pub fn new(data: MoleculeData) -> Result<Self, String> {
        if data.atoms.len() > 1_000_000 || data.bonds.len() > 4_000_000 {
            return Err("Molecular structure exceeds the atom/bond budget".into());
        }
        if data.atoms.is_empty() {
            return Err("Molecular structure contains no atoms".into());
        }
        for atom in &data.atoms {
            if !atom.position.iter().all(|value| value.is_finite())
                || atom.element.is_empty()
                || !atom
                    .element
                    .chars()
                    .all(|character| character.is_ascii_alphabetic())
            {
                return Err("Atoms require a valid element label and finite coordinates".into());
            }
        }
        for bond in &data.bonds {
            if bond.atoms[0] == bond.atoms[1]
                || bond.atoms.iter().any(|index| *index >= data.atoms.len())
                || !(1..=3).contains(&bond.order)
            {
                return Err("Bond endpoints or bond order are invalid".into());
            }
        }
        Ok(Self(Arc::new(data)))
    }

    pub fn atoms(&self) -> &[MoleculeAtom] {
        &self.0.atoms
    }
    pub fn bonds(&self) -> &[MoleculeBond] {
        &self.0.bonds
    }

    pub fn infer_bonds(&self) -> Result<Self, String> {
        if !self.bonds().is_empty() {
            return Ok(self.clone());
        }
        let mut cells: HashMap<[i64; 3], Vec<usize>> = HashMap::new();
        let mut bonds = Vec::new();
        let mut comparisons = 0usize;
        for (index, atom) in self.atoms().iter().enumerate() {
            let Some(radius) = covalent_radius(&atom.element) else {
                continue;
            };
            let cell = atom
                .position
                .map(|coordinate| (f64::from(coordinate) / 3.6).floor() as i64);
            for horizontal in -1..=1 {
                for vertical in -1..=1 {
                    for depth in -1..=1 {
                        let Some(neighbor) = cell[0]
                            .checked_add(horizontal)
                            .zip(cell[1].checked_add(vertical))
                            .zip(cell[2].checked_add(depth))
                        else {
                            continue;
                        };
                        let key = [neighbor.0 .0, neighbor.0 .1, neighbor.1];
                        for &other in cells.get(&key).into_iter().flatten() {
                            comparisons += 1;
                            if comparisons > 16_000_000 || bonds.len() >= 4_000_000 {
                                return Err("Bond inference exceeds the density budget; provide explicit connectivity".into());
                            }
                            let other_atom = &self.atoms()[other];
                            let distance_squared: f64 = atom
                                .position
                                .iter()
                                .zip(other_atom.position)
                                .map(|(left, right)| (f64::from(*left) - f64::from(right)).powi(2))
                                .sum();
                            let cutoff =
                                1.2 * (radius + covalent_radius(&other_atom.element).unwrap());
                            if distance_squared >= 0.16 && distance_squared <= cutoff * cutoff {
                                bonds.push(MoleculeBond {
                                    atoms: [other, index],
                                    order: 1,
                                    inferred: true,
                                });
                            }
                        }
                    }
                }
            }
            cells.entry(cell).or_default().push(index);
        }
        Self::new(MoleculeData {
            atoms: self.atoms().to_vec(),
            bonds,
        })
    }

    pub fn from_json(source: &str) -> Result<Self, String> {
        if source.len() > 64 * 1024 * 1024 {
            return Err("Molecule JSON exceeds 64 MiB".into());
        }
        Self::new(serde_json::from_str(source).map_err(|error| error.to_string())?)
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(&*self.0).expect("validated atom coordinates are finite")
    }

    pub fn from_xyz(source: &str) -> Result<Self, String> {
        let mut lines = source.lines();
        let count: usize = lines
            .next()
            .ok_or("Missing XYZ atom count")?
            .trim()
            .parse()
            .map_err(|_| "Invalid XYZ atom count")?;
        if count > 1_000_000 {
            return Err("XYZ atom count exceeds the rendering budget".into());
        }
        lines.next().ok_or("Missing XYZ comment line")?;
        let mut atoms = Vec::with_capacity(count);
        for line in lines.by_ref().take(count) {
            let fields: Vec<_> = line.split_whitespace().collect();
            if fields.len() < 4 {
                return Err("XYZ atom line requires an element and x/y/z".into());
            }
            let coordinate = |position: usize| {
                fields[position]
                    .parse::<f32>()
                    .map_err(|_| "Invalid XYZ coordinate".to_string())
            };
            atoms.push(MoleculeAtom {
                element: fields[0].into(),
                position: [coordinate(1)?, coordinate(2)?, coordinate(3)?],
                label: String::new(),
            });
        }
        if atoms.len() != count {
            return Err("XYZ atom count does not match its coordinates".into());
        }
        Self::new(MoleculeData {
            atoms,
            bonds: Vec::new(),
        })?
        .infer_bonds()
    }

    pub fn from_pdb(source: &str) -> Result<Self, String> {
        let mut atoms = Vec::new();
        let mut serials = HashMap::new();
        let mut connections = Vec::new();
        let mut first_model_finished = false;
        for line in source.lines() {
            if line.starts_with("ENDMDL") {
                first_model_finished = true;
                continue;
            }
            if line.starts_with("ATOM  ") || line.starts_with("HETATM") {
                if first_model_finished {
                    continue;
                }
                if atoms.len() >= 1_000_000 {
                    return Err("PDB exceeds the atom budget".into());
                }
                let alternate = line.get(16..17).unwrap_or(" ");
                if alternate != " " && alternate != "A" {
                    continue;
                }
                let serial = line
                    .get(6..11)
                    .ok_or("Short PDB atom record")?
                    .trim()
                    .parse::<u32>()
                    .map_err(|_| "Invalid PDB atom serial")?;
                let coordinate = |range: std::ops::Range<usize>| -> Result<f32, String> {
                    line.get(range)
                        .ok_or("Short PDB coordinate record")?
                        .trim()
                        .parse()
                        .map_err(|_| "Invalid PDB coordinate".into())
                };
                let label = line.get(12..16).unwrap_or("").trim().to_string();
                let element = line.get(76..78).unwrap_or("").trim();
                let element = if element.is_empty() {
                    label
                        .chars()
                        .find(|character| character.is_ascii_alphabetic())
                        .ok_or("PDB atom lacks an element")?
                        .to_string()
                } else {
                    element.into()
                };
                serials.insert(serial, atoms.len());
                atoms.push(MoleculeAtom {
                    element,
                    position: [
                        coordinate(30..38)?,
                        coordinate(38..46)?,
                        coordinate(46..54)?,
                    ],
                    label,
                });
            } else if line.starts_with("CONECT") {
                let values: Result<Vec<u32>, _> = line
                    .get(6..)
                    .unwrap_or("")
                    .as_bytes()
                    .chunks(5)
                    .map(|field| std::str::from_utf8(field).unwrap_or("").trim())
                    .filter(|field| !field.is_empty())
                    .map(str::parse)
                    .collect();
                let values = values.map_err(|_| "Invalid PDB CONECT record")?;
                if let Some(source) = values.first() {
                    for target in &values[1..] {
                        connections.push((*source, *target));
                    }
                }
            }
        }
        let mut bonds = std::collections::BTreeSet::new();
        for (from, to) in connections {
            if let (Some(from), Some(to)) = (serials.get(&from), serials.get(&to)) {
                if from != to {
                    bonds.insert([(*from).min(*to), (*from).max(*to)]);
                }
            }
        }
        Self::new(MoleculeData {
            atoms,
            bonds: bonds
                .into_iter()
                .map(|atoms| MoleculeBond {
                    atoms,
                    order: 1,
                    inferred: false,
                })
                .collect(),
        })?
        .infer_bonds()
    }
}

fn covalent_radius(element: &str) -> Option<f64> {
    Some(match element.to_ascii_uppercase().as_str() {
        "H" | "D" => 0.31,
        "B" => 0.84,
        "C" => 0.76,
        "N" => 0.71,
        "O" => 0.66,
        "F" => 0.57,
        "SI" => 1.11,
        "P" => 1.07,
        "S" => 1.05,
        "CL" => 1.02,
        "BR" => 1.20,
        "I" => 1.39,
        _ => return None,
    })
}

#[cfg(test)]
#[path = "tests/molecule.rs"]
mod tests;
