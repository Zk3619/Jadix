//! # Path primitives
//!
//! 折线 / 多边形 / 扇形点列辅助。渲染器若尚无 path 管线，可将序列序列化到 Element attrs，
//! 并由 [`draw_polyline_batch_stub`] 登记；图表 MVP 优先用矩形条（见 [`crate::components::chart`]）。

use crate::core::tree::Element;

/// 2D 点（逻辑像素）。
pub type Point2 = (f32, f32);

/// 路径绘制命令（为未来 GPU path 预留）。
#[derive(Clone, Debug, PartialEq)]
pub enum PathCommand {
    MoveTo(Point2),
    LineTo(Point2),
    Close,
}

/// 折线 → 命令列表。
pub fn polyline(points: &[Point2]) -> Vec<PathCommand> {
    let mut out = Vec::with_capacity(points.len().saturating_add(1));
    let mut iter = points.iter();
    if let Some(&p0) = iter.next() {
        out.push(PathCommand::MoveTo(p0));
        for &p in iter {
            out.push(PathCommand::LineTo(p));
        }
    }
    out
}

/// 闭合多边形。
pub fn polygon(points: &[Point2]) -> Vec<PathCommand> {
    let mut cmds = polyline(points);
    if !cmds.is_empty() {
        cmds.push(PathCommand::Close);
    }
    cmds
}

/// 饼图扇区：圆心 + 半径 + 起止角（弧度，0 = 正右，逆时针）。
pub fn pie_sector(cx: f32, cy: f32, r: f32, start: f32, end: f32, segments: usize) -> Vec<PathCommand> {
    let n = segments.max(3);
    let mut pts = Vec::with_capacity(n + 2);
    pts.push((cx, cy));
    for i in 0..=n {
        let t = i as f32 / n as f32;
        let a = start + (end - start) * t;
        pts.push((cx + r * a.cos(), cy + r * a.sin()));
    }
    polygon(&pts)
}

/// 将点列编码为 attr（`x,y;x,y;…`）。
pub fn encode_points(points: &[Point2]) -> String {
    points
        .iter()
        .map(|(x, y)| format!("{x:.3},{y:.3}"))
        .collect::<Vec<_>>()
        .join(";")
}

/// 把折线数据挂到 Element attrs，供未来 renderer 消费。
pub fn attach_polyline(mut el: Element, points: &[Point2]) -> Element {
    el.set_attr("rui_path_kind", "polyline");
    el.set_attr("rui_path_points", encode_points(points));
    el
}

/// 最小 stub：记录「需要 polyline batch」的意图；当前帧不绘制 GPU path。
///
/// 文档：真正的 `draw_polyline` 管线尚未接入 wgpu；图表请用矩形条 MVP。
pub fn draw_polyline_batch_stub(points: &[Point2]) -> Element {
    let mut el = Element::new().w(1.0).h(1.0);
    el = attach_polyline(el, points);
    el.set_attr("rui_draw_polyline_stub", "1");
    el
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn polyline_and_pie_encode() {
        let cmds = polyline(&[(0.0, 0.0), (1.0, 2.0)]);
        assert_eq!(cmds.len(), 2);
        let pie = pie_sector(0.0, 0.0, 10.0, 0.0, std::f32::consts::FRAC_PI_2, 4);
        assert!(pie.len() >= 4);
        let s = encode_points(&[(1.5, 2.5)]);
        assert_eq!(s, "1.500,2.500");
    }
}
