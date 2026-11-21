//! 用户配置、热键配置与文本解码模块
//! 用户配置统一写入 `user/config.ini`，并兼容旧版分文件读取。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::{fs, io};

/// 将字节流解码为 UTF-8 字符串（优先尝试 UTF-8，失败则调用 Windows CP936 GBK 解码）
pub fn decode_text_bytes(bytes: &[u8]) -> String {
    let bytes = if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        &bytes[3..]
    } else {
        bytes
    };

    if let Ok(s) = std::str::from_utf8(bytes) {
        return s.to_string();
    }

    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::Globalization::MultiByteToWideChar;
        const CP_GBK: u32 = 936;
        let len = MultiByteToWideChar(
            CP_GBK,
            0,
            bytes.as_ptr(),
            bytes.len() as i32,
            std::ptr::null_mut(),
            0,
        );
        if len > 0 {
            let mut wide = vec![0u16; len as usize];
            MultiByteToWideChar(
                CP_GBK,
                0,
                bytes.as_ptr(),
                bytes.len() as i32,
                wide.as_mut_ptr(),
                len,
            );
            return String::from_utf16_lossy(&wide);
        }
    }

    String::from_utf8_lossy(bytes).into_owned()
}

/// 解析通用 INI 文本（兼容 VB6 单引号 `'`、分号 `;`、井号 `#` 注释），返回有序键值对列表
pub fn parse_ini_pairs(content: &str) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line.is_empty()
            || line.starts_with('\'')
            || line.starts_with(';')
            || line.starts_with('#')
            || (line.starts_with('[') && line.ends_with(']'))
        {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            pairs.push((k.trim().to_string(), v.trim().to_string()));
        }
    }
    pairs
}

/// 解析通用 INI 文本为 HashMap
pub fn parse_ini_map(content: &str) -> HashMap<String, String> {
    parse_ini_pairs(content).into_iter().collect()
}

pub(crate) type IniSections = HashMap<String, Vec<(String, String)>>;

pub(crate) fn parse_ini_sections(content: &str) -> IniSections {
    let mut sections = HashMap::new();
    let mut section = "config".to_string();
    for line in content.lines().map(str::trim) {
        if let Some(name) = line
            .strip_prefix('[')
            .and_then(|line| line.strip_suffix(']'))
        {
            section = name.trim().to_ascii_lowercase();
            sections.entry(section.clone()).or_insert_with(Vec::new);
        } else if !line.starts_with([';', '#', '\'']) {
            if let Some((key, value)) = line.split_once('=') {
                sections
                    .entry(section.clone())
                    .or_insert_with(Vec::new)
                    .push((key.trim().to_string(), value.trim().to_string()));
            }
        }
    }
    sections
}

pub(crate) fn read_ini_sections(user_dir: &Path) -> IniSections {
    fs::read(user_dir.join("config.ini"))
        .map(|bytes| parse_ini_sections(&decode_text_bytes(&bytes)))
        .unwrap_or_default()
}

pub(crate) fn write_ini_value(output: &mut String, key: &str, value: &str) {
    if key.is_empty()
        || key.starts_with([';', '#', '\'', '['])
        || key.contains(['=', '\r', '\n', '\0'])
    {
        return;
    }
    output.push_str(key);
    output.push('=');
    output.push_str(&value.replace(['\r', '\n', '\0'], " "));
    output.push_str("\r\n");
}

pub(crate) fn parse_switch(value: &str) -> Option<bool> {
    match value.to_ascii_lowercase().as_str() {
        "1" | "true" => Some(true),
        "0" | "false" => Some(false),
        _ => None,
    }
}

/// 验证值与热键由各自入口更新，其他配置保存时加载最新值。
#[derive(Default)]
pub(crate) struct Verification {
    pub login: String,
    pub lock: String,
}

impl Verification {
    pub fn load(user_dir: &Path) -> Self {
        let sections = read_ini_sections(user_dir);
        if let Some(entries) = sections.get("verification") {
            let mut result = Self::default();
            for (key, value) in entries {
                if key.eq_ignore_ascii_case("Login") {
                    result.login = value.clone();
                }
                if key.eq_ignore_ascii_case("Lock") {
                    result.lock = value.clone();
                }
            }
            result
        } else {
            let legacy = |name| {
                fs::read_to_string(user_dir.join(name))
                    .unwrap_or_default()
                    .trim()
                    .to_owned()
            };
            Self {
                login: legacy("login.credential"),
                lock: legacy("lock.credential"),
            }
        }
    }
}

/// 皮肤元数据信息（对应 `/skin` 下的一个子文件夹）
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct SkinEntry {
    /// 皮肤名称（即子文件夹名称）
    pub name: String,
    /// 皮肤目录完整路径
    pub dir_path: PathBuf,
    /// Setup.ini 路径
    pub setup_ini_path: PathBuf,
    /// 作者与日期说明信息（来自 Pre.txt）
    pub author_info: String,
    /// 缩略图路径（优先 `<皮肤名>.gif`，其次 `Pre.gif`）
    pub preview_gif_path: Option<PathBuf>,
}

/// 扫描 `/skin` 目录下的所有子文件夹，获取全部可用皮肤列表
pub fn scan_skins(base_dir: &Path) -> Vec<SkinEntry> {
    let skin_root = base_dir.join("skin");
    let mut skins = Vec::new();

    if let Ok(entries) = fs::read_dir(&skin_root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let setup_ini = path.join("Setup.ini");
                if setup_ini.exists() {
                    let name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default();

                    let pre_txt_path = path.join("Pre.txt");
                    let author_info = if let Ok(bytes) = fs::read(&pre_txt_path) {
                        decode_text_bytes(&bytes).trim().to_string()
                    } else {
                        "作者：未知".to_string()
                    };

                    let named_gif = path.join(format!("{}.gif", name));
                    let pre_gif = path.join("Pre.gif");
                    let preview_gif_path = if named_gif.exists() {
                        Some(named_gif)
                    } else if pre_gif.exists() {
                        Some(pre_gif)
                    } else {
                        None
                    };

                    skins.push(SkinEntry {
                        name,
                        dir_path: path,
                        setup_ini_path: setup_ini,
                        author_info,
                        preview_gif_path,
                    });
                }
            }
        }
    }

    skins.sort_by(|a, b| a.name.cmp(&b.name));
    skins
}

/// 用户全局配置结构体（对应 `/user/config.ini`）
#[derive(Debug, Clone)]
pub struct UserConfig {
    /// 配置文件中期望的皮肤名称
    pub configured_skin: Option<String>,
    /// 总在最上
    pub always_on_top: bool,
    /// 钉在桌面
    pub pin_to_desktop: bool,
    /// 禁止移动
    pub lock_move: bool,
    /// 启用列表
    pub enable_list: bool,
    /// 启用热键
    pub enable_hotkey: bool,
    /// 启动锁定
    pub startup_lock: bool,
    /// 搜索框按回车时使用的模式
    pub search_mode: crate::search::SearchMode,
    pub advanced: crate::advanced::AdvancedSettings,
    /// 窗口宽度
    pub window_width: i32,
    /// 窗口高度
    pub window_height: i32,
}

impl Default for UserConfig {
    fn default() -> Self {
        Self {
            configured_skin: Some("Windows Live Messenger(横排版)".to_string()),
            always_on_top: true,
            pin_to_desktop: false,
            lock_move: false,
            enable_list: true,
            enable_hotkey: true,
            startup_lock: false,
            search_mode: crate::search::SearchMode::default(),
            advanced: crate::advanced::AdvancedSettings::default(),
            window_width: 390,
            window_height: 845,
        }
    }
}

impl UserConfig {
    /// 从 `/user/config.ini` 加载用户配置
    pub fn load(base_dir: &Path) -> Self {
        Self::load_from(&base_dir.join("user"))
    }

    pub(crate) fn load_from(user_dir: &Path) -> Self {
        let sections = read_ini_sections(user_dir);
        let mut cfg = Self::default();
        cfg.advanced = crate::advanced::AdvancedSettings::load_from(user_dir, &sections);

        if let Some(pairs) = sections.get("config") {
            let map: HashMap<_, _> = pairs.iter().cloned().collect();

            if let Some(skin) = map.get("Skin") {
                if !skin.is_empty() {
                    cfg.configured_skin = Some(skin.clone());
                } else {
                    cfg.configured_skin = None;
                }
            }

            if let Some(v) = map.get("AlwaysOnTop") {
                cfg.always_on_top = parse_switch(v).unwrap_or(cfg.always_on_top);
            }
            if let Some(v) = map.get("PinToDesktop") {
                cfg.pin_to_desktop = parse_switch(v).unwrap_or(cfg.pin_to_desktop);
            }
            if let Some(v) = map.get("LockMove") {
                cfg.lock_move = parse_switch(v).unwrap_or(cfg.lock_move);
            }
            if let Some(v) = map.get("EnableList") {
                cfg.enable_list = parse_switch(v).unwrap_or(cfg.enable_list);
            }
            if let Some(v) = map.get("EnableHotkey") {
                cfg.enable_hotkey = parse_switch(v).unwrap_or(cfg.enable_hotkey);
            }
            if let Some(v) = map.get("StartupLock") {
                cfg.startup_lock = parse_switch(v).unwrap_or(cfg.startup_lock);
            }
            if let Some(v) = map.get("SearchMode") {
                cfg.search_mode = crate::search::SearchMode::from_config(v);
            }
            if let Some(w) = map.get("WindowWidth").and_then(|s| s.parse::<i32>().ok()) {
                cfg.window_width = w.clamp(260, 1200);
            }
            if let Some(h) = map.get("WindowHeight").and_then(|s| s.parse::<i32>().ok()) {
                cfg.window_height = h.clamp(400, 1600);
            }
        }

        cfg
    }

    /// 保存用户配置到 `/user/config.ini`
    pub fn save(&self, base_dir: &Path) {
        if let Err(error) = self.try_save(base_dir) {
            eprintln!("保存配置失败：{error}");
        }
    }

    pub fn try_save(&self, base_dir: &Path) -> io::Result<()> {
        let user_dir = base_dir.join("user");
        self.save_to(
            &user_dir,
            &HotkeyConfig::load_from(&user_dir),
            &Verification::load(&user_dir),
        )
    }

    pub(crate) fn save_to(
        &self,
        user_dir: &Path,
        hotkeys: &HotkeyConfig,
        verification: &Verification,
    ) -> io::Result<()> {
        let skin_name = self.configured_skin.as_deref().unwrap_or("");
        let mut content = format!(
            "[Config]\r\n\
             ' 默认皮肤名称（若该皮肤目录不存在，则自动回退加载 skin 目录下的第一个皮肤）\r\n\
             Skin={}\r\n\
             AlwaysOnTop={}\r\n\
             PinToDesktop={}\r\n\
             LockMove={}\r\n\
             EnableList={}\r\n\
             EnableHotkey={}\r\n\
             StartupLock={}\r\n\
             SearchMode={}\r\n\
             WindowWidth={}\r\n\
             WindowHeight={}\r\n",
            skin_name.replace(['\r', '\n', '\0'], " "),
            if self.always_on_top { 1 } else { 0 },
            if self.pin_to_desktop { 1 } else { 0 },
            if self.lock_move { 1 } else { 0 },
            if self.enable_list { 1 } else { 0 },
            if self.enable_hotkey { 1 } else { 0 },
            if self.startup_lock { 1 } else { 0 },
            self.search_mode.config_value(),
            self.window_width,
            self.window_height,
        );

        self.advanced.write_ini(&mut content);
        content.push_str("\r\n[Hotkeys]\r\n");
        for (key, target) in &hotkeys.bindings {
            let value = serde_json::to_string(target).map_err(io::Error::other)?;
            write_ini_value(&mut content, key, &value);
        }
        content.push_str("\r\n[Verification]\r\n");
        write_ini_value(&mut content, "Login", &verification.login);
        write_ini_value(&mut content, "Lock", &verification.lock);

        static SERIAL: AtomicU64 = AtomicU64::new(0);
        fs::create_dir_all(user_dir)?;
        let temp = user_dir.join(format!(
            ".config-{}-{}.tmp",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        let result =
            fs::write(&temp, content).and_then(|_| fs::rename(&temp, user_dir.join("config.ini")));
        if result.is_err() {
            let _ = fs::remove_file(temp);
        }
        result
    }

    /// 根据 `/user/config.ini` 的 `Skin` 配置项和扫描到的皮肤列表，确定初始激活皮肤索引
    pub fn resolve_active_skin_index(&self, skins: &[SkinEntry]) -> Option<usize> {
        if skins.is_empty() {
            return None;
        }
        if let Some(target_name) = &self.configured_skin {
            if let Some(idx) = skins
                .iter()
                .position(|s| s.name.eq_ignore_ascii_case(target_name))
            {
                return Some(idx);
            }
        }
        Some(0)
    }
}

/// 热键映射（`user/config.ini` 的 `[Hotkeys]` 节）。
#[derive(Debug, Clone)]
pub struct HotkeyConfig {
    /// 有序保存 `(key_event, shortcut_id_or_action)`
    pub bindings: Vec<(String, crate::hotkeys::Binding)>,
}

impl Default for HotkeyConfig {
    fn default() -> Self {
        Self {
            bindings: vec![
                ("Ctrl+Alt+V", "/AH/"),
                ("Ctrl+`", "/Search/"),
                ("F1", "sc_8"),
                ("F2", "sc_12"),
            ]
            .into_iter()
            .map(|(key, target)| (key.into(), crate::hotkeys::Binding::legacy(target)))
            .collect(),
        }
    }
}

impl HotkeyConfig {
    pub fn load(base_dir: &Path) -> Self {
        Self::load_from(&base_dir.join("user"))
    }

    pub(crate) fn load_from(user_dir: &Path) -> Self {
        let from_pairs = |pairs: Vec<(String, String)>| Self {
            bindings: pairs
                .into_iter()
                .map(|(key, value)| {
                    let binding = serde_json::from_str(&value)
                        .unwrap_or_else(|_| crate::hotkeys::Binding::legacy(&value));
                    (key, binding)
                })
                .collect(),
        };
        if let Some(bindings) = read_ini_sections(user_dir).remove("hotkeys") {
            return from_pairs(bindings);
        }
        let path = user_dir.join("hotkey.ini");
        if let Ok(bytes) = fs::read(&path) {
            let text = decode_text_bytes(&bytes);
            if let Some(bindings) = parse_ini_sections(&text).remove("hotkeys") {
                return from_pairs(bindings);
            }
            let pairs = parse_ini_pairs(&text);
            if !pairs.is_empty() {
                return from_pairs(pairs);
            }
        }
        Self::default()
    }

    pub fn save(&self, base_dir: &Path) {
        if let Err(error) = self.try_save(base_dir) {
            eprintln!("保存热键失败：{error}");
        }
    }

    pub fn try_save(&self, base_dir: &Path) -> io::Result<()> {
        let user_dir = base_dir.join("user");
        UserConfig::load_from(&user_dir).save_to(&user_dir, self, &Verification::load(&user_dir))
    }

    /// 根据快捷方式 `id` 查找已绑定的键盘事件名称
    pub fn find_key_by_shortcut_id(&self, shortcut_id: &str) -> Option<&str> {
        self.bindings
            .iter()
            .find(|(_, v)| v.references(shortcut_id))
            .map(|(k, _)| k.as_str())
    }

    /// 为指定快捷方式 `id` 绑定或更新热键（若 `new_key` 为空则移除绑定）
    pub fn set_hotkey_for_shortcut(&mut self, shortcut_id: &str, new_key: &str) {
        let key_trimmed = new_key.trim();
        let existing = self
            .bindings
            .iter()
            .find(|(_, v)| v.references(shortcut_id))
            .map(|(_, binding)| binding.clone());
        self.bindings.retain(|(_, v)| !v.references(shortcut_id));
        if !key_trimmed.is_empty() {
            self.bindings
                .retain(|(k, _)| !k.eq_ignore_ascii_case(key_trimmed));
            self.bindings.push((
                key_trimmed.to_string(),
                existing.unwrap_or_else(|| crate::hotkeys::Binding::shortcut(shortcut_id)),
            ));
        }
    }
}
