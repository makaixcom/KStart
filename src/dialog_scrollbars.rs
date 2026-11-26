//! 辅助窗口使用窄原生 SCROLLBAR，文本和列表仍由原控件处理。
use std::{cell::Cell, mem, ptr};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, WPARAM, RECT, POINT},
    Graphics::Gdi::{MapWindowPoints, ScreenToClient},
    UI::Controls::SetScrollInfo,
    UI::{Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass}, WindowsAndMessaging::*},
};
pub const WIDTH: i32 = 8;
const SUBCLASS: usize = 0x4b5357;
const STATE: &str = "KStart.ThinScrollbars";
pub const SOURCE: &str = "KStart.ScrollbarSource";
struct State { vertical: HWND, horizontal: HWND, syncing: Cell<bool> }

pub unsafe fn install(hwnd: HWND) {
    if hwnd == 0 || GetPropW(hwnd, crate::to_wide(STATE).as_ptr()) != 0 { return; }
    let child = GetWindowLongW(hwnd, GWL_STYLE) as u32 & WS_CHILD != 0;
    let parent = if child { GetParent(hwnd) } else { hwnd };
    let make = |vertical: bool| {
        let bar = CreateWindowExW(if child { 0 } else { WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW }, crate::to_wide("SCROLLBAR").as_ptr(), ptr::null(),
            (if child { WS_CHILD } else { WS_POPUP }) | if vertical { SBS_VERT as u32 } else { SBS_HORZ as u32 },
            0, 0, WIDTH, WIDTH, parent, 0, 0, ptr::null());
        SetPropW(bar, crate::to_wide(SOURCE).as_ptr(), hwnd);
        bar
    };
    let state = Box::new(State { vertical: make(true), horizontal: make(false), syncing: Cell::new(false) });
    let data = Box::into_raw(state);
    SetPropW(hwnd, crate::to_wide(STATE).as_ptr(), data as isize);
    SetWindowSubclass(parent, Some(parent_proc), SUBCLASS, 0);
    SetWindowSubclass(hwnd, Some(proc), SUBCLASS, 0);
    SetWindowPos(hwnd, 0, 0, 0, 0, 0,
        SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED);
    sync(hwnd);
}

unsafe fn sync(hwnd: HWND) {
    let data = GetPropW(hwnd, crate::to_wide(STATE).as_ptr()) as *const State;
    let Some(state) = data.as_ref() else { return; };
    if state.syncing.replace(true) { return; }
    let mut client: RECT = mem::zeroed();
    GetClientRect(hwnd, &mut client);
    let style = GetWindowLongW(hwnd, GWL_STYLE) as u32;
    MapWindowPoints(hwnd, if style & WS_CHILD != 0 { GetParent(hwnd) } else { 0 }, &mut client as *mut RECT as *mut POINT, 2);
    for (bar, axis, flag) in [(state.vertical, SB_VERT, WS_VSCROLL), (state.horizontal, SB_HORZ, WS_HSCROLL)] {
        if bar == 0 { continue; }
        let visible = style & flag != 0 && IsWindowVisible(hwnd) != 0;
        if visible {
            let mut info: SCROLLINFO = mem::zeroed();
            info.cbSize = mem::size_of::<SCROLLINFO>() as u32;
            info.fMask = SIF_RANGE | SIF_PAGE | SIF_POS;
            GetScrollInfo(hwnd, axis, &mut info);
            info.fMask |= SIF_DISABLENOSCROLL;
            SetScrollInfo(bar, SB_CTL, &info, 1);
            let (x,y,w,h) = if axis == SB_VERT {
                (client.right, client.top, WIDTH, (client.bottom-client.top).max(1))
            } else {
                (client.left, client.bottom, (client.right-client.left).max(1), WIDTH)
            };
            SetWindowPos(bar, HWND_TOP, x,y,w,h, SWP_NOACTIVATE | SWP_SHOWWINDOW);
        } else if GetWindowLongW(bar, GWL_STYLE) as u32 & WS_VISIBLE != 0 {
            ShowWindow(bar, SW_HIDE);
        }
    }
    state.syncing.set(false);
}

unsafe extern "system" fn parent_proc(
    hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM, id: usize, _: usize,
) -> isize {
    if matches!(msg, WM_VSCROLL | WM_HSCROLL) && lp != 0 {
        let bar = lp as HWND;
        let source = GetPropW(bar, crate::to_wide(SOURCE).as_ptr());
        if source != 0 {
            let mut command = wp;
            if matches!((wp & 0xffff) as i32, SB_THUMBTRACK | SB_THUMBPOSITION) {
                let mut info: SCROLLINFO = mem::zeroed();
                info.cbSize = mem::size_of::<SCROLLINFO>() as u32;
                info.fMask = SIF_TRACKPOS;
                GetScrollInfo(bar, SB_CTL, &mut info);
                command = (wp & 0xffff) | ((info.nTrackPos.max(0) as usize & 0xffff) << 16);
            }
            SendMessageW(source, msg, command, 0);
            sync(source);
            return 0;
        }
    }
    if msg == WM_NCDESTROY { RemoveWindowSubclass(hwnd, Some(parent_proc), id); }
    DefSubclassProc(hwnd, msg, wp, lp)
}

unsafe extern "system" fn proc(
    hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM, id: usize, _: usize,
) -> isize {
    if msg == WM_NCDESTROY {
        RemoveWindowSubclass(hwnd, Some(proc), id);
        let data = RemovePropW(hwnd, crate::to_wide(STATE).as_ptr()) as *mut State;
        if !data.is_null() {
            let state = Box::from_raw(data);
            for bar in [state.vertical, state.horizontal] {
                if GetPropW(bar, crate::to_wide(SOURCE).as_ptr()) == hwnd { DestroyWindow(bar); }
            }
        }
        return DefSubclassProc(hwnd, msg, wp, lp);
    }
    let result = DefSubclassProc(hwnd, msg, wp, lp);
    if msg == WM_NCHITTEST && matches!(result as u32, HTVSCROLL | HTHSCROLL) {
        let mut point = POINT { x: lp as i16 as i32, y: (lp >> 16) as i16 as i32 };
        let mut client: RECT = mem::zeroed();
        ScreenToClient(hwnd, &mut point);
        GetClientRect(hwnd, &mut client);
        if point.x >= 0 && point.x < client.right && point.y >= 0 && point.y < client.bottom {
            return HTCLIENT as isize;
        }
    }
    if msg == WM_NCCALCSIZE && lp != 0 {
        // 为窄条保留非客户区；控件原有的滚动范围与键盘操作保持有效。
        let rect = &mut *(lp as *mut RECT);
        let style = GetWindowLongW(hwnd, GWL_STYLE) as u32;
        if style & WS_VSCROLL != 0 { rect.right += (GetSystemMetrics(SM_CXVSCROLL) - WIDTH).max(0); }
        if style & WS_HSCROLL != 0 { rect.bottom += (GetSystemMetrics(SM_CYHSCROLL) - WIDTH).max(0); }
    } else if matches!(msg, WM_WINDOWPOSCHANGED | WM_STYLECHANGED | WM_PAINT | WM_SETTEXT
        | WM_VSCROLL | WM_HSCROLL | WM_MOUSEWHEEL | WM_KEYDOWN | WM_SHOWWINDOW)
        || (0x00b0..=0x00df).contains(&msg) || (0x0180..=0x01b0).contains(&msg)
        || (0x1000..=0x10ff).contains(&msg)
    {
        sync(hwnd);
    }
    result
}
