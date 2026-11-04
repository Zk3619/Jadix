fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("usage: inspect model.gltf|model.glb|avatar.vrm")?;
    let scene = jadix_3d::Scene::load(path)?;
    println!(
        "{} nodes, {} meshes, {} skins, {} animations",
        scene.nodes.len(),
        scene.meshes.len(),
        scene.skins.len(),
        scene.animations.len()
    );
    if let Some(vrm) = &scene.vrm {
        println!("VRM humanoid: {} bones", vrm.human_bones.len());
    }
    for (index, clip) in scene.animations.iter().enumerate() {
        println!("clip {index}: {:?}, {} seconds", clip.name, clip.duration);
    }
    Ok(())
}
