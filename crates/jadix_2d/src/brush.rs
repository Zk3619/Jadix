//! Explicit brush coordinates, independent of a shape's bounding box.
use crate::{gradient::Gradient, Image2D, ImageSampling};
/// Image continuation outside its pixel domain, applied separately to each axis.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ExtendMode {
    #[default]
    Pad,
    Repeat,
    Reflect,
    Decal,
}
/// Brush-local coordinates map to current user coordinates through this affine.
/// `[a,b,c,d,e,f]` maps `(x,y)` to `(a*x+c*y+e,b*x+d*y+f)`.
/// The canvas CTM is captured at draw time. Invalid/singular mappings paint nothing.
#[derive(Clone, Debug, PartialEq)]
pub enum Brush2D {
    Vector {
        gradient: crate::VectorGradient,
        transform: [f32; 6],
    },
    Gradient {
        gradient: Gradient,
        rect: [f32; 4],
        transform: [f32; 6],
    },
    Image {
        image: Image2D,
        transform: [f32; 6],
        extend_x: ExtendMode,
        extend_y: ExtendMode,
        sampling: ImageSampling,
    },
}
impl Brush2D {
    pub fn vector(gradient: crate::VectorGradient) -> Self {
        Self::Vector {
            gradient,
            transform: [1., 0., 0., 1., 0., 0.],
        }
    }
    /// CSS gradient geometry resolved within an explicit brush-local rectangle.
    pub fn gradient(gradient: Gradient, rect: [f32; 4]) -> Self {
        Self::Gradient {
            gradient,
            rect,
            transform: [1., 0., 0., 1., 0., 0.],
        }
    }
    /// Image pixels define the brush-local domain; repeating nearest sampling.
    pub fn image(image: Image2D) -> Self {
        Self::Image {
            image,
            transform: [1., 0., 0., 1., 0., 0.],
            extend_x: ExtendMode::Repeat,
            extend_y: ExtendMode::Repeat,
            sampling: ImageSampling::Nearest,
        }
    }
    pub fn with_transform(mut self, matrix: [f32; 6]) -> Self {
        match &mut self {
            Self::Gradient { transform, .. }
            | Self::Image { transform, .. }
            | Self::Vector { transform, .. } => *transform = matrix,
        };
        self
    }
    /// Set image extension per axis. Gradients retain their configured extension.
    pub fn with_extend(mut self, x: ExtendMode, y: ExtendMode) -> Self {
        if let Self::Image {
            extend_x, extend_y, ..
        } = &mut self
        {
            *extend_x = x;
            *extend_y = y;
        }
        self
    }
    /// Set image filtering; this has no effect on gradient variants.
    pub fn with_sampling(mut self, quality: ImageSampling) -> Self {
        if let Self::Image { sampling, .. } = &mut self {
            *sampling = quality;
        }
        self
    }
    pub(crate) fn transform(&self) -> [f32; 6] {
        match self {
            Self::Gradient { transform, .. }
            | Self::Image { transform, .. }
            | Self::Vector { transform, .. } => *transform,
        }
    }
    pub(crate) fn valid(&self) -> bool {
        match self {
            Self::Gradient { rect, .. } => {
                rect.iter().all(|v| v.is_finite()) && rect[2] > 0. && rect[3] > 0.
            }
            Self::Image { .. } | Self::Vector { .. } => true,
        }
    }
}
/// Return premultiplied linear-light pixels, extending each integer texel separately.
pub(crate) fn sample_image(
    image: &Image2D,
    point: (f32, f32),
    x_mode: ExtendMode,
    y_mode: ExtendMode,
    sampling: ImageSampling,
) -> [f32; 4] {
    if !point.0.is_finite() || !point.1.is_finite() {
        return [0.; 4];
    }
    let texel = |x: f32, y: f32| {
        let (Some(x), Some(y)) = (
            texel_index(x, image.width(), x_mode),
            texel_index(y, image.height(), y_mode),
        ) else {
            return [0.; 4];
        };
        image.linear_texel(x, y)
    };
    crate::image::filtered_sample(point, sampling, texel)
}

pub(crate) fn texel_index(i: f32, n: u32, mode: ExtendMode) -> Option<usize> {
    let n = n as f32;
    Some(match mode {
        ExtendMode::Pad => i.clamp(0., n - 1.),
        ExtendMode::Repeat => i.rem_euclid(n),
        ExtendMode::Reflect => {
            let p = i.rem_euclid(2. * n);
            if p < n {
                p
            } else {
                2. * n - 1. - p
            }
        }
        ExtendMode::Decal => {
            if i < 0. || i >= n {
                return None;
            }
            i
        }
    } as usize)
}
