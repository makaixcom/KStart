//! 高级设置的对话框命令及系统操作。

use crate::{
    advanced::{self, color, AdvancedSettings, Key},
    advanced_dialog as dialog, apply_hrgn_safely, close_modal_window,
    native_dialogs::set_ctrl_text,
    pick_file_dialog, pick_folder_dialog, to_wide, ActiveModal, APP_STATE,
};
use std::{
    fs,
    hash::{DefaultHasher, Hash, Hasher},
    io, mem,
    path::{Path, PathBuf},
    ptr,
};
use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, HWND};
use windows_sys::Win32::Graphics::Gdi::{InvalidateRect, HFONT};
use windows_sys::Win32::System::Memory::{
    SetProcessWorkingSetSizeEx, QUOTA_LIMITS_HARDWS_MAX_DISABLE, QUOTA_LIMITS_HARDWS_MAX_ENABLE,
};
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegSetValueExW, HKEY_CURRENT_USER, KEY_SET_VALUE,
    REG_SZ,
};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, GetProcessWorkingSetSize, SetPriorityClass, ABOVE_NORMAL_PRIORITY_CLASS,
    BELOW_NORMAL_PRIORITY_CLASS, HIGH_PRIORITY_CLASS, IDLE_PRIORITY_CLASS, NORMAL_PRIORITY_CLASS,
};
use windows_sys::Win32::UI::Controls::Dialogs::{
    ChooseColorW, CC_FULLOPEN, CC_RGBINIT, CHOOSECOLORW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    MessageBoxW, IDCANCEL, IDOK, IDYES, MB_DEFBUTTON2, MB_ICONINFORMATION, MB_ICONWARNING, MB_OK,
    MB_YESNO,
};

fn categories(app: &crate::AppState) -> Vec<(String, String)> {
    app.categories
        .iter()
        .map(|cat| (cat.id.clone(), cat.name.clone()))
        .collect()
}

pub unsafe fn populate(hwnd: HWND, font: HFONT) -> bool {
    let init = APP_STATE.with(|cell| {
        let guard = cell.try_borrow().ok()?;
        let app = guard.as_ref()?;
        let ActiveModal::Advanced(draft) = &app.modal else {
            return None;
        };
        Some((
            draft.clone(),
            app.skins.iter().map(|s| s.name.clone()).collect::<Vec<_>>(),
            app.active_skin_index,
            categories(app),
            app.advanced_window.advanced_page,
        ))
    });
    if let Some((draft, skins, skin, cats, page)) = init {
        dialog::populate(hwnd, font, &draft, &skins, skin, &cats);
        dialog::select_page(hwnd, page);
        true
    } else {
        false
    }
}

pub fn message(hwnd: HWND, text: &str) {
    unsafe {
        MessageBoxW(
            hwnd,
            to_wide(text).as_ptr(),
            to_wide("高级设置").as_ptr(),
            MB_OK | MB_ICONINFORMATION,
        );
    }
}

pub fn confirm(hwnd: HWND, text: &str) -> bool {
    unsafe {
        MessageBoxW(
            hwnd,
            to_wide(text).as_ptr(),
            to_wide("KStart").as_ptr(),
            MB_YESNO | MB_ICONWARNING | MB_DEFBUTTON2,
        ) == IDYES
    }
}

pub unsafe fn command(hwnd: HWND, id: i32, notification: u32) -> bool {
    let snapshot = APP_STATE.with(|cell| {
        let guard = cell.try_borrow().ok()?;
        let app = guard.as_ref()?;
        let ActiveModal::Advanced(draft) = &app.modal else {
            return None;
        };
        Some((
            draft.clone(),
            app.base_dir.clone(),
            app.main_hwnd,
            categories(app),
            app.user_cfg.advanced.clone(),
        ))
    });
    let Some((draft, base, owner, cats, previous)) = snapshot else {
        return false;
    };
    if dialog::is_page_change(id, notification) {
        dialog::show_page(hwnd);
        return true;
    }
    if let Some((edit, key)) = dialog::picker(id) {
        if matches!(
            key,
            Key::ListBackgroundColor | Key::GroupColor | Key::DefaultColor
        ) {
            let initial = crate::native_dialogs::get_ctrl_text(hwnd, edit);
            if let Some(value) = pick_color(hwnd, &initial) {
                set_ctrl_text(hwnd, edit, &value);
            }
        } else {
            let filters: &[(&str, &str)] = if key == Key::ListBackground {
                &[("图像文件", "*.png;*.jpg;*.jpeg;*.bmp;*.gif")]
            } else {
                &[("应用程序", "*.exe"), ("所有文件", "*.*")]
            };
            if let Some(path) = pick_file_dialog(hwnd, "选择文件", filters) {
                set_ctrl_text(hwnd, edit, &path.to_string_lossy());
            }
        }
        return true;
    }
    match id {
        IDOK => match dialog::read(hwnd, draft, &cats) {
            Ok((settings, skin)) => {
                if let Err(error) = apply(owner, &base, settings, skin, &previous) {
                    message(hwnd, &error);
                } else {
                    close_modal_window();
                }
            }
            Err(error) => message(hwnd, &error),
        },
        IDCANCEL => close_modal_window(),
        dialog::ID_RESET => {
            if dialog::reset_page(hwnd) == 3 {
                let defaults = AdvancedSettings::default();
                set_fonts(defaults.fonts);
                set_spacing(defaults.spacing);
            }
        }
        dialog::ID_FONT => {
            if let Some(fonts) = edit_fonts(hwnd, &draft.fonts) {
                set_fonts(fonts);
            }
        }
        dialog::ID_SPACING => {
            if let Some(spacing) = crate::advanced_prompt::spacing(hwnd, draft.spacing) {
                set_spacing(spacing);
            }
        }
        dialog::ID_BACKUP => {
            if let Some(dir) = pick_folder_dialog(hwnd, "选择备份目录") {
                let folder = dir.join(format!(
                    "KStart-{}",
                    crate::local_clock::now().format("%Y%m%d-%H%M%S")
                ));
                match backup(&base, &folder) {
                    Ok(()) => message(hwnd, &format!("备份已保存到 {}", folder.display())),
                    Err(error) => message(hwnd, &format!("备份失败：{error}")),
                }
            }
        }
        dialog::ID_RESTORE => {
            if let Some(dir) = pick_folder_dialog(hwnd, "选择已有备份目录") {
                if confirm(hwnd, "恢复会替换当前设置和快捷方式。继续恢复？") {
                    match restore(owner, &base, &dir) {
                        Ok(()) => {
                            close_modal_window();
                            message(owner, "备份已恢复。重新启动可重新应用启动相关设置。");
                        }
                        Err(error) => message(hwnd, &format!("恢复失败：{error}")),
                    }
                }
            }
        }
        dialog::ID_CLEAR_CACHE => {
            if confirm(hwnd, "清除缓存并恢复默认图标？自选图标保留。") {
                if let Err(err) = clear_icon_cache(&base) {
                    message(hwnd, &format!("清空缓存失败：{err}"));
                } else {
                    InvalidateRect(owner, ptr::null(), 0);
                }
            }
        }
        dialog::ID_DESKTOP | dialog::ID_START_MENU => {
            if let Err(error) =
                crate::advanced_shell::install_shortcut(&base, id == dialog::ID_DESKTOP)
            {
                message(hwnd, &format!("创建快捷方式失败：{error}"));
            }
        }
        dialog::ID_LOGIN_PASSWORD | dialog::ID_LOCK_PASSWORD => {
            if let Err(error) =
                crate::advanced_prompt::edit_password(hwnd, &base, id == dialog::ID_LOCK_PASSWORD)
            {
                message(hwnd, &error.to_string());
            }
        }
        _ => {}
    }
    true
}

fn set_fonts(fonts: [advanced::FontSetting; 4]) {
    APP_STATE.with(|cell| {
        if let Ok(mut guard) = cell.try_borrow_mut() {
            if let Some(app) = guard.as_mut() {
                if let ActiveModal::Advanced(draft) = &mut app.modal {
                    draft.fonts = fonts;
                }
            }
        }
    });
}

fn set_spacing(spacing: [u32; 5]) {
    APP_STATE.with(|cell| {
        if let Ok(mut guard) = cell.try_borrow_mut() {
            if let Some(app) = guard.as_mut() {
                if let ActiveModal::Advanced(draft) = &mut app.modal {
                    draft.spacing = spacing;
                }
            }
        }
    });
}

unsafe fn pick_color(owner: HWND, initial: &str) -> Option<String> {
    let mut picker: CHOOSECOLORW = mem::zeroed();
    let mut palette = [0x00ffffff; 16];
    picker.lStructSize = mem::size_of::<CHOOSECOLORW>() as u32;
    picker.hwndOwner = owner;
    picker.lpCustColors = palette.as_mut_ptr();
    picker.rgbResult = color(initial).map_or(0, |c| c.to_colorref());
    picker.Flags = CC_FULLOPEN | CC_RGBINIT;
    (ChooseColorW(&mut picker) != 0).then(|| {
        let [r, g, b, _] = picker.rgbResult.to_le_bytes();
        format!("#{r:02X}{g:02X}{b:02X}")
    })
}

unsafe fn edit_fonts(
    owner: HWND,
    fonts: &[advanced::FontSetting; 4],
) -> Option<[advanced::FontSetting; 4]> {
    crate::advanced_prompt::fonts(owner, fonts)
}

fn autorun(root: &Path, enable: bool) -> io::Result<()> {
    let exe = std::env::current_exe()?;
    let command = to_wide(&format!(
        "\"{}\" --base-dir \"{}\"",
        exe.display(),
        root.display()
    ));
    let key = to_wide(r"Software\Microsoft\Windows\CurrentVersion\Run");
    let name = to_wide("KStart");
    let mut handle = 0;
    unsafe {
        let status = RegOpenKeyExW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            0,
            KEY_SET_VALUE,
            &mut handle,
        );
        if status != 0 {
            return Err(io::Error::from_raw_os_error(status as i32));
        }
        let status = if enable {
            RegSetValueExW(
                handle,
                name.as_ptr(),
                0,
                REG_SZ,
                command.as_ptr().cast(),
                (command.len() * 2) as u32,
            )
        } else {
            RegDeleteValueW(handle, name.as_ptr())
        };
        RegCloseKey(handle);
        if status != 0 && (enable || status != ERROR_FILE_NOT_FOUND) {
            return Err(io::Error::from_raw_os_error(status as i32));
        }
    }
    Ok(())
}

pub fn process_options(settings: &AdvancedSettings, previous: &AdvancedSettings) -> io::Result<()> {
    let priorities = [
        NORMAL_PRIORITY_CLASS,
        IDLE_PRIORITY_CLASS,
        BELOW_NORMAL_PRIORITY_CLASS,
        ABOVE_NORMAL_PRIORITY_CLASS,
        HIGH_PRIORITY_CLASS,
    ];
    unsafe {
        let process = GetCurrentProcess();
        if settings.number(Key::MemoryLimit) != previous.number(Key::MemoryLimit) {
            let mut minimum = 0;
            let mut maximum = 0;
            if GetProcessWorkingSetSize(process, &mut minimum, &mut maximum) == 0 {
                return Err(io::Error::last_os_error());
            }
            let limit = settings.number(Key::MemoryLimit) as usize * 1024 * 1024;
            let (max, flags) = if limit == 0 {
                (maximum, QUOTA_LIMITS_HARDWS_MAX_DISABLE)
            } else {
                (limit.max(minimum), QUOTA_LIMITS_HARDWS_MAX_ENABLE)
            };
            if SetProcessWorkingSetSizeEx(process, minimum, max, flags) == 0 {
                return Err(io::Error::last_os_error());
            }
        }
        if settings.number(Key::Priority) != previous.number(Key::Priority)
            && SetPriorityClass(process, priorities[settings.number(Key::Priority) as usize]) == 0
        {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

unsafe fn apply(
    owner: HWND,
    base: &Path,
    mut settings: AdvancedSettings,
    skin: usize,
    previous: &AdvancedSettings,
) -> Result<(), String> {
    settings.launch_counts = previous.launch_counts.clone();
    let (mut next_config, selected) = APP_STATE
        .with(|cell| {
            let guard = cell.try_borrow().ok()?;
            let app = guard.as_ref()?;
            let selected = (skin != app.active_skin_index)
                .then(|| app.skins.get(skin).cloned())
                .flatten();
            Some((app.user_cfg.clone(), selected))
        })
        .ok_or_else(|| "无法读取当前设置。".to_owned())?;
    next_config.advanced = settings.clone();
    if let Some(entry) = selected {
        let cfg = crate::skin_parser::SkinConfig::load_from_ini(&entry.setup_ini_path)?;
        crate::skin_assets::SkinAssets::load(&entry.dir_path, &cfg)?;
        next_config.configured_skin = Some(entry.name);
    }
    let auto_start_changed = settings.enabled(Key::AutoStart) != previous.enabled(Key::AutoStart);
    if auto_start_changed {
        autorun(base, settings.enabled(Key::AutoStart))
            .map_err(|e| format!("无法更新开机启动：{e}"))?;
    }
    if let Err(error) =
        process_options(&settings, previous).and_then(|_| next_config.try_save(base))
    {
        let _ = process_options(previous, &settings);
        if auto_start_changed {
            let _ = autorun(base, previous.enabled(Key::AutoStart));
        }
        return Err(format!("无法应用高级设置：{error}"));
    }
    let region = APP_STATE.with(|cell| {
        let mut guard = cell.try_borrow_mut().ok()?;
        let app = guard.as_mut()?;
        app.user_cfg = next_config;
        let region = (app.active_skin_index != skin)
            .then(|| app.switch_skin(skin))
            .flatten();
        app.reload_advanced_assets();
        region
    });
    if let Some(region) = region {
        apply_hrgn_safely(owner, region);
    }
    crate::advanced_window::update_tray(owner);
    crate::advanced_window::stretch_docked(owner);
    crate::advanced_window::apply_opacity(owner);
    InvalidateRect(owner, ptr::null(), 0);
    Ok(())
}

fn copy_backup_files(
    source: &Path,
    target: &Path,
    verification: &crate::config::Verification,
) -> io::Result<()> {
    fs::create_dir_all(target)?;
    crate::config::UserConfig::load_from(source).save_to(
        target,
        &crate::config::HotkeyConfig::load_from(source),
        verification,
    )?;
    fs::copy(source.join("data.json"), target.join("data.json"))?;
    let icons = source.join("icons");
    if let Ok(entries) = fs::read_dir(icons) {
        let dest = target.join("icons");
        fs::create_dir_all(&dest)?;
        for entry in entries {
            let entry = entry?;
            if entry.file_type()?.is_file()
                && entry
                    .path()
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("png"))
            {
                fs::copy(entry.path(), dest.join(entry.file_name()))?;
            }
        }
    }
    Ok(())
}

pub fn backup(base: &Path, target: &Path) -> io::Result<()> {
    if !base.join("user/data.json").is_file() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "找不到快捷方式数据",
        ));
    }
    let parent = target
        .parent()
        .ok_or_else(|| io::Error::other("没有可用的备份目录"))?;
    fs::create_dir_all(parent)?;
    let nonce = format!(
        "{}-{}",
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos()
    );
    let staging = parent.join(format!(".kstart-backup-{nonce}"));
    let previous = parent.join(format!(".kstart-previous-{nonce}"));
    fs::create_dir(&staging)?;
    let result = (|| {
        copy_backup_files(
            &base.join("user"),
            &staging,
            &crate::config::Verification::default(),
        )?;
        fs::write(staging.join(".kstart-backup"), b"1")?;
        if target.exists() {
            if !target.is_dir() {
                return Err(io::Error::other("备份位置已被文件占用"));
            }
            fs::rename(target, &previous)?;
        }
        if let Err(error) = fs::rename(&staging, target) {
            if previous.exists() {
                fs::rename(&previous, target)?;
            }
            return Err(error);
        }
        if previous.join(".kstart-backup").is_file() {
            let _ = fs::remove_dir_all(&previous);
        }
        Ok(())
    })();
    if result.is_err() && staging.exists() {
        let _ = fs::remove_dir_all(&staging);
    }
    result
}

fn restore(owner: HWND, base: &Path, source: &Path) -> io::Result<()> {
    let data: crate::model::AppDataStore =
        serde_json::from_slice(&fs::read(source.join("data.json"))?)?;
    if data.categories.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "备份中没有类别"));
    }
    if source.join("advanced-settings.json").is_file() {
        let _: AdvancedSettings =
            serde_json::from_slice(&fs::read(source.join("advanced-settings.json"))?)?;
    }
    let rollback = base.join("user/backups").join(format!(
        "restore-{}",
        crate::local_clock::now().format("%Y%m%d-%H%M%S")
    ));
    backup(base, &rollback)?;
    let user_dir = base.join("user");
    let verification = crate::config::Verification::load(&user_dir);
    if let Err(error) = copy_backup_files(source, &user_dir, &verification) {
        copy_backup_files(&rollback, &user_dir, &verification)?;
        return Err(error);
    }
    let config = crate::config::UserConfig::load(base);
    let hotkey = crate::config::HotkeyConfig::load(base);
    crate::hotkeys::changed(owner);
    let region = APP_STATE.with(|cell| {
        let mut guard = cell.try_borrow_mut().ok()?;
        let app = guard.as_mut()?;
        let mut config = config;
        config.window_width = app.user_cfg.window_width;
        config.window_height = app.user_cfg.window_height;
        let skin = config.resolve_active_skin_index(&app.skins);
        app.user_cfg = config;
        app.hotkey_cfg = hotkey;
        app.categories = data.build_runtime_tree(base, &app.builtin_icons.app_icons, &Default::default());
        app.data_store = data;
        app.active_category = 0;
        app.selected_shortcuts.clear();
        app.reload_advanced_assets();
        skin.and_then(|index| app.switch_skin(index))
    });
    if let Some(region) = region {
        apply_hrgn_safely(owner, region);
    }
    unsafe {
        crate::advanced_window::apply_opacity(owner);
        InvalidateRect(owner, ptr::null(), 0);
    }
    Ok(())
}

fn clear_icon_cache(base: &Path) -> io::Result<()> {
    let cache = base.join("user/icon-cache");
    if cache.exists() {
        let entries = fs::read_dir(cache)?;
        for entry in entries {
            let entry = entry?;
            if entry.file_type()?.is_file()
                && entry
                    .path()
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("png"))
            {
                fs::remove_file(entry.path())?;
            }
        }
    }
    Ok(())
}

pub fn backup_on_exit() {
    let base = APP_STATE.with(|cell| {
        let guard = cell.try_borrow().ok()?;
        let app = guard.as_ref()?;
        app.user_cfg
            .advanced
            .enabled(Key::AutoBackup)
            .then(|| app.base_dir.clone())
    });
    if let Some(base) = base {
        if let Err(error) = backup(&base, &base.join("user/backups/auto")) {
            eprintln!("自动备份失败：{error}");
        }
    }
}

pub struct DroppedFile {
    pub name: String,
    pub target: String,
    pub args: String,
    pub work_dir: String,
    pub icon: Option<crate::skin_assets::RgbaSurface>,
    pub icon_source: String,
}

#[cfg(test)]
mod drop_icon_tests {
    use super::*;

    #[test]
    fn shortcut_default_icon_comes_from_its_target_program() {
        let shortcut = Path::new(r"C:\Users\test\Desktop\软件.lnk");
        let program = r"D:\Apps\软件\app.exe";
        assert_eq!(default_drop_icon_source(shortcut, Some(program)), Path::new(program));
        assert_eq!(default_drop_icon_source(shortcut, None), shortcut);
    }

    #[test]
    fn relative_launch_path_keeps_the_original_icon_source() {
        let file = std::env::current_exe().unwrap();
        let base = file.parent().unwrap();
        let mut settings = AdvancedSettings::default();
        settings.set(Key::RelativePaths, "1".into());
        settings.set(Key::SkipIconCache, "1".into());
        settings.set(Key::SkipExeIcons, "1".into());
        let dropped = prepare_drop(base, &settings, &file);
        assert_eq!(dropped.icon_source, file.to_string_lossy());
        assert!(dropped.target.starts_with(".\\"));
        assert!(dropped.icon.is_none());
    }
}

fn default_drop_icon_source(file: &Path, link_target: Option<&str>) -> PathBuf {
    match link_target.filter(|target| !target.is_empty()) {
        Some(target) if Path::new(target).is_absolute() => PathBuf::from(target),
        Some(target) => file.parent().unwrap_or_else(|| Path::new(".")).join(target),
        None => file.to_path_buf(),
    }
}

pub fn relative_target(base: &Path, path: &Path) -> String {
    path.strip_prefix(base)
        .map(|relative| format!(".\\{}", relative.display()))
        .unwrap_or_else(|_| path.to_string_lossy().into_owned())
}

pub fn prepare_drop(base: &Path, settings: &AdvancedSettings, file: &Path) -> DroppedFile {
    let name = file.file_stem().or_else(|| file.file_name()).map_or_else(
        || "新快捷方式".into(),
        |name| name.to_string_lossy().into_owned(),
    );
    let extension = file
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default();
    let mut target = file.to_string_lossy().into_owned();
    let mut args = String::new();
    let mut work_dir = file
        .parent()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    // 默认图标始终取链接目标；是否把启动路径展开为目标由设置另行决定。
    let file_link = extension.eq_ignore_ascii_case("lnk")
        .then(|| crate::advanced_shell::file_link(file)).flatten();
    let icon_path = default_drop_icon_source(file, file_link.as_ref().map(|link| link.0.as_str()));
    if extension.eq_ignore_ascii_case("lnk") && settings.enabled(Key::ReadFileLink) {
        if let Some(link) = file_link {
            (target, args, work_dir) = link;
        }
    } else if extension.eq_ignore_ascii_case("url") && settings.enabled(Key::ReadUrl) {
        if let Some(url) = crate::advanced_shell::url_link(file) {
            target = url;
        }
    }
    let resolved_path = Path::new(&target);
    let icon = if settings.enabled(Key::WebIcons) && crate::advanced_web_icon::is_web_url(&target) {
        None
    } else {
        cached_file_icon(base, settings, &icon_path)
    };
    let icon_source = icon_path.to_string_lossy().into_owned();
    if settings.enabled(Key::RelativePaths) && resolved_path.is_absolute() {
        target = relative_target(base, resolved_path);
        if Path::new(&work_dir).is_absolute() {
            work_dir = relative_target(base, Path::new(&work_dir));
        }
    }
    DroppedFile {
        name,
        target,
        args,
        work_dir,
        icon,
        icon_source,
    }
}

fn cached_file_icon(
    base: &Path,
    settings: &AdvancedSettings,
    file: &Path,
) -> Option<crate::skin_assets::RgbaSurface> {
    if settings.enabled(Key::SkipExeIcons)
        && file
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"))
    {
        return None;
    }
    if settings.enabled(Key::SkipIconCache) {
        return crate::skin_assets::extract_file_icon_32(file);
    }
    let mut hasher = DefaultHasher::new();
    file.hash(&mut hasher);
    fs::metadata(file)
        .ok()
        .and_then(|meta| meta.modified().ok())
        .hash(&mut hasher);
    let directory = base.join("user/icon-cache");
    let cache = directory.join(format!("{:016x}-32.png", hasher.finish()));
    if let Some(image) = crate::skin_assets::RgbaSurface::load_file(&cache) {
        return Some(image);
    }
    let image = crate::skin_assets::extract_file_icon_32(file)?;
    if fs::create_dir_all(directory).is_ok() {
        let _ = image.save_png(&cache);
    }
    Some(image)
}
