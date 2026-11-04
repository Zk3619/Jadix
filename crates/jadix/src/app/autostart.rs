//! Per-user autostart registration using the `auto-launch` platform backend.
//!
//! ```no_run
//! let launch = jadix::autostart::current_app("com.example.widget")?
//!     .build()?;
//! launch.enable()?;
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

pub use auto_launch::{
    AutoLaunch, AutoLaunchBuilder, LinuxLaunchMode, MacOSLaunchMode, WindowsEnableMode,
};

/// Build a per-user autostart entry for this executable.
pub fn current_app(name: &str) -> std::io::Result<AutoLaunchBuilder> {
    let executable = std::env::current_exe()?;
    let path = executable.to_str().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "autostart executable path must be valid Unicode",
        )
    })?;
    let mut builder = AutoLaunchBuilder::new();
    builder.set_app_name(name).set_app_path(&path);

    #[cfg(target_os = "windows")]
    builder.set_windows_enable_mode(WindowsEnableMode::CurrentUser);
    #[cfg(target_os = "macos")]
    builder.set_macos_launch_mode(MacOSLaunchMode::LaunchAgent);
    #[cfg(target_os = "linux")]
    builder.set_linux_launch_mode(LinuxLaunchMode::XdgAutostart);

    Ok(builder)
}
