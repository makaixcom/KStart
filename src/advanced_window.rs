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

#[derive(Default)]
pub struct Runtime {
    pub tray_added: bool,
    popup_hidden: bool,
    resolution_hidden: bool,
    restore_after_resolution: bool,
    baseline: Option<(i32, i32)>,
    button_pressed: bool,
    hover_started: Option<(crate::renderer::HitTarget, Instant)>,
    inside: bool,
    dock: Option<(RECT, Instant)>,
    stretched: Option<(i32, i32)>,
    pub shortcut_scroll: usize,
    pub volume_indicator: Option<(u8, Instant)>,
    tool_hover: Option<(usize, Instant)>,
    pub tooltip: Option<(i32, i32, String)>,
}

unsafe fn tray_data(hwnd: HWND) -> NOTIFYICONDATAW {
    let mut info: NOTIFYICONDATAW = mem::zeroed();
    info.cbSize = mem::size_of::<NOTIFYICONDATAW>() as u32;
    info.hWnd = hwnd;
    info.uID = 1;
    info.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
    info.uCallbackMessage = TRAY_MESSAGE;
    info.hIcon = LoadIconW(0, IDI_APPLICATION);
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
        let show = settings.enabled(Key::TrayStart)
            || app.advanced_window.popup_hidden
            || settings.number(Key::TrayVisibility) == 0
            || settings.number(Key::TrayVisibility) == 2 && (hidden || minimized)
            || settings.enabled(Key::CloseMinimizes) && minimized;
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
}

pub unsafe fn remove_tray(hwnd: HWND) {
    Shell_NotifyIconW(NIM_DELETE, &tray_data(hwnd));
}

pub unsafe fn tray_click(hwnd: HWND, event: u32) {
    if event != WM_LBUTTONUP && event != WM_RBUTTONUP {
        return;
    }
    let mut pointer = POINT { x: 0, y: 0 };
    GetCursorPos(&mut pointer);
    let menu = APP_STATE.with(|cell| {
        let mut guard = cell.try_borrow_mut().ok()?;
        let app = guard.as_mut()?;
        let settings = &app.user_cfg.advanced;
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
        if settings.enabled(Key::TrayClickShowsList) {
            app.user_cfg.enable_list = true;
        }
        app.advanced_window.popup_hidden = false;
        app.advanced_window.dock = None;
        None
    });
    if menu.is_some() {
        SetForegroundWindow(hwnd);
        popup::sync(hwnd);
    } else {
        ShowWindow(hwnd, SW_RESTORE);
        SetForegroundWindow(hwnd);
        InvalidateRect(hwnd, ptr::null(), 0);
    }
    update_tray(hwnd);
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
        ShowWindow(hwnd, SW_MINIMIZE);
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

pub unsafe fn stretch_docked(hwnd: HWND) {
    let mut rect: RECT = mem::zeroed();
    if GetWindowRect(hwnd, &mut rect) == 0 {
        return;
    }
    let Some(work) = working_area(&rect) else {
        return;
    };
    let docked = (rect.left - work.left).abs() <= 3 || (rect.right - work.right).abs() <= 3;
    let position = APP_STATE.with(|cell| {
        let mut guard = cell.try_borrow_mut().ok()?;
        let app = guard.as_mut()?;
        if app.user_cfg.advanced.enabled(Key::StretchDocked) && docked {
            app.advanced_window
                .stretched
                .get_or_insert((rect.top, rect.bottom - rect.top));
            Some((work.top, work.bottom - work.top))
        } else {
            let (top, height) = app.advanced_window.stretched.take()?;
            Some((
                top.clamp(work.top, (work.bottom - height).max(work.top)),
                height,
            ))
        }
    });
    if let Some((top, height)) = position {
        SetWindowPos(
            hwnd,
            0,
            rect.left,
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
            if visible && inside && tooltip_kind < 2 {
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
                        let label = if tooltip_kind == 1 {
                            format!("{}  {}", item.name, item.target_path)
                        } else {
                            item.name.clone()
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
            let opening = !visible || minimized;
            show = Some(opening);
            center = opening && settings.enabled(Key::CenterPointer);
            state.popup_hidden = !opening;
            state.dock = None;
        }
        state.button_pressed = pressed_now;

        let left = (rect.left - work.left).abs() <= 3;
        let right = (rect.right - work.right).abs() <= 3;
        let top = (rect.top - work.top).abs() <= 3;
        let bottom = (rect.bottom - work.bottom).abs() <= 3;
        let edge_hit = ((left && cursor.x <= work.left + 2)
            || (right && cursor.x >= work.right - 3))
            && cursor.y >= rect.top
            && cursor.y < rect.bottom
            || ((top && cursor.y <= work.top + 2) || (bottom && cursor.y >= work.bottom - 3))
                && cursor.x >= rect.left
                && cursor.x < rect.right;
        if show.is_none()
            && !state.resolution_hidden
            && settings.enabled(Key::AutoHide)
            && (left || right || top || bottom)
            && !state.popup_hidden
            && !minimized
        {
            if visible && !inside {
                let since = state.dock.get_or_insert((rect, now)).1;
                if now.duration_since(since) >= delay(settings, Key::HideDelay) {
                    show = Some(false);
                    state.dock = Some((rect, now));
                }
            } else if !visible && state.dock.is_some() {
                if edge_hit {
                    let since = state.dock.as_ref().map_or(now, |entry| entry.1);
                    if now.duration_since(since) >= delay(settings, Key::ShowDelay) {
                        show = Some(true);
                        state.dock = None;
                    }
                } else {
                    state.dock = Some((rect, now));
                }
            } else {
                state.dock = None;
            }
        } else if !settings.enabled(Key::AutoHide)
            && state.dock.take().is_some()
            && !visible
            && !state.resolution_hidden
        {
            show = Some(true);
        }

        let mut changed = indicator_expired || previous_tip != state.tooltip;
        if visible && inside && settings.enabled(Key::HoverSwitch) {
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
        ))
    });
    if let Some((show, center, focus, changed, animate)) = action {
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
    update_tray(hwnd);
}
