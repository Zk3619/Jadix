use std::fmt;
use std::io;
use std::path::PathBuf;

/// Persistence / path errors.
#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    TomlDe(toml::de::Error),
    TomlSer(toml::ser::Error),
    Path(String),
    /// `current_dir` / `current_exe` failed while resolving the default location.
    Location(PathBuf, io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "window state io: {e}"),
            Self::TomlDe(e) => write!(f, "window state toml: {e}"),
            Self::TomlSer(e) => write!(f, "window state toml: {e}"),
            Self::Path(msg) => f.write_str(msg),
            Self::Location(path, e) => write!(f, "window state path {}: {e}", path.display()),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::TomlDe(e) => Some(e),
            Self::TomlSer(e) => Some(e),
            Self::Location(_, e) => Some(e),
            Self::Path(_) => None,
        }
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<toml::de::Error> for Error {
    fn from(e: toml::de::Error) -> Self {
        Self::TomlDe(e)
    }
}

impl From<toml::ser::Error> for Error {
    fn from(e: toml::ser::Error) -> Self {
        Self::TomlSer(e)
    }
}
