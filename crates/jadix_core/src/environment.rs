//! Per-root, validated host preferences. Unknown OS values stay `None`.
//! Time-zone identifiers are metadata; no process TZ or formatter is changed.
use crate::{create_memo, create_store, Memo, ReadStore, Store};
use serde::{Deserialize, Serialize};
use std::{
    io,
    marker::PhantomData,
    rc::Rc,
    sync::atomic::{AtomicU64, Ordering},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HourCycle {
    H11,
    H12,
    H23,
    H24,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MeasurementSystem {
    Metric,
    Us,
    Uk,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TemperatureUnit {
    Celsius,
    Fahrenheit,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ColorScheme {
    Light,
    Dark,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LayoutDirection {
    LeftToRight,
    RightToLeft,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Lifecycle {
    Active,
    Inactive,
    Background,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TimeZoneKind {
    Iana,
    Windows,
    Offset,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TimeZone {
    pub kind: TimeZoneKind,
    pub id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocaleSettings {
    pub locale: String,
    pub languages: Vec<String>,
    pub time_zone: Option<TimeZone>,
    pub hour_cycle: Option<HourCycle>,
    /// ISO weekday, Monday=1 through Sunday=7. Unknown remains None.
    pub first_day_of_week: Option<u8>,
    pub measurement_system: Option<MeasurementSystem>,
    pub temperature_unit: Option<TemperatureUnit>,
}
impl Default for LocaleSettings {
    fn default() -> Self {
        Self {
            locale: "en-US".into(),
            languages: vec!["en-US".into()],
            time_zone: None,
            hour_cycle: None,
            first_day_of_week: None,
            measurement_system: None,
            temperature_unit: None,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnvironmentSnapshot {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform: Option<PlatformInfo>,
    pub locale: LocaleSettings,
    pub color_scheme: Option<ColorScheme>,
    /// Host-provided reading/layout direction; None means the host did not report it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout_direction: Option<LayoutDirection>,
    /// Whether the host reports an active high-contrast preference.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub high_contrast: Option<bool>,
    pub reduced_motion: Option<bool>,
    /// Application/host text multiplier, not display DPI. The system snapshot
    /// currently leaves this at 1.0; it does not read Windows text-size scaling.
    pub font_scale: f32,
    pub lifecycle: Lifecycle,
}
impl Default for EnvironmentSnapshot {
    /// Deterministic fallback, not a claim about the machine's settings.
    fn default() -> Self {
        Self {
            platform: None,
            locale: LocaleSettings::default(),
            color_scheme: None,
            layout_direction: None,
            high_contrast: None,
            reduced_motion: None,
            font_scale: 1.0,
            lifecycle: Lifecycle::Active,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OperatingSystem {
    Windows,
    Macos,
    Linux,
    Android,
    Ios,
    Ohos,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlatformInfo {
    pub name: String,
    pub os: Option<OperatingSystem>,
}

impl PlatformInfo {
    pub fn validate(&self) -> io::Result<()> {
        if self.name.is_empty()
            || self.name.len() > 64
            || !self
                .name
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
            || !self.name.as_bytes()[0].is_ascii_lowercase()
        {
            return Err(invalid("invalid platform name"));
        }
        Ok(())
    }
}
fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}
fn language(value: &str) -> bool {
    // Bounded BCP47-shaped identifiers; no CLDR lookup or canonicalization.
    let mut parts = value.split('-');
    value.len() <= 85
        && parts.next().is_some_and(|p| {
            (2..=8).contains(&p.len()) && p.bytes().all(|c| c.is_ascii_alphabetic())
        })
        && parts.all(|p| (1..=8).contains(&p.len()) && p.bytes().all(|c| c.is_ascii_alphanumeric()))
}
impl EnvironmentSnapshot {
    pub const MAX_JSON_BYTES: usize = 16 * 1024;
    pub fn validate(&self) -> io::Result<()> {
        if let Some(platform) = &self.platform {
            platform.validate()?;
        }
        let l = &self.locale;
        if !language(&l.locale)
            || l.languages.is_empty()
            || l.languages.len() > 16
            || l.languages.iter().any(|v| !language(v))
            || l.languages
                .iter()
                .enumerate()
                .any(|(i, v)| l.languages[..i].iter().any(|p| p.eq_ignore_ascii_case(v)))
            || l.first_day_of_week.is_some_and(|d| !(1..=7).contains(&d))
            || !self.font_scale.is_finite()
            || !(0.5..=4.0).contains(&self.font_scale)
        {
            return Err(invalid(
                "invalid locale, preference count, weekday or font scale",
            ));
        }
        if let Some(zone) = &l.time_zone {
            let id = &zone.id;
            let common = !id.is_empty()
                && id.len() <= 128
                && id.trim() == id
                && !id.chars().any(char::is_control);
            let valid = match zone.kind {
                TimeZoneKind::Windows => common,
                TimeZoneKind::Iana => {
                    common
                        && id.is_ascii()
                        && id.split('/').all(|p| {
                            !p.is_empty()
                                && p != "."
                                && p != ".."
                                && p.bytes()
                                    .all(|c| c.is_ascii_alphanumeric() || b"_+-".contains(&c))
                        })
                }
                TimeZoneKind::Offset => {
                    let b = id.as_bytes();
                    b.len() == 6
                        && matches!(b[0], b'+' | b'-')
                        && b[3] == b':'
                        && [1, 2, 4, 5].iter().all(|i| b[*i].is_ascii_digit())
                        && &id[1..3] <= "23"
                        && &id[4..6] <= "59"
                }
            };
            if !valid {
                return Err(invalid("invalid or over-budget time-zone identifier"));
            }
        }
        Ok(())
    }
    pub fn from_json(bytes: &[u8]) -> io::Result<Self> {
        if bytes.len() > Self::MAX_JSON_BYTES {
            return Err(invalid("environment JSON exceeds budget"));
        }
        let value: Self = serde_json::from_slice(bytes)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        value.validate()?;
        Ok(value)
    }
}

#[derive(Clone, Debug, PartialEq)]
struct Version {
    revision: u64,
    value: EnvironmentSnapshot,
}
/// UI-thread writer. Create under a reactive owner; disposal closes late updates.
pub struct EnvironmentHost {
    identity: u64,
    state: Store<Version>,
    thread: PhantomData<Rc<()>>,
}
/// Read-only, root-local context value; no global preference singleton.
#[derive(Clone, Copy)]
pub struct Environment {
    identity: u64,
    state: ReadStore<Version>,
    thread: PhantomData<Rc<()>>,
}
impl EnvironmentHost {
    pub fn new(initial: EnvironmentSnapshot) -> io::Result<Self> {
        initial.validate()?;
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let identity = NEXT
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .map_err(|_| io::Error::other("environment identities exhausted"))?;
        Ok(Self {
            identity,
            state: create_store(Version {
                revision: 1,
                value: initial,
            }),
            thread: PhantomData,
        })
    }
    pub fn environment(&self) -> Environment {
        Environment {
            identity: self.identity,
            state: self.state.read_only(),
            thread: PhantomData,
        }
    }
    /// A complete snapshot is committed atomically. Stale writers, disposed roots
    /// and invalid values leave all preferences and the revision unchanged.
    pub fn apply(&self, expected_revision: u64, next: EnvironmentSnapshot) -> io::Result<bool> {
        let previous = self.state.read_only().try_peek().ok_or_else(|| {
            io::Error::new(io::ErrorKind::BrokenPipe, "environment owner disposed")
        })?;
        if previous.revision != expected_revision {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "environment revision conflict",
            ));
        }
        next.validate()?;
        if next == previous.value {
            return Ok(false);
        }
        let revision = previous
            .revision
            .checked_add(1)
            .ok_or_else(|| io::Error::other("environment revision exhausted"))?;
        self.state.set(Version {
            revision,
            value: next,
        });
        Ok(true)
    }
}
impl Environment {
    pub fn identity(&self) -> u64 {
        self.identity
    }
    pub fn revision(&self) -> u64 {
        self.state.peek().revision
    }
    pub fn snapshot(&self) -> EnvironmentSnapshot {
        self.state.peek().value
    }
    pub fn is_alive(&self) -> bool {
        self.state.is_alive()
    }
    /// Equality-filtered readonly projection using the existing Memo runtime.
    /// Projections may recompute on a sibling write; unchanged results do not
    /// rerun their downstream effects. Create under the intended consumer owner.
    pub fn select<T: Clone + PartialEq + 'static>(
        &self,
        projection: impl Fn(&EnvironmentSnapshot) -> T + 'static,
    ) -> Memo<T> {
        let state = self.state;
        create_memo(move || state.with(|v| projection(&v.value)))
    }
}

#[cfg(all(
    target_os = "linux",
    not(target_env = "ohos"),
    feature = "system-environment"
))]
#[path = "environment_linux.rs"]
mod linux;
#[path = "environment_watch.rs"]
mod watch;
#[cfg(all(windows, feature = "system-environment"))]
#[path = "environment_windows.rs"]
mod windows;
pub use watch::SystemEnvironmentWatcher;
/// Read the current OS preferences. Implemented for Windows and Linux;
/// other targets/configurations explicitly return Unsupported.
pub fn system_snapshot() -> io::Result<EnvironmentSnapshot> {
    #[cfg(all(windows, feature = "system-environment"))]
    {
        windows::snapshot()
    }
    #[cfg(all(
        target_os = "linux",
        not(target_env = "ohos"),
        feature = "system-environment"
    ))]
    {
        linux::snapshot()
    }
    #[cfg(not(all(
        any(windows, all(target_os = "linux", not(target_env = "ohos"))),
        feature = "system-environment"
    )))]
    {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "system environment adapter unavailable",
        ))
    }
}
