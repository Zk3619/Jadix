//! Platform independent accessibility semantics and explicitly advertised actions.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SemanticRole {
    Button, Link, Textbox, Img, Imagebutton, Key, Text, Adjustable, Heading,
    Summary, Alert, Checkbox, Combobox, Menu, Menubar, Menuitem, Progressbar,
    Radio, Radiogroup, Scrollbar, Spinbutton, Switch, Tab, Tablist, Timer,
    Toolbar, List, Grid, Table, Row, Cell, Columnheader, Rowgroup, Listbox,
    Option, Region, Navigation, Tree, Treeitem, Menuitemcheckbox, Menuitemradio,
    Dialog, Alertdialog,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CheckedState { False, True, Mixed }
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LiveRegion { #[default] Off, Polite, Assertive }

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AccessibleValue {
    pub min: f64,
    pub max: f64,
    pub now: f64,
    #[serde(default)]
    pub text: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AccessibleAction { pub id: String, pub label: String }

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Accessibility {
    pub role: Option<SemanticRole>,
    /// None preserves the platform control's default grouping policy.
    pub accessible: Option<bool>,
    pub hidden: bool,
    pub disabled: bool,
    pub busy: bool,
    pub read_only: bool,
    pub checked: Option<CheckedState>,
    pub pressed: Option<bool>,
    pub invalid: bool,
    pub live: LiveRegion,
    pub value: Option<AccessibleValue>,
    /// Reserved IDs increment, decrement and setValue map to standard AT actions.
    pub actions: Vec<AccessibleAction>,
}
impl Accessibility {
    pub fn from_attributes<'a>(get: impl Fn(&str) -> Option<&'a str>) -> Result<Self, &'static str> {
        if let Some(json) = get("jadix-accessibility") {
            let value: Self = serde_json::from_str(json).map_err(|_| "accessibility semantics")?;
            return value.is_valid().then_some(value).ok_or("accessibility semantics");
        }
        let boolean = |name| match get(name) {
            None => Ok(false), Some("true") => Ok(true), Some("false") => Ok(false),
            _ => Err("accessibility boolean"),
        };
        let enumeration = |value: &str| serde_json::Value::String(value.into());
        let role = get("accesskit_role").or_else(|| get("role"))
            .and_then(|role| serde_json::from_value(enumeration(match role {
                "slider" => "adjustable", "image" => "img", "label" => "text",
                "textfield" | "textinput" => "textbox", _ => role,
            })).ok());
        let checked = get("aria-checked").map(|v| serde_json::from_value(enumeration(v)))
            .transpose().map_err(|_| "accessibility checked state")?;
        let pressed = get("aria-pressed").map(|v| match v { "true" => Ok(true), "false" => Ok(false), _ => Err("accessibility pressed state") }).transpose()?;
        let live = get("aria-live").map(|v| serde_json::from_value(enumeration(v)))
            .transpose().map_err(|_| "accessibility live region")?.unwrap_or_default();
        let value = if ["aria-valuemin", "aria-valuemax", "aria-valuenow"].iter().any(|name| get(name).is_some()) {
            let number = |name| get(name).ok_or("accessibility range")?.parse::<f64>().map_err(|_| "accessibility range");
            Some(AccessibleValue { min: number("aria-valuemin")?, max: number("aria-valuemax")?, now: number("aria-valuenow")?, text: get("aria-valuetext").map(String::from) })
        } else { None };
        let semantics = Self { role, checked, pressed, invalid: boolean("aria-invalid")?, live, value, hidden: boolean("aria-hidden")?,
            disabled: boolean("aria-disabled")?, busy: boolean("aria-busy")?, read_only: boolean("aria-readonly")?, ..Default::default() };
        semantics.is_valid().then_some(semantics).ok_or("accessibility semantics")
    }

    /// Retains the full contract through GPU/DOM authoring round trips.
    pub fn write_attributes(&self, target: &mut impl Extend<(String, String)>) {
        let mut attrs = std::collections::BTreeMap::<String, String>::new();
        attrs.insert("jadix-accessibility".into(), serde_json::to_string(self).expect("validated accessibility"));
        if let Some(role) = self.role {
            if let serde_json::Value::String(role) = serde_json::to_value(role).expect("semantic role") {
                attrs.insert("accesskit_role".into(), role);
            }
        }
        for (name, value) in [("aria-hidden", self.hidden), ("aria-disabled", self.disabled),
            ("aria-busy", self.busy), ("aria-readonly", self.read_only), ("aria-invalid", self.invalid)] { attrs.insert(name.into(), value.to_string()); }
        for (name, value) in [("aria-checked", serde_json::to_value(self.checked)), ("aria-pressed", serde_json::to_value(self.pressed)), ("aria-live", serde_json::to_value(self.live))] {
            if let Ok(serde_json::Value::String(value)) = value { attrs.insert(name.into(), value); }
        }
        if let Some(pressed) = self.pressed { attrs.insert("aria-pressed".into(), pressed.to_string()); }
        if let Some(value) = &self.value {
            for (name, number) in [("aria-valuemin", value.min), ("aria-valuemax", value.max), ("aria-valuenow", value.now)] { attrs.insert(name.into(), number.to_string()); }
            if let Some(text) = &value.text { attrs.insert("aria-valuetext".into(), text.clone()); }
        }
        target.extend(attrs);
    }

    pub fn is_valid(&self) -> bool {
        let text_valid = |text: &str, limit| text.len() <= limit && !text.contains('\0');
        self.value.as_ref().is_none_or(|v| v.min.is_finite() && v.max.is_finite()
            && v.now.is_finite() && v.min < v.max && (v.min..=v.max).contains(&v.now)
            && v.text.as_ref().is_none_or(|text| text_valid(text, 4096)))
            && self.actions.len() <= 32 && self.actions.iter().enumerate().all(|(i, action)| {
                !action.id.is_empty() && text_valid(&action.id, 128)
                    && !action.id.chars().any(char::is_control)
                    && !action.label.trim().is_empty() && text_valid(&action.label, 512)
                    && self.actions[..i].iter().all(|prior| prior.id != action.id)
                    && (!matches!(action.id.as_str(), "increment" | "decrement" | "setValue") || self.value.is_some())
            })
    }
    pub fn accepts(&self, id: &str, value: Option<f64>) -> bool {
        !self.hidden && !self.disabled && !self.read_only && self.accessible != Some(false)
            && self.actions.iter().any(|action| action.id == id)
            && if id == "setValue" {
                self.value.as_ref().zip(value).is_some_and(|(range, value)|
                    value.is_finite() && (range.min..=range.max).contains(&value))
            } else { value.is_none() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn range_actions_reject_unadvertised_and_invalid_values() {
        let mut semantics = Accessibility {
            value: Some(AccessibleValue { min: 0.0, max: 10.0, now: 3.0, text: None }),
            actions: vec![AccessibleAction { id: "setValue".into(), label: "Set rating".into() }],
            ..Default::default()
        };
        assert!(semantics.is_valid());
        assert!(semantics.accepts("setValue", Some(7.0)));
        assert!(!semantics.accepts("setValue", Some(f64::NAN)));
        assert!(!semantics.accepts("setValue", Some(11.0)));
        assert!(!semantics.accepts("increment", None));
        semantics.hidden = true;
        assert!(!semantics.accepts("setValue", Some(7.0)));
        semantics.value.as_mut().unwrap().min = 11.0;
        assert!(!semantics.is_valid());
    }
    #[test]
    fn duplicate_actions_and_unknown_fields_are_rejected() {
        let action = AccessibleAction { id: "open".into(), label: "Open".into() };
        assert!(!Accessibility { actions: vec![action.clone(), action], ..Default::default() }.is_valid());
        assert!(serde_json::from_str::<Accessibility>(r#"{"readOnly":true,"typo":true}"#).is_err());
    }
    #[test]
    fn authoring_round_trip_keeps_custom_actions_and_rejects_invalid_aria() {
        let semantics = Accessibility { busy: true, actions: vec![AccessibleAction { id: "open".into(), label: "Open".into() }], ..Default::default() };
        let mut attrs = std::collections::BTreeMap::new();
        semantics.write_attributes(&mut attrs);
        assert_eq!(Accessibility::from_attributes(|name| attrs.get(name).map(String::as_str)), Ok(semantics));
        attrs.clear(); attrs.insert("aria-hidden".into(), "invalid".into());
        assert!(Accessibility::from_attributes(|name| attrs.get(name).map(String::as_str)).is_err());
    }
}
