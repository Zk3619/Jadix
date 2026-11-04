use jadix_live2d::Player;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let Some(path) = std::env::args().nth(1) else {
        println!("usage: inspect path/to/avatar.model3.json");
        return Ok(());
    };
    let mut player = Player::load(path)?;
    println!("canvas: {:?}", player.runtime().canvas());
    println!("drawables: {}", player.runtime().meshes().len());
    println!("textures: {}", player.model().textures().len());
    for (group, index) in player.motions() {
        println!("motion: {group}[{index}]");
    }
    for name in player.expressions() {
        println!("expression: {name}");
    }
    player.set_look_at(0.25, 0.0)?;
    for _ in 0..60 {
        player.update(1.0 / 60.0)?;
    }
    println!("playback: 60 frames evaluated successfully");
    Ok(())
}
