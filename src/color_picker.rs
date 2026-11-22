//! 颜色色条及常用色下拉面板，更多颜色交由原生完整色盘。
use crate::{native_dialogs::get_ctrl_text, to_wide};
use std::{cell::Cell, mem, ptr};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, RECT, WPARAM},
    Graphics::Gdi::*,
    System::LibraryLoader::GetModuleHandleW,
    UI::{
        Controls::DRAWITEMSTRUCT,
        Input::KeyboardAndMouse::{
            GetCapture, ReleaseCapture, SetCapture, VK_DOWN, VK_ESCAPE, VK_LEFT, VK_RETURN,
            VK_RIGHT, VK_UP,
        },
        WindowsAndMessaging::*,
    },
};
const COLORS: [u32; 64] = [
    0xffffff, 0xffcccc, 0xffe5cc, 0xffffcc, 0xccffcc, 0xccffff, 0xccddff, 0xffccff, 0xdddddd,
    0xff9999, 0xffcc99, 0xffff99, 0x99ff99, 0x99ffff, 0x99bbff, 0xff99ff, 0xbbbbbb, 0xff6666,
    0xff9966, 0xffff66, 0x66ff66, 0x66ffff, 0x6699ff, 0xff66ff, 0x999999, 0xff3333, 0xff6600,
    0xffff00, 0x00ff00, 0x00ffff, 0x0033ff, 0xff00ff, 0x777777, 0xcc0000, 0xcc6600, 0xcccc00,
    0x00cc00, 0x00cccc, 0x0000cc, 0xcc00cc, 0x555555, 0x990000, 0x993300, 0x999900, 0x009900,
    0x009999, 0x000099, 0x990099, 0x333333, 0x660000, 0x663300, 0x666600, 0x006600, 0x006666,
    0x000066, 0x660066, 0x000000, 0x330000, 0x331900, 0x333300, 0x003300, 0x003333, 0x000033,
    0x330033,
];
pub enum Selection {
    Default,
    Color(String),
    More,
}
struct Palette {
    hover: Cell<i32>,
    result: Cell<Option<usize>>,
}
fn colorref(rgb: u32) -> u32 {
    ((rgb >> 16) & 255) | (rgb & 0xff00) | ((rgb & 255) << 16)
}

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

pub unsafe fn choose(owner: HWND, button: HWND) -> Option<Selection> {
    let mut anchor: RECT = mem::zeroed();
    GetWindowRect(button, &mut anchor);
    let mut monitor: MONITORINFO = mem::zeroed();
    monitor.cbSize = mem::size_of::<MONITORINFO>() as u32;
    GetMonitorInfoW(
        MonitorFromRect(&anchor, MONITOR_DEFAULTTONEAREST),
        &mut monitor,
    );
    let (w, h) = (152, 200);
    let work = monitor.rcWork;
    let x = anchor
        .left
        .clamp(work.left, (work.right - w).max(work.left));
    let y = if anchor.bottom + h <= work.bottom {
        anchor.bottom
    } else {
        anchor.top - h
    }
    .max(work.top);
    let state = Palette {
        hover: Cell::new(-1),
        result: Cell::new(None),
    };
    let class = to_wide("KStartColorPalette");
    let instance = GetModuleHandleW(ptr::null());
    RegisterClassW(&WNDCLASSW {
        lpfnWndProc: Some(proc),
        hInstance: instance,
        hCursor: LoadCursorW(0, IDC_ARROW),
        lpszClassName: class.as_ptr(),
        ..mem::zeroed()
    });
    let hwnd = CreateWindowExW(
        WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
        class.as_ptr(),
        ptr::null(),
        WS_POPUP | WS_BORDER,
        x,
        y,
        w,
        h,
        owner,
        0,
        instance,
        &state as *const _ as _,
    );
    if hwnd == 0 {
        return None;
    }
    ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    SetCapture(hwnd);
    let mut msg: MSG = mem::zeroed();
    while IsWindow(hwnd) != 0 && GetCapture() == hwnd {
        let status = GetMessageW(&mut msg, 0, 0, 0);
        if status <= 0 {
            if status == 0 {
                PostQuitMessage(msg.wParam as i32);
            }
            break;
        }
        if msg.message == WM_KEYDOWN {
            SendMessageW(hwnd, msg.message, msg.wParam, msg.lParam);
            continue;
        }
        TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }
    if GetCapture() == hwnd {
        ReleaseCapture();
    }
    if IsWindow(hwnd) != 0 {
        DestroyWindow(hwnd);
    }
    state.result.get().map(|index| {
        if index == 64 {
            Selection::Default
        } else if index == 65 {
            Selection::More
        } else {
            Selection::Color(format!("#{:06X}", COLORS[index]))
        }
    })
}
fn hit(x: i32, y: i32) -> i32 {
    if (4..148).contains(&x) && (4..148).contains(&y) {
        ((y - 4) / 18) * 8 + (x - 4) / 18
    } else if (3..149).contains(&x) && (151..174).contains(&y) {
        64
    } else if (3..149).contains(&x) && (175..198).contains(&y) {
        65
    } else {
        -1
    }
}
unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> isize {
    if msg == WM_NCCREATE {
        SetWindowLongPtrW(
            hwnd,
            GWLP_USERDATA,
            (*(lp as *const CREATESTRUCTW)).lpCreateParams as isize,
        );
    }
    let Some(state) = (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const Palette).as_ref() else {
        return DefWindowProcW(hwnd, msg, wp, lp);
    };
    let next = hit(lp as i16 as i32, (lp >> 16) as i16 as i32);
    match msg {
        WM_ERASEBKGND => 1,
        WM_MOUSEACTIVATE => MA_NOACTIVATE as isize,
        WM_MOUSEMOVE => {
            if state.hover.replace(next) != next {
                InvalidateRect(hwnd, ptr::null(), 0);
            }
            0
        }
        WM_SETCURSOR => {
            SetCursor(LoadCursorW(
                0,
                if state.hover.get() >= 0 {
                    IDC_HAND
                } else {
                    IDC_ARROW
                },
            ));
            1
        }
        WM_LBUTTONDOWN => {
            if next < 0 {
                DestroyWindow(hwnd);
            }
            0
        }
        WM_LBUTTONUP => {
            if next >= 0 {
                state.result.set(Some(next as usize));
            }
            DestroyWindow(hwnd);
            0
        }
        WM_KEYDOWN => {
            let mut index = state.hover.get().max(0);
            match wp as u16 {
                VK_ESCAPE => {
                    DestroyWindow(hwnd);
                    return 0;
                }
                VK_RETURN => {
                    state.result.set(Some(index as usize));
                    DestroyWindow(hwnd);
                    return 0;
                }
                VK_LEFT => index = (index - 1).max(0),
                VK_RIGHT => index = (index + 1).min(65),
                VK_UP => index = (index - 8).max(0),
                VK_DOWN => index = (index + 8).min(65),
                _ => {}
            }
            state.hover.set(index);
            InvalidateRect(hwnd, ptr::null(), 0);
            0
        }
        WM_PAINT => {
            let mut paint: PAINTSTRUCT = mem::zeroed();
            let dc = BeginPaint(hwnd, &mut paint);
            let mut client: RECT = mem::zeroed();
            GetClientRect(hwnd, &mut client);
            FillRect(dc, &client, GetSysColorBrush(COLOR_BTNFACE));
            for (index, &rgb) in COLORS.iter().enumerate() {
                let x = 4 + (index % 8) as i32 * 18;
                let y = 4 + (index / 8) as i32 * 18;
                let r = RECT {
                    left: x,
                    top: y,
                    right: x + 13,
                    bottom: y + 13,
                };
                let brush = CreateSolidBrush(colorref(rgb));
                FillRect(dc, &r, brush);
                DeleteObject(brush as _);
                FrameRect(dc, &r, GetSysColorBrush(COLOR_3DSHADOW));
                if state.hover.get() == index as i32 {
                    let r = RECT {
                        left: x - 2,
                        top: y - 2,
                        right: x + 15,
                        bottom: y + 15,
                    };
                    FrameRect(dc, &r, GetSysColorBrush(COLOR_HIGHLIGHT));
                }
            }
            SetBkMode(dc, TRANSPARENT as i32);
            let old = SelectObject(dc, GetStockObject(DEFAULT_GUI_FONT));
            for (index, text, top) in [(64, "无颜色（使用默认）", 151), (65, "更多颜色…", 175)]
            {
                let mut row = RECT {
                    left: 3,
                    top,
                    right: 149,
                    bottom: top + 23,
                };
                if state.hover.get() == index {
                    FrameRect(dc, &row, GetSysColorBrush(COLOR_HIGHLIGHT));
                }
                DrawTextW(
                    dc,
                    to_wide(text).as_ptr(),
                    -1,
                    &mut row,
                    DT_CENTER | DT_VCENTER | DT_SINGLELINE,
                );
            }
            SelectObject(dc, old);
            EndPaint(hwnd, &paint);
            0
        }
        WM_CLOSE => {
            DestroyWindow(hwnd);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

/// 所有颜色入口共用：空字符串表示清除覆盖并跟随默认色。
pub unsafe fn choose_value(owner: HWND, button: HWND, initial: &str) -> Option<String> {
    match choose(owner, button)? {
        Selection::Default => Some(String::new()),
        Selection::Color(color) => Some(color),
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
