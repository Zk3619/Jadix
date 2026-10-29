//! SVG Filter Effects graph model (Filter Effects Module Level 1).

use crate::layout::dimension::CssDim;

/// Named input to a filter primitive.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum FilterInput {
    SourceGraphic,
    SourceAlpha,
    BackgroundImage,
    BackgroundAlpha,
    FillPaint,
    StrokePaint,
    /// Previous primitive `result` name, or empty = previous output.
    Result(String),
}

impl FilterInput {
    pub fn parse(s: &str) -> Self {
        match s.trim() {
            "" => Self::Result(String::new()),
            "SourceGraphic" => Self::SourceGraphic,
            "SourceAlpha" => Self::SourceAlpha,
            "BackgroundImage" => Self::BackgroundImage,
            "BackgroundAlpha" => Self::BackgroundAlpha,
            "FillPaint" => Self::FillPaint,
            "StrokePaint" => Self::StrokePaint,
            other => Self::Result(other.to_string()),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FilterUnits {
    #[default]
    ObjectBoundingBox,
    UserSpaceOnUse,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PrimitiveUnits {
    ObjectBoundingBox,
    #[default]
    UserSpaceOnUse,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorMatrixType {
    Matrix,
    Saturate,
    HueRotate,
    LuminanceToAlpha,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlendMode {
    Normal,
    Multiply,
    Screen,
    Darken,
    Lighten,
    Overlay,
    ColorDodge,
    ColorBurn,
    HardLight,
    SoftLight,
    Difference,
    Exclusion,
    Hue,
    Saturation,
    Color,
    Luminosity,
}

impl BlendMode {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "multiply" => Self::Multiply,
            "screen" => Self::Screen,
            "darken" => Self::Darken,
            "lighten" => Self::Lighten,
            "overlay" => Self::Overlay,
            "color-dodge" => Self::ColorDodge,
            "color-burn" => Self::ColorBurn,
            "hard-light" => Self::HardLight,
            "soft-light" => Self::SoftLight,
            "difference" => Self::Difference,
            "exclusion" => Self::Exclusion,
            "hue" => Self::Hue,
            "saturation" => Self::Saturation,
            "color" => Self::Color,
            "luminosity" => Self::Luminosity,
            _ => Self::Normal,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompositeOperator {
    Over,
    In,
    Out,
    Atop,
    Xor,
    Lighter,
    Arithmetic,
}

impl CompositeOperator {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "in" => Self::In,
            "out" => Self::Out,
            "atop" => Self::Atop,
            "xor" => Self::Xor,
            "lighter" => Self::Lighter,
            "arithmetic" => Self::Arithmetic,
            _ => Self::Over,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MorphologyOperator {
    Erode,
    Dilate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EdgeMode {
    Duplicate,
    Wrap,
    None,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChannelSelector {
    R,
    G,
    B,
    A,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransferType {
    Identity,
    Table,
    Discrete,
    Linear,
    Gamma,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TransferFunction {
    pub ty: TransferType,
    pub table_values: Vec<f32>,
    pub slope: f32,
    pub intercept: f32,
    pub amplitude: f32,
    pub exponent: f32,
    pub offset: f32,
}

impl Default for TransferFunction {
    fn default() -> Self {
        Self {
            ty: TransferType::Identity,
            table_values: Vec::new(),
            slope: 1.0,
            intercept: 0.0,
            amplitude: 1.0,
            exponent: 1.0,
            offset: 0.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum LightSource {
    Distant {
        azimuth: f32,
        elevation: f32,
    },
    Point {
        x: f32,
        y: f32,
        z: f32,
    },
    Spot {
        x: f32,
        y: f32,
        z: f32,
        points_at_x: f32,
        points_at_y: f32,
        points_at_z: f32,
        specular_exponent: f32,
        limiting_cone_angle: Option<f32>,
    },
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct PrimitiveRegion {
    pub x: Option<CssDim>,
    pub y: Option<CssDim>,
    pub width: Option<CssDim>,
    pub height: Option<CssDim>,
}

/// One SVG filter primitive.
#[derive(Clone, Debug, PartialEq)]
pub enum FilterPrimitive {
    Blend {
        in1: FilterInput,
        in2: FilterInput,
        mode: BlendMode,
        result: String,
        region: PrimitiveRegion,
    },
    ColorMatrix {
        in1: FilterInput,
        ty: ColorMatrixType,
        values: Vec<f32>,
        result: String,
        region: PrimitiveRegion,
    },
    ComponentTransfer {
        in1: FilterInput,
        func_r: TransferFunction,
        func_g: TransferFunction,
        func_b: TransferFunction,
        func_a: TransferFunction,
        result: String,
        region: PrimitiveRegion,
    },
    Composite {
        in1: FilterInput,
        in2: FilterInput,
        operator: CompositeOperator,
        k1: f32,
        k2: f32,
        k3: f32,
        k4: f32,
        result: String,
        region: PrimitiveRegion,
    },
    ConvolveMatrix {
        in1: FilterInput,
        order_x: u32,
        order_y: u32,
        kernel: Vec<f32>,
        divisor: Option<f32>,
        bias: f32,
        target_x: Option<u32>,
        target_y: Option<u32>,
        edge_mode: EdgeMode,
        kernel_unit_length: Option<(f32, f32)>,
        preserve_alpha: bool,
        result: String,
        region: PrimitiveRegion,
    },
    DiffuseLighting {
        in1: FilterInput,
        surface_scale: f32,
        diffuse_constant: f32,
        kernel_unit_length: Option<(f32, f32)>,
        lighting_color: [f32; 4],
        light: LightSource,
        result: String,
        region: PrimitiveRegion,
    },
    DisplacementMap {
        in1: FilterInput,
        in2: FilterInput,
        scale: f32,
        x_channel: ChannelSelector,
        y_channel: ChannelSelector,
        result: String,
        region: PrimitiveRegion,
    },
    Flood {
        color: [f32; 4],
        opacity: f32,
        result: String,
        region: PrimitiveRegion,
    },
    GaussianBlur {
        in1: FilterInput,
        std_deviation: (f32, f32),
        result: String,
        region: PrimitiveRegion,
    },
    Image {
        href: String,
        result: String,
        region: PrimitiveRegion,
    },
    Merge {
        nodes: Vec<FilterInput>,
        result: String,
        region: PrimitiveRegion,
    },
    Morphology {
        in1: FilterInput,
        operator: MorphologyOperator,
        radius: (f32, f32),
        result: String,
        region: PrimitiveRegion,
    },
    Offset {
        in1: FilterInput,
        dx: f32,
        dy: f32,
        result: String,
        region: PrimitiveRegion,
    },
    SpecularLighting {
        in1: FilterInput,
        surface_scale: f32,
        specular_constant: f32,
        specular_exponent: f32,
        kernel_unit_length: Option<(f32, f32)>,
        lighting_color: [f32; 4],
        light: LightSource,
        result: String,
        region: PrimitiveRegion,
    },
    Tile {
        in1: FilterInput,
        result: String,
        region: PrimitiveRegion,
    },
    Turbulence {
        base_frequency: (f32, f32),
        num_octaves: u32,
        seed: f32,
        stitch_tiles: bool,
        fractal_noise: bool,
        result: String,
        region: PrimitiveRegion,
    },
}

impl FilterPrimitive {
    pub fn result_name(&self) -> &str {
        match self {
            Self::Blend { result, .. }
            | Self::ColorMatrix { result, .. }
            | Self::ComponentTransfer { result, .. }
            | Self::Composite { result, .. }
            | Self::ConvolveMatrix { result, .. }
            | Self::DiffuseLighting { result, .. }
            | Self::DisplacementMap { result, .. }
            | Self::Flood { result, .. }
            | Self::GaussianBlur { result, .. }
            | Self::Image { result, .. }
            | Self::Merge { result, .. }
            | Self::Morphology { result, .. }
            | Self::Offset { result, .. }
            | Self::SpecularLighting { result, .. }
            | Self::Tile { result, .. }
            | Self::Turbulence { result, .. } => result.as_str(),
        }
    }
}

/// A complete SVG `<filter>` element.
#[derive(Clone, Debug, PartialEq)]
pub struct FilterGraph {
    pub id: String,
    pub filter_units: FilterUnits,
    pub primitive_units: PrimitiveUnits,
    /// Default region: x=-10%, y=-10%, width=120%, height=120%.
    pub x: CssDim,
    pub y: CssDim,
    pub width: CssDim,
    pub height: CssDim,
    pub primitives: Vec<FilterPrimitive>,
}

impl Default for FilterGraph {
    fn default() -> Self {
        Self {
            id: String::new(),
            filter_units: FilterUnits::ObjectBoundingBox,
            primitive_units: PrimitiveUnits::UserSpaceOnUse,
            x: CssDim::Percent(-0.10),
            y: CssDim::Percent(-0.10),
            width: CssDim::Percent(1.20),
            height: CssDim::Percent(1.20),
            primitives: Vec::new(),
        }
    }
}

impl FilterGraph {
    pub fn is_empty(&self) -> bool {
        self.primitives.is_empty()
    }
}
