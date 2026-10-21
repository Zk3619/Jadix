//! D2D effect wrappers for Windows.UI.Composition `CreateEffectFactory`.

use windows::Foundation::{IPropertyValue, PropertyValue};
use windows::Graphics::Effects::{
    IGraphicsEffect, IGraphicsEffect_Impl, IGraphicsEffectSource, IGraphicsEffectSource_Impl,
};
use windows::Win32::System::WinRT::Graphics::Direct2D::{
    GRAPHICS_EFFECT_PROPERTY_MAPPING, GRAPHICS_EFFECT_PROPERTY_MAPPING_COLOR_TO_VECTOR4,
    GRAPHICS_EFFECT_PROPERTY_MAPPING_DIRECT, GRAPHICS_EFFECT_PROPERTY_MAPPING_UNKNOWN,
    IGraphicsEffectD2D1Interop, IGraphicsEffectD2D1Interop_Impl,
};
use windows::core::{GUID, HSTRING, Interface, PCWSTR, implement};

pub use windows::Win32::Graphics::Direct2D::{
    CLSID_D2D1Blend as CLSID_BLEND, CLSID_D2D1Border as CLSID_BORDER,
    CLSID_D2D1Composite as CLSID_COMPOSITE, CLSID_D2D1CrossFade as CLSID_CROSSFADE,
    CLSID_D2D1Flood as CLSID_FLOOD, CLSID_D2D1GaussianBlur as CLSID_GAUSSIAN_BLUR,
    CLSID_D2D1Opacity as CLSID_OPACITY, CLSID_D2D1Saturation as CLSID_SATURATION,
    CLSID_D2D1Tint as CLSID_TINT, CLSID_D2D1Turbulence as CLSID_TURBULENCE,
};

#[allow(dead_code)]
pub const BLEND_MULTIPLY: u32 = 0;
pub const BLEND_COLOR: u32 = 22;
pub const BLEND_LUMINOSITY: u32 = 23;
pub const COMPOSITE_SOURCE_OVER: u32 = 0;
#[allow(dead_code)]
pub const COMPOSITE_PLUS: u32 = 9;
/// D2D1_BORDER_MODE_SOFT — blur kernel may sample outside without a hard box edge.
pub const BORDER_MODE_SOFT: u32 = 0;
#[allow(dead_code)]
pub const BORDER_MODE_HARD: u32 = 1;
#[allow(dead_code)]
pub const GAUSSIAN_OPT_SPEED: u32 = 0;
pub const GAUSSIAN_OPT_QUALITY: u32 = 2;
#[allow(dead_code)]
pub const BORDER_EDGE_WRAP: u32 = 1;

#[implement(IGraphicsEffect, IGraphicsEffectSource, IGraphicsEffectD2D1Interop)]
pub struct D2dEffect {
    id: GUID,
    name: std::cell::RefCell<HSTRING>,
    props: Vec<IPropertyValue>,
    sources: Vec<IGraphicsEffectSource>,
}

impl D2dEffect {
    pub fn new(
        id: GUID,
        name: &str,
        props: Vec<IPropertyValue>,
        sources: Vec<IGraphicsEffectSource>,
    ) -> Self {
        Self {
            id,
            name: std::cell::RefCell::new(HSTRING::from(name)),
            props,
            sources,
        }
    }

    pub fn into_graphics_effect(self) -> IGraphicsEffect {
        self.into()
    }
}

impl IGraphicsEffectSource_Impl for D2dEffect_Impl {}

impl IGraphicsEffect_Impl for D2dEffect_Impl {
    fn Name(&self) -> windows::core::Result<HSTRING> {
        Ok(self.name.borrow().clone())
    }

    fn SetName(&self, name: &HSTRING) -> windows::core::Result<()> {
        *self.name.borrow_mut() = name.clone();
        Ok(())
    }
}

fn map_property(id: GUID, name: &str) -> Option<(u32, GRAPHICS_EFFECT_PROPERTY_MAPPING)> {
    let direct = GRAPHICS_EFFECT_PROPERTY_MAPPING_DIRECT;
    Some(match (id, name) {
        (CLSID_GAUSSIAN_BLUR, "StandardDeviation" | "BlurAmount") => (0, direct),
        (CLSID_GAUSSIAN_BLUR, "Optimization") => (1, direct),
        (CLSID_GAUSSIAN_BLUR, "BorderMode") => (2, direct),
        (CLSID_FLOOD, "Color") => (0, GRAPHICS_EFFECT_PROPERTY_MAPPING_COLOR_TO_VECTOR4),
        (CLSID_OPACITY, "Opacity") => (0, direct),
        (CLSID_COMPOSITE, "Mode") => (0, direct),
        (CLSID_BLEND, "Mode") => (0, direct),
        (CLSID_SATURATION, "Saturation") => (0, direct),
        (CLSID_TINT, "Color") => (0, GRAPHICS_EFFECT_PROPERTY_MAPPING_COLOR_TO_VECTOR4),
        (CLSID_TINT, "ClampOutput") => (1, direct),
        (CLSID_BORDER, "EdgeModeX") => (0, direct),
        (CLSID_BORDER, "EdgeModeY") => (1, direct),
        (CLSID_TURBULENCE, "Offset") => (0, direct),
        (CLSID_TURBULENCE, "Size") => (1, direct),
        (CLSID_TURBULENCE, "BaseFrequency") => (2, direct),
        (CLSID_TURBULENCE, "NumOctaves") => (3, direct),
        (CLSID_TURBULENCE, "Seed") => (4, direct),
        (CLSID_TURBULENCE, "Noise") => (5, direct),
        (CLSID_TURBULENCE, "Stitchable") => (6, direct),
        (CLSID_CROSSFADE, "Weight") => (0, direct),
        _ => return None,
    })
}

impl IGraphicsEffectD2D1Interop_Impl for D2dEffect_Impl {
    fn GetEffectId(&self) -> windows::core::Result<GUID> {
        Ok(self.id)
    }

    fn GetNamedPropertyMapping(
        &self,
        name: &PCWSTR,
        index: *mut u32,
        mapping: *mut GRAPHICS_EFFECT_PROPERTY_MAPPING,
    ) -> windows::core::Result<()> {
        let n = unsafe { name.to_string() }.unwrap_or_default();
        let key = n.rsplit('.').next().unwrap_or(n.as_str());
        let Some((i, map)) = map_property(self.id, key) else {
            eprintln!("jadix glass: unmapped D2D property `{n}`");
            return Err(windows::core::Error::from(
                windows::Win32::Foundation::E_INVALIDARG,
            ));
        };
        unsafe {
            if !index.is_null() {
                *index = i;
            }
            if !mapping.is_null() {
                *mapping = map;
            }
        }
        Ok(())
    }

    fn GetPropertyCount(&self) -> windows::core::Result<u32> {
        Ok(self.props.len() as u32)
    }

    fn GetProperty(&self, index: u32) -> windows::core::Result<IPropertyValue> {
        self.props
            .get(index as usize)
            .cloned()
            .ok_or_else(|| windows::core::Error::from(windows::Win32::Foundation::E_INVALIDARG))
    }

    fn GetSource(&self, index: u32) -> windows::core::Result<IGraphicsEffectSource> {
        self.sources
            .get(index as usize)
            .cloned()
            .ok_or_else(|| windows::core::Error::from(windows::Win32::Foundation::E_INVALIDARG))
    }

    fn GetSourceCount(&self) -> windows::core::Result<u32> {
        Ok(self.sources.len() as u32)
    }
}

pub fn prop_f32(v: f32) -> windows::core::Result<IPropertyValue> {
    PropertyValue::CreateSingle(v)?.cast()
}

pub fn prop_u32(v: u32) -> windows::core::Result<IPropertyValue> {
    PropertyValue::CreateUInt32(v)?.cast()
}

pub fn prop_vec4(x: f32, y: f32, z: f32, w: f32) -> windows::core::Result<IPropertyValue> {
    PropertyValue::CreateSingleArray(&[x, y, z, w])?.cast()
}

pub fn source_param(name: &str) -> windows::core::Result<IGraphicsEffectSource> {
    windows::UI::Composition::CompositionEffectSourceParameter::Create(&HSTRING::from(name))?.cast()
}

pub fn as_source(effect: IGraphicsEffect) -> windows::core::Result<IGraphicsEffectSource> {
    effect.cast()
}

/// Silence unused mapping constant (kept for the D2D interop contract).
#[allow(dead_code)]
const _UNKNOWN: GRAPHICS_EFFECT_PROPERTY_MAPPING = GRAPHICS_EFFECT_PROPERTY_MAPPING_UNKNOWN;
