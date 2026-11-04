use crate::*;

impl Scene {
    pub fn humanoid_bone(&self, name: &str) -> Option<usize> {
        self.vrm.as_ref()?.human_bones.get(name).copied()
    }

    /// Apply a VRM 0.x blend-shape group or VRM 1.0 expression to local morph weights.
    /// Calls add contributions; start from a fresh sampled pose each frame.
    /// Material-color/UV bindings and override policies are retained as metadata only.
    pub fn apply_expression(&self, pose: &mut LocalPose, name: &str, weight: f32) -> Result<()> {
        if !weight.is_finite()
            || !(0.0..=1.0).contains(&weight)
            || pose.weights.len() != self.nodes.len()
        {
            return Err(Error("invalid expression weight or pose".into()));
        }
        let vrm = self
            .vrm
            .as_ref()
            .ok_or_else(|| Error("scene is not VRM".into()))?;
        let mut updates = Vec::new();
        if let Some(expressions) = vrm
            .extensions
            .get("VRMC_vrm")
            .and_then(|value| value.get("expressions"))
        {
            let expression = ["preset", "custom"]
                .iter()
                .find_map(|kind| expressions.get(kind)?.get(name))
                .ok_or_else(|| Error(format!("unknown VRM expression: {name}")))?;
            let weight = if expression
                .get("isBinary")
                .and_then(|value| value.as_bool())
                .unwrap_or(false)
            {
                if weight > 0.5 {
                    1.0
                } else {
                    0.0
                }
            } else {
                weight
            };
            if let Some(bindings) = expression
                .get("morphTargetBinds")
                .and_then(|value| value.as_array())
            {
                for bind in bindings {
                    updates.push((
                        integer(bind, "node")?,
                        integer(bind, "index")?,
                        scalar(bind, "weight")? * weight,
                    ));
                }
            }
        } else {
            let expression = vrm
                .extensions
                .get("VRM")
                .and_then(|value| value.pointer("/blendShapeMaster/blendShapeGroups"))
                .and_then(|value| value.as_array())
                .and_then(|groups| {
                    groups.iter().find(|group| {
                        group.get("name").and_then(|value| value.as_str()) == Some(name)
                            || group.get("presetName").and_then(|value| value.as_str())
                                == Some(name)
                    })
                })
                .ok_or_else(|| Error(format!("unknown VRM expression: {name}")))?;
            let weight = if expression
                .get("isBinary")
                .and_then(|value| value.as_bool())
                .unwrap_or(false)
            {
                if weight > 0.5 {
                    1.0
                } else {
                    0.0
                }
            } else {
                weight
            };
            if let Some(bindings) = expression.get("binds").and_then(|value| value.as_array()) {
                for bind in bindings {
                    let mesh = integer(bind, "mesh")?;
                    if mesh >= self.meshes.len() {
                        return Err(Error("expression mesh out of range".into()));
                    }
                    for (node, _) in self
                        .nodes
                        .iter()
                        .enumerate()
                        .filter(|(_, node)| node.mesh == Some(mesh))
                    {
                        updates.push((
                            node,
                            integer(bind, "index")?,
                            scalar(bind, "weight")? * 0.01 * weight,
                        ));
                    }
                }
            }
        }
        for &(node, index, value) in &updates {
            let mesh = self
                .nodes
                .get(node)
                .and_then(|node| node.mesh)
                .and_then(|mesh| self.meshes.get(mesh))
                .ok_or_else(|| Error("expression node has no mesh".into()))?;
            if !value.is_finite()
                || mesh
                    .primitives
                    .iter()
                    .any(|primitive| index >= primitive.morph_targets.len())
            {
                return Err(Error("invalid expression morph binding".into()));
            }
        }
        for (node, index, value) in updates {
            let length = pose.weights[node].len().max(index + 1);
            pose.weights[node].resize(length, 0.0);
            pose.weights[node][index] += value;
        }
        Ok(())
    }

    /// CCD inverse kinematics, rotating the ancestors of an end effector toward a world target.
    /// Returns the remaining world-space distance. Work is bounded to 128 iterations.
    pub fn solve_ik(
        &self,
        pose: &mut LocalPose,
        end: usize,
        ancestors: usize,
        target: Vec3,
        iterations: usize,
        tolerance: f32,
    ) -> Result<f32> {
        if end >= self.nodes.len()
            || ancestors == 0
            || iterations == 0
            || iterations > 128
            || !target.is_finite()
            || !tolerance.is_finite()
            || tolerance < 0.0
        {
            return Err(Error("invalid IK chain or target".into()));
        }
        let mut local = pose.clone();
        self.evaluate_pose(local.clone())?;
        let mut parents = vec![None; self.nodes.len()];
        for (parent, node) in self.nodes.iter().enumerate() {
            for &child in &node.children {
                parents[child] = Some(parent);
            }
        }
        let mut distance = f32::INFINITY;
        for _ in 0..iterations {
            let mut joint = parents[end];
            for _ in 0..ancestors {
                let Some(index) = joint else {
                    break;
                };
                let evaluated = self.evaluate_pose(local.clone())?;
                let tip = evaluated.world[end].transform_point3(Vec3::ZERO);
                distance = tip.distance(target);
                if distance <= tolerance {
                    *pose = local;
                    return Ok(distance);
                }
                let parent_world = parents[index]
                    .map(|parent| evaluated.world[parent])
                    .unwrap_or(Mat4::IDENTITY);
                if parent_world.determinant().abs() < 1e-10 {
                    return Err(Error("singular IK parent".into()));
                }
                let inverse = parent_world.inverse();
                let pivot = local.transforms[index].translation;
                let from = inverse.transform_point3(tip) - pivot;
                let to = inverse.transform_point3(target) - pivot;
                if from.length_squared() > 1e-10 && to.length_squared() > 1e-10 {
                    let delta = Quat::from_rotation_arc(from.normalize(), to.normalize());
                    local.transforms[index].rotation =
                        (delta * local.transforms[index].rotation).normalize();
                }
                joint = parents[index];
            }
        }
        if !distance.is_finite() {
            return Err(Error("IK end effector has no ancestors".into()));
        }
        let evaluated = self.evaluate_pose(local.clone())?;
        distance = evaluated.world[end]
            .transform_point3(Vec3::ZERO)
            .distance(target);
        *pose = local;
        Ok(distance)
    }
}

fn integer(value: &serde_json::Value, name: &str) -> Result<usize> {
    value
        .get(name)
        .and_then(|value| value.as_u64())
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| Error(format!("invalid expression {name}")))
}
fn scalar(value: &serde_json::Value, name: &str) -> Result<f32> {
    value
        .get(name)
        .and_then(|value| value.as_f64())
        .filter(|value| value.is_finite())
        .map(|value| value as f32)
        .ok_or_else(|| Error(format!("invalid expression {name}")))
}
