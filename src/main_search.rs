//! 主界面原生搜索输入框：编辑、选择和输入法交由 Windows EDIT 处理。
use crate::{
    native_dialogs::get_ctrl_text,
    renderer::{ActivePopupMenu, HitTarget},
    skin_assets::Canvas,
    skin_parser::RgbColor,
    to_wide, APP_STATE,
};
use std::{
    cell::{Cell, RefCell},
    mem, ptr,
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    System::LibraryLoader::GetModuleHandleW,
    UI::{
        Controls::{EM_SETMARGINS, EM_SETSEL},
        Input::KeyboardAndMouse::*,
        Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
        WindowsAndMessaging::*,
    },
};
pub const ID: i32 = 1600;
pub const WM_SUBMIT: u32 = WM_APP + 35;
pub const WM_SYNC: u32 = WM_APP + 36;
thread_local! {static SYNCING:Cell<bool>=const{Cell::new(false)};}
struct SyncGuard;
impl Drop for SyncGuard {
    fn drop(&mut self) {
        SYNCING.set(false);
    }
}
struct State {
    rect: Cell<(i32, i32, i32, i32)>,
    font: HFONT,
    owned_font: bool,
    ink: Cell<u32>,
    paper: Cell<u32>,
    solid: Cell<HBRUSH>,
    pattern: Cell<HBRUSH>,
    pixels: RefCell<Vec<u32>>,
}
impl Drop for State {
    fn drop(&mut self) {
        unsafe {
            if self.owned_font {
                DeleteObject(self.font);
            }
            DeleteObject(self.solid.get());
            if self.pattern.get() != 0 {
                DeleteObject(self.pattern.get());
            }
        }
    }
}
fn rgb(color: RgbColor) -> u32 {
    u32::from(color.r) | (u32::from(color.g) << 8) | (u32::from(color.b) << 16)
}
unsafe fn state(edit: HWND) -> Option<&'static State> {
    (GetWindowLongPtrW(edit, GWLP_USERDATA) as *const State).as_ref()
}
pub unsafe fn control(parent: HWND) -> HWND {
    GetDlgItem(parent, ID)
}
pub unsafe fn sync(parent: HWND) {
    if parent == 0 || SYNCING.replace(true) {
        return;
    }
    let _guard = SyncGuard;
    let settings = APP_STATE.with(|cell| {
        let guard = cell.try_borrow().ok()?;
        let app = guard.as_ref()?;
        let cfg = &app.skin_cfg;
        let (x, y, w, h) = cfg
            .search_box_layout
            .resolve_rect(app.user_cfg.window_width, app.user_cfg.window_height);
        Some((
            cfg.search_box_visible,
            (
                x + 2,
                y - 1,
                (w - 4).clamp(1, app.user_cfg.window_width),
                (h + 2).clamp(1, app.user_cfg.window_height),
            ),
            rgb(cfg.search_box_text_color),
            rgb(cfg.search_box_bg_color),
            app.search_query.clone(),
            app.search_focused,
        ))
    });
    let Some((visible, rect, ink, paper, query, want_focus)) = settings else {
        return;
    };
    let mut edit = control(parent);
    if edit == 0 {
        if !visible {
            return;
        }
        edit = CreateWindowExW(
            0,
            to_wide("EDIT").as_ptr(),
            to_wide(&query).as_ptr(),
            WS_CHILD | WS_TABSTOP | ES_AUTOHSCROLL as u32,
            rect.0,
            rect.1,
            rect.2,
            rect.3,
            parent,
            ID as isize,
            GetModuleHandleW(ptr::null()),
            ptr::null(),
        );
        if edit == 0 {
            eprintln!("创建主界面搜索框失败：{}", std::io::Error::last_os_error());
            return;
        }
        let font = CreateFontW(
            -12,
            0,
            0,
            0,
            FW_NORMAL as i32,
            0,
            0,
            0,
            GB2312_CHARSET as u32,
            0,
            0,
            DEFAULT_QUALITY as u32,
            0,
            to_wide("宋体").as_ptr(),
        );
        let data = Box::new(State {
            rect: Cell::new(rect),
            font: if font != 0 {
                font
            } else {
                GetStockObject(DEFAULT_GUI_FONT) as HFONT
            },
            owned_font: font != 0,
            ink: Cell::new(ink),
            paper: Cell::new(paper),
            solid: Cell::new(CreateSolidBrush(paper)),
            pattern: Cell::new(0),
            pixels: RefCell::new(Vec::new()),
        });
        SendMessageW(edit, WM_SETFONT, data.font as usize, 0);
        SendMessageW(
            edit,
            EM_SETMARGINS,
            (EC_LEFTMARGIN | EC_RIGHTMARGIN) as usize,
            0,
        );
        SetWindowLongPtrW(edit, GWLP_USERDATA, Box::into_raw(data) as isize);
        if SetWindowSubclass(edit, Some(edit_proc), 1, 0) == 0 {
            let pointer = GetWindowLongPtrW(edit, GWLP_USERDATA) as *mut State;
            SetWindowLongPtrW(edit, GWLP_USERDATA, 0);
            DestroyWindow(edit);
            drop(Box::from_raw(pointer));
            return;
        }
    }
    if let Some(data) = state(edit) {
        if data.ink.replace(ink) != ink {
            InvalidateRect(edit, ptr::null(), 1);
        }
        if data.paper.replace(paper) != paper {
            let old = data.solid.replace(CreateSolidBrush(paper));
            DeleteObject(old);
            data.pixels.borrow_mut().clear();
        }
        if data.rect.replace(rect) != rect {
            data.pixels.borrow_mut().clear();
            SetWindowPos(
                edit,
                0,
                rect.0,
                rect.1,
                rect.2,
                rect.3,
                SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOREDRAW,
            );
        }
    }
    // 不重复设置同一段文字，避免重置用户正在操作的插入点、选区和撤销记录。
    if get_ctrl_text(parent, ID) != query {
        SetWindowTextW(edit, to_wide(&query).as_ptr());
    }
    if (GetWindowLongW(edit, GWL_STYLE) as u32 & WS_VISIBLE != 0) != visible {
        ShowWindow(edit, if visible { SW_SHOWNOACTIVATE } else { SW_HIDE });
    }
    if !want_focus && GetFocus() == edit {
        SetFocus(parent);
    }
    if visible && want_focus && GetFocus() == parent && IsWindowEnabled(parent) != 0 {
        SetFocus(edit);
    }
}
pub unsafe fn focus(parent: HWND) {
    sync(parent);
    let edit = control(parent);
    if edit != 0 && IsWindowVisible(edit) != 0 && IsWindowEnabled(parent) != 0 {
        SetFocus(edit);
    }
}
pub unsafe fn command(parent: HWND, notification: u32, edit: HWND) {
    if edit == 0 || edit != control(parent) {
        return;
    }
    let query = if notification == EN_CHANGE {
        Some(get_ctrl_text(parent, ID))
    } else {
        None
    };
    APP_STATE.with(|cell| {
        if let Ok(mut guard) = cell.try_borrow_mut() {
            if let Some(app) = guard.as_mut() {
                if let Some(query) = query {
                    app.search_query = query;
                }
                if notification == EN_SETFOCUS {
                    app.search_focused = true;
                    app.hover_target = HitTarget::SearchBox;
                }
                if notification == EN_KILLFOCUS {
                    app.search_focused = false;
                }
            }
        }
    });
}
pub fn submit(parent: HWND) {
    let request = APP_STATE.with(|cell| {
        let guard = cell.try_borrow().ok()?;
        let app = guard.as_ref()?;
        Some((app.user_cfg.search_mode, app.search_query.clone()))
    });
    if let Some((mode, query)) = request {
        crate::submit_search(parent, mode, &query);
    }
}
pub unsafe fn color(edit: HWND, dc: HDC) -> Option<HBRUSH> {
    if GetDlgCtrlID(edit) != ID {
        return None;
    }
    let data = state(edit)?;
    SetTextColor(dc, data.ink.get());
    SetBkColor(dc, data.paper.get());
    SetBkMode(dc, TRANSPARENT as i32);
    SetBrushOrgEx(dc, 0, 0, ptr::null_mut());
    Some(if data.pattern.get() != 0 {
        data.pattern.get()
    } else {
        data.solid.get()
    })
}
pub struct Background {
    width: i32,
    height: i32,
    pixels: Vec<u32>,
}
/// 取主画布原有贴图作为输入框底图，保留皮肤的底色、边线和纹理。
pub unsafe fn background(parent: HWND, canvas: &Canvas) -> Option<Background> {
    let data = state(control(parent))?;
    let (x, y, width, height) = data.rect.get();
    let mut pixels = Vec::with_capacity((width * height) as usize);
    for py in y..y + height {
        for px in x..x + width {
            let pixel = if px >= 0 && px < canvas.width && py >= 0 && py < canvas.height {
                canvas.buffer[(py * canvas.width + px) as usize]
            } else {
                let paper = data.paper.get();
                0xff000000 | ((paper & 0xff) << 16) | (paper & 0xff00) | ((paper >> 16) & 0xff)
            };
            pixels.push(pixel);
        }
    }
    Some(Background {
        width,
        height,
        pixels,
    })
}
pub unsafe fn update_background(parent: HWND, background: Background) {
    let edit = control(parent);
    let Some(data) = state(edit) else {
        return;
    };
    if *data.pixels.borrow() == background.pixels {
        return;
    }
    let mut info: BITMAPINFO = mem::zeroed();
    info.bmiHeader = BITMAPINFOHEADER {
        biSize: mem::size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: background.width,
        biHeight: -background.height,
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB,
        ..mem::zeroed()
    };
    let mut bits = ptr::null_mut();
    let bitmap = CreateDIBSection(0, &info, DIB_RGB_COLORS, &mut bits, 0, 0);
    if bitmap == 0 || bits.is_null() {
        if bitmap != 0 {
            DeleteObject(bitmap);
        }
        return;
    }
    ptr::copy_nonoverlapping(
        background.pixels.as_ptr(),
        bits as *mut u32,
        background.pixels.len(),
    );
    let brush = CreatePatternBrush(bitmap);
    DeleteObject(bitmap);
    if brush == 0 {
        return;
    }
    let old = data.pattern.replace(brush);
    if old != 0 {
        DeleteObject(old);
    }
    data.pixels.replace(background.pixels);
    InvalidateRect(edit, ptr::null(), 1);
}
unsafe extern "system" fn edit_proc(
    edit: HWND,
    message: u32,
    wp: WPARAM,
    lp: LPARAM,
    id: usize,
    _data: usize,
) -> LRESULT {
    let parent = GetParent(edit);
    if message == WM_NCDESTROY {
        let pointer = GetWindowLongPtrW(edit, GWLP_USERDATA) as *mut State;
        RemoveWindowSubclass(edit, Some(edit_proc), id);
        let result = DefSubclassProc(edit, message, wp, lp);
        SetWindowLongPtrW(edit, GWLP_USERDATA, 0);
        if !pointer.is_null() {
            drop(Box::from_raw(pointer));
        }
        return result;
    }
    if message == WM_LBUTTONDOWN {
        APP_STATE.with(|cell| {
            if let Ok(mut guard) = cell.try_borrow_mut() {
                if let Some(app) = guard.as_mut() {
                    app.popup_menu = ActivePopupMenu::None;
                    app.hover_target = HitTarget::SearchBox;
                }
            }
        });
        crate::popup::sync(parent);
    }
    if message == WM_KEYDOWN {
        let vk = wp as u16;
        let menu = APP_STATE
            .with(|cell| {
                cell.try_borrow().ok().and_then(|guard| {
                    guard
                        .as_ref()
                        .map(|app| app.popup_menu == ActivePopupMenu::SearchMode)
                })
            })
            .unwrap_or(false);
        if menu && matches!(vk, VK_UP | VK_DOWN | VK_RETURN | VK_ESCAPE) {
            PostMessageW(parent, WM_KEYDOWN, wp, lp);
            return 0;
        }
        if vk == b'A' as u16
            && GetKeyState(VK_CONTROL as i32) < 0
            && GetKeyState(VK_MENU as i32) >= 0
        {
            SendMessageW(edit, EM_SETSEL, 0, -1);
            return 0;
        }
        if vk == VK_RETURN {
            if lp & (1 << 30) == 0 {
                PostMessageW(parent, WM_SUBMIT, 0, 0);
            }
            return 0;
        }
        if vk == VK_ESCAPE {
            PostMessageW(parent, WM_KEYDOWN, wp, lp);
            return 0;
        }
        if vk == VK_TAB {
            SetFocus(parent);
            return 0;
        }
    }
    if message == WM_CHAR && matches!(wp as u32, 1 | 9 | 13 | 27) {
        return 0;
    }
    DefSubclassProc(edit, message, wp, lp)
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::UI::Controls::{EM_GETSEL, EM_REPLACESEL};
    thread_local! {static SUBMITS:Cell<usize>=const{Cell::new(0)};}
    unsafe extern "system" fn parent_proc(
        hwnd: HWND,
        message: u32,
        wp: WPARAM,
        lp: LPARAM,
    ) -> LRESULT {
        if message == WM_DESTROY {
            return 0;
        }
        if message == WM_SUBMIT {
            SUBMITS.with(|count| count.set(count.get() + 1));
            return 0;
        }
        crate::main_wnd_proc(hwnd, message, wp, lp)
    }
    unsafe fn selection(edit: HWND) -> (u32, u32) {
        let mut start = 0u32;
        let mut end = 0u32;
        SendMessageW(
            edit,
            EM_GETSEL,
            &mut start as *mut _ as usize,
            &mut end as *mut _ as isize,
        );
        (start, end)
    }
    unsafe fn with_keys(keys: &[u16], action: impl FnOnce()) {
        let mut saved = [0u8; 256];
        assert_ne!(GetKeyboardState(saved.as_mut_ptr()), 0);
        let mut keyboard = saved;
        for vk in [
            VK_CONTROL,
            VK_LCONTROL,
            VK_RCONTROL,
            VK_SHIFT,
            VK_LSHIFT,
            VK_RSHIFT,
            VK_MENU,
            VK_LMENU,
            VK_RMENU,
        ] {
            keyboard[vk as usize] &= 0x7f;
        }
        for &vk in keys {
            keyboard[vk as usize] |= 0x80;
        }
        assert_ne!(SetKeyboardState(keyboard.as_ptr()), 0);
        action();
        assert_ne!(SetKeyboardState(saved.as_ptr()), 0);
    }
    unsafe fn drain() {
        let mut message: MSG = mem::zeroed();
        while PeekMessageW(&mut message, 0, 0, 0, PM_REMOVE) != 0 {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
    unsafe fn capture_edit(edit: HWND, path: &std::path::Path) {
        let mut rect: RECT = mem::zeroed();
        GetClientRect(edit, &mut rect);
        let (width, height) = (rect.right, rect.bottom);
        let dc = CreateCompatibleDC(0);
        let mut info: BITMAPINFO = mem::zeroed();
        info.bmiHeader = BITMAPINFOHEADER {
            biSize: mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            ..mem::zeroed()
        };
        let mut bits = ptr::null_mut();
        let bitmap = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, 0, 0);
        assert_ne!(bitmap, 0);
        let old = SelectObject(dc, bitmap);
        FillRect(dc, &rect, GetSysColorBrush(COLOR_WINDOW));
        SendMessageW(
            edit,
            WM_PRINTCLIENT,
            dc as usize,
            (PRF_CLIENT | PRF_ERASEBKGND) as isize,
        );
        GdiFlush();
        let pixels =
            std::slice::from_raw_parts(bits as *const u32, (width * height) as usize).to_vec();
        SelectObject(dc, old);
        DeleteObject(bitmap);
        DeleteDC(dc);
        crate::save_bgra_to_png(width as u32, height as u32, pixels, path).unwrap();
    }
    #[test]
    fn native_main_search_keyboard_and_layout_when_requested() {
        let Some(base) = std::env::var_os("KSTART_SEARCH_EDIT_UI_BASE") else {
            return;
        };
        let base = std::path::PathBuf::from(base);
        let mut app = crate::AppState::new(base.clone(), ActivePopupMenu::None).unwrap();
        app.user_cfg.startup_lock = false;
        app.search_query = "alpha beta gamma".into();
        let width = app.user_cfg.window_width;
        let height = app.user_cfg.window_height;
        APP_STATE.with(|cell| *cell.try_borrow_mut().unwrap() = Some(app));
        unsafe {
            let class = to_wide("KStartNativeSearchKeyboardTest");
            let registration = WNDCLASSW {
                lpfnWndProc: Some(parent_proc),
                hInstance: GetModuleHandleW(ptr::null()),
                hCursor: LoadCursorW(0, IDC_ARROW),
                lpszClassName: class.as_ptr(),
                ..mem::zeroed()
            };
            RegisterClassW(&registration);
            let parent = CreateWindowExW(
                0,
                class.as_ptr(),
                to_wide("KStart 搜索框验证").as_ptr(),
                WS_POPUP | WS_CLIPCHILDREN,
                40,
                40,
                width,
                height,
                0,
                0,
                GetModuleHandleW(ptr::null()),
                ptr::null(),
            );
            assert_ne!(parent, 0);
            APP_STATE
                .with(|cell| cell.try_borrow_mut().unwrap().as_mut().unwrap().main_hwnd = parent);
            ShowWindow(parent, SW_SHOWNORMAL);
            sync(parent);
            drain();
            let edit = control(parent);
            assert_ne!(edit, 0);
            SetFocus(edit);
            let data_before = std::fs::read(base.join("user/data.json")).unwrap();
            let mut name = [0u16; 32];
            GetClassNameW(edit, name.as_mut_ptr(), 32);
            assert_eq!(String::from_utf16_lossy(&name[..4]), "Edit");
            with_keys(&[], || {
                SendMessageW(edit, WM_KEYDOWN, VK_END as usize, 0);
                assert_eq!(selection(edit), (16, 16));
                SendMessageW(edit, WM_KEYDOWN, VK_HOME as usize, 0);
                assert_eq!(selection(edit), (0, 0));
            });
            with_keys(&[VK_SHIFT], || {
                SendMessageW(edit, WM_KEYDOWN, VK_END as usize, 0);
                assert_eq!(selection(edit), (0, 16));
            });
            sync(parent);
            assert_eq!(selection(edit), (0, 16), "同步或重绘不得重置选区");
            with_keys(&[], || {
                SendMessageW(edit, WM_KEYDOWN, VK_END as usize, 0);
            });
            with_keys(&[VK_SHIFT], || {
                SendMessageW(edit, WM_KEYDOWN, VK_HOME as usize, 0);
                assert_eq!(selection(edit), (0, 16));
            });
            with_keys(&[VK_CONTROL], || {
                SendMessageW(edit, WM_KEYDOWN, b'A' as usize, 0);
                assert_eq!(selection(edit), (0, 16));
            });
            SendMessageW(edit, EM_SETSEL, 0, 5);
            SendMessageW(edit, WM_KEYDOWN, VK_DELETE as usize, 0);
            assert_eq!(get_ctrl_text(parent, ID), " beta gamma");
            SendMessageW(edit, WM_UNDO, 0, 0);
            assert_eq!(get_ctrl_text(parent, ID), "alpha beta gamma");
            with_keys(&[VK_CONTROL], || {
                SendMessageW(edit, WM_KEYDOWN, b'A' as usize, 0);
            });
            SendMessageW(
                edit,
                EM_REPLACESEL,
                1,
                to_wide("输入法与汉字测试").as_ptr() as isize,
            );
            APP_STATE.with(|cell| {
                assert_eq!(
                    cell.try_borrow().unwrap().as_ref().unwrap().search_query,
                    "输入法与汉字测试"
                )
            });
            SendMessageW(edit, WM_KEYDOWN, VK_RETURN as usize, 0);
            SendMessageW(edit, WM_CHAR, 13, 0);
            drain();
            assert_eq!(SUBMITS.with(Cell::get), 1);
            assert_eq!(get_ctrl_text(parent, ID), "输入法与汉字测试");
            let before = selection(edit);
            SetWindowPos(
                parent,
                0,
                0,
                0,
                width + 100,
                height + 50,
                SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
            );
            sync(parent);
            drain();
            assert_eq!(control(parent), edit);
            assert_eq!(selection(edit), before);
            let mut rect: RECT = mem::zeroed();
            GetWindowRect(edit, &mut rect);
            MapWindowPoints(0, parent, &mut rect as *mut RECT as *mut POINT, 2);
            let stored = state(edit).unwrap().rect.get();
            assert_eq!(
                (
                    rect.left,
                    rect.top,
                    rect.right - rect.left,
                    rect.bottom - rect.top
                ),
                stored
            );
            assert!(
                !state(edit).unwrap().pixels.borrow().is_empty(),
                "输入框应使用主画布的皮肤底图"
            );
            let shots = base.join("shots");
            std::fs::create_dir_all(&shots).unwrap();
            capture_edit(edit, &shots.join("main-search-edit.png"));
            SetFocus(parent);
            APP_STATE.with(|cell| {
                cell.try_borrow_mut()
                    .unwrap()
                    .as_mut()
                    .unwrap()
                    .selected_shortcuts
                    .clear()
            });
            with_keys(&[VK_CONTROL], || {
                SendMessageW(parent, WM_KEYDOWN, b'A' as usize, 0);
            });
            APP_STATE.with(|cell| {
                let guard = cell.try_borrow().unwrap();
                let app = guard.as_ref().unwrap();
                let cat = &app.categories[0];
                assert_eq!(
                    app.selected_shortcuts.len(),
                    cat.groups[cat.active_group_index].items.len()
                );
            });
            SetFocus(edit);
            APP_STATE.with(|cell| {
                cell.try_borrow_mut()
                    .unwrap()
                    .as_mut()
                    .unwrap()
                    .selected_shortcuts
                    .clear()
            });
            with_keys(&[VK_CONTROL], || {
                SendMessageW(edit, WM_KEYDOWN, b'A' as usize, 0);
            });
            APP_STATE.with(|cell| {
                assert!(cell
                    .try_borrow()
                    .unwrap()
                    .as_ref()
                    .unwrap()
                    .selected_shortcuts
                    .is_empty())
            });
            APP_STATE.with(|cell| {
                cell.try_borrow_mut()
                    .unwrap()
                    .as_mut()
                    .unwrap()
                    .skin_cfg
                    .search_box_visible = false
            });
            sync(parent);
            assert_eq!(GetWindowLongW(edit, GWL_STYLE) as u32 & WS_VISIBLE, 0);
            APP_STATE.with(|cell| {
                cell.try_borrow_mut()
                    .unwrap()
                    .as_mut()
                    .unwrap()
                    .skin_cfg
                    .search_box_visible = true
            });
            sync(parent);
            assert_ne!(GetWindowLongW(edit, GWL_STYLE) as u32 & WS_VISIBLE, 0);
            SetFocus(edit);
            SendMessageW(edit, WM_KEYDOWN, VK_ESCAPE as usize, 0);
            drain();
            assert!(get_ctrl_text(parent, ID).is_empty());
            assert_eq!(GetFocus(), parent);
            assert_eq!(
                std::fs::read(base.join("user/data.json")).unwrap(),
                data_before
            );
            DestroyWindow(parent);
        }
        APP_STATE.with(|cell| {
            cell.try_borrow_mut().unwrap().take();
        });
    }
}
