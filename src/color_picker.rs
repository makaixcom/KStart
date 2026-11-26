//! 原生颜色菜单与 Windows 颜色选择对话框，色块仅用于按钮预览。
use crate::{native_dialogs::get_ctrl_text, to_wide};
use std::mem;
use windows_sys::Win32::{
    Foundation::{HWND, RECT},
    Graphics::Gdi::*,
    UI::{Controls::DRAWITEMSTRUCT, WindowsAndMessaging::*},
};
enum Selection { Default, More }

pub unsafe fn draw_button(draw: &DRAWITEMSTRUCT) {
    let r = draw.rcItem;
    FillRect(draw.hDC, &r, GetSysColorBrush(COLOR_WINDOW));
    let mut edge = r;
    DrawEdge(draw.hDC, &mut edge, EDGE_SUNKEN, BF_RECT);
    let value = get_ctrl_text(GetParent(draw.hwndItem), draw.CtlID as i32);
    let strip = RECT {
        left: r.left + 3,
        top: r.top + 3,
        right: r.right - 20,
        bottom: r.bottom - 3,
    };
    if let Some(color) = crate::advanced::color(&value) {
        let brush = CreateSolidBrush(color.to_colorref());
        FillRect(draw.hDC, &strip, brush);
        DeleteObject(brush as _);
    } else {
        // 默认状态单独绘制，避免与显式选择的黑色混淆。
        FillRect(draw.hDC, &strip, GetStockObject(WHITE_BRUSH) as HBRUSH);
        let gray = CreateSolidBrush(0x00e6e6e6);
        for y in (strip.top..strip.bottom).step_by(6) {
            for x in (strip.left..strip.right).step_by(6) {
                if ((x - strip.left) / 6 + (y - strip.top) / 6) % 2 == 0 {
                    let cell = RECT {
                        left: x,
                        top: y,
                        right: (x + 6).min(strip.right),
                        bottom: (y + 6).min(strip.bottom),
                    };
                    FillRect(draw.hDC, &cell, gray);
                }
            }
        }
        DeleteObject(gray as _);
        let old_font = SelectObject(draw.hDC, GetStockObject(DEFAULT_GUI_FONT));
        let old_mode = SetBkMode(draw.hDC, TRANSPARENT as i32);
        let old_color = SetTextColor(draw.hDC, 0x00333333);
        let mut text = strip;
        DrawTextW(
            draw.hDC,
            to_wide("默认").as_ptr(),
            -1,
            &mut text,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
        );
        SetTextColor(draw.hDC, old_color);
        SetBkMode(draw.hDC, old_mode);
        SelectObject(draw.hDC, old_font);
    }
    let mut arrow = RECT {
        left: r.right - 18,
        top: r.top + 2,
        right: r.right - 2,
        bottom: r.bottom - 2,
    };
    DrawFrameControl(draw.hDC, &mut arrow, DFC_SCROLL, DFCS_SCROLLCOMBOBOX);
}

unsafe fn choose(owner: HWND, button: HWND) -> Option<Selection> {
    let mut anchor: RECT = mem::zeroed();
    GetWindowRect(button, &mut anchor);
    let menu = CreatePopupMenu();
    if menu == 0 { return None; }
    AppendMenuW(menu, MF_STRING, 1, to_wide("默认（使用皮肤颜色）").as_ptr());
    AppendMenuW(menu, MF_STRING, 2, to_wide("选择颜色…").as_ptr());
    let selected = TrackPopupMenuEx(menu, TPM_RETURNCMD | TPM_NONOTIFY,
        anchor.left, anchor.bottom, owner, std::ptr::null());
    DestroyMenu(menu);
    match selected {
        1 => Some(Selection::Default),
        2 => Some(Selection::More),
        _ => None,
    }
}

/// 所有颜色入口共用：空字符串表示清除覆盖并跟随默认色。
pub unsafe fn choose_value(owner: HWND, button: HWND, initial: &str) -> Option<String> {
    match choose(owner, button)? {
        Selection::Default => Some(String::new()),
        Selection::More => {
            use windows_sys::Win32::UI::Controls::Dialogs::{
                ChooseColorW, CC_FULLOPEN, CC_RGBINIT, CHOOSECOLORW,
            };
            let mut picker: CHOOSECOLORW = mem::zeroed();
            let mut custom = [0u32; 16];
            picker.lStructSize = mem::size_of::<CHOOSECOLORW>() as u32;
            picker.hwndOwner = owner;
            picker.lpCustColors = custom.as_mut_ptr();
            picker.rgbResult = crate::advanced::color(initial).map_or(0, |c| c.to_colorref());
            picker.Flags = CC_FULLOPEN | CC_RGBINIT;
            (ChooseColorW(&mut picker) != 0).then(|| {
                let [r, g, b, _] = picker.rgbResult.to_le_bytes();
                format!("#{r:02X}{g:02X}{b:02X}")
            })
        }
    }
}
