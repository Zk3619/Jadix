//! Civil values deliberately carry no UTC offset: a calendar date never shifts
//! when a window's time zone or daylight-saving offset changes.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CivilDate { pub year: u16, pub month: u8, pub day: u8 }
impl CivilDate {
    pub fn validate(self) -> Result<(), &'static str> {
        let leap = self.year % 4 == 0 && (self.year % 100 != 0 || self.year % 400 == 0);
        let days = match self.month { 2 if leap => 29, 2 => 28, 4 | 6 | 9 | 11 => 30, 1 | 3 | 5 | 7 | 8 | 10 | 12 => 31, _ => 0 };
        if (1..=9999).contains(&self.year) && self.day > 0 && self.day <= days { Ok(()) }
        else { Err("invalid Gregorian date") }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DateControlMode { Calendar, Field }
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TimeOfDay { pub hour: u8, pub minute: u8, #[serde(default)] pub second: u8 }
impl TimeOfDay {
    pub fn validate(self) -> Result<(), &'static str> {
        if self.hour < 24 && self.minute < 60 && self.second < 60 { Ok(()) } else { Err("invalid time of day") }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hosted::HostedContent;
    #[test]
    fn civil_dates_and_platform_events_obey_the_same_range() {
        let date = |year, month, day| CivilDate { year, month, day };
        assert!(date(2000, 2, 29).validate().is_ok());
        assert!(date(1900, 2, 29).validate().is_err());
        assert!(date(2026, 4, 31).validate().is_err());
        let content = HostedContent::Date { value: None, minimum: Some(date(2026, 2, 1)), maximum: Some(date(2026, 2, 28)),
            mode: DateControlMode::Calendar, locale: Some("zh-CN".into()), enabled: true };
        assert!(content.validate().is_ok());
        assert!(content.accepts_event("change", &serde_json::json!({"year":2026,"month":2,"day":28})));
        assert!(!content.accepts_event("change", &serde_json::json!({"year":2026,"month":3,"day":1})));
        assert!(!content.accepts_event("change", &serde_json::json!({"year":2026,"month":2,"day":29})));
        assert!(!content.accepts_event("change", &serde_json::json!({"year":2026,"month":2,"day":1,"zone":"UTC"})));
        assert!(!HostedContent::Time { value: TimeOfDay::default(), seconds: false, enabled: true }
            .accepts_event("change", &serde_json::json!({"hour":12,"minute":0,"second":1})));
        assert!(!HostedContent::Color { rgba: [0,0,0,255], alpha: false, enabled: true }
            .accepts_event("change", &serde_json::json!([1,2,3,4])));
    }
}
