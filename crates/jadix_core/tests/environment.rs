use jadix_core::{
    create_context, create_effect, create_root, environment::*, provide_context, use_context,
};
use std::{cell::Cell, io, rc::Rc};

#[test]
fn external_platform_identity_preserves_the_physical_os_and_legacy_snapshots() {
    let mut snapshot = EnvironmentSnapshot::default();
    let legacy = serde_json::to_vec(&snapshot).unwrap();
    assert!(!String::from_utf8_lossy(&legacy).contains("layout_direction"));
    assert_eq!(
        EnvironmentSnapshot::from_json(&legacy).unwrap().platform,
        None
    );
    for os in [
        OperatingSystem::Android,
        OperatingSystem::Ios,
        OperatingSystem::Ohos,
    ] {
        snapshot.platform = Some(PlatformInfo {
            name: "miniprogram".into(),
            os: Some(os),
        });
        let parsed =
            EnvironmentSnapshot::from_json(&serde_json::to_vec(&snapshot).unwrap()).unwrap();
        assert_eq!(parsed.platform, snapshot.platform);
    }
    for name in ["", "MiniProgram", "../ohos", "1wechat", "wechat/platform"] {
        snapshot.platform.as_mut().unwrap().name = name.into();
        assert!(snapshot.validate().is_err());
    }
}

#[test]
fn layout_direction_roundtrips_without_changing_legacy_defaults() {
    let mut snapshot = EnvironmentSnapshot::default();
    assert_eq!(snapshot.layout_direction, None);
    assert_eq!(snapshot.high_contrast, None);
    snapshot.layout_direction = Some(LayoutDirection::RightToLeft);
    snapshot.high_contrast = Some(true);
    let encoded = serde_json::to_vec(&snapshot).unwrap();
    assert!(String::from_utf8_lossy(&encoded).contains("right-to-left"));
    assert!(String::from_utf8_lossy(&encoded).contains("high_contrast"));
    assert_eq!(EnvironmentSnapshot::from_json(&encoded).unwrap(), snapshot);
}

#[test]
fn root_local_atomic_preferences_filter_subscribers_and_close_on_disposal() {
    let context = create_context::<Environment>();
    let hour_runs = Rc::new(Cell::new(0));
    let theme_runs = Rc::new(Cell::new(0));
    let all_runs = Rc::new(Cell::new(0));
    let (owner, (host, other, env)) = create_root(|| {
        let host = EnvironmentHost::new(EnvironmentSnapshot::default()).unwrap();
        let other = EnvironmentHost::new(EnvironmentSnapshot::default()).unwrap();
        let env = host.environment();
        provide_context(context, env);
        assert_eq!(use_context(context).unwrap().identity(), env.identity());
        let hour = env.select(|v| v.locale.hour_cycle);
        let theme = env.select(|v| v.color_scheme);
        let combined = env.select(|v| (v.locale.hour_cycle, v.color_scheme));
        let n = hour_runs.clone();
        create_effect(move || {
            hour.get();
            n.set(n.get() + 1);
        });
        let n = theme_runs.clone();
        create_effect(move || {
            theme.get();
            n.set(n.get() + 1);
        });
        let n = all_runs.clone();
        create_effect(move || {
            let value = combined.get();
            assert!(
                value == (None, None) || value == (Some(HourCycle::H23), Some(ColorScheme::Light))
            );
            n.set(n.get() + 1);
        });
        (host, other, env)
    });
    assert_ne!(env.identity(), other.environment().identity());
    let mut next = env.snapshot();
    next.locale.hour_cycle = Some(HourCycle::H23);
    next.color_scheme = Some(ColorScheme::Light);
    assert!(host.apply(1, next.clone()).unwrap());
    assert_eq!(env.revision(), 2);
    assert_eq!(
        (hour_runs.get(), theme_runs.get(), all_runs.get()),
        (2, 2, 2)
    );
    next.locale.time_zone = Some(TimeZone {
        kind: TimeZoneKind::Iana,
        id: "Asia/Shanghai".into(),
    });
    assert!(host.apply(2, next.clone()).unwrap());
    assert_eq!(
        (hour_runs.get(), theme_runs.get(), all_runs.get()),
        (2, 2, 2)
    );
    assert!(!host.apply(3, next.clone()).unwrap());
    assert_eq!(env.revision(), 3);
    assert_eq!(
        host.apply(1, next).unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    assert_eq!(
        other.environment().snapshot(),
        EnvironmentSnapshot::default()
    );
    owner.dispose();
    assert!(!env.is_alive());
    assert_eq!(
        host.apply(3, EnvironmentSnapshot::default())
            .unwrap_err()
            .kind(),
        io::ErrorKind::BrokenPipe
    );
    assert_eq!(hour_runs.get(), 2);
}
#[test]
fn untrusted_preferences_fail_atomically_and_unknown_values_roundtrip() {
    let base = EnvironmentSnapshot::default();
    let (owner, host) = create_root(|| EnvironmentHost::new(base.clone()).unwrap());
    let bytes = serde_json::to_vec(&base).unwrap();
    assert_eq!(EnvironmentSnapshot::from_json(&bytes).unwrap(), base);
    for case in 0..10 {
        let mut next = base.clone();
        match case {
            0 => next.font_scale = f32::NAN,
            1 => next.font_scale = 0.0,
            2 => next.font_scale = 4.1,
            3 => next.locale.languages.clear(),
            4 => next.locale.languages = vec!["en-US".into(), "EN-us".into()],
            5 => next.locale.locale = "en\u{1b}[31m".into(),
            6 => next.locale.languages = vec!["en".into(); 17],
            7 => next.locale.first_day_of_week = Some(0),
            8 => {
                next.locale.time_zone = Some(TimeZone {
                    kind: TimeZoneKind::Offset,
                    id: "+24:00".into(),
                })
            }
            _ => {
                next.locale.time_zone = Some(TimeZone {
                    kind: TimeZoneKind::Iana,
                    id: "../invalid".into(),
                })
            }
        }
        assert!(host.apply(1, next).is_err());
        assert_eq!(host.environment().snapshot(), base);
        assert_eq!(host.environment().revision(), 1);
    }
    assert!(
        EnvironmentSnapshot::from_json(&vec![b' '; EnvironmentSnapshot::MAX_JSON_BYTES + 1])
            .is_err()
    );
    let unknown = String::from_utf8(bytes)
        .unwrap()
        .replace("\"font_scale\":1.0", "\"font_scale\":1.0,\"secret\":true");
    assert!(EnvironmentSnapshot::from_json(unknown.as_bytes()).is_err());
    let mut v = base;
    v.locale.time_zone = Some(TimeZone {
        kind: TimeZoneKind::Windows,
        id: "China Standard Time".into(),
    });
    assert!(host.apply(1, v).unwrap());
    owner.dispose();
}
#[test]
fn system_adapter_reports_real_values_or_explicit_unavailability() {
    match system_snapshot() {
        Ok(value) => {
            value.validate().unwrap();
            println!("{}", serde_json::to_string(&value).unwrap());
            #[cfg(windows)]
            if let Some(zone) = &value.locale.time_zone {
                assert_eq!(zone.kind, TimeZoneKind::Windows);
            }
        }
        Err(e) => {
            #[cfg(all(
                any(windows, all(target_os = "linux", not(target_env = "ohos"))),
                feature = "system-environment"
            ))]
            panic!("System adapter failed: {e}");
            #[cfg(not(all(
                any(windows, all(target_os = "linux", not(target_env = "ohos"))),
                feature = "system-environment"
            )))]
            assert_eq!(e.kind(), io::ErrorKind::Unsupported);
        }
    }
}
