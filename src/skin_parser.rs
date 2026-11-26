//! 皮肤配置文件 `Setup.ini` 语义化解析模块
//! 采用有语义、有规则的变量命名规范（消除无意义的数字 1/2/3 后缀），同时向后兼容原版旧字段名。

use crate::config::{decode_text_bytes, parse_ini_map};
use std::fs;
use std::path::Path;

/// RGB 颜色结构体
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RgbColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl RgbColor {
    /// 构造 RGB 颜色
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// 从 VB6 十进制 BGR (`COLORREF` `0x00BBGGRR`) 转换
    pub const fn from_vb6_bgr(val: u32) -> Self {
        let r = (val & 0xFF) as u8;
        let g = ((val >> 8) & 0xFF) as u8;
        let b = ((val >> 16) & 0xFF) as u8;
        Self { r, g, b }
    }

    /// 转换为 Win32 GDI `COLORREF` (`0x00BBGGRR`)
    pub const fn to_colorref(self) -> u32 {
        (self.r as u32) | ((self.g as u32) << 8) | ((self.b as u32) << 16)
    }

    /// 转换为 32 位 BGRA 像素值 (`0xFFRRGGBB` in little-endian u32)
    pub const fn to_bgra_u32(self) -> u32 {
        (self.b as u32) | ((self.g as u32) << 8) | ((self.r as u32) << 16) | 0xFF00_0000
    }
}

/// 动态锚点坐标表达式（支持绝对像素、右边缘 `R<N>`、底边缘 `B<N>`）
#[derive(Debug, Clone, Copy)]
pub enum CoordExpr {
    /// 绝对像素值（负数表示移出屏幕隐藏）
    Abs(i32),
    /// 相对于窗口右边缘偏移：`WindowWidth - offset`
    Right(i32),
    /// 相对于窗口底边缘偏移：`WindowHeight - offset`
    Bottom(i32),
}

impl CoordExpr {
    /// 解析坐标字符串（如 `"68"`, `"-162"`, `"R55"`, `"B154"`）
    pub fn parse(raw: &str) -> Option<Self> {
        let s = raw.trim();
        if let Some(rest) = s.strip_prefix('R').or_else(|| s.strip_prefix('r')) {
            rest.trim().parse::<i32>().ok().map(CoordExpr::Right)
        } else if let Some(rest) = s.strip_prefix('B').or_else(|| s.strip_prefix('b')) {
            rest.trim().parse::<i32>().ok().map(CoordExpr::Bottom)
        } else {
            s.parse::<i32>().ok().map(CoordExpr::Abs)
        }
    }

    /// 根据当前窗口宽高计算实际像素值
    pub fn resolve(self, win_w: i32, win_h: i32) -> i32 {
        match self {
            CoordExpr::Abs(v) => v,
            CoordExpr::Right(r) => win_w - r,
            CoordExpr::Bottom(b) => win_h - b,
        }
    }
}

/// 四角透明掩码扫描尺寸配置
#[derive(Debug, Clone, Copy)]
pub struct CornerMaskConfig {
    /// 左上角透明扫描宽度
    pub top_left_width: u32,
    /// 左上角透明扫描高度
    pub top_left_height: u32,
    /// 右上角透明扫描宽度
    pub top_right_width: u32,
    /// 右上角透明扫描高度
    pub top_right_height: u32,
    /// 左下角透明扫描宽度
    pub bottom_left_width: u32,
    /// 左下角透明扫描高度
    pub bottom_left_height: u32,
    /// 右下角透明扫描宽度
    pub bottom_right_width: u32,
    /// 右下角透明扫描高度
    pub bottom_right_height: u32,
}

/// 自绘弹出菜单颜色配置（消除原版 MenuColor1..5 无语义数字后缀）
#[derive(Debug, Clone, Copy)]
pub struct MenuThemeColors {
    /// 菜单主体背景色 (`MenuBgColor`)
    pub bg_color: RgbColor,
    /// 菜单外边框颜色 (`MenuBorderColor`)
    pub border_color: RgbColor,
    /// 菜单项悬停/勾选框边框颜色 (`MenuItemBorderColor`)
    pub item_border_color: RgbColor,
    /// 菜单组间水平分隔线颜色 (`MenuSeparatorColor`)
    pub separator_color: RgbColor,
    /// 菜单项鼠标悬停高亮填充色 (`MenuItemHoverBgColor`)
    pub item_hover_bg_color: RgbColor,
}

/// 语义化功能按钮标识（彻底替代原版无语义的 cmd1..cmd11）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolButtonKind {
    /// 在线状态/模式切换按钮 (原 cmd1 / button1)
    Status,
    /// 邮箱检测按钮 (原 cmd2 / button2)
    Mail,
    /// 类别管理按钮 (原 cmd3 / button3)
    Category,
    /// 新建栏目按钮 (原 cmd4 / button4，保留历史枚举名和配置键兼容皮肤)
    FolderLock,
    /// 排序/数据备份按钮 (原 cmd5 / button5)
    Backup,
    /// 计划任务管理按钮 (原 cmd6 / button6)
    Schedule,
    /// 左下角主菜单按钮 (原 cmd7 / button7)
    MainMenu,
    /// 视图模式切换按钮 (原 cmd8 / button8)
    ViewMode,
    /// 快捷换肤画刷按钮 (原 cmd9 / button9)
    ThemeBrush,
    /// 热键管理按钮 (原 cmd10 / button10)
    Hotkey,
    /// 锁定屏幕按钮 (原 cmd11 / button11)
    LockScreen,
}

impl ToolButtonKind {
    /// 获取全部 11 个语义化功能按钮枚举列表
    pub const ALL: [ToolButtonKind; 11] = [
        ToolButtonKind::Status,
        ToolButtonKind::Mail,
        ToolButtonKind::Category,
        ToolButtonKind::FolderLock,
        ToolButtonKind::Backup,
        ToolButtonKind::Schedule,
        ToolButtonKind::MainMenu,
        ToolButtonKind::ViewMode,
        ToolButtonKind::ThemeBrush,
        ToolButtonKind::Hotkey,
        ToolButtonKind::LockScreen,
    ];

    /// 获取该功能按钮在 `Setup.ini` 中的语义化键名前缀与旧版兼容编号
    pub const fn config_keys(self) -> (&'static str, &'static str, usize) {
        match self {
            Self::Status => ("BtnStatusLeft", "BtnStatusTop", 1),
            Self::Mail => ("BtnMailLeft", "BtnMailTop", 2),
            Self::Category => ("BtnCategoryLeft", "BtnCategoryTop", 3),
            Self::FolderLock => ("BtnFolderLockLeft", "BtnFolderLockTop", 4),
            Self::Backup => ("BtnBackupLeft", "BtnBackupTop", 5),
            Self::Schedule => ("BtnScheduleLeft", "BtnScheduleTop", 6),
            Self::MainMenu => ("BtnMainMenuLeft", "BtnMainMenuTop", 7),
            Self::ViewMode => ("BtnViewModeLeft", "BtnViewModeTop", 8),
            Self::ThemeBrush => ("BtnThemeBrushLeft", "BtnThemeBrushTop", 9),
            Self::Hotkey => ("BtnHotkeyLeft", "BtnHotkeyTop", 10),
            Self::LockScreen => ("BtnLockScreenLeft", "BtnLockScreenTop", 11),
        }
    }
}

/// 单个功能按钮的坐标配置
#[derive(Debug, Clone, Copy)]
pub struct ToolButtonPos {
    pub kind: ToolButtonKind,
    pub left: CoordExpr,
    pub top: CoordExpr,
}

/// 窗口控制按钮（最小化/换肤/关闭）矩形配置
#[derive(Debug, Clone, Copy)]
pub struct BoxLayout {
    pub left: CoordExpr,
    pub top: CoordExpr,
    pub width: CoordExpr,
    pub height: CoordExpr,
}

impl BoxLayout {
    /// 根据窗口宽高解析出 `(x, y, w, h)` 像素矩形
    pub fn resolve_rect(self, win_w: i32, win_h: i32) -> (i32, i32, i32, i32) {
        (
            self.left.resolve(win_w, win_h),
            self.top.resolve(win_w, win_h),
            self.width.resolve(win_w, win_h),
            self.height.resolve(win_w, win_h),
        )
    }
}

/// 类别标签排列方向
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabDirection {
    /// 纵向排列 (1)
    Vertical,
    /// 横向排列 (2)
    Horizontal,
}

/// 类别标题相对标签左上角的文字区域及颜色
#[derive(Debug, Clone, Copy)]
pub struct CategoryTabText {
    pub left: i32,
    pub top: i32,
    pub width: i32,
    pub height: i32,
    pub color: RgbColor,
}

/// 完整皮肤布局与配色配置结构体
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct SkinConfig {
    /// 主窗体最大宽度（Twips）
    pub window_max_width: i32,
    /// 主窗体最大高度（Twips）
    pub window_max_height: i32,
    /// 主窗体背景填充色 (`WindowBgColor`)
    pub window_bg_color: RgbColor,
    /// 窗口四角透明抠图色键 (`WindowMaskColor`)
    pub window_mask_color: RgbColor,
    /// 抽屉分组标题栏文字颜色 (`GroupBarTextColor`)
    pub group_bar_text_color: RgbColor,

    /// 四角透明裁剪范围 (`CornerMask*`)
    pub corner_mask: CornerMaskConfig,
    /// 弹出菜单颜色配置 (`Menu*Color`)
    pub menu_colors: MenuThemeColors,

    /// 11 个语义化功能按钮坐标 (`Btn*`)
    pub tool_buttons: Vec<ToolButtonPos>,
    /// 功能按钮常规文字颜色 (`BtnTextNormalColor`)
    pub btn_text_normal_color: RgbColor,
    /// 功能按钮悬停文字颜色 (`BtnTextHoverColor`)
    pub btn_text_hover_color: RgbColor,

    /// 顶部日期时间标签配置 (`TimeLabel*`)
    pub time_label_left: CoordExpr,
    pub time_label_top: CoordExpr,
    pub time_label_color: RgbColor,
    pub time_label_bold: bool,

    /// 右上角最小化、皮肤、关闭按钮 (`TitleBtn*`)
    pub title_btn_min: BoxLayout,
    pub title_btn_skin: BoxLayout,
    pub title_btn_close: BoxLayout,

    /// 中部快捷方式列表区域 (`ItemList*`)
    pub item_list_box: BoxLayout,
    pub item_list_bg_color: RgbColor,

    /// 左上角大图标预览框 (`PreviewIcon*`)
    pub preview_icon_box: BoxLayout,

    /// 左侧/顶部类别标签导航栏 (`CategoryTab*`)
    pub category_tab_start_left: CoordExpr,
    pub category_tab_start_top: CoordExpr,
    pub category_tab_width: i32,
    pub category_tab_height: i32,
    pub category_tab_step_offset: i32,
    pub category_tab_direction: TabDirection,
    pub category_tab_icon_offset_x: i32,
    pub category_tab_icon_offset_y: i32,
    pub category_tab_text: Option<CategoryTabText>,

    /// 底部搜索栏与搜索按钮 (`SearchBox*` / `SearchBtn*`)
    pub search_box_visible: bool,
    pub search_box_use_skin_background: bool,
    pub search_box_bg_color: RgbColor,
    pub search_box_text_color: RgbColor,
    pub search_box_layout: BoxLayout,
    pub search_btn_layout: BoxLayout,
}

impl SkinConfig {
    /// 从 `Setup.ini` 文件加载并解析皮肤配置（优先读取语义化新字段名，同时兼容旧字段名）
    pub fn load_from_ini(path: &Path) -> Result<Self, String> {
        let bytes = fs::read(path).map_err(|e| format!("无法读取 Setup.ini: {}", e))?;
        let content = decode_text_bytes(&bytes);
        let map = parse_ini_map(&content);

        // 辅助闭包：优先读取新语义化键名，若未找到则回退读取旧键名
        let get_str = |semantic_key: &str, legacy_key: &str| -> Option<&str> {
            map.get(semantic_key)
                .or_else(|| map.get(legacy_key))
                .map(|s| s.as_str())
        };

        let get_i32 = |semantic_key: &str, legacy_key: &str, default: i32| -> i32 {
            get_str(semantic_key, legacy_key)
                .and_then(|s| s.parse::<i32>().ok())
                .unwrap_or(default)
        };

        let get_u32 = |semantic_key: &str, legacy_key: &str, default: u32| -> u32 {
            get_str(semantic_key, legacy_key)
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(default)
        };

        let get_color = |semantic_key: &str, legacy_key: &str, default_bgr: u32| -> RgbColor {
            let v = get_u32(semantic_key, legacy_key, default_bgr);
            RgbColor::from_vb6_bgr(v)
        };

        let get_coord = |semantic_key: &str, legacy_key: &str, default: CoordExpr| -> CoordExpr {
            get_str(semantic_key, legacy_key)
                .and_then(CoordExpr::parse)
                .unwrap_or(default)
        };

        // 解析 11 个语义化功能按钮位置
        let mut tool_buttons = Vec::with_capacity(11);
        for kind in ToolButtonKind::ALL {
            let (sem_left, sem_top, idx) = kind.config_keys();
            let leg_left = format!("cmdLeft{}", idx);
            let leg_top = format!("cmdTop{}", idx);
            let left = get_coord(sem_left, &leg_left, CoordExpr::Abs(-200));
            let top = get_coord(sem_top, &leg_top, CoordExpr::Abs(0));
            tool_buttons.push(ToolButtonPos { kind, left, top });
        }

        let dir_val = get_i32("CategoryTabDirection", "ClassBDirection", 1);
        let category_tab_direction = if dir_val == 2 {
            TabDirection::Horizontal
        } else {
            TabDirection::Vertical
        };

        Ok(Self {
            window_max_width: get_i32("WindowMaxWidth", "MaxWidth", 3900),
            window_max_height: get_i32("WindowMaxHeight", "MaxHeight", 7860),
            window_bg_color: get_color("WindowBgColor", "MainColor", 16184300),
            window_mask_color: get_color("WindowMaskColor", "MarkColor", 65535),
            group_bar_text_color: get_color("GroupBarTextColor", "BarColor1", 6953230),

            corner_mask: CornerMaskConfig {
                top_left_width: get_u32("CornerMaskTopLeftWidth", "RngSizeX1", 10),
                top_left_height: get_u32("CornerMaskTopLeftHeight", "RngSizeY1", 10),
                top_right_width: get_u32("CornerMaskTopRightWidth", "RngSizeX2", 10),
                top_right_height: get_u32("CornerMaskTopRightHeight", "RngSizeY2", 10),
                bottom_left_width: get_u32("CornerMaskBottomLeftWidth", "RngSizeX3", 10),
                bottom_left_height: get_u32("CornerMaskBottomLeftHeight", "RngSizeY3", 10),
                bottom_right_width: get_u32("CornerMaskBottomRightWidth", "RngSizeX4", 10),
                bottom_right_height: get_u32("CornerMaskBottomRightHeight", "RngSizeY4", 10),
            },

            menu_colors: MenuThemeColors {
                bg_color: get_color("MenuBgColor", "MenuColor1", 16184300),
                border_color: get_color("MenuBorderColor", "MenuColor2", 12224613),
                item_border_color: get_color("MenuItemBorderColor", "MenuColor3", 12224613),
                separator_color: get_color("MenuSeparatorColor", "MenuColor4", 14735038),
                item_hover_bg_color: get_color("MenuItemHoverBgColor", "MenuColor5", 15919825),
            },

            tool_buttons,
            btn_text_normal_color: get_color("BtnTextNormalColor", "cmdFontColor1", 3355443),
            btn_text_hover_color: get_color("BtnTextHoverColor", "cmdFontColor2", 7358488),

            time_label_left: get_coord("TimeLabelLeft", "TimeCapLeft", CoordExpr::Abs(68)),
            time_label_top: get_coord("TimeLabelTop", "TimeCapTop", CoordExpr::Abs(44)),
            time_label_color: get_color("TimeLabelColor", "TimeCapColor", 3355443),
            time_label_bold: get_i32("TimeLabelBold", "TimeCapBold", 0) != 0,

            title_btn_min: BoxLayout {
                left: get_coord("TitleBtnMinLeft", "CmdMinLeft", CoordExpr::Right(55)),
                top: get_coord("TitleBtnMinTop", "CmdMinTop", CoordExpr::Abs(5)),
                width: get_coord("TitleBtnMinWidth", "CmdMinWidth", CoordExpr::Abs(16)),
                height: get_coord("TitleBtnMinHeight", "CmdMinHeight", CoordExpr::Abs(16)),
            },
            title_btn_skin: BoxLayout {
                left: get_coord("TitleBtnSkinLeft", "CmdSkinLeft", CoordExpr::Right(38)),
                top: get_coord("TitleBtnSkinTop", "CmdSkinTop", CoordExpr::Abs(5)),
                width: get_coord("TitleBtnSkinWidth", "CmdSkinWidth", CoordExpr::Abs(16)),
                height: get_coord("TitleBtnSkinHeight", "CmdSkinHeight", CoordExpr::Abs(16)),
            },
            title_btn_close: BoxLayout {
                left: get_coord("TitleBtnCloseLeft", "CmdCloseLeft", CoordExpr::Right(21)),
                top: get_coord("TitleBtnCloseTop", "CmdCloseTop", CoordExpr::Abs(5)),
                width: get_coord("TitleBtnCloseWidth", "CmdCloseWidth", CoordExpr::Abs(16)),
                height: get_coord("TitleBtnCloseHeight", "CmdCloseHeight", CoordExpr::Abs(16)),
            },

            item_list_box: BoxLayout {
                left: get_coord("ItemListLeft", "ListLeft", CoordExpr::Abs(40)),
                top: get_coord("ItemListTop", "ListTop", CoordExpr::Abs(90)),
                width: get_coord("ItemListWidth", "ListWidth", CoordExpr::Right(50)),
                height: get_coord("ItemListHeight", "ListHeight", CoordExpr::Bottom(154)),
            },
            item_list_bg_color: get_color("ItemListBgColor", "ListColor", 16777215),

            preview_icon_box: BoxLayout {
                left: get_coord("PreviewIconLeft", "RecentBLeft", CoordExpr::Abs(22)),
                top: get_coord("PreviewIconTop", "RecentBTop", CoordExpr::Abs(43)),
                width: get_coord("PreviewIconWidth", "RecentBWidth", CoordExpr::Abs(32)),
                height: get_coord("PreviewIconHeight", "RecentBHeight", CoordExpr::Abs(32)),
            },

            category_tab_start_left: get_coord(
                "CategoryTabStartLeft",
                "ClassBFirstLeft",
                CoordExpr::Abs(8),
            ),
            category_tab_start_top: get_coord(
                "CategoryTabStartTop",
                "ClassBFirstTop",
                CoordExpr::Abs(89),
            ),
            category_tab_width: get_i32("CategoryTabWidth", "ClassBWidth", 31),
            category_tab_height: get_i32("CategoryTabHeight", "ClassBHeight", 39),
            category_tab_step_offset: get_i32("CategoryTabStepOffset", "ClassBSub", 39),
            category_tab_direction,
            category_tab_icon_offset_x: get_i32("CategoryTabIconOffsetX", "ClassBIconLeft", 8),
            category_tab_icon_offset_y: get_i32("CategoryTabIconOffsetY", "ClassBIconTop", 10),
            category_tab_text: (get_i32("CategoryTabTextVisible", "ClassShowText", 0) != 0)
                .then(|| CategoryTabText {
                    left: get_i32("CategoryTabTextLeft", "ClassTextLeft", 24),
                    top: get_i32("CategoryTabTextTop", "ClassTextTop", 7),
                    width: get_i32("CategoryTabTextWidth", "ClassTextWidth", 70),
                    height: get_i32("CategoryTabTextHeight", "ClassTextHeight", 16),
                    color: get_color("CategoryTabTextColor", "ClassTextFontColor", 0),
                }),

            search_box_visible: get_i32("SearchBoxVisible", "ShowSearchBox", 1) != 0,
            search_box_use_skin_background: map
                .get("SearchBoxUseSkinBackground")
                .and_then(|value| value.parse::<i32>().ok())
                .is_some_and(|value| value != 0),
            search_box_bg_color: get_color("SearchBoxBgColor", "SearchBoxBKColor", 16777215),
            search_box_text_color: get_color("SearchBoxTextColor", "SearchBoxFontColor", 0),
            search_box_layout: BoxLayout {
                left: get_coord("SearchBoxLeft", "SearchBoxLeft", CoordExpr::Abs(44)),
                top: get_coord("SearchBoxTop", "SearchBoxTop", CoordExpr::Bottom(57)),
                width: get_coord("SearchBoxWidth", "SearchBoxWidth", CoordExpr::Right(85)),
                height: get_coord("SearchBoxHeight", "SearchBoxHeight", CoordExpr::Abs(12)),
            },
            search_btn_layout: BoxLayout {
                left: get_coord("SearchBtnLeft", "SearchButtonLeft", CoordExpr::Right(33)),
                top: get_coord("SearchBtnTop", "SearchButtonTop", CoordExpr::Bottom(59)),
                width: get_coord("SearchBtnWidth", "SearchButtonWidth", CoordExpr::Abs(16)),
                height: get_coord("SearchBtnHeight", "SearchButtonHeight", CoordExpr::Abs(16)),
            },
        })
    }
}
