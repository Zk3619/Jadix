use crate::{core::Matrix44, Error, Result};

/// An aspect-preserving model-to-viewport transform and inverse pointer mapping.
#[derive(Clone, Copy, Debug)]
pub struct View {
    matrix: Matrix44,
    size: [f32; 2],
}
impl View {
    pub fn fit(bounds: [f32; 4], size: [u32; 2], padding: f32) -> Result<Self> {
        if bounds.iter().any(|value| !value.is_finite())
            || size.contains(&0)
            || !padding.is_finite()
            || !(0.0..0.5).contains(&padding)
            || bounds[2] <= bounds[0]
            || bounds[3] <= bounds[1]
        {
            return Err(Error::Invalid(
                "invalid view bounds, size or padding".into(),
            ));
        }
        let width = size[0] as f32;
        let height = size[1] as f32;
        let horizontal = f64::from(bounds[2]) - f64::from(bounds[0]);
        let vertical = f64::from(bounds[3]) - f64::from(bounds[1]);
        let scale = (f64::from(width) / horizontal).min(f64::from(height) / vertical)
            * f64::from(1.0 - 2.0 * padding);
        let scale_x = (2.0 * scale / f64::from(width)) as f32;
        let scale_y = (2.0 * scale / f64::from(height)) as f32;
        let mut matrix = Matrix44::identity();
        matrix.scale(scale_x, scale_y);
        matrix.translate(
            (-(f64::from(bounds[0]) + f64::from(bounds[2])) * 0.5 * f64::from(scale_x)) as f32,
            (-(f64::from(bounds[1]) + f64::from(bounds[3])) * 0.5 * f64::from(scale_y)) as f32,
        );
        if scale_x <= 0.0
            || scale_y <= 0.0
            || matrix.as_slice().iter().any(|value| !value.is_finite())
        {
            return Err(Error::Invalid(
                "view transform exceeds floating-point range".into(),
            ));
        }
        Ok(Self {
            matrix,
            size: [width, height],
        })
    }
    pub fn matrix(&self) -> &Matrix44 {
        &self.matrix
    }
    /// Converts physical top-left-origin viewport pixels to model coordinates.
    pub fn model_position(&self, position: [f32; 2]) -> Option<[f32; 2]> {
        if position.iter().any(|value| !value.is_finite()) {
            return None;
        }
        let point = [
            self.matrix
                .invert_transform_x(position[0] / self.size[0] * 2.0 - 1.0),
            self.matrix
                .invert_transform_y(1.0 - position[1] / self.size[1] * 2.0),
        ];
        point.iter().all(|value| value.is_finite()).then_some(point)
    }
}
