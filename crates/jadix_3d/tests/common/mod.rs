use serde_json::json;

pub fn model(extension: Option<(&str, serde_json::Value)>) -> Vec<u8> {
    model_modified(extension, |_| {})
}

pub fn model_modified(
    extension: Option<(&str, serde_json::Value)>,
    modify: impl FnOnce(&mut serde_json::Value),
) -> Vec<u8> {
    let sections: Vec<Vec<u8>> = vec![
        floats(&[-0.5, -0.5, 0.0, 0.5, -0.5, 0.0, 0.0, 0.5, 0.0]),
        vec![0; 24],
        floats(&[1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0]),
        floats(&jadix_3d::Mat4::IDENTITY.to_cols_array()),
        floats(&[0.0, 1.0]),
        floats(&[0.0, 0.0, 0.0, 1.0, 0.0, 0.0]),
        floats(&[0.0, 0.25, 0.0, 0.0, 0.25, 0.0, 0.0, 0.25, 0.0]),
    ];
    let mut binary = Vec::new();
    let mut views = Vec::new();
    for section in sections {
        views.push(json!({"buffer":0,"byteOffset":binary.len(),"byteLength":section.len()}));
        binary.extend(section);
    }
    let mut document = json!({
        "asset":{"version":"2.0"}, "scene":0,
        "scenes":[{"name":"main","nodes":[0,1]},{"name":"other","nodes":[2]}],
        "nodes":[{"name":"mesh","mesh":0,"skin":0,"translation":[10.0,0.0,0.0]},
            {"name":"hips","children":[2]},{"name":"head","translation":[0.0,1.0,0.0]}],
        "meshes":[{"weights":[0.0],"primitives":[{"attributes":{"POSITION":0,"JOINTS_0":1,"WEIGHTS_0":2},"material":0,"targets":[{"POSITION":6}]}]}],
        "materials":[{"doubleSided":true,"pbrMetallicRoughness":{"baseColorFactor":[1.0,0.0,0.0,1.0]},"extensions":{"KHR_materials_unlit":{}}}],
        "extensionsUsed":["KHR_materials_unlit"],
        "skins":[{"joints":[1],"inverseBindMatrices":3}],
        "animations":[{"name":"move","samplers":[{"input":4,"output":5}],"channels":[{"sampler":0,"target":{"node":1,"path":"translation"}}]}],
        "buffers":[{"byteLength":binary.len()}],"bufferViews":views,
        "accessors":[
            {"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[-0.5,-0.5,0.0],"max":[0.5,0.5,0.0]},
            {"bufferView":1,"componentType":5123,"count":3,"type":"VEC4"},
            {"bufferView":2,"componentType":5126,"count":3,"type":"VEC4"},
            {"bufferView":3,"componentType":5126,"count":1,"type":"MAT4"},
            {"bufferView":4,"componentType":5126,"count":2,"type":"SCALAR","min":[0.0],"max":[1.0]},
            {"bufferView":5,"componentType":5126,"count":2,"type":"VEC3"},
            {"bufferView":6,"componentType":5126,"count":3,"type":"VEC3","min":[0.0,0.25,0.0],"max":[0.0,0.25,0.0]}
        ]
    });
    if let Some((name, value)) = extension {
        document["extensions"] = json!({name:value});
        document["extensionsUsed"]
            .as_array_mut()
            .unwrap()
            .push(json!(name));
        document["extensionsRequired"] = json!([name]);
    }
    modify(&mut document);
    let mut json = serde_json::to_vec(&document).unwrap();
    while json.len() % 4 != 0 {
        json.push(b' ');
    }
    let length = 12 + 8 + json.len() + 8 + binary.len();
    let mut output = Vec::new();
    for word in [
        0x46546c67u32,
        2,
        length as u32,
        json.len() as u32,
        0x4e4f534a,
    ] {
        output.extend(word.to_le_bytes());
    }
    output.extend(json);
    for word in [binary.len() as u32, 0x004e4942] {
        output.extend(word.to_le_bytes());
    }
    output.extend(binary);
    output
}

fn floats(values: &[f32]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}
