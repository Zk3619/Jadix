//! D2D effect wrappers for Windows.UI.Composition `CreateEffectFactory`.

use windows::core::{implement, Interface, GUID, HSTRING, PCWSTR};
use windows::Foundation::{IPropertyValue, PropertyValue};
use windows::Graphics::Effects::{
    IGraphicsEffect, IGraphicsEffectSource, IGraphicsEffectSource_Impl, IGraphicsEffect_Impl,
};
use windows::Win32::System::WinRT::Graphics::Direct2D::{
    IGraphicsEffectD2D1Interop, IGraphicsEffectD2D1Interop_Impl, GRAPHICS_EFFECT_PROPERTY_MAPPING,
    GRAPHICS_EFFECT_PROPERTY_MAPPING_COLOR_TO_VECTOR4, GRAPHICS_EFFECT_PROPERTY_MAPPING_DIRECT,
    GRAPHICS_EFFECT_PROPERTY_MAPPING_UNKNOWN,
};

pub const CLSID_GAUSSIAN_BLUR: GUID = GUID::from_u128(0x1feb6d69_2fe6_4ac5_9d66_a79502f7d0f0);
pub const CLSID_FLOOD: GUID = GUID::from_u128(0x61c23c20_ae69_4d8e_94cf_50078df638f2);
pub const CLSID_OPACITY: GUID = GUID::from_u128(0x811d79a4_de28_4454_8094_c64685f8bd4c);
pub const CLSID_COMPOSITE: GUID = GUID::from_u128(0x48fc9f51_f6ac_48f1_8b58_3b28ac46f76d);
pub const CLSID_BLEND: GUID = GUID::from_u128(0x81c5b77b_cb08_4c25_af93_002d81462403);
pub const CLSID_SATURATION: GUID = GUID::from_u128(0x5cb2d2d3_0974_4d39_9976_165324bf621e);
pub const CLSID_BORDER: GUID = GUID::from_u128(0x2a2d49c0_4acf_433c_8013_b44bb8823147);
pub const CLSID_TINT: GUID = GUID::from_u128(0x36312b17_f7ee_4b43_865c_c36d33c0e80b);
pub const CLSID_TURBULENCE: GUID = GUID::from_u128(0xcf2bb6ae_889a_4ad7_ba29_a2fd732c9fc9);
pub const CLSID_CROSSFADE: GUID = GUID::from_u128(0x12f575e5_4dc7_4cd0_9e83_2c9c7600798f);

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
