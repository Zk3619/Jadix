use crate::*;

pub(crate) fn pose(scene: &Scene, clip: Option<usize>, seconds: f32, repeat: bool) -> Result<Pose> {
    evaluate(scene, local_pose(scene, clip, seconds, repeat)?)
}

pub(crate) fn local_pose(
    scene: &Scene,
    clip: Option<usize>,
    seconds: f32,
    repeat: bool,
) -> Result<LocalPose> {
    if !seconds.is_finite() {
        return Err(Error("animation time must be finite".into()));
    }
    let mut transforms: Vec<_> = scene.nodes.iter().map(|node| node.transform).collect();
    let mut weights: Vec<_> = scene
        .nodes
        .iter()
        .map(|node| {
            if !node.weights.is_empty() {
                node.weights.clone()
            } else {
                node.mesh
                    .and_then(|index| scene.meshes.get(index))
                    .map(|mesh| mesh.weights.clone())
                    .unwrap_or_default()
            }
        })
        .collect();
    if let Some(index) = clip {
        let animation = scene
            .animations
            .get(index)
            .ok_or_else(|| Error("animation index out of range".into()))?;
        if !animation.duration.is_finite() || animation.duration < 0.0 {
            return Err(Error("invalid animation duration".into()));
        }
        let time = if repeat && animation.duration > 0.0 {
            seconds.rem_euclid(animation.duration)
        } else {
            seconds.clamp(0.0, animation.duration.max(0.0))
        };
        for channel in &animation.channels {
            let transform = transforms
                .get_mut(channel.node)
                .ok_or_else(|| Error("animation node out of range".into()))?;
            let values = sample(channel, time)?;
            match channel.property {
                Property::Translation => transform.translation = Vec3::from_slice(&values),
                Property::Scale => transform.scale = Vec3::from_slice(&values),
                Property::Rotation => {
                    transform.rotation = Quat::from_array(
                        values
                            .try_into()
                            .map_err(|_| Error("invalid rotation channel".into()))?,
                    )
                    .normalize()
                }
                Property::Weights => weights[channel.node] = values,
            }
        }
    }
    Ok(LocalPose {
        transforms,
        weights,
    })
}

pub(crate) fn evaluate(scene: &Scene, local: LocalPose) -> Result<Pose> {
    let LocalPose {
        transforms,
        weights,
    } = local;
    let count = scene.nodes.len();
    if transforms.len() != count
        || weights.len() != count
        || transforms.iter().any(|value| {
            !value.translation.is_finite()
                || !value.scale.is_finite()
                || !value.rotation.is_finite()
                || (value.rotation.length_squared() - 1.0).abs() > 1e-3
        })
        || weights.iter().flatten().any(|weight| !weight.is_finite())
    {
        return Err(Error("invalid local pose or scene mismatch".into()));
    }
    let mut parents = vec![None; count];
    for (parent, node) in scene.nodes.iter().enumerate() {
        for &child in &node.children {
            let slot = parents
                .get_mut(child)
                .ok_or_else(|| Error("child index out of range".into()))?;
            if slot.replace(parent).is_some() {
                return Err(Error("scene node has multiple parents".into()));
            }
        }
    }
    let mut world = vec![Mat4::IDENTITY; count];
    let mut stack: Vec<_> = parents
        .iter()
        .enumerate()
        .filter(|(_, parent)| parent.is_none())
        .map(|(index, _)| index)
        .collect();
    let mut visited = 0;
    while let Some(index) = stack.pop() {
        world[index] = parents[index]
            .map(|parent| world[parent])
            .unwrap_or(Mat4::IDENTITY)
            * transforms[index].matrix();
        stack.extend(&scene.nodes[index].children);
        visited += 1;
    }
    if visited != count {
        return Err(Error("scene hierarchy contains a cycle".into()));
    }
    let mut visible = vec![false; count];
    stack.extend(&scene.roots);
    while let Some(index) = stack.pop() {
        let entry = visible
            .get_mut(index)
            .ok_or_else(|| Error("root node out of range".into()))?;
        if *entry {
            continue;
        }
        *entry = true;
        stack.extend(&scene.nodes[index].children);
    }
    Ok(Pose {
        world,
        weights,
        visible,
    })
}

fn sample(channel: &Channel, time: f32) -> Result<Vec<f32>> {
    let components = channel.components;
    let expected_components = match channel.property {
        Property::Translation | Property::Scale => 3,
        Property::Rotation => 4,
        Property::Weights => components,
    };
    let multiplier = if matches!(channel.interpolation, Interpolation::CubicSpline) {
        3
    } else {
        1
    };
    if components == 0
        || components != expected_components
        || channel.times.is_empty()
        || channel.values.len() != channel.times.len() * components * multiplier
        || channel.times.iter().any(|value| !value.is_finite())
        || channel.times.windows(2).any(|pair| pair[1] <= pair[0])
        || channel.values.iter().any(|value| !value.is_finite())
    {
        return Err(Error("invalid animation channel samples".into()));
    }
    let right = channel.times.partition_point(|key| *key <= time);
    let left = right.saturating_sub(1);
    let right = right.min(channel.times.len() - 1);
    let value = |key: usize| -> &[f32] {
        let start = (key * multiplier + usize::from(multiplier == 3)) * components;
        &channel.values[start..start + components]
    };
    if left == right || matches!(channel.interpolation, Interpolation::Step) {
        if matches!(channel.property, Property::Rotation)
            && Quat::from_slice(value(left)).length_squared() < 1e-10
        {
            return Err(Error("zero animation quaternion".into()));
        }
        return Ok(value(left).to_vec());
    }
    let duration = channel.times[right] - channel.times[left];
    let factor = ((time - channel.times[left]) / duration).clamp(0.0, 1.0);
    let start = value(left);
    let end = value(right);
    if matches!(channel.property, Property::Rotation)
        && matches!(channel.interpolation, Interpolation::Linear)
    {
        let start = Quat::from_slice(start);
        let end = Quat::from_slice(end);
        if start.length_squared() < 1e-10 || end.length_squared() < 1e-10 {
            return Err(Error("zero animation quaternion".into()));
        }
        return Ok(start
            .normalize()
            .slerp(end.normalize(), factor)
            .to_array()
            .to_vec());
    }
    let mut output = Vec::with_capacity(components);
    for component in 0..components {
        output.push(if multiplier == 3 {
            let outgoing = channel.values[(left * 3 + 2) * components + component] * duration;
            let incoming = channel.values[right * 3 * components + component] * duration;
            let squared = factor * factor;
            let cubed = squared * factor;
            (2.0 * cubed - 3.0 * squared + 1.0) * start[component]
                + (cubed - 2.0 * squared + factor) * outgoing
                + (-2.0 * cubed + 3.0 * squared) * end[component]
                + (cubed - squared) * incoming
        } else {
            start[component] + (end[component] - start[component]) * factor
        });
    }
    if matches!(channel.property, Property::Rotation) {
        let rotation = Quat::from_slice(&output);
        if rotation.length_squared() < 1e-10 {
            return Err(Error("zero animation quaternion".into()));
        }
        output.copy_from_slice(&rotation.normalize().to_array());
    }
    Ok(output)
}
