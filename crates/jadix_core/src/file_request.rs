//! Native file operations shared by platform hosts. Cancelling a picker returns
//! an empty path list; operational failures remain errors.
use std::path::{Path, PathBuf};

pub type FileResult = Result<Vec<PathBuf>, String>;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileSelection {
    #[default]
    Files,
    Folders,
    FilesAndFolders,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FilePanelOptions {
    #[serde(default)]
    pub content_types: Vec<String>,
    pub directory: Option<PathBuf>,
    pub title: Option<String>,
    pub prompt: Option<String>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum FileRequest {
    PickPaths {
        selection: FileSelection,
        multiple: bool,
        options: FilePanelOptions,
    },
    PickSavePath {
        file_name: Option<String>,
        options: FilePanelOptions,
    },
    Open {
        path: PathBuf,
    },
    Reveal {
        path: PathBuf,
    },
    Trash {
        path: PathBuf,
    },
}

fn valid_path(path: &Path) -> bool {
    path.is_absolute()
        && path.as_os_str().len() <= 32_768
        && !path.as_os_str().to_string_lossy().contains('\0')
}
impl FileRequest {
    pub fn validate(&self) -> Result<(), String> {
        let options = match self {
            Self::Open { path } | Self::Reveal { path } | Self::Trash { path } => {
                return if valid_path(path) {
                    Ok(())
                } else {
                    Err("file path must be absolute and bounded".into())
                };
            }
            Self::PickPaths { options, .. } => options,
            Self::PickSavePath { file_name, options } => {
                if file_name.as_ref().is_some_and(|n| {
                    n.is_empty()
                        || n.len() > 255
                        || n.contains(['/', '\\', '\0'])
                        || matches!(n.as_str(), "." | "..")
                }) {
                    return Err("invalid save file name".into());
                }
                options
            }
        };
        if options.directory.as_ref().is_some_and(|p| !valid_path(p))
            || [&options.title, &options.prompt].iter().any(|s| {
                s.as_ref()
                    .is_some_and(|s| s.len() > 4096 || s.contains('\0'))
            })
            || options.content_types.len() > 64
            || options
                .content_types
                .iter()
                .any(|s| s.is_empty() || s.len() > 256 || s.contains(['\0', '\n', '\r']))
        {
            return Err("invalid file panel options".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reject_paths_and_names_before_opening_a_panel() {
        assert!(FileRequest::Open {
            path: "relative".into()
        }
        .validate()
        .is_err());
        for name in ["../escape", "a\\b", "", "..", "a\0b"] {
            assert!(FileRequest::PickSavePath {
                file_name: Some(name.into()),
                options: Default::default()
            }
            .validate()
            .is_err());
        }
        assert!(FileRequest::PickSavePath {
            file_name: Some("中文🦀.txt".into()),
            options: Default::default()
        }
        .validate()
        .is_ok());
    }
}
