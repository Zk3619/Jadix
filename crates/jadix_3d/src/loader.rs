use crate::*;

pub(crate) fn load(path: &Path) -> Result<Scene> {
    let bytes = std::fs::read(path).map_err(|error| Error(error.to_string()))?;
    import(&bytes, path.parent())
}

pub(crate) fn from_slice(bytes: &[u8]) -> Result<Scene> {
    import(bytes, None)
}

fn import(bytes: &[u8], base: Option<&Path>) -> Result<Scene> {
    let asset = gltf::Gltf::from_slice_without_validation(bytes)
        .map_err(|error| Error(error.to_string()))?;
    let mut validation = asset.document.as_json().clone();
    validation
        .extensions_required
        .retain(|name| name != "VRM" && name != "VRMC_vrm");
    gltf::Document::from_json(validation).map_err(|error| Error(error.to_string()))?;
    let buffers = gltf::import_buffers(&asset.document, base, asset.blob)
        .map_err(|error| Error(error.to_string()))?;
    let images = asset
        .document
        .images()
        .map(|image| {
            let source = image.source();
            let image_base = match source {
                gltf::image::Source::Uri { uri, .. } if uri.starts_with("data:") => {
                    Some(base.unwrap_or(Path::new(".")))
                }
                _ => base,
            };
            gltf::image::Data::from_source(source, image_base, &buffers)
        })
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|error| Error(error.to_string()))?;
    decode(asset.document, buffers, images)
}

fn decode(
    document: gltf::Document,
    buffers: Vec<gltf::buffer::Data>,
    images: Vec<gltf::image::Data>,
) -> Result<Scene> {
    let mut meshes = Vec::new();
    for mesh in document.meshes() {
        let mut primitives = Vec::new();
        for primitive in mesh.primitives() {
            if primitive.get(&gltf::Semantic::Joints(1)).is_some()
                || primitive.get(&gltf::Semantic::Weights(1)).is_some()
            {
                return Err(Error(
                    "more than four joint influences per vertex are unsupported".into(),
                ));
            }
            let reader = primitive
                .reader(|buffer| buffers.get(buffer.index()).map(|data| data.0.as_slice()));
            let positions: Vec<_> = reader
                .read_positions()
                .ok_or_else(|| Error("primitive has no POSITION accessor".into()))?
                .map(Vec3::from_array)
                .collect();
            let normals: Vec<_> = reader
                .read_normals()
                .map(|values| values.map(Vec3::from_array).collect())
                .unwrap_or_default();
            let uv: Vec<_> = reader
                .read_tex_coords(0)
                .map(|values| values.into_f32().map(Vec2::from_array).collect())
                .unwrap_or_default();
            let colors: Vec<_> = reader
                .read_colors(0)
                .map(|values| values.into_rgba_f32().map(Vec4::from_array).collect())
                .unwrap_or_default();
            let joints: Vec<_> = reader
                .read_joints(0)
                .map(|values| values.into_u16().collect())
                .unwrap_or_default();
            let weights: Vec<_> = reader
                .read_weights(0)
                .map(|values| values.into_f32().map(Vec4::from_array).collect())
                .unwrap_or_default();
            if joints.is_empty() != weights.is_empty()
                || weights
                    .iter()
                    .any(|weights| !weights.is_finite() || weights.min_element() < 0.0)
            {
                return Err(Error("invalid joint/weight attributes".into()));
            }
            let raw_indices: Vec<_> = reader
                .read_indices()
                .map(|values| values.into_u32().collect())
                .unwrap_or_else(|| (0..positions.len() as u32).collect());
            if raw_indices
                .iter()
                .any(|index| *index as usize >= positions.len())
            {
                return Err(Error("primitive index exceeds vertex count".into()));
            }
            let indices = triangles(primitive.mode(), &raw_indices)?;
            for count in [
                normals.len(),
                uv.len(),
                colors.len(),
                joints.len(),
                weights.len(),
            ] {
                if count != 0 && count != positions.len() {
                    return Err(Error("mismatched vertex attribute counts".into()));
                }
            }
            let mut vertices: Vec<_> = positions
                .into_iter()
                .enumerate()
                .map(|(index, position)| Vertex {
                    position,
                    normal: normals.get(index).copied().unwrap_or(Vec3::ZERO),
                    uv: uv.get(index).copied().unwrap_or(Vec2::ZERO),
                    color: colors.get(index).copied().unwrap_or(Vec4::ONE),
                    joints: joints.get(index).copied().unwrap_or([0; 4]),
                    weights: weights.get(index).copied().unwrap_or(Vec4::ZERO),
                })
                .collect();
            if normals.is_empty() {
                for triangle in indices.chunks_exact(3) {
                    let normal = (vertices[triangle[1] as usize].position
                        - vertices[triangle[0] as usize].position)
                        .cross(
                            vertices[triangle[2] as usize].position
                                - vertices[triangle[0] as usize].position,
                        );
                    for index in triangle {
                        vertices[*index as usize].normal += normal;
                    }
                }
                for vertex in &mut vertices {
                    vertex.normal = vertex.normal.normalize_or_zero();
                }
            }
            let morph_targets = reader
                .read_morph_targets()
                .map(|(positions, normals, _)| MorphTarget {
                    positions: positions
                        .map(|values| values.map(Vec3::from_array).collect())
                        .unwrap_or_default(),
                    normals: normals
                        .map(|values| values.map(Vec3::from_array).collect())
                        .unwrap_or_default(),
                })
                .collect::<Vec<_>>();
            for morph in &morph_targets {
                if (!morph.positions.is_empty() && morph.positions.len() != vertices.len())
                    || (!morph.normals.is_empty() && morph.normals.len() != vertices.len())
                {
                    return Err(Error("morph target vertex count mismatch".into()));
                }
            }
            let source = primitive.material();
            let pbr = source.pbr_metallic_roughness();
            let metallic_roughness = pbr.metallic_roughness_texture();
            let normal = source.normal_texture();
            let occlusion = source.occlusion_texture();
            let emissive = source.emissive_texture();
            if metallic_roughness
                .as_ref()
                .is_some_and(|info| info.tex_coord() != 0)
                || normal.as_ref().is_some_and(|info| info.tex_coord() != 0)
                || occlusion.as_ref().is_some_and(|info| info.tex_coord() != 0)
                || emissive.as_ref().is_some_and(|info| info.tex_coord() != 0)
            {
                return Err(Error(
                    "material textures currently require TEXCOORD_0".into(),
                ));
            }
            if uv.is_empty()
                && (pbr.base_color_texture().is_some()
                    || metallic_roughness.is_some()
                    || normal.is_some()
                    || occlusion.is_some()
                    || emissive.is_some())
            {
                return Err(Error(
                    "textured primitives require TEXCOORD_0 attributes".into(),
                ));
            }
            let texture_map = |texture: gltf::Texture<'_>| TextureMap {
                image: texture.source().index(),
                sampler: sampler(texture.sampler()),
            };
            if pbr
                .base_color_texture()
                .is_some_and(|info| info.tex_coord() != 0)
            {
                return Err(Error(
                    "base color textures currently require TEXCOORD_0".into(),
                ));
            }
            let material = Material {
                base_color: Vec4::from_array(pbr.base_color_factor()),
                metallic_roughness_texture: metallic_roughness
                    .map(|info| texture_map(info.texture())),
                normal_scale: normal.as_ref().map_or(1.0, |info| info.scale()),
                normal_texture: normal.map(|info| texture_map(info.texture())),
                occlusion_strength: occlusion.as_ref().map_or(1.0, |info| info.strength()),
                occlusion_texture: occlusion.map(|info| texture_map(info.texture())),
                emissive_texture: emissive.map(|info| texture_map(info.texture())),
                base_color_texture: pbr
                    .base_color_texture()
                    .map(|info| info.texture().source().index()),
                sampler: pbr
                    .base_color_texture()
                    .map(|info| sampler(info.texture().sampler()))
                    .unwrap_or_else(|| Material::default().sampler),
                metallic: pbr.metallic_factor(),
                roughness: pbr.roughness_factor(),
                emissive: Vec3::from_array(source.emissive_factor()),
                alpha: match source.alpha_mode() {
                    gltf::material::AlphaMode::Opaque => AlphaMode::Opaque,
                    gltf::material::AlphaMode::Mask => {
                        AlphaMode::Mask(source.alpha_cutoff().unwrap_or(0.5))
                    }
                    gltf::material::AlphaMode::Blend => AlphaMode::Blend,
                },
                double_sided: source.double_sided(),
                unlit: source.unlit(),
            };
            primitives.push(Primitive {
                vertices,
                indices,
                morph_targets,
                material,
            });
        }
        let target_count = primitives
            .first()
            .map(|primitive| primitive.morph_targets.len())
            .unwrap_or(0);
        meshes.push(Mesh {
            name: mesh.name().map(str::to_owned),
            primitives,
            weights: mesh
                .weights()
                .map(<[f32]>::to_vec)
                .unwrap_or_else(|| vec![0.0; target_count]),
        });
    }
    let nodes: Vec<_> = document
        .nodes()
        .map(|node| {
            let (translation, rotation, scale) = node.transform().decomposed();
            Node {
                name: node.name().map(str::to_owned),
                children: node.children().map(|child| child.index()).collect(),
                transform: Transform {
                    translation: Vec3::from_array(translation),
                    rotation: Quat::from_array(rotation),
                    scale: Vec3::from_array(scale),
                },
                mesh: node.mesh().map(|mesh| mesh.index()),
                skin: node.skin().map(|skin| skin.index()),
                weights: node.weights().map(<[f32]>::to_vec).unwrap_or_default(),
                camera: node.camera().map(|camera| match camera.projection() {
                    gltf::camera::Projection::Perspective(value) => Projection::Perspective {
                        fov_y: value.yfov(),
                        near: value.znear(),
                        far: value.zfar().unwrap_or(10000.0),
                    },
                    gltf::camera::Projection::Orthographic(value) => Projection::Orthographic {
                        xmag: value.xmag(),
                        ymag: value.ymag(),
                        near: value.znear(),
                        far: value.zfar(),
                    },
                }),
                light: node.light().map(|light| Light {
                    color: Vec3::from_array(light.color()),
                    intensity: light.intensity(),
                    range: light.range(),
                    kind: match light.kind() {
                        gltf::khr_lights_punctual::Kind::Directional => LightKind::Directional,
                        gltf::khr_lights_punctual::Kind::Point => LightKind::Point,
                        gltf::khr_lights_punctual::Kind::Spot {
                            inner_cone_angle,
                            outer_cone_angle,
                        } => LightKind::Spot {
                            inner: inner_cone_angle,
                            outer: outer_cone_angle,
                        },
                    },
                }),
            }
        })
        .collect();
    let roots = document
        .default_scene()
        .or_else(|| document.scenes().next())
        .map(|scene| scene.nodes().map(|node| node.index()).collect())
        .unwrap_or_else(|| {
            let children: std::collections::HashSet<_> = nodes
                .iter()
                .flat_map(|node| node.children.iter().copied())
                .collect();
            (0..nodes.len())
                .filter(|index| !children.contains(index))
                .collect()
        });
    let skins = document
        .skins()
        .map(|skin| {
            let joints: Vec<_> = skin.joints().map(|joint| joint.index()).collect();
            let reader =
                skin.reader(|buffer| buffers.get(buffer.index()).map(|data| data.0.as_slice()));
            let inverse_bind_matrices = reader
                .read_inverse_bind_matrices()
                .map(|values| {
                    values
                        .map(|matrix| Mat4::from_cols_array_2d(&matrix))
                        .collect()
                })
                .unwrap_or_else(|| vec![Mat4::IDENTITY; joints.len()]);
            Skin {
                joints,
                inverse_bind_matrices,
            }
        })
        .collect();
    let mut animations = Vec::new();
    for animation in document.animations() {
        let mut channels = Vec::new();
        let mut duration = 0.0f32;
        for channel in animation.channels() {
            let reader =
                channel.reader(|buffer| buffers.get(buffer.index()).map(|data| data.0.as_slice()));
            let times: Vec<_> = reader
                .read_inputs()
                .ok_or_else(|| Error("missing animation times".into()))?
                .collect();
            if let Some(time) = times.last() {
                duration = duration.max(*time);
            }
            let interpolation = match channel.sampler().interpolation() {
                gltf::animation::Interpolation::Step => Interpolation::Step,
                gltf::animation::Interpolation::Linear => Interpolation::Linear,
                gltf::animation::Interpolation::CubicSpline => Interpolation::CubicSpline,
            };
            let output = reader
                .read_outputs()
                .ok_or_else(|| Error("missing animation outputs".into()))?;
            use gltf::animation::util::ReadOutputs;
            let (property, components, values) = match output {
                ReadOutputs::Translations(values) => {
                    (Property::Translation, 3, values.flatten().collect())
                }
                ReadOutputs::Scales(values) => (Property::Scale, 3, values.flatten().collect()),
                ReadOutputs::Rotations(values) => {
                    (Property::Rotation, 4, values.into_f32().flatten().collect())
                }
                ReadOutputs::MorphTargetWeights(values) => {
                    let values: Vec<_> = values.into_f32().collect();
                    let keys = times.len()
                        * if matches!(interpolation, Interpolation::CubicSpline) {
                            3
                        } else {
                            1
                        };
                    let components = if keys > 0 { values.len() / keys } else { 0 };
                    (Property::Weights, components, values)
                }
            };
            channels.push(Channel {
                node: channel.target().node().index(),
                property,
                components,
                values,
                times,
                interpolation,
            });
        }
        animations.push(Animation {
            name: animation.name().map(str::to_owned),
            duration,
            channels,
        });
    }
    let json =
        serde_json::to_value(document.as_json()).map_err(|error| Error(error.to_string()))?;
    let extensions = json
        .get("extensions")
        .and_then(|value| value.as_object())
        .cloned()
        .unwrap_or_default();
    let vrm = if extensions.contains_key("VRM") || extensions.contains_key("VRMC_vrm") {
        let mut human_bones = std::collections::BTreeMap::new();
        if let Some(bones) = extensions
            .get("VRMC_vrm")
            .and_then(|value| value.pointer("/humanoid/humanBones"))
            .and_then(|value| value.as_object())
        {
            for (name, bone) in bones {
                if let Some(index) = bone.get("node").and_then(|value| value.as_u64()) {
                    human_bones.insert(name.clone(), index as usize);
                }
            }
        }
        if let Some(bones) = extensions
            .get("VRM")
            .and_then(|value| value.pointer("/humanoid/humanBones"))
            .and_then(|value| value.as_array())
        {
            for bone in bones {
                if let (Some(name), Some(index)) = (
                    bone.get("bone").and_then(|value| value.as_str()),
                    bone.get("node").and_then(|value| value.as_u64()),
                ) {
                    human_bones.insert(name.into(), index as usize);
                }
            }
        }
        if human_bones.values().any(|index| *index >= nodes.len()) {
            return Err(Error("VRM human bone node out of range".into()));
        }
        Some(Vrm {
            extensions,
            human_bones,
        })
    } else {
        None
    };
    let images = images.into_iter().map(image).collect::<Result<Vec<_>>>()?;
    let scenes = document
        .scenes()
        .map(|scene| SceneDefinition {
            name: scene.name().map(str::to_owned),
            roots: scene.nodes().map(|node| node.index()).collect(),
        })
        .collect();
    let scene = Scene {
        scenes,
        nodes,
        roots,
        meshes,
        skins,
        images,
        animations,
        vrm,
    };
    scene.pose(None, 0.0, false)?;
    for index in 0..scene.animations.len() {
        scene.pose(Some(index), 0.0, false)?;
    }
    Ok(scene)
}

fn triangles(mode: gltf::mesh::Mode, indices: &[u32]) -> Result<Vec<u32>> {
    use gltf::mesh::Mode;
    match mode {
        Mode::Triangles if indices.len() % 3 == 0 => Ok(indices.to_vec()),
        Mode::TriangleStrip => Ok(indices
            .windows(3)
            .enumerate()
            .flat_map(|(index, triple)| {
                if index % 2 == 0 {
                    [triple[0], triple[1], triple[2]]
                } else {
                    [triple[1], triple[0], triple[2]]
                }
            })
            .collect()),
        Mode::TriangleFan => Ok((1..indices.len().saturating_sub(1))
            .flat_map(|index| [indices[0], indices[index], indices[index + 1]])
            .collect()),
        _ => Err(Error(
            "3D renderer supports triangle primitives only".into(),
        )),
    }
}

fn sampler(source: gltf::texture::Sampler<'_>) -> wgpu::SamplerDescriptor<'static> {
    use gltf::texture::{MagFilter, MinFilter, WrappingMode};
    let wrap = |mode| match mode {
        WrappingMode::ClampToEdge => wgpu::AddressMode::ClampToEdge,
        WrappingMode::MirroredRepeat => wgpu::AddressMode::MirrorRepeat,
        WrappingMode::Repeat => wgpu::AddressMode::Repeat,
    };
    wgpu::SamplerDescriptor {
        address_mode_u: wrap(source.wrap_s()),
        address_mode_v: wrap(source.wrap_t()),
        mag_filter: if source.mag_filter() == Some(MagFilter::Nearest) {
            wgpu::FilterMode::Nearest
        } else {
            wgpu::FilterMode::Linear
        },
        min_filter: match source.min_filter() {
            Some(
                MinFilter::Nearest
                | MinFilter::NearestMipmapNearest
                | MinFilter::NearestMipmapLinear,
            ) => wgpu::FilterMode::Nearest,
            _ => wgpu::FilterMode::Linear,
        },
        ..Default::default()
    }
}

fn image(data: gltf::image::Data) -> Result<Image> {
    use gltf::image::Format::*;
    let (channels, bytes) = match data.format {
        R8 => (1, 1),
        R8G8 => (2, 1),
        R8G8B8 => (3, 1),
        R8G8B8A8 => (4, 1),
        R16 => (1, 2),
        R16G16 => (2, 2),
        R16G16B16 => (3, 2),
        R16G16B16A16 => (4, 2),
        R32G32B32FLOAT => (3, 4),
        R32G32B32A32FLOAT => (4, 4),
    };
    let pixels = data.width as usize * data.height as usize;
    if data.pixels.len() != pixels * channels * bytes {
        return Err(Error("invalid decoded image size".into()));
    }
    let mut rgba = Vec::with_capacity(pixels * 4);
    for pixel in data.pixels.chunks_exact(channels * bytes) {
        let sample = |channel: usize| -> u8 {
            let start = channel * bytes;
            match bytes {
                1 => pixel[start],
                2 => (u16::from_ne_bytes([pixel[start], pixel[start + 1]]) / 257) as u8,
                _ => (f32::from_ne_bytes(pixel[start..start + 4].try_into().unwrap())
                    .clamp(0.0, 1.0)
                    * 255.0)
                    .round() as u8,
            }
        };
        let red = sample(0);
        let green = if channels >= 3 { sample(1) } else { red };
        let blue = if channels >= 3 { sample(2) } else { red };
        let alpha = if channels == 2 {
            sample(1)
        } else if channels == 4 {
            sample(3)
        } else {
            255
        };
        rgba.extend_from_slice(&[red, green, blue, alpha]);
    }
    Ok(Image {
        width: data.width,
        height: data.height,
        rgba,
    })
}
