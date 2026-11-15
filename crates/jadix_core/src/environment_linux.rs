//! POSIX process locale plus optional GNOME GSettings. No desktop preference is
//! inferred from a locale name. Commands are bounded, owned and never use a shell.
use super::*;
use std::{
    io::Read,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

fn output(program: &str, args: &[&str]) -> Option<String> {
    let mut child = Command::new(program)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout
            .by_ref()
            .take(16 * 1024 + 1)
            .read_to_end(&mut bytes)
            .ok()?;
        Some(bytes)
    });
    let end = Instant::now() + Duration::from_secs(2);
    let success = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status.success(),
            Ok(None) if Instant::now() < end => std::thread::sleep(Duration::from_millis(10)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break false;
            }
        }
    };
    let bytes = reader.join().ok()??;
    (success && bytes.len() <= 16 * 1024)
        .then(|| String::from_utf8(bytes).ok())
        .flatten()
}
fn setting(key: &str) -> Option<String> {
    output("gsettings", &["get", "org.gnome.desktop.interface", key])
        .map(|v| v.trim().trim_matches('\'').to_owned())
}
fn posix_language(value: &str) -> Option<String> {
    let value = value
        .split('.')
        .next()?
        .split('@')
        .next()?
        .replace('_', "-");
    if matches!(value.as_str(), "C" | "POSIX") {
        Some("und".into())
    } else {
        language(&value).then_some(value)
    }
}
fn bounded_file(path: &str) -> Option<String> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .ok()?
        .take(257)
        .read_to_end(&mut bytes)
        .ok()?;
    (bytes.len() <= 256)
        .then(|| String::from_utf8(bytes).ok())
        .flatten()
        .map(|s| s.trim().to_owned())
}
fn zone() -> Option<TimeZone> {
    let id = std::env::var("TZ")
        .ok()
        .map(|s| s.trim_start_matches(':').to_owned())
        .or_else(|| {
            std::fs::canonicalize("/etc/localtime").ok().and_then(|p| {
                p.to_str()?
                    .split_once("/zoneinfo/")
                    .map(|(_, id)| id.to_owned())
            })
        })
        .or_else(|| bounded_file("/etc/timezone"))?;
    let kind = if matches!(id.as_str(), "UTC" | "GMT") || id.contains('/') && !id.starts_with('/') {
        TimeZoneKind::Iana
    } else {
        // A POSIX TZ offset has the opposite sign to ISO offsets. Preserve
        // unknown instead of incorrectly labelling it as an ISO offset.
        return None;
    };
    let value = TimeZone { kind, id };
    let mut check = EnvironmentSnapshot::default();
    check.locale.time_zone = Some(value.clone());
    check.validate().ok()?;
    Some(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn posix_language_keeps_only_valid_base_language_tags() {
        assert_eq!(posix_language("zh_CN.UTF-8"), Some("zh-CN".into()));
        assert_eq!(posix_language("C.UTF-8"), Some("und".into()));
        assert_eq!(posix_language("sr_RS@latin"), Some("sr-RS".into()));
        assert_eq!(posix_language(""), None);
        assert_eq!(posix_language("en\u{1b}[0m"), None);
    }
}
pub(super) fn snapshot() -> io::Result<EnvironmentSnapshot> {
    let name = ["LC_ALL", "LC_MESSAGES", "LANG"]
        .into_iter()
        .filter_map(|k| std::env::var(k).ok())
        .find(|s| !s.is_empty())
        .unwrap_or_else(|| "C".into());
    let locale = posix_language(&name)
        .ok_or_else(|| io::Error::other("unsupported POSIX locale identifier"))?;
    let mut languages = Vec::new();
    for value in std::env::var("LANGUAGE").unwrap_or_default().split(':') {
        if let Some(value) = posix_language(value) {
            if !languages
                .iter()
                .any(|v: &String| v.eq_ignore_ascii_case(&value))
            {
                languages.push(value);
            }
        }
    }
    if languages.is_empty() {
        languages.push(locale.clone());
    }
    let time = output("locale", &["-k", "LC_TIME"]).unwrap_or_default();
    let property = |key: &str| {
        time.lines()
            .find_map(|line| line.strip_prefix(key))
            .map(|s| s.trim_matches('"'))
    };
    let first = property("first_weekday=")
        .and_then(|v| v.parse::<u8>().ok())
        .filter(|n| (1..=7).contains(n));
    let base = match property("week-1stday=") {
        Some("19971130") => Some(7u8),
        Some("19971201") => Some(1),
        _ => None,
    };
    let format = property("t_fmt=").unwrap_or("");
    let hour_cycle = match setting("clock-format").as_deref() {
        Some("12h") => Some(HourCycle::H12),
        Some("24h") => Some(HourCycle::H23),
        _ => {
            if format.contains("%H") || format.contains("%k") {
                Some(HourCycle::H23)
            } else if format.contains("%I") || format.contains("%l") || format.contains("%r") {
                Some(HourCycle::H12)
            } else {
                None
            }
        }
    };
    let measurement = output("locale", &["-k", "LC_MEASUREMENT"]).and_then(|s| {
        s.lines().find_map(|l| match l {
            "measurement=1" => Some(MeasurementSystem::Metric),
            "measurement=2" => Some(MeasurementSystem::Us),
            _ => None,
        })
    });
    let value = EnvironmentSnapshot {
        platform: Some(PlatformInfo {
            name: "linux".into(),
            os: Some(OperatingSystem::Linux),
        }),
        locale: LocaleSettings {
            locale,
            languages,
            time_zone: zone(),
            hour_cycle,
            first_day_of_week: first.zip(base).map(|(day, base)| (day + base - 2) % 7 + 1),
            measurement_system: measurement,
            temperature_unit: None,
        },
        color_scheme: match setting("color-scheme").as_deref() {
            Some("prefer-dark") => Some(ColorScheme::Dark),
            Some("prefer-light") => Some(ColorScheme::Light),
            _ => None,
        },
        layout_direction: None,
        high_contrast: None,
        reduced_motion: match setting("enable-animations").as_deref() {
            Some("true") => Some(false),
            Some("false") => Some(true),
            _ => None,
        },
        font_scale: setting("text-scaling-factor")
            .and_then(|s| s.parse().ok())
            .unwrap_or(1.0),
        lifecycle: Lifecycle::Active,
    };
    value.validate()?;
    Ok(value)
}

pub(super) fn run_watch(
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    ready: std::sync::mpsc::SyncSender<io::Result<usize>>,
    mut changed: impl FnMut(),
) {
    use std::{os::fd::AsRawFd, sync::atomic::Ordering};
    let monitor = Command::new("gsettings")
        .args(["monitor", "org.gnome.desktop.interface"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn();
    let mut monitor = match monitor {
        Ok(v) => v,
        Err(e) => {
            let _ = ready.send(Err(e));
            return;
        }
    };
    let mut stdout = monitor.stdout.take().unwrap();
    let fd = unsafe { libc::inotify_init1(libc::IN_NONBLOCK | libc::IN_CLOEXEC) };
    if fd >= 0 {
        let path = c"/etc";
        unsafe {
            libc::inotify_add_watch(
                fd,
                path.as_ptr(),
                libc::IN_CLOSE_WRITE | libc::IN_MOVED_TO | libc::IN_ATTRIB,
            );
        }
    }
    let _ = ready.send(Ok(0));
    changed();
    let mut buffer = [0u8; 8192];
    while !stop.load(Ordering::Acquire) {
        let mut fds = [
            libc::pollfd {
                fd: stdout.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            },
            libc::pollfd {
                fd,
                events: libc::POLLIN,
                revents: 0,
            },
        ];
        let status = unsafe { libc::poll(fds.as_mut_ptr(), 2, 100) };
        if status < 0 {
            if io::Error::last_os_error().kind() == io::ErrorKind::Interrupted {
                continue;
            }
            break;
        }
        if fds[0].revents & (libc::POLLHUP | libc::POLLERR | libc::POLLNVAL) != 0 {
            break;
        }
        let mut dirty = false;
        if fds[0].revents & libc::POLLIN != 0 {
            match stdout.read(&mut buffer) {
                Ok(n) if n > 0 => dirty = true,
                _ => break,
            }
        }
        if fd >= 0 && fds[1].revents & libc::POLLIN != 0 {
            unsafe {
                libc::read(fd, buffer.as_mut_ptr().cast(), buffer.len());
            }
            dirty = true;
        }
        if dirty {
            changed();
        }
    }
    let _ = monitor.kill();
    let _ = monitor.wait();
    if fd >= 0 {
        unsafe {
            libc::close(fd);
        }
    }
}
