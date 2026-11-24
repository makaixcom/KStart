//! 界面多层级 Z-Order 渲染管线与命中测试模块
//! 完整实现主界面、自绘弹出菜单、
//! 矩形框选与多选高亮效果，以及六大独立居中管理窗口（类别管理、栏目排序、热键管理、修改快捷方式、皮肤管理器、关于快速启动）的画布渲染。

use crate::config::{SkinEntry, UserConfig};
use crate::advanced::{AdvancedSettings, Key as Setting};
use crate::model::{
    build_context_menu_items, build_item_menu_items, build_popup_menu_items, CategoryTab,
    MenuIconKind, PopupMenuItem,
};
use crate::search::{SearchDialogState, SearchMode};
use crate::skin_assets::{BuiltinIcons, Canvas, RgbaSurface, SkinAssets, WidgetState};
use crate::skin_parser::{RgbColor, SkinConfig, TabDirection, ToolButtonKind};
use std::collections::HashSet;

/// 文本对齐方式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextAlign {
    Left,
    Center,
}

/// GDI 文本绘制指令
#[derive(Debug, Clone)]
pub struct TextDrawCmd {
    pub text: String,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub color: RgbColor,
    pub bold: bool,
    pub align: TextAlign,
    pub mnemonic: bool,
    /// 支持多行自动换行的富文本区域（用于“关于快速启动”软件介绍与“修改快捷方式-备注”页签）
    pub multiline: bool,
    pub font_slot: Option<usize>,
}

/// 按绘制顺序记录覆盖区域及其文字起点，保留被局部遮挡文字的原始布局。
pub struct PopupTextClip {
    pub layers: Vec<((i32, i32, i32, i32), usize)>,
}

/// 当前打开的自绘弹出菜单类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivePopupMenu {
    None,
    /// 左下角 16 项主菜单
    MainMenu,
    /// 右上角皮肤按钮弹出的皮肤快捷菜单
    SkinQuickMenu,
    /// 搜索按钮旁的模式菜单
    SearchMode,
    TrayStart { category: usize, count: usize, x: i32, y: i32 },
    /// 内容区空白处的右键菜单
    ContextMenu { x: i32, y: i32 },
    /// 对着快捷方式图标右键单击弹出的 12 项菜单
    ItemMenu {
        x: i32,
        y: i32,
        target_shortcut: usize,
    },
}

/// 修改快捷方式弹窗中的激活输入框字段
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditShortcutField {
    Title,
    TargetPath,
    IconKey,
    Args,
    WorkDir,
    Hotkey,
    Remark,
}

/// 修改/新建快捷方式弹窗状态（支持【常规】与【备注】两个页签切换）
#[derive(Debug, Clone)]
pub struct EditShortcutDialogState {
    pub target_kind_hint: Option<crate::targets::TargetKind>,
    /// true 表示新建模式（点确定时才插入列表），false 表示编辑现有项
    pub is_new: bool,
    pub shortcut_id: String,
    pub cat_idx: usize,
    pub grp_idx: usize,
    pub item_idx: usize,
    /// 0: 常规, 1: 备注
    pub active_tab: usize,
    pub title: String,
    pub target_path: String,
    pub icon_key: String,
    /// 新建或自选图标时暂存的 16x16 图标表面（点确定后才正式保存）
    pub pending_icon_16: Option<RgbaSurface>,
    pub text_color: RgbColor,
    pub use_default_color: bool,
    pub args: String,
    pub work_dir: String,
    pub hotkey: String,
    pub remark: String,
    pub minimize_on_run: bool,
    pub active_field: EditShortcutField,
}

/// 当前打开的独立居中管理弹窗类型
#[derive(Debug, Clone)]
pub enum ActiveModal {
    None,
    /// 皮肤管理器窗口
    SkinManager,
    /// 类别管理窗口
    CategoryManager {
        selected_idx: usize,
        renaming: bool,
        rename_buf: String,
    },
    /// 栏目排序窗口（支持鼠标拖放交换栏目顺序）
    ColumnSort {
        selected_idx: usize,
        dragging_idx: Option<usize>,
    },
    /// 热键管理与设置热键窗口
    HotkeyManager(Option<String>),
    /// 修改快捷方式窗口（含常规与备注 Tab）
    EditShortcut(EditShortcutDialogState),
    /// 快捷方式搜索结果窗口
    Search(SearchDialogState),
    Advanced(crate::advanced::AdvancedSettings),
    /// 关于快速启动窗口
    About,
}

impl ActiveModal {
    /// 获取当前弹窗独立窗口的目标宽高 `(width, height)`
    pub fn window_size(&self) -> (i32, i32) {
        match self {
            ActiveModal::None => (0, 0),
            ActiveModal::SkinManager => (380, 440),
            ActiveModal::CategoryManager { .. } => (380, 405),
            ActiveModal::ColumnSort { .. } => (360, 355),
            ActiveModal::HotkeyManager(_) => (420, 440),
            ActiveModal::EditShortcut(_) => (371, 458),
            ActiveModal::Search(_) | ActiveModal::Advanced(_) => (0, 0),
            ActiveModal::About => (522, 429),
        }
    }
}

/// 鼠标命中测试区域枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HitTarget {
    None,
    DragCaption,
    TitleMin,
    TitleSkin,
    TitleClose,
    PreviewIcon,
    ToolButton(ToolButtonKind),
    CategoryTab(usize),
    GroupBar(usize),
    ShortcutItem(usize),
    /// 内容区列表空白处（点击/按下可启动矩形框选）
    ListBlank,
    ListScroll(i32),
    SearchBox,
    SearchBtn,

    /// 弹出菜单项
    PopupMenuItem(usize),
    SkinQuickMenuItem(usize),
    ContextMenuItem(usize),
    ItemMenuItem(usize),
    SearchModeItem(usize),
    TrayShortcut(usize),

    /// 弹出菜单垂直滚动条控件
    MenuScrollUp,
    MenuScrollDown,
    MenuScrollTrack(i32),

    /// 弹窗通用标题栏拖拽
    ModalTitleDrag,

    /// 皮肤管理器控件
    SkinMgrSelect(usize),
    SkinMgrApply,
    SkinMgrClose,

    /// 类别管理窗口控件
    CatMgrBtnNew,
    CatMgrBtnDelete,
    CatMgrBtnRename,
    CatMgrBtnEncrypt,
    CatMgrBtnMoveUp,
    CatMgrBtnMoveDown,
    CatMgrSelectRow(usize),
    CatMgrBtnChangeIcon,
    CatMgrClose,

    /// 栏目排序窗口控件
    ColSortRow(usize),
    ColSortClose,


    /// 修改快捷方式窗口控件
    EditScTab(usize),
    EditScField(EditShortcutField),
    EditScPickFile,
    EditScPickDir,
    EditScPickIcon,
    EditScPickColor,
    EditScToggleMinOnRun,
    EditScOk,
    EditScCancel,

    /// 关于快速启动窗口控件
    AboutLink(usize),
    AboutSupportLink,
    AboutClose,
}

/// 主窗口渲染上下文只读快照
#[allow(dead_code)]
pub struct RenderState<'a> {
    pub win_w: i32,
    pub win_h: i32,
    pub skin_cfg: &'a SkinConfig,
    pub skin_assets: &'a SkinAssets,
    pub builtin_icons: &'a BuiltinIcons,
    pub user_cfg: &'a UserConfig,
    pub categories: &'a [CategoryTab],
    pub active_category: usize,
    pub hover_target: HitTarget,
    pub down_target: HitTarget,
    pub time_string: &'a str,
    pub search_query: &'a str,
    pub search_focused: bool,
    pub list_background: Option<&'a RgbaSurface>,
    pub shortcut_scroll: usize,
    pub volume_level: Option<u8>,
    pub tooltip: Option<&'a (i32, i32, String)>,
    pub popup_menu: ActivePopupMenu,
    /// 当前弹出菜单的垂直滚动起始行索引
    pub menu_scroll: usize,
    pub skins: &'a [SkinEntry],
    pub active_skin_index: usize,
    pub selected_skin_index: usize,
    /// 内容区当前正在拖拽的快捷方式索引及鼠标位置
    pub dragging_shortcut: Option<(usize, i32, i32)>,
    /// 内容区当前正在框选的矩形 `(start_x, start_y, cur_x, cur_y)`
    pub marquee_rect: Option<(i32, i32, i32, i32)>,
    /// 当前在内容区选中的快捷方式索引集合
    pub selected_shortcuts: &'a HashSet<usize>,
}

/// 计算弹出菜单自适应高度与可见行数
fn clamp_menu_height(win_h: i32, total_rows: usize, top_margin: i32, bottom_margin: i32) -> (i32, usize) {
    let max_h = (win_h - top_margin - bottom_margin).max(74);
    let max_rows = ((max_h - 2) / 24).max(3) as usize;
    let vis_rows = total_rows.min(max_rows).max(1);
    let h = (vis_rows as i32) * 24 + 2;
    (h, vis_rows)
}

/// 左下角主菜单矩形 `(x, y, w, h)`
pub fn get_popup_menu_rect(_win_w: i32, win_h: i32) -> (i32, i32, i32, i32) {
    let count = build_popup_menu_items(false,false,false,true,true,false).len();
    let (h, vis_rows) = clamp_menu_height(win_h, count, 8, 40);
    let w = if vis_rows < count { 176 } else { 162 };
    let x = 10;
    let y = (win_h - 40 - h).max(8);
    (x, y, w, h)
}

/// 右上角皮肤快捷菜单矩形 `(x, y, w, h)`
pub const SKIN_MENU_SCROLLBAR_WIDTH: i32 = 12;

pub fn get_skin_quick_menu_rect(win_w: i32, win_h: i32, skins_count: usize) -> (i32, i32, i32, i32) {
    let total_rows = skins_count + 2;
    // 最多 15 个皮肤，底部两个功能项固定显示。
    let (h, vis_rows) = clamp_menu_height(win_h, skins_count.min(15) + 2, 22, 10);
    let w = if vis_rows < total_rows { 195 + SKIN_MENU_SCROLLBAR_WIDTH } else { 195 };
    let x = (win_w - w - 6).max(4);
    let y = 22;
    (x, y, w, h)
}

/// 内容区空白处右键菜单矩形 `(x, y, w, h)`
pub fn get_context_menu_rect(win_w: i32, win_h: i32, req_x: i32, req_y: i32) -> (i32, i32, i32, i32) {
    let count = build_context_menu_items().len();
    let (h, vis_rows) = clamp_menu_height(win_h, count, 24, 8);
    let w = if vis_rows < count { 176 } else { 162 };
    let x = req_x.clamp(8, (win_w - w - 8).max(8));
    let y = req_y.clamp(24, (win_h - h - 8).max(24));
    (x, y, w, h)
}

/// 快捷方式图标右键菜单矩形 `(x, y, w, h)`（12 项，总高 290px，宽 174px）
pub fn get_item_menu_rect(win_w: i32, win_h: i32, req_x: i32, req_y: i32) -> (i32, i32, i32, i32) {
    let (h, vis_rows) = clamp_menu_height(win_h, 12, 24, 8);
    let w = if vis_rows < 12 { 188 } else { 174 };
    let x = req_x.clamp(8, (win_w - w - 8).max(8));
    let y = req_y.clamp(24, (win_h - h - 8).max(24));
    (x, y, w, h)
}

/// 获取当前弹出菜单的项目总数。
pub fn popup_item_count(menu: ActivePopupMenu, skins_count: usize) -> usize {
    match menu {
        ActivePopupMenu::MainMenu => build_popup_menu_items(false,false,false,true,true,false).len(),
        ActivePopupMenu::ContextMenu { .. } => build_context_menu_items().len(),
        ActivePopupMenu::SkinQuickMenu => skins_count + 2,
        ActivePopupMenu::ItemMenu { .. } => 12,
        ActivePopupMenu::SearchMode => SearchMode::OPTIONS.len(),
        ActivePopupMenu::TrayStart { count, .. } => count,
        ActivePopupMenu::None => 0,
    }
}

pub fn popup_rect(
    popup_menu: ActivePopupMenu,
    win_w: i32,
    win_h: i32,
    skins_count: usize,
) -> Option<(i32, i32, i32, i32)> {
    match popup_menu {
        ActivePopupMenu::MainMenu => Some(get_popup_menu_rect(win_w, win_h)),
        ActivePopupMenu::SkinQuickMenu => Some(get_skin_quick_menu_rect(win_w, win_h, skins_count)),
        ActivePopupMenu::ContextMenu { x, y } => Some(get_context_menu_rect(win_w, win_h, x, y)),
        ActivePopupMenu::ItemMenu { x, y, .. } => Some(get_item_menu_rect(win_w, win_h, x, y)),
        ActivePopupMenu::TrayStart { count, .. } => {
            let (height, rows) = clamp_menu_height(win_h, count, 8, 8);
            Some((0, 0, if rows < count { 225 } else { 210 }, height))
        }
        ActivePopupMenu::SearchMode | ActivePopupMenu::None => None,
    }
}

pub fn popup_visible_rows(height: i32) -> usize {
    ((height - 2) / 24).max(1) as usize
}

/// 在独立菜单窗口的客户坐标中检测菜单项或滚动条。
pub fn hit_test_popup(
    menu: ActivePopupMenu,
    (x, y): (i32, i32),
    (width, height): (i32, i32),
    scroll: usize,
    skins_count: usize,
) -> HitTarget {
    if x < 1 || y < 1 || x >= width - 1 || y >= height - 1 {
        return HitTarget::None;
    }
    if menu == ActivePopupMenu::SkinQuickMenu {
        let skin_rows = popup_visible_rows(height).saturating_sub(2);
        let footer_y = 1 + skin_rows as i32 * 24;
        if y >= footer_y {
            return HitTarget::SkinQuickMenuItem(skins_count + ((y - footer_y) / 24) as usize);
        }
        if skin_rows < skins_count && x >= width - SKIN_MENU_SCROLLBAR_WIDTH {
            return if y < 15 { HitTarget::MenuScrollUp }
                else if y >= footer_y - 14 { HitTarget::MenuScrollDown }
                else { HitTarget::MenuScrollTrack(y - 15) };
        }
        let index = scroll.min(skins_count.saturating_sub(skin_rows)) + ((y - 1) / 24) as usize;
        return if index < skins_count { HitTarget::SkinQuickMenuItem(index) } else { HitTarget::None };
    }
    let rows = popup_visible_rows(height);
    let count = popup_item_count(menu, skins_count);
    if rows < count && x >= width - 15 {
        return if y < 15 {
            HitTarget::MenuScrollUp
        } else if y >= height - 15 {
            HitTarget::MenuScrollDown
        } else {
            HitTarget::MenuScrollTrack(y - 15)
        };
    }
    let index = scroll.min(count.saturating_sub(rows)) + ((y - 1) / 24) as usize;
    match menu {
        ActivePopupMenu::MainMenu => HitTarget::PopupMenuItem(index),
        ActivePopupMenu::SkinQuickMenu => HitTarget::SkinQuickMenuItem(index),
        ActivePopupMenu::ContextMenu { .. } => HitTarget::ContextMenuItem(index),
        ActivePopupMenu::ItemMenu { .. } => HitTarget::ItemMenuItem(index),
        ActivePopupMenu::SearchMode => HitTarget::SearchModeItem(index),
        ActivePopupMenu::TrayStart { .. } => HitTarget::TrayShortcut(index),
        ActivePopupMenu::None => HitTarget::None,
    }
}

/// 计算第 `i` 个分类 Tab 的矩形
pub fn get_category_tab_rect(
    cfg: &SkinConfig,
    win_w: i32,
    win_h: i32,
    index: usize,
) -> (i32, i32, i32, i32) {
    let start_x = cfg.category_tab_start_left.resolve(win_w, win_h);
    let start_y = cfg.category_tab_start_top.resolve(win_w, win_h);
    let step = cfg.category_tab_step_offset * (index as i32);
    let (x, y) = match cfg.category_tab_direction {
        TabDirection::Vertical => (start_x, start_y + step),
        TabDirection::Horizontal => (start_x + step, start_y),
    };
    (x, y, cfg.category_tab_width, cfg.category_tab_height)
}

/// 计算当前分类下第 `g` 个抽屉栏目标题栏的矩形
pub fn get_group_bar_rect(
    cfg: &SkinConfig,
    win_w: i32,
    win_h: i32,
    total_groups: usize,
    active_group: usize,
    group_index: usize,
) -> (i32, i32, i32, i32) {
    let (lx, ly, lw, lh) = cfg.item_list_box.resolve_rect(win_w, win_h);
    let bar_h = 20;
    let y = if group_index <= active_group {
        ly + (group_index as i32) * bar_h
    } else {
        let bottom_idx = total_groups.saturating_sub(group_index) as i32;
        ly + lh - bottom_idx * bar_h
    };
    (lx, y, lw, bar_h)
}

/// 标准化任意两点的框选矩形 `(left, top, right, bottom)`
#[inline]
pub fn normalize_rect(x0: i32, y0: i32, x1: i32, y1: i32) -> (i32, i32, i32, i32) {
    (x0.min(x1), y0.min(y1), x0.max(x1), y0.max(y1))
}

/// 判断两个矩形是否相交
#[inline]
pub fn rects_intersect(
    ax0: i32,
    ay0: i32,
    ax1: i32,
    ay1: i32,
    bx0: i32,
    by0: i32,
    bx1: i32,
    by1: i32,
) -> bool {
    ax0 < bx1 && ax1 > bx0 && ay0 < by1 && ay1 > by0
}

/// 计算主窗体内容区中所有可见快捷方式的布局坐标 `(orig_idx, icon_x, icon_y, hit_w, hit_h)`
pub fn compute_visible_shortcut_rects(
    cfg: &SkinConfig,
    win_w: i32,
    win_h: i32,
    cat: &CategoryTab,
    settings: &AdvancedSettings,
    scroll: usize,
) -> Vec<(usize, i32, i32, i32, i32)> {
    let Some(grid) = shortcut_grid(cfg, win_w, win_h, cat, settings) else {
        return Vec::new();
    };
    let start = scroll.min(grid.max_scroll) * grid.cols;
    (start..grid.count.min(start + grid.rows * grid.cols)).enumerate().map(|(index, item)| {
        let col = (index % grid.cols) as i32;
        let row = (index / grid.cols) as i32;
        (item, grid.left + 4 + col * grid.column_width, grid.top + row * grid.row_height,
            grid.hit_width, grid.row_height - 5)
    }).collect()
}

#[cfg(test)]
mod spacing_tests {
    use super::*;

    #[test]
    fn skin_container_and_group_edges_survive_category_selection_and_resize() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let skin = root.join("skin/LiveMessenger(搜索版)");
        let cfg = SkinConfig::load_from_ini(&skin.join("Setup.ini")).unwrap();
        let assets = SkinAssets::load(&skin, &cfg).unwrap();
        let builtin = BuiltinIcons::load(root);
        let user = UserConfig::default();
        let categories: Vec<_> = (0..2).map(|index| CategoryTab {
            id: format!("cat_{index}"), name: String::new(), icon_name: String::new(),
            encrypted: false, active_group_index: 0,
            groups: (0..2).map(|group| crate::model::ShortcutGroup {
                id: format!("col_{group}"), category_id: format!("cat_{index}"),
                title: String::new(), encrypted: false, items: Vec::new(),
            }).collect(),
        }).collect();
        let selection = HashSet::new();
        let border = RgbColor::new(190, 214, 224).to_bgra_u32();
        let white = RgbColor::new(255, 255, 255).to_bgra_u32();
        for (width, height) in [(441, 842), (350, 600), (700, 1000)] {
            for active in 0..2 {
                let state = RenderState {
                    win_w: width, win_h: height, skin_cfg: &cfg, skin_assets: &assets,
                    builtin_icons: &builtin, user_cfg: &user, categories: &categories,
                    active_category: active, hover_target: HitTarget::None,
                    down_target: HitTarget::None, time_string: "", search_query: "",
                    search_focused: false, list_background: None, shortcut_scroll: 0,
                    volume_level: None, tooltip: None, popup_menu: ActivePopupMenu::None,
                    menu_scroll: 0, skins: &[], active_skin_index: 0, selected_skin_index: 0,
                    dragging_shortcut: None, marquee_rect: None, selected_shortcuts: &selection,
                };
                let mut canvas = Canvas::new(width, height);
                render_main_frame(&state, &mut canvas);
                let pixel = |x: i32, y: i32| canvas.buffer[(y * width + x) as usize];
                // 参考皮肤的外框与栏目之间始终保留一列空白，不得叠加第二圈边线。
                for y in 90..height - 64 {
                    assert_eq!(pixel(39, y), white, "列表左侧留白 {active}/{width}/{y}");
                    assert_eq!(pixel(width - 9, y), border, "皮肤右边框");
                    if y < height - 65 {
                        assert_eq!(pixel(width - 10, y), cfg.window_bg_color.to_bgra_u32(), "右侧留白 {width}/{y}");
                    }
                }
                for group in 0..2 {
                    let (x, y, _, _) = get_group_bar_rect(&cfg, width, height, 2, 0, group);
                    for row in 1..20 {
                        assert_eq!(pixel(x, y + row), border, "栏目左边框 {active}/{group}/{row}");
                    }
                }
                assert_eq!(pixel(200, height - 64), white, "底部不能出现额外描边");
                assert_eq!(pixel(200, height - 63), border, "保留皮肤底边框");
            }
        }
    }

    #[test]
    fn shortcut_scrollbar_reaches_first_and_last_rows_while_dragging() {
        let grid = ShortcutGrid { left: 0, top: 20, bottom: 420, count: 50, cols: 1,
            rows: 7, column_width: 140, hit_width: 134, row_height: 57, max_scroll: 43 };
        let (thumb_y, _, travel) = flat_scrollbar_metrics(400, 7, 50, 0);
        let grab = grid.scrollbar_grab(thumb_y + 5, 0).unwrap();
        assert_eq!(grid.scroll_during_drag(20 + thumb_y + 5, grab), 0);
        assert_eq!(grid.scroll_during_drag(20 + thumb_y + 5 + travel, grab), 43);
        assert_eq!(grid.scroll_during_drag(2000, grab), 43);
        assert_eq!(grid.scroll_during_drag(-100, grab), 0);
    }

    #[test]
    fn skin_menu_caps_visible_rows_and_scrolls_to_last_item() {
        for count in [0, 5, 13, 15, 16, 40] {
            let (_, _, width, height) = get_skin_quick_menu_rect(800, 1200, count);
            let rows = popup_visible_rows(height);
            assert_eq!(rows, count.min(15) + 2);
            let skin_rows = rows - 2;
            for scroll in [0, count.saturating_sub(skin_rows)] {
                assert_eq!(hit_test_popup(ActivePopupMenu::SkinQuickMenu,
                    (width - 5, 1 + skin_rows as i32 * 24 + 12), (width, height), scroll, count),
                    HitTarget::SkinQuickMenuItem(count));
                assert_eq!(hit_test_popup(ActivePopupMenu::SkinQuickMenu,
                    (width - 5, 1 + (skin_rows as i32 + 1) * 24 + 12), (width, height), scroll, count),
                    HitTarget::SkinQuickMenuItem(count + 1));
            }
            if count + 2 > rows {
                assert_eq!(width, 195 + SKIN_MENU_SCROLLBAR_WIDTH);
                assert_eq!(hit_test_popup(ActivePopupMenu::SkinQuickMenu,
                    (width - 5, 5), (width, height), 0, count), HitTarget::MenuScrollUp);
                let last_scroll = count - skin_rows;
                assert_eq!(hit_test_popup(ActivePopupMenu::SkinQuickMenu,
                    (40, 1 + (skin_rows as i32 - 1) * 24 + 12), (width, height), last_scroll, count),
                    HitTarget::SkinQuickMenuItem(count - 1));
                assert_eq!(hit_test_popup(ActivePopupMenu::SkinQuickMenu,
                    (width - SKIN_MENU_SCROLLBAR_WIDTH - 2, 12), (width, height), 0, count), HitTarget::SkinQuickMenuItem(0));
            } else {
                assert_eq!(width, 195);
            }
        }
        let (_, _, _, height) = get_skin_quick_menu_rect(800, 250, 40);
        assert!(popup_visible_rows(height) < 15);
    }

    #[test]
    fn tooltip_wrapping_preserves_paths_and_chinese_remarks() {
        for text in [r"E:\soft\Everything\everything.exe", "我是备注，这是一段较长的中文说明。"] {
            let lines = wrap_tooltip_line(text, 60);
            assert_eq!(lines.concat(), text);
            assert!(lines.iter().all(|line| estimate_text_px_width(line) <= 60));
        }
    }

    #[test]
    fn resizing_preserves_spacing_in_all_display_modes() {
        let mut cfg = SkinConfig::load_from_ini(std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"), "/skin/01 默认（葱青）/Setup.ini"
        ))).unwrap();
        cfg.item_list_box.width = crate::skin_parser::CoordExpr::Right(0);
        let cat = CategoryTab {
            id: String::new(), name: String::new(), icon_name: String::new(),
            encrypted: false, active_group_index: 0,
            groups: vec![crate::model::ShortcutGroup {
                id: String::new(), category_id: String::new(), title: String::new(),
                encrypted: false, items: Vec::new(),
            }],
        };
        for (mode, large, hidden, right) in [
            (0, false, false, false), (1, false, true, false),
            (2, true, false, false), (3, true, true, false), (4, true, false, true),
        ] {
            for custom in [false, true] {
                let mut settings = AdvancedSettings::default();
                settings.set(Setting::LargeIcons, large.to_string());
                settings.set(Setting::HideTitle, hidden.to_string());
                settings.set(Setting::TitleRight, right.to_string());
                if custom { settings.spacing[mode] = 173; }
                for horizontal in [false, true] {
                    settings.set(Setting::Horizontal, horizontal.to_string());
                    for width in [120, 350, 700, 350] {
                        let grid = shortcut_grid(&cfg, width, 600, &cat, &settings).unwrap();
                        assert_eq!(grid.column_width, settings.spacing[mode] as i32);
                        assert_eq!(grid.hit_width, settings.spacing[mode] as i32 - 6);
                        assert_eq!(grid.cols, if horizontal {
                            ((width - 8) / settings.spacing[mode] as i32).max(1) as usize
                        } else { 1 });
                    }
                }
            }
        }
    }
}

pub struct ShortcutGrid {
    left: i32,
    top: i32,
    bottom: i32,
    count: usize,
    cols: usize,
    rows: usize,
    column_width: i32,
    hit_width: i32,
    row_height: i32,
    pub max_scroll: usize,
}

impl ShortcutGrid {
    pub fn scrollbar_grab(&self, offset: i32, scroll: usize) -> Option<i32> {
        let (thumb_y, thumb_h, _) = flat_scrollbar_metrics(self.bottom - self.top, self.rows,
            self.count.div_ceil(self.cols), scroll);
        (14..self.bottom - self.top - 14).contains(&offset).then_some(
            if (thumb_y..thumb_y + thumb_h).contains(&offset) { offset - thumb_y } else { thumb_h / 2 })
    }

    pub fn scroll_during_drag(&self, y: i32, grab: i32) -> usize {
        let (_, _, travel) = flat_scrollbar_metrics(self.bottom - self.top, self.rows,
            self.count.div_ceil(self.cols), 0);
        if travel == 0 { return 0; }
        let offset = (y - self.top - 14 - grab).clamp(0, travel) as usize;
        (offset * self.max_scroll + travel as usize / 2) / travel as usize
    }

    pub fn position_for_offset(&self, offset: i32) -> usize {
        if offset < 14 { 0 } else if offset >= self.bottom - self.top - 14 { self.max_scroll }
        else { self.scroll_during_drag(self.top + offset, 0) }
    }
}

pub fn shortcut_grid(cfg: &SkinConfig, w: i32, h: i32, cat: &CategoryTab, settings: &AdvancedSettings) -> Option<ShortcutGrid> {
    let (left, y, width, height) = cfg.item_list_box.resolve_rect(w, h);
    let group = cat.active_group_index.min(cat.groups.len().saturating_sub(1));
    let count = cat.groups.get(group)?.items.len();
    let top = y + ((group + 1) as i32) * 20 + 4;
    let bottom = y + height - ((cat.groups.len() - group - 1) as i32) * 20;
    let icon = if settings.enabled(Setting::LargeIcons) { 32 } else { 16 };
    let row_height = icon + if icon == 32 && !settings.enabled(Setting::TitleRight) && !settings.enabled(Setting::HideTitle) { 25 } else { 9 };
    let usable_width = (width - 8).max(1);
    let mode = match (settings.enabled(Setting::LargeIcons), settings.enabled(Setting::HideTitle), settings.enabled(Setting::TitleRight)) {
        (false, false, _) => 0, (false, true, _) => 1, (true, true, _) => 3,
        (true, false, true) => 4, _ => 2,
    };
    let spacing = settings.spacing[mode].clamp(50, 500) as i32;
    let mut cols = if !settings.enabled(Setting::Horizontal) { 1 }
        else { (usable_width / spacing).max(1) } as usize;
    let drawn_height = row_height - 7;
    let rows = if bottom - top < drawn_height { 0 } else { ((bottom - top - drawn_height) / row_height + 1) as usize };
    if count > rows * cols && settings.enabled(Setting::Horizontal) && settings.enabled(Setting::Scrollbar) {
        cols = ((usable_width - SKIN_MENU_SCROLLBAR_WIDTH).max(1) / spacing).max(1) as usize;
    }
    // 间距设置决定固定列宽，窗口缩放只改变列数，不摊分剩余宽度。
    let column_width = spacing;
    let hit_width = (column_width - 6).max(icon + 4);
    Some(ShortcutGrid {
        left, top, bottom, count, cols, rows, column_width, hit_width,
        row_height, max_scroll: count.div_ceil(cols).saturating_sub(rows),
    })
}

/// 主窗口鼠标坐标 `(mx, my)` 命中测试
#[allow(clippy::too_many_arguments)]
pub fn hit_test_main(
    mx: i32,
    my: i32,
    win_w: i32,
    win_h: i32,
    cfg: &SkinConfig,
    assets: &SkinAssets,
    categories: &[CategoryTab],
    active_category: usize,
    settings: &AdvancedSettings,
    scroll: usize,
) -> HitTarget {
    let in_rect = |rx: i32, ry: i32, rw: i32, rh: i32| -> bool {
        mx >= rx && mx < rx + rw && my >= ry && my < ry + rh
    };

    // 2. 右上角最小化、皮肤、关闭按钮
    let (min_x, min_y, min_w, min_h) = cfg.title_btn_min.resolve_rect(win_w, win_h);
    if in_rect(min_x, min_y, min_w, min_h) {
        return HitTarget::TitleMin;
    }
    let (sk_x, sk_y, sk_w, sk_h) = cfg.title_btn_skin.resolve_rect(win_w, win_h);
    if in_rect(sk_x, sk_y, sk_w, sk_h) {
        return HitTarget::TitleSkin;
    }
    let (cl_x, cl_y, cl_w, cl_h) = cfg.title_btn_close.resolve_rect(win_w, win_h);
    if in_rect(cl_x, cl_y, cl_w, cl_h) {
        return HitTarget::TitleClose;
    }

    // 3. 左上角大图标预览框
    let (px, py, pw, ph) = cfg.preview_icon_box.resolve_rect(win_w, win_h);
    if in_rect(px, py, pw, ph) {
        return HitTarget::PreviewIcon;
    }

    // 4. 11 个语义化功能按钮
    for btn in &cfg.tool_buttons {
        let bx = btn.left.resolve(win_w, win_h);
        let by = btn.top.resolve(win_w, win_h);
        if bx >= 0 && by >= 0 {
            if let Some(tri) = assets.tool_buttons.get(&btn.kind) {
                let bw = tri.normal.width as i32;
                let bh = tri.normal.height as i32;
                if in_rect(bx, by, bw, bh) {
                    return HitTarget::ToolButton(btn.kind);
                }
            }
        }
    }

    // 5. 底部搜索框与搜索按钮
    if cfg.search_box_visible {
        let (sbx, sby, sbw, sbh) = cfg.search_btn_layout.resolve_rect(win_w, win_h);
        if !settings.enabled(Setting::HideSearchButton) && in_rect(sbx, sby, sbw, sbh) {
            return HitTarget::SearchBtn;
        }
        let (sx, sy, sw, sh) = cfg.search_box_layout.resolve_rect(win_w, win_h);
        if in_rect(sx, sy - 2, sw, sh + 4) {
            return HitTarget::SearchBox;
        }
    }

    // 6. 左侧/顶部分类 Tab 按钮
    for i in 0..categories.len() {
        let (tx, ty, tw, th) = get_category_tab_rect(cfg, win_w, win_h, i);
        if in_rect(tx, ty, tw, th) {
            return HitTarget::CategoryTab(i);
        }
    }

    // 7. 中部列表区（抽屉栏目标题栏 + 快捷方式项 + 列表空白区）
    let (lx, ly, lw, lh) = cfg.item_list_box.resolve_rect(win_w, win_h);
    if in_rect(lx, ly, lw, lh) {
        if let Some(cat) = categories.get(active_category) {
            let total_groups = cat.groups.len();
            let active_grp = cat.active_group_index.min(total_groups.saturating_sub(1));

            for g in 0..total_groups {
                let (gx, gy, gw, gh) =
                    get_group_bar_rect(cfg, win_w, win_h, total_groups, active_grp, g);
                if in_rect(gx, gy, gw, gh) {
                    return HitTarget::GroupBar(g);
                }
            }
            if mx >= lx + lw - SKIN_MENU_SCROLLBAR_WIDTH - 1 {
                if let Some(grid) = shortcut_grid(cfg, win_w, win_h, cat, settings) {
                    if settings.enabled(Setting::Scrollbar) && grid.max_scroll > 0 && my >= grid.top && my < grid.bottom {
                        return HitTarget::ListScroll(my - grid.top);
                    }
                }
            }

            for (orig_idx, icon_x, icon_y, hw, hh) in
                compute_visible_shortcut_rects(cfg, win_w, win_h, cat, settings, scroll)
            {
                if in_rect(icon_x - 2, icon_y - 2, hw, hh) {
                    return HitTarget::ShortcutItem(orig_idx);
                }
            }
        }
        return HitTarget::ListBlank;
    }

    HitTarget::DragCaption
}

/// 独立弹窗窗口内部鼠标命中测试
pub fn hit_test_modal(
    mx: i32,
    my: i32,
    modal: &ActiveModal,
    categories: &[CategoryTab],
    active_category: usize,
    skins_count: usize,
) -> HitTarget {
    let (dw, dh) = modal.window_size();
    let in_rect = |rx: i32, ry: i32, rw: i32, rh: i32| -> bool {
        mx >= rx && mx < rx + rw && my >= ry && my < ry + rh
    };

    match modal {
        ActiveModal::SkinManager => {
            if in_rect(dw - 26, 4, 20, 18) || in_rect(dw - 80, dh - 34, 68, 24) {
                return HitTarget::SkinMgrClose;
            }
            if in_rect(dw - 160, dh - 34, 72, 24) {
                return HitTarget::SkinMgrApply;
            }
            if my < 26 {
                return HitTarget::ModalTitleDrag;
            }
            for i in 0..skins_count {
                let iy = 38 + (i as i32) * 24;
                if in_rect(10, iy, 140, 22) {
                    return HitTarget::SkinMgrSelect(i);
                }
            }
            HitTarget::None
        }
        ActiveModal::CategoryManager { .. } => {
            if in_rect(dw - 26, 4, 20, 18) || in_rect(dw - 76, dh - 32, 64, 22) {
                return HitTarget::CatMgrClose;
            }
            if my < 26 {
                return HitTarget::ModalTitleDrag;
            }
            let btn_w = 58;
            for (b_idx, target) in [
                HitTarget::CatMgrBtnNew,
                HitTarget::CatMgrBtnDelete,
                HitTarget::CatMgrBtnRename,
                HitTarget::CatMgrBtnEncrypt,
                HitTarget::CatMgrBtnMoveUp,
                HitTarget::CatMgrBtnMoveDown,
            ]
            .iter()
            .enumerate()
            {
                let bx = 10 + (b_idx as i32) * (btn_w + 2);
                if in_rect(bx, 32, btn_w, 24) {
                    return *target;
                }
            }
            for i in 0..categories.len() {
                let ry = 68 + (i as i32) * 22;
                if in_rect(10, ry, dw - 20, 22) {
                    return HitTarget::CatMgrSelectRow(i);
                }
            }
            if in_rect(96, dh - 32, 64, 22) {
                return HitTarget::CatMgrBtnChangeIcon;
            }
            HitTarget::None
        }
        ActiveModal::ColumnSort { .. } => {
            if in_rect(dw - 26, 4, 20, 18) || in_rect(dw - 76, dh - 32, 64, 22) {
                return HitTarget::ColSortClose;
            }
            if my < 26 {
                return HitTarget::ModalTitleDrag;
            }
            if let Some(cat) = categories.get(active_category) {
                for i in 0..cat.groups.len() {
                    let ry = 60 + (i as i32) * 24;
                    if in_rect(12, ry, dw - 24, 24) {
                        return HitTarget::ColSortRow(i);
                    }
                }
            }
            HitTarget::None
        }
        ActiveModal::HotkeyManager(_) => HitTarget::None,
        ActiveModal::EditShortcut(ed) => {
            if in_rect(dw - 28, 4, 22, 18) || in_rect(dw - 84, dh - 34, 70, 24) {
                return HitTarget::EditScCancel;
            }
            if in_rect(dw - 162, dh - 34, 70, 24) {
                return HitTarget::EditScOk;
            }
            if my < 26 {
                return HitTarget::ModalTitleDrag;
            }
            // 页签切换：【常规】 / 【备注】
            if in_rect(16, 36, 48, 22) {
                return HitTarget::EditScTab(0);
            }
            if in_rect(65, 36, 48, 22) {
                return HitTarget::EditScTab(1);
            }
            if ed.active_tab == 1 {
                if in_rect(26, 72, dw - 52, dh - 128) {
                    return HitTarget::EditScField(EditShortcutField::Remark);
                }
                return HitTarget::None;
            }
            if in_rect(116, 80, dw - 146, 22) {
                return HitTarget::EditScField(EditShortcutField::Title);
            }
            if in_rect(248, 125, 20, 20) {
                return HitTarget::EditScPickFile;
            }
            if in_rect(268, 125, 20, 20) {
                return HitTarget::EditScPickDir;
            }
            if in_rect(76, 152, dw - 110, 22) {
                return HitTarget::EditScField(EditShortcutField::TargetPath);
            }
            if in_rect(dw - 54, 184, 22, 22) {
                return HitTarget::EditScPickIcon;
            }
            if in_rect(76, 184, dw - 140, 22) {
                return HitTarget::EditScField(EditShortcutField::IconKey);
            }
            if in_rect(76,214,dw-110,22) {return HitTarget::EditScPickColor;}
            if in_rect(76, 266, dw - 110, 22) {
                return HitTarget::EditScField(EditShortcutField::Args);
            }
            if in_rect(76, 298, dw - 110, 22) {
                return HitTarget::EditScField(EditShortcutField::WorkDir);
            }
            if in_rect(76, 330, dw - 110, 22) {
                return HitTarget::EditScField(EditShortcutField::Hotkey);
            }
            if in_rect(28, 368, 200, 20) {
                return HitTarget::EditScToggleMinOnRun;
            }
            HitTarget::None
        }
        ActiveModal::About => {
            if in_rect(dw - 34, 4, 28, 18) || in_rect(432, 398, 72, 22) {
                return HitTarget::AboutClose;
            }
            if my < 24 {
                return HitTarget::ModalTitleDrag;
            }
            if in_rect(14, 400, 190, 20) {
                return HitTarget::AboutSupportLink;
            }
            for l_idx in 0..3 {
                let ly = 114 + (l_idx as i32) * 24;
                if in_rect(175, ly, 290, 20) {
                    return HitTarget::AboutLink(l_idx);
                }
            }
            HitTarget::None
        }
        ActiveModal::None | ActiveModal::Search(_) | ActiveModal::Advanced(_) => HitTarget::None,
    }
}

/// 估算 12px 宋体字符串渲染像素宽度（ASCII 占 6px，汉字/全角占 12px）
pub(crate) fn estimate_text_px_width(s: &str) -> i32 {
    let mut w = 0i32;
    for ch in s.chars() {
        w += if ch.is_ascii() { 6 } else { 12 };
    }
    w
}

/// 提示内容按字符折行，长路径和中文备注均保留完整内容。
pub(crate) fn wrap_tooltip_line(line: &str, max_width: i32) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    let mut width = 0;
    for ch in line.chars().filter(|ch| *ch != '\r') {
        if ch == '\n' {
            lines.push(std::mem::take(&mut current));
            width = 0;
            continue;
        }
        let advance = if ch.is_ascii() { 6 } else { 12 };
        if width + advance > max_width && !current.is_empty() {
            lines.push(std::mem::take(&mut current));
            width = 0;
        }
        current.push(ch);
        width += advance;
    }
    lines.push(current);
    lines
}

/// 渲染主窗口当前帧到 `Canvas`，返回 GDI 文本指令
pub fn render_main_frame(
    state: &RenderState<'_>,
    canvas: &mut Canvas,
) -> (Vec<TextDrawCmd>, Option<PopupTextClip>) {
    let w = state.win_w;
    let h = state.win_h;
    let cfg = state.skin_cfg;
    let assets = state.skin_assets;
    let builtin = state.builtin_icons;

    canvas.resize(w, h);
    let mut texts = Vec::with_capacity(64);

    let widget_state = |target: HitTarget| -> WidgetState {
        if state.down_target == target {
            WidgetState::Down
        } else if state.hover_target == target {
            WidgetState::Hover
        } else {
            WidgetState::Normal
        }
    };

    // 第 1 层：八切片窗口背景拼合
    canvas.fill_rect(0, 0, w, h, cfg.window_bg_color);

    let w5 = assets.skin_top_left.width as i32;
    let h5 = assets.skin_top_left.height as i32;
    let w6 = assets.skin_top_right.width as i32;
    let h6 = assets.skin_top_right.height as i32;
    let w7 = assets.skin_bottom_left.width as i32;
    let h7 = assets.skin_bottom_left.height as i32;
    let w8 = assets.skin_bottom_right.width as i32;
    let h8 = assets.skin_bottom_right.height as i32;

    let w1 = assets.skin_left.width as i32;
    canvas.blit_stretch(&assets.skin_left, 0, h5, w1, (h - h5 - h7).max(0));

    let h2 = assets.skin_top.height as i32;
    canvas.blit_stretch(&assets.skin_top, w5, 0, (w - w5 - w6).max(0), h2);

    let w3 = assets.skin_right.width as i32;
    canvas.blit_stretch(&assets.skin_right, w - w3, h6, w3, (h - h6 - h8).max(0));

    let h4 = assets.skin_bottom.height as i32;
    canvas.blit_stretch(
        &assets.skin_bottom,
        w7,
        h - h4,
        (w - w7 - w8).max(0),
        h4,
    );

    canvas.blit(&assets.skin_top_left, 0, 0);
    canvas.blit(&assets.skin_top_right, w - w6, 0);
    canvas.blit(&assets.skin_bottom_left, 0, h - h7);
    canvas.blit(&assets.skin_bottom_right, w - w8, h - h8);

    // 第 2 层：右上角按钮、时间标签、左上角大图标预览框
    let (min_x, min_y, _, _) = cfg.title_btn_min.resolve_rect(w, h);
    canvas.blit(
        assets.btn_min.get(widget_state(HitTarget::TitleMin)),
        min_x,
        min_y,
    );

    let (sk_x, sk_y, _, _) = cfg.title_btn_skin.resolve_rect(w, h);
    canvas.blit(
        assets.btn_skin.get(widget_state(HitTarget::TitleSkin)),
        sk_x,
        sk_y,
    );

    let (cl_x, cl_y, _, _) = cfg.title_btn_close.resolve_rect(w, h);
    canvas.blit(
        assets.btn_close.get(widget_state(HitTarget::TitleClose)),
        cl_x,
        cl_y,
    );

    let time_x = cfg.time_label_left.resolve(w, h);
    let time_y = cfg.time_label_top.resolve(w, h);
    if let Some(level) = state.volume_level {
        canvas.fill_rect(time_x, time_y, 107, 14, cfg.menu_colors.bg_color);
        canvas.fill_rect(time_x + 1, time_y + 1, i32::from(level), 12, cfg.menu_colors.item_hover_bg_color);
        canvas.stroke_rect(time_x, time_y, 107, 14, cfg.menu_colors.border_color);
    }
    texts.push(TextDrawCmd {
        text: state.volume_level.map_or_else(|| state.time_string.to_string(), |level| format!("音量 {level}%")),
        x: time_x,
        y: time_y,
        w: 220,
        h: if state.volume_level.is_some(){14}else{14 * state.time_string.lines().count().max(1) as i32},
        color: cfg.time_label_color,
        bold: cfg.time_label_bold,
        align: TextAlign::Left,
        mnemonic: false,
        multiline: state.volume_level.is_none() && state.time_string.contains('\n'),
        font_slot: Some(3),
    });

    let (px, py, _, _) = cfg.preview_icon_box.resolve_rect(w, h);
    let preview_base = &assets.preview_base;
    canvas.blit(preview_base, px, py);
    let folder_icon = &builtin.folder_large;
    let icon_x = px + (preview_base.width as i32 - folder_icon.width as i32) / 2;
    let icon_y = py + (preview_base.height as i32 - folder_icon.height as i32) / 2;
    canvas.blit(folder_icon, icon_x, icon_y);

    // 第 3 层：11 个功能按钮
    for btn in &cfg.tool_buttons {
        let bx = btn.left.resolve(w, h);
        let by = btn.top.resolve(w, h);
        if bx >= 0 && by >= 0 {
            if let Some(tri) = assets.tool_buttons.get(&btn.kind) {
                let st = widget_state(HitTarget::ToolButton(btn.kind));
                canvas.blit(tri.get(st), bx, by);
            }
        }
    }

    // 第 4 层：中部 Outlook 抽屉式分组列表区域 (ItemList)
    let (lx, ly, lw, lh) = cfg.item_list_box.resolve_rect(w, h);
    let settings = &state.user_cfg.advanced;
    let list_color = crate::advanced::color(settings.text(Setting::ListBackgroundColor))
        .unwrap_or(cfg.item_list_bg_color);
    canvas.fill_rect(lx, ly, lw, lh, list_color);
    if let Some(background) = state.list_background {
        canvas.blit_stretch(background, lx, ly, lw, lh);
    }
    if settings.enabled(Setting::VistaMask) && lh > 0 {
        let height = lh.min(58);
        for row in 0..height {
            canvas.fill_rect_alpha(lx, ly + row, lw, 1, RgbColor::new(255, 255, 255),
                (54 * (height - row) / height) as u8);
        }
    }

    // 容器边框由八切片皮肤提供，额外描边会覆盖留白并形成重复边线。

    if state.user_cfg.enable_list {
        if let Some(cat) = state.categories.get(state.active_category) {
            let total_groups = cat.groups.len();
            let active_grp = cat.active_group_index.min(total_groups.saturating_sub(1));

            for (g_idx, grp) in cat.groups.iter().enumerate() {
                let (gx, gy, gw, gh) =
                    get_group_bar_rect(cfg, w, h, total_groups, active_grp, g_idx);
                let is_hover = state.hover_target == HitTarget::GroupBar(g_idx)
                    || state.down_target == HitTarget::GroupBar(g_idx);
                let src_y = if is_hover && assets.group_bar.height >= 40 {
                    20
                } else {
                    0
                };
                let src_h = 20.min(assets.group_bar.height);
                canvas.blit_horizontal_3slice(&assets.group_bar, src_y, src_h, 2, gx, gy, gw, gh);

                if g_idx == active_grp {
                    canvas.blit(&builtin.group_search, gx + 4, gy + 2);
                }

                texts.push(TextDrawCmd {
                    text: format!("-= {} =-", grp.title),
                    x: gx,
                    y: gy + 1,
                    w: gw,
                    h: gh - 1,
                    color: crate::advanced::color(settings.text(Setting::GroupColor)).unwrap_or(cfg.group_bar_text_color),
                    bold: false,
                    align: TextAlign::Center,
                    mnemonic: false,
                    multiline: false,
                    font_slot: Some(1),
                });
            }

            if let Some(group) = cat.groups.get(active_grp) {
                let rects = compute_visible_shortcut_rects(cfg, w, h, cat, settings, state.shortcut_scroll);
                let icon_size = if settings.enabled(Setting::LargeIcons) { 32 } else { 16 };
                let show_title = !settings.enabled(Setting::HideTitle);
                for (orig_idx, cell_x, icon_y, hw, _hit_height) in rects {
                    let Some(item) = group.items.get(orig_idx) else {
                        continue;
                    };
                    let below = icon_size == 32 && !settings.enabled(Setting::TitleRight);
                    // 大图标上下布局共用单元格中心，命中区域仍保持整列宽度。
                    let icon_x = if below { cell_x - 2 + (hw - icon_size) / 2 } else { cell_x };
                    let text_x = if below { cell_x - 2 } else { icon_x + icon_size + 4 };
                    let text_top = if below { icon_y + icon_size + 4 } else { icon_y + (icon_size - 14) / 2 };
                    let text_width = if below { hw } else { (hw - icon_size - 6).max(0) };
                    let is_selected = state.selected_shortcuts.contains(&orig_idx);
                    let is_hover = state.hover_target == HitTarget::ShortcutItem(orig_idx);
                    let is_being_dragged = state
                        .dragging_shortcut
                        .map(|(d_idx, _, _)| d_idx == orig_idx)
                        .unwrap_or(false);

                    // 1:1 复刻截图 5 的选中高亮样式：
                    // 图标框 #0A246A 1px 边框 + #B2BAD0 底色；文字背景填充 #0A246A + 纯白文字 #FFFFFF
                    if is_selected {
                        canvas.fill_rect(
                            icon_x - 2,
                            icon_y - 2,
                            icon_size + 4,
                            icon_size + 4,
                            RgbColor::new(0xB2, 0xBA, 0xD0),
                        );
                        canvas.stroke_rect(
                            icon_x - 2,
                            icon_y - 2,
                            icon_size + 4,
                            icon_size + 4,
                            RgbColor::new(0x0A, 0x24, 0x6A),
                        );
                        if show_title {
                            let highlight_width = (estimate_text_px_width(&item.name) + 4).min(text_width);
                            canvas.fill_rect(if below { text_x + (text_width - highlight_width) / 2 } else { text_x - 1 }, text_top,
                                highlight_width, 15,
                                RgbColor::new(0x0A, 0x24, 0x6A));
                        }
                    } else if is_hover || is_being_dragged {
                        canvas.stroke_rect(
                            icon_x - 2,
                            icon_y - 2,
                            icon_size + 4,
                            icon_size + 4,
                            cfg.menu_colors.item_border_color,
                        );
                    }

                    if let Some(custom_ico) = &item.custom_icon_16 {
                        let icon = custom_ico.resize_to(icon_size as u32, icon_size as u32);
                        canvas.blit(&icon, icon_x, icon_y);
                    } else if let Some(ico) = builtin.app_icons.get(&item.icon_key) {
                        let icon = ico.resize_to(icon_size as u32, icon_size as u32);
                        canvas.blit(&icon, icon_x, icon_y);
                    }

                    if !show_title { continue; }
                    texts.push(TextDrawCmd {
                        text: item.name.clone(),
                        x: text_x,
                        y: text_top,
                        w: text_width,
                        h: 15,
                        color: if is_selected {
                            RgbColor::new(255, 255, 255)
                        } else {
                            if item.use_default_color { crate::advanced::color(settings.text(Setting::DefaultColor)).unwrap_or(RgbColor::new(0,0,0)) }
                            else { item.text_color }
                        },
                        bold: false,
                        align: if below { TextAlign::Center } else { TextAlign::Left },
                        mnemonic: false,
                        multiline: false,
                        font_slot: Some(0),
                    });
                }
                {
                    if let Some(grid) = shortcut_grid(cfg, w, h, cat, settings) {
                        if grid.max_scroll > 0 && settings.enabled(Setting::Scrollbar) {
                            let height = grid.bottom - grid.top;
                            draw_flat_scrollbar(canvas, lx + lw - SKIN_MENU_SCROLLBAR_WIDTH - 1,
                                grid.top, SKIN_MENU_SCROLLBAR_WIDTH, height, grid.rows,
                                grid.count.div_ceil(grid.cols), state.shortcut_scroll);
                        }
                    }
                }

                // 正在进行矩形框选（截图 4：1px 纯黑实线空心框）
                if let Some((x0, y0, x1, y1)) = state.marquee_rect {
                    let (rx0, ry0, rx1, ry1) = normalize_rect(x0, y0, x1, y1);
                    let cx0 = rx0.clamp(lx, lx + lw - 1);
                    let cy0 = ry0.clamp(ly + 20, ly + lh - 20);
                    let cx1 = rx1.clamp(lx, lx + lw - 1);
                    let cy1 = ry1.clamp(ly + 20, ly + lh - 20);
                    canvas.stroke_rect(
                        cx0,
                        cy0,
                        (cx1 - cx0).max(1),
                        (cy1 - cy0).max(1),
                        RgbColor::new(0, 0, 0),
                    );
                }

                // 正在拖拽快捷方式：跟随鼠标绘制半透明预览框
                if let Some((drag_idx, dmx, dmy)) = state.dragging_shortcut {
                    if let Some(item) = group.items.get(drag_idx) {
                        canvas.fill_rect_alpha(
                            dmx - 8,
                            dmy - 8,
                            120,
                            20,
                            cfg.menu_colors.item_hover_bg_color,
                            190,
                        );
                        canvas.stroke_rect(
                            dmx - 8,
                            dmy - 8,
                            120,
                            20,
                            cfg.menu_colors.item_border_color,
                        );
                        if let Some(custom_ico) = &item.custom_icon_16 {
                            canvas.blit_stretch(custom_ico, dmx - 6, dmy - 6, 16, 16);
                        } else if let Some(ico) = builtin.app_icons.get(&item.icon_key) {
                            canvas.blit(ico, dmx - 6, dmy - 6);
                        }
                        texts.push(TextDrawCmd {
                            text: item.name.clone(),
                            x: dmx + 14,
                            y: dmy - 6,
                            w: 96,
                            h: 16,
                            color: item.text_color,
                            bold: false,
                            align: TextAlign::Left,
                            mnemonic: false,
                            multiline: false,
                            font_slot: None,
                        });
                    }
                }
            }
        }
    }

    // 第 5 层：左侧/顶部类别标签栏 (CategoryTab)
    for (i, cat) in state.categories.iter().enumerate() {
        let (tx, ty, tw, th) = get_category_tab_rect(cfg, w, h, i);
        let tab_state = if i == state.active_category {
            WidgetState::Down
        } else {
            widget_state(HitTarget::CategoryTab(i))
        };
        canvas.blit(assets.category_tab.get(tab_state), tx, ty);

        if i == state.active_category && cfg.category_tab_direction == TabDirection::Vertical {
            // 只连接类别标签与列表之间的留白，不能擦入栏目标题的第一列。
            canvas.fill_rect(
                tx + tw - 1,
                ty + 1,
                (lx - (tx + tw - 1)).max(0),
                th - 2,
                list_color,
            );
        }

        let ico_x = tx + cfg.category_tab_icon_offset_x;
        let ico_y = ty + cfg.category_tab_icon_offset_y;
        let cat_ico = builtin.category_icon(&cat.icon_name);
        canvas.blit(cat_ico, ico_x, ico_y);
        if let Some(label) = cfg.category_tab_text {
            texts.push(TextDrawCmd {
                text: cat.name.clone(),
                x: tx + label.left,
                y: ty + label.top,
                w: label.width,
                h: label.height,
                color: label.color,
                bold: false,
                align: TextAlign::Left,
                mnemonic: false,
                multiline: false,
                font_slot: Some(2),
            });
        }
    }

    // 第 6 层：底部搜索输入框与搜索按钮
    if cfg.search_box_visible {
        let (sx, sy, sw, sh) = cfg.search_box_layout.resolve_rect(w, h);
        if !cfg.search_box_use_skin_background {
            canvas.fill_rect(sx - 2, sy - 2, sw + 4, sh + 4, cfg.search_box_bg_color);
            canvas.stroke_rect(
                sx - 2,
                sy - 2,
                sw + 4,
                sh + 4,
                RgbColor::new(0xB5, 0xBD, 0xC6),
            );
        }

        if !state.search_query.is_empty() || state.search_focused {
            let display_str = if state.search_focused {
                format!("{}|", state.search_query)
            } else {
                state.search_query.to_string()
            };
            texts.push(TextDrawCmd {
                text: display_str,
                x: sx + 2,
                y: sy - 1,
                w: sw - 4,
                h: sh + 2,
                color: cfg.search_box_text_color,
                bold: false,
                align: TextAlign::Left,
                mnemonic: false,
                multiline: false,
                font_slot: None,
            });
        }

        if let Some(sbtn) = assets.search_btn.as_ref().filter(|_| !settings.enabled(Setting::HideSearchButton)) {
            let (sbx, sby, _, _) = cfg.search_btn_layout.resolve_rect(w, h);
            canvas.blit(sbtn.get(widget_state(HitTarget::SearchBtn)), sbx, sby);
        }
    }

    let mut text_clip_layers = Vec::new();
    if let Some((x, y, label)) = state.tooltip {
        let lines: Vec<&str> = label.split('\n').collect();
        let width = (lines.iter().map(|line| estimate_text_px_width(line)).max().unwrap_or(0) + 24)
            .clamp(60, (w - 8).max(60));
        let mut tip_texts = Vec::new();
        let mut offset = 8;
        let mut separator = None;
        for (index, line) in lines.iter().enumerate() {
            if index == 2 && lines[2..].iter().any(|line| !line.is_empty()) {
                separator = Some(offset + 3);
                offset += 10;
            }
            if index >= 2 && line.is_empty() { continue; }
            for wrapped in wrap_tooltip_line(line, width - 20) {
                tip_texts.push((wrapped, offset, index == 0));
                offset += 18;
            }
        }
        let height = offset + 8;
        let x = (*x).clamp(4, (w - width - 4).max(4));
        let y = (*y).clamp(4, (h - height - 4).max(4));
        canvas.fill_rect(x, y, width, height, RgbColor::new(255, 255, 225));
        canvas.stroke_rect(x, y, width, height, RgbColor::new(0, 0, 0));
        if let Some(offset) = separator {
            canvas.fill_rect(x + 10, y + offset, width - 20, 1, RgbColor::new(110, 110, 90));
        }
        text_clip_layers.push(((x, y, width, height), texts.len()));
        texts.extend(tip_texts.into_iter().map(|(text, offset, bold)| TextDrawCmd {
            text, x: x + 10, y: y + offset, w: width - 20, h: 18,
            color: RgbColor::new(0,0,0), bold, align: TextAlign::Left,
            mnemonic: false, multiline: false, font_slot: None,
        }));
    }
    // 离线截图保留菜单预览，运行时由独立窗口绘制。
    if let Some(rect) = popup_rect(state.popup_menu, w, h, state.skins.len()) {
        let first_menu_command = texts.len();
        let menu = PopupMenuFrame {
            menu: state.popup_menu, skin_cfg: cfg, skin_assets: assets, builtin,
            user_cfg: state.user_cfg, skins: state.skins, active_skin_index: state.active_skin_index,
            categories: state.categories,
            scroll: state.menu_scroll, hover: state.hover_target,
        };
        render_popup_menu(&menu, canvas, &mut texts, rect);
        text_clip_layers.push((rect, first_menu_command));
    }
    let popup_clip = (!text_clip_layers.is_empty()).then_some(PopupTextClip { layers: text_clip_layers });
    (texts, popup_clip)
}

pub const SEARCH_MENU_WIDTH: i32 = 116;
pub const SEARCH_MENU_HEIGHT: i32 = 2 + 24 * SearchMode::OPTIONS.len() as i32;

pub struct PopupMenuFrame<'a> {
    pub menu: ActivePopupMenu,
    pub skin_cfg: &'a SkinConfig,
    pub skin_assets: &'a SkinAssets,
    pub builtin: &'a BuiltinIcons,
    pub user_cfg: &'a UserConfig,
    pub skins: &'a [SkinEntry],
    pub categories: &'a [CategoryTab],
    pub active_skin_index: usize,
    pub scroll: usize,
    pub hover: HitTarget,
}

pub fn render_popup_menu(
    frame: &PopupMenuFrame<'_>, canvas: &mut Canvas, texts: &mut Vec<TextDrawCmd>,
    (x, y, width, height): (i32, i32, i32, i32),
) {
    let hover = frame.hover;
    macro_rules! draw {
        ($items:expr, $target:path) => {
            render_generic_popup_menu(
                frame.skin_cfg, frame.skin_assets, frame.builtin, canvas, texts,
                x, y, width, height, frame.scroll, &$items,
                |index| hover == $target(index),
            )
        };
    }
    match frame.menu {
        ActivePopupMenu::MainMenu => draw!(
            build_popup_menu_items(
                frame.user_cfg.always_on_top, frame.user_cfg.pin_to_desktop, frame.user_cfg.lock_move,
                frame.user_cfg.enable_list, frame.user_cfg.enable_hotkey, frame.user_cfg.startup_lock,
            ), HitTarget::PopupMenuItem
        ),
        ActivePopupMenu::SkinQuickMenu => {
            let items = skin_quick_menu_items(frame.skins, frame.active_skin_index);
            let count = frame.skins.len();
            let skin_height = popup_visible_rows(height).saturating_sub(2) as i32 * 24;
            if skin_height > 0 {
                render_generic_popup_menu_with_bar(frame.skin_cfg, frame.skin_assets, frame.builtin,
                    canvas, texts, x, y, width, skin_height + 2, frame.scroll, &items[..count],
                    |index| hover == HitTarget::SkinQuickMenuItem(index), SKIN_MENU_SCROLLBAR_WIDTH);
            }
            render_generic_popup_menu(frame.skin_cfg, frame.skin_assets, frame.builtin,
                canvas, texts, x, y + skin_height, width, 50, 0, &items[count..],
                |index| hover == HitTarget::SkinQuickMenuItem(count + index));
        },
        ActivePopupMenu::ContextMenu { .. } => draw!(build_context_menu_items(), HitTarget::ContextMenuItem),
        ActivePopupMenu::ItemMenu { .. } => draw!(build_item_menu_items(), HitTarget::ItemMenuItem),
        ActivePopupMenu::SearchMode => draw!(
            SearchMode::OPTIONS.map(|(mode, label)| PopupMenuItem {
                action: mode,
                label: label.to_string(),
                icon: MenuIconKind::Checkbox(frame.user_cfg.search_mode == mode),
                custom_text_color: None,
                has_separator_below: mode == SearchMode::Shortcuts,
            }),
            HitTarget::SearchModeItem
        ),
        ActivePopupMenu::TrayStart { category, .. } => {
            let items = frame.categories.get(category).into_iter().flat_map(|cat| &cat.groups)
                .flat_map(|group| &group.items).map(|item| PopupMenuItem {
                    action: (),
                    label: item.name.replace('&', "&&"),
                    icon: MenuIconKind::None,
                    custom_text_color: None,
                    has_separator_below: false,
                }).collect::<Vec<_>>();
            draw!(items, HitTarget::TrayShortcut)
        }
        ActivePopupMenu::None => {}
    }
}

/// 渲染通用自绘弹出菜单（支持过长时自动裁剪高度并绘制右侧垂直滚动条）
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_generic_popup_menu<A: Copy>(
    skin_cfg: &SkinConfig,
    assets: &SkinAssets,
    builtin: &BuiltinIcons,
    canvas: &mut Canvas,
    texts: &mut Vec<TextDrawCmd>,
    menu_x: i32,
    menu_y: i32,
    menu_w: i32,
    menu_h: i32,
    scroll_row: usize,
    items: &[PopupMenuItem<A>],
    is_row_hover: impl Fn(usize) -> bool,
) {
    render_generic_popup_menu_with_bar(skin_cfg, assets, builtin, canvas, texts,
        menu_x, menu_y, menu_w, menu_h, scroll_row, items, is_row_hover, 15);
}

#[allow(clippy::too_many_arguments)]
fn render_generic_popup_menu_with_bar<A: Copy>(
    skin_cfg: &SkinConfig, assets: &SkinAssets, builtin: &BuiltinIcons,
    canvas: &mut Canvas, texts: &mut Vec<TextDrawCmd>,
    menu_x: i32, menu_y: i32, menu_w: i32, menu_h: i32,
    scroll_row: usize, items: &[PopupMenuItem<A>],
    is_row_hover: impl Fn(usize) -> bool, scrollbar_width: i32,
) {
    let mc = &skin_cfg.menu_colors;

    let shadow_alphas = [95u8, 65, 38, 16];
    for (i, &alpha) in shadow_alphas.iter().enumerate() {
        let d = (i + 1) as i32;
        canvas.fill_rect_alpha(
            menu_x + menu_w + d - 1,
            menu_y + d,
            1,
            menu_h,
            RgbColor::new(0, 0, 0),
            alpha,
        );
        canvas.fill_rect_alpha(
            menu_x + d,
            menu_y + menu_h + d - 1,
            menu_w,
            1,
            RgbColor::new(0, 0, 0),
            alpha,
        );
    }

    canvas.blit_stretch(
        &assets.menu_left_strip,
        menu_x + 1,
        menu_y + 1,
        24,
        menu_h - 2,
    );
    canvas.fill_rect(menu_x + 25, menu_y + 1, menu_w - 26, menu_h - 2, mc.bg_color);
    canvas.stroke_rect(menu_x, menu_y, menu_w, menu_h, mc.border_color);

    let total_rows = items.len();
    let visible_rows = ((menu_h - 2) / 24).max(1) as usize;
    let has_sb = total_rows > visible_rows;
    let max_scroll = total_rows.saturating_sub(visible_rows);
    let start_row = scroll_row.min(max_scroll);
    let end_row = (start_row + visible_rows).min(total_rows);
    let content_w = if has_sb { menu_w - scrollbar_width } else { menu_w };

    for (local_idx, idx) in (start_row..end_row).enumerate() {
        let item = &items[idx];
        let item_y = menu_y + 1 + (local_idx as i32) * 24;
        if is_row_hover(idx) {
            canvas.fill_rect(
                menu_x + 2,
                item_y + 1,
                content_w - 4,
                22,
                mc.item_hover_bg_color,
            );
            canvas.stroke_rect(
                menu_x + 2,
                item_y + 1,
                content_w - 4,
                22,
                mc.item_border_color,
            );
        }

        match item.icon {
            MenuIconKind::Checkbox(checked) => {
                if checked {
                    let box_x = menu_x + 3;
                    let box_y = item_y + 2;
                    canvas.fill_rect(box_x, box_y, 20, 20, mc.item_hover_bg_color);
                    canvas.stroke_rect(box_x, box_y, 20, 20, mc.item_border_color);
                    canvas.blit(&assets.menu_check, box_x + 2, box_y + 2);
                }
            }
            MenuIconKind::Options => canvas.blit(&builtin.menu_options, menu_x + 5, item_y + 4),
            MenuIconKind::Hotkey => canvas.blit(&builtin.menu_hotkey, menu_x + 5, item_y + 4),
            MenuIconKind::Schedule => canvas.blit(&builtin.menu_schedule, menu_x + 5, item_y + 4),
            MenuIconKind::Skin => canvas.blit(&builtin.menu_skin, menu_x + 5, item_y + 4),
            MenuIconKind::Exit => canvas.blit(&builtin.menu_exit, menu_x + 5, item_y + 4),
            MenuIconKind::CtxAddFile => canvas.blit(&builtin.ctx_add_file, menu_x + 5, item_y + 4),
            MenuIconKind::CtxAddDir => canvas.blit(&builtin.ctx_add_dir, menu_x + 5, item_y + 4),
            MenuIconKind::CtxAddUrl => canvas.blit(&builtin.ctx_add_url, menu_x + 5, item_y + 4),
            MenuIconKind::CtxAddSys => canvas.blit(&builtin.ctx_add_sys, menu_x + 5, item_y + 4),
            MenuIconKind::CtxSearch => canvas.blit(&builtin.ctx_search, menu_x + 5, item_y + 4),
            MenuIconKind::CtxAppearance => {
                canvas.blit(&builtin.ctx_appearance, menu_x + 5, item_y + 4)
            }
            MenuIconKind::CtxPaste => canvas.blit(&builtin.ctx_paste, menu_x + 5, item_y + 4),
            MenuIconKind::CtxClearCol => {
                canvas.blit(&builtin.ctx_clear_col, menu_x + 5, item_y + 4)
            }
            MenuIconKind::CtxDelCol => canvas.blit(&builtin.ctx_del_col, menu_x + 5, item_y + 4),
            MenuIconKind::ItemDelete => canvas.blit(&builtin.im_delete, menu_x + 5, item_y + 4),
            MenuIconKind::ItemCut => canvas.blit(&builtin.im_cut, menu_x + 5, item_y + 4),
            MenuIconKind::ItemCopy => canvas.blit(&builtin.im_copy, menu_x + 5, item_y + 4),
            MenuIconKind::ItemSchedule => canvas.blit(&builtin.im_schedule, menu_x + 5, item_y + 4),
            MenuIconKind::ItemMoveTop => canvas.blit(&builtin.im_move_top, menu_x + 5, item_y + 4),
            MenuIconKind::ItemMoveBottom => {
                canvas.blit(&builtin.im_move_bottom, menu_x + 5, item_y + 4)
            }
            MenuIconKind::ItemEdit => canvas.blit(&builtin.im_edit, menu_x + 5, item_y + 4),
            MenuIconKind::ItemProps => canvas.blit(&builtin.im_props, menu_x + 5, item_y + 4),
            MenuIconKind::None => {}
        }

        let text_color = item.custom_text_color.unwrap_or(RgbColor::new(0, 0, 0));
        texts.push(TextDrawCmd {
            text: item.label.clone(),
            x: menu_x + 31,
            y: item_y,
            w: (content_w - 35).max(20),
            h: 23,
            color: text_color,
            bold: false,
            align: TextAlign::Left,
            mnemonic: true,
            multiline: false,
            font_slot: None,
        });

        if item.has_separator_below && local_idx + 1 < visible_rows {
            let sep_y = menu_y + 1 + ((local_idx + 1) as i32) * 24;
            canvas.draw_hline(menu_x + 25, menu_x + content_w - 2, sep_y, mc.separator_color);
        }
    }

    // 绘制右侧垂直滚动条
    if has_sb {
        let sb_x = menu_x + menu_w - scrollbar_width;
        let sb_y = menu_y + 1;
        let sb_w = scrollbar_width - 1;
        let sb_h = menu_h - 2;
        if scrollbar_width == SKIN_MENU_SCROLLBAR_WIDTH {
            draw_flat_scrollbar(canvas, sb_x, sb_y, scrollbar_width, sb_h,
                visible_rows, total_rows, start_row);
            return;
        }
        canvas.fill_rect(sb_x, sb_y, sb_w, sb_h, RgbColor::new(0xF0, 0xF0, 0xF0));
        canvas.draw_vline(sb_x, sb_y, sb_y + sb_h - 1, mc.separator_color);

        // 上箭头按钮
        canvas.fill_rect(sb_x + 1, sb_y, sb_w - 1, 14, RgbColor::new(0xE1, 0xE1, 0xE1));
        canvas.stroke_3d_rect(
            sb_x + 1,
            sb_y,
            sb_w - 1,
            14,
            RgbColor::new(255, 255, 255),
            RgbColor::new(0xA0, 0xA0, 0xA0),
        );
        let arr_c = RgbColor::new(0x33, 0x33, 0x33);
        let center = sb_x + scrollbar_width / 2;
        let radius = ((sb_w - 2) / 2).min(3);
        for row in 0..=radius {
            canvas.draw_hline(center - row, center + row, sb_y + 5 + row, arr_c);
        }

        // 下箭头按钮
        let dn_y = sb_y + sb_h - 14;
        canvas.fill_rect(sb_x + 1, dn_y, sb_w - 1, 14, RgbColor::new(0xE1, 0xE1, 0xE1));
        canvas.stroke_3d_rect(
            sb_x + 1,
            dn_y,
            sb_w - 1,
            14,
            RgbColor::new(255, 255, 255),
            RgbColor::new(0xA0, 0xA0, 0xA0),
        );
        for row in 0..=radius {
            canvas.draw_hline(center - (radius - row), center + (radius - row), dn_y + 5 + row, arr_c);
        }

        // 滑块
        let track_y = sb_y + 14;
        let track_h = (sb_h - 28).max(16);
        let thumb_h = ((track_h as usize * visible_rows) / total_rows.max(1)).clamp(18, track_h as usize) as i32;
        let travel = (track_h - thumb_h).max(0);
        let thumb_y = if max_scroll > 0 {
            track_y + ((travel as usize * start_row) / max_scroll) as i32
        } else {
            track_y
        };
        canvas.fill_rect(sb_x + 1, thumb_y, sb_w - 1, thumb_h, RgbColor::new(0xCD, 0xCD, 0xCD));
        canvas.stroke_3d_rect(
            sb_x + 1,
            thumb_y,
            sb_w - 1,
            thumb_h,
            RgbColor::new(255, 255, 255),
            RgbColor::new(0x88, 0x88, 0x88),
        );
    }
}

pub(crate) fn flat_scrollbar_metrics(height: i32, visible: usize, total: usize, scroll: usize) -> (i32, i32, i32) {
    let track = (height - 28).max(1);
    let thumb = ((track as usize * visible) / total.max(1))
        .clamp(24.min(track as usize), track as usize) as i32;
    let travel = track - thumb;
    let max_scroll = total.saturating_sub(visible);
    (14 + (travel as usize * scroll.min(max_scroll) / max_scroll.max(1)) as i32, thumb, travel)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_flat_scrollbar(canvas: &mut Canvas, x: i32, y: i32, width: i32, height: i32,
    visible: usize, total: usize, scroll: usize) {
    if height < 30 { return; }
    canvas.fill_rect(x, y, width - 1, height, RgbColor::new(245, 245, 245));
    let center = x + width / 2;
    let arrow = RgbColor::new(110, 110, 110);
    for row in 0..3 {
        canvas.draw_hline(center - row, center + row, y + 5 + row, arrow);
        canvas.draw_hline(center - row, center + row, y + height - 6 - row, arrow);
    }
    let (offset, thumb_h, _) = flat_scrollbar_metrics(height, visible, total, scroll);
    let thumb = RgbColor::new(168, 168, 168);
    canvas.fill_rect(x + 2, y + offset + 2, width - 5, (thumb_h - 4).max(1), thumb);
    canvas.fill_rect(x + 3, y + offset + 1, width - 7, (thumb_h - 2).max(1), thumb);
}

/// 构建右上角皮肤快捷菜单的菜单项。
fn skin_quick_menu_items(skins: &[SkinEntry], active_skin_index: usize) -> Vec<PopupMenuItem<usize>> {
    let mut items: Vec<PopupMenuItem<usize>> = Vec::new();
    for (i, skin) in skins.iter().enumerate() {
        let is_active = i == active_skin_index;
        items.push(PopupMenuItem {
            action: i,
            label: skin.name.clone(),
            icon: MenuIconKind::Checkbox(is_active),
            custom_text_color: if is_active {
                Some(RgbColor::new(0x00, 0x00, 0xFF))
            } else {
                None
            },
            has_separator_below: i + 1 == skins.len(),
        });
    }
    items.push(PopupMenuItem {
        action: skins.len(),
        label: "透明度(&T)...".to_string(),
        icon: MenuIconKind::None,
        custom_text_color: Some(RgbColor::new(0x0E, 0x19, 0x6A)),
        has_separator_below: true,
    });
    items.push(PopupMenuItem {
        action: skins.len() + 1,
        label: "皮肤管理器(&S)...".to_string(),
        icon: MenuIconKind::Skin,
        custom_text_color: Some(RgbColor::new(0xFF, 0x00, 0x00)),
        has_separator_below: false,
    });

    items
}

/// 独立弹窗公共状态上下文（完全隔离于主窗体，防止任何底层内容遮挡）
pub struct ModalRenderContext<'a> {
    pub modal: &'a ActiveModal,
    pub skin_cfg: &'a SkinConfig,
    pub skin_assets: &'a SkinAssets,
    pub builtin_icons: &'a BuiltinIcons,
    pub categories: &'a [CategoryTab],
    pub active_category: usize,
    pub skins: &'a [SkinEntry],
    pub selected_skin_index: usize,
    pub hover_target: HitTarget,
}

/// 渲染独立居中弹窗到其专属 `Canvas`
pub fn render_modal_window_frame(
    ctx: &ModalRenderContext<'_>,
    canvas: &mut Canvas,
) -> Vec<TextDrawCmd> {
    let (dw, dh) = ctx.modal.window_size();
    if dw <= 0 || dh <= 0 {
        return Vec::new();
    }
    canvas.resize(dw, dh);
    let mut texts = Vec::with_capacity(48);

    match ctx.modal {
        ActiveModal::SkinManager => render_skin_manager(ctx, dw, dh, canvas, &mut texts),
        ActiveModal::CategoryManager {
            selected_idx,
            renaming,
            rename_buf,
        } => render_category_manager(
            ctx,
            dw,
            dh,
            canvas,
            &mut texts,
            *selected_idx,
            *renaming,
            rename_buf,
        ),
        ActiveModal::ColumnSort {
            selected_idx,
            dragging_idx,
        } => render_column_sort_dialog(
            ctx,
            dw,
            dh,
            canvas,
            &mut texts,
            *selected_idx,
            *dragging_idx,
        ),
        ActiveModal::HotkeyManager(_) => {}
        ActiveModal::EditShortcut(ed) => {
            render_edit_shortcut_dialog(ctx, dw, dh, canvas, &mut texts, ed)
        }
        ActiveModal::About => render_about_dialog(ctx, dw, dh, canvas, &mut texts),
        ActiveModal::None | ActiveModal::Search(_) | ActiveModal::Advanced(_) => {}
    }

    texts
}

/// 绘制经典 Windows 弹窗标题栏与外框
fn draw_classic_dialog_frame(
    canvas: &mut Canvas,
    texts: &mut Vec<TextDrawCmd>,
    dw: i32,
    dh: i32,
    title: &str,
    close_hover: bool,
) {
    canvas.fill_rect(0, 0, dw, dh, RgbColor::new(0xF0, 0xF0, 0xF0));
    canvas.stroke_rect(0, 0, dw, dh, RgbColor::new(0x45, 0x68, 0x8E));

    // 渐变感 Aero/经典蓝灰标题栏
    for y in 1..24 {
        let t = (y - 1) as f32 / 22.0;
        let r = (0x99 as f32 + (0xB9 - 0x99) as f32 * t) as u8;
        let g = (0xB4 as f32 + (0xD1 - 0xB4) as f32 * t) as u8;
        let b = (0xD1 as f32 + (0xEA - 0xD1) as f32 * t) as u8;
        canvas.draw_hline(1, dw - 2, y, RgbColor::new(r, g, b));
    }
    canvas.draw_hline(1, dw - 2, 24, RgbColor::new(0x8E, 0xA8, 0xC3));

    texts.push(TextDrawCmd {
        text: title.to_string(),
        x: 8,
        y: 1,
        w: dw - 44,
        h: 22,
        color: RgbColor::new(0x10, 0x20, 0x35),
        bold: false,
        align: TextAlign::Left,
        mnemonic: false,
        multiline: false,
        font_slot: None,
    });

    // 右上角经典关闭按钮 [X]
    let bx = dw - 32;
    let by = 3;
    canvas.fill_rect(
        bx,
        by,
        26,
        18,
        if close_hover {
            RgbColor::new(0xE0, 0x43, 0x43)
        } else {
            RgbColor::new(0xC9, 0x6A, 0x5B)
        },
    );
    canvas.stroke_rect(bx, by, 26, 18, RgbColor::new(0x55, 0x25, 0x25));
    texts.push(TextDrawCmd {
        text: "×".to_string(),
        x: bx,
        y: by - 1,
        w: 26,
        h: 18,
        color: RgbColor::new(255, 255, 255),
        bold: true,
        align: TextAlign::Center,
        mnemonic: false,
        multiline: false,
        font_slot: None,
    });
}

/// 绘制经典按钮
fn draw_classic_button(
    canvas: &mut Canvas,
    texts: &mut Vec<TextDrawCmd>,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    label: &str,
    hover: bool,
) {
    canvas.fill_rect(
        x,
        y,
        w,
        h,
        if hover {
            RgbColor::new(0xE5, 0xF1, 0xFB)
        } else {
            RgbColor::new(0xE8, 0xE8, 0xE8)
        },
    );
    canvas.stroke_3d_rect(
        x,
        y,
        w,
        h,
        RgbColor::new(255, 255, 255),
        RgbColor::new(0x70, 0x70, 0x70),
    );
    texts.push(TextDrawCmd {
        text: label.to_string(),
        x,
        y,
        w,
        h,
        color: RgbColor::new(0, 0, 0),
        bold: false,
        align: TextAlign::Center,
        mnemonic: false,
        multiline: false,
        font_slot: None,
    });
}

/// 渲染【关于快速启动】窗口（1:1 复刻截图 3：`522x429`）
fn render_about_dialog(
    ctx: &ModalRenderContext<'_>,
    dw: i32,
    dh: i32,
    canvas: &mut Canvas,
    texts: &mut Vec<TextDrawCmd>,
) {
    draw_classic_dialog_frame(
        canvas,
        texts,
        dw,
        dh,
        "关于快速启动",
        ctx.hover_target == HitTarget::AboutClose,
    );

    // 1. 左侧橙黄渐变横幅 (x=1, y=24, 137x360)
    canvas.blit(&ctx.builtin_icons.about_banner, 1, 24);

    // 2. 右侧版本与版权文本
    texts.push(TextDrawCmd {
        text: "版本号: 5.1 Build 2012.8.20".to_string(),
        x: 152,
        y: 38,
        w: 240,
        h: 16,
        color: RgbColor::new(0, 0, 0),
        bold: false,
        align: TextAlign::Left,
        mnemonic: false,
        multiline: false,
        font_slot: None,
    });
    texts.push(TextDrawCmd {
        text: "藏鲸阁软件工作室 版权所有(C)".to_string(),
        x: 152,
        y: 62,
        w: 240,
        h: 16,
        color: RgbColor::new(0, 0, 0),
        bold: false,
        align: TextAlign::Left,
        mnemonic: false,
        multiline: false,
        font_slot: None,
    });

    // 3. 分组框【联系我们】
    canvas.stroke_3d_rect(
        152,
        100,
        352,
        92,
        RgbColor::new(0xA0, 0xA0, 0xA0),
        RgbColor::new(255, 255, 255),
    );
    canvas.fill_rect(162, 92, 60, 16, RgbColor::new(0xF0, 0xF0, 0xF0));
    texts.push(TextDrawCmd {
        text: "联系我们".to_string(),
        x: 166,
        y: 92,
        w: 56,
        h: 16,
        color: RgbColor::new(0, 0, 0),
        bold: false,
        align: TextAlign::Left,
        mnemonic: false,
        multiline: false,
        font_slot: None,
    });

    let links = [
        "主  页: http://www.makaix.com",
        "网  盘: http://www.makaix.com",
        "微信群: http://www.makaix.com",
    ];
    for (idx, &link_txt) in links.iter().enumerate() {
        let ly = 114 + (idx as i32) * 24;
        texts.push(TextDrawCmd {
            text: link_txt.to_string(),
            x: 184,
            y: ly,
            w: 300,
            h: 18,
            color: if ctx.hover_target == HitTarget::AboutLink(idx) {
                RgbColor::new(0xCC, 0x00, 0x00)
            } else {
                RgbColor::new(0x00, 0x00, 0xFF)
            },
            bold: false,
            align: TextAlign::Left,
            mnemonic: false,
            multiline: false,
            font_slot: None,
        });
    }

    // 4. 【软件介绍:】多行白底文本框 (带模拟滚动条)
    texts.push(TextDrawCmd {
        text: "软件介绍:".to_string(),
        x: 152,
        y: 204,
        w: 100,
        h: 16,
        color: RgbColor::new(0, 0, 0),
        bold: false,
        align: TextAlign::Left,
        mnemonic: false,
        multiline: false,
        font_slot: None,
    });

    let bx = 152;
    let by = 223;
    let bw = 352;
    let bh = 153;
    canvas.fill_rect(bx, by, bw, bh, RgbColor::new(255, 255, 255));
    canvas.stroke_3d_rect(
        bx,
        by,
        bw,
        bh,
        RgbColor::new(0xA0, 0xA0, 0xA0),
        RgbColor::new(255, 255, 255),
    );
    // 右侧模拟垂直滚动条
    canvas.fill_rect(bx + bw - 16, by + 2, 14, bh - 4, RgbColor::new(0xF0, 0xF0, 0xF0));
    canvas.fill_rect(bx + bw - 14, by + 18, 10, 68, RgbColor::new(0xB0, 0xB0, 0xB0));

    let intro = "    欢迎使用快速启动。快速启动是一款完全免费的软件，不存在任何功能或时间限制。它具有以下激动人心的特性：\r\n\r\n\
        1) 超方便管理软件、文件夹、网址、以及强大的系统功能。\r\n\
        2) 无数量限制的热键设定，一键执行任何需要的功能。\r\n\
        3) 漂亮的OICQ式界面，强大的换肤功能(拥有众多皮肤)。\r\n\
        4) 低于2MB的内存消耗，零CPU占用，不增加系统负担。\r\n\
        5) 邮件检测、收集面板、天气预报等实用功能/插件。\r\n\r\n\
        \x20   如果您喜欢快速启动，请捐助我们；或介绍给您的朋友、或写文章投稿于报刊网站；另外，欢迎反馈任何的建议（留言簿或QQ）";
    texts.push(TextDrawCmd {
        text: intro.to_string(),
        x: bx + 4,
        y: by + 3,
        w: bw - 22,
        h: bh - 6,
        color: RgbColor::new(0, 0, 0),
        bold: false,
        align: TextAlign::Left,
        mnemonic: false,
        multiline: true,
        font_slot: None,
    });

    // 5. 底部分隔线、红色链接与检查升级/关闭按钮
    canvas.draw_hline(1, dw - 2, 383, RgbColor::new(0xA0, 0xA0, 0xA0));
    canvas.draw_hline(1, dw - 2, 384, RgbColor::new(255, 255, 255));

    texts.push(TextDrawCmd {
        text: "点击这里,支持快速启动的发展".to_string(),
        x: 15,
        y: 400,
        w: 220,
        h: 18,
        color: RgbColor::new(0xFF, 0x00, 0x00),
        bold: false,
        align: TextAlign::Left,
        mnemonic: false,
        multiline: false,
        font_slot: None,
    });

    draw_classic_button(
        canvas,
        texts,
        432,
        398,
        72,
        22,
        "关闭",
        ctx.hover_target == HitTarget::AboutClose,
    );
}

/// 渲染【类别管理】独立窗口
#[allow(clippy::too_many_arguments)]
fn render_category_manager(
    ctx: &ModalRenderContext<'_>,
    dw: i32,
    dh: i32,
    canvas: &mut Canvas,
    texts: &mut Vec<TextDrawCmd>,
    selected_idx: usize,
    renaming: bool,
    rename_buf: &str,
) {
    draw_classic_dialog_frame(
        canvas,
        texts,
        dw,
        dh,
        "类别管理",
        ctx.hover_target == HitTarget::CatMgrClose,
    );

    let btn_defs = [
        ("新建", HitTarget::CatMgrBtnNew),
        ("删除", HitTarget::CatMgrBtnDelete),
        ("改名", HitTarget::CatMgrBtnRename),
        ("加密", HitTarget::CatMgrBtnEncrypt),
        ("上移", HitTarget::CatMgrBtnMoveUp),
        ("下移", HitTarget::CatMgrBtnMoveDown),
    ];
    let btn_w = 58;
    for (i, (label, target)) in btn_defs.iter().enumerate() {
        let bx = 10 + (i as i32) * (btn_w + 2);
        draw_classic_button(
            canvas,
            texts,
            bx,
            32,
            btn_w,
            24,
            label,
            ctx.hover_target == *target,
        );
    }

    let list_x = 10;
    let list_y = 64;
    let list_w = dw - 20;
    let list_h = dh - 106;
    canvas.fill_rect(list_x, list_y, list_w, list_h, RgbColor::new(255, 255, 255));
    canvas.stroke_3d_rect(
        list_x,
        list_y,
        list_w,
        list_h,
        RgbColor::new(0x80, 0x80, 0x80),
        RgbColor::new(255, 255, 255),
    );

    for (i, cat) in ctx.categories.iter().enumerate() {
        let ry = list_y + 4 + (i as i32) * 22;
        let is_sel = i == selected_idx;
        if is_sel {
            canvas.fill_rect(
                list_x + 2,
                ry,
                list_w - 4,
                20,
                RgbColor::new(0xD1, 0xEA, 0xF2),
            );
        }
        let ico = ctx.builtin_icons.category_icon(&cat.icon_name);
        canvas.blit(ico, list_x + 6, ry + 2);

        let display_name = if is_sel && renaming {
            format!("{}|", rename_buf)
        } else if cat.encrypted {
            format!("{} [已加密]", cat.name)
        } else {
            cat.name.clone()
        };
        texts.push(TextDrawCmd {
            text: display_name,
            x: list_x + 26,
            y: ry,
            w: list_w - 32,
            h: 20,
            color: RgbColor::new(0, 0, 0),
            bold: is_sel,
            align: TextAlign::Left,
            mnemonic: false,
            multiline: false,
            font_slot: None,
        });
    }

    let bot_y = dh - 32;
    texts.push(TextDrawCmd {
        text: "修改图标:".to_string(),
        x: 10,
        y: bot_y,
        w: 60,
        h: 22,
        color: RgbColor::new(0, 0, 0),
        bold: false,
        align: TextAlign::Left,
        mnemonic: false,
        multiline: false,
        font_slot: None,
    });

    if let Some(sel_cat) = ctx.categories.get(selected_idx) {
        let ico = ctx.builtin_icons.category_icon(&sel_cat.icon_name);
        canvas.blit(ico, 72, bot_y + 3);
    }

    draw_classic_button(
        canvas,
        texts,
        96,
        bot_y,
        64,
        22,
        "增加图标",
        ctx.hover_target == HitTarget::CatMgrBtnChangeIcon,
    );
    draw_classic_button(canvas, texts, 166, bot_y, 64, 22, "管理图标", false);
    draw_classic_button(
        canvas,
        texts,
        dw - 76,
        bot_y,
        64,
        22,
        "关闭",
        ctx.hover_target == HitTarget::CatMgrClose,
    );
}

/// 渲染【栏目排序】独立窗口
fn render_column_sort_dialog(
    ctx: &ModalRenderContext<'_>,
    dw: i32,
    dh: i32,
    canvas: &mut Canvas,
    texts: &mut Vec<TextDrawCmd>,
    selected_idx: usize,
    dragging_idx: Option<usize>,
) {
    draw_classic_dialog_frame(
        canvas,
        texts,
        dw,
        dh,
        "栏目排序",
        ctx.hover_target == HitTarget::ColSortClose,
    );

    texts.push(TextDrawCmd {
        text: "请通过拖放以交换栏目位置：".to_string(),
        x: 12,
        y: 34,
        w: dw - 24,
        h: 18,
        color: RgbColor::new(0, 0, 0),
        bold: false,
        align: TextAlign::Left,
        mnemonic: false,
        multiline: false,
        font_slot: None,
    });

    let list_x = 12;
    let list_y = 56;
    let list_w = dw - 24;
    let list_h = dh - 98;
    canvas.fill_rect(list_x, list_y, list_w, list_h, RgbColor::new(255, 255, 255));
    canvas.stroke_3d_rect(
        list_x,
        list_y,
        list_w,
        list_h,
        RgbColor::new(0x80, 0x80, 0x80),
        RgbColor::new(255, 255, 255),
    );

    if let Some(cat) = ctx.categories.get(ctx.active_category) {
        for (i, grp) in cat.groups.iter().enumerate() {
            let ry = list_y + 4 + (i as i32) * 24;
            let is_sel = i == selected_idx || dragging_idx == Some(i);
            let is_hov = ctx.hover_target == HitTarget::ColSortRow(i);
            if is_sel || is_hov {
                canvas.fill_rect(
                    list_x + 2,
                    ry,
                    list_w - 4,
                    22,
                    RgbColor::new(0xD1, 0xEA, 0xF2),
                );
                if dragging_idx.is_some() && is_hov {
                    canvas.stroke_rect(
                        list_x + 2,
                        ry,
                        list_w - 4,
                        22,
                        RgbColor::new(0x00, 0x55, 0xCC),
                    );
                }
            }
            canvas.stroke_rect(list_x + 6, ry + 4, 14, 14, RgbColor::new(0x4A, 0x70, 0x9C));
            canvas.fill_rect(list_x + 8, ry + 6, 4, 10, RgbColor::new(0x65, 0x88, 0xBA));

            texts.push(TextDrawCmd {
                text: format!("({})-= {} =-", i + 1, grp.title),
                x: list_x + 26,
                y: ry + 1,
                w: list_w - 32,
                h: 20,
                color: RgbColor::new(0, 0, 0),
                bold: is_sel,
                align: TextAlign::Left,
                mnemonic: false,
                multiline: false,
                font_slot: None,
            });
        }
    }

    draw_classic_button(
        canvas,
        texts,
        dw - 76,
        dh - 32,
        64,
        22,
        "关闭",
        ctx.hover_target == HitTarget::ColSortClose,
    );
}

/// 渲染【修改快捷方式】独立窗口（1:1 复刻截图 6、7：`371x458`，含【常规】与【备注】页签）
fn render_edit_shortcut_dialog(
    ctx: &ModalRenderContext<'_>,
    dw: i32,
    dh: i32,
    canvas: &mut Canvas,
    texts: &mut Vec<TextDrawCmd>,
    ed: &EditShortcutDialogState,
) {
    draw_classic_dialog_frame(
        canvas,
        texts,
        dw,
        dh,
        if ed.is_new {
            "新建快捷方式"
        } else {
            "修改快捷方式"
        },
        ctx.hover_target == HitTarget::EditScCancel,
    );

    // 顶部 Tab 页签：【常规】 | 【备注】
    let tab0_active = ed.active_tab == 0;
    let tab1_active = ed.active_tab == 1;

    canvas.fill_rect(
        16,
        if tab0_active { 35 } else { 37 },
        48,
        if tab0_active { 23 } else { 20 },
        if tab0_active {
            RgbColor::new(0xF0, 0xF0, 0xF0)
        } else {
            RgbColor::new(0xE4, 0xE4, 0xE4)
        },
    );
    canvas.stroke_3d_rect(
        16,
        if tab0_active { 35 } else { 37 },
        48,
        if tab0_active { 23 } else { 20 },
        RgbColor::new(255, 255, 255),
        RgbColor::new(0x80, 0x80, 0x80),
    );
    texts.push(TextDrawCmd {
        text: "常规".to_string(),
        x: 16,
        y: 36,
        w: 48,
        h: 20,
        color: RgbColor::new(0, 0, 0),
        bold: tab0_active,
        align: TextAlign::Center,
        mnemonic: false,
        multiline: false,
        font_slot: None,
    });

    canvas.fill_rect(
        65,
        if tab1_active { 35 } else { 37 },
        48,
        if tab1_active { 23 } else { 20 },
        if tab1_active {
            RgbColor::new(0xF0, 0xF0, 0xF0)
        } else {
            RgbColor::new(0xE4, 0xE4, 0xE4)
        },
    );
    canvas.stroke_3d_rect(
        65,
        if tab1_active { 35 } else { 37 },
        48,
        if tab1_active { 23 } else { 20 },
        RgbColor::new(255, 255, 255),
        RgbColor::new(0x80, 0x80, 0x80),
    );
    texts.push(TextDrawCmd {
        text: "备注".to_string(),
        x: 65,
        y: 36,
        w: 48,
        h: 20,
        color: RgbColor::new(0, 0, 0),
        bold: tab1_active,
        align: TextAlign::Center,
        mnemonic: false,
        multiline: false,
        font_slot: None,
    });

    // 主体分组外框
    canvas.stroke_3d_rect(
        16,
        57,
        dw - 32,
        dh - 102,
        RgbColor::new(255, 255, 255),
        RgbColor::new(0x80, 0x80, 0x80),
    );

    if ed.active_tab == 1 {
        // 【备注】页签（1:1 复刻截图 7）
        let rx = 28;
        let ry = 74;
        let rw = dw - 56;
        let rh = dh - 134;
        canvas.fill_rect(rx, ry, rw, rh, RgbColor::new(255, 255, 255));
        canvas.stroke_3d_rect(
            rx,
            ry,
            rw,
            rh,
            RgbColor::new(0x80, 0x80, 0x80),
            RgbColor::new(255, 255, 255),
        );
        // 右侧模拟垂直滚动条
        canvas.fill_rect(rx + rw - 16, ry + 2, 14, rh - 4, RgbColor::new(0xF0, 0xF0, 0xF0));
        canvas.draw_hline(rx + rw - 16, rx + rw - 2, ry + 16, RgbColor::new(0xC0, 0xC0, 0xC0));
        canvas.draw_hline(
            rx + rw - 16,
            rx + rw - 2,
            ry + rh - 16,
            RgbColor::new(0xC0, 0xC0, 0xC0),
        );

        let remark_display = if ed.active_field == EditShortcutField::Remark {
            format!("{}|", ed.remark)
        } else {
            ed.remark.clone()
        };
        texts.push(TextDrawCmd {
            text: remark_display,
            x: rx + 6,
            y: ry + 6,
            w: rw - 26,
            h: rh - 12,
            color: RgbColor::new(0, 0, 0),
            bold: false,
            align: TextAlign::Left,
            mnemonic: false,
            multiline: true,
            font_slot: None,
        });
    } else {
        // 【常规】页签（1:1 复刻截图 6）
        let preview_ico = ed
            .pending_icon_16
            .as_ref()
            .or_else(|| ctx.builtin_icons.app_icons.get(&ed.icon_key));
        if let Some(ico) = preview_ico {
            canvas.blit_stretch(ico, 32, 74, 32, 32);
        } else {
            canvas.blit(&ctx.builtin_icons.folder_large, 32, 74);
        }

        texts.push(TextDrawCmd {
            text: "标题:".to_string(),
            x: 78,
            y: 80,
            w: 36,
            h: 22,
            color: RgbColor::new(0, 0, 0),
            bold: false,
            align: TextAlign::Left,
            mnemonic: false,
            multiline: false,
            font_slot: None,
        });
        canvas.fill_rect(116, 80, dw - 146, 22, RgbColor::new(255, 255, 255));
        canvas.stroke_3d_rect(
            116,
            80,
            dw - 146,
            22,
            RgbColor::new(0x80, 0x80, 0x80),
            RgbColor::new(255, 255, 255),
        );
        texts.push(TextDrawCmd {
            text: if ed.active_field == EditShortcutField::Title {
                format!("{}|", ed.title)
            } else {
                ed.title.clone()
            },
            x: 120,
            y: 81,
            w: dw - 154,
            h: 20,
            color: RgbColor::new(0, 0, 0),
            bold: false,
            align: TextAlign::Left,
            mnemonic: false,
            multiline: false,
            font_slot: None,
        });

        canvas.draw_hline(26, dw - 26, 118, RgbColor::new(0xC0, 0xC0, 0xC0));

        texts.push(TextDrawCmd {
            text: "链 接:".to_string(),
            x: 26,
            y: 126,
            w: 42,
            h: 22,
            color: RgbColor::new(0, 0, 0),
            bold: false,
            align: TextAlign::Left,
            mnemonic: false,
            multiline: false,
            font_slot: None,
        });
        draw_classic_button(canvas, texts, 146, 124, 42, 22, "转换", false);
        draw_classic_button(canvas, texts, 192, 124, 50, 22, "添加批", false);
        canvas.blit(&ctx.builtin_icons.ctx_add_file, 248, 127);
        canvas.blit(&ctx.builtin_icons.ctx_add_dir, 268, 127);
        canvas.blit(&ctx.builtin_icons.ctx_add_sys, 288, 127);

        let draw_row_input = |canvas: &mut Canvas,
                              texts: &mut Vec<TextDrawCmd>,
                              label: &str,
                              ry: i32,
                              val: &str,
                              field: EditShortcutField| {
            if !label.is_empty() {
                texts.push(TextDrawCmd {
                    text: label.to_string(),
                    x: 26,
                    y: ry,
                    w: 42,
                    h: 22,
                    color: RgbColor::new(0, 0, 0),
                    bold: false,
                    align: TextAlign::Left,
                    mnemonic: false,
                    multiline: false,
                    font_slot: None,
                });
            }
            let iw = dw - 110;
            canvas.fill_rect(76, ry, iw, 22, RgbColor::new(255, 255, 255));
            canvas.stroke_3d_rect(
                76,
                ry,
                iw,
                22,
                RgbColor::new(0x80, 0x80, 0x80),
                RgbColor::new(255, 255, 255),
            );
            texts.push(TextDrawCmd {
                text: if ed.active_field == field {
                    format!("{}|", val)
                } else {
                    val.to_string()
                },
                x: 80,
                y: ry + 1,
                w: iw - 8,
                h: 20,
                color: RgbColor::new(0, 0, 0),
                bold: false,
                align: TextAlign::Left,
                mnemonic: false,
                multiline: false,
                font_slot: None,
            });
        };

        draw_row_input(
            canvas,
            texts,
            "",
            152,
            &ed.target_path,
            EditShortcutField::TargetPath,
        );
        draw_row_input(
            canvas,
            texts,
            "图 标:",
            184,
            &ed.icon_key,
            EditShortcutField::IconKey,
        );

        // 右侧小图标选择按钮（点击可选取程序或图片作为图标）
        let pick_btn_hov = ctx.hover_target == HitTarget::EditScPickIcon;
        canvas.fill_rect(
            dw - 54,
            184,
            22,
            22,
            if pick_btn_hov {
                RgbColor::new(0xE5, 0xF1, 0xFB)
            } else {
                RgbColor::new(0xE8, 0xE8, 0xE8)
            },
        );
        canvas.stroke_3d_rect(
            dw - 54,
            184,
            22,
            22,
            RgbColor::new(255, 255, 255),
            RgbColor::new(0x80, 0x80, 0x80),
        );
        if let Some(ico) = ed
            .pending_icon_16
            .as_ref()
            .or_else(|| ctx.builtin_icons.app_icons.get(&ed.icon_key))
        {
            canvas.blit_stretch(ico, dw - 51, 187, 16, 16);
        } else {
            canvas.blit(&ctx.builtin_icons.ctx_appearance, dw - 51, 187);
        }

        texts.push(TextDrawCmd {
            text: "颜 色:".to_string(),
            x: 26,
            y: 214,
            w: 42,
            h: 20,
            color: RgbColor::new(0, 0, 0),
            bold: false,
            align: TextAlign::Left,
            mnemonic: false,
            multiline: false,
            font_slot: None,
        });
        draw_classic_button(canvas,texts,76,214,dw-110,22,"▼",ctx.hover_target==HitTarget::EditScPickColor);
        canvas.fill_rect(79,217,dw-135,16,if ed.use_default_color{RgbColor::new(0,0,0)}else{ed.text_color});

        draw_row_input(canvas, texts, "参 数:", 266, &ed.args, EditShortcutField::Args);
        draw_row_input(
            canvas,
            texts,
            "起 始:",
            298,
            &ed.work_dir,
            EditShortcutField::WorkDir,
        );
        draw_row_input(
            canvas,
            texts,
            "热 键:",
            330,
            &ed.hotkey,
            EditShortcutField::Hotkey,
        );

        canvas.draw_hline(26, dw - 26, 360, RgbColor::new(0xC0, 0xC0, 0xC0));

        let chk_prefix = if ed.minimize_on_run { "☑" } else { "☐" };
        texts.push(TextDrawCmd {
            text: format!("{} 运行后最小化快速启动", chk_prefix),
            x: 28,
            y: 368,
            w: 200,
            h: 20,
            color: RgbColor::new(0, 0, 0),
            bold: false,
            align: TextAlign::Left,
            mnemonic: false,
            multiline: false,
            font_slot: None,
        });
    }

    draw_classic_button(
        canvas,
        texts,
        dw - 162,
        dh - 34,
        70,
        24,
        "确定",
        ctx.hover_target == HitTarget::EditScOk,
    );
    draw_classic_button(
        canvas,
        texts,
        dw - 84,
        dh - 34,
        70,
        24,
        "取消",
        ctx.hover_target == HitTarget::EditScCancel,
    );
}

/// 渲染【皮肤管理器】独立窗口
fn render_skin_manager(
    ctx: &ModalRenderContext<'_>,
    dw: i32,
    dh: i32,
    canvas: &mut Canvas,
    texts: &mut Vec<TextDrawCmd>,
) {
    let mc = &ctx.skin_cfg.menu_colors;
    draw_classic_dialog_frame(
        canvas,
        texts,
        dw,
        dh,
        &format!("皮肤管理器 (共 {} 款皮肤)", ctx.skins.len()),
        ctx.hover_target == HitTarget::SkinMgrClose,
    );

    let list_x = 10;
    let list_y = 36;
    let list_w = 140;
    let list_h = dh - 82;
    canvas.fill_rect(list_x, list_y, list_w, list_h, RgbColor::new(255, 255, 255));
    canvas.stroke_rect(list_x, list_y, list_w, list_h, mc.border_color);

    for (i, skin) in ctx.skins.iter().enumerate() {
        let iy = list_y + 2 + (i as i32) * 24;
        if iy + 22 > list_y + list_h {
            break;
        }
        let is_sel = i == ctx.selected_skin_index;
        let is_hov = ctx.hover_target == HitTarget::SkinMgrSelect(i);
        if is_sel || is_hov {
            canvas.fill_rect(list_x + 2, iy, list_w - 4, 22, mc.item_hover_bg_color);
            canvas.stroke_rect(list_x + 2, iy, list_w - 4, 22, mc.item_border_color);
        }
        texts.push(TextDrawCmd {
            text: skin.name.clone(),
            x: list_x + 6,
            y: iy,
            w: list_w - 12,
            h: 22,
            color: RgbColor::new(0, 0, 0),
            bold: is_sel,
            align: TextAlign::Left,
            mnemonic: false,
            multiline: false,
            font_slot: None,
        });
    }

    let prev_x = list_x + list_w + 10;
    let prev_y = list_y;
    let prev_w = dw - (list_w + 30);
    let prev_h = list_h - 46;
    canvas.fill_rect(prev_x, prev_y, prev_w, prev_h, RgbColor::new(255, 255, 255));
    canvas.stroke_rect(prev_x, prev_y, prev_w, prev_h, mc.border_color);

    if let Some(thumb) = &ctx.skin_assets.preview_thumbnail {
        canvas.blit_stretch(thumb, prev_x + 2, prev_y + 2, prev_w - 4, prev_h - 4);
    }

    if let Some(sel_skin) = ctx.skins.get(ctx.selected_skin_index) {
        let info_lines: Vec<&str> = sel_skin.author_info.lines().collect();
        for (l_idx, line) in info_lines.iter().take(2).enumerate() {
            texts.push(TextDrawCmd {
                text: (*line).to_string(),
                x: prev_x,
                y: prev_y + prev_h + 6 + (l_idx as i32) * 18,
                w: prev_w,
                h: 16,
                color: RgbColor::new(0x18, 0x48, 0x70),
                bold: false,
                align: TextAlign::Left,
                mnemonic: false,
                multiline: false,
                font_slot: None,
            });
        }
    }

    draw_classic_button(
        canvas,
        texts,
        dw - 160,
        dh - 34,
        72,
        24,
        "应用皮肤",
        ctx.hover_target == HitTarget::SkinMgrApply,
    );
    draw_classic_button(
        canvas,
        texts,
        dw - 80,
        dh - 34,
        68,
        24,
        "关闭",
        ctx.hover_target == HitTarget::SkinMgrClose,
    );
}
