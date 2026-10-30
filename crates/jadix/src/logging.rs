//! 可选运行时日志（`--features runtime-logging`）。
//!
//! - **控制台**：经 `env_logger` 写 stderr；过滤由 `RUST_LOG` 控制。
//! - **文件**：设置 `JADIX_LOG_FILE` 为路径时，同一格式的行会追加写入该文件。
//!
//! 未启用 feature 时 [`init`] / [`init_with`] 为空操作。已初始化过则再次调用也会安全返回。

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
        use std::fs::OpenOptions;
        use std::io::Write;
        use std::sync::{Arc, Mutex};

        let file = std::env::var_os("JADIX_LOG_FILE").and_then(|path| {
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .ok()
                .map(|f| Arc::new(Mutex::new(f)))
        });

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
            if let Some(f) = &file {
                if let Ok(mut g) = f.lock() {
                    let _ = writeln!(g, "{line}");
                    let _ = g.flush();
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
