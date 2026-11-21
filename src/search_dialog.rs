//! 原生搜索窗口及其双行结果列表。

use crate::native_dialogs::{blit_surface_on_bg, create_ctrl, get_ctrl_text, set_ctrl_text};
use crate::search::{SearchResultRow, SearchScope};
use crate::to_wide;
use std::ptr;
use windows_sys::Win32::Foundation::{HWND, RECT};
use windows_sys::Win32::Graphics::Gdi::{
    DrawFocusRect, DrawTextW, FillRect, GetSysColor, GetSysColorBrush, InvalidateRect,
    SelectObject, SetBkMode, SetTextColor, COLOR_GRAYTEXT, COLOR_HIGHLIGHT, COLOR_HIGHLIGHTTEXT,
    COLOR_WINDOW, COLOR_WINDOWTEXT, DT_END_ELLIPSIS, DT_NOPREFIX, DT_PATH_ELLIPSIS, DT_SINGLELINE,
    DT_VCENTER, HFONT, TRANSPARENT,
};
use windows_sys::Win32::UI::Controls::{DRAWITEMSTRUCT, ODS_FOCUS, ODS_SELECTED};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetDlgItem, SendMessageW, BM_GETCHECK, BM_SETCHECK, BS_AUTOCHECKBOX, BS_AUTORADIOBUTTON,
    BS_PUSHBUTTON, ES_AUTOHSCROLL, IDCANCEL, LBS_HASSTRINGS, LBS_NOINTEGRALHEIGHT, LBS_NOTIFY,
    LBS_OWNERDRAWFIXED, LB_ADDSTRING, LB_GETCURSEL, LB_RESETCONTENT, LB_SETCURSEL,
    LB_SETITEMHEIGHT, WM_SETREDRAW, WS_EX_CLIENTEDGE, WS_GROUP, WS_TABSTOP, WS_VSCROLL,
};

pub const WIDTH: i32 = 419;
pub const HEIGHT: i32 = 446;
pub const ID_QUERY: i32 = 1500;
pub const ID_SUBMIT: i32 = 1501;
pub const ID_RESULTS: i32 = 1502;
const ID_GROUP: i32 = 1504;
const ID_FUZZY: i32 = 1505;
const ID_COUNT: i32 = 1506;
const ID_CATEGORY: i32 = 1507;
const ID_ALL: i32 = 1508;
const SS_CENTER: u32 = 1;
const ROW_HEIGHT: usize = 40;

pub unsafe fn populate(hwnd: HWND, font: HFONT, query: &str) {
    create_ctrl(hwnd, font, "STATIC", "关键字：", 0, 0, 14, 19, 52, 18, -1);
    create_ctrl(
        hwnd,
        font,
        "EDIT",
        query,
        WS_EX_CLIENTEDGE,
        WS_TABSTOP | ES_AUTOHSCROLL as u32,
        66,
        15,
        202,
        22,
        ID_QUERY,
    );
    let fuzzy = create_ctrl(
        hwnd,
        font,
        "BUTTON",
        "模糊",
        0,
        WS_TABSTOP | BS_AUTOCHECKBOX as u32,
        279,
        15,
        56,
        22,
        ID_FUZZY,
    );
    SendMessageW(fuzzy, BM_SETCHECK, 1, 0);
    create_ctrl(
        hwnd,
        font,
        "BUTTON",
        "搜索",
        0,
        WS_TABSTOP | BS_PUSHBUTTON as u32,
        345,
        14,
        60,
        24,
        ID_SUBMIT,
    );

    create_ctrl(
        hwnd, font, "STATIC", "", 0, SS_CENTER, 14, 50, 391, 18, ID_COUNT,
    );
    let list = create_ctrl(
        hwnd,
        font,
        "LISTBOX",
        "",
        WS_EX_CLIENTEDGE,
        WS_TABSTOP
            | WS_VSCROLL
            | (LBS_NOTIFY | LBS_HASSTRINGS | LBS_OWNERDRAWFIXED | LBS_NOINTEGRALHEIGHT) as u32,
        14,
        71,
        391,
        335,
        ID_RESULTS,
    );
    SendMessageW(list, LB_SETITEMHEIGHT, 0, ROW_HEIGHT as isize);

    create_ctrl(hwnd, font, "STATIC", "范围：", 0, 0, 14, 422, 43, 18, -1);
    for (id, label, x, group_style) in [
        (ID_GROUP, "当前栏目", 57, WS_GROUP),
        (ID_CATEGORY, "当前类别", 140, 0),
        (ID_ALL, "所有", 225, WS_TABSTOP),
    ] {
        let radio = create_ctrl(
            hwnd,
            font,
            "BUTTON",
            label,
            0,
            group_style | BS_AUTORADIOBUTTON as u32,
            x,
            417,
            77,
            24,
            id,
        );
        if id == ID_ALL {
            SendMessageW(radio, BM_SETCHECK, 1, 0);
        }
    }
    create_ctrl(
        hwnd,
        font,
        "BUTTON",
        "取消",
        0,
        WS_GROUP | WS_TABSTOP,
        332,
        415,
        73,
        26,
        IDCANCEL,
    );
}

pub unsafe fn criteria(hwnd: HWND) -> (String, SearchScope, bool) {
    let checked = |id| SendMessageW(GetDlgItem(hwnd, id), BM_GETCHECK, 0, 0) == 1;
    let scope = if checked(ID_GROUP) {
        SearchScope::Group
    } else if checked(ID_CATEGORY) {
        SearchScope::Category
    } else {
        SearchScope::All
    };
    (get_ctrl_text(hwnd, ID_QUERY), scope, checked(ID_FUZZY))
}

pub unsafe fn render_results(hwnd: HWND, names: &[String]) {
    let list = GetDlgItem(hwnd, ID_RESULTS);
    SendMessageW(list, WM_SETREDRAW, 0, 0);
    SendMessageW(list, LB_RESETCONTENT, 0, 0);
    let mut count = 0;
    for name in names {
        let text = to_wide(name);
        if SendMessageW(list, LB_ADDSTRING, 0, text.as_ptr() as isize) < 0 {
            break;
        }
        count += 1;
    }
    set_ctrl_text(hwnd, ID_COUNT, &format!("搜索到 {count} 个结果"));
    if count != 0 {
        SendMessageW(list, LB_SETCURSEL, 0, 0);
    }
    SendMessageW(list, WM_SETREDRAW, 1, 0);
    InvalidateRect(list, ptr::null(), 1);
    SetFocus(if count == 0 {
        GetDlgItem(hwnd, ID_QUERY)
    } else {
        list
    });
}

pub unsafe fn selection(hwnd: HWND) -> Option<usize> {
    usize::try_from(SendMessageW(
        GetDlgItem(hwnd, ID_RESULTS),
        LB_GETCURSEL,
        0,
        0,
    ))
    .ok()
}

pub unsafe fn draw_result(draw: &DRAWITEMSTRUCT, font: HFONT, row: Option<&SearchResultRow>) {
    let selected = draw.itemState & ODS_SELECTED != 0;
    let background = if selected {
        COLOR_HIGHLIGHT
    } else {
        COLOR_WINDOW
    };
    FillRect(draw.hDC, &draw.rcItem, GetSysColorBrush(background));
    let Some(row) = row else {
        return;
    };
    let rect = draw.rcItem;
    if let Some(icon) = &row.icon {
        let [r, g, b, _] = GetSysColor(background).to_le_bytes();
        blit_surface_on_bg(
            draw.hDC,
            icon,
            rect.left + 5,
            rect.top + 8,
            24,
            24,
            [r, g, b],
        );
    }

    let old_font = SelectObject(draw.hDC, font);
    let old_mode = SetBkMode(draw.hDC, TRANSPARENT as i32);
    let old_color = SetTextColor(
        draw.hDC,
        GetSysColor(if selected {
            COLOR_HIGHLIGHTTEXT
        } else {
            COLOR_WINDOWTEXT
        }),
    );
    let mut title = RECT {
        left: rect.left + 36,
        top: rect.top + 4,
        right: rect.right - 6,
        bottom: rect.top + 20,
    };
    let mut path = RECT {
        top: title.bottom,
        bottom: rect.bottom - 4,
        ..title
    };
    let flags = DT_SINGLELINE | DT_NOPREFIX | DT_VCENTER;
    let name = to_wide(&row.name);
    DrawTextW(
        draw.hDC,
        name.as_ptr(),
        -1,
        &mut title,
        flags | DT_END_ELLIPSIS,
    );
    if !selected {
        SetTextColor(draw.hDC, GetSysColor(COLOR_GRAYTEXT));
    }
    let target = to_wide(&row.path);
    DrawTextW(
        draw.hDC,
        target.as_ptr(),
        -1,
        &mut path,
        flags | DT_PATH_ELLIPSIS,
    );
    SetTextColor(draw.hDC, old_color);
    SetBkMode(draw.hDC, old_mode);
    SelectObject(draw.hDC, old_font);
    if draw.itemState & ODS_FOCUS != 0 {
        DrawFocusRect(draw.hDC, &rect);
    }
}
