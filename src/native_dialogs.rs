//! Windows 原生控件管理弹窗模块
//! 将【修改快捷方式】【热键管理】【类别管理】【栏目排序】【皮肤管理器】五大管理弹窗
//! 从纯软件自绘迁移为 Windows 原生标准控件（LISTBOX / EDIT / BUTTON / SysTabControl32），
//! 天然支持键盘上下键切换、原生滚动条、Tab 焦点流转及系统中文输入法 (IME)。

use crate::model::{CategoryTab, ShortcutItem};
use crate::renderer::{ActiveModal, EditShortcutDialogState};
use crate::skin_assets::RgbaSurface;
use crate::{to_wide, AppState};
use std::ptr;

use windows_sys::Win32::Foundation::{HWND, RECT};
use windows_sys::Win32::Graphics::Gdi::{
    FillRect, InvalidateRect, SetBkMode, SetDIBitsToDevice,
    BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HDC, HFONT,
    TRANSPARENT,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Controls::{
    TCIF_TEXT, TCITEMW, TCM_GETCURSEL, TCM_INSERTITEMW, WC_TABCONTROLW,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, GetDlgItem, GetWindowTextLengthW, GetWindowTextW, SendMessageW,
    SetWindowTextW, ShowWindow, BM_GETCHECK, BM_SETCHECK, BS_AUTOCHECKBOX, BS_DEFPUSHBUTTON,
    BS_OWNERDRAW, BS_PUSHBUTTON, ES_AUTOHSCROLL, ES_AUTOVSCROLL, ES_MULTILINE, ES_READONLY,
    ES_WANTRETURN, IDCANCEL, IDOK, LBS_NOINTEGRALHEIGHT, LBS_NOTIFY, LB_ADDSTRING, LB_GETCURSEL,
    LB_RESETCONTENT, LB_SETCURSEL, SW_HIDE, SW_SHOW, WM_SETFONT, WS_BORDER, WS_CHILD,
    WS_EX_CLIENTEDGE, WS_TABSTOP, WS_VISIBLE, WS_VSCROLL,
};

// ==========================================
// 原生子控件 ID 常量定义
// ==========================================
pub const ID_SKIN_LIST: i32 = 1001;
pub const ID_SKIN_AUTHOR_EDIT: i32 = 1002;
pub const ID_SKIN_APPLY: i32 = 1003;

pub const ID_CAT_BTN_NEW: i32 = 1101;
pub const ID_CAT_BTN_DEL: i32 = 1102;
pub const ID_CAT_BTN_RENAME: i32 = 1103;
pub const ID_CAT_BTN_ENC: i32 = 1104;
pub const ID_CAT_BTN_UP: i32 = 1105;
pub const ID_CAT_BTN_DOWN: i32 = 1106;
pub const ID_CAT_LIST: i32 = 1107;
pub const ID_CAT_RENAME_EDIT: i32 = 1108;
pub const ID_CAT_RENAME_OK: i32 = 1109;
pub const ID_CAT_BTN_ICON: i32 = 1110;
pub const ID_CAT_ADD_ICON: i32 = 1111;
pub const ID_CAT_MANAGE_ICONS: i32 = 1112;

pub const ID_COL_LIST: i32 = 1201;
pub const ID_COL_BTN_UP: i32 = 1202;
pub const ID_COL_BTN_DOWN: i32 = 1203;


pub const ID_EDIT_TAB: i32 = 1400;
pub const ID_EDIT_LBL_TITLE: i32 = 1401;
pub const ID_EDIT_TITLE: i32 = 1402;
pub const ID_EDIT_LBL_LINK: i32 = 1403;
pub const ID_EDIT_PICK_FILE: i32 = 1404;
pub const ID_EDIT_PICK_DIR: i32 = 1405;
pub const ID_EDIT_PATH: i32 = 1406;
pub const ID_EDIT_LBL_ICON: i32 = 1407;
pub const ID_EDIT_ICON: i32 = 1408;
pub const ID_EDIT_PICK_ICON: i32 = 1409;
pub const ID_EDIT_ICON_PREVIEW: i32 = 1499;
pub const ID_EDIT_LBL_COLOR: i32 = 1410;
pub const ID_EDIT_LBL_ARGS: i32 = 1411;
pub const ID_EDIT_ARGS: i32 = 1412;
pub const ID_EDIT_LBL_DIR: i32 = 1413;
pub const ID_EDIT_DIR: i32 = 1414;
pub const ID_EDIT_LBL_HK: i32 = 1415;
pub const ID_EDIT_HK: i32 = 1416;
pub const ID_EDIT_CHK_MIN: i32 = 1417;
pub const ID_EDIT_REMARK: i32 = 1418;
pub const ID_EDIT_COLOR_BASE: i32 = 1450; // 统一文字颜色选择器

/// 弹窗客户区基础宽高 `(width, height)` 与标题
pub fn native_modal_client_info(modal: &ActiveModal) -> (i32, i32, &'static str) {
    match modal {
        ActiveModal::Advanced(_) => (crate::advanced_dialog::WIDTH, crate::advanced_dialog::HEIGHT, "高级设置"),
        ActiveModal::SkinManager => (430, 390, "皮肤管理器"),
        ActiveModal::CategoryManager { .. } => (410, 438, "类别管理"),
        ActiveModal::ColumnSort { .. } => (350, 320, "栏目排序"),
        ActiveModal::Search(_) => (crate::search_dialog::WIDTH, crate::search_dialog::HEIGHT, "搜索"),
        ActiveModal::EditShortcut(ed) => {
            if ed.is_new {
                (380, 400, "新建快捷方式")
            } else {
                (380, 400, "修改快捷方式")
            }
        }
        _ => (0, 0, ""),
    }
}

/// 创建原生子控件并统一样式字体
pub(crate) unsafe fn create_ctrl(
    parent: HWND,
    hfont: HFONT,
    cls: &str,
    text: &str,
    ex_style: u32,
    style: u32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    id: i32,
) -> HWND {
    let hinstance = GetModuleHandleW(ptr::null());
    let w_cls = to_wide(cls);
    let w_txt = to_wide(text);
    let ctrl = CreateWindowExW(
        ex_style,
        w_cls.as_ptr(),
        w_txt.as_ptr(),
        WS_CHILD | WS_VISIBLE | style,
        x,
        y,
        w,
        h,
        parent,
        id as isize,
        hinstance,
        ptr::null(),
    );
    if ctrl != 0 && hfont != 0 {
        SendMessageW(ctrl, WM_SETFONT, hfont as usize, 1);
    }
    if cls.eq_ignore_ascii_case("BUTTON") {
        let kind = style & 0xf;
        if kind <= BS_AUTOCHECKBOX as u32 {
            crate::advanced_style::button(ctrl, kind == BS_AUTOCHECKBOX as u32 || kind == 2,
                id == IDOK);
        }
    }
    ctrl
}

/// 读取控件文本
pub unsafe fn get_ctrl_text(parent: HWND, id: i32) -> String {
    let h = GetDlgItem(parent, id);
    if h == 0 {
        return String::new();
    }
    let len = GetWindowTextLengthW(h);
    if len <= 0 {
        return String::new();
    }
    let mut buf = vec![0u16; (len + 1) as usize];
    let copied = GetWindowTextW(h, buf.as_mut_ptr(), len + 1);
    String::from_utf16_lossy(&buf[..copied.max(0) as usize])
}

/// 设置控件文本
pub unsafe fn set_ctrl_text(parent: HWND, id: i32, text: &str) {
    let h = GetDlgItem(parent, id);
    if h != 0 {
        let w = to_wide(text);
        SetWindowTextW(h, w.as_ptr());
    }
}

/// 向 ListBox 追加条目
unsafe fn listbox_add(lb: HWND, text: &str) {
    let w = to_wide(text);
    SendMessageW(lb, LB_ADDSTRING, 0, w.as_ptr() as isize);
}

/// 在原生窗口上创建对应弹窗的全部子控件
pub unsafe fn populate_native_dialog(hwnd: HWND, hfont: HFONT, app: &AppState) {
    match &app.modal {
        ActiveModal::SkinManager => {
            let lb = create_ctrl(
                hwnd,
                hfont,
                "LISTBOX",
                "",
                WS_EX_CLIENTEDGE,
                WS_BORDER | WS_VSCROLL | WS_TABSTOP | (LBS_NOTIFY as u32) | (LBS_NOINTEGRALHEIGHT as u32),
                12,
                12,
                176,
                326,
                ID_SKIN_LIST,
            );
            for s in &app.skins {
                listbox_add(lb, &s.name);
            }
            if !app.skins.is_empty() {
                let sel = app.selected_skin_index.min(app.skins.len() - 1);
                SendMessageW(lb, LB_SETCURSEL, sel, 0);
            }

            create_ctrl(
                hwnd,
                hfont,
                "EDIT",
                app.skins
                    .get(app.selected_skin_index)
                    .map(|s| s.author_info.as_str())
                    .unwrap_or(""),
                WS_EX_CLIENTEDGE,
                (ES_MULTILINE as u32) | (ES_READONLY as u32) | (ES_AUTOVSCROLL as u32) | WS_VSCROLL,
                200,
                244,
                218,
                94,
                ID_SKIN_AUTHOR_EDIT,
            );

            create_ctrl(
                hwnd,
                hfont,
                "BUTTON",
                "应用(&A)",
                0,
                WS_TABSTOP | (BS_DEFPUSHBUTTON as u32),
                252,
                350,
                78,
                26,
                ID_SKIN_APPLY,
            );
            create_ctrl(
                hwnd,
                hfont,
                "BUTTON",
                "关闭(&C)",
                0,
                WS_TABSTOP | (BS_PUSHBUTTON as u32),
                340,
                350,
                78,
                26,
                IDCANCEL,
            );
            SetFocus(lb);
        }

        ActiveModal::CategoryManager { selected_idx, .. } => {
            crate::category_dialog::populate(hwnd, hfont, app, *selected_idx);
        }

        ActiveModal::ColumnSort { selected_idx, .. } => {
            create_ctrl(
                hwnd,
                hfont,
                "STATIC",
                "请选择栏目并使用【上移】/【下移】调整栏目顺序：",
                0,
                0,
                12,
                12,
                326,
                18,
                -1,
            );
            let lb = create_ctrl(
                hwnd,
                hfont,
                "LISTBOX",
                "",
                WS_EX_CLIENTEDGE,
                WS_BORDER | WS_VSCROLL | WS_TABSTOP | (LBS_NOTIFY as u32) | (LBS_NOINTEGRALHEIGHT as u32),
                12,
                36,
                326,
                232,
                ID_COL_LIST,
            );
            if let Some(cat) = app.categories.get(app.active_category) {
                refresh_column_listbox(lb, cat, *selected_idx);
            }
            create_ctrl(
                hwnd,
                hfont,
                "BUTTON",
                "上移(&U)",
                0,
                WS_TABSTOP | (BS_PUSHBUTTON as u32),
                12,
                280,
                76,
                26,
                ID_COL_BTN_UP,
            );
            create_ctrl(
                hwnd,
                hfont,
                "BUTTON",
                "下移(&D)",
                0,
                WS_TABSTOP | (BS_PUSHBUTTON as u32),
                96,
                280,
                76,
                26,
                ID_COL_BTN_DOWN,
            );
            create_ctrl(
                hwnd,
                hfont,
                "BUTTON",
                "关闭",
                0,
                WS_TABSTOP | (BS_DEFPUSHBUTTON as u32),
                262,
                280,
                76,
                26,
                IDCANCEL,
            );
            SetFocus(lb);
        }

        ActiveModal::EditShortcut(ed) => {
            let w_tab_cls = String::from_utf16_lossy(
                std::slice::from_raw_parts(WC_TABCONTROLW, 15),
            );
            let tab = create_ctrl(
                hwnd,
                hfont,
                &w_tab_cls,
                "",
                0,
                WS_TABSTOP,
                10,
                10,
                360,
                340,
                ID_EDIT_TAB,
            );
            let mut w_t0 = to_wide("常规");
            let mut item0 = TCITEMW {
                mask: TCIF_TEXT,
                dwState: 0,
                dwStateMask: 0,
                pszText: w_t0.as_mut_ptr(),
                cchTextMax: 0,
                iImage: 0,
                lParam: 0,
            };
            SendMessageW(tab, TCM_INSERTITEMW, 0, &mut item0 as *mut _ as isize);

            let mut w_t1 = to_wide("备注");
            let mut item1 = TCITEMW {
                mask: TCIF_TEXT,
                dwState: 0,
                dwStateMask: 0,
                pszText: w_t1.as_mut_ptr(),
                cchTextMax: 0,
                iImage: 0,
                lParam: 0,
            };
            SendMessageW(tab, TCM_INSERTITEMW, 1, &mut item1 as *mut _ as isize);

            // 【常规】页签控件
            create_ctrl(hwnd, hfont, "STATIC", "", 0,
                0x000D, // SS_OWNERDRAW：由父窗口处理 WM_DRAWITEM。
                26, 44, 36, 36, ID_EDIT_ICON_PREVIEW);
            create_ctrl(hwnd, hfont, "STATIC", "标题:", 0, 0, 68, 52, 42, 18, ID_EDIT_LBL_TITLE);
            let ed_title = create_ctrl(
                hwnd,
                hfont,
                "EDIT",
                &ed.title,
                WS_EX_CLIENTEDGE,
                WS_TABSTOP | (ES_AUTOHSCROLL as u32),
                114,
                49,
                242,
                22,
                ID_EDIT_TITLE,
            );

            create_ctrl(hwnd, hfont, "STATIC", "链接:", 0, 0, 22, 86, 42, 18, ID_EDIT_LBL_LINK);
            create_ctrl(
                hwnd,
                hfont,
                "BUTTON",
                "选文件...",
                0,
                WS_TABSTOP | (BS_PUSHBUTTON as u32),
                200,
                82,
                74,
                24,
                ID_EDIT_PICK_FILE,
            );
            create_ctrl(
                hwnd,
                hfont,
                "BUTTON",
                "选目录...",
                0,
                WS_TABSTOP | (BS_PUSHBUTTON as u32),
                282,
                82,
                74,
                24,
                ID_EDIT_PICK_DIR,
            );
            create_ctrl(
                hwnd,
                hfont,
                "EDIT",
                &ed.target_path,
                WS_EX_CLIENTEDGE,
                WS_TABSTOP | (ES_AUTOHSCROLL as u32),
                68,
                112,
                288,
                22,
                ID_EDIT_PATH,
            );

            create_ctrl(hwnd, hfont, "STATIC", "图标:", 0, 0, 22, 148, 42, 18, ID_EDIT_LBL_ICON);
            create_ctrl(
                hwnd,
                hfont,
                "EDIT",
                &ed.icon_key,
                WS_EX_CLIENTEDGE,
                WS_TABSTOP | (ES_AUTOHSCROLL as u32),
                68,
                145,
                206,
                22,
                ID_EDIT_ICON,
            );
            create_ctrl(
                hwnd,
                hfont,
                "BUTTON",
                "选图标...",
                0,
                WS_TABSTOP | (BS_PUSHBUTTON as u32),
                282,
                144,
                74,
                24,
                ID_EDIT_PICK_ICON,
            );

            create_ctrl(hwnd, hfont, "STATIC", "颜色:", 0, 0, 22, 184, 42, 18, ID_EDIT_LBL_COLOR);
            let color=if ed.use_default_color {String::new()} else {crate::model::format_hex_color(ed.text_color)};
            create_ctrl(hwnd,hfont,"BUTTON",&color,0,WS_TABSTOP|BS_OWNERDRAW as u32,
                68,181,288,22,ID_EDIT_COLOR_BASE);

            create_ctrl(hwnd, hfont, "STATIC", "参数:", 0, 0, 22, 218, 42, 18, ID_EDIT_LBL_ARGS);
            create_ctrl(
                hwnd,
                hfont,
                "EDIT",
                &ed.args,
                WS_EX_CLIENTEDGE,
                WS_TABSTOP | (ES_AUTOHSCROLL as u32),
                68,
                215,
                288,
                22,
                ID_EDIT_ARGS,
            );

            create_ctrl(hwnd, hfont, "STATIC", "起始:", 0, 0, 22, 250, 42, 18, ID_EDIT_LBL_DIR);
            create_ctrl(
                hwnd,
                hfont,
                "EDIT",
                &ed.work_dir,
                WS_EX_CLIENTEDGE,
                WS_TABSTOP | (ES_AUTOHSCROLL as u32),
                68,
                247,
                288,
                22,
                ID_EDIT_DIR,
            );

            create_ctrl(hwnd, hfont, "STATIC", "热键:", 0, 0, 22, 282, 42, 18, ID_EDIT_LBL_HK);
            create_ctrl(
                hwnd,
                hfont,
                "EDIT",
                &ed.hotkey,
                WS_EX_CLIENTEDGE,
                WS_TABSTOP | (ES_AUTOHSCROLL as u32),
                68,
                279,
                288,
                22,
                ID_EDIT_HK,
            );

            let chk = create_ctrl(
                hwnd,
                hfont,
                "BUTTON",
                "运行后最小化音速启动",
                0,
                WS_TABSTOP | (BS_AUTOCHECKBOX as u32),
                68,
                312,
                200,
                20,
                ID_EDIT_CHK_MIN,
            );
            SendMessageW(chk, BM_SETCHECK, if ed.minimize_on_run { 1 } else { 0 }, 0);

            // 【备注】页签控件
            create_ctrl(
                hwnd,
                hfont,
                "EDIT",
                &ed.remark,
                WS_EX_CLIENTEDGE,
                WS_TABSTOP | (ES_MULTILINE as u32) | (ES_AUTOVSCROLL as u32) | WS_VSCROLL | (ES_WANTRETURN as u32),
                24,
                46,
                332,
                288,
                ID_EDIT_REMARK,
            );

            create_ctrl(
                hwnd,
                hfont,
                "BUTTON",
                "确定",
                0,
                WS_TABSTOP | (BS_DEFPUSHBUTTON as u32),
                206,
                362,
                78,
                26,
                IDOK,
            );
            create_ctrl(
                hwnd,
                hfont,
                "BUTTON",
                "取消",
                0,
                WS_TABSTOP | (BS_PUSHBUTTON as u32),
                292,
                362,
                78,
                26,
                IDCANCEL,
            );
            switch_edit_tab_visibility(hwnd, ed.active_tab);
            SetFocus(ed_title);
        }

        _ => {}
    }
}

pub unsafe fn refresh_category_listbox(lb: HWND, categories: &[CategoryTab], sel: usize) {
    if lb == 0 {
        return;
    }
    SendMessageW(lb, LB_RESETCONTENT, 0, 0);
    for cat in categories {
        let suffix = if cat.encrypted { " [已加密]" } else { "" };
        listbox_add(lb, &format!("{}{}", cat.name, suffix));
    }
    if !categories.is_empty() {
        SendMessageW(lb, LB_SETCURSEL, sel.min(categories.len() - 1), 0);
    }
}

pub unsafe fn refresh_column_listbox(lb: HWND, cat: &CategoryTab, sel: usize) {
    if lb == 0 {
        return;
    }
    SendMessageW(lb, LB_RESETCONTENT, 0, 0);
    for (idx, grp) in cat.groups.iter().enumerate() {
        listbox_add(lb, &format!("({}) -= {} =-", idx + 1, grp.title));
    }
    if !cat.groups.is_empty() {
        SendMessageW(lb, LB_SETCURSEL, sel.min(cat.groups.len() - 1), 0);
    }
}

pub unsafe fn switch_edit_tab_visibility(hwnd: HWND, active_tab: usize) {
    let show_gen = if active_tab == 0 { SW_SHOW } else { SW_HIDE };
    let show_rem = if active_tab == 1 { SW_SHOW } else { SW_HIDE };
    for id in [
        ID_EDIT_ICON_PREVIEW,
        ID_EDIT_LBL_TITLE,
        ID_EDIT_TITLE,
        ID_EDIT_LBL_LINK,
        ID_EDIT_PICK_FILE,
        ID_EDIT_PICK_DIR,
        ID_EDIT_PATH,
        ID_EDIT_LBL_ICON,
        ID_EDIT_ICON,
        ID_EDIT_PICK_ICON,
        ID_EDIT_LBL_COLOR,
        ID_EDIT_LBL_ARGS,
        ID_EDIT_ARGS,
        ID_EDIT_LBL_DIR,
        ID_EDIT_DIR,
        ID_EDIT_LBL_HK,
        ID_EDIT_HK,
        ID_EDIT_CHK_MIN,
    ] {
        let h = GetDlgItem(hwnd, id);
        if h != 0 {
            ShowWindow(h, show_gen);
        }
    }
    for idx in 0..8 {
        let h = GetDlgItem(hwnd, ID_EDIT_COLOR_BASE + idx);
        if h != 0 {
            ShowWindow(h, show_gen);
        }
    }
    let h_rem = GetDlgItem(hwnd, ID_EDIT_REMARK);
    if h_rem != 0 {
        ShowWindow(h_rem, show_rem);
    }
    InvalidateRect(hwnd, ptr::null(), 1);
}

/// 将 `RgbaSurface` 合成至指定背景并直接用 GDI 绘制到 HDC
pub(crate) unsafe fn blit_surface_on_bg(
    hdc: HDC,
    surf: &RgbaSurface,
    dst_x: i32,
    dst_y: i32,
    dst_w: i32,
    dst_h: i32,
    bg_rgb: [u8; 3],
) {
    if surf.width == 0 || surf.height == 0 || dst_w <= 0 || dst_h <= 0 {
        return;
    }
    // SetDIBitsToDevice 只复制像素，不会缩放；先匹配目标尺寸，避免小图标被裁切。
    let scaled = surf.resize_to(dst_w as u32, dst_h as u32);
    let surf = &scaled;
    let mut bgra = Vec::with_capacity((surf.width * surf.height) as usize);
    for p in surf.pixels() {
        let a = p[3] as u32;
        let inv = 255 - a;
        let r = ((p[0] as u32 * a + bg_rgb[0] as u32 * inv) / 255) as u32;
        let g = ((p[1] as u32 * a + bg_rgb[1] as u32 * inv) / 255) as u32;
        let b = ((p[2] as u32 * a + bg_rgb[2] as u32 * inv) / 255) as u32;
        bgra.push(b | (g << 8) | (r << 16) | 0xFF00_0000);
    }
    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: surf.width as i32,
            biHeight: -(surf.height as i32),
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
    SetDIBitsToDevice(
        hdc,
        dst_x,
        dst_y,
        dst_w.min(surf.width as i32) as u32,
        dst_h.min(surf.height as i32) as u32,
        0,
        0,
        0,
        surf.height,
        bgra.as_ptr() as *const core::ffi::c_void,
        &bmi,
        DIB_RGB_COLORS,
    );
}

/// 绘制原生弹窗中的图像预览与色块（如皮肤缩略图、分类图标、快捷方式图标及 8 色调色板）
pub unsafe fn paint_native_modal_extras(hwnd: HWND, hdc: HDC, app: &AppState) {
    let sys_face = crate::advanced_style::PAGE;
    let bg_rgb = [
        (sys_face & 0xFF) as u8,
        ((sys_face >> 8) & 0xFF) as u8,
        ((sys_face >> 16) & 0xFF) as u8,
    ];
    SetBkMode(hdc, TRANSPARENT as i32);

    match &app.modal {
        ActiveModal::SkinManager => {
            let mut client: RECT = std::mem::zeroed();
            windows_sys::Win32::UI::WindowsAndMessaging::GetClientRect(hwnd, &mut client);
            let split = crate::dialog_layout::skin_split(hwnd).clamp(120, (client.right-160).max(120));
            let bar = RECT { left: split-2, top:12, right:split+2, bottom:client.bottom-52 };
            FillRect(hdc, &bar, windows_sys::Win32::Graphics::Gdi::GetSysColorBrush(
                windows_sys::Win32::Graphics::Gdi::COLOR_3DSHADOW));
            if let Some(entry) = app.skins.get(app.selected_skin_index) {
                let thumb = entry
                    .preview_gif_path
                    .as_ref()
                    .and_then(|p| RgbaSurface::load_file(p));
                if let Some(surf) = thumb {
                    let (width, height) = ((client.right-split-22).max(1), (client.bottom-170).max(1));
                    let resized = surf.resize_to(width as u32, height as u32);
                    blit_surface_on_bg(hdc, &resized, split+10, 14, width, height, bg_rgb);
                }
            }
        }
        ActiveModal::CategoryManager { .. } => {}
        ActiveModal::EditShortcut(_) => {}
        _ => {}
    }
}

/// 从原生 Edit 控件同步【修改/新建快捷方式】内容并提交保存
pub unsafe fn commit_edit_shortcut_from_controls(
    hwnd: HWND,
    app: &mut AppState,
    ed_state: EditShortcutDialogState,
) {
    let title = get_ctrl_text(hwnd, ID_EDIT_TITLE);
    let mut target_path = get_ctrl_text(hwnd, ID_EDIT_PATH);
    let icon_key = get_ctrl_text(hwnd, ID_EDIT_ICON);
    let args = get_ctrl_text(hwnd, ID_EDIT_ARGS);
    let mut work_dir = get_ctrl_text(hwnd, ID_EDIT_DIR);
    let hotkey = get_ctrl_text(hwnd, ID_EDIT_HK);
    let remark = get_ctrl_text(hwnd, ID_EDIT_REMARK);
    let min_chk = SendMessageW(GetDlgItem(hwnd, ID_EDIT_CHK_MIN), BM_GETCHECK, 0, 0) != 0;
    let target_kind = if target_path == ed_state.target_path {
        ed_state.target_kind_hint.unwrap_or_else(|| crate::targets::TargetKind::infer(&app.base_dir, &target_path))
    } else {
        crate::targets::TargetKind::infer(&app.base_dir, &target_path)
    };
    if ed_state.is_new && matches!(target_kind, crate::targets::TargetKind::File | crate::targets::TargetKind::Folder)
        && app.user_cfg.advanced.enabled(crate::advanced::Key::RelativePaths) {
        target_path = crate::advanced_runtime::relative_target(&app.base_dir, std::path::Path::new(&target_path));
        work_dir = crate::advanced_runtime::relative_target(&app.base_dir, std::path::Path::new(&work_dir));
    }
    if app.user_cfg.advanced.enabled(crate::advanced::Key::WebIcons)
        && crate::advanced_web_icon::is_web_url(&target_path) && ed_state.pending_icon_16.is_none() {
        crate::advanced_web_icon::fetch_for_shortcut(app.base_dir.clone(), target_path.clone(),
            app.user_cfg.advanced.enabled(crate::advanced::Key::CacheWebIcons));
    }

    let mut updated_item_id = ed_state.shortcut_id.clone();

    if ed_state.is_new {
        if let Some(cat) = app.categories.get_mut(ed_state.cat_idx) {
            let grp_idx = ed_state.grp_idx.min(cat.groups.len().saturating_sub(1));
            if let Some(grp) = cat.groups.get_mut(grp_idx) {
                let final_title = if title.trim().is_empty() {
                    "新建快捷方式".to_string()
                } else {
                    title.trim().to_string()
                };
                let mut custom_icon_16 = ed_state.pending_icon_16.clone();
                if let Some(ref ico) = custom_icon_16 {
                    let icon_dir = app.base_dir.join("user").join("icons");
                    let _ = std::fs::create_dir_all(&icon_dir);
                    let _ = ico.save_png(&icon_dir.join(format!("{}.png", updated_item_id)));
                } else {
                    let _ = &mut custom_icon_16;
                }
                let final_icon_key = if icon_key.trim().is_empty() {
                    "app_msedge".to_string()
                } else {
                    icon_key.trim().to_string()
                };
                grp.items.push(ShortcutItem { use_default_color: ed_state.use_default_color,
                    id: updated_item_id.clone(),
                    category_id: cat.id.clone(),
                    column_id: grp.id.clone(),
                    name: final_title,
                    target_path,
                    target_kind,
                    icon_key: final_icon_key,
                    custom_icon_16,
                    text_color: ed_state.text_color,
                    args,
                    work_dir,
                    remark,
                    minimize_on_run: min_chk,
                });
            }
        }
    } else if let Some(cat) = app.categories.get_mut(ed_state.cat_idx) {
        if let Some(grp) = cat.groups.get_mut(ed_state.grp_idx) {
            if let Some(item) = grp.items.get_mut(ed_state.item_idx) {
                updated_item_id = item.id.clone();
                item.name = title;
                if item.target_path != target_path { item.target_kind = target_kind; }
                item.target_path = target_path;
                item.icon_key = icon_key;
                if let Some(ref ico) = ed_state.pending_icon_16 {
                    let icon_dir = app.base_dir.join("user").join("icons");
                    let _ = std::fs::create_dir_all(&icon_dir);
                    let _ = ico.save_png(&icon_dir.join(format!("{}.png", item.id)));
                    item.custom_icon_16 = Some(ico.clone());
                }
                item.text_color = ed_state.text_color;
                item.use_default_color = ed_state.use_default_color;
                item.args = args;
                item.work_dir = work_dir;
                item.remark = remark;
                item.minimize_on_run = min_chk;
            }
        }
    }

    app.hotkey_cfg.set_hotkey_for_shortcut(&updated_item_id, &hotkey);
    app.hotkey_cfg.save(&app.base_dir);
    crate::hotkeys::changed(app.main_hwnd);
    app.persist_data();
}

/// 读取当前 ListBox 选中索引
pub unsafe fn listbox_get_cursel(hwnd: HWND, id: i32) -> Option<usize> {
    let lb = GetDlgItem(hwnd, id);
    if lb == 0 {
        return None;
    }
    let sel = SendMessageW(lb, LB_GETCURSEL, 0, 0) as isize;
    if sel >= 0 {
        Some(sel as usize)
    } else {
        None
    }
}

/// 读取 `TabControl` 当前选中页签索引
pub unsafe fn tab_get_cursel(hwnd: HWND, id: i32) -> usize {
    let tab = GetDlgItem(hwnd, id);
    if tab == 0 {
        return 0;
    }
    let sel = SendMessageW(tab, TCM_GETCURSEL, 0, 0) as isize;
    if sel >= 0 {
        sel as usize
    } else {
        0
    }
}
