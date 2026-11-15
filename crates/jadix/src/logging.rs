//! 可选运行时日志（`--features runtime-logging`）。
//!
//! - **控制台**：经 `env_logger` 写 stderr；过滤由 `RUST_LOG` 控制。
//! - **文件**：默认写入当前应用的数据目录 `logs/app.log`；设置 `JADIX_LOG_FILE` 时使用该路径。
//!   文件达到 5 MiB 后轮转，保留最近 3 个文件。
//!
//! 未启用 feature 时 [`init`] / [`init_with`] 为空操作。已初始化过则再次调用也会安全返回。

use std::path::PathBuf;
use std::sync::OnceLock;

const MAX_LOG_BYTES: u64 = 5 * 1024 * 1024;
const LOG_BACKUPS: usize = 3;
static LOG_FILE_PATH: OnceLock<Option<PathBuf>> = OnceLock::new();

/// 返回已启用日志文件的路径；`runtime-logging` 未启用或文件不可用时返回 `None`。
pub fn log_file_path() -> Option<PathBuf> {
    LOG_FILE_PATH.get().cloned().flatten()
}

fn resolve_log_file_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("JADIX_LOG_FILE") {
        return Some(path.into());
    }
    if let Some(directory) = std::env::var_os("JADIX_LOG_DIR") {
        return Some(PathBuf::from(directory).join("logs").join("app.log"));
    }

    let app_name = std::env::current_exe()
        .ok()?
        .file_stem()?
        .to_string_lossy()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    let data_dir = if cfg!(target_os = "windows") {
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|home| PathBuf::from(home).join("Library/Application Support"))
    } else if cfg!(target_env = "ohos") {
        std::env::var_os("APP_DATA_DIR").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state"))
            })
    }?;
    Some(data_dir.join(app_name).join("logs").join("app.log"))
}

fn rotate_log_file(path: &std::path::Path, max_bytes: u64, backups: usize) -> std::io::Result<()> {
    let Ok(metadata) = std::fs::metadata(path) else {
        return Ok(());
    };
    if metadata.len() < max_bytes {
        return Ok(());
    }

    for index in (1..=backups).rev() {
        let source = if index == 1 {
            path.to_owned()
        } else {
            rotated_path(path, index - 1)
        };
        let destination = rotated_path(path, index);
        if source.exists() {
            if destination.exists() {
                std::fs::remove_file(&destination)?;
            }
            std::fs::rename(source, destination)?;
        }
    }
    Ok(())
}

fn rotated_path(path: &std::path::Path, index: usize) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".{index}"));
    path.with_file_name(name)
}

fn write_log_line(path: &std::path::Path, line: &str) -> std::io::Result<()> {
    use std::io::Write;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    rotate_log_file(path, MAX_LOG_BYTES, LOG_BACKUPS)?;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    writeln!(file, "{line}")
}

/// [`init_with`] 的选项。
#[derive(Clone, Copy, Debug)]
pub struct InitOptions {
    /// `RUST_LOG` 未设置时的默认过滤器（`env_logger` 语法）。
    pub default_filter: &'static str,
}

impl Default for InitOptions {
    fn default() -> Self {
        Self {
            // 与历史 `lifecycle` 默认一致：框架告警级，压制部分噪声依赖。
            default_filter: "warn,wgpu::hal::vulkan::instance=off,fontdb=error",
        }
    }
}

/// 使用 [`InitOptions::default`] 初始化全局 logger（至多一次）。
pub fn init() {
    init_with(InitOptions::default());
}

/// 按选项初始化全局 logger（至多一次）。
pub fn init_with(opts: InitOptions) {
    #[cfg(feature = "runtime-logging")]
    {
        use std::io::Write;
        let file = resolve_log_file_path().filter(|path| {
            path.parent()
                .is_none_or(|parent| std::fs::create_dir_all(parent).is_ok())
                && std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(path)
                    .is_ok()
        });
        let _ = LOG_FILE_PATH.set(file.clone());

        let mut builder = env_logger::Builder::from_env(
            env_logger::Env::default().default_filter_or(opts.default_filter),
        );
        builder.format(move |buf, record| {
            let ts = buf.timestamp();
            let line = format!(
                "[{} {:<5} {}] {}",
                ts,
                record.level(),
                record.target(),
                record.args()
            );
            writeln!(buf, "{line}")?;
            if let Some(path) = &file {
                if let Ok(_guard) = FILE_LOCK.lock() {
                    let _ = write_log_line(path, &line);
                }
            }
            Ok(())
        });
        let _ = builder.try_init();
    }
    #[cfg(not(feature = "runtime-logging"))]
    {
        let _ = opts;
    }
}

#[cfg(feature = "runtime-logging")]
static FILE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
mod tests {
    use super::{rotate_log_file, rotated_path};
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicUsize, Ordering},
    };

    static NEXT_ID: AtomicUsize = AtomicUsize::new(0);

    fn temp_log_path() -> PathBuf {
        std::env::temp_dir().join(format!(
            "jadix-log-{}-{}.log",
            std::process::id(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[test]
    fn rotates_in_order_and_removes_the_oldest_backup() {
        let path = temp_log_path();
        fs::write(&path, "current").unwrap();
        fs::write(rotated_path(&path, 1), "one").unwrap();
        fs::write(rotated_path(&path, 2), "two").unwrap();
        fs::write(rotated_path(&path, 3), "oldest").unwrap();

        rotate_log_file(&path, 1, 3).unwrap();

        assert!(!path.exists());
        assert_eq!(
            fs::read_to_string(rotated_path(&path, 1)).unwrap(),
            "current"
        );
        assert_eq!(fs::read_to_string(rotated_path(&path, 2)).unwrap(), "one");
        assert_eq!(fs::read_to_string(rotated_path(&path, 3)).unwrap(), "two");
        for index in 1..=3 {
            let _ = fs::remove_file(rotated_path(&path, index));
        }
    }

    #[test]
    fn leaves_a_log_below_the_rotation_limit_in_place() {
        let path = temp_log_path();
        fs::write(&path, "small").unwrap();
        rotate_log_file(&path, 100, 3).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "small");
        let _ = fs::remove_file(path);
    }

    #[cfg(not(feature = "runtime-logging"))]
    #[test]
    fn path_is_unavailable_without_runtime_logging() {
        assert_eq!(super::log_file_path(), None);
    }
}
