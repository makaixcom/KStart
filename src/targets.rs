//! 快捷方式与热键共用的目标类型和路径表示。

use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetKind {
    #[default]
    File,
    Folder,
    System,
    Web,
    Launcher,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PathMode {
    Relative,
    Absolute,
}

impl TargetKind {
    pub fn infer(base: &Path, value: &str) -> Self {
        if matches!(value, "/AH/" | "/Search/") || value.starts_with("kstart:app-") {
            Self::Launcher
        } else if value.starts_with("kstart:")
            || value.starts_with("shell:")
            || value.eq_ignore_ascii_case("control.exe")
        {
            Self::System
        } else if crate::advanced_web_icon::is_web_url(value) {
            Self::Web
        } else if value.ends_with(['/', '\\']) || resolve_path(base, value).is_dir() {
            Self::Folder
        } else {
            Self::File
        }
    }

    pub fn path_mode(self, value: &str) -> Option<PathMode> {
        if value.is_empty() || !matches!(self, Self::File | Self::Folder) {
            return None;
        }
        Some(if Path::new(value).is_absolute() {
            PathMode::Absolute
        } else {
            PathMode::Relative
        })
    }
}

pub fn resolve_path(base: &Path, value: &str) -> PathBuf {
    let path = Path::new(value);
    if path.is_absolute() {
        path.to_owned()
    } else {
        base.join(path)
    }
}

pub fn convert_path(base: &Path, value: &str, mode: PathMode) -> Option<String> {
    let path = Path::new(value);
    if value.is_empty()
        || (!path.is_absolute() && path.has_root())
        || (!path.is_absolute() && matches!(path.components().next(), Some(Component::Prefix(_))))
    {
        return None;
    }
    fn normalized(path: &Path) -> PathBuf {
        let mut result = PathBuf::new();
        for part in path.components() {
            match part {
                Component::CurDir => {}
                Component::ParentDir => {
                    result.pop();
                }
                _ => result.push(part.as_os_str()),
            }
        }
        result
    }
    let absolute = normalized(&resolve_path(base, value));
    if mode == PathMode::Absolute {
        return absolute
            .is_absolute()
            .then(|| absolute.to_string_lossy().into_owned());
    }
    let base = normalized(base);
    let mut from = base.components();
    let mut to = absolute.components();
    if !from
        .next()?
        .as_os_str()
        .eq_ignore_ascii_case(to.next()?.as_os_str())
    {
        return None;
    }
    let (mut from, mut to) = (from.peekable(), to.peekable());
    while matches!((from.peek(), to.peek()), (Some(a), Some(b)) if a.as_os_str().eq_ignore_ascii_case(b.as_os_str()))
    {
        from.next();
        to.next();
    }
    let mut result = PathBuf::from(".");
    for _ in from {
        result.push("..");
    }
    for part in to {
        result.push(part.as_os_str());
    }
    Some(result.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_paths_round_trip_and_cross_drive_paths_stay_absolute() {
        let base = Path::new(r"D:\Apps\KStart");
        let relative = convert_path(base, r"D:\Apps\Tools\tools.exe", PathMode::Relative).unwrap();
        assert_eq!(Path::new(&relative), Path::new(r".\..\Tools\tools.exe"));
        assert_eq!(
            convert_path(base, &relative, PathMode::Absolute).as_deref(),
            Some(r"D:\Apps\Tools\tools.exe")
        );
        assert_eq!(
            convert_path(base, r"E:\Tools\tools.exe", PathMode::Relative),
            None
        );
        assert_eq!(convert_path(base, r"D:tools.exe", PathMode::Absolute), None);
    }
}
