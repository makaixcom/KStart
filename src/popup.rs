//! 独立皮肤菜单窗口的定位、绘制和输入处理。

use crate::model::{MenuIconKind, PopupMenuItem};
use crate::renderer::{
    hit_test_popup, popup_item_count, popup_rect, popup_visible_rows, render_popup_menu,
    ActivePopupMenu, HitTarget, PopupMenuFrame, SEARCH_MENU_HEIGHT, SEARCH_MENU_WIDTH,
};
use crate::skin_assets::Canvas;
use crate::skin_parser::ToolButtonKind;
use crate::{apply_cursor_for_hit, blit_canvas_with_gdi_texts, to_wide, APP_STATE};
use std::{cell::Cell, mem, ptr};
use windows_sys::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    BeginPaint, ClientToScreen, EndPaint, GetMonitorInfoW, InvalidateRect, MonitorFromRect,
    MONITORINFO, MONITOR_DEFAULTTONEAREST, PAINTSTRUCT, RedrawWindow,
    RDW_INVALIDATE, RDW_UPDATENOW, RDW_NOERASE,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{SetCapture, VK_ESCAPE};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, LoadCursorW,
    RegisterClassW, SendMessageW, SetForegroundWindow, ShowWindow, CS_DROPSHADOW, IDC_ARROW,
    MA_NOACTIVATE, SW_SHOW, SW_SHOWNOACTIVATE, WM_APP, WM_CANCELMODE, WM_CAPTURECHANGED, WM_CLOSE,
    WM_ERASEBKGND, WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MOUSEACTIVATE,
    WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_PAINT, WM_RBUTTONDOWN, WM_SETCURSOR, WNDCLASSW,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};
use windows_sys::Win32::UI::{
    Input::KeyboardAndMouse::{GetCapture, VK_DOWN, VK_RETURN, VK_UP},
    WindowsAndMessaging::{
        DispatchMessageW, GetMessageW, GetWindowLongPtrW, GetWindowRect, IsWindow, PostQuitMessage,
        SetWindowLongPtrW, TranslateMessage, CREATESTRUCTW, GWLP_USERDATA, MSG, WM_NCCREATE,
        WM_NCDESTROY, WM_SYSKEYDOWN,
    },
};

const CLASS_NAME: &str = "KStartPopupMenuClass";
pub const WM_SELECT: u32 = WM_APP + 1;

struct ChoiceMenu<'a> {
    rows: &'a [PopupMenuItem<()>],
    size: (i32, i32),
    hover: Cell<Option<usize>>,
    first: Cell<usize>,
    result: Cell<Option<usize>>,
}

impl ChoiceMenu<'_> {
    fn visible(&self) -> usize {
        ((self.size.1 - 2) / 24) as usize
    }

    fn hit(&self, param: LPARAM) -> Option<usize> {
        let (x, y) = mouse_point(param);
        if x < 1 || x >= self.size.0 - 1 || !(1..self.size.1 - 1).contains(&y) {
            return None;
        }
        Some(self.first.get() + ((y - 1) / 24) as usize).filter(|row| *row < self.rows.len())
    }

    fn select(&self, index: usize) {
        self.hover.set(Some(index));
        if index < self.first.get() {
            self.first.set(index);
        } else if index >= self.first.get() + self.visible() {
            self.first.set(index + 1 - self.visible());
        }
    }
}

/// 对话框的下拉命令共用皮肤菜单样式和独立顶层窗口。
pub unsafe fn choose(
    owner: HWND,
    trigger: HWND,
    labels: &[(&str, bool)],
    width: i32,
) -> Option<usize> {
    if labels.is_empty() {
        return None;
    }
    let mut anchor: RECT = mem::zeroed();
    if GetWindowRect(trigger, &mut anchor) == 0 {
        return None;
    }
    let monitor = MonitorFromRect(&anchor, MONITOR_DEFAULTTONEAREST);
    let mut monitor_info: MONITORINFO = mem::zeroed();
    monitor_info.cbSize = mem::size_of::<MONITORINFO>() as u32;
    if GetMonitorInfoW(monitor, &mut monitor_info) == 0 {
        return None;
    }
    let work = monitor_info.rcWork;
    let rows = labels
        .iter()
        .map(|(label, separator)| PopupMenuItem {
            action: (),
            label: (*label).into(),
            icon: MenuIconKind::None,
            custom_text_color: None,
            has_separator_below: *separator,
        })
        .collect::<Vec<_>>();
    let visible = labels
        .len()
        .min(((work.bottom - work.top - 2) / 24).max(1) as usize);
    let size = (width.min(work.right - work.left), 2 + 24 * visible as i32);
    let (x, y) = position(ActivePopupMenu::SearchMode, anchor, work, size);
    let menu = ChoiceMenu {
        rows: &rows,
        size,
        hover: Cell::new(None),
        first: Cell::new(0),
        result: Cell::new(None),
    };
    let class = to_wide(CLASS_NAME);
    let hwnd = CreateWindowExW(
        WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
        class.as_ptr(),
        ptr::null(),
        WS_POPUP,
        x,
        y,
        size.0,
        size.1,
        owner,
        0,
        GetModuleHandleW(ptr::null()),
        (&menu as *const ChoiceMenu).cast(),
    );
    if hwnd == 0 {
        return None;
    }
    ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    SetCapture(hwnd);
    let mut message: MSG = mem::zeroed();
    while IsWindow(hwnd) != 0 && GetCapture() == hwnd {
        let status = GetMessageW(&mut message, 0, 0, 0);
        if status <= 0 {
            if status == 0 {
                PostQuitMessage(message.wParam as i32);
            }
            break;
        }
        if matches!(message.message, WM_KEYDOWN | WM_SYSKEYDOWN) {
            let count = labels.len();
            match message.wParam as u16 {
                VK_ESCAPE => break,
                VK_RETURN => {
                    menu.result.set(menu.hover.get());
                    break;
                }
                VK_DOWN => menu.select(menu.hover.get().map_or(0, |row| (row + 1) % count)),
                VK_UP => menu.select(
                    menu.hover
                        .get()
                        .map_or(count - 1, |row| (row + count - 1) % count),
                ),
                _ => {}
            }
            InvalidateRect(hwnd, ptr::null(), 0);
        } else {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
    if IsWindow(hwnd) != 0 {
        DestroyWindow(hwnd);
    }
    menu.result.get()
}

unsafe fn choice_message(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    menu: &ChoiceMenu,
) -> LRESULT {
    match message {
        WM_MOUSEACTIVATE => MA_NOACTIVATE as LRESULT,
        WM_ERASEBKGND => 1,
        WM_CANCELMODE | WM_CLOSE | WM_RBUTTONDOWN | WM_MBUTTONDOWN => {
            DestroyWindow(hwnd);
            0
        }
        WM_LBUTTONDOWN if menu.hit(lparam).is_none() => {
            DestroyWindow(hwnd);
            0
        }
        WM_LBUTTONUP => {
            menu.result.set(menu.hit(lparam));
            DestroyWindow(hwnd);
            0
        }
        WM_MOUSEMOVE => {
            menu.hover.set(menu.hit(lparam));
            InvalidateRect(hwnd, ptr::null(), 0);
            0
        }
        WM_SETCURSOR => {
            let cursor = if menu.hover.get().is_some() {
                windows_sys::Win32::UI::WindowsAndMessaging::IDC_HAND
            } else {
                IDC_ARROW
            };
            windows_sys::Win32::UI::WindowsAndMessaging::SetCursor(LoadCursorW(0, cursor));
            1
        }
        WM_MOUSEWHEEL => {
            let current = menu.first.get();
            let last = menu.rows.len().saturating_sub(menu.visible());
            let next = if ((wparam >> 16) as i16) < 0 {
                (current + 1).min(last)
            } else {
                current.saturating_sub(1)
            };
            menu.first.set(next);
            menu.hover.set(None);
            InvalidateRect(hwnd, ptr::null(), 0);
            0
        }
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = mem::zeroed();
            let dc = BeginPaint(hwnd, &mut ps);
            APP_STATE.with(|cell| {
                let Ok(guard) = cell.try_borrow() else {
                    return;
                };
                let Some(app) = guard.as_ref() else {
                    return;
                };
                let (width, height) = menu.size;
                let mut canvas = Canvas::new(width, height);
                let mut texts = Vec::new();
                crate::renderer::render_generic_popup_menu(
                    &app.skin_cfg,
                    &app.skin_assets,
                    &app.builtin_icons,
                    &mut canvas,
                    &mut texts,
                    0,
                    0,
                    width,
                    height,
                    menu.first.get(),
                    menu.rows,
                    |index| menu.hover.get() == Some(index),
                );
                blit_canvas_with_gdi_texts(&canvas, &texts, None, dc, false, None);
            });
            EndPaint(hwnd, &ps);
            0
        }
        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}

#[derive(Clone, Copy)]
pub struct PopupWindow {
    hwnd: HWND,
    menu: ActivePopupMenu,
    size: (i32, i32),
    scroll_drag_offset: Option<i32>,
    from_tray: bool,
}

impl PopupWindow {
    fn scroll_geometry(self, skins_count: usize) -> Option<(i32, i32, usize)> {
        let (visible, count) = self.scroll_bounds(skins_count);
        let max_scroll = count.saturating_sub(visible);
        if max_scroll == 0 { return None; }
        let track = (visible as i32 * 24 - 28).max(18);
        let minimum = if self.menu == ActivePopupMenu::SkinQuickMenu { 24 } else { 18 };
        let thumb = ((track as usize * visible) / count.max(1))
            .clamp(minimum.min(track as usize), track as usize) as i32;
        Some((track - thumb, thumb, max_scroll))
    }
    pub fn hit(self, position: (i32, i32), scroll: usize, skins_count: usize) -> HitTarget {
        hit_test_popup(self.menu, position, self.size, scroll, skins_count)
    }

    pub fn scroll_bounds(self, skins_count: usize) -> (usize, usize) {
        if self.menu == ActivePopupMenu::SkinQuickMenu {
            return (popup_visible_rows(self.size.1).saturating_sub(2), skins_count);
        }
        (
            popup_visible_rows(self.size.1),
            popup_item_count(self.menu, skins_count),
        )
    }
}

pub unsafe fn register(instance: HINSTANCE) {
    let class = to_wide(CLASS_NAME);
    RegisterClassW(&WNDCLASSW {
        style: CS_DROPSHADOW,
        lpfnWndProc: Some(wnd_proc),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: instance,
        hIcon: 0,
        hCursor: LoadCursorW(0, IDC_ARROW),
        hbrBackground: 0,
        lpszMenuName: ptr::null(),
        lpszClassName: class.as_ptr(),
    });
}

/// 锚点、工作区和返回位置均使用屏幕坐标。
fn position(menu: ActivePopupMenu, anchor: RECT, work: RECT, (w, h): (i32, i32)) -> (i32, i32) {
    let x = match menu {
        ActivePopupMenu::SearchMode if anchor.right + w > work.right => anchor.left - w,
        ActivePopupMenu::SearchMode => anchor.right,
        ActivePopupMenu::SkinQuickMenu => anchor.right - w,
        ActivePopupMenu::ContextMenu { .. }
        | ActivePopupMenu::ItemMenu { .. }
        | ActivePopupMenu::TrayStart { .. }
            if anchor.left + w > work.right =>
        {
            anchor.left - w
        }
        _ => anchor.left,
    };
    let y = if menu == ActivePopupMenu::MainMenu && anchor.top - h >= work.top {
        anchor.top - h
    } else if anchor.bottom + h <= work.bottom {
        anchor.bottom
    } else {
        anchor.top - h
    };
    (
        x.clamp(work.left, (work.right - w).max(work.left)),
        y.clamp(work.top, (work.bottom - h).max(work.top)),
    )
}

pub unsafe fn sync(owner: HWND) {
    sync_with_anchor(owner, None);
}

/// 托盘主菜单以屏幕坐标定位，不依赖主窗口是否可见。
pub unsafe fn sync_from_tray(owner: HWND, point: POINT) {
    sync_with_anchor(owner, Some(point));
}

fn skin_menu_initial_scroll(active: usize, count: usize, visible: usize) -> usize {
    active.min(count.saturating_sub(1)).saturating_sub(visible / 2)
        .min(count.saturating_sub(visible))
}

unsafe fn sync_with_anchor(owner: HWND, screen_anchor: Option<POINT>) {
    let Some((menu, previous)) = APP_STATE.with(|cell| {
        let mut guard = cell.try_borrow_mut().ok()?;
        let app = guard.as_mut()?;
        if let Some(window) = app.popup_window {
            if window.menu == app.popup_menu && screen_anchor.is_none() {
                InvalidateRect(window.hwnd, ptr::null(), 0);
                return None;
            }
        }
        Some((app.popup_menu, app.popup_window.take()))
    }) else {
        return;
    };

    if let Some(window) = previous {
        DestroyWindow(window.hwnd);
    }
    if menu == ActivePopupMenu::None {
        return;
    }

    let Some(((x, y, w, h), skins_count, win_w)) = APP_STATE.with(|cell| {
        let guard = cell.try_borrow().ok()?;
        let app = guard.as_ref()?;
        let cfg = &app.skin_cfg;
        let (width, height) = (app.user_cfg.window_width, app.user_cfg.window_height);
        let rect = if let Some(point) = screen_anchor {
            (point.x, point.y, 0, 0)
        } else { match menu {
            ActivePopupMenu::SearchMode => cfg.search_btn_layout.resolve_rect(width, height),
            ActivePopupMenu::SkinQuickMenu => cfg.title_btn_skin.resolve_rect(width, height),
            ActivePopupMenu::MainMenu => {
                let button = cfg
                    .tool_buttons
                    .iter()
                    .find(|button| button.kind == ToolButtonKind::MainMenu)?;
                let image = &app.skin_assets.tool_buttons.get(&button.kind)?.normal;
                (
                    button.left.resolve(width, height),
                    button.top.resolve(width, height),
                    image.width as i32,
                    image.height as i32,
                )
            }
            ActivePopupMenu::ContextMenu { x, y }
            | ActivePopupMenu::ItemMenu { x, y, .. }
            | ActivePopupMenu::TrayStart { x, y, .. } => (x, y, 0, 0),
            ActivePopupMenu::None => return None,
        }};
        Some((rect, app.skins.len(), width))
    }) else {
        dismiss(owner);
        return;
    };
    let mut origin = POINT { x, y };
    if screen_anchor.is_none() && !matches!(menu, ActivePopupMenu::TrayStart { .. }) && ClientToScreen(owner, &mut origin) == 0
    {
        dismiss(owner);
        return;
    }
    let anchor = RECT {
        left: origin.x,
        top: origin.y,
        right: origin.x + w.max(1),
        bottom: origin.y + h.max(1),
    };
    let monitor = MonitorFromRect(&anchor, MONITOR_DEFAULTTONEAREST);
    let mut info: MONITORINFO = mem::zeroed();
    info.cbSize = mem::size_of::<MONITORINFO>() as u32;
    if GetMonitorInfoW(monitor, &mut info) == 0 {
        dismiss(owner);
        return;
    }
    let work = info.rcWork;
    let size = if menu == ActivePopupMenu::SearchMode {
        (SEARCH_MENU_WIDTH, SEARCH_MENU_HEIGHT)
    } else {
        let Some((_, _, width, height)) =
            popup_rect(menu, win_w, work.bottom - work.top, skins_count)
        else {
            dismiss(owner);
            return;
        };
        (width, height)
    };
    let (left, top) = position(menu, anchor, work, size);
    let class = to_wide(CLASS_NAME);
    let from_tray = screen_anchor.is_some() || matches!(menu, ActivePopupMenu::TrayStart { .. });
    let tray_only = from_tray;
    let hwnd = CreateWindowExW(
        WS_EX_TOOLWINDOW
            | if tray_only {
                WS_EX_TOPMOST
            } else {
                WS_EX_NOACTIVATE
            },
        class.as_ptr(),
        ptr::null(),
        WS_POPUP,
        left,
        top,
        size.0,
        size.1,
        if tray_only { 0 } else { owner },
        0,
        GetModuleHandleW(ptr::null()),
        ptr::null(),
    );
    if hwnd == 0 {
        dismiss(owner);
        return;
    }
    let registered = APP_STATE.with(|cell| {
        let Ok(mut guard) = cell.try_borrow_mut() else {
            return false;
        };
        let Some(app) = guard.as_mut() else {
            return false;
        };
        if app.popup_menu != menu || app.popup_window.is_some() {
            return false;
        }
        if menu == ActivePopupMenu::SkinQuickMenu {
            let visible = popup_visible_rows(size.1).saturating_sub(2);
            app.menu_scroll = skin_menu_initial_scroll(app.active_skin_index, app.skins.len(), visible);
        }
        app.popup_window = Some(PopupWindow { hwnd, menu, size, scroll_drag_offset: None, from_tray });
        true
    });
    if registered {
        if tray_only {
            ShowWindow(hwnd, SW_SHOW);
            SetForegroundWindow(hwnd);
        } else {
            ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        }
        SetCapture(hwnd);
    } else {
        DestroyWindow(hwnd);
    }
}

pub unsafe fn dismiss(owner: HWND) {
    APP_STATE.with(|cell| {
        if let Ok(mut guard) = cell.try_borrow_mut() {
            if let Some(app) = guard.as_mut() {
                if app.popup_menu != ActivePopupMenu::None || app.popup_window.is_some() {
                    app.popup_menu = ActivePopupMenu::None;
                    app.hover_target = HitTarget::None;
                }
            }
        }
    });
    sync(owner);
}

pub unsafe fn dismiss_visible(owner: HWND) {
    let visible = APP_STATE.with(|cell| {
        cell.try_borrow()
            .ok()
            .is_some_and(|guard| guard.as_ref().is_some_and(|app| app.popup_window.is_some()))
    });
    if visible {
        dismiss(owner);
    }
}

pub fn is_popup_window(hwnd: HWND) -> bool {
    APP_STATE.with(|cell| cell.try_borrow().ok().is_some_and(|guard|
        guard.as_ref().is_some_and(|app| app.popup_window.is_some_and(|window| window.hwnd == hwnd))))
}

pub unsafe fn dismiss_with_owner(owner: HWND) {
    let follows_owner = APP_STATE.with(|cell| cell.try_borrow().ok().is_some_and(|guard|
        guard.as_ref().is_some_and(|app| app.popup_window.is_some_and(|window| !window.from_tray))));
    if follows_owner { dismiss(owner); }
}

/// 菜单不抢键盘焦点，滚轮消息可能由主窗口或菜单窗口收到。
pub unsafe fn scroll(delta: i16) {
    let hwnd = APP_STATE.with(|cell| {
        let mut guard = cell.try_borrow_mut().ok()?;
        let app = guard.as_mut()?;
        let window = app.popup_window?;
        let (visible, count) = window.scroll_bounds(app.skins.len());
        let next = if delta > 0 {
            app.menu_scroll.saturating_sub(3)
        } else if delta < 0 {
            (app.menu_scroll + 3).min(count.saturating_sub(visible))
        } else {
            app.menu_scroll
        };
        if next == app.menu_scroll {
            return None;
        }
        app.menu_scroll = next;
        Some(window.hwnd)
    });
    if let Some(hwnd) = hwnd {
        InvalidateRect(hwnd, ptr::null(), 0);
    }
}

fn mouse_point(lparam: LPARAM) -> (i32, i32) {
    (lparam as i16 as i32, (lparam >> 16) as i16 as i32)
}

fn scroll_for_drag(y: i32, grab: i32, travel: i32, max_scroll: usize) -> usize {
    if travel <= 0 { return 0; }
    let offset = (y - 15 - grab).clamp(0, travel) as usize;
    (offset * max_scroll + travel as usize / 2) / travel as usize
}

/// 捕获期间持续更新滚动位置，释放状态借用后再同步绘制，避免消息重入。
unsafe fn drag_scroll(hwnd: HWND, point: (i32, i32), begin: bool, end: bool) -> bool {
    let handled = APP_STATE.with(|cell| {
        let mut guard = cell.try_borrow_mut().ok()?;
        let app = guard.as_mut()?;
        let window = app.popup_window.filter(|window| window.hwnd == hwnd)?;
        let (travel, thumb, max_scroll) = window.scroll_geometry(app.skins.len())?;
        let grab = if begin {
            if !matches!(window.hit(point, app.menu_scroll, app.skins.len()), HitTarget::MenuScrollTrack(_)) {
                return None;
            }
            let thumb_y = 15 + (travel as usize * app.menu_scroll.min(max_scroll) / max_scroll) as i32;
            if (thumb_y..thumb_y + thumb).contains(&point.1) { point.1 - thumb_y } else { thumb / 2 }
        } else { window.scroll_drag_offset? };
        app.menu_scroll = scroll_for_drag(point.1, grab, travel, max_scroll);
        app.hover_target = HitTarget::MenuScrollTrack(point.1 - 15);
        app.popup_window.as_mut()?.scroll_drag_offset = if end { None } else { Some(grab) };
        Some(())
    }).is_some();
    if handled {
        RedrawWindow(hwnd, ptr::null(), 0, RDW_INVALIDATE | RDW_UPDATENOW | RDW_NOERASE);
    }
    handled
}

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_NCCREATE {
        let create = &*(lparam as *const CREATESTRUCTW);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
    }
    let choice = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const ChoiceMenu;
    if let Some(menu) = choice.as_ref() {
        if message == WM_NCDESTROY {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
        }
        return choice_message(hwnd, message, wparam, lparam, menu);
    }
    let snapshot = || {
        APP_STATE.with(|cell| {
            let guard = cell.try_borrow().ok()?;
            let app = guard.as_ref()?;
            let window = app.popup_window.filter(|window| window.hwnd == hwnd)?;
            Some((window, app.main_hwnd, app.menu_scroll, app.skins.len()))
        })
    };
    match message {
        windows_sys::Win32::UI::WindowsAndMessaging::WM_ACTIVATE if wparam & 0xffff == 0 => {
            if let Some((window, owner, _, _)) = snapshot() {
                if window.from_tray { dismiss(owner); }
            }
            0
        }
        WM_MOUSEACTIVATE => MA_NOACTIVATE as LRESULT,
        WM_ERASEBKGND => 1,
        WM_KEYDOWN if wparam as u16 == VK_ESCAPE => {
            if let Some((_, owner, _, _)) = snapshot() {
                dismiss(owner);
            }
            0
        }
        WM_CAPTURECHANGED | WM_CANCELMODE | WM_CLOSE | WM_RBUTTONDOWN | WM_MBUTTONDOWN => {
            if let Some((_, owner, _, _)) = snapshot() {
                dismiss(owner);
            }
            0
        }
        WM_LBUTTONDOWN => {
            if drag_scroll(hwnd, mouse_point(lparam), true, false) { return 0; }
            if let Some((window, owner, _, _)) = snapshot() {
                let (x, y) = mouse_point(lparam);
                if x < 0 || y < 0 || x >= window.size.0 || y >= window.size.1 {
                    dismiss(owner);
                }
            }
            0
        }
        WM_LBUTTONUP => {
            if drag_scroll(hwnd, mouse_point(lparam), false, true) { return 0; }
            if let Some((window, owner, scroll, skins)) = snapshot() {
                if window.hit(mouse_point(lparam), scroll, skins) != HitTarget::None {
                    SendMessageW(owner, WM_SELECT, 0, lparam);
                }
            }
            0
        }
        WM_MOUSEWHEEL => {
            scroll((wparam >> 16) as i16);
            0
        }
        WM_MOUSEMOVE => {
            if drag_scroll(hwnd, mouse_point(lparam), false, false) { return 0; }
            if let Some((window, _, scroll, skins)) = snapshot() {
                APP_STATE.with(|cell| {
                    if let Ok(mut guard) = cell.try_borrow_mut() {
                        if let Some(app) = guard.as_mut() {
                            app.hover_target = window.hit(mouse_point(lparam), scroll, skins);
                        }
                    }
                });
                InvalidateRect(hwnd, ptr::null(), 0);
            }
            0
        }
        WM_SETCURSOR => {
            let target = APP_STATE.with(|cell| {
                cell.try_borrow()
                    .ok()
                    .and_then(|guard| guard.as_ref().map(|app| app.hover_target))
            });
            apply_cursor_for_hit(target.unwrap_or(HitTarget::None));
            1
        }
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = mem::zeroed();
            let dc = BeginPaint(hwnd, &mut ps);
            APP_STATE.with(|cell| {
                let Ok(guard) = cell.try_borrow() else {
                    return;
                };
                let Some(app) = guard.as_ref() else {
                    return;
                };
                let Some(window) = app.popup_window.filter(|window| window.hwnd == hwnd) else {
                    return;
                };
                let (w, h) = window.size;
                let mut canvas = Canvas::new(w, h);
                let mut texts = Vec::new();
                let frame = PopupMenuFrame {
                    menu: window.menu,
                    skin_cfg: &app.skin_cfg,
                    skin_assets: &app.skin_assets,
                    builtin: &app.builtin_icons,
                    user_cfg: &app.user_cfg,
                    skins: &app.skins,
                    categories: &app.categories,
                    active_skin_index: app.active_skin_index,
                    scroll: app.menu_scroll,
                    hover: app.hover_target,
                };
                render_popup_menu(&frame, &mut canvas, &mut texts, (0, 0, w, h));
                blit_canvas_with_gdi_texts(&canvas, &texts, None, dc, false, None);
            });
            EndPaint(hwnd, &ps);
            0
        }
        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dragging_updates_scroll_without_releasing_and_clamps_at_edges() {
        let window = PopupWindow { hwnd: 0, menu: ActivePopupMenu::SkinQuickMenu,
            size: (207, 410), scroll_drag_offset: None, from_tray: false };
        let (travel, thumb, max_scroll) = window.scroll_geometry(50).unwrap();
        assert_eq!(max_scroll, 35);
        let grab = thumb / 2;
        let top = scroll_for_drag(15 + grab, grab, travel, max_scroll);
        let middle = scroll_for_drag(15 + grab + travel / 2, grab, travel, max_scroll);
        let bottom = scroll_for_drag(15 + grab + travel, grab, travel, max_scroll);
        assert_eq!(top, 0);
        assert!(middle > top && middle < bottom);
        assert_eq!(bottom, max_scroll);
        assert_eq!(scroll_for_drag(-500, grab, travel, max_scroll), 0);
        assert_eq!(scroll_for_drag(2000, grab, travel, max_scroll), max_scroll);
    }

    #[test]
    fn skin_menu_opens_with_active_skin_visible() {
        for count in [0, 5, 15, 16, 50] {
            let visible = count.min(15);
            for active in 0..count.max(1) {
                let first = skin_menu_initial_scroll(active, count, visible);
                assert!(first <= count.saturating_sub(visible));
                if count > 0 { assert!((first..first + visible).contains(&active)); }
            }
        }
        assert_eq!(skin_menu_initial_scroll(25, 50, 15), 18);
    }

    #[test]
    fn tray_main_menu_stays_above_taskbar_and_inside_work_area() {
        let work = RECT { left: 0, top: 0, right: 1920, bottom: 1040 };
        let anchor = RECT { left: 1880, top: 1060, right: 1881, bottom: 1061 };
        let (x, y) = position(ActivePopupMenu::MainMenu, anchor, work, (180, 400));
        assert!(x >= work.left && x + 180 <= work.right);
        assert!(y >= work.top && y + 400 <= work.bottom);
    }

    #[test]
    fn search_menu_prefers_right_and_flips_at_screen_edge() {
        let work = RECT {
            left: -800,
            top: 30,
            right: 0,
            bottom: 600,
        };
        let anchor = RECT {
            left: -300,
            top: 50,
            right: -284,
            bottom: 66,
        };
        let size = (SEARCH_MENU_WIDTH, SEARCH_MENU_HEIGHT);
        assert_eq!(
            position(ActivePopupMenu::SearchMode, anchor, work, size),
            (-284, 66)
        );
        let edge = RECT {
            left: -44,
            right: -28,
            ..anchor
        };
        assert_eq!(
            position(ActivePopupMenu::SearchMode, edge, work, size),
            (-160, 66)
        );
    }
}
