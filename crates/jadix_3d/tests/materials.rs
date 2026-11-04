mod common;
use jadix_3d::*;
use serde_json::{json, Value};

fn textured(document: &mut Value) {
    document["images"] = json!([{"uri":"data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGP4////fwAJ+wP9KobjigAAAABJRU5ErkJggg=="}]);
    document["samplers"] = json!([
        {"magFilter":9728,"minFilter":9728,"wrapS":33071,"wrapT":33648},
        {"magFilter":9729,"minFilter":9729,"wrapS":10497,"wrapT":10497}
    ]);
    document["textures"] = json!([{"source":0,"sampler":0},{"source":0,"sampler":1}]);
    let attributes = &mut document["meshes"][0]["primitives"][0]["attributes"];
    attributes["TEXCOORD_0"] = json!(7);
    document["accessors"]
        .as_array_mut()
        .unwrap()
        .push(json!({"bufferView":5,"componentType":5126,"count":3,"type":"VEC2"}));
    let material = &mut document["materials"][0];
    material["pbrMetallicRoughness"]["baseColorTexture"] = json!({"index":0});
    material["pbrMetallicRoughness"]["metallicRoughnessTexture"] = json!({"index":1});
    material["normalTexture"] = json!({"index":0,"scale":0.3});
    material["occlusionTexture"] = json!({"index":1,"strength":0.4});
    material["emissiveTexture"] = json!({"index":1});
    material["emissiveFactor"] = json!([0.1, 0.2, 0.3]);
}

#[test]
fn all_material_slots_preserve_factors_and_independent_samplers() {
    assert!(Scene::from_slice(&common::model(None)).is_ok());
    let scene = Scene::from_slice(&common::model_modified(None, textured)).unwrap();
    assert_eq!(scene.images[0].rgba, [255, 255, 255, 255]);
    let material = &scene.meshes[0].primitives[0].material;
    assert_eq!(material.base_color_texture, Some(0));
    assert_eq!(material.normal_scale, 0.3);
    assert_eq!(material.occlusion_strength, 0.4);
    assert_eq!(material.emissive, Vec3::new(0.1, 0.2, 0.3));
    assert_eq!(
        material.sampler.address_mode_u,
        wgpu::AddressMode::ClampToEdge
    );
    assert_eq!(
        material.sampler.address_mode_v,
        wgpu::AddressMode::MirrorRepeat
    );
    assert_eq!(material.sampler.mag_filter, wgpu::FilterMode::Nearest);
    for map in [
        &material.metallic_roughness_texture,
        &material.occlusion_texture,
        &material.emissive_texture,
    ] {
        let map = map.as_ref().unwrap();
        assert_eq!(map.image, 0);
        assert_eq!(map.sampler.mag_filter, wgpu::FilterMode::Linear);
        assert_eq!(map.sampler.address_mode_u, wgpu::AddressMode::Repeat);
    }
    assert_eq!(
        material.normal_texture.as_ref().unwrap().sampler.mag_filter,
        wgpu::FilterMode::Nearest
    );
}

#[test]
fn missing_or_unsupported_uv_sets_are_reported_not_silently_sampled() {
    let missing = common::model_modified(None, |document| {
        textured(document);
        document["meshes"][0]["primitives"][0]["attributes"]
            .as_object_mut()
            .unwrap()
            .remove("TEXCOORD_0");
    });
    assert!(Scene::from_slice(&missing)
        .unwrap_err()
        .0
        .contains("TEXCOORD_0"));
    let unsupported = common::model_modified(None, |document| {
        textured(document);
        document["materials"][0]["normalTexture"]["texCoord"] = json!(1);
    });
    assert!(Scene::from_slice(&unsupported)
        .unwrap_err()
        .0
        .contains("TEXCOORD_0"));
}
