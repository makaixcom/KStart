//! 栏目快捷方式的文件搜索、原生链接导出及完整配置交换。
use crate::{
    advanced::{AdvancedSettings, Key},
    model::{self, ShortcutItem, ShortcutRecord},
    skin_assets::RgbaSurface,
    targets::{self, TargetKind},
};
use serde::{Deserialize, Serialize};
use std::os::windows::fs::MetadataExt;
use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
};

pub enum ScanEvent {
    Files(Vec<PathBuf>),
    Finished(usize),
}

/// 文件搜索在工作线程运行，不跟随联接点，关闭窗口即可中止。
pub fn scan(
    root: PathBuf,
    patterns: String,
    recursive: bool,
    cancel: Arc<AtomicBool>,
) -> mpsc::Receiver<ScanEvent> {
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut directories = vec![root];
        let mut batch = Vec::new();
        let mut skipped = 0;
        while let Some(directory) = directories.pop() {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            let entries = match fs::read_dir(directory) {
                Ok(entries) => entries,
                Err(_) => {
                    skipped += 1;
                    continue;
                }
            };
            for entry in entries {
                if cancel.load(Ordering::Relaxed) {
                    break;
                }
                let Ok(entry) = entry else {
                    skipped += 1;
                    continue;
                };
                let Ok(metadata) = fs::symlink_metadata(entry.path()) else {
                    skipped += 1;
                    continue;
                };
                if metadata.is_dir() {
                    if recursive && metadata.file_attributes() & 0x400 == 0 {
                        directories.push(entry.path());
                    }
                } else if metadata.is_file()
                    && matches_patterns(&entry.file_name().to_string_lossy(), &patterns)
                {
                    batch.push(entry.path());
                    if batch.len() >= 64
                        && sender
                            .send(ScanEvent::Files(std::mem::take(&mut batch)))
                            .is_err()
                    {
                        return;
                    }
                }
            }
        }
        if !batch.is_empty() {
            let _ = sender.send(ScanEvent::Files(batch));
        }
        let _ = sender.send(ScanEvent::Finished(skipped));
    });
    receiver
}

fn matches_patterns(name: &str, patterns: &str) -> bool {
    let name = name.to_lowercase();
    patterns.split(';').any(|pattern| {
        let pattern = pattern.trim().to_lowercase();
        if pattern == "*.*" {
            return true;
        }
        let (text, pattern) = (name.as_bytes(), pattern.as_bytes());
        let (mut i, mut j, mut star, mut resume) = (0, 0, None, 0);
        while i < text.len() {
            if j < pattern.len() && (pattern[j] == b'?' || pattern[j] == text[i]) {
                i += 1;
                j += 1;
            } else if j < pattern.len() && pattern[j] == b'*' {
                star = Some(j);
                j += 1;
                resume = i;
            } else if let Some(index) = star {
                resume += 1;
                i = resume;
                j = index + 1;
            } else {
                return false;
            }
        }
        while j < pattern.len() && pattern[j] == b'*' {
            j += 1;
        }
        j == pattern.len() && !pattern.is_empty()
    })
}

#[derive(Serialize, Deserialize)]
struct PortableShortcut {
    record: ShortcutRecord,
    icon: Option<PortableIcon>,
}
#[derive(Serialize, Deserialize)]
struct PortableIcon {
    width: u32,
    height: u32,
    pixels: Vec<[u8; 4]>,
}
#[derive(Serialize, Deserialize)]
struct Archive {
    version: u32,
    shortcuts: Vec<PortableShortcut>,
}

fn absolute(base: &Path, value: &str) -> String {
    if value.is_empty() {
        String::new()
    } else {
        targets::resolve_path(base, value)
            .to_string_lossy()
            .into_owned()
    }
}

fn portable(base: &Path, item: &ShortcutItem) -> PortableShortcut {
    let target = if matches!(item.target_kind, TargetKind::File | TargetKind::Folder) {
        absolute(base, &item.target_path)
    } else {
        item.target_path.clone()
    };
    let icon_key =
        if item.custom_icon_16.is_some() || targets::resolve_path(base, &item.icon_key).is_file() {
            absolute(base, &item.icon_key)
        } else {
            item.icon_key.clone()
        };
    PortableShortcut {
        record: ShortcutRecord {
            column_added_at: item.column_added_at,
            id: item.id.clone(),
            category_id: item.category_id.clone(),
            column_id: item.column_id.clone(),
            name: item.name.clone(),
            target_path: target,
            target_kind: Some(item.target_kind),
            path_mode: None,
            work_dir_mode: None,
            icon_key,
            args: item.args.clone(),
            work_dir: absolute(base, &item.work_dir),
            remark: item.remark.clone(),
            text_color: if item.use_default_color {
                String::new()
            } else {
                model::format_hex_color(item.text_color)
            },
            minimize_on_run: item.minimize_on_run,
            sort_order: 0,
        },
        icon: item.custom_icon_16.as_ref().map(|icon| PortableIcon {
            width: icon.width,
            height: icon.height,
            pixels: icon.pixels().to_vec(),
        }),
    }
}

fn unique_file(folder: &Path, name: &str, extension: &str) -> io::Result<(PathBuf, fs::File)> {
    let clean: String = name
        .chars()
        .map(|c| {
            if c.is_control() || r#"<>:"/\|?*"#.contains(c) {
                '_'
            } else {
                c
            }
        })
        .take(100)
        .collect();
    let clean = clean.trim_matches([' ', '.']);
    let clean = if clean.is_empty() {
        "快捷方式"
    } else {
        clean
    };
    // 前缀也避开 CON、NUL 等 Windows 设备名，且绝不覆盖已有文件。
    let reserved = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    let clean = if reserved.contains(
        &clean
            .split('.')
            .next()
            .unwrap_or("")
            .to_uppercase()
            .as_str(),
    ) {
        format!("_{clean}")
    } else {
        clean.into()
    };
    for index in 0..10_000 {
        let suffix = if index == 0 {
            String::new()
        } else {
            format!(" ({index})")
        };
        let path = folder.join(format!("{clean}{suffix}.{extension}"));
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::other("同名文件过多"))
}

pub struct ExportReport {
    pub archive: PathBuf,
    pub native_count: usize,
    pub failures: Vec<String>,
}

pub fn export(base: &Path, folder: &Path, items: &[ShortcutItem]) -> io::Result<ExportReport> {
    let archive = Archive {
        version: 1,
        shortcuts: items.iter().map(|item| portable(base, item)).collect(),
    };
    let json = serde_json::to_vec_pretty(&archive).map_err(io::Error::other)?;
    let (path, mut file) = unique_file(folder, "KStart快捷方式", "kstart.json")?;
    if let Err(error) = file.write_all(&json) {
        drop(file);
        let _ = fs::remove_file(&path);
        return Err(error);
    }
    let mut report = ExportReport {
        archive: path,
        native_count: 0,
        failures: Vec::new(),
    };
    for item in items {
        if !matches!(
            item.target_kind,
            TargetKind::File | TargetKind::Folder | TargetKind::Web
        ) {
            continue;
        }
        let extension = if item.target_kind == TargetKind::Web {
            "url"
        } else {
            "lnk"
        };
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            let (path, mut output) = unique_file(folder, &item.name, extension)?;
            let result = if extension == "url" {
                output
                    .write_all(
                        format!(
                            "[InternetShortcut]\r\nURL={}\r\n",
                            item.target_path.replace(['\r', '\n'], "")
                        )
                        .as_bytes(),
                    )
                    .map_err(Into::into)
            } else {
                drop(output);
                crate::advanced_shell::write_file_link(
                    &path,
                    &absolute(base, &item.target_path),
                    &item.args,
                    &absolute(base, &item.work_dir),
                    &absolute(base, &item.icon_key),
                    &item.remark,
                )
            };
            if result.is_err() {
                let _ = fs::remove_file(path);
            }
            result
        })();
        match result {
            Ok(()) => report.native_count += 1,
            Err(error) => report.failures.push(format!("{}：{error}", item.name)),
        }
    }
    Ok(report)
}

pub fn import_file(
    path: &Path,
    base: &Path,
    settings: &AdvancedSettings,
) -> io::Result<Vec<ShortcutItem>> {
    if path.file_name().is_some_and(|name| {
        name.to_string_lossy()
            .to_lowercase()
            .ends_with(".kstart.json")
    }) {
        if fs::metadata(path)?.len() > 32 * 1024 * 1024 {
            return Err(io::Error::other("配置包超过 32 MB"));
        }
        let archive: Archive =
            serde_json::from_slice(&fs::read(path)?).map_err(io::Error::other)?;
        if archive.version != 1 {
            return Err(io::Error::other("不支持的快捷方式配置版本"));
        }
        return archive
            .shortcuts
            .into_iter()
            .map(|entry| {
                let icon = entry
                    .icon
                    .map(|icon| {
                        if icon.width == 0
                            || icon.height == 0
                            || icon.width > 256
                            || icon.height > 256
                            || icon.pixels.len() != icon.width as usize * icon.height as usize
                        {
                            return Err(io::Error::other("配置包中的图标无效"));
                        }
                        Ok(RgbaSurface::from_pixels(
                            icon.width,
                            icon.height,
                            icon.pixels,
                        ))
                    })
                    .transpose()?;
                let r = entry.record;
                Ok(ShortcutItem {
                    column_added_at: 0,
                    id: String::new(),
                    category_id: String::new(),
                    column_id: String::new(),
                    target_kind: r
                        .target_kind
                        .unwrap_or_else(|| TargetKind::infer(base, &r.target_path)),
                    name: r.name,
                    target_path: r.target_path,
                    icon_key: r.icon_key,
                    args: r.args,
                    work_dir: r.work_dir,
                    remark: r.remark,
                    use_default_color: r.text_color.is_empty(),
                    text_color: model::parse_hex_color(&r.text_color),
                    minimize_on_run: r.minimize_on_run,
                    custom_icon_16: icon,
                })
            })
            .collect();
    }
    let mut settings = settings.clone();
    settings.set(Key::ReadFileLink, "1".into());
    settings.set(Key::ReadUrl, "1".into());
    let dropped = crate::advanced_runtime::prepare_drop(base, &settings, path);
    if path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("url"))
        && crate::advanced_shell::url_link(path).is_none()
    {
        return Err(io::Error::other("网址快捷方式缺少 URL"));
    }
    let color = crate::advanced::color(settings.text(Key::DefaultColor))
        .unwrap_or(crate::skin_parser::RgbColor::new(0, 0, 0));
    Ok(vec![ShortcutItem {
        column_added_at: 0,
        id: String::new(),
        category_id: String::new(),
        column_id: String::new(),
        target_kind: TargetKind::infer(base, &dropped.target),
        name: dropped.name,
        target_path: dropped.target,
        args: dropped.args,
        work_dir: dropped.work_dir,
        icon_key: dropped.icon_source,
        custom_icon_16: dropped.icon,
        remark: String::new(),
        text_color: color,
        use_default_color: true,
        minimize_on_run: false,
    }])
}

/// 目录部分按 Windows 的大小写规则匹配，避免修改文件名中的同名片段。
pub fn replace_path_prefix(value: &str, before: &str, after: &str) -> String {
    let before = before.trim_end_matches(['\\', '/']);
    if before.is_empty() {
        return value.into();
    }
    let Some(prefix) = value.get(..before.len()) else {
        return value.into();
    };
    let rest = &value[before.len()..];
    if prefix.eq_ignore_ascii_case(before) && (rest.is_empty() || rest.starts_with(['\\', '/'])) {
        format!("{}{}", after.trim_end_matches(['\\', '/']), rest)
    } else {
        value.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Temp(PathBuf);
    impl Temp {
        fn new(name: &str) -> Self {
            let root =
                std::env::temp_dir().join(format!("kstart-transfer-{name}-{}", std::process::id()));
            fs::create_dir_all(&root).unwrap();
            Self(root)
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn search_filters_recursion_and_cancel() {
        let root = Temp::new("scan");
        fs::create_dir_all(root.0.join("nested")).unwrap();
        for name in ["test.URL", "test.lnk", "readme.txt", "nested/inside.url"] {
            fs::write(root.0.join(name), "").unwrap();
        }
        let collect = |recursive, cancel: bool| {
            let receiver = scan(
                root.0.clone(),
                "*.lnk; *.url".into(),
                recursive,
                Arc::new(AtomicBool::new(cancel)),
            );
            receiver
                .iter()
                .filter_map(|event| {
                    if let ScanEvent::Files(paths) = event {
                        Some(paths.len())
                    } else {
                        None
                    }
                })
                .sum::<usize>()
        };
        assert_eq!(collect(false, false), 2);
        assert_eq!(collect(true, false), 3);
        assert_eq!(collect(true, true), 0);
        assert!(matches_patterns("示例.kstart.json", "*.kstart.json"));
    }

    #[test]
    fn archive_round_trip_preserves_launch_fields_and_embedded_icon() {
        let root = Temp::new("roundtrip");
        let settings = AdvancedSettings::default();
        let tree = crate::model::AppDataStore::default_store().build_runtime_tree(
            &root.0,
            &Default::default(),
            &Default::default(),
        );
        let mut item = tree[0].groups[0].items[0].clone();
        item.name = "中文网站<>".into();
        item.target_kind = TargetKind::Web;
        item.target_path = "https://example.com/a?q=中文".into();
        item.args = "--profile \"测试\"".into();
        item.remark = "说明\n第二行".into();
        item.minimize_on_run = true;
        item.use_default_color = false;
        item.text_color = crate::skin_parser::RgbColor::new(1, 2, 3);
        item.custom_icon_16 = Some(RgbaSurface::from_pixels(1, 1, vec![[1, 2, 3, 255]]));
        let report = export(&root.0, &root.0, &[item.clone()]).unwrap();
        assert_eq!(report.native_count, 1);
        assert!(report.failures.is_empty());
        let loaded = import_file(&report.archive, &root.0, &settings)
            .unwrap()
            .remove(0);
        assert_eq!(loaded.target_path, item.target_path);
        assert_eq!(loaded.args, item.args);
        assert_eq!(loaded.remark, item.remark);
        assert!(loaded.minimize_on_run);
        assert_eq!(loaded.text_color.r, 1);
        assert_eq!(loaded.custom_icon_16.unwrap().pixels(), &[[1, 2, 3, 255]]);
        assert!(loaded.id.is_empty());
        assert_eq!(loaded.column_added_at, 0);
        let again = export(&root.0, &root.0, &[item]).unwrap();
        assert_ne!(again.archive, report.archive);
        assert!(root.0.join("中文网站__ (1).url").exists());
    }

    #[test]
    fn windows_links_round_trip_arguments_workdir_and_url() {
        let root = Temp::new("native");
        let target = std::env::current_exe()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let output = root.0.join("程序.lnk");
        crate::advanced_shell::write_file_link(
            &output,
            &target,
            "--profile \"中文\"",
            &root.0.to_string_lossy(),
            "",
            "备注",
        )
        .unwrap();
        let (path, args, directory) = crate::advanced_shell::file_link(&output).unwrap();
        assert_eq!(path, target);
        assert_eq!(args, "--profile \"中文\"");
        assert_eq!(directory, root.0.to_string_lossy());
        fs::write(
            root.0.join("网址.url"),
            "[InternetShortcut]\r\nURL=https://example.com/\r\n",
        )
        .unwrap();
        let imported = import_file(
            &root.0.join("网址.url"),
            &root.0,
            &AdvancedSettings::default(),
        )
        .unwrap();
        assert_eq!(imported[0].target_kind, TargetKind::Web);
        assert_eq!(imported[0].target_path, "https://example.com/");
    }

    #[test]
    fn directory_replacement_does_not_touch_sibling_prefix_or_filename() {
        assert_eq!(
            replace_path_prefix(r"D:\SOFT\App\app.exe", r"d:\soft", r"E:\Tools"),
            r"E:\Tools\App\app.exe"
        );
        assert_eq!(
            replace_path_prefix(r"D:\software\app.exe", r"D:\soft", r"E:\Tools"),
            r"D:\software\app.exe"
        );
        assert_eq!(
            replace_path_prefix("https://example.com/soft", "soft", "other"),
            "https://example.com/soft"
        );
    }
}
