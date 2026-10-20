// Shared by both adapters. Logical pixels; ordinary, dense and nested are
// synthetic calibration workloads, never stand-ins for Gallery Forms/Layout.
pub const FONT: &str = "Segoe UI";
pub const FONT_SIZE: f32 = 16.0;
pub const LINE_HEIGHT: f32 = 25.0;
pub const HEADER: &str = "Frame comparison - fixed content / no virtualization";
pub const HEADER_HEIGHT: f32 = 56.0;
pub const ROW_HEIGHT: f32 = 48.0;
pub const BACKGROUND: u32 = 0x18181b;
pub const FOREGROUND: u32 = 0xfafafa;
pub const BORDER: u32 = 0x3f3f46;

pub fn scene() -> String {
    let scene = std::env::var("JADIX_GALLERY_SECTION").unwrap_or_else(|_| "ordinary".into());
    assert!(
        ["ordinary", "dense", "nested"].contains(&scene.as_str()),
        "unknown scene"
    );
    scene
}

pub fn rows(scene: &str) -> usize {
    if scene == "dense" {
        600
    } else {
        200
    }
}

pub fn labels(index: usize) -> [String; 3] {
    [
        format!("{:03}", index + 1),
        format!("List item {index:03}"),
        "Ready".into(),
    ]
}
