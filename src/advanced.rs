//! 高级设置的选项、兼容默认值和存储。

use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, path::Path};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Key {
    AutoStart,
    StartWithoutList,
    AutoHide,
    StretchDocked,
    RankByUse,
    Sound,
    FocusOnHover,
    MiddleToggle,
    ToggleCtrl,
    ToggleAlt,
    ToggleShift,
    RightToggle,
    CenterPointer,
    BlockLockedExit,
    TrayVisibility,
    CloseMinimizes,
    TrayClickShowsList,
    ShowDelay,
    HideDelay,
    DoubleClick,
    ExplorerFolders,
    DropOnProgram,
    WheelChangesGroup,
    HoverSwitch,
    HoverDelay,
    HideOnLowerResolution,
    AfterLaunch,
    FolderProgram,
    Browser,
    CustomBrowser,
    ReadFileLink,
    ReadUrl,
    ConfirmDelete,
    RelativePaths,
    SkipExeIcons,
    SkipIconCache,
    WebIcons,
    CacheWebIcons,
    ConfirmPower,
    ConfirmDisconnect,
    LockWithDisplayOff,
    CdCloseDelay,
    HideVolumeOverlay,
    VolumeStep,
    RecycleNoConfirm,
    RecycleNoProgress,
    RecycleSilent,
    Skin,
    ListBackgroundColor,
    GroupColor,
    DefaultColor,
    LargeIcons,
    Horizontal,
    TitleRight,
    HideTitle,
    AnimateHide,
    AnimateMinimize,
    Tooltips,
    Scrollbar,
    HideSearchButton,
    VistaMask,
    ListOpacity,
    ListBackground,
    TimeFormat,
    TrayStart,
    TrayCategory,
    AutoBackup,
    Priority,
    MemoryLimit,
}

#[derive(Clone, Copy)]
pub enum FieldKind {
    Check(bool),
    Choice(&'static [&'static str], usize),
    Number(u32, u32, u32),
    Text(&'static str),
    Skin,
    Category,
}

#[derive(Clone, Copy)]
pub struct Field {
    pub key: Key,
    pub label: &'static str,
    pub kind: FieldKind,
}

macro_rules! check {
    ($key:ident, $label:literal) => {
        field!($key, $label, FieldKind::Check(false))
    };
    ($key:ident, $label:literal, $value:literal) => {
        field!($key, $label, FieldKind::Check($value))
    };
}
macro_rules! field {
    ($key:ident, $label:literal, $kind:expr) => {
        Field {
            key: Key::$key,
            label: $label,
            kind: $kind,
        }
    };
}

pub const PAGES: [&str; 6] = [
    "基本设置",
    "打开方式",
    "快捷方式",
    "外观设置",
    "TrayStart",
    "其他",
];
pub const FIELDS: [&[Field]; 6] = [
    &[
        check!(AutoStart, "开机启动本程序"),
        check!(AutoHide, "靠边时自动隐藏"),
        check!(StartWithoutList, "启动时不显示列表"),
        check!(StretchDocked, "左右靠边时自动拉长"),
        check!(RankByUse, "按使用频率调整次序"),
        check!(Sound, "允许声音效果"),
        check!(FocusOnHover, "移到主界面时自动获取焦点"),
        check!(MiddleToggle, "鼠标中键激活或隐藏主界面"),
        check!(ToggleCtrl, "要求 Ctrl"),
        check!(ToggleAlt, "要求 Alt"),
        check!(ToggleShift, "要求 Shift"),
        check!(RightToggle, "使用鼠标右键激活"),
        check!(CenterPointer, "激活时移到鼠标中央"),
        check!(BlockLockedExit, "锁定时禁止退出"),
        field!(
            TrayVisibility,
            "显示托盘图标",
            FieldKind::Choice(&["显示", "不显示", "仅最小化时"], 0)
        ),
        check!(CloseMinimizes, "点击关闭按钮仅最小化"),
        check!(TrayClickShowsList, "单击托盘图标激活列表"),
        field!(
            ShowDelay,
            "延迟弹出（毫秒）",
            FieldKind::Number(0, 10000, 0)
        ),
        field!(
            HideDelay,
            "延迟弹入（毫秒）",
            FieldKind::Number(0, 10000, 0)
        ),
    ],
    &[
        check!(DoubleClick, "双击打开链接"),
        check!(ExplorerFolders, "以资源管理器方式打开目录"),
        check!(DropOnProgram, "拖到程序图标时用该程序打开"),
        check!(WheelChangesGroup, "滚轮切换栏目，Ctrl+滚轮滚动列表"),
        check!(HoverSwitch, "鼠标悬停自动切换类别或栏目"),
        field!(
            HoverDelay,
            "悬停切换延时（毫秒）",
            FieldKind::Number(0, 10000, 500)
        ),
        check!(HideOnLowerResolution, "低于当前分辨率时隐藏并自动恢复"),
        field!(
            AfterLaunch,
            "运行快捷方式后",
            FieldKind::Choice(&["保持", "最小化", "自动隐藏"], 0)
        ),
        field!(
            FolderProgram,
            "第三方文件夹程序（留空用系统）",
            FieldKind::Text("")
        ),
        field!(
            Browser,
            "网址打开方式",
            FieldKind::Choice(&["系统默认浏览器", "Internet Explorer", "自定义浏览器"], 0)
        ),
        field!(CustomBrowser, "自定义浏览器路径", FieldKind::Text("")),
    ],
    &[
        check!(ReadFileLink, "读取文件快捷方式指向的原文件", true),
        check!(ReadUrl, "读取网址快捷方式的网址", true),
        check!(ConfirmDelete, "删除快捷方式前询问", true),
        check!(RelativePaths, "添加时默认使用相对路径"),
        check!(SkipExeIcons, "不读取 EXE 图标"),
        check!(SkipIconCache, "关闭图标缓存"),
        check!(WebIcons, "自动读取网页图标"),
        check!(CacheWebIcons, "缓存网页图标"),
        check!(ConfirmPower, "关机、重启、注销、休眠前询问", true),
        check!(ConfirmDisconnect, "断开网络前询问"),
        check!(LockWithDisplayOff, "关闭显示器或启动屏保时锁定计算机"),
        field!(
            CdCloseDelay,
            "自动关闭光驱延迟（秒）",
            FieldKind::Number(0, 300, 4)
        ),
        check!(HideVolumeOverlay, "控制音量时不显示屏幕进度条"),
        field!(
            VolumeStep,
            "音量变化幅度（%）",
            FieldKind::Number(1, 100, 5)
        ),
        check!(RecycleNoConfirm, "清空回收站时不提示"),
        check!(RecycleNoProgress, "清空回收站时不显示进度"),
        check!(RecycleSilent, "清空回收站时没有声效"),
    ],
    &[
        field!(Skin, "当前皮肤", FieldKind::Skin),
        field!(
            ListBackgroundColor,
            "列表背景色（留空用皮肤色）",
            FieldKind::Text("")
        ),
        field!(
            GroupColor,
            "栏目文字色（留空用皮肤色）",
            FieldKind::Text("")
        ),
        field!(
            DefaultColor,
            "默认文字色（留空用原色）",
            FieldKind::Text("")
        ),
        check!(LargeIcons, "大图标模式"),
        check!(Horizontal, "允许横排", true),
        check!(TitleRight, "标题居右"),
        check!(HideTitle, "隐藏标题"),
        check!(AnimateHide, "自动隐藏时使用动画"),
        check!(AnimateMinimize, "最小化时使用动画"),
        field!(
            Tooltips,
            "提示条",
            FieldKind::Choice(&["简单提示条", "标准提示条", "不显示"], 2)
        ),
        check!(Scrollbar, "滚动条模式"),
        check!(HideSearchButton, "隐藏搜索按钮"),
        check!(VistaMask, "Vista 风格蒙版"),
        field!(
            ListOpacity,
            "列表不透明度（%）",
            FieldKind::Number(10, 100, 100)
        ),
        field!(ListBackground, "列表背景图片", FieldKind::Text("")),
        field!(
            TimeFormat,
            "日期时间格式",
            FieldKind::Text("m月d日 week time")
        ),
    ],
    &[
        check!(TrayStart, "启用 TrayStart"),
        field!(TrayCategory, "使用以下类别", FieldKind::Category),
    ],
    &[
        field!(AutoBackup, "正常退出时自动备份", FieldKind::Check(false)),
        field!(
            Priority,
            "进程优先级",
            FieldKind::Choice(&["标准", "低", "较低", "较高", "高"], 0)
        ),
        field!(
            MemoryLimit,
            "工作集上限（MB，0 为系统默认）",
            FieldKind::Number(0, 1024, 0)
        ),
    ],
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AdvancedSettings {
    values: BTreeMap<Key, String>,
    pub fonts: [FontSetting; 4],
    pub spacing: [u32; 5],
    pub launch_counts: BTreeMap<String, u64>,
}

impl Default for AdvancedSettings {
    fn default() -> Self {
        Self {
            values: BTreeMap::new(),
            fonts: std::array::from_fn(|_| FontSetting::default()),
            spacing: [140, 80, 140, 95, 160],
            launch_counts: BTreeMap::new(),
        }
    }
}

pub fn sort_by_usage(categories: &mut [crate::model::CategoryTab], settings: &AdvancedSettings) {
    if !settings.enabled(Key::RankByUse) {
        return;
    }
    for group in categories
        .iter_mut()
        .flat_map(|category| &mut category.groups)
    {
        group.items.sort_by_cached_key(|item| {
            std::cmp::Reverse(settings.launch_counts.get(&item.id).copied().unwrap_or(0))
        });
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct FontSetting {
    pub face: String,
    pub points: i32,
}

impl Default for FontSetting {
    fn default() -> Self {
        Self {
            face: "宋体".into(),
            points: 9,
        }
    }
}

impl AdvancedSettings {
    pub(crate) fn load_from(user_dir: &Path, sections: &crate::config::IniSections) -> Self {
        let mut result = if sections.contains_key("advanced") {
            Self::default()
        } else {
            fs::read(user_dir.join("advanced-settings.json"))
                .ok()
                .and_then(|bytes| serde_json::from_slice(&bytes).ok())
                .unwrap_or_default()
        };
        if let Some(advanced) = sections.get("advanced") {
            for (key, value) in advanced {
                if let Ok(key) =
                    serde_json::from_value::<Key>(serde_json::Value::String(key.clone()))
                {
                    if key != Key::Skin {
                        result.set(key, value.clone());
                    }
                }
            }
        }
        if let Some(fonts) = sections.get("fonts") {
            for (index, font) in result.fonts.iter_mut().enumerate() {
                if let Some(face) =
                    ini_value(fonts, &format!("Face{}", index + 1)).filter(|name| !name.is_empty())
                {
                    font.face = face.to_owned();
                }
                if let Some(points) = ini_value(fonts, &format!("Points{}", index + 1))
                    .and_then(|s| s.parse::<i32>().ok())
                {
                    font.points = points.clamp(5, 48);
                }
            }
        }
        if let Some(spacing) = sections.get("spacing") {
            for (index, width) in result.spacing.iter_mut().enumerate() {
                if let Some(pixels) = ini_value(spacing, &format!("Width{}", index + 1))
                    .and_then(|s| s.parse::<u32>().ok())
                {
                    *width = pixels.clamp(50, 500);
                }
            }
        }
        if let Some(usage) = sections.get("usage") {
            result.launch_counts.extend(
                usage.iter().filter_map(|(key, value)| {
                    value.parse().ok().map(|count| (key.clone(), count))
                }),
            );
        }
        result
    }

    pub(crate) fn write_ini(&self, output: &mut String) {
        use crate::config::write_ini_value as write;
        output.push_str("\r\n[Advanced]\r\n");
        write(output, "check_updates", "0");
        write(output, "auto_install_updates", "0");
        for field in FIELDS.iter().flat_map(|page| page.iter()) {
            let value = match field.kind {
                FieldKind::Skin => continue,
                FieldKind::Check(_) => u8::from(self.enabled(field.key)).to_string(),
                FieldKind::Choice(..) | FieldKind::Number(..) => self.number(field.key).to_string(),
                FieldKind::Text(_) | FieldKind::Category => self.text(field.key).to_owned(),
            };
            if let Ok(serde_json::Value::String(key)) = serde_json::to_value(field.key) {
                write(output, &key, &value);
            }
        }
        output.push_str("\r\n[Fonts]\r\n");
        for (index, font) in self.fonts.iter().enumerate() {
            write(output, &format!("Face{}", index + 1), &font.face);
            write(
                output,
                &format!("Points{}", index + 1),
                &font.points.to_string(),
            );
        }
        output.push_str("\r\n[Spacing]\r\n");
        for (index, width) in self.spacing.iter().enumerate() {
            write(output, &format!("Width{}", index + 1), &width.to_string());
        }
        output.push_str("\r\n[Usage]\r\n");
        for (key, count) in &self.launch_counts {
            write(output, key, &count.to_string());
        }
    }

    fn field(key: Key) -> Field {
        *FIELDS
            .iter()
            .flat_map(|page| page.iter())
            .find(|field| field.key == key)
            .expect("高级设置键必须有字段定义")
    }

    pub fn text(&self, key: Key) -> &str {
        self.values
            .get(&key)
            .map(String::as_str)
            .unwrap_or_else(|| match Self::field(key).kind {
                FieldKind::Text(value) => value,
                _ => "",
            })
    }

    pub fn enabled(&self, key: Key) -> bool {
        self.values
            .get(&key)
            .and_then(|value| crate::config::parse_switch(value))
            .unwrap_or_else(|| matches!(Self::field(key).kind, FieldKind::Check(true)))
    }

    pub fn launch_on_click(&self, double_click: bool) -> bool {
        self.enabled(Key::DoubleClick) == double_click
    }

    pub fn number(&self, key: Key) -> u32 {
        let (min, max, default) = match Self::field(key).kind {
            FieldKind::Choice(labels, index) => (0, (labels.len() - 1) as u32, index as u32),
            FieldKind::Number(min, max, default) => (min, max, default),
            _ => return 0,
        };
        self.text(key)
            .parse::<u32>()
            .unwrap_or(default)
            .clamp(min, max)
    }

    pub fn set(&mut self, key: Key, value: String) {
        self.values.insert(key, value);
    }
}

fn ini_value<'a>(entries: &'a [(String, String)], key: &str) -> Option<&'a str> {
    entries
        .iter()
        .rev()
        .find(|(name, _)| name.eq_ignore_ascii_case(key))
        .map(|(_, value)| value.as_str())
}

pub fn color(text: &str) -> Option<crate::skin_parser::RgbColor> {
    let text = text.trim().trim_start_matches('#');
    if text.len() != 6 {
        return None;
    }
    let value = u32::from_str_radix(text, 16).ok()?;
    Some(crate::skin_parser::RgbColor::new(
        (value >> 16) as u8,
        (value >> 8) as u8,
        value as u8,
    ))
}

pub fn format_time(pattern: &str, now: chrono::DateTime<chrono::FixedOffset>, weekday: &str) -> String {
    use chrono::Datelike;
    let pattern = if pattern.is_empty() {
        "m月d日 week time"
    } else {
        pattern
    };
    if pattern.contains('%') {
        let items: Vec<_> = chrono::format::StrftimeItems::new(pattern).collect();
        if !items
            .iter()
            .any(|item| matches!(item, chrono::format::Item::Error))
        {
            return now.format_with_items(items.iter()).to_string();
        }
    }
    let mut out = String::new();
    let mut remaining = pattern;
    while let Some(ch) = remaining.chars().next() {
        if let Some(tail) = remaining.strip_prefix("week") {
            out.push_str(weekday);
            remaining = tail;
            continue;
        }
        if let Some(tail) = remaining.strip_prefix("time") {
            out.push_str(&now.format("%H:%M").to_string());
            remaining = tail;
            continue;
        }
        match ch {
            'y' => out.push_str(&now.year().to_string()),
            'm' => out.push_str(&now.month().to_string()),
            'd' => out.push_str(&now.day().to_string()),
            _ => out.push(ch),
        }
        remaining = &remaining[ch.len_utf8()..];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn missing_values_and_custom_date_tokens_are_compatible() {
        let empty: AdvancedSettings = serde_json::from_str("{}").unwrap();
        assert!(!empty.enabled(Key::AutoStart));
        assert!(empty.enabled(Key::Horizontal));
        assert_eq!(empty.number(Key::TrayVisibility), 0);
        let mut clicks = AdvancedSettings::default();
        clicks.set(Key::DoubleClick, "1".into());
        assert!(!clicks.launch_on_click(false));
        assert!(clicks.launch_on_click(true));
        clicks.set(Key::DoubleClick, "0".into());
        assert!(clicks.launch_on_click(false));
        assert!(!clicks.launch_on_click(true));
        assert_eq!(empty.number(Key::HoverDelay), 500);
        let now = chrono::FixedOffset::east_opt(8 * 3600).unwrap()
            .with_ymd_and_hms(2026, 11, 21, 9, 8, 0)
            .single()
            .unwrap();
        assert_eq!(
            format_time("m月d日 week time", now, "星期六"),
            "11月21日 星期六 09:08"
        );
        assert_eq!(format_time("%Y-%m-%d", now, "星期六"), "2026-11-21");
    }
}
