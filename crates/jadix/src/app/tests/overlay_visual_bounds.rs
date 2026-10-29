use super::AppState;
use crate::renderer::InstanceInput;
use bytemuck::Zeroable;

fn rect(x: f32, y: f32, w: f32, h: f32, alpha: f32) -> InstanceInput {
    let mut i = InstanceInput::zeroed();
    i.rect_pos = [x, y];
    i.rect_size = [w, h];
    i.color[3] = alpha;
    i.transform = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
    i.clip_size = InstanceInput::NO_CLIP_SIZE;
    i
}

#[test]
fn transparent_fullscreen_portal_shell_does_not_expand_avatar_bounds() {
    let shell = rect(0.0, 0.0, 1280.0, 720.0, 0.0);
    let avatar_panel = rect(120.0, 80.0, 160.0, 48.0, 1.0);

    let bounds =
        AppState::overlay_visual_bounds_logical(&[shell, avatar_panel], &[], &[], &[], 1.0);

    assert_eq!(bounds, Some([118.0, 78.0, 164.0, 52.0]));
}

#[test]
fn transparent_shell_does_not_hide_backdrop_visual_occupancy() {
    let mut shell = rect(0.0, 0.0, 1280.0, 720.0, 0.0);
    shell.backdrop_blur = 12.0;

    let bounds = AppState::overlay_visual_bounds_logical(&[shell], &[], &[], &[], 1.0);

    assert_eq!(bounds, Some([-2.0, -2.0, 1284.0, 724.0]));
}
