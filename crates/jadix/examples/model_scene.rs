use jadix::primitives::scene3d::{jadix_3d::Scene, scene3d, Scene3dCamera, SceneView};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args_os()
        .nth(1)
        .ok_or("usage: model_scene <model.gltf|model.glb|avatar.vrm>")?;
    let view = SceneView::new(Scene::load(path)?);
    if !view.scene().animations.is_empty() {
        view.set_animation(Some(0))?;
    }
    jadix::app::app()
        .title("Jadix glTF / VRM")
        .size(900.0, 700.0)
        .run(move || {
            scene3d()
                .w("100%")
                .h("100%")
                .model(&view)
                .camera(Scene3dCamera {
                    fov_deg: 45.0,
                    pitch: 0.0,
                    distance: 3.0,
                })
                .background("#10141d")
                .into()
        });
    Ok(())
}
