//! 数据存储模型 (`user/data.json`) 与自绘菜单结构定义模块
//! 包含分类 (`categories`)、栏目 (`columns`)、快捷方式 (`shortcuts`) 三大关系数组。

use crate::skin_assets::RgbaSurface;
use crate::skin_parser::RgbColor;
use crate::targets::{PathMode, TargetKind};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// 分类数据记录（对应 `user/data.json` 中的 `categories` 数组元素）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryRecord {
    /// 分类唯一 ID（如 `"cat_1"`）
    pub id: String,
    /// 分类名称（如 `"我的程序"`、`"编程开发"`）
    pub name: String,
    /// 分类图标标识（如 `"cat_laptop"`、`"cat_cube"`）
    pub icon: String,
    /// 是否加密
    #[serde(default)]
    pub encrypted: bool,
    /// 排序权重
    #[serde(default)]
    pub sort_order: i32,
}

/// 栏目数据记录（对应 `user/data.json` 中的 `columns` 数组元素，含所属 `category_id`）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnRecord {
    /// 栏目唯一 ID（如 `"col_1"`）
    pub id: String,
    /// 所属分类 ID
    pub category_id: String,
    /// 栏目名称（如 `"系统功能"`、`"开发工具"`、`"AI"`）
    pub name: String,
    /// 是否加密
    #[serde(default)]
    pub encrypted: bool,
    /// 排序权重
    #[serde(default)]
    pub sort_order: i32,
}

/// 快捷方式数据记录（对应 `user/data.json` 中的 `shortcuts` 数组元素，含 `category_id` 与 `column_id`）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShortcutRecord {
    /// 快捷方式唯一 ID（如 `"sc_1"`，用于 `user/config.ini` 的 `[Hotkeys]`）
    pub id: String,
    /// 所属分类 ID
    pub category_id: String,
    /// 所属栏目 ID
    pub column_id: String,
    /// 快捷方式标题名称
    pub name: String,
    /// 目标可执行文件/目录/网址路径
    pub target_path: String,
    #[serde(default)]
    pub target_kind: Option<TargetKind>,
    #[serde(default)]
    pub path_mode: Option<PathMode>,
    #[serde(default)]
    pub work_dir_mode: Option<PathMode>,
    /// 图标键名或本地图标路径
    pub icon_key: String,
    /// 启动命令行参数
    #[serde(default)]
    pub args: String,
    /// 起始工作目录
    #[serde(default)]
    pub work_dir: String,
    /// 备注说明
    #[serde(default)]
    pub remark: String,
    /// 显示文字颜色：空字符串跟随默认色，显式十六进制颜色保留旧版行为。
    #[serde(default = "default_text_color_hex")]
    pub text_color: String,
    /// 运行后是否最小化快速启动
    #[serde(default)]
    pub minimize_on_run: bool,
    /// 排序权重
    #[serde(default)]
    pub sort_order: i32,
}

fn default_text_color_hex() -> String {
    "#000000".to_string()
}

/// 解析十六进制颜色字符串（如 `"#FF0000"`）为 `RgbColor`
pub fn parse_hex_color(hex: &str) -> RgbColor {
    let s = hex.trim().trim_start_matches('#');
    if s.len() == 6 {
        let r = u8::from_str_radix(&s[0..2], 16).unwrap_or(0);
        let g = u8::from_str_radix(&s[2..4], 16).unwrap_or(0);
        let b = u8::from_str_radix(&s[4..6], 16).unwrap_or(0);
        return RgbColor::new(r, g, b);
    }
    RgbColor::new(0, 0, 0)
}

/// 将 `RgbColor` 格式化为 `"#RRGGBB"`
pub fn format_hex_color(c: RgbColor) -> String {
    format!("#{:02X}{:02X}{:02X}", c.r, c.g, c.b)
}

/// `user/data.json` 顶层持久化结构（严格包含三个数组：`categories`、`columns`、`shortcuts`）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppDataStore {
    /// 分类数组
    pub categories: Vec<CategoryRecord>,
    /// 栏目数组（含 `category_id`）
    pub columns: Vec<ColumnRecord>,
    /// 快捷方式数组（含 `column_id` 和 `category_id`）
    pub shortcuts: Vec<ShortcutRecord>,
}

#[cfg(test)]
mod icon_cache_tests {
    use super::*;
    #[test]
    fn empty_text_color_inherits_default_and_explicit_black_is_preserved() {
        let base=std::env::temp_dir().join(format!("kstart-default-color-audit-{}",std::process::id()));
        let mut store=AppDataStore::default_store();store.shortcuts[0].text_color.clear();
        store.shortcuts[1].text_color="#000000".into();
        let tree=store.build_runtime_tree(&base,&Default::default(),&Default::default());
        let all:Vec<_>=tree.iter().flat_map(|cat|&cat.groups).flat_map(|group|&group.items).collect();
        assert!(all.iter().find(|item|item.id==store.shortcuts[0].id).unwrap().use_default_color);
        assert!(!all.iter().find(|item|item.id==store.shortcuts[1].id).unwrap().use_default_color);
        store.sync_from_runtime_tree(&tree,&base);
        let saved=AppDataStore::load(&base);
        assert!(saved.shortcuts.iter().find(|item|item.id=="sc_1").unwrap().text_color.is_empty());
        assert_eq!(saved.shortcuts.iter().find(|item|item.id=="sc_2").unwrap().text_color,"#000000");
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn source_path_and_internal_icon_cache_remain_independent() {
        let mut store = AppDataStore::default_store();
        let item = &mut store.shortcuts[0];
        let id = item.id.clone();
        item.icon_key = r"Z:\missing-source\application.exe".into();
        let mut icons = HashMap::new();
        let mut icon = RgbaSurface::new_empty(32, 32);
        icon.pixels_mut()[0] = [10, 20, 30, 255];
        icons.insert(id.clone(), icon);
        let tree = store.build_runtime_tree(Path::new("."), &icons, &HashMap::new());
        let loaded = tree.iter().flat_map(|cat| &cat.groups).flat_map(|group| &group.items)
            .find(|item| item.id == id).unwrap();
        assert_eq!(loaded.icon_key, r"Z:\missing-source\application.exe");
        assert_eq!(loaded.custom_icon_16.as_ref().unwrap().pixels()[0], [10, 20, 30, 255]);
    }

}

impl AppDataStore {
    /// 从 `user/data.json` 加载数据；若文件不存在或解析失败则构建默认数据并写入磁盘
    pub fn load(base_dir: &Path) -> Self {
        let path = base_dir.join("user").join("data.json");
        if let Ok(bytes) = fs::read(&path) {
            if let Ok(store) = serde_json::from_slice::<AppDataStore>(&bytes) {
                if !store.categories.is_empty() {
                    return store;
                }
            }
        }
        let store = Self::default_store();
        store.save(base_dir);
        store
    }

    /// 将当前数据持久化保存到 `user/data.json`
    pub fn save(&self, base_dir: &Path) {
        let user_dir = base_dir.join("user");
        let _ = fs::create_dir_all(&user_dir);
        let path = user_dir.join("data.json");
        let mut data = self.clone();
        for item in &mut data.shortcuts {
            let kind = item
                .target_kind
                .unwrap_or_else(|| TargetKind::infer(base_dir, &item.target_path));
            item.target_kind = Some(kind);
            item.path_mode = kind.path_mode(&item.target_path);
            item.work_dir_mode = TargetKind::Folder.path_mode(&item.work_dir);
        }
        if let Ok(json_str) = serde_json::to_string_pretty(&data) {
            let _ = fs::write(path, json_str.as_bytes());
        }
    }

    /// 将三表关系数据构建为运行时层级视图 `Vec<CategoryTab>`
    pub fn build_runtime_tree(
        &self,
        base_dir: &Path,
        custom_icons: &HashMap<String, RgbaSurface>,
        prev_active_groups: &HashMap<String, usize>,
    ) -> Vec<CategoryTab> {
        let mut cats = self.categories.clone();
        cats.sort_by_key(|c| c.sort_order);

        let mut result = Vec::with_capacity(cats.len());
        for cat in cats {
            let mut cols: Vec<ColumnRecord> = self
                .columns
                .iter()
                .filter(|col| col.category_id == cat.id)
                .cloned()
                .collect();
            cols.sort_by_key(|c| c.sort_order);

            let mut groups = Vec::with_capacity(cols.len());
            for col in cols {
                let mut sc_list: Vec<ShortcutRecord> = self
                    .shortcuts
                    .iter()
                    .filter(|s| s.category_id == cat.id && s.column_id == col.id)
                    .cloned()
                    .collect();
                sc_list.sort_by_key(|s| s.sort_order);

                let items = sc_list
                    .into_iter()
                    .map(|sc| {
                        let custom_icon_16 = custom_icons.get(&sc.icon_key)
                            .or_else(|| custom_icons.get(&sc.id)).cloned().or_else(|| {
                            // 兼容旧版把图标来源路径写入 icon_key 的记录。
                            let path = crate::targets::resolve_path(base_dir, &sc.icon_key);
                            path.is_file().then(|| crate::skin_assets::load_custom_icon_16(&path)).flatten()
                        });
                        ShortcutItem {
                            target_kind: sc
                                .target_kind
                                .unwrap_or_else(|| TargetKind::infer(base_dir, &sc.target_path)),
                            id: sc.id,
                            category_id: sc.category_id,
                            column_id: sc.column_id,
                            name: sc.name,
                            target_path: sc.target_path,
                            icon_key: sc.icon_key,
                            args: sc.args,
                            work_dir: sc.work_dir,
                            remark: sc.remark,
                            text_color: parse_hex_color(&sc.text_color),
                            use_default_color: sc.text_color.is_empty(),
                            minimize_on_run: sc.minimize_on_run,
                            custom_icon_16,
                        }
                    })
                    .collect();

                groups.push(ShortcutGroup {
                    id: col.id,
                    category_id: col.category_id,
                    title: col.name,
                    encrypted: col.encrypted,
                    items,
                });
            }

            let active_group_index = prev_active_groups
                .get(&cat.id)
                .copied()
                .unwrap_or(0)
                .min(groups.len().saturating_sub(1));

            result.push(CategoryTab {
                id: cat.id,
                name: cat.name,
                icon_name: cat.icon,
                encrypted: cat.encrypted,
                active_group_index,
                groups,
            });
        }
        result
    }

    /// 从运行时层级树 `Vec<CategoryTab>` 同步回三表结构并保存到 `user/data.json`
    pub fn sync_from_runtime_tree(&mut self, tree: &[CategoryTab], base_dir: &Path) {
        self.categories.clear();
        self.columns.clear();
        self.shortcuts.clear();

        for (c_idx, cat) in tree.iter().enumerate() {
            self.categories.push(CategoryRecord {
                id: cat.id.clone(),
                name: cat.name.clone(),
                icon: cat.icon_name.clone(),
                encrypted: cat.encrypted,
                sort_order: (c_idx + 1) as i32,
            });

            for (g_idx, grp) in cat.groups.iter().enumerate() {
                self.columns.push(ColumnRecord {
                    id: grp.id.clone(),
                    category_id: cat.id.clone(),
                    name: grp.title.clone(),
                    encrypted: grp.encrypted,
                    sort_order: (g_idx + 1) as i32,
                });

                for (s_idx, item) in grp.items.iter().enumerate() {
                    self.shortcuts.push(ShortcutRecord {
                        id: item.id.clone(),
                        category_id: cat.id.clone(),
                        column_id: grp.id.clone(),
                        name: item.name.clone(),
                        target_path: item.target_path.clone(),
                        target_kind: Some(item.target_kind),
                        path_mode: item.target_kind.path_mode(&item.target_path),
                        work_dir_mode: TargetKind::Folder.path_mode(&item.work_dir),
                        icon_key: item.icon_key.clone(),
                        args: item.args.clone(),
                        work_dir: item.work_dir.clone(),
                        remark: item.remark.clone(),
                        text_color: if item.use_default_color { String::new() } else {format_hex_color(item.text_color)},
                        minimize_on_run: item.minimize_on_run,
                        sort_order: (s_idx + 1) as i32,
                    });
                }
            }
        }

        self.save(base_dir);
    }

    /// 生成唯一的快捷方式 ID
    pub fn next_shortcut_id(&self) -> String {
        format!("sc_{}", self.next_shortcut_number())
    }

    pub fn next_shortcut_number(&self) -> u32 {
        let mut max_n = 0u32;
        for sc in &self.shortcuts {
            if let Some(num_str) = sc.id.strip_prefix("sc_") {
                if let Ok(n) = num_str.parse::<u32>() {
                    max_n = max_n.max(n);
                }
            }
        }
        max_n + 1
    }

    /// 默认初始数据（包含“我的程序”与“编程开发”两大分类、4 个栏目、23 个快捷方式）
    pub fn default_store() -> Self {
        Self {
            categories: vec![
                CategoryRecord {
                    id: "cat_1".to_string(),
                    name: "我的程序".to_string(),
                    icon: "cat_laptop".to_string(),
                    encrypted: false,
                    sort_order: 1,
                },
                CategoryRecord {
                    id: "cat_2".to_string(),
                    name: "编程开发".to_string(),
                    icon: "cat_cube".to_string(),
                    encrypted: false,
                    sort_order: 2,
                },
            ],
            columns: vec![
                ColumnRecord {
                    id: "col_1".to_string(),
                    category_id: "cat_1".to_string(),
                    name: "系统功能".to_string(),
                    encrypted: false,
                    sort_order: 1,
                },
                ColumnRecord {
                    id: "col_2".to_string(),
                    category_id: "cat_1".to_string(),
                    name: "办公软件".to_string(),
                    encrypted: false,
                    sort_order: 2,
                },
                ColumnRecord {
                    id: "col_3".to_string(),
                    category_id: "cat_2".to_string(),
                    name: "开发工具".to_string(),
                    encrypted: false,
                    sort_order: 1,
                },
                ColumnRecord {
                    id: "col_4".to_string(),
                    category_id: "cat_2".to_string(),
                    name: "AI".to_string(),
                    encrypted: false,
                    sort_order: 2,
                },
            ],
            shortcuts: vec![
                ShortcutRecord {
                    id: "sc_1".to_string(),
                    category_id: "cat_1".to_string(),
                    column_id: "col_1".to_string(),
                    name: "msedge".to_string(),
                    target_path: "msedge.exe".to_string(),
                    target_kind: None,
                    path_mode: None,
                    work_dir_mode: None,
                    icon_key: "app_msedge".to_string(),
                    args: String::new(),
                    work_dir: String::new(),
                    remark: String::new(),
                    text_color: "#000000".to_string(),
                    minimize_on_run: false,
                    sort_order: 1,
                },
                ShortcutRecord {
                    id: "sc_2".to_string(),
                    category_id: "cat_1".to_string(),
                    column_id: "col_1".to_string(),
                    name: "7zFM".to_string(),
                    target_path: "7zFM.exe".to_string(),
                    target_kind: None,
                    path_mode: None,
                    work_dir_mode: None,
                    icon_key: "app_7zfm".to_string(),
                    args: String::new(),
                    work_dir: String::new(),
                    remark: String::new(),
                    text_color: "#000000".to_string(),
                    minimize_on_run: false,
                    sort_order: 2,
                },
                ShortcutRecord {
                    id: "sc_3".to_string(),
                    category_id: "cat_1".to_string(),
                    column_id: "col_1".to_string(),
                    name: "郑煤机集团ERP系统".to_string(),
                    target_path: "explorer.exe".to_string(),
                    target_kind: None,
                    path_mode: None,
                    work_dir_mode: None,
                    icon_key: "app_erp".to_string(),
                    args: String::new(),
                    work_dir: String::new(),
                    remark: String::new(),
                    text_color: "#000000".to_string(),
                    minimize_on_run: false,
                    sort_order: 3,
                },
                ShortcutRecord {
                    id: "sc_4".to_string(),
                    category_id: "cat_1".to_string(),
                    column_id: "col_1".to_string(),
                    name: "Everything".to_string(),
                    target_path: "Everything.exe".to_string(),
                    target_kind: None,
                    path_mode: None,
                    work_dir_mode: None,
                    icon_key: "app_everything".to_string(),
                    args: String::new(),
                    work_dir: String::new(),
                    remark: String::new(),
                    text_color: "#000000".to_string(),
                    minimize_on_run: false,
                    sort_order: 4,
                },
                ShortcutRecord {
                    id: "sc_5".to_string(),
                    category_id: "cat_1".to_string(),
                    column_id: "col_1".to_string(),
                    name: "语雀".to_string(),
                    target_path: "yuque.exe".to_string(),
                    target_kind: None,
                    path_mode: None,
                    work_dir_mode: None,
                    icon_key: "app_yuque".to_string(),
                    args: String::new(),
                    work_dir: String::new(),
                    remark: String::new(),
                    text_color: "#000000".to_string(),
                    minimize_on_run: false,
                    sort_order: 5,
                },
                ShortcutRecord {
                    id: "sc_6".to_string(),
                    category_id: "cat_1".to_string(),
                    column_id: "col_1".to_string(),
                    name: "UniVPN".to_string(),
                    target_path: "UniVPN.exe".to_string(),
                    target_kind: None,
                    path_mode: None,
                    work_dir_mode: None,
                    icon_key: "app_univpn".to_string(),
                    args: String::new(),
                    work_dir: String::new(),
                    remark: String::new(),
                    text_color: "#000000".to_string(),
                    minimize_on_run: false,
                    sort_order: 6,
                },
                ShortcutRecord {
                    id: "sc_7".to_string(),
                    category_id: "cat_1".to_string(),
                    column_id: "col_1".to_string(),
                    name: "微信".to_string(),
                    target_path: "WeChat.exe".to_string(),
                    target_kind: None,
                    path_mode: None,
                    work_dir_mode: None,
                    icon_key: "app_wechat".to_string(),
                    args: String::new(),
                    work_dir: String::new(),
                    remark: String::new(),
                    text_color: "#000000".to_string(),
                    minimize_on_run: false,
                    sort_order: 7,
                },
                ShortcutRecord {
                    id: "sc_8".to_string(),
                    category_id: "cat_1".to_string(),
                    column_id: "col_1".to_string(),
                    name: "PreventTurnOff_x6..".to_string(),
                    target_path: "D:\\soft\\PreventTurnOff\\PreventTurnOff.exe".to_string(),
                    target_kind: None,
                    path_mode: None,
                    work_dir_mode: None,
                    icon_key: "app_prevent".to_string(),
                    args: String::new(),
                    work_dir: "D:\\soft\\PreventTurnOff".to_string(),
                    remark: String::new(),
                    text_color: "#000000".to_string(),
                    minimize_on_run: false,
                    sort_order: 8,
                },
                ShortcutRecord {
                    id: "sc_9".to_string(),
                    category_id: "cat_1".to_string(),
                    column_id: "col_1".to_string(),
                    name: "clash-verge".to_string(),
                    target_path: "clash-verge.exe".to_string(),
                    target_kind: None,
                    path_mode: None,
                    work_dir_mode: None,
                    icon_key: "app_clash".to_string(),
                    args: String::new(),
                    work_dir: String::new(),
                    remark: String::new(),
                    text_color: "#000000".to_string(),
                    minimize_on_run: false,
                    sort_order: 9,
                },
            ],
        }
    }
}

/// 运行时单个快捷方式项
#[derive(Debug, Clone)]
pub struct ShortcutItem {
    pub id: String,
    pub category_id: String,
    pub column_id: String,
    pub name: String,
    pub target_path: String,
    pub target_kind: TargetKind,
    pub icon_key: String,
    pub args: String,
    pub work_dir: String,
    pub remark: String,
    pub text_color: RgbColor,
    pub use_default_color: bool,
    pub minimize_on_run: bool,
    pub custom_icon_16: Option<RgbaSurface>,
}

/// 运行时抽屉式栏目（对应 `ColumnRecord`）
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct ShortcutGroup {
    pub id: String,
    pub category_id: String,
    pub title: String,
    pub encrypted: bool,
    pub items: Vec<ShortcutItem>,
}

/// 运行时顶层分类 Tab（对应 `CategoryRecord`）
#[derive(Debug, Clone)]
pub struct CategoryTab {
    pub id: String,
    pub name: String,
    pub icon_name: String,
    pub encrypted: bool,
    pub active_group_index: usize,
    pub groups: Vec<ShortcutGroup>,
}

/// 左下角弹出主菜单动作枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuAction {
    ToggleAlwaysOnTop,
    TogglePinToDesktop,
    ToggleLockMove,
    ToggleEnableList,
    ToggleEnableHotkey,
    ToggleStartupLock,
    OpenAdvancedOptions,
    OpenHotkeyManager,
    OpenScheduleManager,
    OpenSkinManager,
    SupportAuthor,
    AboutApp,
    ExitApp,
}

/// 内容区空白处右键菜单动作枚举（16 项菜单）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextMenuAction {
    AddFile,
    AddDirectory,
    AddHyperlink,
    AddSystemFunc,
    AddAutoLogin,
    CustomAddOrEdit,
    SearchItem,
    SpecificView,
    AppearanceSetting,
    SortShortcuts,
    BatchExportImport,
    PasteItem,
    ClearColumn,
    RenameColumnTitle,
    DeleteColumn,
}

/// 快捷方式图标右键菜单动作枚举（严格对应截图 2 的 12 项菜单）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemMenuAction {
    Delete,
    Cut,
    Copy,
    SetHotkey,
    SetSchedule,
    SendToRecent,
    CreateDesktopShortcut,
    MoveToTop,
    MoveToBottom,
    EditDetails,
    BrowseLocation,
    Properties,
}

/// 弹出菜单左侧图标类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuIconKind {
    None,
    Checkbox(bool),
    Options,
    Hotkey,
    Schedule,
    Skin,
    Exit,
    CtxAddFile,
    CtxAddDir,
    CtxAddUrl,
    CtxAddSys,
    CtxAddLogin,
    CtxSearch,
    CtxAppearance,
    CtxPaste,
    CtxClearCol,
    CtxDelCol,
    ItemDelete,
    ItemCut,
    ItemCopy,
    ItemSchedule,
    ItemMoveTop,
    ItemMoveBottom,
    ItemEdit,
    ItemProps,
}

/// 通用自绘弹出菜单单项描述
#[derive(Debug, Clone)]
pub struct PopupMenuItem<A: Copy> {
    pub action: A,
    pub label: String,
    pub icon: MenuIconKind,
    pub custom_text_color: Option<RgbColor>,
    pub has_separator_below: bool,
}

/// 构建左下角自绘主菜单
pub fn build_popup_menu_items(
    always_on_top: bool,
    pin_to_desktop: bool,
    lock_move: bool,
    enable_list: bool,
    enable_hotkey: bool,
    startup_lock: bool,
) -> Vec<PopupMenuItem<MenuAction>> {
    vec![
        PopupMenuItem {
            action: MenuAction::ToggleAlwaysOnTop,
            label: "总在最上(&T)".to_string(),
            icon: MenuIconKind::Checkbox(always_on_top),
            custom_text_color: None,
            has_separator_below: false,
        },
        PopupMenuItem {
            action: MenuAction::TogglePinToDesktop,
            label: "钉在桌面(&D)".to_string(),
            icon: MenuIconKind::Checkbox(pin_to_desktop),
            custom_text_color: None,
            has_separator_below: false,
        },
        PopupMenuItem {
            action: MenuAction::ToggleLockMove,
            label: "禁止移动(&M)".to_string(),
            icon: MenuIconKind::Checkbox(lock_move),
            custom_text_color: None,
            has_separator_below: true,
        },
        PopupMenuItem {
            action: MenuAction::ToggleEnableList,
            label: "启用列表(&L)".to_string(),
            icon: MenuIconKind::Checkbox(enable_list),
            custom_text_color: None,
            has_separator_below: false,
        },
        PopupMenuItem {
            action: MenuAction::ToggleEnableHotkey,
            label: "启用热键(&H)".to_string(),
            icon: MenuIconKind::Checkbox(enable_hotkey),
            custom_text_color: None,
            has_separator_below: false,
        },
        PopupMenuItem {
            action: MenuAction::ToggleStartupLock,
            label: "启动锁定(&O)".to_string(),
            icon: MenuIconKind::Checkbox(startup_lock),
            custom_text_color: None,
            has_separator_below: true,
        },
        PopupMenuItem {
            action: MenuAction::OpenAdvancedOptions,
            label: "高级选项(&C)".to_string(),
            icon: MenuIconKind::Options,
            custom_text_color: None,
            has_separator_below: false,
        },
        PopupMenuItem {
            action: MenuAction::OpenHotkeyManager,
            label: "热键管理(&K)".to_string(),
            icon: MenuIconKind::Hotkey,
            custom_text_color: None,
            has_separator_below: false,
        },
        PopupMenuItem {
            action: MenuAction::OpenScheduleManager,
            label: "计划任务管理(&P)".to_string(),
            icon: MenuIconKind::Schedule,
            custom_text_color: None,
            has_separator_below: true,
        },
        PopupMenuItem {
            action: MenuAction::OpenSkinManager,
            label: "皮肤管理器(&S)...".to_string(),
            icon: MenuIconKind::Skin,
            custom_text_color: Some(RgbColor::new(0xA0, 0x00, 0xD0)),
            has_separator_below: true,
        },
        PopupMenuItem {
            action: MenuAction::SupportAuthor,
            label: "支持快速启动(&Z)".to_string(),
            icon: MenuIconKind::None,
            custom_text_color: Some(RgbColor::new(0xFF, 0x00, 0x00)),
            has_separator_below: false,
        },
        PopupMenuItem {
            action: MenuAction::AboutApp,
            label: "关于本软件(&A)".to_string(),
            icon: MenuIconKind::None,
            custom_text_color: None,
            has_separator_below: true,
        },
        PopupMenuItem {
            action: MenuAction::ExitApp,
            label: "退出本程序(&X)".to_string(),
            icon: MenuIconKind::Exit,
            custom_text_color: Some(RgbColor::new(0x00, 0x00, 0xFF)),
            has_separator_below: false,
        },
    ]
}

/// 构建内容区右键单击弹出的菜单（1:1 复刻截图 4）
pub fn build_context_menu_items() -> Vec<PopupMenuItem<ContextMenuAction>> {
    vec![
        PopupMenuItem {
            action: ContextMenuAction::AddFile,
            label: "添加文件(&1)".to_string(),
            icon: MenuIconKind::CtxAddFile,
            custom_text_color: None,
            has_separator_below: false,
        },
        PopupMenuItem {
            action: ContextMenuAction::AddDirectory,
            label: "添加目录(&2)".to_string(),
            icon: MenuIconKind::CtxAddDir,
            custom_text_color: None,
            has_separator_below: false,
        },
        PopupMenuItem {
            action: ContextMenuAction::AddHyperlink,
            label: "添加超链接(&3)".to_string(),
            icon: MenuIconKind::CtxAddUrl,
            custom_text_color: None,
            has_separator_below: false,
        },
        PopupMenuItem {
            action: ContextMenuAction::AddSystemFunc,
            label: "添加系统功能(&4)".to_string(),
            icon: MenuIconKind::CtxAddSys,
            custom_text_color: None,
            has_separator_below: false,
        },
        PopupMenuItem {
            action: ContextMenuAction::AddAutoLogin,
            label: "添加自动登录(&5)".to_string(),
            icon: MenuIconKind::CtxAddLogin,
            custom_text_color: None,
            has_separator_below: false,
        },
        PopupMenuItem {
            action: ContextMenuAction::CustomAddOrEdit,
            label: "自定义添加(&U)...".to_string(),
            icon: MenuIconKind::None,
            custom_text_color: Some(RgbColor::new(0x00, 0x00, 0xFF)),
            has_separator_below: true,
        },
        PopupMenuItem {
            action: ContextMenuAction::SearchItem,
            label: "搜索(&F)...".to_string(),
            icon: MenuIconKind::CtxSearch,
            custom_text_color: None,
            has_separator_below: true,
        },
        PopupMenuItem {
            action: ContextMenuAction::SpecificView,
            label: "特定视图...".to_string(),
            icon: MenuIconKind::None,
            custom_text_color: None,
            has_separator_below: false,
        },
        PopupMenuItem {
            action: ContextMenuAction::AppearanceSetting,
            label: "外观设定...".to_string(),
            icon: MenuIconKind::CtxAppearance,
            custom_text_color: None,
            has_separator_below: true,
        },
        PopupMenuItem {
            action: ContextMenuAction::SortShortcuts,
            label: "快捷方式排序(&S)...".to_string(),
            icon: MenuIconKind::None,
            custom_text_color: None,
            has_separator_below: false,
        },
        PopupMenuItem {
            action: ContextMenuAction::BatchExportImport,
            label: "批量/导出/导入...".to_string(),
            icon: MenuIconKind::None,
            custom_text_color: None,
            has_separator_below: true,
        },
        PopupMenuItem {
            action: ContextMenuAction::PasteItem,
            label: "粘贴(&P)".to_string(),
            icon: MenuIconKind::CtxPaste,
            custom_text_color: None,
            has_separator_below: false,
        },
        PopupMenuItem {
            action: ContextMenuAction::ClearColumn,
            label: "清空本栏目(&C)".to_string(),
            icon: MenuIconKind::CtxClearCol,
            custom_text_color: None,
            has_separator_below: true,
        },
        PopupMenuItem {
            action: ContextMenuAction::RenameColumnTitle,
            label: "修改栏目标题(&M)".to_string(),
            icon: MenuIconKind::None,
            custom_text_color: None,
            has_separator_below: false,
        },
        PopupMenuItem {
            action: ContextMenuAction::DeleteColumn,
            label: "删除本栏目(&D)".to_string(),
            icon: MenuIconKind::CtxDelCol,
            custom_text_color: Some(RgbColor::new(0xFF, 0x00, 0x00)),
            has_separator_below: false,
        },
    ]
}

/// 构建对着快捷方式图标右键点击弹出的 12 项菜单（1:1 复刻截图 2）
pub fn build_item_menu_items() -> Vec<PopupMenuItem<ItemMenuAction>> {
    let blue = Some(RgbColor::new(0x00, 0x00, 0xFF));
    vec![
        PopupMenuItem {
            action: ItemMenuAction::Delete,
            label: "删除(&D)".to_string(),
            icon: MenuIconKind::ItemDelete,
            custom_text_color: Some(RgbColor::new(0xFF, 0x00, 0x00)),
            has_separator_below: false,
        },
        PopupMenuItem {
            action: ItemMenuAction::Cut,
            label: "剪切(&T)".to_string(),
            icon: MenuIconKind::ItemCut,
            custom_text_color: None,
            has_separator_below: false,
        },
        PopupMenuItem {
            action: ItemMenuAction::Copy,
            label: "复制(&C)".to_string(),
            icon: MenuIconKind::ItemCopy,
            custom_text_color: None,
            has_separator_below: true,
        },
        PopupMenuItem {
            action: ItemMenuAction::SetHotkey,
            label: "设置热键(&K)...".to_string(),
            icon: MenuIconKind::None,
            custom_text_color: blue,
            has_separator_below: false,
        },
        PopupMenuItem {
            action: ItemMenuAction::SetSchedule,
            label: "设置计划任务(&P)...".to_string(),
            icon: MenuIconKind::ItemSchedule,
            custom_text_color: blue,
            has_separator_below: false,
        },
        PopupMenuItem {
            action: ItemMenuAction::SendToRecent,
            label: "发送到最近使用".to_string(),
            icon: MenuIconKind::None,
            custom_text_color: blue,
            has_separator_below: false,
        },
        PopupMenuItem {
            action: ItemMenuAction::CreateDesktopShortcut,
            label: "生成桌面快捷方式".to_string(),
            icon: MenuIconKind::None,
            custom_text_color: blue,
            has_separator_below: true,
        },
        PopupMenuItem {
            action: ItemMenuAction::MoveToTop,
            label: "上移到顶部(↑)".to_string(),
            icon: MenuIconKind::ItemMoveTop,
            custom_text_color: None,
            has_separator_below: false,
        },
        PopupMenuItem {
            action: ItemMenuAction::MoveToBottom,
            label: "下移到底部(↓)".to_string(),
            icon: MenuIconKind::ItemMoveBottom,
            custom_text_color: None,
            has_separator_below: true,
        },
        PopupMenuItem {
            action: ItemMenuAction::EditDetails,
            label: "修改资料(&M)...".to_string(),
            icon: MenuIconKind::ItemEdit,
            custom_text_color: None,
            has_separator_below: false,
        },
        PopupMenuItem {
            action: ItemMenuAction::BrowseLocation,
            label: "浏览文件/目录(&B)...".to_string(),
            icon: MenuIconKind::None,
            custom_text_color: None,
            has_separator_below: true,
        },
        PopupMenuItem {
            action: ItemMenuAction::Properties,
            label: "属性(&R)".to_string(),
            icon: MenuIconKind::ItemProps,
            custom_text_color: None,
            has_separator_below: false,
        },
    ]
}
