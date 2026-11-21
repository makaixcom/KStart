#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
//! 快速启动 5 (KStart 5) 1:1 像素级复刻版主程序
//! 支持：
//! - 主窗体四条边和四个角拖动调整大小 (`WM_NCHITTEST` + `WM_SIZE`) 及四角异形透明裁剪
//! - 快捷方式区矩形框选 (`marquee_rect`) 与多选高亮 (`#0A246A`)、内部拖拽排序、外部文件拖入
//! - 独立屏幕居中原生弹窗 (`KStartModalDialogClass`)，彻底隔离渲染，杜绝背景遮挡
//! - 四大自绘弹出菜单（主菜单、皮肤快捷菜单、内容区空白右键菜单、快捷方式图标 12 项右键菜单）

mod advanced;
mod advanced_dialog;
mod advanced_runtime;
mod advanced_prompt;
mod advanced_shell;
mod advanced_style;
mod advanced_system;
mod advanced_web_icon;
mod advanced_window;
mod hotkey_dialog;
mod hotkeys;
mod targets;
mod config;
mod model;
mod native_dialogs;
mod popup;
mod renderer;
mod search;
mod search_dialog;
mod skin_assets;
mod skin_parser;

use advanced::Key as Setting;
use chrono::{Datelike, Local, Weekday};
use config::{scan_skins, HotkeyConfig, SkinEntry, UserConfig};
use model::{
    build_context_menu_items, build_item_menu_items, build_popup_menu_items, AppDataStore,
    CategoryTab, ContextMenuAction, ItemMenuAction, MenuAction, ShortcutGroup, ShortcutItem,
};
use renderer::{
    compute_visible_shortcut_rects, hit_test_main, hit_test_modal,
    normalize_rect, rects_intersect, render_main_frame, render_modal_window_frame, ActiveModal,
    ActivePopupMenu, EditShortcutDialogState, EditShortcutField, HitTarget,
    ModalRenderContext, PopupTextClip, RenderState, TextAlign, TextDrawCmd,
    STANDARD_PALETTE,
};
use search::{matching_shortcuts, split_run_input, SearchDialogState, SearchMode, SearchResultRow};
use skin_assets::{extract_file_icon_16, load_custom_icon_16, BuiltinIcons, Canvas, SkinAssets};
use skin_parser::{RgbColor, SkinConfig, ToolButtonKind};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::env;
use std::ffi::OsStr;
use std::fs;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::ptr;

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, CombineRgn, CreateCompatibleDC, CreateDIBSection, CreateFontW,
    CreateRectRgn, CreateSolidBrush, DeleteDC, DeleteObject, DrawTextW, EndPaint, ExcludeClipRect,
    FillRect, GetSysColorBrush, InvalidateRect, RedrawWindow, ScreenToClient, SelectClipRgn,
    SelectObject, SetBkMode,
    SetTextColor, SetWindowRgn, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, COLOR_BTNFACE,
    DEFAULT_QUALITY, DIB_RGB_COLORS, DT_CENTER, DT_END_ELLIPSIS, DT_LEFT, DT_NOPREFIX,
    DT_SINGLELINE, DT_VCENTER, DT_WORDBREAK, FW_BOLD, FW_NORMAL, GB2312_CHARSET, HBRUSH, HDC,
    HFONT, HRGN, OUT_DEFAULT_PRECIS, PAINTSTRUCT, RDW_INVALIDATE, RDW_NOERASE, RDW_UPDATENOW,
    RGN_DIFF, SRCCOPY, TRANSPARENT,
};
use windows_sys::Win32::System::Com::CoTaskMemFree;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Controls::Dialogs::{
    GetOpenFileNameW, OFN_FILEMUSTEXIST, OFN_NOCHANGEDIR, OFN_PATHMUSTEXIST, OPENFILENAMEW,
};
use windows_sys::Win32::UI::Controls::{
    InitCommonControlsEx, DRAWITEMSTRUCT, ICC_LISTVIEW_CLASSES, ICC_TAB_CLASSES, INITCOMMONCONTROLSEX, NMHDR,
    TCN_SELCHANGE,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    ReleaseCapture, SetCapture, SetFocus, VK_BACK, VK_DOWN, VK_ESCAPE, VK_F1, VK_F12, VK_RETURN, VK_UP,
};
use windows_sys::Win32::UI::Shell::{
    DragAcceptFiles, DragFinish, DragQueryFileW, DragQueryPoint, ShellExecuteW, BIF_NEWDIALOGSTYLE,
    BIF_RETURNONLYFSDIRS, HDROP,
};

#[repr(C)]
struct BrowseInfoW {
    hwnd_owner: HWND,
    pidl_root: *const core::ffi::c_void,
    psz_display_name: *mut u16,
    lpsz_title: *const u16,
    ul_flags: u32,
    lpfn: Option<unsafe extern "system" fn(HWND, u32, LPARAM, LPARAM) -> i32>,
    l_param: LPARAM,
    i_image: i32,
}

#[link(name = "shell32")]
extern "system" {
    fn SHBrowseForFolderW(lpbi: *const BrowseInfoW) -> *mut core::ffi::c_void;
    fn SHGetPathFromIDListW(pidl: *const core::ffi::c_void, psz_path: *mut u16) -> i32;
}
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AdjustWindowRectEx, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
    GetClientRect, GetDlgItem, GetForegroundWindow, GetMessageW, GetSystemMetrics, GetWindowLongPtrW, IsDialogMessageW,
    IsIconic, IsWindowVisible, LoadCursorW, MessageBoxW, PostQuitMessage, RegisterClassW, SendMessageW, SetCursor,
    SetForegroundWindow, SetTimer, SetWindowLongPtrW, SetWindowPos, ShowWindow,
    TranslateMessage, CS_HREDRAW, CS_VREDRAW, GWLP_USERDATA, GWL_STYLE, HTBOTTOM, HTBOTTOMLEFT, HTBOTTOMRIGHT, HTCAPTION,
    HTCLIENT, HTLEFT, HTRIGHT, HTTOP, HTTOPLEFT, HTTOPRIGHT, HWND_NOTOPMOST, HWND_TOPMOST,
    IDCANCEL, IDC_ARROW, IDC_HAND, IDC_IBEAM, IDC_SIZENESW, IDC_SIZENS, IDC_SIZENWSE, IDC_SIZEWE,
    IDOK, LBN_DBLCLK, LBN_SELCHANGE, MB_ICONINFORMATION, MB_OK, MINMAXINFO, MSG, SIZE_MINIMIZED,
    SM_CXSCREEN, SM_CYSCREEN, SWP_NOCOPYBITS, SWP_NOMOVE, SWP_NOSIZE, SW_HIDE, SW_MINIMIZE,
    SW_RESTORE, SW_SHOW, SW_SHOWNORMAL, WINDOWPOS, WM_CHAR, WM_CLOSE, WM_COMMAND, WM_CREATE, WM_HOTKEY,
    WM_ACTIVATE, WM_CTLCOLORSTATIC, WM_DESTROY, WM_DRAWITEM, WM_DROPFILES, WM_ERASEBKGND, WM_EXITSIZEMOVE,
    WM_GETMINMAXINFO, WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_MOVE, WM_NCACTIVATE,
    WM_NCCALCSIZE, WM_NCHITTEST, WM_NCLBUTTONDOWN, WM_NCPAINT, WM_NOTIFY, WM_PAINT, WM_RBUTTONUP,
    CS_DBLCLKS, WM_LBUTTONDBLCLK, WM_MOUSEWHEEL, WM_SETCURSOR, WM_SIZE, WM_TIMER, WM_WINDOWPOSCHANGING, WNDCLASSW, WS_CAPTION,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_MINIMIZEBOX, WS_POPUP, WS_SYSMENU, WS_THICKFRAME,
    WS_VISIBLE,
};

const MODAL_CLASS_NAME: &str = "KStartModalDialogClass";
const NATIVE_MODAL_CLASS_NAME: &str = "KStartNativeDialogClass";

/// 将 Rust 字符串转换为以 `\0` 结尾的宽字符数组
fn to_wide(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
}

/// 弹出 Windows 原生文件选择对话框 (`GetOpenFileNameW`)
fn pick_file_dialog(owner: HWND, title: &str, filter_pairs: &[(&str, &str)]) -> Option<PathBuf> {
    let mut filter_w: Vec<u16> = Vec::new();
    for (desc, pat) in filter_pairs {
        filter_w.extend(OsStr::new(desc).encode_wide());
        filter_w.push(0);
        filter_w.extend(OsStr::new(pat).encode_wide());
        filter_w.push(0);
    }
    filter_w.push(0);

    let title_w = to_wide(title);
    let mut file_buf = vec![0u16; 1024];

    unsafe {
        let mut ofn: OPENFILENAMEW = std::mem::zeroed();
        ofn.lStructSize = std::mem::size_of::<OPENFILENAMEW>() as u32;
        ofn.hwndOwner = owner;
        ofn.lpstrFilter = filter_w.as_ptr();
        ofn.lpstrFile = file_buf.as_mut_ptr();
        ofn.nMaxFile = file_buf.len() as u32;
        ofn.lpstrTitle = title_w.as_ptr();
        ofn.Flags = OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR;

        if GetOpenFileNameW(&mut ofn) != 0 {
            let end = file_buf.iter().position(|&c| c == 0).unwrap_or(0);
            if end > 0 {
                return Some(PathBuf::from(String::from_utf16_lossy(&file_buf[..end])));
            }
        }
    }
    None
}

/// 弹出 Windows 原生文件夹选择对话框 (`SHBrowseForFolderW`)
fn pick_folder_dialog(owner: HWND, title: &str) -> Option<PathBuf> {
    let title_w = to_wide(title);
    unsafe {
        let bi = BrowseInfoW {
            hwnd_owner: owner,
            pidl_root: ptr::null(),
            psz_display_name: ptr::null_mut(),
            lpsz_title: title_w.as_ptr(),
            ul_flags: BIF_RETURNONLYFSDIRS | BIF_NEWDIALOGSTYLE,
            lpfn: None,
            l_param: 0,
            i_image: 0,
        };
        let pidl = SHBrowseForFolderW(&bi);
        if pidl.is_null() {
            return None;
        }
        let mut path_buf = vec![0u16; 1024];
        let ok = SHGetPathFromIDListW(pidl, path_buf.as_mut_ptr());
        CoTaskMemFree(pidl);
        if ok != 0 {
            let end = path_buf.iter().position(|&c| c == 0).unwrap_or(0);
            if end > 0 {
                return Some(PathBuf::from(String::from_utf16_lossy(&path_buf[..end])));
            }
        }
    }
    None
}

/// 格式化当前本地日期时间为 `11月18日 星期三 09:02`
fn format_current_time(settings: &advanced::AdvancedSettings) -> String {
    let now = Local::now();
    let weekday_str = match now.weekday() {
        Weekday::Mon => "星期一",
        Weekday::Tue => "星期二",
        Weekday::Wed => "星期三",
        Weekday::Thu => "星期四",
        Weekday::Fri => "星期五",
        Weekday::Sat => "星期六",
        Weekday::Sun => "星期日",
    };
    advanced::format_time(settings.text(Setting::TimeFormat), now, weekday_str)
}

fn load_list_background(base: &Path, settings: &advanced::AdvancedSettings) -> Option<skin_assets::RgbaSurface> {
    let path = settings.text(Setting::ListBackground);
    if path.is_empty() { return None; }
    skin_assets::RgbaSurface::load_file(&base.join(path))
}

/// 应用程序全局运行状态
struct AppState {
    base_dir: PathBuf,
    main_hwnd: HWND,
    modal_hwnd: HWND,
    /// 当前打开的独立弹窗是否为 Windows 原生控件弹窗
    modal_is_native: bool,
    user_cfg: UserConfig,
    advanced_window: advanced_window::Runtime,
    list_background: Option<skin_assets::RgbaSurface>,
    hotkey_cfg: HotkeyConfig,
    data_store: AppDataStore,
    skins: Vec<SkinEntry>,
    active_skin_index: usize,
    selected_skin_index: usize,
    skin_cfg: SkinConfig,
    skin_assets: SkinAssets,
    builtin_icons: BuiltinIcons,
    categories: Vec<CategoryTab>,
    active_category: usize,
    hover_target: HitTarget,
    down_target: HitTarget,
    modal_hover_target: HitTarget,
    press_pos: (i32, i32),
    dragging_shortcut: Option<(usize, i32, i32)>,
    marquee_rect: Option<(i32, i32, i32, i32)>,
    selected_shortcuts: HashSet<usize>,
    clipboard_items: Vec<ShortcutItem>,
    time_string: String,
    search_query: String,
    search_focused: bool,
    popup_menu: ActivePopupMenu,
    popup_window: Option<popup::PopupWindow>,
    /// 当前弹出菜单的垂直滚动起始行索引
    menu_scroll: usize,
    modal: ActiveModal,
    canvas: Canvas,
    modal_canvas: Canvas,
    /// 记录最近一次已成功应用到主窗体的异形区域尺寸，避免 `SetWindowRgn` 同步消息重入递归
    applied_rgn_size: (i32, i32),
}

impl AppState {
    /// 初始化应用程序状态
    fn new(base_dir: PathBuf, initial_menu: ActivePopupMenu) -> Result<Self, String> {
        let mut user_cfg = UserConfig::load(&base_dir);
        if user_cfg.advanced.enabled(Setting::StartWithoutList) { user_cfg.enable_list = false; }
        let hotkey_cfg = HotkeyConfig::load(&base_dir);
        let data_store = AppDataStore::load(&base_dir);

        let skins = scan_skins(&base_dir);
        if skins.is_empty() {
            return Err(format!(
                "在 {:?}/skin 目录下未找到任何包含 Setup.ini 的皮肤文件夹！",
                base_dir
            ));
        }

        let active_skin_index = user_cfg.resolve_active_skin_index(&skins).unwrap_or(0);
        let active_entry = &skins[active_skin_index];

        let skin_cfg = SkinConfig::load_from_ini(&active_entry.setup_ini_path)?;
        let skin_assets = SkinAssets::load(&active_entry.dir_path, &skin_cfg)?;
        let builtin_icons = BuiltinIcons::load(&base_dir);

        let mut categories =
            data_store.build_runtime_tree(&base_dir, &builtin_icons.app_icons, &HashMap::new());
        advanced::sort_by_usage(&mut categories, &user_cfg.advanced);

        let win_w = user_cfg.window_width;
        let win_h = user_cfg.window_height;

        let list_background = load_list_background(&base_dir, &user_cfg.advanced);
        Ok(Self {
            base_dir,
            main_hwnd: 0,
            modal_hwnd: 0,
            modal_is_native: false,
            time_string: format_current_time(&user_cfg.advanced),
            list_background,
            user_cfg,
            advanced_window: advanced_window::Runtime::default(),
            hotkey_cfg,
            data_store,
            skins,
            active_skin_index,
            selected_skin_index: active_skin_index,
            skin_cfg,
            skin_assets,
            builtin_icons,
            categories,
            active_category: 0,
            hover_target: HitTarget::None,
            down_target: HitTarget::None,
            modal_hover_target: HitTarget::None,
            press_pos: (0, 0),
            dragging_shortcut: None,
            marquee_rect: None,
            selected_shortcuts: HashSet::new(),
            clipboard_items: Vec::new(),
            search_query: String::new(),
            search_focused: false,
            popup_menu: initial_menu,
            popup_window: None,
            menu_scroll: 0,
            modal: ActiveModal::None,
            canvas: Canvas::new(win_w, win_h),
            modal_canvas: Canvas::new(400, 400),
            applied_rgn_size: (0, 0),
        })
    }

    fn reload_advanced_assets(&mut self) {
        self.list_background = load_list_background(&self.base_dir, &self.user_cfg.advanced);
        self.time_string = format_current_time(&self.user_cfg.advanced);
        advanced::sort_by_usage(&mut self.categories, &self.user_cfg.advanced);
    }

    fn persist_data(&mut self) {
        self.data_store
            .sync_from_runtime_tree(&self.categories, &self.base_dir);
    }

    fn switch_skin(&mut self, index: usize) -> Option<HRGN> {
        if let Some(entry) = self.skins.get(index) {
            if let Ok(new_cfg) = SkinConfig::load_from_ini(&entry.setup_ini_path) {
                if let Ok(new_assets) = SkinAssets::load(&entry.dir_path, &new_cfg) {
                    self.active_skin_index = index;
                    self.selected_skin_index = index;
                    self.skin_cfg = new_cfg;
                    self.skin_assets = new_assets;
                    self.user_cfg.configured_skin = Some(entry.name.clone());
                    self.user_cfg.save(&self.base_dir);
                    self.applied_rgn_size =
                        (self.user_cfg.window_width, self.user_cfg.window_height);

                    return Some(build_window_corner_region(
                        self.user_cfg.window_width,
                        self.user_cfg.window_height,
                        &self.skin_cfg,
                        &self.skin_assets,
                    ));
                }
            }
        }
        None
    }

    /// 构造指定快捷方式的【修改快捷方式】弹窗数据
    fn make_edit_shortcut_modal(
        &self,
        cat_idx: usize,
        grp_idx: usize,
        item_idx: usize,
    ) -> Option<ActiveModal> {
        let cat = self.categories.get(cat_idx)?;
        let grp = cat.groups.get(grp_idx)?;
        let item = grp.items.get(item_idx)?;
        let hk = self
            .hotkey_cfg
            .find_key_by_shortcut_id(&item.id)
            .unwrap_or("")
            .to_string();
        Some(ActiveModal::EditShortcut(EditShortcutDialogState {
            target_kind_hint: Some(item.target_kind),
            is_new: false,
            shortcut_id: item.id.clone(),
            cat_idx,
            grp_idx,
            item_idx,
            active_tab: 0,
            title: item.name.clone(),
            target_path: item.target_path.clone(),
            icon_key: item.icon_key.clone(),
            pending_icon_16: item.custom_icon_16.clone(),
            text_color: item.text_color,
            args: item.args.clone(),
            work_dir: item.work_dir.clone(),
            hotkey: hk,
            remark: item.remark.clone(),
            minimize_on_run: item.minimize_on_run,
            active_field: EditShortcutField::Title,
        }))
    }

    /// 构造【新建快捷方式】弹窗数据（尚未插入列表，只有点“确定”才真正写入）
    fn make_new_shortcut_modal(
        &self,
        cat_idx: usize,
        grp_idx: usize,
        title: String,
        target_path: String,
        work_dir: String,
        pending_icon_16: Option<skin_assets::RgbaSurface>,
        fallback_icon_key: &str,
    ) -> ActiveModal {
        let new_id = self.data_store.next_shortcut_id();
        let icon_key = if pending_icon_16.is_some() {
            new_id.clone()
        } else {
            fallback_icon_key.to_string()
        };
        ActiveModal::EditShortcut(EditShortcutDialogState {
            target_kind_hint: None,
            is_new: true,
            shortcut_id: new_id,
            cat_idx,
            grp_idx,
            item_idx: 0,
            active_tab: 0,
            title,
            target_path,
            icon_key,
            pending_icon_16,
            text_color: advanced::color(self.user_cfg.advanced.text(Setting::DefaultColor)).unwrap_or(RgbColor::new(0, 0, 0)),
            args: String::new(),
            work_dir,
            hotkey: String::new(),
            remark: String::new(),
            minimize_on_run: false,
            active_field: EditShortcutField::Title,
        })
    }

    fn add_dropped_file(&mut self, file: advanced_runtime::DroppedFile) {
        let Some(cat) = self.categories.get_mut(self.active_category) else {
            return;
        };
        if cat.groups.is_empty() {
            cat.groups.push(ShortcutGroup {
                id: format!("col_{}", self.data_store.columns.len() + 1),
                category_id: cat.id.clone(),
                title: "默认栏目".to_string(),
                encrypted: false,
                items: Vec::new(),
            });
        }
        let grp_idx = cat.active_group_index.min(cat.groups.len() - 1);
        let new_id = self.data_store.next_shortcut_id();
        let icon_key = if let Some(ico_surf) = file.icon {
            let user_icon_dir = self.base_dir.join("user").join("icons");
            let _ = fs::create_dir_all(&user_icon_dir);
            let png_path = user_icon_dir.join(format!("{}.png", new_id));
            let _ = ico_surf.save_png(&png_path);
            self.builtin_icons.app_icons.insert(new_id.clone(), ico_surf);
            new_id.clone()
        } else {
            "app_msedge".to_string()
        };

        let custom_icon_16 = self.builtin_icons.app_icons.get(&icon_key).cloned();
        let cat_id = cat.id.clone();
        let col_id = cat.groups[grp_idx].id.clone();

        cat.groups[grp_idx].items.push(ShortcutItem {
            id: new_id,
            category_id: cat_id,
            column_id: col_id,
            name: file.name,
            target_kind: targets::TargetKind::infer(&self.base_dir, &file.target),
            target_path: file.target,
            icon_key,
            args: file.args,
            work_dir: file.work_dir,
            remark: String::new(),
            text_color: advanced::color(self.user_cfg.advanced.text(Setting::DefaultColor)).unwrap_or(RgbColor::new(0, 0, 0)),
            minimize_on_run: false,
            custom_icon_16,
        });

        self.persist_data();
    }
}

thread_local! {
    static APP_STATE: RefCell<Option<AppState>> = const { RefCell::new(None) };
}

/// 判断该弹窗是否采用 Windows 原生控件实现
fn is_native_modal(modal: &ActiveModal) -> bool {
    matches!(
        modal,
        ActiveModal::SkinManager
            | ActiveModal::CategoryManager { .. }
            | ActiveModal::ColumnSort { .. }
            | ActiveModal::EditShortcut(_)
            | ActiveModal::Search(_)
            | ActiveModal::Advanced(_)
    )
}

/// 打开或更新独立屏幕居中弹窗窗口（五大管理弹窗采用原生 Win32 控件，关于弹窗保留经典自绘窗口）
fn show_centered_modal(modal: ActiveModal) {
    if let ActiveModal::HotkeyManager(shortcut) = &modal {
        close_modal_window();
        hotkey_dialog::open(shortcut.as_deref());
        return;
    }
    if matches!(modal, ActiveModal::None) {
        close_modal_window();
        return;
    }

    let use_native = is_native_modal(&modal);
    let is_search = matches!(modal, ActiveModal::Search(_));
    let mut existing_hwnd: HWND = 0;
    let mut owner_hwnd: HWND = 0;
    APP_STATE.with(|cell| {
        if let Ok(mut guard) = cell.try_borrow_mut() {
            if let Some(app) = guard.as_mut() {
                existing_hwnd = app.modal_hwnd;
                app.modal_hwnd = 0;
                app.modal = modal;
                app.modal_is_native = use_native;
                app.modal_hover_target = HitTarget::None;
                owner_hwnd = app.main_hwnd;
            }
        }
    });

    unsafe {
        if existing_hwnd != 0 {
            DestroyWindow(existing_hwnd);
        }

        let screen_w = GetSystemMetrics(SM_CXSCREEN);
        let screen_h = GetSystemMetrics(SM_CYSCREEN);
        let hinstance = GetModuleHandleW(ptr::null());

        let hwnd = if use_native {
            let mut cw = 400;
            let mut ch = 400;
            let mut title_str = "KStartDialog";
            APP_STATE.with(|cell| {
                if let Ok(guard) = cell.try_borrow() {
                    if let Some(app) = guard.as_ref() {
                        let info = native_dialogs::native_modal_client_info(&app.modal);
                        cw = info.0;
                        ch = info.1;
                        title_str = info.2;
                    }
                }
            });
            let style = WS_POPUP | WS_CAPTION | WS_SYSMENU | WS_VISIBLE;
            let ex_style = WS_EX_TOPMOST | WS_EX_TOOLWINDOW;
            let mut wr = RECT {
                left: 0,
                top: 0,
                right: cw,
                bottom: ch,
            };
            AdjustWindowRectEx(&mut wr, style, 0, ex_style);
            let win_w = wr.right - wr.left;
            let win_h = wr.bottom - wr.top;
            let x = ((screen_w - win_w) / 2).max(10);
            let y = ((screen_h - win_h) / 2).max(10);
            let cls = to_wide(if is_search {
                MODAL_CLASS_NAME
            } else {
                NATIVE_MODAL_CLASS_NAME
            });
            let title = to_wide(title_str);
            CreateWindowExW(
                ex_style,
                cls.as_ptr(),
                title.as_ptr(),
                style,
                x,
                y,
                win_w,
                win_h,
                owner_hwnd,
                0,
                hinstance,
                ptr::null(),
            )
        } else {
            let mut dw = 0;
            let mut dh = 0;
            APP_STATE.with(|cell| {
                if let Ok(guard) = cell.try_borrow() {
                    if let Some(app) = guard.as_ref() {
                        let sz = app.modal.window_size();
                        dw = sz.0;
                        dh = sz.1;
                    }
                }
            });
            let x = ((screen_w - dw) / 2).max(10);
            let y = ((screen_h - dh) / 2).max(10);
            let cls = to_wide(MODAL_CLASS_NAME);
            let title = to_wide("KStartDialog");
            CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
                cls.as_ptr(),
                title.as_ptr(),
                WS_POPUP | WS_VISIBLE,
                x,
                y,
                dw,
                dh,
                owner_hwnd,
                0,
                hinstance,
                ptr::null(),
            )
        };

        APP_STATE.with(|cell| {
            if let Ok(mut guard) = cell.try_borrow_mut() {
                if let Some(app) = guard.as_mut() {
                    app.modal_hwnd = hwnd;
                }
            }
        });
        if hwnd != 0 {
            ShowWindow(hwnd, SW_SHOWNORMAL);
            SetForegroundWindow(hwnd);
            if is_search {
                refresh_search_dialog(hwnd);
            }
            InvalidateRect(hwnd, ptr::null(), 0);
        }
    }
}

/// 关闭并销毁独立弹窗窗口
fn close_modal_window() {
    let mut target_hwnd: HWND = 0;
    let mut main_hwnd: HWND = 0;
    APP_STATE.with(|cell| {
        if let Ok(mut guard) = cell.try_borrow_mut() {
            if let Some(app) = guard.as_mut() {
                app.modal = ActiveModal::None;
                app.modal_is_native = false;
                app.modal_hover_target = HitTarget::None;
                target_hwnd = app.modal_hwnd;
                app.modal_hwnd = 0;
                main_hwnd = app.main_hwnd;
            }
        }
    });
    if target_hwnd != 0 {
        unsafe {
            DestroyWindow(target_hwnd);
        }
    }
    if main_hwnd != 0 {
        unsafe {
            InvalidateRect(main_hwnd, ptr::null(), 0);
        }
    }
}

/// 根据 `Setup.ini` 的 `CornerMask*` 范围构建原生 Win32 异形窗口裁剪区域 (`HRGN`)
/// 注意：纯计算 HRGN，不触发任何 Win32 窗口消息，可在持有 RefCell 借用时安全调用
fn build_window_corner_region(
    win_w: i32,
    win_h: i32,
    cfg: &SkinConfig,
    assets: &SkinAssets,
) -> HRGN {
    unsafe {
        let hrgn_main: HRGN = CreateRectRgn(0, 0, win_w, win_h);
        if hrgn_main == 0 {
            return 0;
        }

        let cm = &cfg.corner_mask;
        // 按水平连续透明像素段批量扣减区域，显著降低 GDI HRGN 调用开销
        let punch_transparent_pixels =
            |surf: &skin_assets::RgbaSurface,
             src_rx: u32,
             src_ry: u32,
             rw: u32,
             rh: u32,
             dst_offset_x: i32,
             dst_offset_y: i32| {
                let x_end = (src_rx + rw).min(surf.width);
                let y_end = (src_ry + rh).min(surf.height);
                for sy in src_ry..y_end {
                    let mut sx = src_rx;
                    while sx < x_end {
                        if surf.get_pixel(sx, sy)[3] == 0 {
                            let run_start = sx;
                            sx += 1;
                            while sx < x_end && surf.get_pixel(sx, sy)[3] == 0 {
                                sx += 1;
                            }
                            let wx0 = dst_offset_x + run_start as i32;
                            let wx1 = dst_offset_x + sx as i32;
                            let wy = dst_offset_y + sy as i32;
                            let hrgn_span = CreateRectRgn(wx0, wy, wx1, wy + 1);
                            if hrgn_span != 0 {
                                CombineRgn(hrgn_main, hrgn_main, hrgn_span, RGN_DIFF);
                                DeleteObject(hrgn_span as _);
                            }
                        } else {
                            sx += 1;
                        }
                    }
                }
            };

        punch_transparent_pixels(
            &assets.skin_top_left,
            0,
            0,
            cm.top_left_width,
            cm.top_left_height,
            0,
            0,
        );

        let tr_src_x = assets
            .skin_top_right
            .width
            .saturating_sub(cm.top_right_width);
        let tr_dst_x = win_w - assets.skin_top_right.width as i32;
        punch_transparent_pixels(
            &assets.skin_top_right,
            tr_src_x,
            0,
            cm.top_right_width,
            cm.top_right_height,
            tr_dst_x,
            0,
        );

        let bl_src_y = assets
            .skin_bottom_left
            .height
            .saturating_sub(cm.bottom_left_height);
        let bl_dst_y = win_h - assets.skin_bottom_left.height as i32;
        punch_transparent_pixels(
            &assets.skin_bottom_left,
            0,
            bl_src_y,
            cm.bottom_left_width,
            cm.bottom_left_height,
            0,
            bl_dst_y,
        );

        let br_src_x = assets
            .skin_bottom_right
            .width
            .saturating_sub(cm.bottom_right_width);
        let br_src_y = assets
            .skin_bottom_right
            .height
            .saturating_sub(cm.bottom_right_height);
        let br_dst_x = win_w - assets.skin_bottom_right.width as i32;
        let br_dst_y = win_h - assets.skin_bottom_right.height as i32;
        punch_transparent_pixels(
            &assets.skin_bottom_right,
            br_src_x,
            br_src_y,
            cm.bottom_right_width,
            cm.bottom_right_height,
            br_dst_x,
            br_dst_y,
        );

        hrgn_main
    }
}

/// 将创建好的区域句柄安全设置给窗口（必须在释放 APP_STATE 借用后调用，传 1 以通知 DWM 同步刷新窗口裁剪轮廓）
fn apply_hrgn_safely(hwnd: HWND, hrgn: HRGN) {
    if hwnd != 0 && hrgn != 0 {
        unsafe {
            SetWindowRgn(hwnd, hrgn, 1);
        }
    }
}

/// 通用 GDI DIBSection + 12px 宋体光栅化与呈现函数（供主窗体与独立弹窗共同复用）
fn blit_canvas_with_gdi_texts(
    canvas: &Canvas,
    texts: &[TextDrawCmd],
    popup_clip: Option<PopupTextClip>,
    target_hdc: HDC,
    capture_pixels: bool,
    settings_fonts: Option<&[advanced::FontSetting; 4]>,
) -> Option<Vec<u32>> {
    let w = canvas.width;
    let h = canvas.height;
    unsafe {
        let mem_dc = CreateCompatibleDC(target_hdc);
        if mem_dc == 0 {
            return None;
        }

        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                biSizeImage: 0,
                biXPelsPerMeter: 0,
                biYPelsPerMeter: 0,
                biClrUsed: 0,
                biClrImportant: 0,
            },
            bmiColors: [std::mem::zeroed()],
        };

        let mut bits_ptr: *mut core::ffi::c_void = ptr::null_mut();
        let hbm = CreateDIBSection(mem_dc, &bmi, DIB_RGB_COLORS, &mut bits_ptr, 0, 0);
        if hbm == 0 || bits_ptr.is_null() {
            DeleteDC(mem_dc);
            return None;
        }

        let old_bm = SelectObject(mem_dc, hbm as _);
        let pixel_count = (w * h) as usize;
        let dst_slice = std::slice::from_raw_parts_mut(bits_ptr as *mut u32, pixel_count);
        dst_slice.copy_from_slice(&canvas.buffer[..pixel_count]);

        let font_name = to_wide("宋体");
        let hfont_normal: HFONT = CreateFontW(
            -12,
            0,
            0,
            0,
            FW_NORMAL as i32,
            0,
            0,
            0,
            GB2312_CHARSET as u32,
            OUT_DEFAULT_PRECIS as u32,
            0,
            DEFAULT_QUALITY as u32,
            0,
            font_name.as_ptr(),
        );
        let hfont_bold: HFONT = CreateFontW(
            -12,
            0,
            0,
            0,
            FW_BOLD as i32,
            0,
            0,
            0,
            GB2312_CHARSET as u32,
            OUT_DEFAULT_PRECIS as u32,
            0,
            DEFAULT_QUALITY as u32,
            0,
            font_name.as_ptr(),
        );

        SetBkMode(mem_dc, TRANSPARENT as i32);
        let old_font = SelectObject(mem_dc, hfont_normal as _);
        let mut custom_fonts = [0; 4];
        if let Some(fonts) = settings_fonts {
            for (index, font) in fonts.iter().enumerate() {
                if font.face == "宋体" && font.points == 9 { continue; }
                let face = to_wide(&font.face);
                custom_fonts[index] = CreateFontW(
                    -(font.points.clamp(5, 48) * 96 / 72), 0, 0, 0, FW_NORMAL as i32,
                    0, 0, 0, GB2312_CHARSET as u32, OUT_DEFAULT_PRECIS as u32, 0,
                    DEFAULT_QUALITY as u32, 0, face.as_ptr(),
                );
            }
        }

        let draw_text = |cmd: &TextDrawCmd| {
            let font = cmd.font_slot.and_then(|index| custom_fonts.get(index).copied())
                .filter(|&font| font != 0);
            SelectObject(
                mem_dc,
                if let Some(font) = font {
                    font
                } else if cmd.bold {
                    hfont_bold as _
                } else {
                    hfont_normal as _
                },
            );
            SetTextColor(mem_dc, cmd.color.to_colorref());

            let mut rc = RECT {
                left: cmd.x,
                top: cmd.y,
                right: cmd.x + cmd.w,
                bottom: cmd.y + cmd.h,
            };
            let wide_text = to_wide(&cmd.text);
            let mut flags = if cmd.multiline {
                DT_WORDBREAK
            } else {
                DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS
            };
            flags |= match cmd.align {
                TextAlign::Left => DT_LEFT,
                TextAlign::Center => DT_CENTER,
            };
            if !cmd.mnemonic {
                flags |= DT_NOPREFIX;
            }
            DrawTextW(
                mem_dc,
                wide_text.as_ptr(),
                (wide_text.len().saturating_sub(1)) as i32,
                &mut rc,
                flags,
            );
        };

        if let Some(clip) = popup_clip {
            let (under_menu, menu) = texts.split_at(clip.first_menu_command);
            let (x, y, width, height) = clip.rect;
            ExcludeClipRect(mem_dc, x, y, x + width, y + height);
            under_menu.iter().for_each(draw_text);
            SelectClipRgn(mem_dc, 0);
            menu.iter().for_each(draw_text);
        } else {
            texts.iter().for_each(draw_text);
        }

        if target_hdc != 0 {
            BitBlt(target_hdc, 0, 0, w, h, mem_dc, 0, 0, SRCCOPY);
        }

        let captured = if capture_pixels {
            Some(dst_slice.to_vec())
        } else {
            None
        };

        SelectObject(mem_dc, old_font);
        for font in custom_fonts.into_iter().filter(|&font| font != 0) {
            DeleteObject(font);
        }
        SelectObject(mem_dc, old_bm);
        DeleteObject(hfont_normal as _);
        DeleteObject(hfont_bold as _);
        DeleteObject(hbm as _);
        DeleteDC(mem_dc);

        captured
    }
}

/// 将 `Canvas` 与捕获到的像素保存为 PNG 截图文件
fn save_bgra_to_png(w: u32, h: u32, bgra: Vec<u32>, out_path: &Path) -> Result<(), String> {
    let mut rgba_bytes = Vec::with_capacity((w * h * 4) as usize);
    for px in bgra {
        let b = (px & 0xFF) as u8;
        let g = ((px >> 8) & 0xFF) as u8;
        let r = ((px >> 16) & 0xFF) as u8;
        rgba_bytes.extend_from_slice(&[r, g, b, 255]);
    }
    let img_buf = image::RgbaImage::from_raw(w, h, rgba_bytes).ok_or("创建 RgbaImage 失败")?;
    img_buf
        .save(out_path)
        .map_err(|e| format!("保存截图失败: {}", e))
}

/// 启动快捷方式或打开网址
fn launch_target_url_or_path(target: &str, args: &str, work_dir: &str, hwnd: HWND) -> bool {
    let op = to_wide("open");
    let file = to_wide(target);
    let params = if args.is_empty() {
        Vec::new()
    } else {
        to_wide(args)
    };
    let dir = if work_dir.is_empty() {
        Vec::new()
    } else {
        to_wide(work_dir)
    };
    unsafe {
        ShellExecuteW(
            hwnd,
            op.as_ptr(),
            file.as_ptr(),
            if params.is_empty() {
                ptr::null()
            } else {
                params.as_ptr()
            },
            if dir.is_empty() {
                ptr::null()
            } else {
                dir.as_ptr()
            },
            SW_SHOWNORMAL as i32,
        ) > 32
    }
}

fn launch_typed_target(
    kind: targets::TargetKind, target: &str, args: &str, work_dir: &str, hwnd: HWND,
    base: &Path, settings: &advanced::AdvancedSettings,
) -> bool {
    use targets::TargetKind::*;
    let quote = |value: &str| format!("\"{}\"", value.replace('"', "%22"));
    match kind {
        Launcher => {
            unsafe {
                match target {
                    "/Search/" => {
                        let query = APP_STATE.with(|cell| cell.try_borrow().ok()
                            .and_then(|guard| guard.as_ref().map(|app| app.search_query.clone())))
                            .unwrap_or_default();
                        submit_search(hwnd, SearchMode::Shortcuts, &query);
                    }
                    "/AH/" if IsWindowVisible(hwnd) != 0 && IsIconic(hwnd) == 0
                        && GetForegroundWindow() == hwnd => {
                        advanced_window::hide_after_launch(hwnd, 2, false, false);
                    }
                    "/AH/" | "kstart:app-show" => {
                        ShowWindow(hwnd, SW_RESTORE);
                        SetForegroundWindow(hwnd);
                    }
                    "kstart:app-hide" => advanced_window::hide_after_launch(hwnd, 2, false, false),
                    "kstart:app-hotkeys" => show_centered_modal(ActiveModal::HotkeyManager(None)),
                    "kstart:app-advanced" => show_centered_modal(ActiveModal::Advanced(settings.clone())),
                    "kstart:app-exit" => request_main_close(hwnd, true),
                    _ => return false,
                }
            }
            true
        }
        System => {
            if let Some(name) = target.strip_prefix("kstart:") {
                advanced_system::execute(hwnd, name, settings);
                true
            } else {
                launch_target_url_or_path(target, args, work_dir, hwnd)
            }
        }
        Web => launch_configured_url(target, hwnd, settings),
        File | Folder => {
            if target.is_empty() { return false; }
            let resolved = targets::resolve_path(base, target);
            // 保留旧记录中仅填写程序名的 Shell 查找行为。
            let target = if kind == File && Path::new(target).file_name().is_some_and(|name| name == target)
                && !resolved.exists() { target.into() } else { resolved.to_string_lossy() };
            let folder_program = settings.text(Setting::FolderProgram);
            if kind == Folder && !folder_program.is_empty() {
                launch_target_url_or_path(folder_program, &quote(&target), "", hwnd)
            } else if kind == Folder && settings.enabled(Setting::ExplorerFolders) {
                launch_target_url_or_path("explorer.exe", &quote(&target), "", hwnd)
            } else {
                let dir = if work_dir.is_empty() { String::new() }
                    else { targets::resolve_path(base, work_dir).to_string_lossy().into_owned() };
                launch_target_url_or_path(&target, args, &dir, hwnd)
            }
        }
    }
}

fn launch_shortcut(item: &ShortcutItem, hwnd: HWND) {
    let settings = APP_STATE.with(|cell| {
        let guard = cell.try_borrow().ok()?;
        let app = guard.as_ref()?;
        (!app.user_cfg.startup_lock).then(|| (app.base_dir.clone(), app.user_cfg.advanced.clone()))
    });
    let Some((base, settings)) = settings else { return; };
    let target = &item.target_path;
    let is_url = item.target_kind == targets::TargetKind::Web;
    let result = launch_typed_target(item.target_kind, target, &item.args, &item.work_dir, hwnd, &base, &settings);
    if !result {
        if is_url && settings.number(Setting::Browser) == 1 {
            advanced_runtime::message(hwnd, "Internet Explorer 无法打开，请在高级设置中选择浏览器。");
        }
        return;
    }
    if matches!(item.target_kind, targets::TargetKind::System | targets::TargetKind::Launcher) { return; }
    if is_url && settings.enabled(Setting::WebIcons) && item.custom_icon_16.is_none() {
        advanced_web_icon::fetch_for_shortcut(base.clone(), target.clone(), settings.enabled(Setting::CacheWebIcons));
    }
    if settings.enabled(Setting::RankByUse) {
        let changed = APP_STATE.with(|cell| {
            let mut guard = cell.try_borrow_mut().ok()?;
            let app = guard.as_mut()?;
            let count = app.user_cfg.advanced.launch_counts.entry(item.id.clone()).or_default();
            *count = count.saturating_add(1);
            advanced::sort_by_usage(&mut app.categories, &app.user_cfg.advanced);
            app.selected_shortcuts.clear();
            Some((app.user_cfg.clone(), app.base_dir.clone()))
        });
        if let Some((settings, base)) = changed {
            settings.save(&base);
        }
    }
    if settings.enabled(Setting::Sound) {
        unsafe { windows_sys::Win32::System::Diagnostics::Debug::MessageBeep(MB_OK); }
    }
    if hwnd != 0 {
        unsafe {
            advanced_window::hide_after_launch(hwnd, settings.number(Setting::AfterLaunch),
                item.minimize_on_run, settings.enabled(Setting::AnimateMinimize));
        }
    }
}

fn launch_configured_url(url: &str, owner: HWND, settings: &advanced::AdvancedSettings) -> bool {
    let quoted = format!("\"{}\"", url.replace('"', "%22"));
    match settings.number(Setting::Browser) {
        1 => launch_target_url_or_path("iexplore.exe", &format!("-new {quoted}"), "", owner),
        2 if !settings.text(Setting::CustomBrowser).is_empty() => {
            launch_target_url_or_path(settings.text(Setting::CustomBrowser), &quoted, "", owner)
        }
        _ => launch_target_url_or_path(url, "", "", owner),
    }
}

unsafe fn request_main_close(hwnd: HWND, from_menu: bool) {
    let action = APP_STATE.with(|cell| {
        let guard = cell.try_borrow().ok()?;
        let app = guard.as_ref()?;
        let settings = &app.user_cfg.advanced;
        if app.user_cfg.startup_lock && settings.enabled(Setting::BlockLockedExit) {
            Some(0)
        } else if !from_menu && settings.enabled(Setting::CloseMinimizes) {
            Some(1)
        } else { Some(2) }
    });
    match action {
        Some(0) => advanced_runtime::message(hwnd, "请先解除锁定再退出 KStart。"),
        Some(1) => { ShowWindow(hwnd, SW_MINIMIZE); advanced_window::update_tray(hwnd); }
        Some(2) => { DestroyWindow(hwnd); }
        _ => {}
    }
}

fn shortcut_at(app: &AppState, index: usize) -> Option<&ShortcutItem> {
    let cat = app.categories.get(app.active_category)?;
    cat.groups.get(cat.active_group_index.min(cat.groups.len().saturating_sub(1)))?.items.get(index)
}

fn advanced_list_grid(app: &AppState) -> Option<renderer::ShortcutGrid> {
    renderer::shortcut_grid(&app.skin_cfg, app.user_cfg.window_width, app.user_cfg.window_height,
        app.categories.get(app.active_category)?, &app.user_cfg.advanced)
}

unsafe fn wheel_main(hwnd: HWND, delta: i16, ctrl: bool) -> bool {
    if delta == 0 { return false; }
    let handled = APP_STATE.with(|cell| {
        let Ok(mut guard) = cell.try_borrow_mut() else { return false; };
        let Some(app) = guard.as_mut() else { return false; };
        if app.popup_menu != ActivePopupMenu::None { return false; }
        if app.user_cfg.advanced.enabled(Setting::WheelChangesGroup) && !ctrl {
            if let Some(cat) = app.categories.get_mut(app.active_category) {
                if delta > 0 {
                    cat.active_group_index = cat.active_group_index.saturating_sub(1);
                } else {
                    cat.active_group_index = (cat.active_group_index + 1).min(cat.groups.len().saturating_sub(1));
                }
                app.selected_shortcuts.clear();
                app.advanced_window.shortcut_scroll = 0;
            }
        } else if let Some(grid) = advanced_list_grid(app) {
            let current = app.advanced_window.shortcut_scroll;
            app.advanced_window.shortcut_scroll = if delta > 0 { current.saturating_sub(3) }
                else { (current + 3).min(grid.max_scroll) };
        }
        true
    });
    if handled { InvalidateRect(hwnd, ptr::null(), 0); }
    handled
}

fn select_search_mode(app: &mut AppState, index: usize) {
    let Some(&(mode, _)) = SearchMode::OPTIONS.get(index) else {
        return;
    };
    if app.user_cfg.search_mode != mode {
        app.user_cfg.search_mode = mode;
        app.user_cfg.save(&app.base_dir);
    }
    app.popup_menu = ActivePopupMenu::None;
    app.hover_target = HitTarget::None;
    app.search_focused = true;
}

fn submit_search(hwnd: HWND, mode: SearchMode, query: &str) {
    let query = query.trim();
    if mode == SearchMode::Shortcuts {
        show_centered_modal(ActiveModal::Search(SearchDialogState::new(query.to_string())));
        return;
    }
    if query.is_empty() {
        return;
    }

    let opened = if let Some(url) = mode.web_url(query) {
        let settings = APP_STATE.with(|cell| {
            let guard = cell.try_borrow().ok()?;
            guard.as_ref().map(|app| app.user_cfg.advanced.clone())
        });
        launch_configured_url(&url, hwnd, &settings.unwrap_or_default())
    } else {
        let (target, args) = split_run_input(query);
        launch_target_url_or_path(target, args, "", hwnd)
    };
    if !opened {
        let title = to_wide("搜索框");
        let message = to_wide("无法打开目标。请检查输入内容和系统的默认打开方式。");
        unsafe {
            MessageBoxW(hwnd, message.as_ptr(), title.as_ptr(), MB_OK | MB_ICONINFORMATION);
        }
    }
}

unsafe fn refresh_search_dialog(hwnd: HWND) {
    let (query, scope, fuzzy) = search_dialog::criteria(hwnd);
    let rows = APP_STATE.with(|cell| {
        let mut guard = cell.try_borrow_mut().ok()?;
        let app = guard.as_mut()?;
        if app.modal_hwnd != hwnd {
            return None;
        }
        let ActiveModal::Search(dialog) = &mut app.modal else {
            return None;
        };
        dialog.results = matching_shortcuts(&app.categories, app.active_category, &query, scope, fuzzy)
            .into_iter()
            .map(|item| SearchResultRow {
                id: item.id.clone(),
                name: item.name.clone(),
                path: item.target_path.clone(),
                icon: item
                    .custom_icon_16
                    .as_ref()
                    .or_else(|| app.builtin_icons.app_icons.get(&item.icon_key))
                    .map(|icon| icon.resize_to(24, 24)),
            })
            .collect();
        Some(dialog.results.iter().map(|row| row.name.clone()).collect::<Vec<_>>())
    });
    if let Some(rows) = rows {
        search_dialog::render_results(hwnd, &rows);
    }
}

unsafe fn open_search_result(hwnd: HWND) {
    let Some(index) = search_dialog::selection(hwnd) else {
        return;
    };
    let result = APP_STATE.with(|cell| {
        let guard = cell.try_borrow().ok()?;
        let app = guard.as_ref()?;
        if app.modal_hwnd != hwnd {
            return None;
        }
        let ActiveModal::Search(dialog) = &app.modal else {
            return None;
        };
        let id = &dialog.results.get(index)?.id;
        let item = app
            .categories
            .iter()
            .flat_map(|category| &category.groups)
            .flat_map(|group| &group.items)
            .find(|item| item.id == *id)?;
        Some((app.main_hwnd, item.clone()))
    });
    if let Some((owner, item)) = result {
        close_modal_window();
        launch_shortcut(&item, owner);
    } else {
        refresh_search_dialog(hwnd);
    }
}

/// 在 IsDialogMessage 处理默认按钮前，为关键词和结果列表分派回车。
unsafe fn handle_search_dialog_key(hwnd: HWND, message: &MSG) -> bool {
    if message.message != WM_KEYDOWN {
        return false;
    }
    let source = message.hwnd;
    let query = GetDlgItem(hwnd, search_dialog::ID_QUERY);
    let results = GetDlgItem(hwnd, search_dialog::ID_RESULTS);
    if message.wParam as u16 == VK_ESCAPE && (source == query || source == results) {
        close_modal_window();
        return true;
    }
    if message.wParam as u16 != VK_RETURN {
        return false;
    }
    let submit = source == query || source == GetDlgItem(hwnd, search_dialog::ID_SUBMIT);
    let open = source == results;
    if message.lParam & (1 << 30) == 0 {
        if submit {
            refresh_search_dialog(hwnd);
        } else if open {
            open_search_result(hwnd);
        }
    }
    submit || open
}

/// 处理左下角主菜单项点击
fn main_menu_action(app: &AppState, item_index: usize) -> Option<MenuAction> {
    let items = build_popup_menu_items(
        app.user_cfg.always_on_top,
        app.user_cfg.pin_to_desktop,
        app.user_cfg.lock_move,
        app.user_cfg.enable_list,
        app.user_cfg.enable_hotkey,
        app.user_cfg.startup_lock,
    );
    items.get(item_index).map(|item| item.action)
}

fn handle_main_menu_action(app: &mut AppState, action: MenuAction) -> Option<ActiveModal> {
    match action {
        MenuAction::ToggleAlwaysOnTop => {
            app.user_cfg.always_on_top = !app.user_cfg.always_on_top;
            app.user_cfg.save(&app.base_dir);
            None
        }
        MenuAction::TogglePinToDesktop => {
            app.user_cfg.pin_to_desktop = !app.user_cfg.pin_to_desktop;
            app.user_cfg.save(&app.base_dir);
            None
        }
        MenuAction::ToggleLockMove => {
            app.user_cfg.lock_move = !app.user_cfg.lock_move;
            app.user_cfg.save(&app.base_dir);
            None
        }
        MenuAction::ToggleEnableList => {
            app.user_cfg.enable_list = !app.user_cfg.enable_list;
            app.user_cfg.save(&app.base_dir);
            None
        }
        MenuAction::ToggleEnableHotkey => {
            app.user_cfg.enable_hotkey = !app.user_cfg.enable_hotkey;
            hotkeys::changed(app.main_hwnd);
            app.user_cfg.save(&app.base_dir);
            None
        }
        MenuAction::OpenAdvancedOptions => {
            app.popup_menu = ActivePopupMenu::None;
            Some(ActiveModal::Advanced(app.user_cfg.advanced.clone()))
        }
        MenuAction::OpenHotkeyManager => {
            app.popup_menu = ActivePopupMenu::None;
            Some(ActiveModal::HotkeyManager(None))
        }
        MenuAction::OpenSkinManager => {
            app.popup_menu = ActivePopupMenu::None;
            Some(ActiveModal::SkinManager)
        }
        MenuAction::AboutApp | MenuAction::SupportAuthor => {
            app.popup_menu = ActivePopupMenu::None;
            Some(ActiveModal::About)
        }
        _ => {
            app.popup_menu = ActivePopupMenu::None;
            None
        }
    }
}

/// 延迟到 RefCell 借用释放后再弹出的系统原生对话框请求（彻底避免模态消息循环重入 Panic）
enum DeferredDialogRequest {
    None,
    ContextAddFile(usize, usize),
    ContextAddDir(usize, usize),
    ContextSystemAction(usize, usize),
    EditPickFile,
    EditPickDir,
    EditPickIcon,
    AboutCheckUpdate,
}

/// 处理内容区空白处右键菜单项点击（16 项）
/// 注意：添加文件/目录/超链接等操作不会提前插入列表，必须等弹窗点击【确定】才写入！
fn handle_context_menu_action(
    app: &mut AppState,
    item_index: usize,
) -> (Option<ActiveModal>, DeferredDialogRequest) {
    let items = build_context_menu_items();
    let Some(menu_item) = items.get(item_index) else {
        return (None, DeferredDialogRequest::None);
    };
    app.popup_menu = ActivePopupMenu::None;

    let cat_idx = app.active_category;
    let Some(grp_idx) = app
        .categories
        .get(cat_idx)
        .map(|c| c.active_group_index.min(c.groups.len().saturating_sub(1)))
    else {
        return (None, DeferredDialogRequest::None);
    };

    match menu_item.action {
        ContextMenuAction::AddFile => (None, DeferredDialogRequest::ContextAddFile(cat_idx, grp_idx)),
        ContextMenuAction::AddDirectory => {
            (None, DeferredDialogRequest::ContextAddDir(cat_idx, grp_idx))
        }
        ContextMenuAction::AddHyperlink => (
            Some(app.make_new_shortcut_modal(
                cat_idx,
                grp_idx,
                "新建网址".to_string(),
                "https://".to_string(),
                String::new(),
                None,
                "ctx_add_url",
            )),
            DeferredDialogRequest::None,
        ),
        ContextMenuAction::AddSystemFunc => (
            None, DeferredDialogRequest::ContextSystemAction(cat_idx, grp_idx),
        ),
        ContextMenuAction::AddAutoLogin | ContextMenuAction::CustomAddOrEdit => (
            Some(app.make_new_shortcut_modal(
                cat_idx,
                grp_idx,
                "新建快捷方式".to_string(),
                String::new(),
                String::new(),
                None,
                "app_notepadpp",
            )),
            DeferredDialogRequest::None,
        ),
        ContextMenuAction::SearchItem => (
            Some(ActiveModal::Search(SearchDialogState::new(app.search_query.clone()))),
            DeferredDialogRequest::None,
        ),
        ContextMenuAction::AppearanceSetting => {
            (Some(ActiveModal::SkinManager), DeferredDialogRequest::None)
        }
        ContextMenuAction::SortShortcuts => {
            if let Some(cat) = app.categories.get_mut(cat_idx) {
                if let Some(grp) = cat.groups.get_mut(grp_idx) {
                    grp.items.sort_by(|a, b| a.name.cmp(&b.name));
                    app.persist_data();
                }
            }
            (None, DeferredDialogRequest::None)
        }
        ContextMenuAction::PasteItem => {
            let pasted = app.clipboard_items.clone();
            if !pasted.is_empty() {
                if let Some(cat) = app.categories.get_mut(cat_idx) {
                    let cat_id = cat.id.clone();
                    if let Some(grp) = cat.groups.get_mut(grp_idx) {
                        let col_id = grp.id.clone();
                        for mut it in pasted {
                            it.id = app.data_store.next_shortcut_id();
                            it.category_id = cat_id.clone();
                            it.column_id = col_id.clone();
                            grp.items.push(it);
                        }
                        app.persist_data();
                    }
                }
            }
            (None, DeferredDialogRequest::None)
        }
        ContextMenuAction::ClearColumn => {
            if let Some(cat) = app.categories.get_mut(cat_idx) {
                if let Some(grp) = cat.groups.get_mut(grp_idx) {
                    grp.items.clear();
                    app.selected_shortcuts.clear();
                    app.persist_data();
                }
            }
            (None, DeferredDialogRequest::None)
        }
        ContextMenuAction::EncryptColumn => {
            if let Some(cat) = app.categories.get_mut(cat_idx) {
                if let Some(grp) = cat.groups.get_mut(grp_idx) {
                    grp.encrypted = !grp.encrypted;
                    app.persist_data();
                }
            }
            (None, DeferredDialogRequest::None)
        }
        ContextMenuAction::RenameColumnTitle => {
            if let Some(cat) = app.categories.get_mut(cat_idx) {
                let next_num = cat.groups.len() + 1;
                let cat_id = cat.id.clone();
                cat.groups.push(ShortcutGroup {
                    id: format!("col_{}", app.data_store.columns.len() + next_num),
                    category_id: cat_id,
                    title: format!("新栏目{}", next_num),
                    encrypted: false,
                    items: Vec::new(),
                });
                cat.active_group_index = cat.groups.len() - 1;
                app.persist_data();
            }
            (None, DeferredDialogRequest::None)
        }
        ContextMenuAction::DeleteColumn => {
            if let Some(cat) = app.categories.get_mut(cat_idx) {
                if cat.groups.len() > 1 {
                    cat.groups.remove(grp_idx);
                    cat.active_group_index =
                        cat.active_group_index.min(cat.groups.len().saturating_sub(1));
                    app.selected_shortcuts.clear();
                    app.persist_data();
                }
            }
            (None, DeferredDialogRequest::None)
        }
        _ => (None, DeferredDialogRequest::None),
    }
}

/// 处理对着快捷方式图标右键点击弹出的 12 项菜单动作（对应截图 2）
fn handle_item_menu_action(
    app: &mut AppState,
    item_index: usize,
    target_shortcut: usize,
    browse_location: &mut Option<String>,
) -> Option<ActiveModal> {
    let items = build_item_menu_items();
    let menu_item = items.get(item_index)?;
    app.popup_menu = ActivePopupMenu::None;

    let cat_idx = app.active_category;
    let grp_idx = app
        .categories
        .get(cat_idx)
        .map(|c| c.active_group_index.min(c.groups.len().saturating_sub(1)))?;

    // 若多选集合中包含多个图标，批量操作全部选中项；否则操作 target_shortcut
    let mut indices: Vec<usize> = if app.selected_shortcuts.len() > 1
        && app.selected_shortcuts.contains(&target_shortcut)
    {
        app.selected_shortcuts.iter().copied().collect()
    } else {
        vec![target_shortcut]
    };
    indices.sort_unstable();

    match menu_item.action {
        ItemMenuAction::Delete => {
            if let Some(cat) = app.categories.get_mut(cat_idx) {
                if let Some(grp) = cat.groups.get_mut(grp_idx) {
                    for &idx in indices.iter().rev() {
                        if idx < grp.items.len() {
                            grp.items.remove(idx);
                        }
                    }
                    app.selected_shortcuts.clear();
                    app.persist_data();
                }
            }
            None
        }
        ItemMenuAction::Cut => {
            app.clipboard_items.clear();
            if let Some(cat) = app.categories.get_mut(cat_idx) {
                if let Some(grp) = cat.groups.get_mut(grp_idx) {
                    for &idx in indices.iter().rev() {
                        if idx < grp.items.len() {
                            app.clipboard_items.push(grp.items.remove(idx));
                        }
                    }
                    app.clipboard_items.reverse();
                    app.selected_shortcuts.clear();
                    app.persist_data();
                }
            }
            None
        }
        ItemMenuAction::Copy => {
            app.clipboard_items.clear();
            if let Some(cat) = app.categories.get(cat_idx) {
                if let Some(grp) = cat.groups.get(grp_idx) {
                    for &idx in &indices {
                        if let Some(it) = grp.items.get(idx) {
                            app.clipboard_items.push(it.clone());
                        }
                    }
                }
            }
            None
        }
        ItemMenuAction::SetHotkey => {
            let shortcut = app.categories.get(cat_idx)?.groups.get(grp_idx)?.items.get(target_shortcut)?;
            Some(ActiveModal::HotkeyManager(Some(shortcut.id.clone())))
        }
        ItemMenuAction::MoveToTop => {
            if let Some(cat) = app.categories.get_mut(cat_idx) {
                if let Some(grp) = cat.groups.get_mut(grp_idx) {
                    if target_shortcut > 0 && target_shortcut < grp.items.len() {
                        let it = grp.items.remove(target_shortcut);
                        grp.items.insert(0, it);
                        app.selected_shortcuts.clear();
                        app.selected_shortcuts.insert(0);
                        app.persist_data();
                    }
                }
            }
            None
        }
        ItemMenuAction::MoveToBottom => {
            if let Some(cat) = app.categories.get_mut(cat_idx) {
                if let Some(grp) = cat.groups.get_mut(grp_idx) {
                    if target_shortcut + 1 < grp.items.len() {
                        let it = grp.items.remove(target_shortcut);
                        grp.items.push(it);
                        app.selected_shortcuts.clear();
                        app.selected_shortcuts.insert(grp.items.len() - 1);
                        app.persist_data();
                    }
                }
            }
            None
        }
        ItemMenuAction::EditDetails | ItemMenuAction::Properties => {
            app.make_edit_shortcut_modal(cat_idx, grp_idx, target_shortcut)
        }
        ItemMenuAction::BrowseLocation => {
            if let Some(item) = app
                .categories
                .get(cat_idx)
                .and_then(|c| c.groups.get(grp_idx))
                .and_then(|g| g.items.get(target_shortcut))
            {
                if matches!(item.target_kind, targets::TargetKind::File | targets::TargetKind::Folder) {
                    let path = targets::resolve_path(&app.base_dir, &item.target_path);
                    *browse_location = Some(format!("/select,\"{}\"", path.display()));
                }
            }
            None
        }
        _ => None,
    }
}

/// 根据命中元素类型设置手形/文本/默认鼠标光标
unsafe fn apply_cursor_for_hit(target: HitTarget) {
    let cursor_id = match target {
        HitTarget::TitleMin
        | HitTarget::TitleSkin
        | HitTarget::TitleClose
        | HitTarget::ToolButton(_)
        | HitTarget::CategoryTab(_)
        | HitTarget::GroupBar(_)
        | HitTarget::ListScroll(_)
        | HitTarget::ShortcutItem(_)
        | HitTarget::SearchBtn
        | HitTarget::SearchModeItem(_)
        | HitTarget::TrayShortcut(_)
        | HitTarget::PopupMenuItem(_)
        | HitTarget::SkinQuickMenuItem(_)
        | HitTarget::ContextMenuItem(_)
        | HitTarget::ItemMenuItem(_)
        | HitTarget::MenuScrollUp
        | HitTarget::MenuScrollDown
        | HitTarget::MenuScrollTrack(_)
        | HitTarget::SkinMgrSelect(_)
        | HitTarget::SkinMgrApply
        | HitTarget::SkinMgrClose
        | HitTarget::CatMgrBtnNew
        | HitTarget::CatMgrBtnDelete
        | HitTarget::CatMgrBtnRename
        | HitTarget::CatMgrBtnEncrypt
        | HitTarget::CatMgrBtnMoveUp
        | HitTarget::CatMgrBtnMoveDown
        | HitTarget::CatMgrSelectRow(_)
        | HitTarget::CatMgrBtnChangeIcon
        | HitTarget::CatMgrClose
        | HitTarget::ColSortRow(_)
        | HitTarget::ColSortClose
        | HitTarget::EditScTab(_)
        | HitTarget::EditScPickFile
        | HitTarget::EditScPickDir
        | HitTarget::EditScPickIcon
        | HitTarget::EditScColorPalette(_)
        | HitTarget::EditScToggleMinOnRun
        | HitTarget::EditScOk
        | HitTarget::EditScCancel
        | HitTarget::AboutLink(_)
        | HitTarget::AboutSupportLink
        | HitTarget::AboutCheckUpdate
        | HitTarget::AboutClose => IDC_HAND,
        HitTarget::SearchBox
        | HitTarget::EditScField(_) => IDC_IBEAM,
        _ => IDC_ARROW,
    };
    SetCursor(LoadCursorW(0, cursor_id));
}

/// 判断鼠标在主窗口四条边或四个角的调整大小区域命中值
fn detect_resize_border_hit(mx: i32, my: i32, win_w: i32, win_h: i32, cfg: &SkinConfig) -> isize {
    let edge = 7;
    let corner = 15;

    // 仅避开右上角最小化/换肤/关闭按钮真实矩形内部（保留顶边 y < min_y 可拖动缩放）
    let (min_x, min_y, _, _) = cfg.title_btn_min.resolve_rect(win_w, win_h);
    let (cl_x, _, cl_w, cl_h) = cfg.title_btn_close.resolve_rect(win_w, win_h);
    if mx >= min_x && mx < cl_x + cl_w && my >= min_y && my < min_y + cl_h {
        return HTCLIENT as isize;
    }

    let left = mx < edge;
    let right = mx >= win_w - edge;
    let top = my < edge;
    let bottom = my >= win_h - edge;

    if mx < corner && my < corner {
        return HTTOPLEFT as isize;
    }
    if mx >= win_w - corner && my < corner {
        return HTTOPRIGHT as isize;
    }
    if mx < corner && my >= win_h - corner {
        return HTBOTTOMLEFT as isize;
    }
    if mx >= win_w - corner && my >= win_h - corner {
        return HTBOTTOMRIGHT as isize;
    }

    if left {
        return HTLEFT as isize;
    }
    if right {
        return HTRIGHT as isize;
    }
    if top {
        return HTTOP as isize;
    }
    if bottom {
        return HTBOTTOM as isize;
    }

    HTCLIENT as isize
}

/// 主窗口过程 (`WndProc`)
unsafe extern "system" fn main_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_ERASEBKGND => 1,

        // 抑制系统非客户区默认绘制，防止缩放或失焦时出现原生灰色边框线
        WM_NCPAINT => 0,

        WM_NCACTIVATE => {
            if IsIconic(hwnd) != 0 {
                DefWindowProcW(hwnd, msg, wparam, lparam)
            } else {
                1
            }
        }

        // 缩放或移动时不复用旧位图残留像素，强制完整重绘
        WM_WINDOWPOSCHANGING => {
            let wp = lparam as *mut WINDOWPOS;
            if !wp.is_null() {
                (*wp).flags |= SWP_NOCOPYBITS;
            }
            0
        }

        // 剥离 WS_THICKFRAME 原生非客户区厚边框，实现纯皮肤外观与四边四角流畅缩放兼得
        WM_NCCALCSIZE => {
            if wparam != 0 {
                0
            } else {
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
        }

        // 支持主窗体四条边和四个角直接拖动缩放（以真实客户区矩形为准）
        WM_NCHITTEST => {
            let sx = (lparam & 0xFFFF) as i16 as i32;
            let sy = ((lparam >> 16) & 0xFFFF) as i16 as i32;
            let mut pt = POINT { x: sx, y: sy };
            ScreenToClient(hwnd, &mut pt);
            let mut cr = RECT {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            };
            GetClientRect(hwnd, &mut cr);

            let mut ht = HTCLIENT as isize;
            APP_STATE.with(|cell| {
                if let Ok(guard) = cell.try_borrow() {
                    if let Some(app) = guard.as_ref() {
                        if !app.user_cfg.lock_move {
                            let cw = if cr.right > 0 {
                                cr.right
                            } else {
                                app.user_cfg.window_width
                            };
                            let ch = if cr.bottom > 0 {
                                cr.bottom
                            } else {
                                app.user_cfg.window_height
                            };
                            ht = detect_resize_border_hit(pt.x, pt.y, cw, ch, &app.skin_cfg);
                        }
                    }
                }
            });
            ht
        }

        // 设置动态光标：四边四角显示缩放双向箭头，可交互元素与快捷方式显示手形光标
        WM_SETCURSOR => {
            let ht = (lparam & 0xFFFF) as u16 as isize;
            match ht {
                x if x == HTLEFT as isize || x == HTRIGHT as isize => {
                    SetCursor(LoadCursorW(0, IDC_SIZEWE));
                    1
                }
                x if x == HTTOP as isize || x == HTBOTTOM as isize => {
                    SetCursor(LoadCursorW(0, IDC_SIZENS));
                    1
                }
                x if x == HTTOPLEFT as isize || x == HTBOTTOMRIGHT as isize => {
                    SetCursor(LoadCursorW(0, IDC_SIZENWSE));
                    1
                }
                x if x == HTTOPRIGHT as isize || x == HTBOTTOMLEFT as isize => {
                    SetCursor(LoadCursorW(0, IDC_SIZENESW));
                    1
                }
                x if x == HTCLIENT as isize => {
                    let mut target = HitTarget::None;
                    APP_STATE.with(|cell| {
                        if let Ok(guard) = cell.try_borrow() {
                            if let Some(app) = guard.as_ref() {
                                target = app.hover_target;
                            }
                        }
                    });
                    apply_cursor_for_hit(target);
                    1
                }
                _ => DefWindowProcW(hwnd, msg, wparam, lparam),
            }
        }

        // 限制主窗体最小宽高 (`280x420`)
        WM_GETMINMAXINFO => {
            let mmi = lparam as *mut MINMAXINFO;
            if !mmi.is_null() {
                (*mmi).ptMinTrackSize.x = 280;
                (*mmi).ptMinTrackSize.y = 420;
            }
            0
        }

        // 窗体真实尺寸改变后再更新宽高、画布、四角异形裁剪并立即同步重绘（避免放大时裁剪区域滞后或右侧留白）
        WM_MOVE | WM_ACTIVATE if msg == WM_MOVE || wparam & 0xFFFF == 0 => {
            popup::dismiss_visible(hwnd);
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_SIZE => {
            popup::dismiss_visible(hwnd);
            if wparam != SIZE_MINIMIZED as usize {
                let mut cr = RECT {
                    left: 0,
                    top: 0,
                    right: 0,
                    bottom: 0,
                };
                GetClientRect(hwnd, &mut cr);
                let lparam_w = (lparam & 0xFFFF) as u16 as i32;
                let lparam_h = ((lparam >> 16) & 0xFFFF) as u16 as i32;
                let new_w = if cr.right > 0 { cr.right } else { lparam_w };
                let new_h = if cr.bottom > 0 { cr.bottom } else { lparam_h };

                if new_w >= 260 && new_h >= 400 {
                    let mut pending_hrgn: HRGN = 0;
                    APP_STATE.with(|cell| {
                        if let Ok(mut guard) = cell.try_borrow_mut() {
                            if let Some(app) = guard.as_mut() {
                                app.user_cfg.window_width = new_w;
                                app.user_cfg.window_height = new_h;
                                app.canvas.resize(new_w, new_h);
                                // 使用 applied_rgn_size 守卫，防止 SetWindowRgn(..., 1) 重入 WM_SIZE 造成递归
                                if app.applied_rgn_size != (new_w, new_h) {
                                    app.applied_rgn_size = (new_w, new_h);
                                    pending_hrgn = build_window_corner_region(
                                        new_w,
                                        new_h,
                                        &app.skin_cfg,
                                        &app.skin_assets,
                                    );
                                }
                            }
                        }
                    });
                    if pending_hrgn != 0 {
                        apply_hrgn_safely(hwnd, pending_hrgn);
                    }
                    RedrawWindow(
                        hwnd,
                        ptr::null(),
                        0,
                        RDW_INVALIDATE | RDW_UPDATENOW | RDW_NOERASE,
                    );
                }
            }
            0
        }

        // 拖拽缩放结束后二次校验裁剪区域并保存新窗口大小至 `user/config.ini`
        WM_EXITSIZEMOVE => {
            let mut cr = RECT {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            };
            GetClientRect(hwnd, &mut cr);
            let mut pending_hrgn: HRGN = 0;
            APP_STATE.with(|cell| {
                if let Ok(mut guard) = cell.try_borrow_mut() {
                    if let Some(app) = guard.as_mut() {
                        if cr.right >= 260 && cr.bottom >= 400 {
                            app.user_cfg.window_width = cr.right;
                            app.user_cfg.window_height = cr.bottom;
                            app.canvas.resize(cr.right, cr.bottom);
                            if app.applied_rgn_size != (cr.right, cr.bottom) {
                                app.applied_rgn_size = (cr.right, cr.bottom);
                                pending_hrgn = build_window_corner_region(
                                    cr.right,
                                    cr.bottom,
                                    &app.skin_cfg,
                                    &app.skin_assets,
                                );
                            }
                        }
                        app.user_cfg.save(&app.base_dir);
                    }
                }
            });
            if pending_hrgn != 0 {
                apply_hrgn_safely(hwnd, pending_hrgn);
                RedrawWindow(
                    hwnd,
                    ptr::null(),
                    0,
                    RDW_INVALIDATE | RDW_UPDATENOW | RDW_NOERASE,
                );
            }
            advanced_window::stretch_docked(hwnd);
            0
        }

        hotkeys::WM_SYNC => { hotkeys::sync(hwnd); 0 }
        WM_HOTKEY => {
            hotkeys::dispatch(hwnd, wparam as i32, hotkeys::Key {
                modifiers: lparam as u32 & 0x0f, vk: (lparam >> 16) as u16,
            });
            0
        }
        WM_TIMER if wparam == hotkeys::TEXT_TIMER => { hotkeys::output_text(hwnd); 0 }
        WM_TIMER if wparam == advanced_window::TIMER_ID => {
            advanced_window::tick(hwnd);
            0
        }
        advanced_window::TRAY_MESSAGE => {
            advanced_window::tray_click(hwnd, lparam as u32);
            0
        }
        WM_TIMER => {
            APP_STATE.with(|cell| {
                if let Ok(mut guard) = cell.try_borrow_mut() {
                    if let Some(app) = guard.as_mut() {
                        let new_time = format_current_time(&app.user_cfg.advanced);
                        if app.time_string != new_time {
                            app.time_string = new_time;
                            InvalidateRect(hwnd, ptr::null(), 0);
                        }
                    }
                }
            });
            0
        }

        WM_DROPFILES => {
            let drop = wparam as HDROP;
            let mut point = POINT { x: 0, y: 0 };
            DragQueryPoint(drop, &mut point);
            let mut paths = Vec::new();
            for index in 0..DragQueryFileW(drop, u32::MAX, ptr::null_mut(), 0) {
                let length = DragQueryFileW(drop, index, ptr::null_mut(), 0);
                let mut buffer = vec![0u16; length as usize + 1];
                let copied = DragQueryFileW(drop, index, buffer.as_mut_ptr(), length + 1) as usize;
                if copied != 0 { paths.push(PathBuf::from(String::from_utf16_lossy(&buffer[..copied]))); }
            }
            DragFinish(drop);
            let plan = APP_STATE.with(|cell| {
                let guard = cell.try_borrow().ok()?;
                let app = guard.as_ref()?;
                if app.user_cfg.startup_lock { return None; }
                let settings = &app.user_cfg.advanced;
                let target = if settings.enabled(Setting::DropOnProgram) {
                    match hit_test_main(point.x, point.y, app.user_cfg.window_width, app.user_cfg.window_height,
                        &app.skin_cfg, &app.skin_assets, &app.categories, app.active_category, settings,
                        app.advanced_window.shortcut_scroll) {
                        HitTarget::ShortcutItem(index) => shortcut_at(app, index).cloned(),
                        _ => None,
                    }
                } else { None };
                Some((app.base_dir.clone(), settings.clone(), target))
            });
            if let Some((base, settings, target)) = plan {
                if let Some(mut item) = target {
                    for path in paths {
                        if !item.args.is_empty() { item.args.push(' '); }
                        item.args.push_str(&format!("\"{}\"", path.display()));
                    }
                    launch_shortcut(&item, hwnd);
                } else {
                    let files = paths.iter().map(|path| advanced_runtime::prepare_drop(&base, &settings, path))
                        .collect::<Vec<_>>();
                    if settings.enabled(Setting::WebIcons) {
                        for file in &files {
                            if advanced_web_icon::is_web_url(&file.target) {
                                advanced_web_icon::fetch_for_shortcut(base.clone(), file.target.clone(),
                                    settings.enabled(Setting::CacheWebIcons));
                            }
                        }
                    }
                    APP_STATE.with(|cell| {
                        if let Ok(mut guard) = cell.try_borrow_mut() {
                            if let Some(app) = guard.as_mut() {
                                for file in files { app.add_dropped_file(file); }
                            }
                        }
                    });
                    InvalidateRect(hwnd, ptr::null(), 0);
                }
            }
            0
        }

        WM_MOUSEMOVE => {
            let mx = (lparam & 0xFFFF) as i16 as i32;
            let my = ((lparam >> 16) & 0xFFFF) as i16 as i32;
            APP_STATE.with(|cell| {
                if let Ok(mut guard) = cell.try_borrow_mut() {
                    if let Some(app) = guard.as_mut() {
                        // 1. 正在内容区空白处拖动矩形框选（截图 4 & 5）
                        if app.down_target == HitTarget::ListBlank {
                            let dx = (mx - app.press_pos.0).abs();
                            let dy = (my - app.press_pos.1).abs();
                            if dx > 3 || dy > 3 || app.marquee_rect.is_some() {
                                app.marquee_rect = Some((app.press_pos.0, app.press_pos.1, mx, my));
                                let (rx0, ry0, rx1, ry1) =
                                    normalize_rect(app.press_pos.0, app.press_pos.1, mx, my);

                                app.selected_shortcuts.clear();
                                if let Some(cat) = app.categories.get(app.active_category) {
                                    for (orig_idx, ix, iy, hw, hh) in compute_visible_shortcut_rects(
                                        &app.skin_cfg,
                                        app.user_cfg.window_width,
                                        app.user_cfg.window_height,
                                        cat,
                                        &app.user_cfg.advanced, app.advanced_window.shortcut_scroll,
                                    ) {
                                        if rects_intersect(
                                            rx0,
                                            ry0,
                                            rx1,
                                            ry1,
                                            ix - 2,
                                            iy - 2,
                                            ix - 2 + hw,
                                            iy - 2 + hh,
                                        ) {
                                            app.selected_shortcuts.insert(orig_idx);
                                        }
                                    }
                                }
                                InvalidateRect(hwnd, ptr::null(), 0);
                                return;
                            }
                        }

                        // 2. 正在拖拽快捷方式排序
                        if let HitTarget::ShortcutItem(sc_idx) = app.down_target {
                            let dx = (mx - app.press_pos.0).abs();
                            let dy = (my - app.press_pos.1).abs();
                            if dx > 5 || dy > 5 {
                                app.dragging_shortcut = Some((sc_idx, mx, my));
                                InvalidateRect(hwnd, ptr::null(), 0);
                            }
                        }

                        let new_hover = hit_test_main(
                            mx,
                            my,
                            app.user_cfg.window_width,
                            app.user_cfg.window_height,
                            &app.skin_cfg,
                            &app.skin_assets,
                            &app.categories,
                            app.active_category,
                            &app.user_cfg.advanced, app.advanced_window.shortcut_scroll,
                        );
                        if new_hover != app.hover_target {
                            app.hover_target = new_hover;
                            InvalidateRect(hwnd, ptr::null(), 0);
                        }
                    }
                }
            });
            0
        }

        WM_MOUSEWHEEL => {
            let delta = (wparam >> 16) as i16;
            if !wheel_main(hwnd, delta, wparam as u16 & 8 != 0) { popup::scroll(delta); }
            0
        }

        WM_LBUTTONDOWN => {
            let mx = (lparam & 0xFFFF) as i16 as i32;
            let my = ((lparam >> 16) & 0xFFFF) as i16 as i32;
            let mut start_drag = false;

            APP_STATE.with(|cell| {
                if let Ok(mut guard) = cell.try_borrow_mut() {
                    if let Some(app) = guard.as_mut() {
                        app.press_pos = (mx, my);
                        let target = hit_test_main(
                            mx,
                            my,
                            app.user_cfg.window_width,
                            app.user_cfg.window_height,
                            &app.skin_cfg,
                            &app.skin_assets,
                            &app.categories,
                            app.active_category,
                            &app.user_cfg.advanced, app.advanced_window.shortcut_scroll,
                        );

                        if app.popup_menu != ActivePopupMenu::None {
                            match target {
                                HitTarget::SearchBtn
                                | HitTarget::ToolButton(ToolButtonKind::MainMenu)
                                | HitTarget::TitleSkin => {}
                                _ => {
                                    app.popup_menu = ActivePopupMenu::None;
                                    app.menu_scroll = 0;
                                }
                            }
                        }

                        if target == HitTarget::ListBlank {
                            app.selected_shortcuts.clear();
                        }

                        if target != HitTarget::SearchBtn {
                            app.search_focused = target == HitTarget::SearchBox;
                        }
                        app.down_target = target;
                        InvalidateRect(hwnd, ptr::null(), 0);

                        if target == HitTarget::DragCaption && !app.user_cfg.lock_move {
                            start_drag = true;
                        } else {
                            SetCapture(hwnd);
                        }
                    }
                }
            });

            if start_drag {
                ReleaseCapture();
                SendMessageW(hwnd, WM_NCLBUTTONDOWN, HTCAPTION as usize, 0);
            }
            0
        }

        WM_LBUTTONDBLCLK => {
            let x = lparam as i16 as i32;
            let y = (lparam >> 16) as i16 as i32;
            let item = APP_STATE.with(|cell| {
                let guard = cell.try_borrow().ok()?;
                let app = guard.as_ref()?;
                if !app.user_cfg.advanced.enabled(Setting::DoubleClick) { return None; }
                let HitTarget::ShortcutItem(index) = hit_test_main(x, y, app.user_cfg.window_width,
                    app.user_cfg.window_height, &app.skin_cfg, &app.skin_assets, &app.categories,
                    app.active_category, &app.user_cfg.advanced, app.advanced_window.shortcut_scroll
                ) else { return None; };
                shortcut_at(app, index).cloned()
            });
            if let Some(item) = item { launch_shortcut(&item, hwnd); }
            0
        }
        WM_LBUTTONUP | popup::WM_SELECT => {
            let from_popup = msg == popup::WM_SELECT;
            if !from_popup { ReleaseCapture(); }
            let mx = (lparam & 0xFFFF) as i16 as i32;
            let my = ((lparam >> 16) & 0xFFFF) as i16 as i32;
            let mut modal_to_open: Option<ActiveModal> = None;
            let mut deferred_req = DeferredDialogRequest::None;
            let mut skin_hrgn: HRGN = 0;
            let mut topmost = None;
            let mut browse_location = None;
            let mut shortcut_to_launch = None;
            let mut close_requested = false;
            let mut lock_change = None;
            let mut confirm_delete = None;

            APP_STATE.with(|cell| {
                if let Ok(mut guard) = cell.try_borrow_mut() {
                    if let Some(app) = guard.as_mut() {
                        if app.marquee_rect.take().is_some() {
                            app.down_target = HitTarget::None;
                            InvalidateRect(hwnd, ptr::null(), 0);
                            return;
                        }

                        let up_target = if from_popup {
                            let Some(window) = app.popup_window else { return; };
                            window.hit((mx, my), app.menu_scroll, app.skins.len())
                        } else { hit_test_main(
                            mx,
                            my,
                            app.user_cfg.window_width,
                            app.user_cfg.window_height,
                            &app.skin_cfg,
                            &app.skin_assets,
                            &app.categories,
                            app.active_category,
                            &app.user_cfg.advanced, app.advanced_window.shortcut_scroll,
                        ) };

                        if let Some((src_idx, _, _)) = app.dragging_shortcut.take() {
                            app.down_target = HitTarget::None;
                            let cat_idx = app.active_category;
                            match up_target {
                                HitTarget::ShortcutItem(dst_idx) if dst_idx != src_idx => {
                                    if let Some(cat) = app.categories.get_mut(cat_idx) {
                                        let grp_idx = cat
                                            .active_group_index
                                            .min(cat.groups.len().saturating_sub(1));
                                        if let Some(grp) = cat.groups.get_mut(grp_idx) {
                                            if src_idx < grp.items.len()
                                                && dst_idx < grp.items.len()
                                            {
                                                grp.items.swap(src_idx, dst_idx);
                                                app.persist_data();
                                            }
                                        }
                                    }
                                }
                                HitTarget::GroupBar(dst_grp_idx) => {
                                    if let Some(cat) = app.categories.get_mut(cat_idx) {
                                        let src_grp_idx = cat
                                            .active_group_index
                                            .min(cat.groups.len().saturating_sub(1));
                                        if dst_grp_idx != src_grp_idx
                                            && dst_grp_idx < cat.groups.len()
                                        {
                                            if src_idx < cat.groups[src_grp_idx].items.len() {
                                                let mut item =
                                                    cat.groups[src_grp_idx].items.remove(src_idx);
                                                item.column_id =
                                                    cat.groups[dst_grp_idx].id.clone();
                                                cat.groups[dst_grp_idx].items.push(item);
                                                app.persist_data();
                                            }
                                        }
                                    }
                                }
                                HitTarget::CategoryTab(dst_cat_idx) if dst_cat_idx != cat_idx => {
                                    if dst_cat_idx < app.categories.len() {
                                        let src_grp_idx = app.categories[cat_idx]
                                            .active_group_index
                                            .min(app.categories[cat_idx].groups.len().saturating_sub(1));
                                        if src_idx
                                            < app.categories[cat_idx].groups[src_grp_idx].items.len()
                                            && !app.categories[dst_cat_idx].groups.is_empty()
                                        {
                                            let mut item = app.categories[cat_idx].groups
                                                [src_grp_idx]
                                                .items
                                                .remove(src_idx);
                                            item.category_id =
                                                app.categories[dst_cat_idx].id.clone();
                                            item.column_id =
                                                app.categories[dst_cat_idx].groups[0].id.clone();
                                            app.categories[dst_cat_idx].groups[0].items.push(item);
                                            app.persist_data();
                                        }
                                    }
                                }
                                _ => {}
                            }
                            InvalidateRect(hwnd, ptr::null(), 0);
                            return;
                        }

                        let pressed = if from_popup { up_target } else { app.down_target };
                        app.down_target = HitTarget::None;

                        if up_target == pressed {
                            match up_target {
                                HitTarget::SearchBtn => {
                                    app.menu_scroll = 0;
                                    if app.popup_menu == ActivePopupMenu::SearchMode {
                                        app.popup_menu = ActivePopupMenu::None;
                                    } else {
                                        app.popup_menu = ActivePopupMenu::SearchMode;
                                        app.hover_target = HitTarget::SearchBtn;
                                    }
                                }
                                HitTarget::SearchModeItem(index) => select_search_mode(app, index),
                                HitTarget::TrayShortcut(index) => {
                                    if let ActivePopupMenu::TrayStart { category, .. } = app.popup_menu {
                                        shortcut_to_launch = app.categories.get(category)
                                            .and_then(|cat| cat.groups.iter().flat_map(|g| &g.items).nth(index)).cloned();
                                    }
                                    app.popup_menu = ActivePopupMenu::None;
                                }
                                HitTarget::TitleMin => {
                                    ShowWindow(hwnd, SW_MINIMIZE);
                                }
                                HitTarget::TitleClose => {
                                    close_requested = true;
                                }
                                HitTarget::TitleSkin => {
                                    app.menu_scroll = 0;
                                    app.popup_menu =
                                        if app.popup_menu == ActivePopupMenu::SkinQuickMenu {
                                            ActivePopupMenu::None
                                        } else {
                                            ActivePopupMenu::SkinQuickMenu
                                        };
                                }
                                HitTarget::ToolButton(ToolButtonKind::ThemeBrush) => {
                                    app.popup_menu = ActivePopupMenu::None;
                                    modal_to_open = Some(ActiveModal::SkinManager);
                                }
                                HitTarget::ToolButton(ToolButtonKind::MainMenu) => {
                                    app.menu_scroll = 0;
                                    app.popup_menu =
                                        if app.popup_menu == ActivePopupMenu::MainMenu {
                                            ActivePopupMenu::None
                                        } else {
                                            ActivePopupMenu::MainMenu
                                        };
                                }
                                HitTarget::MenuScrollUp => {
                                    app.menu_scroll = app.menu_scroll.saturating_sub(1);
                                }
                                HitTarget::MenuScrollDown => {
                                    let (vis_rows, total_rows) = app.popup_window.map_or((0, 0), |window| window.scroll_bounds(app.skins.len()));
                                    let max_scroll = total_rows.saturating_sub(vis_rows);
                                    app.menu_scroll = (app.menu_scroll + 1).min(max_scroll);
                                }
                                HitTarget::MenuScrollTrack(rel_y) => {
                                    let (vis_rows, total_rows) = app.popup_window.map_or((0, 0), |window| window.scroll_bounds(app.skins.len()));
                                    let max_scroll = total_rows.saturating_sub(vis_rows);
                                    if max_scroll > 0 {
                                        let track_h = ((vis_rows as i32) * 24 - 28).max(16);
                                        let ratio = (rel_y.max(0) as f32 / track_h as f32).clamp(0.0, 1.0);
                                        app.menu_scroll = ((ratio * max_scroll as f32).round() as usize).min(max_scroll);
                                    }
                                }
                                HitTarget::ToolButton(ToolButtonKind::Category) => {
                                    app.popup_menu = ActivePopupMenu::None;
                                    modal_to_open = Some(ActiveModal::CategoryManager {
                                        selected_idx: app.active_category,
                                        renaming: false,
                                        rename_buf: String::new(),
                                    });
                                }
                                HitTarget::ToolButton(ToolButtonKind::Backup) => {
                                    app.popup_menu = ActivePopupMenu::None;
                                    let sel = app
                                        .categories
                                        .get(app.active_category)
                                        .map(|c| c.active_group_index)
                                        .unwrap_or(0);
                                    modal_to_open = Some(ActiveModal::ColumnSort {
                                        selected_idx: sel,
                                        dragging_idx: None,
                                    });
                                }
                                HitTarget::CategoryTab(cat_idx) => {
                                    if cat_idx < app.categories.len() {
                                        app.active_category = cat_idx;
                                        app.selected_shortcuts.clear();
                                        app.advanced_window.shortcut_scroll = 0;
                                    }
                                }
                                HitTarget::ListScroll(offset) => {
                                    if let Some(grid) = advanced_list_grid(app) {
                                        app.advanced_window.shortcut_scroll = grid.position_for_offset(offset);
                                    }
                                }
                                HitTarget::GroupBar(grp_idx) => {
                                    if let Some(cat) = app.categories.get_mut(app.active_category) {
                                        if grp_idx < cat.groups.len() {
                                            cat.active_group_index = grp_idx;
                                            app.selected_shortcuts.clear();
                                            app.advanced_window.shortcut_scroll = 0;
                                        }
                                    }
                                }
                                HitTarget::ShortcutItem(item_idx) => {
                                    app.selected_shortcuts.clear();
                                    app.selected_shortcuts.insert(item_idx);
                                    if let Some(cat) = app.categories.get(app.active_category) {
                                        let grp_idx = cat
                                            .active_group_index
                                            .min(cat.groups.len().saturating_sub(1));
                                        if let Some(grp) = cat.groups.get(grp_idx) {
                                            if let Some(item) = grp.items.get(item_idx) {
                                                if !app.user_cfg.advanced.enabled(Setting::DoubleClick) {
                                                    shortcut_to_launch = Some(item.clone());
                                                }
                                            }
                                        }
                                    }
                                }
                                HitTarget::PopupMenuItem(menu_idx) => {
                                    let was_topmost = app.user_cfg.always_on_top;
                                    let Some(action) = main_menu_action(app, menu_idx) else { return; };
                                    if matches!(action, MenuAction::ToggleStartupLock | MenuAction::ExitApp) {
                                        app.popup_menu = ActivePopupMenu::None;
                                        if action == MenuAction::ToggleStartupLock {
                                            lock_change = Some((app.base_dir.clone(), app.user_cfg.startup_lock));
                                        } else { close_requested = true; }
                                    } else {
                                        modal_to_open = handle_main_menu_action(app, action);
                                    }
                                    if was_topmost != app.user_cfg.always_on_top {
                                        topmost = Some(app.user_cfg.always_on_top);
                                    }
                                }
                                HitTarget::SkinQuickMenuItem(q_idx) => {
                                    app.popup_menu = ActivePopupMenu::None;
                                    if q_idx < app.skins.len() {
                                        if let Some(rgn) = app.switch_skin(q_idx) {
                                            skin_hrgn = rgn;
                                        }
                                    } else if q_idx == app.skins.len() + 1 {
                                        modal_to_open = Some(ActiveModal::SkinManager);
                                    }
                                }
                                HitTarget::ContextMenuItem(c_idx) => {
                                    let (m, req) = handle_context_menu_action(app, c_idx);
                                    modal_to_open = m;
                                    deferred_req = req;
                                }
                                HitTarget::ItemMenuItem(i_idx) => {
                                    if let ActivePopupMenu::ItemMenu {
                                        target_shortcut, ..
                                    } = app.popup_menu
                                    {
                                        let deleting = build_item_menu_items().get(i_idx)
                                            .is_some_and(|item| item.action == ItemMenuAction::Delete);
                                        if deleting && app.user_cfg.advanced.enabled(Setting::ConfirmDelete) {
                                            app.popup_menu = ActivePopupMenu::None;
                                            confirm_delete = Some((i_idx, target_shortcut));
                                        } else {
                                            modal_to_open = handle_item_menu_action(app, i_idx, target_shortcut, &mut browse_location);
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                        InvalidateRect(hwnd, ptr::null(), 0);
                    }
                }
            });

            popup::sync(hwnd);
            if let Some((index, target)) = confirm_delete {
                if advanced_runtime::confirm(hwnd, "删除选中的快捷方式？此操作只会移除 KStart 中的条目。") {
                    APP_STATE.with(|cell| {
                        if let Ok(mut guard) = cell.try_borrow_mut() {
                            if let Some(app) = guard.as_mut() {
                                handle_item_menu_action(app, index, target, &mut browse_location);
                            }
                        }
                    });
                    InvalidateRect(hwnd, ptr::null(), 0);
                }
            }
            if close_requested { request_main_close(hwnd, from_popup); }
            if let Some((base, locked)) = lock_change {
                if !locked || advanced_prompt::authenticate(hwnd, &base, true) {
                    APP_STATE.with(|cell| {
                        if let Ok(mut guard) = cell.try_borrow_mut() {
                            if let Some(app) = guard.as_mut() {
                                app.user_cfg.startup_lock = !locked;
                                app.user_cfg.save(&app.base_dir);
                            }
                        }
                    });
                    InvalidateRect(hwnd, ptr::null(), 0);
                }
            }
            if let Some(item) = shortcut_to_launch { launch_shortcut(&item, hwnd); }
            if let Some(topmost) = topmost {
                let insert_after = if topmost { HWND_TOPMOST } else { HWND_NOTOPMOST };
                SetWindowPos(hwnd, insert_after, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE);
            }
            if let Some(args) = browse_location {
                launch_target_url_or_path("explorer.exe", &args, "", hwnd);
            }
            if skin_hrgn != 0 {
                apply_hrgn_safely(hwnd, skin_hrgn);
                InvalidateRect(hwnd, ptr::null(), 0);
            }

            // 在 RefCell 借用完全释放后再弹出文件/目录选择框，防止重入崩溃
            match deferred_req {
                DeferredDialogRequest::ContextSystemAction(cat_idx, grp_idx) => {
                    if let Some(choice) = hotkey_dialog::choose_system(hwnd) {
                        modal_to_open = APP_STATE.with(|cell| {
                            let guard = cell.try_borrow().ok()?;
                            let app = guard.as_ref()?;
                            let mut modal = app.make_new_shortcut_modal(cat_idx, grp_idx, choice.label.into(),
                                choice.target.into(), String::new(), None, "ctx_add_sys");
                            if let ActiveModal::EditShortcut(editor) = &mut modal {
                                editor.args = choice.args.into();
                                editor.target_kind_hint = Some(choice.kind);
                            }
                            Some(modal)
                        });
                    }
                }
                DeferredDialogRequest::ContextAddFile(cat_idx, grp_idx) => {
                    if let Some(picked) = pick_file_dialog(
                        hwnd,
                        "选择要添加的文件或程序",
                        &[
                            ("所有文件 (*.*)", "*.*"),
                            ("可执行程序 (*.exe;*.lnk)", "*.exe;*.lnk"),
                        ],
                    ) {
                        let name = picked
                            .file_stem()
                            .or_else(|| picked.file_name())
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or_else(|| "新建快捷方式".to_string());
                        let target_path = picked.to_string_lossy().to_string();
                        let work_dir = picked
                            .parent()
                            .map(|p| p.to_string_lossy().to_string())
                            .unwrap_or_default();
                        let icon_16 = extract_file_icon_16(&picked);
                        APP_STATE.with(|cell| {
                            if let Ok(guard) = cell.try_borrow() {
                                if let Some(app) = guard.as_ref() {
                                    modal_to_open = Some(app.make_new_shortcut_modal(
                                        cat_idx,
                                        grp_idx,
                                        name,
                                        target_path,
                                        work_dir,
                                        icon_16,
                                        "app_msedge",
                                    ));
                                }
                            }
                        });
                    }
                }
                DeferredDialogRequest::ContextAddDir(cat_idx, grp_idx) => {
                    if let Some(picked) = pick_folder_dialog(hwnd, "选择要添加的目录文件夹") {
                        let name = picked
                            .file_name()
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or_else(|| picked.to_string_lossy().to_string());
                        let target_path = picked.to_string_lossy().to_string();
                        let icon_16 = extract_file_icon_16(&picked);
                        APP_STATE.with(|cell| {
                            if let Ok(guard) = cell.try_borrow() {
                                if let Some(app) = guard.as_ref() {
                                    modal_to_open = Some(app.make_new_shortcut_modal(
                                        cat_idx,
                                        grp_idx,
                                        name,
                                        target_path,
                                        String::new(),
                                        icon_16,
                                        "ctx_add_dir",
                                    ));
                                }
                            }
                        });
                    }
                }
                _ => {}
            }

            if let Some(modal) = modal_to_open {
                show_centered_modal(modal);
            }
            0
        }

        WM_RBUTTONUP => {
            let mx = (lparam & 0xFFFF) as i16 as i32;
            let my = ((lparam >> 16) & 0xFFFF) as i16 as i32;
            APP_STATE.with(|cell| {
                if let Ok(mut guard) = cell.try_borrow_mut() {
                    if let Some(app) = guard.as_mut() {
                        let target = hit_test_main(
                            mx,
                            my,
                            app.user_cfg.window_width,
                            app.user_cfg.window_height,
                            &app.skin_cfg,
                            &app.skin_assets,
                            &app.categories,
                            app.active_category,
                            &app.user_cfg.advanced, app.advanced_window.shortcut_scroll,
                        );
                        let (lx, ly, lw, lh) = app.skin_cfg.item_list_box.resolve_rect(
                            app.user_cfg.window_width,
                            app.user_cfg.window_height,
                        );
                        if mx >= lx && mx < lx + lw && my >= ly && my < ly + lh {
                            app.menu_scroll = 0;
                            if let HitTarget::ShortcutItem(idx) = target {
                                if !app.selected_shortcuts.contains(&idx) {
                                    app.selected_shortcuts.clear();
                                    app.selected_shortcuts.insert(idx);
                                }
                                app.popup_menu = ActivePopupMenu::ItemMenu {
                                    x: mx,
                                    y: my,
                                    target_shortcut: idx,
                                };
                            } else {
                                app.popup_menu = ActivePopupMenu::ContextMenu { x: mx, y: my };
                            }
                        } else if target == HitTarget::ToolButton(ToolButtonKind::MainMenu) {
                            // 仅当明确点击左下角【菜单】按钮时才弹出主菜单
                            app.menu_scroll = 0;
                            app.popup_menu = if app.popup_menu == ActivePopupMenu::MainMenu {
                                ActivePopupMenu::None
                            } else {
                                ActivePopupMenu::MainMenu
                            };
                        } else {
                            // 在非菜单、非列表区域右键时仅关闭当前已打开的菜单，不再误弹【菜单】
                            app.menu_scroll = 0;
                            app.popup_menu = ActivePopupMenu::None;
                        }
                        InvalidateRect(hwnd, ptr::null(), 0);
                    }
                }
            });
            popup::sync(hwnd);
            0
        }

        WM_KEYDOWN => {
            let vk = wparam as u16;
            let mut search_submission = None;
            APP_STATE.with(|cell| {
                if let Ok(mut guard) = cell.try_borrow_mut() {
                    if let Some(app) = guard.as_mut() {
                        if vk == VK_ESCAPE {
                            if app.popup_menu != ActivePopupMenu::None {
                                app.popup_menu = ActivePopupMenu::None;
                            } else if !app.selected_shortcuts.is_empty() {
                                app.selected_shortcuts.clear();
                            } else {
                                app.search_query.clear();
                                app.search_focused = false;
                            }
                            InvalidateRect(hwnd, ptr::null(), 0);
                            return;
                        }

                        if app.popup_menu == ActivePopupMenu::SearchMode {
                            let len = SearchMode::OPTIONS.len();
                            let index = match app.hover_target {
                                HitTarget::SearchModeItem(index) => index,
                                _ => SearchMode::OPTIONS.iter()
                                    .position(|&(mode, _)| mode == app.user_cfg.search_mode)
                                    .unwrap_or(0),
                            };
                            match vk {
                                VK_UP => app.hover_target = HitTarget::SearchModeItem((index + len - 1) % len),
                                VK_DOWN => app.hover_target = HitTarget::SearchModeItem((index + 1) % len),
                                VK_RETURN if lparam & (1 << 30) == 0 => select_search_mode(app, index),
                                _ => {}
                            }
                            InvalidateRect(hwnd, ptr::null(), 0);
                            return;
                        }

                        if vk == VK_RETURN
                            && app.search_focused
                            && app.popup_menu == ActivePopupMenu::None
                        {
                            if lparam & (1 << 30) == 0 {
                                search_submission =
                                    Some((app.user_cfg.search_mode, app.search_query.clone()));
                            }
                            return;
                        }

                        if vk == VK_BACK && app.search_focused {
                            app.search_query.pop();
                            InvalidateRect(hwnd, ptr::null(), 0);
                        }
                    }
                }
            });
            popup::sync(hwnd);
            if let Some((mode, query)) = search_submission {
                submit_search(hwnd, mode, &query);
            }
            0
        }

        WM_CHAR => {
            if let Some(ch) = char::from_u32(wparam as u32) {
                if !ch.is_control() {
                    APP_STATE.with(|cell| {
                        if let Ok(mut guard) = cell.try_borrow_mut() {
                            if let Some(app) = guard.as_mut() {
                                if app.popup_menu == ActivePopupMenu::SearchMode {
                                    if let Some(index) = SearchMode::OPTIONS.iter().position(|&(_, label)| {
                                        label.split_once('&').is_some_and(|(_, suffix)| suffix.starts_with(ch.to_ascii_uppercase()))
                                    }) {
                                        select_search_mode(app, index);
                                        InvalidateRect(hwnd, ptr::null(), 0);
                                    }
                                } else if app.search_focused {
                                    app.search_query.push(ch);
                                    InvalidateRect(hwnd, ptr::null(), 0);
                                }
                            }
                        }
                    });
                }
            }
            popup::sync(hwnd);
            0
        }

        WM_PAINT => {
            let mut cr = RECT {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            };
            GetClientRect(hwnd, &mut cr);
            let mut ps: PAINTSTRUCT = std::mem::zeroed();
            let hdc = BeginPaint(hwnd, &mut ps);
            APP_STATE.with(|cell| {
                if let Ok(mut guard) = cell.try_borrow_mut() {
                    if let Some(app) = guard.as_mut() {
                        if cr.right >= 260 && cr.bottom >= 400 {
                            if app.user_cfg.window_width != cr.right
                                || app.user_cfg.window_height != cr.bottom
                            {
                                app.user_cfg.window_width = cr.right;
                                app.user_cfg.window_height = cr.bottom;
                                app.canvas.resize(cr.right, cr.bottom);
                            }
                        }
                        let rstate = RenderState {
                            win_w: app.user_cfg.window_width,
                            win_h: app.user_cfg.window_height,
                            skin_cfg: &app.skin_cfg,
                            skin_assets: &app.skin_assets,
                            builtin_icons: &app.builtin_icons,
                            user_cfg: &app.user_cfg,
                            categories: &app.categories,
                            active_category: app.active_category,
                            hover_target: app.hover_target,
                            down_target: app.down_target,
                            time_string: &app.time_string,
                            list_background: app.list_background.as_ref(),
                            shortcut_scroll: app.advanced_window.shortcut_scroll,
                            volume_level: app.advanced_window.volume_indicator.map(|(level, _)| level),
                            tooltip: app.advanced_window.tooltip.as_ref().filter(|_| app.popup_menu == ActivePopupMenu::None),
                            search_query: &app.search_query,
                            search_focused: app.search_focused,
                            popup_menu: ActivePopupMenu::None,
                            menu_scroll: app.menu_scroll,
                            skins: &app.skins,
                            active_skin_index: app.active_skin_index,
                            selected_skin_index: app.selected_skin_index,
                            dragging_shortcut: app.dragging_shortcut,
                            marquee_rect: app.marquee_rect,
                            selected_shortcuts: &app.selected_shortcuts,
                        };
                        let (texts, popup_clip) = render_main_frame(&rstate, &mut app.canvas);
                        let _ = blit_canvas_with_gdi_texts(&app.canvas, &texts, popup_clip, hdc, false,
                            Some(&app.user_cfg.advanced.fonts));
                    }
                }
            });
            EndPaint(hwnd, &ps);
            0
        }

        WM_CLOSE => {
            request_main_close(hwnd, false);
            0
        }
        WM_DESTROY => {
            popup::dismiss_visible(hwnd);
            hotkeys::clear(hwnd);
            advanced_window::remove_tray(hwnd);
            PostQuitMessage(0);
            0
        }

        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

/// 独立屏幕居中弹窗窗口过程 (`ModalWndProc`)
unsafe extern "system" fn modal_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // 搜索窗口沿用独立弹窗窗口类，并使用系统标题栏和原生子控件。
    if GetWindowLongPtrW(hwnd, GWL_STYLE) as u32 & WS_CAPTION == WS_CAPTION {
        return native_modal_wnd_proc(hwnd, msg, wparam, lparam);
    }
    match msg {
        WM_ERASEBKGND => 1,

        WM_CLOSE => {
            close_modal_window();
            0
        }

        WM_SETCURSOR => {
            let ht = (lparam & 0xFFFF) as u16 as isize;
            if ht == HTCLIENT as isize {
                let mut target = HitTarget::None;
                APP_STATE.with(|cell| {
                    if let Ok(guard) = cell.try_borrow() {
                        if let Some(app) = guard.as_ref() {
                            target = app.modal_hover_target;
                        }
                    }
                });
                apply_cursor_for_hit(target);
                1
            } else {
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
        }

        WM_MOUSEMOVE => {
            let mx = (lparam & 0xFFFF) as i16 as i32;
            let my = ((lparam >> 16) & 0xFFFF) as i16 as i32;
            APP_STATE.with(|cell| {
                if let Ok(mut guard) = cell.try_borrow_mut() {
                    if let Some(app) = guard.as_mut() {
                        let new_hov = hit_test_modal(
                            mx,
                            my,
                            &app.modal,
                            &app.categories,
                            app.active_category,
                            app.skins.len(),
                        );
                        if new_hov != app.modal_hover_target {
                            app.modal_hover_target = new_hov;
                            InvalidateRect(hwnd, ptr::null(), 0);
                        }
                    }
                }
            });
            0
        }

        WM_LBUTTONDOWN => {
            let mx = (lparam & 0xFFFF) as i16 as i32;
            let my = ((lparam >> 16) & 0xFFFF) as i16 as i32;
            let mut drag_modal = false;

            APP_STATE.with(|cell| {
                if let Ok(mut guard) = cell.try_borrow_mut() {
                    if let Some(app) = guard.as_mut() {
                        let target = hit_test_modal(
                            mx,
                            my,
                            &app.modal,
                            &app.categories,
                            app.active_category,
                            app.skins.len(),
                        );
                        if target == HitTarget::ModalTitleDrag {
                            drag_modal = true;
                        } else if let HitTarget::ColSortRow(r_idx) = target {
                            if let ActiveModal::ColumnSort {
                                selected_idx,
                                dragging_idx,
                            } = &mut app.modal
                            {
                                *selected_idx = r_idx;
                                *dragging_idx = Some(r_idx);
                            }
                            SetCapture(hwnd);
                        } else {
                            SetCapture(hwnd);
                        }
                        app.modal_hover_target = target;
                        InvalidateRect(hwnd, ptr::null(), 0);
                    }
                }
            });

            if drag_modal {
                ReleaseCapture();
                SendMessageW(hwnd, WM_NCLBUTTONDOWN, HTCAPTION as usize, 0);
            }
            0
        }

        WM_LBUTTONUP => {
            ReleaseCapture();
            let mx = (lparam & 0xFFFF) as i16 as i32;
            let my = ((lparam >> 16) & 0xFFFF) as i16 as i32;
            let mut should_close = false;
            let mut refresh_main = false;
            let mut main_hwnd: HWND = 0;
            let mut skin_hrgn: HRGN = 0;
            let mut deferred_req = DeferredDialogRequest::None;

            APP_STATE.with(|cell| {
                if let Ok(mut guard) = cell.try_borrow_mut() {
                    if let Some(app) = guard.as_mut() {
                        main_hwnd = app.main_hwnd;
                        let target = hit_test_modal(
                            mx,
                            my,
                            &app.modal,
                            &app.categories,
                            app.active_category,
                            app.skins.len(),
                        );

                        if let ActiveModal::ColumnSort {
                            selected_idx,
                            dragging_idx,
                        } = &mut app.modal
                        {
                            if let Some(src_col) = dragging_idx.take() {
                                if let HitTarget::ColSortRow(dst_col) = target {
                                    if src_col != dst_col {
                                        if let Some(cat) =
                                            app.categories.get_mut(app.active_category)
                                        {
                                            if src_col < cat.groups.len()
                                                && dst_col < cat.groups.len()
                                            {
                                                cat.groups.swap(src_col, dst_col);
                                                *selected_idx = dst_col;
                                                app.persist_data();
                                                refresh_main = true;
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        match target {
                            HitTarget::SkinMgrSelect(idx) => {
                                app.selected_skin_index = idx;
                            }
                            HitTarget::SkinMgrApply => {
                                let idx = app.selected_skin_index;
                                if let Some(rgn) = app.switch_skin(idx) {
                                    skin_hrgn = rgn;
                                }
                                refresh_main = true;
                            }
                            HitTarget::SkinMgrClose
                            | HitTarget::CatMgrClose
                            | HitTarget::ColSortClose
                            | HitTarget::EditScCancel
                            | HitTarget::AboutClose => {
                                should_close = true;
                            }

                            HitTarget::CatMgrSelectRow(r_idx) => {
                                if let ActiveModal::CategoryManager {
                                    selected_idx,
                                    renaming,
                                    ..
                                } = &mut app.modal
                                {
                                    *selected_idx = r_idx;
                                    *renaming = false;
                                }
                            }
                            HitTarget::CatMgrBtnNew => {
                                let next_n = app.categories.len() + 1;
                                let cat_id = format!("cat_{}", next_n);
                                let col_id =
                                    format!("col_{}", app.data_store.columns.len() + next_n);
                                app.categories.push(CategoryTab {
                                    id: cat_id.clone(),
                                    name: format!("新分类{}", next_n),
                                    icon_name: "cat_laptop".to_string(),
                                    encrypted: false,
                                    active_group_index: 0,
                                    groups: vec![ShortcutGroup {
                                        id: col_id,
                                        category_id: cat_id,
                                        title: "默认栏目".to_string(),
                                        encrypted: false,
                                        items: Vec::new(),
                                    }],
                                });
                                let new_sel = app.categories.len() - 1;
                                app.modal = ActiveModal::CategoryManager {
                                    selected_idx: new_sel,
                                    renaming: false,
                                    rename_buf: String::new(),
                                };
                                app.persist_data();
                                refresh_main = true;
                            }
                            HitTarget::CatMgrBtnDelete => {
                                if let ActiveModal::CategoryManager { selected_idx, .. } =
                                    &mut app.modal
                                {
                                    if app.categories.len() > 1
                                        && *selected_idx < app.categories.len()
                                    {
                                        app.categories.remove(*selected_idx);
                                        *selected_idx =
                                            (*selected_idx).min(app.categories.len() - 1);
                                        app.active_category =
                                            app.active_category.min(app.categories.len() - 1);
                                        app.persist_data();
                                        refresh_main = true;
                                    }
                                }
                            }
                            HitTarget::CatMgrBtnRename => {
                                if let ActiveModal::CategoryManager {
                                    selected_idx,
                                    renaming,
                                    rename_buf,
                                } = &mut app.modal
                                {
                                    if let Some(cat) = app.categories.get(*selected_idx) {
                                        *rename_buf = cat.name.clone();
                                        *renaming = true;
                                    }
                                }
                            }
                            HitTarget::CatMgrBtnEncrypt => {
                                if let ActiveModal::CategoryManager { selected_idx, .. } =
                                    &mut app.modal
                                {
                                    if let Some(cat) = app.categories.get_mut(*selected_idx) {
                                        cat.encrypted = !cat.encrypted;
                                        app.persist_data();
                                    }
                                }
                            }
                            HitTarget::CatMgrBtnMoveUp => {
                                if let ActiveModal::CategoryManager { selected_idx, .. } =
                                    &mut app.modal
                                {
                                    if *selected_idx > 0 && *selected_idx < app.categories.len() {
                                        app.categories.swap(*selected_idx, *selected_idx - 1);
                                        *selected_idx -= 1;
                                        app.persist_data();
                                        refresh_main = true;
                                    }
                                }
                            }
                            HitTarget::CatMgrBtnMoveDown => {
                                if let ActiveModal::CategoryManager { selected_idx, .. } =
                                    &mut app.modal
                                {
                                    if *selected_idx + 1 < app.categories.len() {
                                        app.categories.swap(*selected_idx, *selected_idx + 1);
                                        *selected_idx += 1;
                                        app.persist_data();
                                        refresh_main = true;
                                    }
                                }
                            }
                            HitTarget::CatMgrBtnChangeIcon => {
                                if let ActiveModal::CategoryManager { selected_idx, .. } =
                                    &mut app.modal
                                {
                                    if let Some(cat) = app.categories.get_mut(*selected_idx) {
                                        cat.icon_name = if cat.icon_name.contains("cube") {
                                            "cat_laptop".to_string()
                                        } else {
                                            "cat_cube".to_string()
                                        };
                                        app.persist_data();
                                        refresh_main = true;
                                    }
                                }
                            }

                            HitTarget::EditScTab(tab_idx) => {
                                if let ActiveModal::EditShortcut(ed) = &mut app.modal {
                                    ed.active_tab = tab_idx;
                                    ed.active_field = if tab_idx == 1 {
                                        EditShortcutField::Remark
                                    } else {
                                        EditShortcutField::Title
                                    };
                                }
                            }
                            HitTarget::EditScField(field) => {
                                if let ActiveModal::EditShortcut(ed) = &mut app.modal {
                                    ed.active_field = field;
                                }
                            }
                            HitTarget::EditScPickFile => {
                                deferred_req = DeferredDialogRequest::EditPickFile;
                            }
                            HitTarget::EditScPickDir => {
                                deferred_req = DeferredDialogRequest::EditPickDir;
                            }
                            HitTarget::EditScPickIcon => {
                                deferred_req = DeferredDialogRequest::EditPickIcon;
                            }
                            HitTarget::EditScColorPalette(c_idx) => {
                                if let ActiveModal::EditShortcut(ed) = &mut app.modal {
                                    if let Some(&col) = STANDARD_PALETTE.get(c_idx) {
                                        ed.text_color = col;
                                    }
                                }
                            }
                            HitTarget::EditScToggleMinOnRun => {
                                if let ActiveModal::EditShortcut(ed) = &mut app.modal {
                                    ed.minimize_on_run = !ed.minimize_on_run;
                                }
                            }
                            HitTarget::EditScOk => {
                                let ed_clone = if let ActiveModal::EditShortcut(ed) = &app.modal {
                                    Some(ed.clone())
                                } else {
                                    None
                                };
                                if let Some(ed) = ed_clone {
                                    if let Some(ico) = &ed.pending_icon_16 {
                                        let user_icon_dir = app.base_dir.join("user").join("icons");
                                        let _ = fs::create_dir_all(&user_icon_dir);
                                        let png_path =
                                            user_icon_dir.join(format!("{}.png", ed.shortcut_id));
                                        let _ = ico.save_png(&png_path);
                                        app.builtin_icons
                                            .app_icons
                                            .insert(ed.icon_key.clone(), ico.clone());
                                    }

                                    if ed.is_new {
                                        if let Some(cat) = app.categories.get_mut(ed.cat_idx) {
                                            let cat_id = cat.id.clone();
                                            if let Some(grp) = cat.groups.get_mut(ed.grp_idx) {
                                                let col_id = grp.id.clone();
                                                grp.items.push(ShortcutItem {
                                                    id: ed.shortcut_id.clone(),
                                                    category_id: cat_id,
                                                    column_id: col_id,
                                                    name: if ed.title.trim().is_empty() {
                                                        "未命名快捷方式".to_string()
                                                    } else {
                                                        ed.title
                                                    },
                                                    target_kind: ed.target_kind_hint.unwrap_or_else(|| targets::TargetKind::infer(&app.base_dir, &ed.target_path)),
                                                    target_path: ed.target_path,
                                                    icon_key: ed.icon_key,
                                                    args: ed.args,
                                                    work_dir: ed.work_dir,
                                                    remark: ed.remark,
                                                    text_color: ed.text_color,
                                                    minimize_on_run: ed.minimize_on_run,
                                                    custom_icon_16: ed.pending_icon_16,
                                                });
                                            }
                                        }
                                    } else if let Some(cat) = app.categories.get_mut(ed.cat_idx) {
                                        if let Some(grp) = cat.groups.get_mut(ed.grp_idx) {
                                            if let Some(item) = grp.items.get_mut(ed.item_idx) {
                                                item.name = ed.title;
                                                if item.target_path != ed.target_path { item.target_kind = targets::TargetKind::infer(&app.base_dir, &ed.target_path); }
                                                item.target_path = ed.target_path;
                                                item.icon_key = ed.icon_key;
                                                item.text_color = ed.text_color;
                                                item.args = ed.args;
                                                item.work_dir = ed.work_dir;
                                                item.remark = ed.remark;
                                                item.minimize_on_run = ed.minimize_on_run;
                                                if let Some(ico) = ed.pending_icon_16 {
                                                    item.custom_icon_16 = Some(ico);
                                                }
                                            }
                                        }
                                    }
                                    app.hotkey_cfg
                                        .set_hotkey_for_shortcut(&ed.shortcut_id, &ed.hotkey);
                                    app.hotkey_cfg.save(&app.base_dir);
                                    hotkeys::changed(app.main_hwnd);
                                    app.persist_data();
                                    should_close = true;
                                    refresh_main = true;
                                }
                            }

                            HitTarget::AboutLink(l_idx) => {
                                let url = match l_idx {
                                    0 => "http://www.makaix.com",
                                    1 => "http://www.makaix.com",
                                    _ => "http://www.makaix.com",
                                };
                                launch_target_url_or_path(url, "", "", hwnd);
                            }
                            HitTarget::AboutSupportLink => {
                                launch_target_url_or_path(
                                    "http://www.makaix.com",
                                    "",
                                    "",
                                    hwnd,
                                );
                            }
                            HitTarget::AboutCheckUpdate => {
                                deferred_req = DeferredDialogRequest::AboutCheckUpdate;
                            }
                            _ => {}
                        }
                    }
                }
            });

            if skin_hrgn != 0 && main_hwnd != 0 {
                apply_hrgn_safely(main_hwnd, skin_hrgn);
            }

            // 在 RefCell 借用释放后安全弹出原生选择框 / 提示框
            match deferred_req {
                DeferredDialogRequest::EditPickFile => {
                    if let Some(picked) =
                        pick_file_dialog(hwnd, "选择目标文件或程序", &[("所有文件 (*.*)", "*.*")])
                    {
                        let name = picked
                            .file_stem()
                            .or_else(|| picked.file_name())
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or_default();
                        let target_path = picked.to_string_lossy().to_string();
                        let work_dir = picked
                            .parent()
                            .map(|p| p.to_string_lossy().to_string())
                            .unwrap_or_default();
                        let icon_16 = extract_file_icon_16(&picked);
                        APP_STATE.with(|cell| {
                            if let Ok(mut guard) = cell.try_borrow_mut() {
                                if let Some(app) = guard.as_mut() {
                                    if let ActiveModal::EditShortcut(ed) = &mut app.modal {
                                        if ed.title.is_empty() || ed.title == "新建快捷方式" {
                                            ed.title = name;
                                        }
                                        ed.target_path = target_path;
                                        ed.work_dir = work_dir;
                                        if let Some(ico) = icon_16 {
                                            ed.pending_icon_16 = Some(ico);
                                            ed.icon_key = ed.shortcut_id.clone();
                                        }
                                    }
                                }
                            }
                        });
                    }
                }
                DeferredDialogRequest::EditPickDir => {
                    if let Some(picked) = pick_folder_dialog(hwnd, "选择目标目录") {
                        let name = picked
                            .file_name()
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or_else(|| picked.to_string_lossy().to_string());
                        let target_path = picked.to_string_lossy().to_string();
                        let icon_16 = extract_file_icon_16(&picked);
                        APP_STATE.with(|cell| {
                            if let Ok(mut guard) = cell.try_borrow_mut() {
                                if let Some(app) = guard.as_mut() {
                                    if let ActiveModal::EditShortcut(ed) = &mut app.modal {
                                        if ed.title.is_empty() || ed.title == "新建快捷方式" {
                                            ed.title = name;
                                        }
                                        ed.target_path = target_path;
                                        if let Some(ico) = icon_16 {
                                            ed.pending_icon_16 = Some(ico);
                                            ed.icon_key = ed.shortcut_id.clone();
                                        }
                                    }
                                }
                            }
                        });
                    }
                }
                DeferredDialogRequest::EditPickIcon => {
                    if let Some(picked) = pick_file_dialog(
                        hwnd,
                        "选择图标（支持程序或图片）",
                        &[
                            (
                                "程序与图标图片 (*.exe;*.ico;*.png;*.bmp;*.jpg;*.dll)",
                                "*.exe;*.ico;*.png;*.bmp;*.jpg;*.jpeg;*.gif;*.dll",
                            ),
                            ("所有文件 (*.*)", "*.*"),
                        ],
                    ) {
                        if let Some(ico) = load_custom_icon_16(&picked) {
                            APP_STATE.with(|cell| {
                                if let Ok(mut guard) = cell.try_borrow_mut() {
                                    if let Some(app) = guard.as_mut() {
                                        if let ActiveModal::EditShortcut(ed) = &mut app.modal {
                                            ed.pending_icon_16 = Some(ico);
                                            ed.icon_key = ed.shortcut_id.clone();
                                        }
                                    }
                                }
                            });
                        }
                    }
                }
                DeferredDialogRequest::AboutCheckUpdate => {
                    let m = to_wide("当前版本 5.1 Build 2012.8.20 已是最新复刻版！");
                    let t = to_wide("检查升级");
                    MessageBoxW(hwnd, m.as_ptr(), t.as_ptr(), MB_OK | MB_ICONINFORMATION);
                }
                _ => {}
            }

            if should_close {
                close_modal_window();
            } else {
                InvalidateRect(hwnd, ptr::null(), 0);
            }
            if refresh_main && main_hwnd != 0 {
                InvalidateRect(main_hwnd, ptr::null(), 0);
            }
            0
        }

        WM_KEYDOWN => {
            let vk = wparam as u16;
            if vk == VK_ESCAPE {
                close_modal_window();
                return 0;
            }
            APP_STATE.with(|cell| {
                if let Ok(mut guard) = cell.try_borrow_mut() {
                    if let Some(app) = guard.as_mut() {
                        if (VK_F1..=VK_F12).contains(&vk) {
                            let key_name = format!("F{}", vk - VK_F1 + 1);
                            if let ActiveModal::EditShortcut(ed) = &mut app.modal {
                                if ed.active_field == EditShortcutField::Hotkey {
                                    ed.hotkey = key_name;
                                    InvalidateRect(hwnd, ptr::null(), 0);
                                    return;
                                }
                            }
                        }

                        if vk == VK_RETURN {
                            if let ActiveModal::CategoryManager {
                                selected_idx,
                                renaming,
                                rename_buf,
                            } = &mut app.modal
                            {
                                if *renaming {
                                    if let Some(cat) = app.categories.get_mut(*selected_idx) {
                                        if !rename_buf.trim().is_empty() {
                                            cat.name = rename_buf.trim().to_string();
                                        }
                                    }
                                    *renaming = false;
                                    app.persist_data();
                                    if app.main_hwnd != 0 {
                                        InvalidateRect(app.main_hwnd, ptr::null(), 0);
                                    }
                                    InvalidateRect(hwnd, ptr::null(), 0);
                                    return;
                                }
                            }
                            if let ActiveModal::EditShortcut(ed) = &mut app.modal {
                                if ed.active_field == EditShortcutField::Remark {
                                    ed.remark.push_str("\r\n");
                                    InvalidateRect(hwnd, ptr::null(), 0);
                                    return;
                                }
                            }
                        }

                        if vk == VK_BACK {
                            match &mut app.modal {
                                ActiveModal::CategoryManager {
                                    renaming,
                                    rename_buf,
                                    ..
                                } if *renaming => {
                                    rename_buf.pop();
                                }
                                ActiveModal::EditShortcut(ed) => match ed.active_field {
                                    EditShortcutField::Title => {
                                        ed.title.pop();
                                    }
                                    EditShortcutField::TargetPath => {
                                        ed.target_path.pop();
                                    }
                                    EditShortcutField::IconKey => {
                                        ed.icon_key.pop();
                                    }
                                    EditShortcutField::Args => {
                                        ed.args.pop();
                                    }
                                    EditShortcutField::WorkDir => {
                                        ed.work_dir.pop();
                                    }
                                    EditShortcutField::Hotkey => {
                                        ed.hotkey.pop();
                                    }
                                    EditShortcutField::Remark => {
                                        ed.remark.pop();
                                    }
                                },
                                _ => {}
                            }
                            InvalidateRect(hwnd, ptr::null(), 0);
                        }
                    }
                }
            });
            0
        }

        WM_CHAR => {
            if let Some(ch) = char::from_u32(wparam as u32) {
                if !ch.is_control() {
                    APP_STATE.with(|cell| {
                        if let Ok(mut guard) = cell.try_borrow_mut() {
                            if let Some(app) = guard.as_mut() {
                                match &mut app.modal {
                                    ActiveModal::CategoryManager {
                                        renaming,
                                        rename_buf,
                                        ..
                                    } if *renaming => {
                                        rename_buf.push(ch);
                                    }
                                    ActiveModal::EditShortcut(ed) => match ed.active_field {
                                        EditShortcutField::Title => ed.title.push(ch),
                                        EditShortcutField::TargetPath => ed.target_path.push(ch),
                                        EditShortcutField::IconKey => ed.icon_key.push(ch),
                                        EditShortcutField::Args => ed.args.push(ch),
                                        EditShortcutField::WorkDir => ed.work_dir.push(ch),
                                        EditShortcutField::Hotkey => ed.hotkey.push(ch),
                                        EditShortcutField::Remark => ed.remark.push(ch),
                                    },
                                    _ => {}
                                }
                                InvalidateRect(hwnd, ptr::null(), 0);
                            }
                        }
                    });
                }
            }
            0
        }

        WM_PAINT => {
            let mut ps: PAINTSTRUCT = std::mem::zeroed();
            let hdc = BeginPaint(hwnd, &mut ps);
            APP_STATE.with(|cell| {
                if let Ok(mut guard) = cell.try_borrow_mut() {
                    if let Some(app) = guard.as_mut() {
                        let ctx = ModalRenderContext {
                            modal: &app.modal,
                            skin_cfg: &app.skin_cfg,
                            skin_assets: &app.skin_assets,
                            builtin_icons: &app.builtin_icons,
                            categories: &app.categories,
                            active_category: app.active_category,
                            skins: &app.skins,
                            selected_skin_index: app.selected_skin_index,
                            hover_target: app.modal_hover_target,
                        };
                        let texts = render_modal_window_frame(&ctx, &mut app.modal_canvas);
                        let _ = blit_canvas_with_gdi_texts(&app.modal_canvas, &texts, None, hdc, false, None);
                    }
                }
            });
            EndPaint(hwnd, &ps);
            0
        }

        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

/// Windows 原生控件弹窗窗口过程（承载皮肤管理器、类别管理、栏目排序、热键管理、修改快捷方式五大弹窗）
unsafe extern "system" fn native_modal_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let is_advanced = || GetDlgItem(hwnd, advanced_dialog::ID_PAGE) != 0;
    match msg {
        WM_CREATE => {
            let font_name = to_wide("宋体");
            let hfont: HFONT = CreateFontW(
                -12,
                0,
                0,
                0,
                FW_NORMAL as i32,
                0,
                0,
                0,
                GB2312_CHARSET as u32,
                OUT_DEFAULT_PRECIS as u32,
                0,
                DEFAULT_QUALITY as u32,
                0,
                font_name.as_ptr(),
            );
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, hfont as isize);
            if advanced_runtime::populate(hwnd, hfont) {
                return 0;
            }
            let search_query = APP_STATE.with(|cell| {
                let guard = cell.try_borrow().ok()?;
                let ActiveModal::Search(dialog) = &guard.as_ref()?.modal else {
                    return None;
                };
                Some(dialog.initial_query.clone())
            });
            if let Some(query) = search_query {
                search_dialog::populate(hwnd, hfont, &query);
                return 0;
            }
            APP_STATE.with(|cell| {
                if let Ok(guard) = cell.try_borrow() {
                    if let Some(app) = guard.as_ref() {
                        native_dialogs::populate_native_dialog(hwnd, hfont, app);
                    }
                }
            });
            0
        }

        WM_DESTROY => {
            let hfont = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as HFONT;
            if hfont != 0 {
                DeleteObject(hfont as _);
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            }
            0
        }

        WM_CLOSE => {
            close_modal_window();
            0
        }

        WM_ERASEBKGND if is_advanced() => 1,
        WM_CTLCOLORSTATIC if is_advanced() => advanced_style::static_color(wparam as HDC) as LRESULT,
        WM_CTLCOLORSTATIC => {
            let hdc = wparam as HDC;
            SetBkMode(hdc, TRANSPARENT as i32);
            GetSysColorBrush(COLOR_BTNFACE) as LRESULT
        }

        WM_DRAWITEM => {
            let dis = lparam as *const DRAWITEMSTRUCT;
            if !dis.is_null() {
                let d = &*dis;
                let cid = d.CtlID as i32;
                if cid == advanced_dialog::ID_PAGE {
                    advanced_style::draw_navigation(d, GetWindowLongPtrW(hwnd, GWLP_USERDATA) as HFONT);
                    return 1;
                }
                if cid == search_dialog::ID_RESULTS {
                    let font = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as HFONT;
                    APP_STATE.with(|cell| {
                        if let Ok(guard) = cell.try_borrow() {
                            if let Some(app) = guard.as_ref() {
                                if let ActiveModal::Search(dialog) = &app.modal {
                                    search_dialog::draw_result(d, font, dialog.results.get(d.itemID as usize));
                                }
                            }
                        }
                    });
                    return 1;
                }
                if (native_dialogs::ID_EDIT_COLOR_BASE..native_dialogs::ID_EDIT_COLOR_BASE + 8)
                    .contains(&cid)
                {
                    let c = native_dialogs::palette_color((cid - native_dialogs::ID_EDIT_COLOR_BASE) as usize);
                    let brush: HBRUSH = CreateSolidBrush(c.to_colorref());
                    if brush != 0 {
                        FillRect(d.hDC, &d.rcItem, brush);
                        DeleteObject(brush as _);
                    }
                    return 1;
                }
            }
            0
        }

        WM_PAINT => {
            let mut ps: PAINTSTRUCT = std::mem::zeroed();
            let hdc = BeginPaint(hwnd, &mut ps);
            if is_advanced() {
                advanced_dialog::paint(hwnd, hdc, GetWindowLongPtrW(hwnd, GWLP_USERDATA) as HFONT);
                EndPaint(hwnd, &ps);
                return 0;
            }
            APP_STATE.with(|cell| {
                if let Ok(guard) = cell.try_borrow() {
                    if let Some(app) = guard.as_ref() {
                        native_dialogs::paint_native_modal_extras(hwnd, hdc, app);
                    }
                }
            });
            EndPaint(hwnd, &ps);
            0
        }

        WM_NOTIFY => {
            let nm = lparam as *const NMHDR;
            if !nm.is_null() {
                let hdr = &*nm;
                if hdr.idFrom == native_dialogs::ID_EDIT_TAB as usize && hdr.code == TCN_SELCHANGE {
                    let tab_idx = native_dialogs::tab_get_cursel(hwnd, native_dialogs::ID_EDIT_TAB);
                    APP_STATE.with(|cell| {
                        if let Ok(mut guard) = cell.try_borrow_mut() {
                            if let Some(app) = guard.as_mut() {
                                if let ActiveModal::EditShortcut(ref mut ed) = app.modal {
                                    ed.active_tab = tab_idx;
                                }
                            }
                        }
                    });
                    native_dialogs::switch_edit_tab_visibility(hwnd, tab_idx);
                }
            }
            0
        }

        WM_COMMAND => {
            let ctrl_id = (wparam & 0xFFFF) as u16 as i32;
            let notif = ((wparam >> 16) & 0xFFFF) as u16 as u32;
            if advanced_runtime::command(hwnd, ctrl_id, notif) {
                return 0;
            }
            if ctrl_id == search_dialog::ID_SUBMIT {
                refresh_search_dialog(hwnd);
                return 0;
            }
            if ctrl_id == search_dialog::ID_RESULTS && notif == LBN_DBLCLK {
                open_search_result(hwnd);
                return 0;
            }
            let mut close_after = false;
            let mut need_persist = false;
            let mut skin_hrgn: HRGN = 0;
            let mut main_hwnd: HWND = 0;
            let mut deferred = DeferredDialogRequest::None;

            APP_STATE.with(|cell| {
                if let Ok(mut guard) = cell.try_borrow_mut() {
                    if let Some(app) = guard.as_mut() {
                        main_hwnd = app.main_hwnd;
                        if ctrl_id == IDCANCEL {
                            close_after = true;
                            return;
                        }

                        match &mut app.modal {
                            ActiveModal::SkinManager => {
                                if ctrl_id == native_dialogs::ID_SKIN_LIST {
                                    if notif == LBN_SELCHANGE {
                                        if let Some(sel) =
                                            native_dialogs::listbox_get_cursel(hwnd, ctrl_id)
                                        {
                                            if sel < app.skins.len() {
                                                app.selected_skin_index = sel;
                                                let author = app.skins[sel].author_info.clone();
                                                native_dialogs::set_ctrl_text(
                                                    hwnd,
                                                    native_dialogs::ID_SKIN_AUTHOR_EDIT,
                                                    &author,
                                                );
                                                InvalidateRect(hwnd, ptr::null(), 1);
                                            }
                                        }
                                    } else if notif == LBN_DBLCLK {
                                        if let Some(rgn) = app.switch_skin(app.selected_skin_index) {
                                            skin_hrgn = rgn;
                                        }
                                    }
                                } else if ctrl_id == native_dialogs::ID_SKIN_APPLY {
                                    if let Some(rgn) = app.switch_skin(app.selected_skin_index) {
                                        skin_hrgn = rgn;
                                    }
                                }
                            }

                            ActiveModal::CategoryManager {
                                selected_idx,
                                renaming,
                                ..
                            } => {
                                let lb = GetDlgItem(hwnd, native_dialogs::ID_CAT_LIST);
                                if ctrl_id == native_dialogs::ID_CAT_LIST && notif == LBN_SELCHANGE {
                                    if let Some(sel) =
                                        native_dialogs::listbox_get_cursel(hwnd, ctrl_id)
                                    {
                                        *selected_idx = sel.min(app.categories.len().saturating_sub(1));
                                        *renaming = false;
                                        let ed = GetDlgItem(hwnd, native_dialogs::ID_CAT_RENAME_EDIT);
                                        let ok = GetDlgItem(hwnd, native_dialogs::ID_CAT_RENAME_OK);
                                        ShowWindow(ed, SW_HIDE);
                                        ShowWindow(ok, SW_HIDE);
                                        InvalidateRect(hwnd, ptr::null(), 1);
                                    }
                                } else if ctrl_id == native_dialogs::ID_CAT_BTN_NEW {
                                    let next_n = app.categories.len() + 1;
                                    let new_id = format!("cat_{}", next_n);
                                    let col_id = format!(
                                        "col_{}",
                                        app.data_store.columns.len() + next_n
                                    );
                                    app.categories.push(CategoryTab {
                                        id: new_id.clone(),
                                        name: format!("新分类{}", app.categories.len() + 1),
                                        icon_name: "cat_laptop".to_string(),
                                        encrypted: false,
                                        groups: vec![ShortcutGroup {
                                            id: col_id,
                                            category_id: new_id,
                                            title: "默认栏目".to_string(),
                                            encrypted: false,
                                            items: Vec::new(),
                                        }],
                                        active_group_index: 0,
                                    });
                                    *selected_idx = app.categories.len() - 1;
                                    need_persist = true;
                                    native_dialogs::refresh_category_listbox(
                                        lb,
                                        &app.categories,
                                        *selected_idx,
                                    );
                                    InvalidateRect(hwnd, ptr::null(), 1);
                                } else if ctrl_id == native_dialogs::ID_CAT_BTN_DEL {
                                    if app.categories.len() > 1 && *selected_idx < app.categories.len() {
                                        app.categories.remove(*selected_idx);
                                        if *selected_idx >= app.categories.len() {
                                            *selected_idx = app.categories.len() - 1;
                                        }
                                        if app.active_category >= app.categories.len() {
                                            app.active_category = app.categories.len() - 1;
                                        }
                                        need_persist = true;
                                        native_dialogs::refresh_category_listbox(
                                            lb,
                                            &app.categories,
                                            *selected_idx,
                                        );
                                        InvalidateRect(hwnd, ptr::null(), 1);
                                    }
                                } else if ctrl_id == native_dialogs::ID_CAT_BTN_RENAME {
                                    if let Some(cat) = app.categories.get(*selected_idx) {
                                        *renaming = true;
                                        let ed = GetDlgItem(hwnd, native_dialogs::ID_CAT_RENAME_EDIT);
                                        let ok = GetDlgItem(hwnd, native_dialogs::ID_CAT_RENAME_OK);
                                        native_dialogs::set_ctrl_text(
                                            hwnd,
                                            native_dialogs::ID_CAT_RENAME_EDIT,
                                            &cat.name,
                                        );
                                        ShowWindow(ed, SW_SHOW);
                                        ShowWindow(ok, SW_SHOW);
                                        SetFocus(ed);
                                    }
                                } else if ctrl_id == native_dialogs::ID_CAT_RENAME_OK {
                                    let new_name = native_dialogs::get_ctrl_text(
                                        hwnd,
                                        native_dialogs::ID_CAT_RENAME_EDIT,
                                    );
                                    if let Some(cat) = app.categories.get_mut(*selected_idx) {
                                        if !new_name.trim().is_empty() {
                                            cat.name = new_name.trim().to_string();
                                            need_persist = true;
                                        }
                                    }
                                    *renaming = false;
                                    ShowWindow(GetDlgItem(hwnd, native_dialogs::ID_CAT_RENAME_EDIT), SW_HIDE);
                                    ShowWindow(GetDlgItem(hwnd, native_dialogs::ID_CAT_RENAME_OK), SW_HIDE);
                                    native_dialogs::refresh_category_listbox(
                                        lb,
                                        &app.categories,
                                        *selected_idx,
                                    );
                                } else if ctrl_id == native_dialogs::ID_CAT_BTN_ENC {
                                    if let Some(cat) = app.categories.get_mut(*selected_idx) {
                                        cat.encrypted = !cat.encrypted;
                                        need_persist = true;
                                    }
                                    native_dialogs::refresh_category_listbox(
                                        lb,
                                        &app.categories,
                                        *selected_idx,
                                    );
                                } else if ctrl_id == native_dialogs::ID_CAT_BTN_UP {
                                    if *selected_idx > 0 && *selected_idx < app.categories.len() {
                                        app.categories.swap(*selected_idx, *selected_idx - 1);
                                        *selected_idx -= 1;
                                        need_persist = true;
                                        native_dialogs::refresh_category_listbox(
                                            lb,
                                            &app.categories,
                                            *selected_idx,
                                        );
                                    }
                                } else if ctrl_id == native_dialogs::ID_CAT_BTN_DOWN {
                                    if *selected_idx + 1 < app.categories.len() {
                                        app.categories.swap(*selected_idx, *selected_idx + 1);
                                        *selected_idx += 1;
                                        need_persist = true;
                                        native_dialogs::refresh_category_listbox(
                                            lb,
                                            &app.categories,
                                            *selected_idx,
                                        );
                                    }
                                } else if ctrl_id == native_dialogs::ID_CAT_BTN_ICON {
                                    if let Some(cat) = app.categories.get_mut(*selected_idx) {
                                        cat.icon_name = if cat.icon_name.contains("cube") {
                                            "cat_laptop".to_string()
                                        } else {
                                            "cat_cube".to_string()
                                        };
                                        need_persist = true;
                                    }
                                    InvalidateRect(hwnd, ptr::null(), 1);
                                }
                            }

                            ActiveModal::ColumnSort { selected_idx, .. } => {
                                let lb = GetDlgItem(hwnd, native_dialogs::ID_COL_LIST);
                                let cat_idx = app.active_category;
                                if ctrl_id == native_dialogs::ID_COL_LIST && notif == LBN_SELCHANGE {
                                    if let Some(sel) =
                                        native_dialogs::listbox_get_cursel(hwnd, ctrl_id)
                                    {
                                        *selected_idx = sel;
                                    }
                                } else if ctrl_id == native_dialogs::ID_COL_BTN_UP {
                                    if let Some(cat) = app.categories.get_mut(cat_idx) {
                                        if *selected_idx > 0 && *selected_idx < cat.groups.len() {
                                            cat.groups.swap(*selected_idx, *selected_idx - 1);
                                            *selected_idx -= 1;
                                            cat.active_group_index = *selected_idx;
                                            need_persist = true;
                                            native_dialogs::refresh_column_listbox(
                                                lb,
                                                cat,
                                                *selected_idx,
                                            );
                                        }
                                    }
                                } else if ctrl_id == native_dialogs::ID_COL_BTN_DOWN {
                                    if let Some(cat) = app.categories.get_mut(cat_idx) {
                                        if *selected_idx + 1 < cat.groups.len() {
                                            cat.groups.swap(*selected_idx, *selected_idx + 1);
                                            *selected_idx += 1;
                                            cat.active_group_index = *selected_idx;
                                            need_persist = true;
                                            native_dialogs::refresh_column_listbox(
                                                lb,
                                                cat,
                                                *selected_idx,
                                            );
                                        }
                                    }
                                }
                            }

                            ActiveModal::EditShortcut(ed) => {
                                if ctrl_id == IDOK {
                                    let ed_copy = ed.clone();
                                    native_dialogs::commit_edit_shortcut_from_controls(
                                        hwnd, app, ed_copy,
                                    );
                                    close_after = true;
                                } else if ctrl_id == native_dialogs::ID_EDIT_PICK_FILE {
                                    deferred = DeferredDialogRequest::EditPickFile;
                                } else if ctrl_id == native_dialogs::ID_EDIT_PICK_DIR {
                                    deferred = DeferredDialogRequest::EditPickDir;
                                } else if ctrl_id == native_dialogs::ID_EDIT_PICK_ICON {
                                    deferred = DeferredDialogRequest::EditPickIcon;
                                } else if (native_dialogs::ID_EDIT_COLOR_BASE
                                    ..native_dialogs::ID_EDIT_COLOR_BASE + 8)
                                    .contains(&ctrl_id)
                                {
                                    let idx = (ctrl_id - native_dialogs::ID_EDIT_COLOR_BASE) as usize;
                                    ed.text_color = native_dialogs::palette_color(idx);
                                    InvalidateRect(hwnd, ptr::null(), 1);
                                }
                            }

                            _ => {}
                        }
                        if need_persist {
                            app.persist_data();
                        }
                    }
                }
            });

            if skin_hrgn != 0 && main_hwnd != 0 {
                apply_hrgn_safely(main_hwnd, skin_hrgn);
                InvalidateRect(main_hwnd, ptr::null(), 0);
            }

            // 借用释放后再弹出原生文件/目录/图标选择框
            match deferred {
                DeferredDialogRequest::EditPickFile => {
                    if let Some(picked) = pick_file_dialog(
                        hwnd,
                        "选择快捷方式目标文件或程序",
                        &[
                            ("所有文件 (*.*)", "*.*"),
                            ("可执行程序 (*.exe;*.lnk)", "*.exe;*.lnk"),
                        ],
                    ) {
                        let path_str = picked.to_string_lossy().to_string();
                        let stem = picked
                            .file_stem()
                            .or_else(|| picked.file_name())
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or_default();
                        let work = picked
                            .parent()
                            .map(|p| p.to_string_lossy().to_string())
                            .unwrap_or_default();
                        let icon_16 = extract_file_icon_16(&picked);
                        native_dialogs::set_ctrl_text(hwnd, native_dialogs::ID_EDIT_PATH, &path_str);
                        if native_dialogs::get_ctrl_text(hwnd, native_dialogs::ID_EDIT_TITLE)
                            .trim()
                            .is_empty()
                            && !stem.is_empty()
                        {
                            native_dialogs::set_ctrl_text(
                                hwnd,
                                native_dialogs::ID_EDIT_TITLE,
                                &stem,
                            );
                        }
                        if native_dialogs::get_ctrl_text(hwnd, native_dialogs::ID_EDIT_DIR)
                            .trim()
                            .is_empty()
                            && !work.is_empty()
                        {
                            native_dialogs::set_ctrl_text(hwnd, native_dialogs::ID_EDIT_DIR, &work);
                        }
                        APP_STATE.with(|cell| {
                            if let Ok(mut guard) = cell.try_borrow_mut() {
                                if let Some(app) = guard.as_mut() {
                                    if let ActiveModal::EditShortcut(ref mut ed) = app.modal {
                                        ed.pending_icon_16 = icon_16;
                                    }
                                }
                            }
                        });
                        InvalidateRect(hwnd, ptr::null(), 1);
                    }
                }
                DeferredDialogRequest::EditPickDir => {
                    if let Some(picked) = pick_folder_dialog(hwnd, "选择目标目录文件夹") {
                        let path_str = picked.to_string_lossy().to_string();
                        let name = picked
                            .file_name()
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or_else(|| path_str.clone());
                        let icon_16 = extract_file_icon_16(&picked);
                        native_dialogs::set_ctrl_text(hwnd, native_dialogs::ID_EDIT_PATH, &path_str);
                        if native_dialogs::get_ctrl_text(hwnd, native_dialogs::ID_EDIT_TITLE)
                            .trim()
                            .is_empty()
                        {
                            native_dialogs::set_ctrl_text(
                                hwnd,
                                native_dialogs::ID_EDIT_TITLE,
                                &name,
                            );
                        }
                        APP_STATE.with(|cell| {
                            if let Ok(mut guard) = cell.try_borrow_mut() {
                                if let Some(app) = guard.as_mut() {
                                    if let ActiveModal::EditShortcut(ref mut ed) = app.modal {
                                        ed.pending_icon_16 = icon_16;
                                    }
                                }
                            }
                        });
                        InvalidateRect(hwnd, ptr::null(), 1);
                    }
                }
                DeferredDialogRequest::EditPickIcon => {
                    if let Some(picked) = pick_file_dialog(
                        hwnd,
                        "选择程序或图片作为图标",
                        &[
                            (
                                "程序与图片 (*.exe;*.ico;*.png;*.bmp;*.jpg;*.gif)",
                                "*.exe;*.ico;*.png;*.bmp;*.jpg;*.gif",
                            ),
                            ("所有文件 (*.*)", "*.*"),
                        ],
                    ) {
                        if let Some(icon_16) = load_custom_icon_16(&picked) {
                            let path_str = picked.to_string_lossy().to_string();
                            native_dialogs::set_ctrl_text(
                                hwnd,
                                native_dialogs::ID_EDIT_ICON,
                                &path_str,
                            );
                            APP_STATE.with(|cell| {
                                if let Ok(mut guard) = cell.try_borrow_mut() {
                                    if let Some(app) = guard.as_mut() {
                                        if let ActiveModal::EditShortcut(ref mut ed) = app.modal {
                                            ed.pending_icon_16 = Some(icon_16);
                                        }
                                    }
                                }
                            });
                            InvalidateRect(hwnd, ptr::null(), 1);
                        }
                    }
                }
                _ => {}
            }

            if close_after {
                close_modal_window();
            } else if main_hwnd != 0 {
                InvalidateRect(main_hwnd, ptr::null(), 0);
            }
            0
        }

        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

/// 定位项目根目录
fn locate_base_dir() -> PathBuf {
    if let Ok(cwd) = env::current_dir() {
        if cwd.join("skin").exists() {
            return cwd;
        }
    }
    if let Ok(exe_path) = env::current_exe() {
        let mut cur = exe_path.parent();
        while let Some(dir) = cur {
            if dir.join("skin").exists() {
                return dir.to_path_buf();
            }
            cur = dir.parent();
        }
    }
    PathBuf::from("D:\\code\\KStart")
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let base_dir = args.windows(2).find(|pair| pair[0] == "--base-dir")
        .map(|pair| PathBuf::from(&pair[1])).unwrap_or_else(locate_base_dir);

    let mut dump_path: Option<PathBuf> = None;
    let initial_menu = if args.iter().any(|a| a == "--open-menu") {
        ActivePopupMenu::MainMenu
    } else if args.iter().any(|a| a == "--open-ctx-menu") {
        ActivePopupMenu::ContextMenu { x: 134, y: 329 }
    } else if args.iter().any(|a| a == "--open-item-menu") {
        ActivePopupMenu::ItemMenu {
            x: 206,
            y: 171,
            target_shortcut: 5,
        }
    } else if args.iter().any(|a| a == "--open-skin-menu") {
        ActivePopupMenu::SkinQuickMenu
    } else {
        ActivePopupMenu::None
    };

    for i in 0..args.len() {
        if args[i] == "--dump-screenshot" && i + 1 < args.len() {
            dump_path = Some(PathBuf::from(&args[i + 1]));
        }
    }

    let mut app_state = match AppState::new(base_dir, initial_menu) {
        Ok(s) => s,
        Err(err) => {
            eprintln!("初始化失败: {}", err);
            return;
        }
    };

    if args.iter().any(|a| a == "--cat2") && app_state.categories.len() > 1 {
        app_state.active_category = 1;
    }
    if args.iter().any(|a| a == "--demo-marquee") {
        app_state.marquee_rect = Some((58, 148, 300, 250));
    }
    if args.iter().any(|a| a == "--demo-selected") {
        for i in 2..=8 {
            app_state.selected_shortcuts.insert(i);
        }
    }

    // 处理独立弹窗截图导出模式
    let dump_modal: Option<ActiveModal> = if args.iter().any(|a| a == "--modal-about") {
        Some(ActiveModal::About)
    } else if args.iter().any(|a| a == "--modal-edit") {
        app_state.make_edit_shortcut_modal(0, 0, 3)
    } else if args.iter().any(|a| a == "--modal-edit-remark") {
        app_state.make_edit_shortcut_modal(0, 0, 3).map(|mut m| {
            if let ActiveModal::EditShortcut(ref mut ed) = m {
                ed.active_tab = 1;
                ed.active_field = EditShortcutField::Remark;
            }
            m
        })
    } else if args.iter().any(|a| a == "--modal-cat") {
        Some(ActiveModal::CategoryManager {
            selected_idx: 0,
            renaming: false,
            rename_buf: String::new(),
        })
    } else {
        None
    };

    if let Some(out_file) = dump_path {
        app_state.time_string = "11月18日 星期三 15:18".to_string();
        if let Some(m) = dump_modal {
            app_state.modal = m;
            let ctx = ModalRenderContext {
                modal: &app_state.modal,
                skin_cfg: &app_state.skin_cfg,
                skin_assets: &app_state.skin_assets,
                builtin_icons: &app_state.builtin_icons,
                categories: &app_state.categories,
                active_category: app_state.active_category,
                skins: &app_state.skins,
                selected_skin_index: app_state.selected_skin_index,
                hover_target: HitTarget::None,
            };
            let texts = render_modal_window_frame(&ctx, &mut app_state.modal_canvas);
            if let Some(bgra) = blit_canvas_with_gdi_texts(&app_state.modal_canvas, &texts, None, 0, true, None)
            {
                let _ = save_bgra_to_png(
                    app_state.modal_canvas.width as u32,
                    app_state.modal_canvas.height as u32,
                    bgra,
                    &out_file,
                );
                println!("已导出独立弹窗截图至: {:?}", out_file);
            }
            return;
        } else {
            let rstate = RenderState {
                win_w: app_state.user_cfg.window_width,
                win_h: app_state.user_cfg.window_height,
                skin_cfg: &app_state.skin_cfg,
                skin_assets: &app_state.skin_assets,
                builtin_icons: &app_state.builtin_icons,
                user_cfg: &app_state.user_cfg,
                categories: &app_state.categories,
                active_category: app_state.active_category,
                hover_target: HitTarget::None,
                down_target: HitTarget::None,
                time_string: &app_state.time_string,
                list_background: app_state.list_background.as_ref(),
                shortcut_scroll: app_state.advanced_window.shortcut_scroll,
                volume_level: None,
                tooltip: None,
                search_query: &app_state.search_query,
                search_focused: app_state.search_focused,
                popup_menu: app_state.popup_menu,
                menu_scroll: app_state.menu_scroll,
                skins: &app_state.skins,
                active_skin_index: app_state.active_skin_index,
                selected_skin_index: app_state.selected_skin_index,
                dragging_shortcut: app_state.dragging_shortcut,
                marquee_rect: app_state.marquee_rect,
                selected_shortcuts: &app_state.selected_shortcuts,
            };
            let (texts, popup_clip) = render_main_frame(&rstate, &mut app_state.canvas);
            if let Some(bgra) = blit_canvas_with_gdi_texts(&app_state.canvas, &texts, popup_clip, 0, true,
                Some(&app_state.user_cfg.advanced.fonts)) {
                let _ = save_bgra_to_png(
                    app_state.canvas.width as u32,
                    app_state.canvas.height as u32,
                    bgra,
                    &out_file,
                );
                println!("已导出主窗体截图至: {:?}", out_file);
            }
            return;
        }
    }

    if unsafe { !advanced_prompt::authenticate(0, &app_state.base_dir, false) } { return; }
    let win_w = app_state.user_cfg.window_width;
    let win_h = app_state.user_cfg.window_height;
    let always_on_top = app_state.user_cfg.always_on_top;

    APP_STATE.with(|cell| {
        *cell.borrow_mut() = Some(app_state);
    });

    unsafe {
        let icc = INITCOMMONCONTROLSEX {
            dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_TAB_CLASSES | ICC_LISTVIEW_CLASSES,
        };
        InitCommonControlsEx(&icc);

        let hinstance = GetModuleHandleW(ptr::null());

        // 注册主窗口类
        let main_cls = to_wide("KStartKStart5WindowClass");
        let wc_main = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW | CS_DBLCLKS,
            lpfnWndProc: Some(main_wnd_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinstance,
            hIcon: 0,
            hCursor: LoadCursorW(0, IDC_ARROW),
            hbrBackground: 0,
            lpszMenuName: ptr::null(),
            lpszClassName: main_cls.as_ptr(),
        };
        RegisterClassW(&wc_main);
        popup::register(hinstance);

        // 注册独立屏幕居中弹窗窗口类，供自绘弹窗和原生搜索窗口使用。
        let modal_cls = to_wide(MODAL_CLASS_NAME);
        let wc_modal = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(modal_wnd_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinstance,
            hIcon: 0,
            hCursor: LoadCursorW(0, IDC_ARROW),
            hbrBackground: (COLOR_BTNFACE + 1) as HBRUSH,
            lpszMenuName: ptr::null(),
            lpszClassName: modal_cls.as_ptr(),
        };
        RegisterClassW(&wc_modal);

        // 注册 Windows 原生控件管理弹窗窗口类
        let native_cls = to_wide(NATIVE_MODAL_CLASS_NAME);
        let wc_native = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(native_modal_wnd_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinstance,
            hIcon: 0,
            hCursor: LoadCursorW(0, IDC_ARROW),
            hbrBackground: (COLOR_BTNFACE + 1) as HBRUSH,
            lpszMenuName: ptr::null(),
            lpszClassName: native_cls.as_ptr(),
        };
        RegisterClassW(&wc_native);

        let screen_w = GetSystemMetrics(SM_CXSCREEN);
        let screen_h = GetSystemMetrics(SM_CYSCREEN);
        let start_x = ((screen_w - win_w) / 2).max(20);
        let start_y = ((screen_h - win_h) / 2).max(20);

        let ex_style = if always_on_top {
            WS_EX_TOPMOST
        } else {
            WS_EX_TOOLWINDOW
        };
        let win_title = to_wide("快速启动 5");

        let hwnd = CreateWindowExW(
            ex_style,
            main_cls.as_ptr(),
            win_title.as_ptr(),
            WS_POPUP | WS_THICKFRAME | WS_VISIBLE | WS_SYSMENU | WS_MINIMIZEBOX,
            start_x,
            start_y,
            win_w,
            win_h,
            0,
            0,
            hinstance,
            ptr::null(),
        );

        if hwnd == 0 {
            return;
        }

        DragAcceptFiles(hwnd, 1);

        APP_STATE.with(|cell| {
            if let Ok(mut guard) = cell.try_borrow_mut() {
                if let Some(app) = guard.as_mut() {
                    app.main_hwnd = hwnd;
                }
            }
        });
        let mut init_hrgn: HRGN = 0;
        APP_STATE.with(|cell| {
            if let Ok(mut guard) = cell.try_borrow_mut() {
                if let Some(app) = guard.as_mut() {
                    if app.applied_rgn_size != (win_w, win_h) {
                        app.applied_rgn_size = (win_w, win_h);
                        init_hrgn = build_window_corner_region(
                            win_w,
                            win_h,
                            &app.skin_cfg,
                            &app.skin_assets,
                        );
                    }
                }
            }
        });
        apply_hrgn_safely(hwnd, init_hrgn);

        SetTimer(hwnd, 1, 10_000, None);
        SetTimer(hwnd, advanced_window::TIMER_ID, 100, None);
        ShowWindow(hwnd, SW_SHOWNORMAL);
        popup::sync(hwnd);
        advanced_window::update_tray(hwnd);
        hotkeys::sync(hwnd);
        APP_STATE.with(|cell| {
            if let Ok(guard) = cell.try_borrow() {
                if let Some(app) = guard.as_ref() {
                    let _ = advanced_runtime::process_options(&app.user_cfg.advanced, &advanced::AdvancedSettings::default());
                }
            }
        });

        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, 0, 0, 0) > 0 {
            let mut active_native_dlg: HWND = 0;
            let mut is_search_dialog = false;
            APP_STATE.with(|cell| {
                if let Ok(guard) = cell.try_borrow() {
                    if let Some(app) = guard.as_ref() {
                        if app.modal_is_native && app.modal_hwnd != 0 {
                            active_native_dlg = app.modal_hwnd;
                            is_search_dialog = matches!(app.modal, ActiveModal::Search(_));
                        }
                    }
                }
            });
            if is_search_dialog && handle_search_dialog_key(active_native_dlg, &msg) {
                continue;
            }
            if active_native_dlg != 0 && IsDialogMessageW(active_native_dlg, &msg) != 0 {
                continue;
            }
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        advanced_runtime::backup_on_exit();
    }
}
