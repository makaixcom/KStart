//! 弹窗统一缩放、子控件布局和皮肤管理器左右分隔条。
use std::{cell::RefCell, mem, ptr};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, POINT, RECT, WPARAM},
    Graphics::Gdi::*,
    UI::{
        Input::KeyboardAndMouse::{ReleaseCapture, SetCapture},
        Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
        WindowsAndMessaging::*,
    },
};

const SUBCLASS: usize = 0x4b5301;
const GRIP: i32 = 0x4b53;
#[derive(Clone)]
struct Child {
    hwnd: HWND,
    rect: RECT,
}
#[derive(Clone)]
struct Layout {
    base: (i32, i32),
    children: Vec<Child>,
    grip: HWND,
    custom: bool,
    skin: bool,
    split: i32,
    dragging: bool,
}

unsafe fn client(hwnd: HWND) -> RECT {
    let mut r = mem::zeroed();
    GetClientRect(hwnd, &mut r);
    r
}
unsafe fn move_control(hwnd: HWND, x: i32, y: i32, w: i32, h: i32) {
    SetWindowPos(
        hwnd,
        0,
        x,
        y,
        w.max(1),
        h.max(1),
        SWP_NOZORDER | SWP_NOACTIVATE,
    );
}
unsafe extern "system" fn collect(hwnd: HWND, param: LPARAM) -> i32 {
    let (parent, children) = &mut *(param as *mut (HWND, Vec<Child>));
    if GetParent(hwnd) == *parent {
        let mut rect = mem::zeroed();
        GetWindowRect(hwnd, &mut rect);
        MapWindowPoints(0, *parent, &mut rect as *mut RECT as *mut POINT, 2);
        children.push(Child { hwnd, rect });
    }
    1
}

/// 创建控件后、释放 APP_STATE 借用后安装，窗口自身持有布局状态。
pub unsafe fn install(hwnd: HWND, custom: bool) {
    let base = client(hwnd);
    let mut children = (hwnd, Vec::new());
    EnumChildWindows(hwnd, Some(collect), &mut children as *mut _ as LPARAM);
    let skin = GetDlgItem(hwnd, crate::native_dialogs::ID_SKIN_LIST) != 0;
    let grip = CreateWindowExW(
        0,
        crate::to_wide("SCROLLBAR").as_ptr(),
        ptr::null(),
        WS_CHILD | WS_VISIBLE | SBS_SIZEGRIP as u32 | SBS_SIZEBOXBOTTOMRIGHTALIGN as u32,
        base.right - 15,
        base.bottom - 15,
        15,
        15,
        hwnd,
        GRIP as _,
        0,
        ptr::null(),
    );
    let state = Box::new(RefCell::new(Layout {
        base: (base.right, base.bottom),
        children: children.1,
        grip,
        custom,
        skin,
        split: 194,
        dragging: false,
    }));
    let data = Box::into_raw(state);
    SetPropW(
        hwnd,
        crate::to_wide("KStart.DialogLayout").as_ptr(),
        data as isize,
    );
    SetWindowSubclass(hwnd, Some(proc), SUBCLASS, data as usize);
    let style = GetWindowLongW(hwnd, GWL_STYLE) as u32 | WS_THICKFRAME | WS_CLIPCHILDREN;
    SetWindowLongW(hwnd, GWL_STYLE, style as i32);
    let mut bounds = RECT {
        right: base.right,
        bottom: base.bottom,
        ..mem::zeroed()
    };
    if !custom {
        AdjustWindowRectEx(
            &mut bounds,
            style,
            0,
            GetWindowLongW(hwnd, GWL_EXSTYLE) as u32,
        );
    }
    SetWindowPos(
        hwnd,
        0,
        0,
        0,
        bounds.right - bounds.left,
        bounds.bottom - bounds.top,
        SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
    );
}

pub unsafe fn skin_split(hwnd: HWND) -> i32 {
    let data = GetPropW(hwnd, crate::to_wide("KStart.DialogLayout").as_ptr());
    if data != 0 {
        if let Ok(state) = (*(data as *const RefCell<Layout>)).try_borrow() {
            return state.split;
        }
    }
    194
}

pub unsafe fn base_size(hwnd: HWND) -> (i32, i32) {
    let data = GetPropW(hwnd, crate::to_wide("KStart.DialogLayout").as_ptr());
    if data != 0 {
        if let Ok(state) = (*(data as *const RefCell<Layout>)).try_borrow() {
            return state.base;
        }
    }
    let size = client(hwnd);
    (size.right.max(1), size.bottom.max(1))
}

/// 自绘背景和控件使用同一基准坐标，缩放后绘制与点击保持一致。
pub unsafe fn begin_paint_scale(hwnd: HWND, dc: HDC, base: (i32, i32)) -> i32 {
    let saved = SaveDC(dc);
    let size = client(hwnd);
    SetMapMode(dc, MM_ANISOTROPIC);
    SetWindowExtEx(dc, base.0, base.1, ptr::null_mut());
    SetViewportExtEx(dc, size.right.max(1), size.bottom.max(1), ptr::null_mut());
    saved
}

pub unsafe fn logical_mouse(hwnd: HWND, lp: LPARAM, base: (i32, i32)) -> LPARAM {
    let size = client(hwnd);
    let x = (lp as i16 as i32) * base.0 / size.right.max(1);
    let y = ((lp >> 16) as i16 as i32) * base.1 / size.bottom.max(1);
    (x as u16 as usize | ((y as u16 as usize) << 16)) as LPARAM
}

fn split_limit(split: i32, width: i32) -> i32 {
    split.clamp(120, (width - 160).max(120))
}
unsafe fn arrange(hwnd: HWND, state: &Layout) {
    let size = client(hwnd);
    let (w, h) = (size.right.max(1), size.bottom.max(1));
    let system = crate::hotkey_dialog::layout_system_dialog(hwnd);
    for child in &state.children {
        let r = child.rect;
        if state.skin || system {
            continue;
        }
        move_control(
            child.hwnd,
            r.left * w / state.base.0,
            r.top * h / state.base.1,
            (r.right - r.left) * w / state.base.0,
            (r.bottom - r.top) * h / state.base.1,
        );
    }
    if state.skin {
        use crate::native_dialogs::{ID_SKIN_APPLY, ID_SKIN_AUTHOR_EDIT, ID_SKIN_LIST};
        let split = split_limit(state.split, w);
        move_control(GetDlgItem(hwnd, ID_SKIN_LIST), 12, 12, split - 18, h - 64);
        move_control(
            GetDlgItem(hwnd, ID_SKIN_AUTHOR_EDIT),
            split + 6,
            (h - 146).max(100),
            w - split - 18,
            94,
        );
        move_control(GetDlgItem(hwnd, ID_SKIN_APPLY), w - 178, h - 40, 78, 26);
        move_control(GetDlgItem(hwnd, IDCANCEL), w - 90, h - 40, 78, 26);
    }
    move_control(state.grip, w - 15, h - 15, 15, 15);
    InvalidateRect(hwnd, ptr::null(), 1);
}

unsafe extern "system" fn proc(
    hwnd: HWND,
    msg: u32,
    wp: WPARAM,
    lp: LPARAM,
    _: usize,
    data: usize,
) -> isize {
    let cell = &*(data as *const RefCell<Layout>);
    if msg == WM_NCDESTROY {
        RemovePropW(hwnd, crate::to_wide("KStart.DialogLayout").as_ptr());
        RemoveWindowSubclass(hwnd, Some(proc), SUBCLASS);
        let result = DefSubclassProc(hwnd, msg, wp, lp);
        drop(Box::from_raw(data as *mut RefCell<Layout>));
        return result;
    }
    // 状态仅用于计算布局；会同步发送消息的窗口操作放到借用之外。
    if msg == WM_SIZE && wp != SIZE_MINIMIZED as usize {
        let layout = cell.try_borrow().ok().map(|s| s.clone());
        if let Some(layout) = layout {
            arrange(hwnd, &layout);
        }
    }
    if msg == WM_GETMINMAXINFO {
        if let Ok(s) = cell.try_borrow() {
            let mut r = RECT {
                right: s.base.0 * 4 / 5,
                bottom: s.base.1 * 4 / 5,
                ..mem::zeroed()
            };
            if !s.custom {
                AdjustWindowRectEx(
                    &mut r,
                    GetWindowLongW(hwnd, GWL_STYLE) as u32,
                    0,
                    GetWindowLongW(hwnd, GWL_EXSTYLE) as u32,
                );
            }
            let limits = &mut *(lp as *mut MINMAXINFO);
            limits.ptMinTrackSize = POINT {
                x: r.right - r.left,
                y: r.bottom - r.top,
            };
            return 0;
        }
    }
    let Ok(mut state) = cell.try_borrow_mut() else {
        return DefSubclassProc(hwnd, msg, wp, lp);
    };
    if state.custom && msg == WM_NCCALCSIZE {
        return 0;
    }
    if msg == WM_NCHITTEST && state.custom {
        let mut point = POINT {
            x: lp as i16 as i32,
            y: (lp >> 16) as i16 as i32,
        };
        ScreenToClient(hwnd, &mut point);
        let r = client(hwnd);
        if point.x >= r.right - 16 && point.y >= r.bottom - 16 {
            return HTBOTTOMRIGHT as isize;
        }
    }
    if state.skin {
        let x = lp as i16 as i32;
        let y = (lp >> 16) as i16 as i32;
        let r = client(hwnd);
        let near =
            (x - split_limit(state.split, r.right)).abs() <= 5 && y >= 12 && y < r.bottom - 52;
        if msg == WM_LBUTTONDOWN && near {
            state.dragging = true;
            drop(state);
            SetCapture(hwnd);
            return 0;
        }
        if msg == WM_MOUSEMOVE && state.dragging {
            state.split = split_limit(x, r.right);
            drop(state);
            SendMessageW(hwnd, WM_SIZE, 0, 0);
            return 0;
        }
        if msg == WM_LBUTTONUP && state.dragging {
            state.dragging = false;
            drop(state);
            ReleaseCapture();
            return 0;
        }
        if msg == WM_CAPTURECHANGED {
            state.dragging = false;
        }
        if msg == WM_SETCURSOR && (lp as u16 as u32 == HTCLIENT) {
            let mut point = mem::zeroed();
            GetCursorPos(&mut point);
            ScreenToClient(hwnd, &mut point);
            if (point.x - split_limit(state.split, r.right)).abs() <= 5
                && point.y >= 12
                && point.y < r.bottom - 52
            {
                SetCursor(LoadCursorW(0, IDC_SIZEWE));
                return 1;
            }
        }
    }
    drop(state);
    DefSubclassProc(hwnd, msg, wp, lp)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn skin_split_keeps_both_panes_visible() {
        assert_eq!(split_limit(-100, 430), 120);
        assert_eq!(split_limit(999, 430), 270);
        assert_eq!(split_limit(400, 800), 400);
    }
}
