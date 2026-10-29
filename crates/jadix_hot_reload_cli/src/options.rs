use std::io;
use std::path::{Path, PathBuf};

#[derive(Default, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
struct DevConfig {
    #[serde(rename = "hot-reload")]
    hot_reload: WatchConfig,
}

#[derive(Default, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
struct WatchConfig {
    root: Option<PathBuf>,
    addr: Option<String>,
    watch: Vec<PathBuf>,
    exclude: Vec<PathBuf>,
    run: Vec<String>,
}

fn read_config(path: &Path) -> io::Result<WatchConfig> {
    let source = std::fs::read_to_string(path)?;
    let config: DevConfig = toml::from_str(&source).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{}: {error}", path.display()),
        )
    })?;
    Ok(config.hot_reload)
}

pub(super) struct Options {
    pub root: PathBuf,
    pub addr: String,
    pub watch: Vec<PathBuf>,
    pub exclude: Vec<PathBuf>,
    pub run: Vec<String>,
}

impl Options {
    pub fn parse(args: impl Iterator<Item = String>) -> io::Result<Self> {
        let args: Vec<String> = args.collect();
        let mut filtered = Vec::new();
        let mut config_path = None;
        let mut iter = args.into_iter();
        while let Some(arg) = iter.next() {
            if arg == "--run" {
                filtered.push(arg);
                filtered.extend(iter);
                break;
            }
            if arg == "--config" {
                config_path =
                    Some(PathBuf::from(iter.next().ok_or_else(|| {
                        io::Error::other("missing value for --config")
                    })?));
            } else {
                filtered.push(arg);
            }
        }
        let default_config = workspace_root().join("jadix.dev.toml");
        let config_path =
            config_path.or_else(|| default_config.is_file().then_some(default_config));
        let (config, config_dir) = if let Some(path) = config_path {
            let path = path.canonicalize()?;
            (read_config(&path)?, path.parent().unwrap().to_path_buf())
        } else {
            (WatchConfig::default(), workspace_root())
        };
        let mut args = filtered.into_iter().peekable();
        let mut root = std::env::var_os("JADIX_HOT_RELOAD_ROOT").map(PathBuf::from);
        let mut addr =
            std::env::var("JADIX_HOT_RELOAD_ADDR").unwrap_or_else(|_| super::DEFAULT_ADDR.into());
        let mut watch = Vec::new();
        let mut addr_overridden = std::env::var_os("JADIX_HOT_RELOAD_ADDR").is_some();
        let mut exclude = Vec::new();
        let mut run = Vec::new();
        let mut positional = 0;
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--root" | "--watch" | "--exclude" | "--addr" => {
                    let value = args
                        .next()
                        .ok_or_else(|| io::Error::other(format!("missing value for {arg}")))?;
                    match arg.as_str() {
                        "--root" => root = Some(PathBuf::from(value)),
                        "--watch" => watch.push(PathBuf::from(value)),
                        "--exclude" => exclude.push(PathBuf::from(value)),
                        _ => {
                            addr = value;
                            addr_overridden = true;
                        }
                    }
                }
                "--run" => {
                    if args.peek().is_some_and(|next| next == "--") {
                        args.next();
                    }
                    run.extend(args);
                    if run.is_empty() {
                        return Err(io::Error::other(
                            "--run requires a command, for example: --run -- cargo run -p gallery",
                        ));
                    }
                    break;
                }
                "--help" | "-h" => {
                    println!("jadix_hot_reload_cli [ROOT [ADDR]] [--config PATH] [--root PATH] [--watch PATH] [--exclude PATH] [--addr ADDR] [--run -- COMMAND ARGS...]\nDefault: load jadix.dev.toml from the Cargo workspace, or use --config. CLI/environment override TOML. Watch the Cargo workspace when no config is present; --watch and --exclude may be repeated. Paths are relative to ROOT.\n--run owns the command process tree and rebuilds on Rust logic/unsupported UI changes. Example: --run -- cargo run -p gallery");
                    std::process::exit(0);
                }
                value if value.starts_with('-') => {
                    return Err(io::Error::other(format!("unknown option: {value}")))
                }
                value => {
                    match positional {
                        0 => root = Some(PathBuf::from(value)),
                        1 => {
                            addr = value.to_string();
                            addr_overridden = true;
                        }
                        _ => return Err(io::Error::other("unexpected positional argument")),
                    }
                    positional += 1;
                }
            }
        }
        let root = root
            .or_else(|| config.root.map(|path| config_dir.join(path)))
            .unwrap_or(config_dir)
            .canonicalize()?;
        let root = PathBuf::from(super::normalize_path_text(&root));
        if !addr_overridden {
            if let Some(value) = config.addr {
                addr = value;
            }
        }
        if watch.is_empty() {
            if let Some(paths) = std::env::var_os("JADIX_HOT_RELOAD_WATCH") {
                watch.extend(std::env::split_paths(&paths));
            }
        }
        if watch.is_empty() {
            watch = config.watch;
        }
        if watch.is_empty() {
            watch.push(root.clone());
        }
        exclude.extend(config.exclude);
        if run.is_empty() {
            run = config.run;
        }
        for path in &mut watch {
            *path = PathBuf::from(super::normalize_path_text(
                &root.join(&*path).canonicalize()?,
            ));
            if !path.starts_with(&root) {
                return Err(io::Error::other("watch path must be inside ROOT"));
            }
        }
        for path in &mut exclude {
            *path = root.join(&*path);
        }
        Ok(Self {
            root,
            addr,
            watch,
            exclude,
            run,
        })
    }

    pub fn tracks(&self, path: &Path) -> bool {
        self.watch.iter().any(|watch| path.starts_with(watch))
            && !self
                .exclude
                .iter()
                .any(|excluded| path.starts_with(excluded))
            && super::is_tracked_project_file(path, &self.root)
    }
}

fn workspace_root() -> PathBuf {
    std::process::Command::new("cargo")
        .args(["locate-project", "--workspace", "--message-format", "plain"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .and_then(|manifest| {
            PathBuf::from(manifest.trim())
                .parent()
                .map(Path::to_path_buf)
        })
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}

#[cfg(test)]
#[path = "tests/options.rs"]
mod tests;
