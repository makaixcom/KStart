//! 高级设置中的主窗体与托盘行为。

use crate::{
    advanced::{AdvancedSettings, Key},
    popup, to_wide, APP_STATE,
};
use std::{
    mem, ptr,
    time::{Duration, Instant},
};
use windows_sys::Win32::Foundation::{HWND, POINT, RECT};
use windows_sys::Win32::Graphics::Gdi::{
    GetMonitorInfoW, InvalidateRect, MonitorFromRect, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, VK_CONTROL, VK_MBUTTON, VK_MENU, VK_RBUTTON, VK_SHIFT,
};
use windows_sys::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AnimateWindow, GetCursorPos, GetForegroundWindow, GetSystemMetrics, GetWindowRect, IsIconic,
    IsWindowVisible, LoadIconW, SetForegroundWindow, SetWindowPos, ShowWindow, AW_BLEND, AW_HIDE,
    IDI_APPLICATION, SM_CXSCREEN, SM_CYSCREEN, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER, SW_HIDE,
    SW_MINIMIZE, SW_RESTORE, SW_SHOWNOACTIVATE, WM_APP, WM_LBUTTONUP, WM_RBUTTONUP,
};

pub const TRAY_MESSAGE: u32 = WM_APP + 22;
pub const TIMER_ID: usize = 22;

/// 整体透明度由系统合成窗口，实现透见后方窗口，而非混合列表底色。
pub unsafe fn apply_opacity(hwnd: HWND) {
    let percent = APP_STATE.with(|cell| {
        let guard = cell.try_borrow().ok()?;
        Some(guard.as_ref()?.user_cfg.advanced.number(Key::ListOpacity))
    });
    if let Some(percent) = percent { set_window_opacity(hwnd, percent); }
}

unsafe fn set_window_opacity(hwnd: HWND, percent: u32) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetWindowLongPtrW, SetWindowLongPtrW,
        SetLayeredWindowAttributes, GWL_EXSTYLE, WS_EX_LAYERED, LWA_ALPHA};
    let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
    if style & WS_EX_LAYERED as isize == 0 {
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style | WS_EX_LAYERED as isize);
    }
    SetLayeredWindowAttributes(hwnd, 0, (percent.clamp(10, 100) * 255 / 100) as u8, LWA_ALPHA);
}

#[cfg(test)]
mod opacity_tests {
    use super::*;
    use windows_sys::Win32::UI::WindowsAndMessaging::*;

    #[test]
    fn whole_window_alpha_can_be_reduced_and_restored() {
        unsafe {
            let hwnd = CreateWindowExW(WS_EX_TOOLWINDOW, to_wide("STATIC").as_ptr(),
                ptr::null(), WS_POPUP, 0, 0, 40, 40, 0, 0, 0, ptr::null());
            assert_ne!(hwnd, 0);
            let mut observed = Vec::new();
            for percent in [100, 40, 0, 100] {
                set_window_opacity(hwnd, percent);
                let mut alpha = 0;
                let mut flags = 0;
                let success = GetLayeredWindowAttributes(hwnd, ptr::null_mut(), &mut alpha, &mut flags);
                observed.push((success != 0, alpha, flags));
            }
            let preserved_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) & WS_EX_TOOLWINDOW as isize != 0;
            DestroyWindow(hwnd);
            assert!(preserved_style);
            assert_eq!(observed, vec![(true, 255, LWA_ALPHA), (true, 102, LWA_ALPHA),
                (true, 25, LWA_ALPHA), (true, 255, LWA_ALPHA)]);
        }
    }
}

#[derive(Default)]
pub struct Runtime {
    pub visibility_dirty: bool,
    pub stretch_pending: bool,
    tray_double_at: Option<Instant>,
    pub advanced_page: usize,
    pub tray_added: bool,
    pub popup_hidden: bool,
    resolution_hidden: bool,
    restore_after_resolution: bool,
    baseline: Option<(i32, i32)>,
    button_pressed: bool,
    hover_started: Option<(crate::renderer::HitTarget, Instant)>,
    inside: bool,
    dock: Option<(RECT, Instant)>,
    dock_hidden: Option<DockEdge>,
    stretched: Option<(i32, i32)>,
    pub shortcut_scroll: usize,
    pub shortcut_scroll_grab: Option<i32>,
    pub volume_indicator: Option<(u8, Instant)>,
    tool_hover: Option<(usize, Instant)>,
    pub tooltip: Option<(i32, i32, String)>,
    pub tooltip_hwnd: HWND,
    pub shown_tooltip: Option<(i32, i32, String, u32)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DockEdge { Left, Right, Top, Bottom }

impl DockEdge {
    fn detect(rect: RECT, work: RECT) -> Option<Self> {
        if (rect.left - work.left).abs() <= 3 { Some(Self::Left) }
        else if (rect.right - work.right).abs() <= 3 { Some(Self::Right) }
        else if (rect.top - work.top).abs() <= 3 { Some(Self::Top) }
        else if (rect.bottom - work.bottom).abs() <= 3 { Some(Self::Bottom) }
        else { None }
    }

    fn strip(self, width: i32, height: i32) -> RECT {
        match self {
            Self::Left => RECT { left: 0, top: 0, right: 3.min(width), bottom: height },
            Self::Right => RECT { left: (width - 3).max(0), top: 0, right: width, bottom: height },
            Self::Top => RECT { left: 0, top: 0, right: width, bottom: 3.min(height) },
            Self::Bottom => RECT { left: 0, top: (height - 3).max(0), right: width, bottom: height },
        }
    }

    fn contains(self, rect: RECT, cursor: POINT) -> bool {
        let strip = self.strip(rect.right - rect.left, rect.bottom - rect.top);
        let x = cursor.x - rect.left;
        let y = cursor.y - rect.top;
        x >= strip.left && x < strip.right && y >= strip.top && y < strip.bottom
    }
}

/// 保持窗口尺寸和位置，仅裁出靠边细条，避免多屏时露到相邻屏幕。
unsafe fn apply_dock_region(hwnd: HWND, edge: Option<DockEdge>) {
    use windows_sys::Win32::Graphics::Gdi::{CreateRectRgn, CombineRgn, DeleteObject, RGN_AND};
    let region = APP_STATE.with(|cell| {
        let guard = cell.try_borrow().ok()?;
        let app = guard.as_ref()?;
        let width = app.user_cfg.window_width;
        let height = app.user_cfg.window_height;
        let mut region = crate::build_window_corner_region(width, height, &app.skin_cfg, &app.skin_assets);
        if let Some(edge) = edge {
            let rect = edge.strip(width, height);
            let strip = CreateRectRgn(rect.left, rect.top, rect.right, rect.bottom);
            if region == 0 { region = strip; }
            else { CombineRgn(region, region, strip, RGN_AND); DeleteObject(strip); }
        }
        Some(region)
    });
    if let Some(region) = region {
        crate::apply_hrgn_safely(hwnd, region);
    }
}

pub unsafe fn restore_docked(hwnd: HWND) {
    let restore = APP_STATE.with(|cell| {
        let Ok(mut guard) = cell.try_borrow_mut() else { return false; };
        let Some(app) = guard.as_mut() else { return false; };
        app.advanced_window.dock = None;
        app.advanced_window.dock_hidden.take().is_some()
    });
    if restore { apply_dock_region(hwnd, None); }
}

#[allow(clippy::too_many_arguments)]
fn dock_step(state: &mut Runtime, enabled: bool, can_hide: bool, rect: RECT, work: RECT,
    cursor: POINT, hide_delay: Duration, show_delay: Duration, now: Instant) -> Option<Option<DockEdge>> {
    let edge = DockEdge::detect(rect, work);
    if !enabled || !can_hide || edge.is_none() {
        state.dock = None;
        return state.dock_hidden.take().map(|_| None);
    }
    if let Some(hidden) = state.dock_hidden {
        if hidden.contains(rect, cursor) {
            let since = state.dock.get_or_insert((rect, now)).1;
            if now.duration_since(since) >= show_delay {
                state.dock = None;
                state.dock_hidden = None;
                return Some(None);
            }
        } else { state.dock = None; }
    } else {
        let inside = cursor.x >= rect.left && cursor.x < rect.right && cursor.y >= rect.top && cursor.y < rect.bottom;
        if inside { state.dock = None; }
        else {
            let since = state.dock.get_or_insert((rect, now)).1;
            if now.duration_since(since) >= hide_delay {
                state.dock_hidden = edge;
                state.dock = None;
                state.tooltip = None;
                state.tool_hover = None;
                return Some(edge);
            }
        }
    }
    None
}

#[cfg(test)]
mod docking_tests {
    use super::*;

    #[test]
    fn moving_stays_within_monitor_work_area() {
        let work = RECT { left: -1920, top: 30, right: 0, bottom: 1040 };
        for (x, y) in [(-2200, -40), (-100, 800), (-1800, 200)] {
            let rect = constrain_to_work_area(RECT { left: x, top: y, right: x + 350, bottom: y + 600 }, work);
            assert!(rect.left >= work.left && rect.top >= work.top);
            assert!(rect.right <= work.right && rect.bottom <= work.bottom);
            assert_eq!((rect.right - rect.left, rect.bottom - rect.top), (350, 600));
        }
        let oversized = constrain_to_work_area(RECT { left: -2200, top: -50, right: 400, bottom: 2000 }, work);
        assert_eq!((oversized.left, oversized.top, oversized.right, oversized.bottom),
            (work.left, work.top, work.right, work.bottom));
    }

    #[test]
    fn tray_click_and_visibility_follow_settings() {
        use windows_sys::Win32::UI::WindowsAndMessaging::WM_LBUTTONDBLCLK;
        assert!(!tray_click_activates(WM_LBUTTONUP, false, false));
        assert!(tray_click_activates(WM_LBUTTONUP, true, false));
        assert!(tray_click_activates(WM_LBUTTONDBLCLK, false, false));
        assert!(tray_click_activates(WM_RBUTTONUP, false, false));
        for visible in [false, true] {
            for minimized in [false, true] {
                assert!(should_show_tray(0, visible, minimized));
                assert!(!should_show_tray(1, visible, minimized));
                assert_eq!(should_show_tray(2, visible, minimized), !visible || minimized);
            }
        }
    }

    #[test]
    fn stretched_side_window_obeys_distinct_hide_and_show_delays() {
        let work = RECT { left: 0, top: 0, right: 1920, bottom: 1040 };
        let rect = RECT { left: 0, top: 0, right: 350, bottom: 1040 };
        let mut state = Runtime::default();
        let start = Instant::now();
        let hide = Duration::from_millis(600);
        let show = Duration::from_millis(300);
        let outside = POINT { x: 800, y: 500 };
        let strip = POINT { x: 1, y: 500 };
        assert_eq!(dock_step(&mut state, true, true, rect, work, outside, hide, show, start), None);
        assert_eq!(dock_step(&mut state, true, true, rect, work, outside, hide, show, start + hide / 2), None);
        assert_eq!(dock_step(&mut state, true, true, rect, work, outside, hide, show, start + hide), Some(Some(DockEdge::Left)));
        assert_eq!(dock_step(&mut state, true, true, rect, work, strip, hide, show, start + hide), None);
        assert_eq!(dock_step(&mut state, true, true, rect, work, strip, hide, show, start + hide + show / 2), None);
        assert_eq!(dock_step(&mut state, true, true, rect, work, strip, hide, show, start + hide + show), Some(None));
    }

    #[test]
    fn four_edges_keep_a_three_pixel_strip_and_obey_hover_delays() {
        let work = RECT { left: -1920, top: 0, right: 0, bottom: 1040 };
        for (rect, edge) in [
            (RECT { left: -1920, top: 200, right: -1620, bottom: 700 }, DockEdge::Left),
            (RECT { left: -300, top: 200, right: 0, bottom: 700 }, DockEdge::Right),
            (RECT { left: -1500, top: 0, right: -1200, bottom: 500 }, DockEdge::Top),
            (RECT { left: -1500, top: 540, right: -1200, bottom: 1040 }, DockEdge::Bottom),
        ] {
            let mut state = Runtime::default();
            let now = Instant::now();
            let delay = Duration::from_millis(200);
            let outside = POINT { x: 500, y: 500 };
            assert_eq!(dock_step(&mut state, true, true, rect, work, outside, delay, delay, now), None);
            assert_eq!(dock_step(&mut state, true, true, rect, work, outside, delay, delay, now + delay), Some(Some(edge)));
            let strip = edge.strip(300, 500);
            assert!(strip.right - strip.left == 3 || strip.bottom - strip.top == 3);
            let center = POINT { x: (rect.left + rect.right) / 2, y: (rect.top + rect.bottom) / 2 };
            assert_eq!(dock_step(&mut state, true, true, rect, work, center, delay, delay, now + delay * 2), None);
            assert_eq!(state.dock_hidden, Some(edge));
            let hover = POINT { x: rect.left + (strip.left + strip.right) / 2,
                y: rect.top + (strip.top + strip.bottom) / 2 };
            assert_eq!(dock_step(&mut state, true, true, rect, work, hover, delay, delay, now + delay * 3), None);
            assert_eq!(dock_step(&mut state, true, true, rect, work, hover, delay, delay, now + delay * 4), Some(None));
            assert!(state.dock_hidden.is_none());
            state.dock_hidden = Some(edge);
            assert_eq!(dock_step(&mut state, false, true, rect, work, outside, delay, delay, now), Some(None));
        }
    }
}

unsafe fn tray_data(hwnd: HWND) -> NOTIFYICONDATAW {
    let mut info: NOTIFYICONDATAW = mem::zeroed();
    info.cbSize = mem::size_of::<NOTIFYICONDATAW>() as u32;
    info.hWnd = hwnd;
    info.uID = 1;
    info.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
    info.uCallbackMessage = TRAY_MESSAGE;
    info.hIcon = LoadIconW(windows_sys::Win32::System::LibraryLoader::GetModuleHandleW(ptr::null()), 1usize as *const u16);
    if info.hIcon == 0 { info.hIcon = LoadIconW(0, IDI_APPLICATION); }
    for (dst, src) in info.szTip.iter_mut().zip(to_wide("KStart")) {
        *dst = src;
    }
    info
}

pub unsafe fn update_tray(hwnd: HWND) {
    if hwnd == 0 {
        return;
    }
    let hidden = IsWindowVisible(hwnd) == 0;
    let minimized = IsIconic(hwnd) != 0;
    let desired = APP_STATE.with(|cell| {
        let guard = cell.try_borrow().ok()?;
        let app = guard.as_ref()?;
        let settings = &app.user_cfg.advanced;
        let show = should_show_tray(settings.number(Key::TrayVisibility), !hidden, minimized);
        (show != app.advanced_window.tray_added).then_some(show)
    });
    if let Some(show) = desired {
        let changed =
            Shell_NotifyIconW(if show { NIM_ADD } else { NIM_DELETE }, &tray_data(hwnd)) != 0;
        APP_STATE.with(|cell| {
            if let Ok(mut guard) = cell.try_borrow_mut() {
                if let Some(app) = guard.as_mut() {
                    if changed || !show {
                        app.advanced_window.tray_added = show;
                    }
                }
            }
        });
    }
    let tray = APP_STATE.with(|cell| cell.try_borrow().ok()
        .and_then(|guard| guard.as_ref().map(|app| app.advanced_window.tray_added))).unwrap_or(false);
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetWindowLongPtrW, SetWindowLongPtrW,
        GWL_EXSTYLE, WS_EX_TOOLWINDOW, WS_EX_APPWINDOW, SWP_NOMOVE, SWP_FRAMECHANGED};
    let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
    let next = if tray { (style | WS_EX_TOOLWINDOW as isize) & !(WS_EX_APPWINDOW as isize) }
        else { (style | WS_EX_APPWINDOW as isize) & !(WS_EX_TOOLWINDOW as isize) };
    if style != next {
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, next);
        SetWindowPos(hwnd, 0, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED);
    }
}

fn should_show_tray(mode: u32, visible: bool, minimized: bool) -> bool {
    mode == 0 || mode == 2 && (!visible || minimized)
}

fn tray_click_activates(event: u32, single_click: bool, tray_start: bool) -> bool {
    event == windows_sys::Win32::UI::WindowsAndMessaging::WM_LBUTTONDBLCLK
        || event == WM_RBUTTONUP || event == WM_LBUTTONUP && (single_click || tray_start)
}

pub unsafe fn sync_list_visibility(hwnd: HWND) {
    restore_docked(hwnd);
    APP_STATE.with(|cell| { if let Ok(mut guard) = cell.try_borrow_mut() {
        if let Some(app) = guard.as_mut() { app.advanced_window.popup_hidden = !app.user_cfg.enable_list; }
    }});
    update_tray(hwnd);
    let state = APP_STATE.with(|cell| {
        let guard = cell.try_borrow().ok()?;
        let app = guard.as_ref()?;
        Some((app.user_cfg.enable_list, app.advanced_window.tray_added))
    });
    match state {
        Some((true, _)) => { ShowWindow(hwnd, SW_RESTORE); }
        Some((false, true)) => { ShowWindow(hwnd, SW_HIDE); }
        Some((false, false)) => { ShowWindow(hwnd, SW_MINIMIZE); }
        None => {}
    }
    update_tray(hwnd);
}

pub unsafe fn activate_main(hwnd: HWND) {
    APP_STATE.with(|cell| { if let Ok(mut guard) = cell.try_borrow_mut() {
        if let Some(app) = guard.as_mut() {
            app.user_cfg.enable_list = true;
            app.advanced_window.popup_hidden = false;
            app.user_cfg.save(&app.base_dir);
        }
    }});
    sync_list_visibility(hwnd);
    SetForegroundWindow(hwnd);
}

pub unsafe fn minimize_main(hwnd: HWND) {
    save_position(hwnd);
    restore_docked(hwnd);
    let tray_mode = APP_STATE.with(|cell| cell.try_borrow().ok().and_then(|guard|
        guard.as_ref().map(|app| app.user_cfg.advanced.number(Key::TrayVisibility)))).unwrap_or(1);
    ShowWindow(hwnd, if tray_mode == 1 { SW_MINIMIZE } else { SW_HIDE });
    update_tray(hwnd);
}

pub unsafe fn save_position(hwnd: HWND) {
    if IsIconic(hwnd) != 0 { return; }
    let mut rect: RECT = mem::zeroed();
    if GetWindowRect(hwnd, &mut rect) == 0 { return; }
    APP_STATE.with(|cell| { if let Ok(mut guard) = cell.try_borrow_mut() {
        if let Some(app) = guard.as_mut() {
            app.user_cfg.window_position = Some((rect.left, rect.top));
            app.user_cfg.save(&app.base_dir);
        }
    }});
}

pub unsafe fn remove_tray(hwnd: HWND) {
    Shell_NotifyIconW(NIM_DELETE, &tray_data(hwnd));
}

pub unsafe fn tray_click(hwnd: HWND, event: u32) {
    use windows_sys::Win32::UI::WindowsAndMessaging::WM_LBUTTONDBLCLK;
    if event != WM_LBUTTONUP && event != WM_RBUTTONUP && event != WM_LBUTTONDBLCLK {
        return;
    }
    let ignore_up = APP_STATE.with(|cell| {
        let Ok(mut guard) = cell.try_borrow_mut() else { return false; };
        let Some(app) = guard.as_mut() else { return false; };
        if event == WM_LBUTTONDBLCLK { app.advanced_window.tray_double_at = Some(Instant::now()); }
        event == WM_LBUTTONUP && app.advanced_window.tray_double_at
            .is_some_and(|then| then.elapsed() < Duration::from_millis(300))
    });
    if ignore_up { return; }
    let allow = APP_STATE.with(|cell| cell.try_borrow().ok().and_then(|guard|
        guard.as_ref().map(|app| tray_click_activates(event,
            app.user_cfg.advanced.enabled(Key::TrayClickShowsList),
            app.user_cfg.advanced.enabled(Key::TrayStart))))).unwrap_or(false);
    if !allow { return; }
    restore_docked(hwnd);
    let mut pointer = POINT { x: 0, y: 0 };
    GetCursorPos(&mut pointer);
    let menu = APP_STATE.with(|cell| {
        let mut guard = cell.try_borrow_mut().ok()?;
        let app = guard.as_mut()?;
        let settings = &app.user_cfg.advanced;
        if event == WM_RBUTTONUP {
            app.popup_menu = crate::renderer::ActivePopupMenu::MainMenu;
            app.menu_scroll = 0;
            app.advanced_window.tooltip = None;
            return Some(app.popup_menu);
        }
        if event == WM_LBUTTONUP && settings.enabled(Key::TrayStart) {
            let category = app
                .categories
                .iter()
                .position(|c| c.id == settings.text(Key::TrayCategory))
                .unwrap_or(app.active_category);
            if let Some(cat) = app.categories.get(category) {
                let count = cat.groups.iter().map(|group| group.items.len()).sum();
                if count != 0 {
                    let menu = crate::renderer::ActivePopupMenu::TrayStart {
                        category,
                        count,
                        x: pointer.x,
                        y: pointer.y,
                    };
                    app.popup_menu = menu;
                    app.menu_scroll = 0;
                    return Some(menu);
                }
            }
        }
        app.user_cfg.enable_list = true;
        app.popup_menu = crate::renderer::ActivePopupMenu::None;
        app.user_cfg.save(&app.base_dir);
        app.advanced_window.popup_hidden = false;
        app.advanced_window.dock = None;
        None
    });
    if menu.is_some() {
        if event == WM_RBUTTONUP {
            popup::sync_from_tray(hwnd, pointer);
        } else {
            popup::sync(hwnd);
        }
    } else {
        popup::sync(hwnd);
        activate_main(hwnd);
        InvalidateRect(hwnd, ptr::null(), 0);
    }
}

pub unsafe fn hide_after_launch(hwnd: HWND, run_action: u32, item_minimizes: bool, animate: bool) {
    if run_action == 2 {
        APP_STATE.with(|cell| {
            if let Ok(mut guard) = cell.try_borrow_mut() {
                if let Some(app) = guard.as_mut() {
                    app.advanced_window.popup_hidden = true;
                }
            }
        });
        update_tray(hwnd);
        let accessible = APP_STATE.with(|cell| {
            cell.try_borrow().ok().is_some_and(|guard| {
                guard
                    .as_ref()
                    .is_some_and(|app| app.advanced_window.tray_added)
            })
        });
        if accessible {
            hide(hwnd, animate);
        } else {
            ShowWindow(hwnd, SW_MINIMIZE);
        }
    } else if run_action == 1 || item_minimizes {
        if animate {
            AnimateWindow(hwnd, 120, AW_HIDE | AW_BLEND);
        }
        minimize_main(hwnd);
    }
    update_tray(hwnd);
}

unsafe fn hide(hwnd: HWND, animate: bool) {
    if !animate || AnimateWindow(hwnd, 120, AW_HIDE | AW_BLEND) == 0 {
        ShowWindow(hwnd, SW_HIDE);
    }
}

unsafe fn working_area(rect: &RECT) -> Option<RECT> {
    let mut monitor: MONITORINFO = mem::zeroed();
    monitor.cbSize = mem::size_of_val(&monitor) as u32;
    (GetMonitorInfoW(
        MonitorFromRect(rect, MONITOR_DEFAULTTONEAREST),
        &mut monitor,
    ) != 0)
        .then_some(monitor.rcWork)
}

pub fn constrain_to_work_area(rect: RECT, work: RECT) -> RECT {
    let width = (rect.right - rect.left).clamp(1, (work.right - work.left).max(1));
    let height = (rect.bottom - rect.top).clamp(1, (work.bottom - work.top).max(1));
    let left = rect.left.clamp(work.left, (work.right - width).max(work.left));
    let top = rect.top.clamp(work.top, (work.bottom - height).max(work.top));
    RECT { left, top, right: left + width, bottom: top + height }
}

pub unsafe fn stretch_docked(hwnd: HWND) {
    let mut rect: RECT = mem::zeroed();
    if GetWindowRect(hwnd, &mut rect) == 0 {
        return;
    }
    let Some(work) = working_area(&rect) else {
        return;
    };
    let side_x = if (rect.left - work.left).abs() <= 12 { Some(work.left) }
        else if (rect.right - work.right).abs() <= 12 { Some(work.right - (rect.right - rect.left)) }
        else { None };
    let position = APP_STATE.with(|cell| {
        let mut guard = cell.try_borrow_mut().ok()?;
        let app = guard.as_mut()?;
        if app.user_cfg.advanced.enabled(Key::StretchDocked) && side_x.is_some() {
            app.advanced_window
                .stretched
                .get_or_insert((rect.top, rect.bottom - rect.top));
            let x = side_x.unwrap();
            if rect.left == x && rect.top == work.top && rect.bottom == work.bottom { None }
            else { Some((x, work.top, work.bottom - work.top)) }
        } else {
            let (top, height) = app.advanced_window.stretched.take()?;
            Some((
                rect.left,
                top.clamp(work.top, (work.bottom - height).max(work.top)),
                height,
            ))
        }
    });
    if let Some((x, top, height)) = position {
        restore_docked(hwnd);
        SetWindowPos(
            hwnd,
            0,
            x,
            top,
            rect.right - rect.left,
            height,
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
}

fn delay(settings: &AdvancedSettings, key: Key) -> Duration {
    Duration::from_millis(u64::from(settings.number(key)))
}

unsafe fn pressed(key: u16) -> bool {
    GetAsyncKeyState(i32::from(key)) < 0
}

pub unsafe fn tick(hwnd: HWND) {
    let stretch = APP_STATE.with(|cell| {
        let Ok(mut guard) = cell.try_borrow_mut() else { return false; };
        let Some(app) = guard.as_mut() else { return false; };
        std::mem::take(&mut app.advanced_window.stretch_pending)
    });
    if stretch { stretch_docked(hwnd); save_position(hwnd); }
    let dirty = APP_STATE.with(|cell| {
        let Ok(mut guard) = cell.try_borrow_mut() else { return false; };
        let Some(app) = guard.as_mut() else { return false; };
        std::mem::take(&mut app.advanced_window.visibility_dirty)
    });
    if dirty { sync_list_visibility(hwnd); }
    let mut rect: RECT = mem::zeroed();
    let mut cursor = POINT { x: 0, y: 0 };
    if GetWindowRect(hwnd, &mut rect) == 0 || GetCursorPos(&mut cursor) == 0 {
        return;
    }
    let Some(work) = working_area(&rect) else {
        return;
    };
    let visible = IsWindowVisible(hwnd) != 0;
    let minimized = IsIconic(hwnd) != 0;
    let inside = cursor.x >= rect.left
        && cursor.x < rect.right
        && cursor.y >= rect.top
        && cursor.y < rect.bottom;
    let foreground = GetForegroundWindow() == hwnd;
    let resolution = (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN));
    let now = Instant::now();
    let icons = crate::advanced_web_icon::take_results();
    if !icons.is_empty() {
        APP_STATE.with(|cell| {
            if let Ok(mut guard) = cell.try_borrow_mut() {
                if let Some(app) = guard.as_mut() {
                    if app.user_cfg.advanced.enabled(Key::WebIcons) {
                        for item in app
                            .categories
                            .iter_mut()
                            .flat_map(|c| &mut c.groups)
                            .flat_map(|g| &mut g.items)
                        {
                            if item.custom_icon_16.is_none() {
                                if let Some((_, icon)) =
                                    icons.iter().find(|(url, _)| url == &item.target_path)
                                {
                                    item.custom_icon_16 = Some(icon.clone());
                                }
                            }
                        }
                    }
                }
            }
        });
        InvalidateRect(hwnd, ptr::null(), 0);
    }
    let action = APP_STATE.with(|cell| {
        let mut guard = cell.try_borrow_mut().ok()?;
        let app = guard.as_mut()?;
        if app.modal_hwnd != 0 || app.popup_menu != crate::renderer::ActivePopupMenu::None {
            return None;
        }
        let state = &mut app.advanced_window;
        let settings = &app.user_cfg.advanced;
        let indicator_expired = state
            .volume_indicator
            .is_some_and(|(_, at)| now.duration_since(at) >= Duration::from_millis(1500));
        if indicator_expired {
            state.volume_indicator = None;
        }
        let previous_tip = state.tooltip.clone();
        let tooltip_kind = settings.number(Key::Tooltips);
        if let crate::renderer::HitTarget::ShortcutItem(index) = app.hover_target {
            if visible && inside && state.dock_hidden.is_none() && tooltip_kind < 2 {
                if state.tool_hover.is_none_or(|(last, _)| last != index) {
                    state.tool_hover = Some((index, now));
                    state.tooltip = None;
                } else if state.tooltip.is_none()
                    && state.tool_hover.is_some_and(|(_, then)| {
                        now.duration_since(then) >= Duration::from_millis(500)
                    })
                {
                    if let Some(item) = app
                        .categories
                        .get(app.active_category)
                        .and_then(|c| {
                            c.groups
                                .get(c.active_group_index.min(c.groups.len().saturating_sub(1)))
                        })
                        .and_then(|g| g.items.get(index))
                    {
                        let label = {
                            let path = if matches!(item.target_kind,
                                crate::targets::TargetKind::File | crate::targets::TargetKind::Folder) {
                                crate::targets::resolve_path(&app.base_dir, &item.target_path).to_string_lossy().into_owned()
                            } else { item.target_path.clone() };
                            format!("{}\n{}\n{}", item.name.replace(['\r', '\n'], " "), path, item.remark)
                        };
                        state.tooltip =
                            Some((cursor.x - rect.left + 12, cursor.y - rect.top + 17, label));
                    }
                }
            } else {
                state.tool_hover = None;
                state.tooltip = None;
            }
        } else {
            state.tool_hover = None;
            state.tooltip = None;
        }
        let mut show = None;
        let mut center = false;
        let focus = visible
            && inside
            && state.dock_hidden.is_none()
            && !state.inside
            && !foreground
            && settings.enabled(Key::FocusOnHover);
        state.inside = inside;

        if settings.enabled(Key::HideOnLowerResolution) {
            let base = *state.baseline.get_or_insert(resolution);
            let lower = resolution.0 < base.0 || resolution.1 < base.1;
            if lower && !state.resolution_hidden {
                state.resolution_hidden = true;
                state.restore_after_resolution = visible && !minimized;
                if state.restore_after_resolution {
                    show = Some(false);
                }
            } else if !lower && state.resolution_hidden {
                state.resolution_hidden = false;
                if state.restore_after_resolution {
                    show = Some(true);
                }
            }
        } else {
            state.baseline = None;
            if state.resolution_hidden && state.restore_after_resolution {
                show = Some(true);
            }
            state.resolution_hidden = false;
        }

        let button = if settings.enabled(Key::RightToggle) {
            VK_RBUTTON
        } else {
            VK_MBUTTON
        };
        let pressed_now = settings.enabled(Key::MiddleToggle) && pressed(button);
        if pressed_now
            && !state.button_pressed
            && !state.resolution_hidden
            && [
                (Key::ToggleCtrl, VK_CONTROL),
                (Key::ToggleAlt, VK_MENU),
                (Key::ToggleShift, VK_SHIFT),
            ]
            .iter()
            .all(|&(key, vk)| !settings.enabled(key) || pressed(vk))
        {
            let opening = !visible || minimized || state.dock_hidden.is_some();
            if opening { app.user_cfg.enable_list = true; }
            show = Some(opening);
            center = opening && settings.enabled(Key::CenterPointer);
            state.popup_hidden = !opening;
            state.dock = None;
        }
        state.button_pressed = pressed_now;

        let dock_action = if state.dock_hidden.is_none()
            && pressed(windows_sys::Win32::UI::Input::KeyboardAndMouse::VK_LBUTTON) {
            // 拖动、缩放或点击过程中不开始收起计时。
            state.dock = None;
            None
        } else {
            dock_step(state, settings.enabled(Key::AutoHide),
                show.is_none() && visible && !minimized && !state.popup_hidden && !state.resolution_hidden,
                rect, work, cursor, delay(settings, Key::HideDelay), delay(settings, Key::ShowDelay), now)
        };
        let mut changed = indicator_expired || previous_tip != state.tooltip;
        if visible && inside && state.dock_hidden.is_none() && settings.enabled(Key::HoverSwitch) {
            use crate::renderer::HitTarget;
            let target = app.hover_target;
            if matches!(target, HitTarget::CategoryTab(_) | HitTarget::GroupBar(_)) {
                if state.hover_started.is_none_or(|(prev, _)| prev != target) {
                    state.hover_started = Some((target, now));
                }
                if state.hover_started.is_some_and(|(_, since)| {
                    now.duration_since(since) >= delay(settings, Key::HoverDelay)
                }) {
                    match target {
                        HitTarget::CategoryTab(index) if app.active_category != index => {
                            app.active_category = index;
                            changed = true;
                        }
                        HitTarget::GroupBar(index) => {
                            if let Some(cat) = app.categories.get_mut(app.active_category) {
                                if cat.active_group_index != index {
                                    cat.active_group_index = index;
                                    changed = true;
                                }
                            }
                        }
                        _ => {}
                    }
                    if changed {
                        app.selected_shortcuts.clear();
                        state.shortcut_scroll = 0;
                    }
                }
            } else {
                state.hover_started = None;
            }
        } else {
            state.hover_started = None;
        }
        Some((
            show,
            center,
            focus,
            changed,
            settings.enabled(Key::AnimateHide),
            dock_action,
        ))
    });
    if let Some((show, center, focus, changed, animate, dock_action)) = action {
        if let Some(edge) = dock_action { apply_dock_region(hwnd, edge); }
        if center {
            let width = rect.right - rect.left;
            let height = rect.bottom - rect.top;
            let x = (cursor.x - width / 2).clamp(work.left, (work.right - width).max(work.left));
            let y = (cursor.y - height / 2).clamp(work.top, (work.bottom - height).max(work.top));
            SetWindowPos(
                hwnd,
                0,
                x,
                y,
                0,
                0,
                SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOZORDER,
            );
        }
        match show {
            Some(true) if minimized => {
                ShowWindow(hwnd, SW_RESTORE);
            }
            Some(true) => {
                if !animate || AnimateWindow(hwnd, 120, AW_BLEND) == 0 {
                    ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                }
            }
            Some(false) => hide(hwnd, animate),
            None => {}
        }
        if focus || center {
            SetForegroundWindow(hwnd);
        }
        if changed {
            InvalidateRect(hwnd, ptr::null(), 0);
        }
    }
    crate::tooltip::sync(hwnd);
    update_tray(hwnd);
}
