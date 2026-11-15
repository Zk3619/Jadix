use jadix_2d::{Canvas2D, GpuCanvas, RasterCanvas};

#[test]
fn canvas_renders_without_framework_layout_or_window() {
    fn draw(canvas: &mut dyn Canvas2D) {
        canvas.translate(2.0, 0.0);
        canvas.clip_rect(0.0, 0.0, 4.0, 8.0);
        canvas.set_fill("#ff0000");
        canvas.fill_rect(0.0, 0.0, 8.0, 8.0);
    }
    let mut cpu = RasterCanvas::new(10, 8);
    draw(&mut cpu);
    assert_eq!(cpu.get_image_data(3, 3, 1, 1).unwrap(), [255, 0, 0, 255]);
    assert_eq!(cpu.get_image_data(8, 3, 1, 1).unwrap(), [0, 0, 0, 0]);
    let mut gpu = GpuCanvas::new(10, 8, None);
    draw(&mut gpu);
    assert_eq!(
        gpu.get_image_data(3, 3, 1, 1),
        cpu.get_image_data(3, 3, 1, 1)
    );
    assert!(!gpu.into_ops().is_empty());
}
