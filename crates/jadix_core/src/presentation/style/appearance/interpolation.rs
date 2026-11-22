use super::{ViewAppearance, ViewTransition};

/// Numeric leaves that can be interpolated without changing the shape of a style.
/// Hosts apply these values over the target style; discrete changes take effect immediately.
#[derive(Clone, Debug, serde::Serialize)]
pub struct AppearanceChannel {
    pub path: Vec<String>,
    pub from: f64,
    pub to: f64,
    pub integral: bool,
    pub minimum: f64,
    pub maximum: f64,
}

impl ViewAppearance {
    pub fn transition_channels(&self, target: &Self, properties: u8) -> Vec<AppearanceChannel> {
        let mut channels = Vec::new();
        let from = serde_json::to_value(self).expect("serializable appearance");
        let to = serde_json::to_value(target).expect("serializable appearance");
        for (mask, fields) in [
            (ViewTransition::COLORS, &["background", "tint", "border_colors", "outline_color", "accent", "glass_tint"][..]),
            (ViewTransition::OPACITY, &["opacity"][..]),
            (ViewTransition::TRANSFORM, &["transform", "origin"][..]),
            (ViewTransition::SHADOW, &["shadows"][..]),
            (ViewTransition::SHAPE, &["corners", "border_widths", "outline_width", "outline_offset"][..]),
        ] {
            if properties & mask == 0 { continue; }
            for field in fields {
                collect(&from[field], &to[field], &mut vec![(*field).into()], &mut channels);
            }
        }
        channels
    }
}

fn collect(from: &serde_json::Value, to: &serde_json::Value, path: &mut Vec<String>, channels: &mut Vec<AppearanceChannel>) {
    use serde_json::Value;
    match (from, to) {
        (Value::Number(a), Value::Number(b)) => {
            if let (Some(a), Some(b)) = (a.as_f64(), b.as_f64()) {
                if a != b {
                    let integral=from.as_number().is_some_and(|n|n.is_i64()||n.is_u64());
                    let opacity=path[0]=="opacity";
                    let nonnegative=integral || matches!(path[0].as_str(),"corners"|"border_widths"|"outline_width") || path.iter().any(|s|s=="blur");
                    channels.push(AppearanceChannel { path:path.clone(), from:a, to:b, integral,
                        minimum:if opacity||nonnegative {0.} else {-1_000_000.},
                        maximum:if opacity {1.} else if integral {255.} else {1_000_000.},
                    });
                }
            }
        }
        (Value::Array(a), Value::Array(b)) if a.len() == b.len() => {
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                path.push(i.to_string()); collect(a, b, path, channels); path.pop();
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            for (key, value) in a {
                if let Some(other) = b.get(key) { path.push(key.clone()); collect(value, other, path, channels); path.pop(); }
            }
        }
        _ => {}
    }
}
